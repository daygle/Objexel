use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraStatus {
    Online,
    Offline,
    Degraded,
    Unknown,
}

impl Default for CameraStatus {
    fn default() -> Self { Self::Unknown }
}

impl Default for EventSeverity {
    fn default() -> Self { Self::Info }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Camera {
    pub id: Uuid,
    pub name: String,
    pub rtsp_url: String,
    pub enabled: bool,
    pub status: CameraStatus,
    pub active_model_id: Option<Uuid>,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub last_snapshot_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateCamera {
    pub name: String,
    pub rtsp_url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateCamera {
    pub name: Option<String>,
    pub rtsp_url: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StreamMetadata {
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CameraTestResult {
    pub camera_id: Uuid,
    pub status: CameraStatus,
    pub latency_ms: Option<u64>,
    pub metadata: Option<StreamMetadata>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Model {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub model_type: String,
    pub path: String,
    pub input_width: u32,
    pub input_height: u32,
    pub class_list: Vec<String>,
    pub enabled: bool,
    pub default_model: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateModel {
    pub name: String,
    pub version: String,
    pub model_type: String,
    pub path: String,
    #[serde(default = "default_input_size")]
    pub input_width: u32,
    #[serde(default = "default_input_size")]
    pub input_height: u32,
    #[serde(default)]
    pub class_list: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub default_model: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub model_type: String,
    pub download_url: String,
    pub sha256: String,
    #[serde(default = "default_input_size")]
    pub input_width: u32,
    #[serde(default = "default_input_size")]
    pub input_height: u32,
    #[serde(default)]
    pub class_list: Vec<String>,
    #[serde(default)]
    pub archive_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateModelEnabled {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ModelDownload {
    pub id: Uuid,
    pub catalog_id: String,
    pub status: String,
    pub progress_percent: u8,
    pub bytes_downloaded: i64,
    pub total_bytes: Option<i64>,
    pub error: Option<String>,
    pub model_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_input_size() -> u32 { 640 }

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BenchmarkResult {
    pub id: Uuid,
    pub model_id: Uuid,
    pub fps: f32,
    pub average_inference_time_ms: f32,
    pub gpu_memory_usage_mb: Option<u64>,
    pub cpu_usage_percent: Option<f32>,
    pub test_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ModelAssignment {
    pub id: Uuid,
    pub camera_id: Uuid,
    pub model_id: Uuid,
    pub priority: i32,
    pub confidence_threshold: f32,
    pub fps_limit: Option<f32>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateModelAssignment {
    pub camera_id: Uuid,
    pub model_id: Uuid,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_assignment_threshold")]
    pub confidence_threshold: f32,
    pub fps_limit: Option<f32>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_assignment_threshold() -> f32 { 0.25 }

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FusionResult {
    pub id: Uuid,
    pub camera_id: Uuid,
    pub detection_id: Uuid,
    pub source_model_ids: Vec<Uuid>,
    pub fused_confidence: f32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Detection {
    pub id: Uuid,
    pub camera_id: Uuid,
    pub track_id: Option<Uuid>,
    pub model_id: Option<Uuid>,
    pub object_class: String,
    pub confidence: f32,
    pub bounding_box: BoundingBox,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Track {
    pub id: Uuid,
    pub camera_id: Uuid,
    pub object_class: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub duration_ms: i64,
    pub movement_path: Vec<BoundingBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Observation {
    pub id: Uuid,
    pub camera_id: Uuid,
    pub track_id: Uuid,
    pub observation_type: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PolygonPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Zone {
    pub id: Uuid,
    pub camera_id: Uuid,
    pub name: String,
    pub polygon_coordinates: Vec<PolygonPoint>,
    pub colour: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateZone {
    pub camera_id: Uuid,
    pub name: String,
    pub polygon_coordinates: Vec<PolygonPoint>,
    #[serde(default = "default_zone_colour")]
    pub colour: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_zone_colour() -> String { "#74e0b4".into() }

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateZone {
    pub name: Option<String>,
    pub polygon_coordinates: Option<Vec<PolygonPoint>>,
    pub colour: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZoneEventType { Entered, Exited, Occupied }

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ZoneEvent {
    pub id: Uuid,
    pub zone_id: Uuid,
    pub camera_id: Uuid,
    pub track_id: Uuid,
    pub event_type: ZoneEventType,
    pub occurred_at: DateTime<Utc>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventSeverity { Info, Warning, Critical }

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RuleCondition {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub object_class: Option<String>,
    pub zone_id: Option<Uuid>,
    pub observation_type: Option<String>,
    pub behaviour_type: Option<String>,
    pub confidence_threshold: Option<f32>,
    pub minimum_duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Rule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub cooldown_seconds: i64,
    pub suppression_seconds: i64,
    pub severity: EventSeverity,
    pub conditions: Vec<RuleCondition>,
    #[serde(default)]
    pub action_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRule {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub cooldown_seconds: i64,
    #[serde(default)]
    pub suppression_seconds: i64,
    #[serde(default)]
    pub severity: EventSeverity,
    pub conditions: Vec<RuleConditionInput>,
    #[serde(default)]
    pub action_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RuleConditionInput {
    pub object_class: Option<String>,
    pub zone_id: Option<Uuid>,
    pub observation_type: Option<String>,
    pub behaviour_type: Option<String>,
    pub identity_id: Option<Uuid>,
    pub familiarity: Option<String>,
    pub confidence_threshold: Option<f32>,
    pub minimum_duration_ms: Option<i64>,
    pub minimum_priority: Option<String>,
    pub minimum_anomaly_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateRule {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub description: Option<String>,
    pub cooldown_seconds: Option<i64>,
    pub suppression_seconds: Option<i64>,
    pub severity: Option<EventSeverity>,
    pub conditions: Option<Vec<RuleConditionInput>>,
    pub action_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Event {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub camera_id: Uuid,
    pub track_id: Option<Uuid>,
    pub observation_id: Option<Uuid>,
    pub event_type: String,
    pub summary: String,
    pub severity: EventSeverity,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NotificationProvider {
    pub id: Uuid,
    pub provider_type: String,
    pub enabled: bool,
    pub configuration: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NotificationTemplate {
    pub id: Uuid,
    pub name: String,
    pub subject: String,
    pub body: String,
    pub html: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Action {
    pub id: Uuid,
    pub name: String,
    pub action_type: String,
    pub provider_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
    pub enabled: bool,
    pub configuration: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateAction {
    pub name: String,
    pub action_type: String,
    pub provider_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub configuration: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateAction {
    pub name: Option<String>,
    pub provider_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
    pub enabled: Option<bool>,
    pub configuration: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ActionExecution {
    pub id: Uuid,
    pub action_id: Uuid,
    pub event_id: Uuid,
    pub status: String,
    pub execution_time_ms: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Notification {
    pub id: Uuid,
    pub event_id: Option<Uuid>,
    pub title: String,
    pub body: String,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNotificationProvider {
    pub provider_type: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub configuration: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateNotificationProvider {
    pub enabled: Option<bool>,
    pub configuration: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNotificationTemplate {
    pub name: String,
    #[serde(default)]
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub html: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Recording {
    pub id: Uuid,
    pub camera_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Clip {
    pub id: Uuid,
    pub event_id: Option<Uuid>,
    pub recording_id: Option<Uuid>,
    pub clip_start: DateTime<Utc>,
    pub clip_end: DateTime<Utc>,
    pub clip_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Snapshot {
    pub id: Uuid,
    pub event_id: Option<Uuid>,
    pub camera_id: Uuid,
    pub image_path: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Behaviour {
    pub id: Uuid,
    pub track_id: Uuid,
    pub camera_id: Uuid,
    pub object_class: String,
    pub behaviour_type: String,
    pub confidence: f32,
    pub summary: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub recording_id: Option<Uuid>,
    pub clip_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Identity {
    pub id: Uuid,
    pub object_class: String,
    pub display_name: Option<String>,
    pub familiarity_score: f32,
    pub familiarity: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub sightings: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IdentityObservation {
    pub id: Uuid,
    pub identity_id: Uuid,
    pub track_id: Uuid,
    pub camera_id: Uuid,
    pub similarity: f32,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IdentityStatistics {
    pub identity_id: Uuid,
    pub average_duration_ms: i64,
    pub active_days: i64,
    pub top_zone_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateIdentity {
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IdentityScore {
    pub id: Uuid,
    pub identity_id: Uuid,
    pub familiarity_score: f32,
    pub confidence: f32,
    pub scored_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BehaviourScore {
    pub id: Uuid,
    pub behaviour_id: Uuid,
    pub anomaly_score: f32,
    pub behaviour_level: String,
    pub confidence: f32,
    pub scored_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AnomalyEvent {
    pub id: Uuid,
    pub event_id: Option<Uuid>,
    pub identity_id: Option<Uuid>,
    pub behaviour_id: Option<Uuid>,
    pub anomaly_score: f32,
    pub priority_score: f32,
    pub priority: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IntelligenceSummary {
    pub identities: Vec<Identity>,
    pub anomalies: Vec<AnomalyEvent>,
    pub highest_priority: Vec<AnomalyEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
}
