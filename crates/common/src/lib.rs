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

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
}
