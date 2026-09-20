use chrono::{DateTime, Utc};
use objexel_common::{Behaviour, Track};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviourType { Loitering, StationaryObject, RouteFollowing }

impl BehaviourType {
    pub fn as_str(self) -> &'static str { match self { Self::Loitering => "loitering", Self::StationaryObject => "stationary_object", Self::RouteFollowing => "route_following" } }
}

#[derive(Debug, Clone)]
pub struct BehaviourAnalyzer { pub loitering_ms: i64, pub stationary_ms: i64 }

impl Default for BehaviourAnalyzer {
    fn default() -> Self { Self { loitering_ms: 60_000, stationary_ms: 30_000 } }
}

impl BehaviourAnalyzer {
    pub fn analyze(&self, track: &Track, now: DateTime<Utc>) -> Option<Behaviour> {
        let duration_ms = track.duration_ms.max(0);
        let path_points = track.movement_path.len();
        let displacement = track.movement_path.first().zip(track.movement_path.last()).map(|(first, last)| {
            let first_x = first.x + first.width / 2.0; let first_y = first.y + first.height / 2.0;
            let last_x = last.x + last.width / 2.0; let last_y = last.y + last.height / 2.0;
            ((last_x - first_x).powi(2) + (last_y - first_y).powi(2)).sqrt()
        }).unwrap_or(0.0);
        let (kind, confidence, summary) = if duration_ms >= self.loitering_ms {
            (BehaviourType::Loitering, (0.65 + (duration_ms as f32 / 600_000.0).min(0.3)).min(0.95), format!("{} loitered for {} seconds", track.object_class, duration_ms / 1000))
        } else if duration_ms >= self.stationary_ms && displacement < 0.05 {
            (BehaviourType::StationaryObject, 0.9, format!("{} remained stationary for {} seconds", track.object_class, duration_ms / 1000))
        } else if path_points >= 5 && displacement >= 0.1 {
            (BehaviourType::RouteFollowing, 0.7, format!("{} followed a route across {} path points", track.object_class, path_points))
        } else { return None };
        Some(Behaviour { id: Uuid::new_v4(), track_id: track.id, camera_id: track.camera_id, object_class: track.object_class.clone(), behaviour_type: kind.as_str().into(), confidence, summary, start_time: track.first_seen, end_time: now.min(track.last_seen), recording_id: None, clip_id: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objexel_common::BoundingBox;
    #[test]
    fn long_stationary_track_is_loitering() {
        let now = Utc::now();
        let track = Track { id: Uuid::new_v4(), camera_id: Uuid::new_v4(), object_class: "cat".into(), first_seen: now - chrono::Duration::seconds(70), last_seen: now, duration_ms: 70_000, movement_path: vec![BoundingBox { x: 0.2, y: 0.2, width: 0.1, height: 0.1 }, BoundingBox { x: 0.2, y: 0.2, width: 0.1, height: 0.1 }] };
        assert_eq!(BehaviourAnalyzer::default().analyze(&track, now).unwrap().behaviour_type, "loitering");
    }
}
