use chrono::Utc;
use objexel_common::{Observation, Track};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ObservationConfig {
    pub minimum_duration_ms: i64,
}

impl Default for ObservationConfig {
    fn default() -> Self { Self { minimum_duration_ms: 3_000 } }
}

#[derive(Debug, Clone)]
pub struct ObservationEngine {
    config: ObservationConfig,
}

impl ObservationEngine {
    pub fn new(config: ObservationConfig) -> Self { Self { config } }

    pub fn from_track(&self, track: &Track) -> Option<Observation> {
        if track.duration_ms < self.config.minimum_duration_ms { return None; }
        Some(Observation {
            id: Uuid::new_v4(), camera_id: track.camera_id, track_id: track.id,
            observation_type: "presence".into(),
            summary: format!("{} observed for {} seconds", track.object_class, track.duration_ms / 1_000),
            created_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    #[test]
    fn short_tracks_are_not_observations() {
        let engine = ObservationEngine::default();
        let track = Track { id: Uuid::new_v4(), camera_id: Uuid::new_v4(), object_class: "cat".into(), first_seen: Utc::now(), last_seen: Utc::now(), duration_ms: 100, movement_path: vec![] };
        assert!(engine.from_track(&track).is_none());
    }
}

impl Default for ObservationEngine {
    fn default() -> Self { Self::new(ObservationConfig::default()) }
}
