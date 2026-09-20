use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use ndarray::{ArrayD, IxDyn};
use objexel_common::{BoundingBox, Detection};
use ort::{ep, session::Session, value::TensorRef};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::{Path, PathBuf}, sync::{Arc, Mutex}};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f32,
}

fn default_confidence() -> f32 { 0.25 }

#[derive(Debug, Clone)]
pub struct FrameTensor {
    pub camera_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub shape: Vec<usize>,
    pub data: Vec<f32>,
}

struct LoadedModel {
    config: ModelConfig,
    session: Mutex<Session>,
}

#[derive(Clone, Default)]
pub struct ModelManager {
    models: Arc<tokio::sync::RwLock<HashMap<Uuid, Arc<LoadedModel>>>>,
    active: Arc<tokio::sync::RwLock<Option<Uuid>>>,
}

impl ModelManager {
    pub async fn load(&self, config: ModelConfig) -> Result<()> {
        validate_model_config(&config)?;
        let mut builder = Session::builder().context("create ONNX Runtime session builder")?;
        builder = builder.with_execution_providers([ep::CUDA::default().build()])?;
        let session = builder.commit_from_file(&config.path).with_context(|| format!("load ONNX model {}", config.path.display()))?;
        let loaded = Arc::new(LoadedModel { config: config.clone(), session: Mutex::new(session) });
        self.models.write().await.insert(config.id, loaded);
        let mut active = self.active.write().await;
        if active.is_none() { *active = Some(config.id); }
        tracing::info!(model_id = %config.id, model = %config.name, "ONNX model loaded");
        Ok(())
    }

    pub async fn reload(&self, config: ModelConfig) -> Result<()> {
        self.load(config).await
    }

    pub async fn set_active(&self, model_id: Uuid) -> Result<()> {
        if !self.models.read().await.contains_key(&model_id) { bail!("model {model_id} is not loaded"); }
        *self.active.write().await = Some(model_id);
        Ok(())
    }

    pub async fn active_config(&self) -> Option<ModelConfig> {
        let id = (*self.active.read().await)?;
        self.models.read().await.get(&id).map(|model| model.config.clone())
    }

    async fn active_model(&self) -> Result<Arc<LoadedModel>> {
        let id = (*self.active.read().await).context("no active ONNX model")?;
        self.models.read().await.get(&id).cloned().context("active ONNX model is not loaded")
    }

    pub async fn detect(&self, frame: FrameTensor) -> Result<Vec<Detection>> {
        let model = self.active_model().await?;
        let config = model.config.clone();
        let input = ArrayD::from_shape_vec(IxDyn(&frame.shape), frame.data).context("invalid detector tensor shape")?;
        let mut session = model.session.lock().map_err(|_| anyhow::anyhow!("ONNX session lock poisoned"))?;
        let outputs = session.run(ort::inputs![TensorRef::from_array_view(input.view())?]).context("run ONNX inference")?;
        let output = outputs.get(0).context("ONNX model returned no outputs")?;
        let (_, values) = output.try_extract_tensor::<f32>().context("decode ONNX output tensor")?;
        if values.len() % 6 != 0 { bail!("unsupported detector output: expected groups of 6 values, got {}", values.len()); }
        let mut detections = Vec::new();
        for row in values.chunks_exact(6) {
            let confidence = row[4];
            if confidence < config.confidence_threshold { continue; }
            let class_id = row[5] as usize;
            let object_class = config.labels.get(class_id).cloned().unwrap_or_else(|| format!("class_{class_id}"));
            detections.push(Detection {
                id: Uuid::new_v4(), camera_id: frame.camera_id, track_id: None, model_id: Some(config.id), object_class,
                confidence, bounding_box: BoundingBox { x: row[0], y: row[1], width: row[2], height: row[3] }, observed_at: frame.observed_at,
            });
        }
        Ok(detections)
    }
}

fn validate_model_config(config: &ModelConfig) -> Result<()> {
    if config.path.extension().and_then(|ext| ext.to_str()) != Some("onnx") { bail!("model path must have an .onnx extension"); }
    if !(0.0..=1.0).contains(&config.confidence_threshold) { bail!("confidence threshold must be between 0 and 1"); }
    if !Path::new(&config.path).is_file() { bail!("model file does not exist: {}", config.path.display()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_validation_rejects_invalid_threshold() {
        let config = ModelConfig { id: Uuid::new_v4(), name: "test".into(), version: "1".into(), path: "model.onnx".into(), labels: vec![], confidence_threshold: 1.1 };
        assert!(validate_model_config(&config).is_err());
    }

    #[test]
    fn model_validation_requires_onnx_extension() {
        let config = ModelConfig { id: Uuid::new_v4(), name: "test".into(), version: "1".into(), path: "model.bin".into(), labels: vec![], confidence_threshold: 0.5 };
        assert!(validate_model_config(&config).is_err());
    }
}
