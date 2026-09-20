use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchFilters {
    pub q: Option<String>,
    pub camera_id: Option<Uuid>,
    pub object_class: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub zone_id: Option<Uuid>,
    pub confidence_min: Option<f32>,
    pub observation_type: Option<String>,
    pub event_type: Option<String>,
    pub model_id: Option<Uuid>,
    pub limit: Option<i64>,
}

impl SearchFilters { pub fn limit(&self) -> i64 { self.limit.unwrap_or(100).clamp(1, 500) } }

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SearchResult {
    pub entity_type: String,
    pub id: Uuid,
    pub camera_id: Uuid,
    pub object_class: Option<String>,
    pub summary: String,
    pub occurred_at: DateTime<Utc>,
    pub confidence: Option<f32>,
    pub zone_id: Option<Uuid>,
    pub model_id: Option<Uuid>,
    pub recording_id: Option<Uuid>,
    pub clip_id: Option<Uuid>,
    pub snapshot_id: Option<Uuid>,
}
