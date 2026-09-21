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
        let (shape, values) = outputs[0].try_extract_tensor::<f32>().context("decode ONNX output tensor")?;
        let dims: Vec<usize> = shape.iter().map(|value| (*value).max(0) as usize).collect();
        let candidates = decode_output(&dims, values, config.labels.len(), config.confidence_threshold, config.input_width as f32, config.input_height as f32)?;
        let detections = candidates.into_iter().map(|candidate| Detection {
            id: Uuid::new_v4(), camera_id: frame.camera_id, track_id: None, model_id: Some(config.id),
            object_class: config.labels.get(candidate.class_id).cloned().unwrap_or_else(|| format!("class_{}", candidate.class_id)),
            confidence: candidate.confidence,
            bounding_box: BoundingBox { x: candidate.x, y: candidate.y, width: candidate.width, height: candidate.height },
            observed_at: frame.observed_at,
        }).collect();
        Ok(detections)
    }
}

/// Non-maximum-suppression IoU threshold applied to decoded YOLO candidates.
const NMS_IOU_THRESHOLD: f32 = 0.45;

/// A single decoded detection in normalised, top-left `xywh` coordinates (0..1 of the frame).
#[derive(Debug, Clone)]
struct Candidate { class_id: usize, confidence: f32, x: f32, y: f32, width: f32, height: f32 }

/// Decode a raw detector output tensor into normalised detections.
///
/// Supports the common Ultralytics ONNX export layouts so that models downloaded
/// straight from the catalog work without a bespoke post-processing step:
///   * Anchor-free heads (YOLOv8 / YOLOv11 / YOLO26 raw): `[1, 4 + C, A]` or `[1, A, 4 + C]`
///   * Objectness heads (YOLOv5): `[1, A, 5 + C]` or `[1, 5 + C, A]`
///   * Pre-decoded / end-to-end rows of six: `[..., 6]` as `[x, y, w, h, confidence, class]`
///     already expressed in normalised, top-left coordinates.
///
/// Anchor-based layouts are decoded (centre `xywh` in input-pixel space → normalised
/// top-left `xywh`) and passed through per-class non-maximum suppression. The rows-of-six
/// layout is treated as already post-processed and returned as-is after the confidence gate.
fn decode_output(dims: &[usize], values: &[f32], num_labels: usize, confidence_threshold: f32, input_width: f32, input_height: f32) -> Result<Vec<Candidate>> {
    // Collapse leading unit (batch) dimensions: [1, F, A] -> [F, A], [1, N, 6] -> [N, 6].
    let meaningful: Vec<usize> = dims.iter().copied().filter(|value| *value != 1).collect();
    let input_width = if input_width > 0.0 { input_width } else { 640.0 };
    let input_height = if input_height > 0.0 { input_height } else { 640.0 };

    // Establish the (features, anchors, features_first) axes for a 2-D head, if this is
    // one. When the label count is known, the feature axis is the one whose length is
    // `4 + C` (anchor-free) or `5 + C` (objectness) - this is robust regardless of which
    // axis is larger. Otherwise fall back to assuming the smaller axis holds the features
    // (real exports carry far more anchors than feature channels).
    let two_dim = match meaningful.as_slice() {
        [d0, d1] => {
            let (d0, d1) = (*d0, *d1);
            if num_labels > 0 && (d0 == num_labels + 4 || d0 == num_labels + 5) {
                Some((d0, d1, true))
            } else if num_labels > 0 && (d1 == num_labels + 4 || d1 == num_labels + 5) {
                Some((d1, d0, false))
            } else if d0 <= d1 {
                Some((d0, d1, true))
            } else {
                Some((d1, d0, false))
            }
        }
        _ => None,
    };
    // A known-label-count anchor head takes priority so that a genuine 2-class
    // anchor-free model (features == 6) is not mistaken for pre-decoded rows.
    let is_known_head = matches!(two_dim, Some((features, _, _)) if num_labels > 0 && (features == num_labels + 4 || features == num_labels + 5));

    // Flat or [N, 6] pre-decoded output: already normalised top-left xywh + conf + class.
    // Pre-decoded exports carry the six fields as the innermost (last) dimension.
    let is_rows_of_six = meaningful.len() <= 1 || meaningful.last() == Some(&6);
    if is_rows_of_six && !is_known_head {
        if values.len() % 6 != 0 { bail!("unsupported detector output: expected groups of 6 values, got {}", values.len()); }
        let mut out = Vec::new();
        for row in values.chunks_exact(6) {
            if row[4] < confidence_threshold { continue; }
            out.push(Candidate { class_id: row[5] as usize, confidence: row[4], x: row[0], y: row[1], width: row[2], height: row[3] });
        }
        return Ok(out);
    }

    // The channel/feature axis is the smaller dimension; anchors are the larger one.
    let Some((features, anchors, features_first)) = two_dim else { bail!("unsupported detector output shape: {dims:?}"); };
    if features < 5 || anchors == 0 { bail!("unsupported detector output shape: {dims:?}"); }
    if values.len() != features * anchors { bail!("detector output size {} does not match shape {dims:?}", values.len()); }

    // Choose the head layout. When the label count is known we match it exactly;
    // otherwise assume a modern anchor-free head (4 box values + class scores).
    let objectness = num_labels > 0 && features == num_labels + 5;
    let class_offset = if objectness { 5 } else { 4 };
    let num_classes = features - class_offset;
    if num_classes == 0 { bail!("detector output has no class channels: {dims:?}"); }

    // Indexing helper for either channel-first `[F, A]` or channel-last `[A, F]` layout.
    let at = |feature: usize, anchor: usize| -> f32 {
        if features_first { values[feature * anchors + anchor] } else { values[anchor * features + feature] }
    };

    let mut candidates = Vec::new();
    for anchor in 0..anchors {
        let (cx, cy, w, h) = (at(0, anchor), at(1, anchor), at(2, anchor), at(3, anchor));
        let objectness_score = if objectness { at(4, anchor) } else { 1.0 };
        let mut best_class = 0usize;
        let mut best_score = 0.0f32;
        for class in 0..num_classes {
            let score = at(class_offset + class, anchor);
            if score > best_score { best_score = score; best_class = class; }
        }
        let confidence = objectness_score * best_score;
        if confidence < confidence_threshold { continue; }
        // Centre xywh in input-pixel space -> normalised top-left xywh.
        candidates.push(Candidate {
            class_id: best_class,
            confidence,
            x: (cx - w / 2.0) / input_width,
            y: (cy - h / 2.0) / input_height,
            width: w / input_width,
            height: h / input_height,
        });
    }
    Ok(non_max_suppression(candidates, NMS_IOU_THRESHOLD))
}

/// Greedy per-class non-maximum suppression over normalised `xywh` candidates.
fn non_max_suppression(mut candidates: Vec<Candidate>, iou_threshold: f32) -> Vec<Candidate> {
    candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        if kept.iter().any(|existing| existing.class_id == candidate.class_id && iou(existing, &candidate) > iou_threshold) { continue; }
        kept.push(candidate);
    }
    kept
}

/// Intersection-over-union of two normalised, top-left `xywh` boxes.
fn iou(a: &Candidate, b: &Candidate) -> f32 {
    let ax2 = a.x + a.width; let ay2 = a.y + a.height;
    let bx2 = b.x + b.width; let by2 = b.y + b.height;
    let ix1 = a.x.max(b.x); let iy1 = a.y.max(b.y);
    let ix2 = ax2.min(bx2); let iy2 = ay2.min(by2);
    let iw = (ix2 - ix1).max(0.0); let ih = (iy2 - iy1).max(0.0);
    let intersection = iw * ih;
    let union = (a.width * a.height) + (b.width * b.height) - intersection;
    if union <= 0.0 { 0.0 } else { intersection / union }
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

    #[test]
    fn decodes_anchor_free_head_channel_first() {
        // [1, 6, 2] => 2 classes, 2 anchors, channel-first (index = feature * anchors + anchor).
        // Anchor 0 is a strong class-0 box centred at (320,320) size 64; anchor 1 is empty.
        let values = vec![320.0, 0.0, 320.0, 0.0, 64.0, 0.0, 64.0, 0.0, 0.9, 0.0, 0.1, 0.0];
        let out = decode_output(&[1, 6, 2], &values, 2, 0.25, 640.0, 640.0).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].class_id, 0);
        assert!((out[0].confidence - 0.9).abs() < 1e-5);
        assert!((out[0].x - 0.45).abs() < 1e-4 && (out[0].y - 0.45).abs() < 1e-4);
        assert!((out[0].width - 0.1).abs() < 1e-4 && (out[0].height - 0.1).abs() < 1e-4);
    }

    #[test]
    fn decodes_v5_objectness_head_channel_last() {
        // [1, 2, 85] => 80 classes + objectness, channel-last (index = anchor * 85 + feature).
        let features = 85; let anchors = 2;
        let mut values = vec![0.0f32; features * anchors];
        // Anchor 0: box (320,320,64,64), objectness 0.9, class 5 score 0.8 => conf 0.72.
        values[0] = 320.0; values[1] = 320.0; values[2] = 64.0; values[3] = 64.0; values[4] = 0.9; values[5 + 5] = 0.8;
        let out = decode_output(&[1, 2, 85], &values, 80, 0.25, 640.0, 640.0).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].class_id, 5);
        assert!((out[0].confidence - 0.72).abs() < 1e-5);
    }

    #[test]
    fn passes_through_pre_decoded_rows_of_six() {
        // [1, 3, 6] rows of [x, y, w, h, conf, class] already normalised; middle row is below threshold.
        let values = vec![
            0.1, 0.1, 0.2, 0.2, 0.90, 3.0,
            0.5, 0.5, 0.1, 0.1, 0.10, 1.0,
            0.7, 0.7, 0.1, 0.1, 0.50, 2.0,
        ];
        let out = decode_output(&[1, 3, 6], &values, 0, 0.25, 640.0, 640.0).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].class_id, 3);
        assert_eq!(out[1].class_id, 2);
        assert!((out[0].x - 0.1).abs() < 1e-6 && (out[0].width - 0.2).abs() < 1e-6);
    }

    #[test]
    fn suppresses_overlapping_same_class_boxes() {
        // Two near-identical class-0 boxes plus one distinct class-0 box.
        let a = Candidate { class_id: 0, confidence: 0.9, x: 0.10, y: 0.10, width: 0.20, height: 0.20 };
        let b = Candidate { class_id: 0, confidence: 0.8, x: 0.11, y: 0.11, width: 0.20, height: 0.20 };
        let c = Candidate { class_id: 0, confidence: 0.7, x: 0.70, y: 0.70, width: 0.20, height: 0.20 };
        let kept = non_max_suppression(vec![a, b, c], 0.45);
        assert_eq!(kept.len(), 2);
        assert!((kept[0].confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn keeps_overlapping_boxes_of_different_classes() {
        let a = Candidate { class_id: 0, confidence: 0.9, x: 0.10, y: 0.10, width: 0.20, height: 0.20 };
        let b = Candidate { class_id: 1, confidence: 0.8, x: 0.11, y: 0.11, width: 0.20, height: 0.20 };
        let kept = non_max_suppression(vec![a, b], 0.45);
        assert_eq!(kept.len(), 2);
    }
}
