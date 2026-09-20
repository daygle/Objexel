use anyhow::Context;
use objexel_common::{Camera, CameraStatus, CreateCamera, Detection, Model, Observation, Track, UpdateCamera};
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .context("connect to PostgreSQL")?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self { Self { pool } }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::migrate!("../../migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn list_cameras(&self) -> anyhow::Result<Vec<Camera>> {
        let rows = sqlx::query(
            "SELECT id, name, rtsp_url, enabled, status, last_connected_at, last_snapshot_at, last_error, created_at, updated_at FROM cameras ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(camera_from_row).collect()
    }

    pub async fn create_camera(&self, input: CreateCamera) -> anyhow::Result<Camera> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO cameras (id, name, rtsp_url, enabled) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(&input.name)
            .bind(&input.rtsp_url)
            .bind(input.enabled)
            .execute(&self.pool)
            .await?;
        self.get_camera(id).await?.context("camera was not returned after insert")
    }

    pub async fn get_camera(&self, id: Uuid) -> anyhow::Result<Option<Camera>> {
        let row = sqlx::query(
            "SELECT id, name, rtsp_url, enabled, status, last_connected_at, last_snapshot_at, last_error, created_at, updated_at FROM cameras WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(camera_from_row).transpose()
    }

    pub async fn update_camera(&self, id: Uuid, input: UpdateCamera) -> anyhow::Result<Option<Camera>> {
        let result = sqlx::query(
            "UPDATE cameras SET name = COALESCE($2, name), rtsp_url = COALESCE($3, rtsp_url), enabled = COALESCE($4, enabled) WHERE id = $1",
        )
        .bind(id)
        .bind(input.name)
        .bind(input.rtsp_url)
        .bind(input.enabled)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 { return Ok(None); }
        self.get_camera(id).await
    }

    pub async fn update_camera_health(
        &self,
        id: Uuid,
        status: CameraStatus,
        last_connected_at: Option<chrono::DateTime<chrono::Utc>>,
        last_snapshot_at: Option<chrono::DateTime<chrono::Utc>>,
        last_error: Option<&str>,
    ) -> anyhow::Result<Option<Camera>> {
        let status = status_string(&status);
        let result = sqlx::query(
            "UPDATE cameras SET status = $2, last_connected_at = COALESCE($3, last_connected_at), last_snapshot_at = COALESCE($4, last_snapshot_at), last_error = $5 WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(last_connected_at)
        .bind(last_snapshot_at)
        .bind(last_error)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 { return Ok(None); }
        self.get_camera(id).await
    }

    pub async fn list_models(&self) -> anyhow::Result<Vec<Model>> {
        let rows = sqlx::query("SELECT id, name, version, path, active, created_at FROM models ORDER BY name")
            .fetch_all(&self.pool).await?;
        rows.into_iter().map(model_from_row).collect()
    }

    pub async fn list_detections(&self, limit: i64) -> anyhow::Result<Vec<Detection>> {
        let rows = sqlx::query("SELECT id, camera_id, track_id, model_id, object_class, confidence, bbox_x, bbox_y, bbox_width, bbox_height, observed_at FROM detections ORDER BY observed_at DESC LIMIT $1")
            .bind(limit.clamp(1, 500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(detection_from_row).collect()
    }

    pub async fn get_detection(&self, id: Uuid) -> anyhow::Result<Option<Detection>> {
        let row = sqlx::query("SELECT id, camera_id, track_id, model_id, object_class, confidence, bbox_x, bbox_y, bbox_width, bbox_height, observed_at FROM detections WHERE id = $1")
            .bind(id).fetch_optional(&self.pool).await?;
        row.map(detection_from_row).transpose()
    }

    pub async fn list_tracks(&self, limit: i64) -> anyhow::Result<Vec<Track>> {
        let rows = sqlx::query("SELECT id, camera_id, object_class, first_seen, last_seen, duration_ms, movement_path FROM tracks ORDER BY last_seen DESC LIMIT $1")
            .bind(limit.clamp(1, 500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(track_from_row).collect()
    }

    pub async fn get_track(&self, id: Uuid) -> anyhow::Result<Option<Track>> {
        let row = sqlx::query("SELECT id, camera_id, object_class, first_seen, last_seen, duration_ms, movement_path FROM tracks WHERE id = $1")
            .bind(id).fetch_optional(&self.pool).await?;
        row.map(track_from_row).transpose()
    }

    pub async fn list_observations(&self, limit: i64) -> anyhow::Result<Vec<Observation>> {
        let rows = sqlx::query("SELECT id, camera_id, track_id, observation_type, summary, created_at FROM observations ORDER BY created_at DESC LIMIT $1")
            .bind(limit.clamp(1, 500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(observation_from_row).collect()
    }

    pub async fn get_observation(&self, id: Uuid) -> anyhow::Result<Option<Observation>> {
        let row = sqlx::query("SELECT id, camera_id, track_id, observation_type, summary, created_at FROM observations WHERE id = $1")
            .bind(id).fetch_optional(&self.pool).await?;
        row.map(observation_from_row).transpose()
    }

    pub async fn insert_detection(&self, detection: &Detection) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO detections (id, camera_id, track_id, model_id, label, object_class, confidence, bbox_x, bbox_y, bbox_width, bbox_height, observed_at) VALUES ($1,$2,$3,$4,$5,$5,$6,$7,$8,$9,$10,$11)")
            .bind(detection.id).bind(detection.camera_id).bind(detection.track_id).bind(detection.model_id)
            .bind(&detection.object_class).bind(detection.confidence).bind(detection.bounding_box.x).bind(detection.bounding_box.y)
            .bind(detection.bounding_box.width).bind(detection.bounding_box.height).bind(detection.observed_at)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_track(&self, track: &Track) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO tracks (id, camera_id, object_class, first_seen, last_seen, duration_ms, movement_path) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (id) DO UPDATE SET last_seen=EXCLUDED.last_seen, duration_ms=EXCLUDED.duration_ms, movement_path=EXCLUDED.movement_path")
            .bind(track.id).bind(track.camera_id).bind(&track.object_class).bind(track.first_seen).bind(track.last_seen).bind(track.duration_ms)
            .bind(serde_json::to_value(&track.movement_path)?).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn insert_observation(&self, observation: &Observation) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO observations (id, camera_id, track_id, observation_type, summary, created_at) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(observation.id).bind(observation.camera_id).bind(observation.track_id).bind(&observation.observation_type).bind(&observation.summary).bind(observation.created_at)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn delete_camera(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM cameras WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }
}

fn camera_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Camera> {
    let status = match row.try_get::<String, _>("status")?.as_str() {
        "online" => CameraStatus::Online,
        "offline" => CameraStatus::Offline,
        "degraded" => CameraStatus::Degraded,
        _ => CameraStatus::Unknown,
    };
    Ok(Camera {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        rtsp_url: row.try_get("rtsp_url")?,
        enabled: row.try_get("enabled")?,
        status,
        created_at: row.try_get("created_at")?,
        last_connected_at: row.try_get("last_connected_at")?,
        last_snapshot_at: row.try_get("last_snapshot_at")?,
        last_error: row.try_get("last_error")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn model_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Model> {
    Ok(Model { id: row.try_get("id")?, name: row.try_get("name")?, version: row.try_get("version")?, path: row.try_get("path")?, active: row.try_get("active")?, created_at: row.try_get("created_at")? })
}

fn detection_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Detection> {
    Ok(Detection { id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, track_id: row.try_get("track_id")?, model_id: row.try_get("model_id")?, object_class: row.try_get("object_class")?, confidence: row.try_get("confidence")?, bounding_box: objexel_common::BoundingBox { x: row.try_get("bbox_x")?, y: row.try_get("bbox_y")?, width: row.try_get("bbox_width")?, height: row.try_get("bbox_height")? }, observed_at: row.try_get("observed_at")? })
}

fn track_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Track> {
    let path: Value = row.try_get("movement_path")?;
    Ok(Track { id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, object_class: row.try_get("object_class")?, first_seen: row.try_get("first_seen")?, last_seen: row.try_get("last_seen")?, duration_ms: row.try_get("duration_ms")?, movement_path: serde_json::from_value(path)? })
}

fn observation_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Observation> {
    Ok(Observation { id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, track_id: row.try_get("track_id")?, observation_type: row.try_get("observation_type")?, summary: row.try_get("summary")?, created_at: row.try_get("created_at")? })
}

fn status_string(status: &CameraStatus) -> &'static str {
    match status {
        CameraStatus::Online => "online",
        CameraStatus::Offline => "offline",
        CameraStatus::Degraded => "degraded",
        CameraStatus::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use objexel_common::{CameraStatus, UpdateCamera};
    #[test]
    fn unknown_status_is_safe() {
        assert_eq!(CameraStatus::default(), CameraStatus::Unknown);
    }

    #[test]
    fn update_input_can_be_partial() {
        let input = UpdateCamera { name: Some("Back yard".into()), rtsp_url: None, enabled: None };
        assert_eq!(input.name.as_deref(), Some("Back yard"));
        assert!(input.rtsp_url.is_none());
    }
}
