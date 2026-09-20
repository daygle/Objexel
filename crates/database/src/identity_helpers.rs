use objexel_common::{Identity, IdentityObservation, IdentityStatistics};
use sqlx::Row;
use uuid::Uuid;

pub fn identity_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Identity> { Ok(Identity { id: row.try_get("id")?, object_class: row.try_get("object_class")?, display_name: row.try_get("display_name")?, familiarity_score: row.try_get("familiarity_score")?, familiarity: row.try_get("familiarity")?, first_seen: row.try_get("first_seen")?, last_seen: row.try_get("last_seen")?, sightings: row.try_get("sightings")? }) }
pub fn identity_observation_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<IdentityObservation> { Ok(IdentityObservation { id: row.try_get("id")?, identity_id: row.try_get("identity_id")?, track_id: row.try_get("track_id")?, camera_id: row.try_get("camera_id")?, similarity: row.try_get("similarity")?, observed_at: row.try_get("observed_at")? }) }
pub fn identity_statistics_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<IdentityStatistics> { Ok(IdentityStatistics { identity_id: row.try_get("identity_id")?, average_duration_ms: row.try_get("average_duration_ms")?, active_days: row.try_get("active_days")?, top_zone_id: row.try_get("top_zone_id")? }) }

pub fn _identity_id(_: Uuid) {}
