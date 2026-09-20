use anyhow::{bail, Context, Result};
use chrono::Utc;
use objexel_common::{BenchmarkResult, CreateModel, Model};
use objexel_detector::{ModelConfig, ModelManager};
use std::{path::{Path, PathBuf}, time::Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub enum InferenceProfile { Fast, Balanced, Accurate }

impl InferenceProfile {
    pub fn confidence_threshold(self) -> f32 { match self { Self::Fast => 0.40, Self::Balanced => 0.25, Self::Accurate => 0.15 } }
}

#[derive(Clone)]
pub struct ModelRegistry {
    pub model_manager: ModelManager,
    pub root: PathBuf,
}

impl ModelRegistry {
    pub fn new(root: impl Into<PathBuf>, model_manager: ModelManager) -> Self { Self { root: root.into(), model_manager } }

    pub async fn discover(&self) -> Result<Vec<ModelConfig>> {
        let mut configs = Vec::new();
        for (directory, model_type) in [("yolov8", "yolov8"), ("yolov11", "yolov11"), ("yolov26", "yolov26")] {
            let path = self.root.join(directory);
            if !path.is_dir() { continue; }
            for entry in std::fs::read_dir(&path).with_context(|| format!("scan {}", path.display()))? {
                let file = entry?.path();
                if file.extension().and_then(|value| value.to_str()) != Some("onnx") { continue; }
                configs.push(ModelConfig { id: Uuid::new_v4(), name: directory.into(), version: "discovered".into(), model_type: model_type.into(), path: file, input_width: 640, input_height: 640, labels: Vec::new(), confidence_threshold: 0.25 });
            }
        }
        Ok(configs)
    }

    pub async fn load_all(&self) -> Result<usize> {
        let configs = self.discover().await?;
        let loaded = self.load_configs(configs).await?;
        tracing::info!(loaded, root = %self.root.display(), "model discovery complete");
        Ok(loaded)
    }

    /// Load models already registered in PostgreSQL while preserving their stable IDs.
    pub async fn load_registered(&self, models: &[Model]) -> Result<usize> {
        let configs = models.iter().filter(|model| model.enabled).map(|model| ModelConfig {
            id: model.id,
            name: model.name.clone(),
            version: model.version.clone(),
            model_type: model.model_type.clone(),
            path: model.path.clone().into(),
            input_width: model.input_width,
            input_height: model.input_height,
            labels: model.class_list.clone(),
            confidence_threshold: InferenceProfile::Balanced.confidence_threshold(),
        }).collect();
        self.load_configs(configs).await
    }

    async fn load_configs(&self, configs: Vec<ModelConfig>) -> Result<usize> {
        let mut loaded = 0;
        for config in configs {
            self.model_manager.load(config).await?;
            loaded += 1;
        }
        Ok(loaded)
    }

    pub async fn validate(path: &Path, input_width: u32, input_height: u32) -> Result<()> {
        if path.extension().and_then(|value| value.to_str()) != Some("onnx") { bail!("model must use the ONNX format"); }
        if !path.is_file() { bail!("model file does not exist: {}", path.display()); }
        if input_width == 0 || input_height == 0 { bail!("model input dimensions must be positive"); }
        let bytes = std::fs::read(path).with_context(|| format!("read model {}", path.display()))?;
        if bytes.len() < 4 || &bytes[0..4] != b"\x08\x00\x00\x00" && !bytes.starts_with(b"ONNX") { tracing::debug!("model header is not recognizable; ONNX Runtime remains the authoritative validator"); }
        Ok(())
    }

    pub async fn benchmark(&self, model: &Model, iterations: u32) -> Result<BenchmarkResult> {
        let iterations = iterations.max(1);
        let config = self.model_manager.model_config(model.id).await.context("model is not loaded")?;
        let started = Instant::now();
        for _ in 0..iterations {
            self.model_manager.benchmark_once(model.id, config.input_width, config.input_height).await?;
        }
        let elapsed = started.elapsed().as_secs_f32().max(0.000_001);
        Ok(BenchmarkResult { id: Uuid::new_v4(), model_id: model.id, fps: iterations as f32 / elapsed, average_inference_time_ms: elapsed * 1000.0 / iterations as f32, gpu_memory_usage_mb: None, cpu_usage_percent: None, test_timestamp: Utc::now() })
    }
}

pub fn model_config_from_create(id: Uuid, input: CreateModel) -> ModelConfig { ModelConfig { id, name: input.name, version: input.version, model_type: input.model_type, path: input.path.into(), input_width: input.input_width, input_height: input.input_height, labels: input.class_list, confidence_threshold: InferenceProfile::Balanced.confidence_threshold() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_are_ordered_by_confidence() { assert!(InferenceProfile::Fast.confidence_threshold() > InferenceProfile::Accurate.confidence_threshold()); }
    #[tokio::test]
    async fn validation_rejects_non_onnx() { assert!(ModelRegistry::validate(Path::new("model.bin"), 640, 640).await.is_err()); }
}
