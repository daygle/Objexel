use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use ndarray::{ArrayD, IxDyn};
use objexel_common::{BoundingBox, Detection};
use ort::{ep, session::Session, value::TensorRef};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::{AtomicUsize, Ordering}, Arc, Mutex},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub model_type: String,
    pub path: PathBuf,
    pub input_width: u32,
    pub input_height: u32,
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
    sessions: Vec<Mutex<Session>>,
    next_session: AtomicUsize,
}

impl LoadedModel {
    fn next_session(&self) -> &Mutex<Session> {
        let index = self.next_session.fetch_add(1, Ordering::Relaxed) % self.sessions.len();
        &self.sessions[index]
    }
}

#[derive(Clone, Default)]
pub struct ModelManager {
    models: Arc<tokio::sync::RwLock<HashMap<Uuid, Arc<LoadedModel>>>>,
    active: Arc<tokio::sync::RwLock<Option<Uuid>>>,
}

impl ModelManager {
    pub async fn load(&self, config: ModelConfig) -> Result<()> {
        validate_model_config(&config)?;
        // Build the session in a scope that ends before any await: ort's
        // SessionBuilder is neither Send nor Sync, so it must not be held
        // across the map lock below.
        let pool_size = inference_pool_size();
        let loaded = {
            let sessions = (0..pool_size)
                .map(|_| build_session(&config))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(Mutex::new)
                .collect();
            Arc::new(LoadedModel { config: config.clone(), sessions, next_session: AtomicUsize::new(0) })
        };
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

    pub async fn model_config(&self, model_id: Uuid) -> Option<ModelConfig> {
        self.models.read().await.get(&model_id).map(|model| model.config.clone())
    }

    /// Run one deterministic zero-filled inference for the benchmark engine.
    pub async fn benchmark_once(&self, model_id: Uuid, width: u32, height: u32) -> Result<()> {
        let frame = FrameTensor {
            camera_id: Uuid::nil(),
            observed_at: chrono::Utc::now(),
            shape: vec![1, 3, height as usize, width as usize],
            data: vec![0.0; 3 * width as usize * height as usize],
        };
        self.detect_with_model(Some(model_id), frame).await.map(|_| ())
    }

    async fn active_model(&self, model_id: Option<Uuid>) -> Result<Arc<LoadedModel>> {
        let id = match model_id { Some(id) => id, None => (*self.active.read().await).context("no active ONNX model")? };
        self.models.read().await.get(&id).cloned().context("selected ONNX model is not loaded")
    }

    pub async fn detect(&self, frame: FrameTensor) -> Result<Vec<Detection>> { self.detect_with_model(None, frame).await }

    pub async fn detect_with_model(&self, model_id: Option<Uuid>, frame: FrameTensor) -> Result<Vec<Detection>> {
        let model = self.active_model(model_id).await?;
        let config = model.config.clone();
        let input = ArrayD::from_shape_vec(IxDyn(&frame.shape), frame.data).context("invalid detector tensor shape")?;
        let mut session = model.next_session().lock().map_err(|_| anyhow::anyhow!("ONNX session lock poisoned"))?;
        let outputs = session.run(ort::inputs![TensorRef::from_array_view(input.view())?]).context("run ONNX inference")?;
        if outputs.len() == 0 { bail!("ONNX model returned no outputs"); }
        let (_, values) = outputs[0].try_extract_tensor::<f32>().context("decode ONNX output tensor")?;
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

fn inference_pool_size() -> usize {
    std::env::var("OBJEXEL_INFERENCE_SESSIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(1, 16))
        .unwrap_or(2)
}

fn build_session(config: &ModelConfig) -> Result<Session> {
    let mut builder = Session::builder().context("create ONNX Runtime session builder")?;
    // Prefer CUDA when compiled with the cuda feature, while retaining the built-in
    // CPU provider as a portable fallback. A failed provider configuration recovers
    // the builder so the session still loads with defaults.
    #[cfg(feature = "cuda")]
    let providers = [ep::CUDA::default().build(), ep::CPU::default().build()];
    #[cfg(not(feature = "cuda"))]
    let providers = [ep::CPU::default().build()];
    builder = match builder.with_execution_providers(providers) {
        Ok(builder) => builder,
        Err(error) => {
            tracing::warn!(error = %error.message(), "failed to configure ONNX execution providers; using runtime defaults");
            error.recover()
        }
    };
    builder.commit_from_file(&config.path).with_context(|| format!("load ONNX model {}", config.path.display()))
}

fn validate_model_config(config: &ModelConfig) -> Result<()> {
    if config.path.extension().and_then(|ext| ext.to_str()) != Some("onnx") { bail!("model path must have an .onnx extension"); }
    if config.input_width == 0 || config.input_height == 0 { bail!("model input dimensions must be positive"); }
    if !(0.0..=1.0).contains(&config.confidence_threshold) { bail!("confidence threshold must be between 0 and 1"); }
    if !Path::new(&config.path).is_file() { bail!("model file does not exist: {}", config.path.display()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_validation_rejects_invalid_threshold() {
        let config = ModelConfig { id: Uuid::new_v4(), name: "test".into(), version: "1".into(), model_type: "yolo".into(), path: "model.onnx".into(), input_width: 640, input_height: 640, labels: vec![], confidence_threshold: 1.1 };
        assert!(validate_model_config(&config).is_err());
    }

    #[test]
    fn model_validation_requires_onnx_extension() {
        let config = ModelConfig { id: Uuid::new_v4(), name: "test".into(), version: "1".into(), model_type: "yolo".into(), path: "model.bin".into(), input_width: 640, input_height: 640, labels: vec![], confidence_threshold: 0.5 };
        assert!(validate_model_config(&config).is_err());
    }

    #[test]
    fn inference_pool_size_is_bounded_and_configurable() {
        assert!((1..=16).contains(&inference_pool_size()));
    }
}
