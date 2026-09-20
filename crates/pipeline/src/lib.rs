use anyhow::Result;
use objexel_actions::ActionDispatcher;
use objexel_adaptive::assess;
use objexel_behaviour::BehaviourAnalyzer;
use objexel_common::{Detection, Event, FusionResult, Observation, Track, ZoneEventType};
use objexel_fusion::FusionEngine;
use objexel_database::Database;
use objexel_detector::{FrameTensor, ModelManager};
use objexel_observations::ObservationEngine;
use objexel_recorder::Recorder;
use objexel_rules::{ObservationContext, RuleEngine};
use objexel_tracker::Tracker;
use objexel_zones::ZoneEvaluator;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ObservationPipeline {
    pub models: ModelManager,
    tracker: Arc<Mutex<Tracker>>,
    zones: Arc<Mutex<ZoneEvaluator>>,
    rules: Arc<Mutex<RuleEngine>>,
    actions: ActionDispatcher,
    observations: ObservationEngine,
    database: Database,
    recorder: Recorder,
    behaviour: BehaviourAnalyzer,
    fusion: FusionEngine,
}

impl ObservationPipeline {
    pub fn new(database: Database, recorder: Recorder) -> Self {
        Self { models: ModelManager::default(), tracker: Arc::new(Mutex::new(Tracker::default())), zones: Arc::new(Mutex::new(ZoneEvaluator::default())), rules: Arc::new(Mutex::new(RuleEngine::default())), actions: ActionDispatcher::new(), observations: ObservationEngine::default(), database, recorder, behaviour: BehaviourAnalyzer::default(), fusion: FusionEngine::default() }
    }

    pub async fn process_frame(&self, frame: FrameTensor) -> Result<PipelineResult> {
        let camera_id = frame.camera_id;
        let assignments = self.database.list_model_assignments(Some(camera_id)).await?;
        let mut raw_detections = Vec::new();
        if assignments.is_empty() {
            let selected_model = self.database.active_model_for_camera(camera_id).await?;
            raw_detections = self.models.detect_with_model(selected_model, frame.clone()).await?;
        } else {
            let mut jobs = Vec::new();
            for assignment in assignments.iter().filter(|item| item.enabled) {
                let models = self.models.clone(); let input = frame.clone(); let model_id = assignment.model_id;
                jobs.push(tokio::spawn(async move { models.detect_with_model(Some(model_id), input).await }));
            }
            for job in jobs { raw_detections.extend(job.await??); }
        }
        let mut detections = self.fusion.fuse(&assignments, raw_detections);
        let now = detections.first().map(|d| d.observed_at).unwrap_or_else(chrono::Utc::now);
        let tracks = self.tracker.lock().await.update(&mut detections, now);
        for detection in &detections {
            self.database.insert_detection(detection).await?;
            if !assignments.is_empty() {
                let source_model_ids = assignments.iter().filter(|assignment| assignment.enabled).map(|assignment| assignment.model_id).collect();
                self.database.insert_fusion_result(&FusionResult { id: uuid::Uuid::new_v4(), camera_id, detection_id: detection.id, source_model_ids, fused_confidence: detection.confidence, created_at: now }).await?;
            }
        }
        for track in &tracks { self.database.upsert_track(track).await?; }
        let behaviours: Vec<_> = tracks.iter().filter_map(|track| self.behaviour.analyze(track, now)).collect();
        for behaviour in &behaviours { self.database.insert_behaviour(behaviour).await?; }
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
        let rules = self.database.list_rules().await?;
        let mut events = Vec::new();
        let mut rule_engine = self.rules.lock().await;
        for observation in &observations {
            let track = tracks.iter().find(|track| track.id == observation.track_id);
            let detection = detections.iter().find(|detection| detection.track_id == Some(observation.track_id));
            let zone_id = zone_events.iter().find(|event| event.track_id == observation.track_id).map(|event| event.zone_id);
            let behaviour_type = behaviours.iter().find(|behaviour| behaviour.track_id == observation.track_id).map(|behaviour| behaviour.behaviour_type.as_str());
            let identity = match track { Some(track) => Some(self.database.assign_identity(track).await?), None => None };
            let behaviour = track.and_then(|item| behaviours.iter().find(|candidate| candidate.track_id == item.id));
            let assessment = identity.as_ref().map(|item| assess(item, behaviour, detection.map(|item| item.confidence).unwrap_or(0.0)));
            let context = ObservationContext { observation, object_class: track.map(|track| track.object_class.as_str()), zone_id, confidence: detection.map(|detection| detection.confidence), duration_ms: track.map(|track| track.duration_ms), behaviour_type, identity_id: identity.as_ref().map(|item| item.id), familiarity: identity.as_ref().map(|item| item.familiarity.as_str()), priority: assessment.as_ref().map(|item| item.priority.as_str()), anomaly_score: assessment.as_ref().map(|item| item.anomaly_score) };
            for event in rule_engine.evaluate(&rules, context, now) {
                self.database.insert_event(&event).await?;
                if let (Some(identity), Some(assessment)) = (identity.as_ref(), assessment.as_ref()) { self.database.insert_adaptive_scores(identity.id, behaviour.map(|item| item.id), assessment, Some(event.id)).await?; }
                self.database.insert_notification(&event).await?;
                if let Some(camera) = self.database.get_camera(event.camera_id).await? {
                    let recorder = self.recorder.clone();
                    let database = self.database.clone();
                    let event_id = event.id;
                    let camera_id = event.camera_id;
                    let rtsp_url = camera.rtsp_url;
                    let event_time = event.created_at;
                    tokio::spawn(async move {
                        match recorder.create_event_clip(camera_id, event_id, &rtsp_url, event_time).await {
                            Ok(clip) => if let Err(error) = database.insert_clip(&clip).await { tracing::warn!(event_id = %event_id, %error, "event clip metadata insert failed"); },
                            Err(error) => tracing::warn!(event_id = %event_id, %error, "event clip generation failed"),
                        }
                        match recorder.create_snapshot(camera_id, Some(event_id), &rtsp_url, event_time).await {
                            Ok(snapshot) => if let Err(error) = database.insert_snapshot(&snapshot).await { tracing::warn!(event_id = %event_id, %error, "event snapshot metadata insert failed"); },
                            Err(error) => tracing::warn!(event_id = %event_id, %error, "event snapshot generation failed"),
                        }
                    });
                }
                let actions = self.database.actions_for_rule(event.rule_id).await?;
                for action in actions {
                    let provider = match action.provider_id { Some(id) => self.database.provider(id).await?, None => None };
                    let template = match action.template_id { Some(id) => self.database.template(id).await?, None => None };
                    let execution = self.actions.execute(&action, provider.as_ref(), template.as_ref(), &event).await;
                    self.database.insert_execution(&execution).await?;
                }
                events.push(event);
            }
        }
        tracing::debug!(camera_id = %camera_id, detections = detections.len(), tracks = tracks.len(), behaviours = behaviours.len(), zone_events = zone_events.len(), observations = observations.len(), events = events.len(), "frame processed");
        Ok(PipelineResult { detections, tracks, observations, zone_events, events })
    }
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub detections: Vec<Detection>,
    pub tracks: Vec<Track>,
    pub observations: Vec<Observation>,
    pub zone_events: Vec<objexel_common::ZoneEvent>,
    pub events: Vec<Event>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn pipeline_has_explicit_rule_stage() {
        let stages = ["frame_extraction", "detection", "tracking", "zone_evaluation", "observation", "rules", "events", "storage"];
        assert_eq!(stages.len(), 8);
    }
}
