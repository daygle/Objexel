use objexel_common::{BoundingBox, Detection, ModelAssignment};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FusionEngine { pub iou_threshold: f32 }

impl Default for FusionEngine { fn default() -> Self { Self { iou_threshold: 0.5 } } }

impl FusionEngine {
    pub fn fuse(&self, assignments: &[ModelAssignment], detections: Vec<Detection>) -> Vec<Detection> {
        let mut order: HashMap<Uuid, (i32, f32)> = HashMap::new();
        for assignment in assignments.iter().filter(|item| item.enabled) { order.insert(assignment.model_id, (assignment.priority, assignment.confidence_threshold)); }
        let mut fused: Vec<Detection> = Vec::new();
        for detection in detections.into_iter().filter(|item| order.get(&item.model_id.unwrap_or_default()).map(|(_, threshold)| item.confidence >= *threshold).unwrap_or(true)) {
            if let Some(existing) = fused.iter_mut().find(|item| item.object_class == detection.object_class && iou(&item.bounding_box, &detection.bounding_box) >= self.iou_threshold) {
                let existing_score = order.get(&existing.model_id.unwrap_or_default()).map(|(priority, weight)| existing.confidence * (1.0 + *weight) + *priority as f32 * 0.001).unwrap_or(existing.confidence);
                let new_score = order.get(&detection.model_id.unwrap_or_default()).map(|(priority, weight)| detection.confidence * (1.0 + *weight) + *priority as f32 * 0.001).unwrap_or(detection.confidence);
                if new_score > existing_score { existing.model_id = detection.model_id; existing.confidence = detection.confidence; existing.bounding_box = detection.bounding_box; }
            } else { fused.push(detection); }
        }
        fused
    }
}

fn iou(a: &BoundingBox, b: &BoundingBox) -> f32 {
    let ax2 = a.x + a.width; let ay2 = a.y + a.height; let bx2 = b.x + b.width; let by2 = b.y + b.height;
    let ix = (ax2.min(bx2) - a.x.max(b.x)).max(0.0); let iy = (ay2.min(by2) - a.y.max(b.y)).max(0.0);
    let intersection = ix * iy; let union = a.width * a.height + b.width * b.height - intersection;
    if union <= 0.0 { 0.0 } else { intersection / union }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_same_class_detections_are_collapsed() {
        let camera_id = Uuid::new_v4(); let model_a = Uuid::new_v4(); let model_b = Uuid::new_v4();
        let assignment = |model_id| ModelAssignment { id: Uuid::new_v4(), camera_id, model_id, priority: 0, confidence_threshold: 0.1, fps_limit: None, enabled: true };
        let detection = |model_id, confidence| Detection { id: Uuid::new_v4(), camera_id, track_id: None, model_id: Some(model_id), object_class: "person".into(), confidence, bounding_box: BoundingBox { x: 0.1, y: 0.1, width: 0.3, height: 0.3 }, observed_at: chrono::Utc::now() };
        assert_eq!(FusionEngine::default().fuse(&[assignment(model_a), assignment(model_b)], vec![detection(model_a, 0.87), detection(model_b, 0.73)]).len(), 1);
    }
}
