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
    pub path: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RuleConditionInput {
    pub object_class: Option<String>,
    pub zone_id: Option<Uuid>,
    pub observation_type: Option<String>,
    pub confidence_threshold: Option<f32>,
    pub minimum_duration_ms: Option<i64>,
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

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
}
