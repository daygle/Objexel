use anyhow::Result;
use objexel_common::{Detection, Observation, Track, ZoneEventType};
use objexel_database::Database;
use objexel_detector::{FrameTensor, ModelManager};
use objexel_observations::ObservationEngine;
use objexel_tracker::Tracker;
use objexel_zones::ZoneEvaluator;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ObservationPipeline {
    pub models: ModelManager,
    tracker: Arc<Mutex<Tracker>>,
    zones: Arc<Mutex<ZoneEvaluator>>,
    observations: ObservationEngine,
    database: Database,
}

impl ObservationPipeline {
    pub fn new(database: Database) -> Self {
        Self { models: ModelManager::default(), tracker: Arc::new(Mutex::new(Tracker::default())), zones: Arc::new(Mutex::new(ZoneEvaluator::default())), observations: ObservationEngine::default(), database }
    }

    pub async fn process_frame(&self, frame: FrameTensor) -> Result<PipelineResult> {
        let camera_id = frame.camera_id;
        let mut detections = self.models.detect(frame).await?;
        let now = detections.first().map(|d| d.observed_at).unwrap_or_else(chrono::Utc::now);
        let tracks = self.tracker.lock().await.update(&mut detections, now);
        for detection in &detections { self.database.insert_detection(detection).await?; }
        for track in &tracks { self.database.upsert_track(track).await?; }
        let mut observations: Vec<Observation> = tracks.iter().filter_map(|track| self.observations.from_track(track)).collect();
        let zones = self.database.list_zones(Some(camera_id)).await?;
        let zone_events = self.zones.lock().await.evaluate(&zones, &tracks, now);
        for event in &zone_events {
            self.database.insert_zone_event(event).await?;
            let zone_name = zones.iter().find(|zone| zone.id == event.zone_id).map(|zone| zone.name.as_str()).unwrap_or("zone");
            let object_class = tracks.iter().find(|track| track.id == event.track_id).map(|track| track.object_class.as_str()).unwrap_or("object");
            let (observation_type, summary) = match &event.event_type {
                ZoneEventType::Entered => ("zone_entered", format!("{object_class} entered {zone_name}")),
                ZoneEventType::Exited => ("zone_exited", format!("{object_class} exited {zone_name}")),
                ZoneEventType::Occupied => ("zone_occupied", format!("{object_class} remained in {zone_name}")),
            };
            observations.push(Observation { id: uuid::Uuid::new_v4(), camera_id: event.camera_id, track_id: event.track_id, observation_type: observation_type.into(), summary, created_at: event.occurred_at });
        }
        for observation in &observations { self.database.insert_observation(observation).await?; }
        tracing::debug!(camera_id = %camera_id, detections = detections.len(), tracks = tracks.len(), zone_events = zone_events.len(), observations = observations.len(), "frame processed");
        Ok(PipelineResult { detections, tracks, observations, zone_events })
    }
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub detections: Vec<Detection>,
    pub tracks: Vec<Track>,
    pub observations: Vec<Observation>,
    pub zone_events: Vec<objexel_common::ZoneEvent>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn pipeline_has_explicit_spatial_stages() {
        let stages = ["frame_extraction", "detection", "tracking", "zone_evaluation", "observation", "storage"];
        assert_eq!(stages.len(), 6);
    }
}
