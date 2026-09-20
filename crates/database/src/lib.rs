use anyhow::Context;
use objexel_common::{Camera, CameraStatus, CreateCamera, UpdateCamera};
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
