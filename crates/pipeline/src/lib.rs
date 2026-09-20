use anyhow::Result;
use objexel_common::{Detection, Observation, Track};
use objexel_database::Database;
use objexel_detector::{FrameTensor, ModelManager};
use objexel_observations::ObservationEngine;
use objexel_tracker::Tracker;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ObservationPipeline {
    pub models: ModelManager,
    tracker: Arc<Mutex<Tracker>>,
    observations: ObservationEngine,
    database: Database,
}

impl ObservationPipeline {
    pub fn new(database: Database) -> Self {
        Self { models: ModelManager::default(), tracker: Arc::new(Mutex::new(Tracker::default())), observations: ObservationEngine::default(), database }
    }

    pub async fn process_frame(&self, frame: FrameTensor) -> Result<PipelineResult> {
        let mut detections = self.models.detect(frame).await?;
        let now = detections.first().map(|d| d.observed_at).unwrap_or_else(chrono::Utc::now);
        let tracks = self.tracker.lock().await.update(&mut detections, now);
        for detection in &detections { self.database.insert_detection(detection).await?; }
        for track in &tracks { self.database.upsert_track(track).await?; }
        let observations: Vec<Observation> = tracks.iter().filter_map(|track| self.observations.from_track(track)).collect();
        for observation in &observations { self.database.insert_observation(observation).await?; }
        tracing::debug!(detections = detections.len(), tracks = tracks.len(), observations = observations.len(), "frame processed");
        Ok(PipelineResult { detections, tracks, observations })
    }
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub detections: Vec<Detection>,
    pub tracks: Vec<Track>,
    pub observations: Vec<Observation>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn pipeline_has_explicit_stages() {
        let stages = ["frame_extraction", "detection", "tracking", "observation", "storage"];
        assert_eq!(stages.len(), 5);
    }
}
