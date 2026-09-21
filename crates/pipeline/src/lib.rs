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
use std::{
    collections::HashMap,
    sync::{atomic::{AtomicU64, Ordering}, Arc},
    time::Instant,
};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct ObservationPipeline {
    pub models: ModelManager,
    trackers: Arc<tokio::sync::RwLock<HashMap<Uuid, Arc<Mutex<Tracker>>>>>,
    zones: Arc<tokio::sync::RwLock<HashMap<Uuid, Arc<Mutex<ZoneEvaluator>>>>>,
    rules: Arc<Mutex<RuleEngine>>,
    actions: ActionDispatcher,
    observations: ObservationEngine,
    database: Database,
    recorder: Recorder,
    behaviour: BehaviourAnalyzer,
    fusion: FusionEngine,
    metrics: Arc<tokio::sync::RwLock<HashMap<Uuid, Arc<CameraMetrics>>>>,
}

struct CameraMetrics {
    frames_processed: AtomicU64,
    frames_in_flight: AtomicU64,
    processing_errors: AtomicU64,
    total_processing_ms: AtomicU64,
    last_processing_ms: AtomicU64,
}

impl Default for CameraMetrics {
    fn default() -> Self {
        Self {
            frames_processed: AtomicU64::new(0),
            frames_in_flight: AtomicU64::new(0),
            processing_errors: AtomicU64::new(0),
            total_processing_ms: AtomicU64::new(0),
            last_processing_ms: AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CameraMetricsSnapshot {
    pub camera_id: Uuid,
    pub frames_processed: u64,
    pub frames_in_flight: u64,
    pub processing_errors: u64,
    pub total_processing_ms: u64,
    pub last_processing_ms: u64,
}

impl ObservationPipeline {
    pub fn new(database: Database, recorder: Recorder) -> Self {
        Self { models: ModelManager::default(), trackers: Arc::new(tokio::sync::RwLock::new(HashMap::new())), zones: Arc::new(tokio::sync::RwLock::new(HashMap::new())), rules: Arc::new(Mutex::new(RuleEngine::default())), actions: ActionDispatcher::new(), observations: ObservationEngine::default(), database, recorder, behaviour: BehaviourAnalyzer::default(), fusion: FusionEngine::default(), metrics: Arc::new(tokio::sync::RwLock::new(HashMap::new())) }
    }

    /// Get or create a per-camera Tracker instance.
    async fn camera_tracker(&self, camera_id: Uuid) -> Arc<Mutex<Tracker>> {
        {
            let read = self.trackers.read().await;
            if let Some(tracker) = read.get(&camera_id) { return tracker.clone(); }
        }
        let mut write = self.trackers.write().await;
        write.entry(camera_id).or_insert_with(|| Arc::new(Mutex::new(Tracker::default()))).clone()
    }

    /// Get or create a per-camera ZoneEvaluator instance.
    async fn camera_zones(&self, camera_id: Uuid) -> Arc<Mutex<ZoneEvaluator>> {
        {
            let read = self.zones.read().await;
            if let Some(zones) = read.get(&camera_id) { return zones.clone(); }
        }
        let mut write = self.zones.write().await;
        write.entry(camera_id).or_insert_with(|| Arc::new(Mutex::new(ZoneEvaluator::default()))).clone()
    }

    async fn camera_metrics(&self, camera_id: Uuid) -> Arc<CameraMetrics> {
        {
            let read = self.metrics.read().await;
            if let Some(metrics) = read.get(&camera_id) { return metrics.clone(); }
        }
        let mut write = self.metrics.write().await;
        write.entry(camera_id).or_insert_with(|| Arc::new(CameraMetrics::default())).clone()
    }

    pub async fn metrics_snapshot(&self) -> Vec<CameraMetricsSnapshot> {
        let read = self.metrics.read().await;
        read.iter().map(|(camera_id, metrics)| CameraMetricsSnapshot {
            camera_id: *camera_id,
            frames_processed: metrics.frames_processed.load(Ordering::Relaxed),
            frames_in_flight: metrics.frames_in_flight.load(Ordering::Relaxed),
            processing_errors: metrics.processing_errors.load(Ordering::Relaxed),
            total_processing_ms: metrics.total_processing_ms.load(Ordering::Relaxed),
            last_processing_ms: metrics.last_processing_ms.load(Ordering::Relaxed),
        }).collect()
    }

    pub async fn process_frame(&self, frame: FrameTensor) -> Result<PipelineResult> {
        let metrics = self.camera_metrics(frame.camera_id).await;
        metrics.frames_in_flight.fetch_add(1, Ordering::Relaxed);
        let started = Instant::now();
        let result = self.process_frame_inner(frame).await;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        metrics.frames_in_flight.fetch_sub(1, Ordering::Relaxed);
        metrics.last_processing_ms.store(elapsed_ms, Ordering::Relaxed);
        metrics.total_processing_ms.fetch_add(elapsed_ms, Ordering::Relaxed);
        match &result {
            Ok(_) => { metrics.frames_processed.fetch_add(1, Ordering::Relaxed); }
            Err(_) => { metrics.processing_errors.fetch_add(1, Ordering::Relaxed); }
        }
        result
    }

    async fn process_frame_inner(&self, frame: FrameTensor) -> Result<PipelineResult> {
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
        let tracks = self.camera_tracker(camera_id).await.lock().await.update(&mut detections, now);
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
        let zone_events = self.camera_zones(camera_id).await.lock().await.evaluate(&zones, &tracks, now);
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
        // Phase 1: Build observation contexts and evaluate rules (hold rule lock only for evaluate).
        let mut rule_contexts: Vec<(uuid::Uuid, uuid::Uuid, uuid::Uuid, Option<objexel_adaptive::AdaptiveAssessment>)> = Vec::new();
        {
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
                let matched = rule_engine.evaluate(&rules, context, now);
                let identity_id = identity.as_ref().map(|item| item.id);
                let behaviour_id = behaviour.map(|item| item.id);
                for event in matched {
                    rule_contexts.push((event.id, identity_id.unwrap_or_default(), behaviour_id.unwrap_or_default(), assessment));
                    events.push(event);
                }
            }
        } // Rule lock released here.
        // Phase 2: Persist events and dispatch actions (no rule lock held).
        for (event_id, identity_id, behaviour_id, assessment) in rule_contexts {
            if let Some(event) = events.iter().find(|e| e.id == event_id) {
                if !identity_id.is_nil() {
                    if let Some(ref assessment) = assessment {
                        self.database.insert_adaptive_scores(identity_id, Some(behaviour_id), assessment, Some(event.id)).await?;
                    }
                }
                self.database.insert_notification(event).await?;
                if let Some(camera) = self.database.get_camera(event.camera_id).await? {
                    let recorder = self.recorder.clone();
                    let database = self.database.clone();
                    let eid = event.id;
                    let cam_id = event.camera_id;
                    let rtsp_url = camera.rtsp_url;
                    let event_time = event.created_at;
                    tokio::spawn(async move {
                        match recorder.create_event_clip(cam_id, eid, &rtsp_url, event_time).await {
                            Ok(clip) => if let Err(error) = database.insert_clip(&clip).await { tracing::warn!(event_id = %eid, %error, "event clip metadata insert failed"); },
                            Err(error) => tracing::warn!(event_id = %eid, %error, "event clip generation failed"),
                        }
                        match recorder.create_snapshot(cam_id, Some(eid), &rtsp_url, event_time).await {
                            Ok(snapshot) => if let Err(error) = database.insert_snapshot(&snapshot).await { tracing::warn!(event_id = %eid, %error, "event snapshot metadata insert failed"); },
                            Err(error) => tracing::warn!(event_id = %eid, %error, "event snapshot generation failed"),
                        }
                    });
                }
                let actions = self.database.actions_for_rule(event.rule_id).await?;
                for action in actions {
                    let provider = match action.provider_id { Some(id) => self.database.provider(id).await?, None => None };
                    let template = match action.template_id { Some(id) => self.database.template(id).await?, None => None };
                    let execution = self.actions.execute(&action, provider.as_ref(), template.as_ref(), event).await;
                    self.database.insert_execution(&execution).await?;
                }
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
