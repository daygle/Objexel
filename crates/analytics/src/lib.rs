use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AnalyticsMetric { pub label: String, pub count: i64 }

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AnalyticsSummary {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub detections: i64,
    pub events: i64,
    pub observations: i64,
    #[serde(default)]
    pub behaviours: i64,
    pub top_objects: Vec<AnalyticsMetric>,
    pub camera_activity: Vec<AnalyticsMetric>,
    pub zone_activity: Vec<AnalyticsMetric>,
    pub model_activity: Vec<AnalyticsMetric>,
    pub detections_per_day: Vec<AnalyticsMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AnalyticsSnapshot { pub id: Uuid, pub period: String, pub period_start: DateTime<Utc>, pub summary: serde_json::Value, pub created_at: DateTime<Utc> }
