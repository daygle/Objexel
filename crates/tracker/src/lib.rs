use chrono::{DateTime, Duration, Utc};
use objexel_common::{BoundingBox, Detection, Track};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TrackerConfig {
    pub iou_threshold: f32,
    pub max_age: Duration,
}

impl Default for TrackerConfig {
    fn default() -> Self { Self { iou_threshold: 0.30, max_age: Duration::seconds(2) } }
}

#[derive(Default)]
pub struct Tracker {
    config: TrackerConfig,
    tracks: HashMap<Uuid, Track>,
}

impl Tracker {
    pub fn new(config: TrackerConfig) -> Self { Self { config, tracks: HashMap::new() } }

    pub fn update(&mut self, detections: &mut [Detection], now: DateTime<Utc>) -> Vec<Track> {
        let mut used = HashMap::new();
        for detection in detections.iter_mut() {
            let candidate = self.tracks.iter()
                .filter(|(id, track)| !used.contains_key(*id) && track.object_class == detection.object_class && now - track.last_seen <= self.config.max_age)
                .max_by(|(_, left), (_, right)| {
                    let left_iou = iou(left.movement_path.last().unwrap_or(&detection.bounding_box), &detection.bounding_box);
                    let right_iou = iou(right.movement_path.last().unwrap_or(&detection.bounding_box), &detection.bounding_box);
                    left_iou.partial_cmp(&right_iou).unwrap_or(std::cmp::Ordering::Equal)
                });
            let track_id = candidate.and_then(|(id, track)| {
                if iou(track.movement_path.last().unwrap_or(&detection.bounding_box), &detection.bounding_box) >= self.config.iou_threshold { Some(*id) } else { None }
            }).unwrap_or_else(Uuid::new_v4);
            let track = self.tracks.entry(track_id).or_insert_with(|| Track { id: track_id, camera_id: detection.camera_id, object_class: detection.object_class.clone(), first_seen: detection.observed_at, last_seen: detection.observed_at, duration_ms: 0, movement_path: Vec::new() });
            track.last_seen = detection.observed_at;
            track.duration_ms = (track.last_seen - track.first_seen).num_milliseconds();
            track.movement_path.push(detection.bounding_box.clone());
            if track.movement_path.len() > 64 { track.movement_path.remove(0); }
            detection.track_id = Some(track_id);
            used.insert(track_id, true);
        }
        self.expire(now);
        detections.iter().filter_map(|d| d.track_id.and_then(|id| self.tracks.get(&id).cloned())).collect()
    }

    pub fn active_tracks(&self) -> Vec<Track> { self.tracks.values().cloned().collect() }

    fn expire(&mut self, now: DateTime<Utc>) { self.tracks.retain(|_, track| now - track.last_seen <= self.config.max_age); }
}

fn iou(left: &BoundingBox, right: &BoundingBox) -> f32 {
    let x1 = left.x.max(right.x);
    let y1 = left.y.max(right.y);
    let x2 = (left.x + left.width).min(right.x + right.width);
    let y2 = (left.y + left.height).min(right.y + right.height);
    let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = left.width * left.height + right.width * right.height - intersection;
    if union <= 0.0 { 0.0 } else { intersection / union }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_object_keeps_identity() {
        let now = Utc::now();
        let mut tracker = Tracker::new(TrackerConfig::default());
        let mut first = vec![detection(now, 0.1)];
        let first_track = tracker.update(&mut first, now)[0].id;
        let mut second = vec![detection(now + Duration::milliseconds(100), 0.11)];
        let second_track = tracker.update(&mut second, now + Duration::milliseconds(100))[0].id;
        assert_eq!(first_track, second_track);
    }

    fn detection(timestamp: DateTime<Utc>, x: f32) -> Detection { Detection { id: Uuid::new_v4(), camera_id: Uuid::new_v4(), track_id: None, model_id: None, object_class: "cat".into(), confidence: 0.9, bounding_box: BoundingBox { x, y: 0.1, width: 0.2, height: 0.2 }, observed_at: timestamp } }
}
