use anyhow::Context;
use objexel_auth::{Role, SessionUser, User};
use objexel_analytics::{AnalyticsMetric, AnalyticsSnapshot, AnalyticsSummary};
use objexel_adaptive::AdaptiveAssessment;
use objexel_behaviour::Behaviour;
use objexel_identity::{familiarity, new_identity, score, similarity, signature, ObjectSignature};
use objexel_common::{CreateModelAssignment, FusionResult, Identity, IdentityObservation, IdentityStatistics, ModelAssignment, UpdateIdentity};
use objexel_search::{SearchFilters, SearchResult};
use objexel_common::{Action, ActionExecution, BenchmarkResult, Camera, CameraStatus, Clip, CreateAction, CreateCamera, CreateModel, CreateNotificationProvider, CreateNotificationTemplate, CreateRule, CreateZone, Detection, Event, EventSeverity, Model, Notification, NotificationProvider, NotificationTemplate, Observation, Recording, Rule, RuleCondition, RuleConditionInput, Snapshot, Track, UpdateAction, UpdateCamera, UpdateNotificationProvider, UpdateRule, UpdateZone, Zone, ZoneEvent, ZoneEventType};
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use uuid::Uuid;
mod identity_helpers;
use identity_helpers::{identity_from_row, identity_observation_from_row, identity_statistics_from_row};

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .min_connections(1)
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(10))
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

    /// Append an operational audit record. Callers must pass redacted details and never secrets.
    pub async fn audit(&self, actor_id: Option<Uuid>, action: &str, resource_type: &str, resource_id: Option<Uuid>, details: Value) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO audit_logs (id, actor_id, action, resource_type, resource_id, details) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(Uuid::new_v4())
            .bind(actor_id)
            .bind(action)
            .bind(resource_type)
            .bind(resource_id)
            .bind(details)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn user_count(&self) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&self.pool).await?)
    }

    pub async fn create_user(&self, id: Uuid, username: &str, email: Option<&str>, password_hash: &str, role: Role) -> anyhow::Result<User> {
        sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES ($1, $2, $3, $4, $5)")
            .bind(id).bind(username).bind(email).bind(password_hash).bind(role.as_str()).execute(&self.pool).await?;
        self.get_user(id).await?.context("user was not returned after insert")
    }

    pub async fn get_user(&self, id: Uuid) -> anyhow::Result<Option<User>> {
        sqlx::query("SELECT id, username, email, role, enabled, created_at FROM users WHERE id = $1")
            .bind(id).fetch_optional(&self.pool).await?.map(user_from_row).transpose()
    }

    pub async fn get_user_credentials(&self, username: &str) -> anyhow::Result<Option<(User, String)>> {
        sqlx::query("SELECT id, username, email, role, enabled, created_at, password_hash FROM users WHERE username = $1")
            .bind(username).fetch_optional(&self.pool).await?.map(|row| Ok((user_from_row(row.clone())?, row.try_get("password_hash")?))).transpose()
    }

    pub async fn update_user(&self, id: Uuid, email: Option<Option<&str>>, password_hash: Option<&str>, role: Option<Role>, enabled: Option<bool>) -> anyhow::Result<Option<User>> {
        let role_name = role.map(|value| value.as_str().to_owned());
        let result = sqlx::query("UPDATE users SET email = COALESCE($2, email), password_hash = COALESCE($3, password_hash), role = COALESCE($4, role), enabled = COALESCE($5, enabled) WHERE id = $1")
            .bind(id).bind(email.flatten()).bind(password_hash).bind(role_name).bind(enabled).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Ok(None); }
        self.get_user(id).await
    }

    pub async fn delete_user(&self, id: Uuid) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM users WHERE id = $1").bind(id).execute(&self.pool).await?.rows_affected() > 0)
    }

    pub async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        let rows = sqlx::query("SELECT id, username, email, role, enabled, created_at FROM users ORDER BY username").fetch_all(&self.pool).await?;
        rows.into_iter().map(user_from_row).collect()
    }

    pub async fn insert_session(&self, id: Uuid, user_id: Uuid, token_hash: &str, csrf_token_hash: &str, expires_at: chrono::DateTime<chrono::Utc>) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO sessions (id, user_id, token_hash, csrf_token_hash, expires_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(id).bind(user_id).bind(token_hash).bind(csrf_token_hash).bind(expires_at).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn session_user(&self, token_hash: &str) -> anyhow::Result<Option<SessionUser>> {
        sqlx::query("SELECT s.id AS session_id, s.csrf_token_hash, u.id, u.username, u.email, u.role, u.enabled, u.created_at FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_hash = $1 AND s.expires_at > NOW() AND u.enabled")
            .bind(token_hash).fetch_optional(&self.pool).await?.map(|row| Ok(SessionUser { session_id: row.try_get("session_id")?, csrf_token_hash: row.try_get("csrf_token_hash")?, user: user_from_row(row)? })).transpose()
    }

    pub async fn delete_session(&self, token_hash: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(token_hash).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn update_session_seen(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE sessions SET last_seen_at = NOW() WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_cameras(&self) -> anyhow::Result<Vec<Camera>> {
        let rows = sqlx::query(
            "SELECT c.id, c.name, c.rtsp_url, c.enabled, c.status, (SELECT model_id FROM camera_models cm WHERE cm.camera_id = c.id) AS active_model_id, c.last_connected_at, c.last_snapshot_at, c.last_error, c.created_at, c.updated_at FROM cameras c ORDER BY c.name",
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
            "SELECT c.id, c.name, c.rtsp_url, c.enabled, c.status, (SELECT model_id FROM camera_models cm WHERE cm.camera_id = c.id) AS active_model_id, c.last_connected_at, c.last_snapshot_at, c.last_error, c.created_at, c.updated_at FROM cameras c WHERE c.id = $1",
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
        let rows = sqlx::query("SELECT id, name, version, model_type, path, input_width, input_height, class_list, enabled, default_model, created_at FROM models ORDER BY name")
            .fetch_all(&self.pool).await?;
        rows.into_iter().map(model_from_row).collect()
    }

    pub async fn get_model(&self, id: Uuid) -> anyhow::Result<Option<Model>> {
        let row = sqlx::query("SELECT id, name, version, model_type, path, input_width, input_height, class_list, enabled, default_model, created_at FROM models WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(model_from_row).transpose()
    }

    pub async fn create_model(&self, input: CreateModel) -> anyhow::Result<Model> {
        let id = Uuid::new_v4();
        if input.default_model { sqlx::query("UPDATE models SET default_model=FALSE").execute(&self.pool).await?; }
        sqlx::query("INSERT INTO models (id,name,version,model_type,path,input_width,input_height,class_list,enabled,default_model) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(id).bind(input.name).bind(input.version).bind(input.model_type).bind(input.path).bind(input.input_width).bind(input.input_height).bind(serde_json::to_value(input.class_list)?).bind(input.enabled).bind(input.default_model).execute(&self.pool).await?;
        self.get_model(id).await?.context("model was not returned after insert")
    }

    pub async fn delete_model(&self, id: Uuid) -> anyhow::Result<bool> { let result = sqlx::query("DELETE FROM models WHERE id=$1").bind(id).execute(&self.pool).await?; Ok(result.rows_affected() == 1) }

    pub async fn activate_model(&self, id: Uuid) -> anyhow::Result<bool> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE models SET default_model=FALSE").execute(&mut *transaction).await?;
        let result = sqlx::query("UPDATE models SET default_model=TRUE WHERE id=$1 AND enabled=TRUE").bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn assign_camera_model(&self, camera_id: Uuid, model_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("INSERT INTO camera_models (camera_id,model_id) VALUES ($1,$2) ON CONFLICT (camera_id) DO UPDATE SET model_id=EXCLUDED.model_id, assigned_at=NOW()")
            .bind(camera_id).bind(model_id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_model_assignments(&self, camera_id: Option<Uuid>) -> anyhow::Result<Vec<ModelAssignment>> {
        let rows = match camera_id {
            Some(id) => sqlx::query("SELECT id,camera_id,model_id,priority,confidence_threshold,fps_limit,enabled FROM model_assignments WHERE camera_id=$1 ORDER BY priority DESC").bind(id).fetch_all(&self.pool).await?,
            None => sqlx::query("SELECT id,camera_id,model_id,priority,confidence_threshold,fps_limit,enabled FROM model_assignments ORDER BY camera_id,priority DESC").fetch_all(&self.pool).await?,
        };
        rows.into_iter().map(model_assignment_from_row).collect()
    }

    pub async fn create_model_assignment(&self, input: CreateModelAssignment) -> anyhow::Result<ModelAssignment> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO model_assignments (id,camera_id,model_id,priority,confidence_threshold,fps_limit,enabled) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (camera_id,model_id) DO UPDATE SET priority=EXCLUDED.priority,confidence_threshold=EXCLUDED.confidence_threshold,fps_limit=EXCLUDED.fps_limit,enabled=EXCLUDED.enabled")
            .bind(id).bind(input.camera_id).bind(input.model_id).bind(input.priority).bind(input.confidence_threshold.clamp(0.0,1.0)).bind(input.fps_limit).bind(input.enabled).execute(&self.pool).await?;
        self.list_model_assignments(Some(input.camera_id)).await?.into_iter().find(|item| item.model_id == input.model_id).context("assignment was not returned after insert")
    }

    pub async fn list_fusion_results(&self, camera_id: Option<Uuid>, limit: i64) -> anyhow::Result<Vec<FusionResult>> {
        let rows = match camera_id {
            Some(id) => sqlx::query("SELECT id,camera_id,detection_id,source_model_ids,fused_confidence,created_at FROM fusion_results WHERE camera_id=$1 ORDER BY created_at DESC LIMIT $2").bind(id).bind(limit.clamp(1,500)).fetch_all(&self.pool).await?,
            None => sqlx::query("SELECT id,camera_id,detection_id,source_model_ids,fused_confidence,created_at FROM fusion_results ORDER BY created_at DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?,
        };
        rows.into_iter().map(fusion_result_from_row).collect()
    }

    pub async fn insert_fusion_result(&self, result: &FusionResult) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO fusion_results (id,camera_id,detection_id,source_model_ids,fused_confidence,created_at) VALUES ($1,$2,$3,$4,$5,$6)").bind(result.id).bind(result.camera_id).bind(result.detection_id).bind(serde_json::to_value(&result.source_model_ids)?).bind(result.fused_confidence).bind(result.created_at).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn active_model_for_camera(&self, camera_id: Uuid) -> anyhow::Result<Option<Uuid>> {
        let row = sqlx::query("SELECT COALESCE((SELECT model_id FROM camera_models WHERE camera_id=$1),(SELECT id FROM models WHERE default_model=TRUE AND enabled=TRUE LIMIT 1)) AS model_id").bind(camera_id).fetch_one(&self.pool).await?;
        row.try_get("model_id").map_err(Into::into)
    }

    pub async fn list_benchmarks(&self, model_id: Option<Uuid>) -> anyhow::Result<Vec<BenchmarkResult>> {
        let rows = match model_id { Some(id) => sqlx::query("SELECT id,model_id,fps,average_inference_time_ms,gpu_memory_usage_mb,cpu_usage_percent,test_timestamp FROM benchmark_results WHERE model_id=$1 ORDER BY test_timestamp DESC").bind(id).fetch_all(&self.pool).await?, None => sqlx::query("SELECT id,model_id,fps,average_inference_time_ms,gpu_memory_usage_mb,cpu_usage_percent,test_timestamp FROM benchmark_results ORDER BY test_timestamp DESC").fetch_all(&self.pool).await? };
        rows.into_iter().map(benchmark_from_row).collect()
    }

    pub async fn insert_benchmark(&self, benchmark: &BenchmarkResult) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO benchmark_results (id,model_id,fps,average_inference_time_ms,gpu_memory_usage_mb,cpu_usage_percent,test_timestamp) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(benchmark.id).bind(benchmark.model_id).bind(benchmark.fps).bind(benchmark.average_inference_time_ms).bind(benchmark.gpu_memory_usage_mb.map(|value| value as i64)).bind(benchmark.cpu_usage_percent).bind(benchmark.test_timestamp).execute(&self.pool).await?;
        Ok(())
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

    pub async fn list_zones(&self, camera_id: Option<Uuid>) -> anyhow::Result<Vec<Zone>> {
        let rows = match camera_id {
            Some(camera_id) => sqlx::query("SELECT id, camera_id, name, polygon_coordinates, colour, enabled, created_at FROM zones WHERE camera_id = $1 ORDER BY name").bind(camera_id).fetch_all(&self.pool).await?,
            None => sqlx::query("SELECT id, camera_id, name, polygon_coordinates, colour, enabled, created_at FROM zones ORDER BY name").fetch_all(&self.pool).await?,
        };
        rows.into_iter().map(zone_from_row).collect()
    }

    pub async fn get_zone(&self, id: Uuid) -> anyhow::Result<Option<Zone>> {
        let row = sqlx::query("SELECT id, camera_id, name, polygon_coordinates, colour, enabled, created_at FROM zones WHERE id = $1").bind(id).fetch_optional(&self.pool).await?;
        row.map(zone_from_row).transpose()
    }

    pub async fn create_zone(&self, input: CreateZone) -> anyhow::Result<Zone> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO zones (id, camera_id, name, polygon_coordinates, colour, enabled) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(id).bind(input.camera_id).bind(input.name).bind(serde_json::to_value(input.polygon_coordinates)?).bind(input.colour).bind(input.enabled).execute(&self.pool).await?;
        self.get_zone(id).await?.context("zone was not returned after insert")
    }

    pub async fn update_zone(&self, id: Uuid, input: UpdateZone) -> anyhow::Result<Option<Zone>> {
        let polygon = input.polygon_coordinates.map(serde_json::to_value).transpose()?;
        let result = sqlx::query("UPDATE zones SET name = COALESCE($2,name), polygon_coordinates = COALESCE($3,polygon_coordinates), colour = COALESCE($4,colour), enabled = COALESCE($5,enabled) WHERE id = $1")
            .bind(id).bind(input.name).bind(polygon).bind(input.colour).bind(input.enabled).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Ok(None); }
        self.get_zone(id).await
    }

    pub async fn delete_zone(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM zones WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn list_zone_events(&self, limit: i64) -> anyhow::Result<Vec<ZoneEvent>> {
        let rows = sqlx::query("SELECT id, zone_id, camera_id, track_id, event_type, occurred_at, duration_ms FROM zone_events ORDER BY occurred_at DESC LIMIT $1").bind(limit.clamp(1, 500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(zone_event_from_row).collect()
    }

    pub async fn insert_zone_event(&self, event: &ZoneEvent) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO zone_events (id, zone_id, camera_id, track_id, event_type, occurred_at, duration_ms) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(event.id).bind(event.zone_id).bind(event.camera_id).bind(event.track_id).bind(zone_event_type_string(&event.event_type)).bind(event.occurred_at).bind(event.duration_ms).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_rules(&self) -> anyhow::Result<Vec<Rule>> {
        let rows = sqlx::query("SELECT id, name, enabled, description, cooldown_seconds, suppression_seconds, severity, created_at, updated_at FROM rules ORDER BY name").fetch_all(&self.pool).await?;
        let mut rules = Vec::with_capacity(rows.len());
        for row in rows { rules.push(self.rule_from_row(row).await?); }
        Ok(rules)
    }

    pub async fn get_rule(&self, id: Uuid) -> anyhow::Result<Option<Rule>> {
        let row = sqlx::query("SELECT id, name, enabled, description, cooldown_seconds, suppression_seconds, severity, created_at, updated_at FROM rules WHERE id = $1").bind(id).fetch_optional(&self.pool).await?;
        match row { Some(row) => Ok(Some(self.rule_from_row(row).await?)), None => Ok(None) }
    }

    pub async fn create_rule(&self, input: CreateRule) -> anyhow::Result<Rule> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO rules (id,name,enabled,description,cooldown_seconds,suppression_seconds,severity) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(id).bind(input.name).bind(input.enabled).bind(input.description).bind(input.cooldown_seconds.max(0)).bind(input.suppression_seconds.max(0)).bind(severity_string(&input.severity)).execute(&self.pool).await?;
        self.replace_rule_conditions(id, input.conditions).await?;
        self.replace_rule_actions(id, input.action_ids).await?;
        self.get_rule(id).await?.context("rule was not returned after insert")
    }

    pub async fn update_rule(&self, id: Uuid, input: UpdateRule) -> anyhow::Result<Option<Rule>> {
        let severity = input.severity.as_ref().map(severity_string);
        let result = sqlx::query("UPDATE rules SET name=COALESCE($2,name), enabled=COALESCE($3,enabled), description=COALESCE($4,description), cooldown_seconds=COALESCE($5,cooldown_seconds), suppression_seconds=COALESCE($6,suppression_seconds), severity=COALESCE($7,severity), updated_at=NOW() WHERE id=$1")
            .bind(id).bind(input.name).bind(input.enabled).bind(input.description).bind(input.cooldown_seconds.map(|value| value.max(0))).bind(input.suppression_seconds.map(|value| value.max(0))).bind(severity).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Ok(None); }
        if let Some(conditions) = input.conditions { self.replace_rule_conditions(id, conditions).await?; }
        if let Some(action_ids) = input.action_ids { self.replace_rule_actions(id, action_ids).await?; }
        self.get_rule(id).await
    }

    async fn replace_rule_actions(&self, rule_id: Uuid, action_ids: Vec<Uuid>) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM rule_actions WHERE rule_id=$1").bind(rule_id).execute(&self.pool).await?;
        for action_id in action_ids { sqlx::query("INSERT INTO rule_actions (rule_id,action_id) VALUES ($1,$2)").bind(rule_id).bind(action_id).execute(&self.pool).await?; }
        Ok(())
    }

    async fn replace_rule_conditions(&self, rule_id: Uuid, conditions: Vec<RuleConditionInput>) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM rule_conditions WHERE rule_id=$1").bind(rule_id).execute(&self.pool).await?;
        for condition in conditions {
            sqlx::query("INSERT INTO rule_conditions (id,rule_id,object_class,zone_id,observation_type,behaviour_type,identity_id,familiarity,confidence_threshold,minimum_duration_ms,minimum_priority,minimum_anomaly_score) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
                .bind(Uuid::new_v4()).bind(rule_id).bind(condition.object_class).bind(condition.zone_id).bind(condition.observation_type).bind(condition.behaviour_type).bind(condition.identity_id).bind(condition.familiarity).bind(condition.confidence_threshold).bind(condition.minimum_duration_ms).bind(condition.minimum_priority).bind(condition.minimum_anomaly_score).execute(&self.pool).await?;
        }
        Ok(())
    }

    async fn rule_from_row(&self, row: sqlx::postgres::PgRow) -> anyhow::Result<Rule> {
        let id: Uuid = row.try_get("id")?;
        let condition_rows = sqlx::query("SELECT id,rule_id,object_class,zone_id,observation_type,behaviour_type,identity_id,familiarity,confidence_threshold,minimum_duration_ms,minimum_priority,minimum_anomaly_score FROM rule_conditions WHERE rule_id=$1").bind(id).fetch_all(&self.pool).await?;
        let conditions = condition_rows.into_iter().map(condition_from_row).collect::<anyhow::Result<Vec<_>>>()?;
        let action_rows = sqlx::query("SELECT action_id FROM rule_actions WHERE rule_id=$1").bind(id).fetch_all(&self.pool).await?;
        let action_ids = action_rows.into_iter().map(|action| action.try_get("action_id")).collect::<Result<Vec<Uuid>, _>>()?;
        Ok(Rule { id, name: row.try_get("name")?, enabled: row.try_get("enabled")?, description: row.try_get("description")?, cooldown_seconds: row.try_get("cooldown_seconds")?, suppression_seconds: row.try_get("suppression_seconds")?, severity: severity_from_string(&row.try_get::<String,_>("severity")?), conditions, action_ids, created_at: row.try_get("created_at")?, updated_at: row.try_get("updated_at")? })
    }

    pub async fn delete_rule(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM rules WHERE id=$1").bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn list_events(&self, limit: i64) -> anyhow::Result<Vec<Event>> {
        let rows = sqlx::query("SELECT id,rule_id,camera_id,track_id,observation_id,event_type,summary,severity,created_at FROM events ORDER BY created_at DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(event_from_row).collect()
    }

    pub async fn get_event(&self, id: Uuid) -> anyhow::Result<Option<Event>> {
        let row = sqlx::query("SELECT id,rule_id,camera_id,track_id,observation_id,event_type,summary,severity,created_at FROM events WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(event_from_row).transpose()
    }

    pub async fn insert_event(&self, event: &Event) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO events (id,rule_id,camera_id,track_id,observation_id,event_type,summary,severity,kind,payload,occurred_at,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$6,'{}',$9,$9)")
            .bind(event.id).bind(event.rule_id).bind(event.camera_id).bind(event.track_id).bind(event.observation_id).bind(&event.event_type).bind(&event.summary).bind(severity_string(&event.severity)).bind(event.created_at).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_providers(&self) -> anyhow::Result<Vec<NotificationProvider>> {
        let rows = sqlx::query("SELECT id,type,enabled,configuration,created_at FROM notification_providers ORDER BY created_at DESC").fetch_all(&self.pool).await?;
        rows.into_iter().map(provider_from_row).collect()
    }

    pub async fn create_provider(&self, input: CreateNotificationProvider) -> anyhow::Result<NotificationProvider> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO notification_providers (id,type,enabled,configuration) VALUES ($1,$2,$3,$4)").bind(id).bind(input.provider_type).bind(input.enabled).bind(input.configuration).execute(&self.pool).await?;
        self.list_providers().await?.into_iter().find(|provider| provider.id == id).context("provider was not returned after insert")
    }

    pub async fn update_provider(&self, id: Uuid, input: UpdateNotificationProvider) -> anyhow::Result<Option<NotificationProvider>> {
        let result = sqlx::query("UPDATE notification_providers SET enabled=COALESCE($2,enabled),configuration=COALESCE($3,configuration) WHERE id=$1").bind(id).bind(input.enabled).bind(input.configuration).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Ok(None); }
        self.provider(id).await
    }

    pub async fn list_templates(&self) -> anyhow::Result<Vec<NotificationTemplate>> {
        let rows = sqlx::query("SELECT id,name,subject,body,html,created_at FROM notification_templates ORDER BY name").fetch_all(&self.pool).await?;
        rows.into_iter().map(template_from_row).collect()
    }

    pub async fn create_template(&self, input: CreateNotificationTemplate) -> anyhow::Result<NotificationTemplate> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO notification_templates (id,name,subject,body,html) VALUES ($1,$2,$3,$4,$5)").bind(id).bind(input.name).bind(input.subject).bind(input.body).bind(input.html).execute(&self.pool).await?;
        self.list_templates().await?.into_iter().find(|template| template.id == id).context("template was not returned after insert")
    }

    pub async fn list_actions(&self) -> anyhow::Result<Vec<Action>> {
        let rows = sqlx::query("SELECT id,name,action_type,provider_id,template_id,enabled,configuration,created_at,updated_at FROM actions ORDER BY name").fetch_all(&self.pool).await?;
        rows.into_iter().map(action_from_row).collect()
    }

    pub async fn get_action(&self, id: Uuid) -> anyhow::Result<Option<Action>> {
        let row = sqlx::query("SELECT id,name,action_type,provider_id,template_id,enabled,configuration,created_at,updated_at FROM actions WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(action_from_row).transpose()
    }

    pub async fn create_action(&self, input: CreateAction) -> anyhow::Result<Action> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO actions (id,name,action_type,provider_id,template_id,enabled,configuration) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(input.name).bind(input.action_type).bind(input.provider_id).bind(input.template_id).bind(input.enabled).bind(input.configuration).execute(&self.pool).await?;
        self.get_action(id).await?.context("action was not returned after insert")
    }

    pub async fn update_action(&self, id: Uuid, input: UpdateAction) -> anyhow::Result<Option<Action>> {
        let result = sqlx::query("UPDATE actions SET name=COALESCE($2,name),provider_id=COALESCE($3,provider_id),template_id=COALESCE($4,template_id),enabled=COALESCE($5,enabled),configuration=COALESCE($6,configuration),updated_at=NOW() WHERE id=$1").bind(id).bind(input.name).bind(input.provider_id).bind(input.template_id).bind(input.enabled).bind(input.configuration).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Ok(None); }
        self.get_action(id).await
    }

    pub async fn delete_action(&self, id: Uuid) -> anyhow::Result<bool> { Ok(sqlx::query("DELETE FROM actions WHERE id=$1").bind(id).execute(&self.pool).await?.rows_affected() == 1) }

    pub async fn list_executions(&self, limit: i64) -> anyhow::Result<Vec<ActionExecution>> {
        let rows = sqlx::query("SELECT id,action_id,event_id,status,execution_time_ms,error_message,created_at FROM action_executions ORDER BY created_at DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(execution_from_row).collect()
    }

    pub async fn insert_execution(&self, execution: &ActionExecution) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO action_executions (id,action_id,event_id,status,execution_time_ms,error_message,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(execution.id).bind(execution.action_id).bind(execution.event_id).bind(&execution.status).bind(execution.execution_time_ms).bind(&execution.error_message).bind(execution.created_at).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_notifications(&self, limit: i64) -> anyhow::Result<Vec<Notification>> {
        let rows = sqlx::query("SELECT id,event_id,title,body,read,created_at FROM notifications ORDER BY created_at DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(notification_from_row).collect()
    }

    pub async fn insert_notification(&self, event: &Event) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO notifications (event_id,title,body) VALUES ($1,$2,$3)").bind(event.id).bind(&event.event_type).bind(&event.summary).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn actions_for_rule(&self, rule_id: Uuid) -> anyhow::Result<Vec<Action>> {
        let rows = sqlx::query("SELECT a.id,a.name,a.action_type,a.provider_id,a.template_id,a.enabled,a.configuration,a.created_at,a.updated_at FROM actions a INNER JOIN rule_actions ra ON ra.action_id=a.id WHERE ra.rule_id=$1 ORDER BY a.name").bind(rule_id).fetch_all(&self.pool).await?;
        rows.into_iter().map(action_from_row).collect()
    }

    pub async fn provider(&self, id: Uuid) -> anyhow::Result<Option<NotificationProvider>> {
        let row = sqlx::query("SELECT id,type,enabled,configuration,created_at FROM notification_providers WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(provider_from_row).transpose()
    }

    pub async fn template(&self, id: Uuid) -> anyhow::Result<Option<NotificationTemplate>> {
        let row = sqlx::query("SELECT id,name,subject,body,html,created_at FROM notification_templates WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(template_from_row).transpose()
    }

    pub async fn list_recordings(&self, camera_id: Option<Uuid>, limit: i64) -> anyhow::Result<Vec<Recording>> {
        let rows = match camera_id {
            Some(camera_id) => sqlx::query("SELECT id,camera_id,start_time,end_time,file_path,file_size,mode FROM recordings WHERE camera_id=$1 ORDER BY start_time DESC LIMIT $2").bind(camera_id).bind(limit.clamp(1,500)).fetch_all(&self.pool).await?,
            None => sqlx::query("SELECT id,camera_id,start_time,end_time,file_path,file_size,mode FROM recordings ORDER BY start_time DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?,
        };
        rows.into_iter().map(recording_from_row).collect()
    }

    pub async fn get_recording(&self, id: Uuid) -> anyhow::Result<Option<Recording>> {
        let row = sqlx::query("SELECT id,camera_id,start_time,end_time,file_path,file_size,mode FROM recordings WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(recording_from_row).transpose()
    }

    pub async fn insert_recording(&self, recording: &Recording) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO recordings (id,camera_id,start_time,end_time,file_path,file_size,mode) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(recording.id).bind(recording.camera_id).bind(recording.start_time).bind(recording.end_time).bind(&recording.file_path).bind(recording.file_size).bind(&recording.mode).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_clips(&self, limit: i64) -> anyhow::Result<Vec<Clip>> {
        let rows = sqlx::query("SELECT id,event_id,recording_id,clip_start,clip_end,clip_path FROM clips ORDER BY clip_start DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(clip_from_row).collect()
    }

    pub async fn get_clip(&self, id: Uuid) -> anyhow::Result<Option<Clip>> {
        let row = sqlx::query("SELECT id,event_id,recording_id,clip_start,clip_end,clip_path FROM clips WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(clip_from_row).transpose()
    }

    pub async fn insert_clip(&self, clip: &Clip) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO clips (id,event_id,recording_id,clip_start,clip_end,clip_path) VALUES ($1,$2,$3,$4,$5,$6)").bind(clip.id).bind(clip.event_id).bind(clip.recording_id).bind(clip.clip_start).bind(clip.clip_end).bind(&clip.clip_path).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_snapshots(&self, camera_id: Option<Uuid>, limit: i64) -> anyhow::Result<Vec<Snapshot>> {
        let rows = match camera_id {
            Some(camera_id) => sqlx::query("SELECT id,event_id,camera_id,image_path,timestamp FROM snapshots WHERE camera_id=$1 ORDER BY timestamp DESC LIMIT $2").bind(camera_id).bind(limit.clamp(1,500)).fetch_all(&self.pool).await?,
            None => sqlx::query("SELECT id,event_id,camera_id,image_path,timestamp FROM snapshots ORDER BY timestamp DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?,
        };
        rows.into_iter().map(snapshot_from_row).collect()
    }

    pub async fn get_snapshot(&self, id: Uuid) -> anyhow::Result<Option<Snapshot>> {
        let row = sqlx::query("SELECT id,event_id,camera_id,image_path,timestamp FROM snapshots WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(snapshot_from_row).transpose()
    }

    pub async fn insert_snapshot(&self, snapshot: &Snapshot) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO snapshots (id,event_id,camera_id,image_path,timestamp) VALUES ($1,$2,$3,$4,$5)").bind(snapshot.id).bind(snapshot.event_id).bind(snapshot.camera_id).bind(&snapshot.image_path).bind(snapshot.timestamp).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn search_detections(&self, filters: &SearchFilters) -> anyhow::Result<Vec<SearchResult>> {
        let rows = sqlx::query("SELECT d.id,d.camera_id,d.object_class,d.confidence,d.model_id,d.observed_at FROM detections d WHERE ($1::text IS NULL OR d.object_class=$1 OR d.object_class ILIKE '%' || $1 || '%') AND ($2::uuid IS NULL OR d.camera_id=$2) AND ($3::uuid IS NULL OR d.model_id=$3) AND ($4::real IS NULL OR d.confidence >= $4) AND ($5::timestamptz IS NULL OR d.observed_at >= $5) AND ($6::timestamptz IS NULL OR d.observed_at <= $6) ORDER BY d.observed_at DESC LIMIT $7")
            .bind(&filters.object_class).bind(filters.camera_id).bind(filters.model_id).bind(filters.confidence_min).bind(filters.from).bind(filters.to).bind(filters.limit()).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(SearchResult { entity_type: "detection".into(), id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, object_class: row.try_get("object_class")?, summary: format!("{} detected", row.try_get::<String,_>("object_class")?), occurred_at: row.try_get("observed_at")?, confidence: row.try_get("confidence")?, zone_id: None, model_id: row.try_get("model_id")?, recording_id: None, clip_id: None, snapshot_id: None })).collect()
    }

    pub async fn search_events(&self, filters: &SearchFilters) -> anyhow::Result<Vec<SearchResult>> {
        let rows = sqlx::query("SELECT e.id,e.camera_id,e.event_type,e.summary,e.created_at,e.severity,(SELECT id FROM clips WHERE event_id=e.id ORDER BY created_at DESC LIMIT 1) AS clip_id,(SELECT id FROM snapshots WHERE event_id=e.id ORDER BY created_at DESC LIMIT 1) AS snapshot_id FROM events e WHERE ($1::text IS NULL OR e.summary ILIKE '%' || $1 || '%' OR e.event_type ILIKE '%' || $1 || '%') AND ($2::uuid IS NULL OR e.camera_id=$2) AND ($3::text IS NULL OR e.event_type=$3) AND ($4::timestamptz IS NULL OR e.created_at >= $4) AND ($5::timestamptz IS NULL OR e.created_at <= $5) ORDER BY e.created_at DESC LIMIT $6")
            .bind(&filters.q).bind(filters.camera_id).bind(&filters.event_type).bind(filters.from).bind(filters.to).bind(filters.limit()).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(SearchResult { entity_type: "event".into(), id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, object_class: None, summary: row.try_get("summary")?, occurred_at: row.try_get("created_at")?, confidence: None, zone_id: None, model_id: None, recording_id: None, clip_id: row.try_get("clip_id")?, snapshot_id: row.try_get("snapshot_id")? })).collect()
    }

    pub async fn search_observations(&self, filters: &SearchFilters) -> anyhow::Result<Vec<SearchResult>> {
        let rows = sqlx::query("SELECT o.id,o.camera_id,o.observation_type,o.summary,o.created_at,t.object_class FROM observations o JOIN tracks t ON t.id=o.track_id WHERE ($1::text IS NULL OR o.summary ILIKE '%' || $1 || '%' OR o.observation_type ILIKE '%' || $1 || '%' OR t.object_class ILIKE '%' || $1 || '%') AND ($2::uuid IS NULL OR o.camera_id=$2) AND ($3::text IS NULL OR t.object_class=$3) AND ($4::text IS NULL OR o.observation_type=$4) AND ($5::timestamptz IS NULL OR o.created_at >= $5) AND ($6::timestamptz IS NULL OR o.created_at <= $6) ORDER BY o.created_at DESC LIMIT $7")
            .bind(&filters.q).bind(filters.camera_id).bind(&filters.object_class).bind(&filters.observation_type).bind(filters.from).bind(filters.to).bind(filters.limit()).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(SearchResult { entity_type: "observation".into(), id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, object_class: row.try_get("object_class")?, summary: row.try_get("summary")?, occurred_at: row.try_get("created_at")?, confidence: None, zone_id: None, model_id: None, recording_id: None, clip_id: None, snapshot_id: None })).collect()
    }

    pub async fn search_tracks(&self, filters: &SearchFilters) -> anyhow::Result<Vec<SearchResult>> {
        let rows = sqlx::query("SELECT id,camera_id,object_class,first_seen,last_seen,duration_ms FROM tracks WHERE ($1::uuid IS NULL OR camera_id=$1) AND ($2::text IS NULL OR object_class=$2) AND ($3::timestamptz IS NULL OR first_seen >= $3) AND ($4::timestamptz IS NULL OR last_seen <= $4) ORDER BY last_seen DESC LIMIT $5")
            .bind(filters.camera_id).bind(&filters.object_class).bind(filters.from).bind(filters.to).bind(filters.limit()).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(SearchResult { entity_type: "track".into(), id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, object_class: row.try_get("object_class")?, summary: format!("{} track · {} seconds", row.try_get::<String,_>("object_class")?, row.try_get::<i64,_>("duration_ms")? / 1000), occurred_at: row.try_get("last_seen")?, confidence: None, zone_id: None, model_id: None, recording_id: None, clip_id: None, snapshot_id: None })).collect()
    }

    pub async fn search_recordings(&self, filters: &SearchFilters) -> anyhow::Result<Vec<SearchResult>> {
        let rows = sqlx::query("SELECT id,camera_id,start_time,file_path,mode FROM recordings WHERE ($1::uuid IS NULL OR camera_id=$1) AND ($2::timestamptz IS NULL OR start_time >= $2) AND ($3::timestamptz IS NULL OR start_time <= $3) ORDER BY start_time DESC LIMIT $4")
            .bind(filters.camera_id).bind(filters.from).bind(filters.to).bind(filters.limit()).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(SearchResult { entity_type: "recording".into(), id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, object_class: None, summary: format!("{} recording", row.try_get::<String,_>("mode")?), occurred_at: row.try_get("start_time")?, confidence: None, zone_id: None, model_id: None, recording_id: row.try_get("id")?, clip_id: None, snapshot_id: None })).collect()
    }

    pub async fn analytics(&self, from: Option<chrono::DateTime<chrono::Utc>>, to: Option<chrono::DateTime<chrono::Utc>>) -> anyhow::Result<AnalyticsSummary> {
        let detections: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM detections WHERE ($1::timestamptz IS NULL OR observed_at >= $1) AND ($2::timestamptz IS NULL OR observed_at <= $2)").bind(from).bind(to).fetch_one(&self.pool).await?;
        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE ($1::timestamptz IS NULL OR created_at >= $1) AND ($2::timestamptz IS NULL OR created_at <= $2)").bind(from).bind(to).fetch_one(&self.pool).await?;
        let observations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE ($1::timestamptz IS NULL OR created_at >= $1) AND ($2::timestamptz IS NULL OR created_at <= $2)").bind(from).bind(to).fetch_one(&self.pool).await?;
        let behaviours: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM behaviours WHERE ($1::timestamptz IS NULL OR end_time >= $1) AND ($2::timestamptz IS NULL OR end_time <= $2)").bind(from).bind(to).fetch_one(&self.pool).await?;
        let object_rows = sqlx::query("SELECT object_class,COUNT(*) AS count FROM detections WHERE ($1::timestamptz IS NULL OR observed_at >= $1) AND ($2::timestamptz IS NULL OR observed_at <= $2) GROUP BY object_class ORDER BY count DESC LIMIT 10").bind(from).bind(to).fetch_all(&self.pool).await?;
        let camera_rows = sqlx::query("SELECT c.name,COUNT(*) AS count FROM detections d JOIN cameras c ON c.id=d.camera_id WHERE ($1::timestamptz IS NULL OR d.observed_at >= $1) AND ($2::timestamptz IS NULL OR d.observed_at <= $2) GROUP BY c.name ORDER BY count DESC LIMIT 10").bind(from).bind(to).fetch_all(&self.pool).await?;
        let daily_rows = sqlx::query("SELECT TO_CHAR(DATE_TRUNC('day',observed_at),'YYYY-MM-DD') AS label,COUNT(*) AS count FROM detections WHERE ($1::timestamptz IS NULL OR observed_at >= $1) AND ($2::timestamptz IS NULL OR observed_at <= $2) GROUP BY 1 ORDER BY 1").bind(from).bind(to).fetch_all(&self.pool).await?;
        let zone_rows = sqlx::query("SELECT z.name,COUNT(*) AS count FROM zone_events ze JOIN zones z ON z.id=ze.zone_id WHERE ($1::timestamptz IS NULL OR ze.occurred_at >= $1) AND ($2::timestamptz IS NULL OR ze.occurred_at <= $2) GROUP BY z.name ORDER BY count DESC LIMIT 10").bind(from).bind(to).fetch_all(&self.pool).await?;
        let model_rows = sqlx::query("SELECT COALESCE(m.name,'unknown') AS name,COUNT(*) AS count FROM detections d LEFT JOIN models m ON m.id=d.model_id WHERE ($1::timestamptz IS NULL OR d.observed_at >= $1) AND ($2::timestamptz IS NULL OR d.observed_at <= $2) GROUP BY 1 ORDER BY count DESC LIMIT 10").bind(from).bind(to).fetch_all(&self.pool).await?;
        Ok(AnalyticsSummary { from, to, detections, events, observations, behaviours, top_objects: metric_rows(object_rows, "object_class")?, camera_activity: metric_rows(camera_rows, "name")?, zone_activity: metric_rows(zone_rows, "name")?, model_activity: metric_rows(model_rows, "name")?, detections_per_day: metric_rows(daily_rows, "label")? })
    }

    pub async fn list_analytics_snapshots(&self, limit: i64) -> anyhow::Result<Vec<AnalyticsSnapshot>> {
        let rows = sqlx::query("SELECT id,period,period_start,summary,created_at FROM analytics_snapshots ORDER BY period_start DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(AnalyticsSnapshot { id: row.try_get("id")?, period: row.try_get("period")?, period_start: row.try_get("period_start")?, summary: row.try_get("summary")?, created_at: row.try_get("created_at")? })).collect()
    }

    pub async fn insert_behaviour(&self, behaviour: &Behaviour) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO behaviours (id,track_id,camera_id,object_class,behaviour_type,confidence,summary,start_time,end_time,recording_id,clip_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (track_id,behaviour_type,end_time) DO NOTHING")
            .bind(behaviour.id).bind(behaviour.track_id).bind(behaviour.camera_id).bind(&behaviour.object_class).bind(&behaviour.behaviour_type).bind(behaviour.confidence).bind(&behaviour.summary).bind(behaviour.start_time).bind(behaviour.end_time).bind(behaviour.recording_id).bind(behaviour.clip_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_behaviours(&self, limit: i64) -> anyhow::Result<Vec<Behaviour>> {
        let rows = sqlx::query("SELECT b.id,b.track_id,b.camera_id,b.object_class,b.behaviour_type,b.confidence,b.summary,b.start_time,b.end_time,b.recording_id,b.clip_id FROM behaviours b ORDER BY b.end_time DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(behaviour_from_row).collect()
    }

    pub async fn get_behaviour(&self, id: Uuid) -> anyhow::Result<Option<Behaviour>> {
        let row = sqlx::query("SELECT b.id,b.track_id,b.camera_id,b.object_class,b.behaviour_type,b.confidence,b.summary,b.start_time,b.end_time,b.recording_id,b.clip_id FROM behaviours b WHERE b.id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(behaviour_from_row).transpose()
    }

    pub async fn search_behaviours(&self, filters: &SearchFilters) -> anyhow::Result<Vec<SearchResult>> {
        let rows = sqlx::query("SELECT id,camera_id,object_class,behaviour_type,summary,end_time,confidence,recording_id,clip_id FROM behaviours WHERE ($1::text IS NULL OR behaviour_type=$1 OR summary ILIKE '%' || $1 || '%' OR object_class ILIKE '%' || $1 || '%') AND ($2::uuid IS NULL OR camera_id=$2) AND ($3::text IS NULL OR object_class=$3) AND ($4::timestamptz IS NULL OR end_time >= $4) AND ($5::timestamptz IS NULL OR end_time <= $5) ORDER BY end_time DESC LIMIT $6").bind(&filters.behaviour_type).bind(filters.camera_id).bind(&filters.object_class).bind(filters.from).bind(filters.to).bind(filters.limit()).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(SearchResult { entity_type: "behaviour".into(), id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, object_class: row.try_get("object_class")?, summary: row.try_get("summary")?, occurred_at: row.try_get("end_time")?, confidence: row.try_get("confidence")?, zone_id: None, model_id: None, recording_id: row.try_get("recording_id")?, clip_id: row.try_get("clip_id")?, snapshot_id: None })).collect()
    }

    pub async fn list_identities(&self, limit: i64) -> anyhow::Result<Vec<Identity>> {
        let rows = sqlx::query("SELECT id,object_class,display_name,familiarity_score,familiarity,first_seen,last_seen,sightings FROM identities ORDER BY last_seen DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(identity_from_row).collect()
    }

    pub async fn get_identity(&self, id: Uuid) -> anyhow::Result<Option<Identity>> {
        let row = sqlx::query("SELECT id,object_class,display_name,familiarity_score,familiarity,first_seen,last_seen,sightings FROM identities WHERE id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(identity_from_row).transpose()
    }

    pub async fn update_identity(&self, id: Uuid, input: UpdateIdentity) -> anyhow::Result<Option<Identity>> {
        let result = sqlx::query("UPDATE identities SET display_name=$2 WHERE id=$1").bind(id).bind(input.display_name).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Ok(None); }
        self.get_identity(id).await
    }

    pub async fn identity_history(&self, id: Uuid, limit: i64) -> anyhow::Result<Vec<IdentityObservation>> {
        let rows = sqlx::query("SELECT id,identity_id,track_id,camera_id,similarity,observed_at FROM identity_observations WHERE identity_id=$1 ORDER BY observed_at DESC LIMIT $2").bind(id).bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(identity_observation_from_row).collect()
    }

    pub async fn identity_statistics(&self, id: Uuid) -> anyhow::Result<Option<IdentityStatistics>> {
        let row = sqlx::query("SELECT identity_id,average_duration_ms,active_days,top_zone_id FROM identity_statistics WHERE identity_id=$1").bind(id).fetch_optional(&self.pool).await?;
        row.map(identity_statistics_from_row).transpose()
    }

    pub async fn assign_identity(&self, track: &Track) -> anyhow::Result<Identity> {
        let candidate_rows = sqlx::query("SELECT id,object_class,display_name,familiarity_score,familiarity,first_seen,last_seen,sightings,signature FROM identities WHERE object_class=$1 ORDER BY last_seen DESC LIMIT 50").bind(&track.object_class).fetch_all(&self.pool).await?;
        let current = signature(track);
        let mut best: Option<(Identity, f32)> = None;
        for row in candidate_rows {
            let identity = identity_from_row(row.clone())?;
            let stored: Value = row.try_get("signature")?;
            let stored: ObjectSignature = serde_json::from_value(stored)?;
            let value = similarity(&current, &stored);
            if value >= 0.72 && best.as_ref().map(|(_, score)| value > *score).unwrap_or(true) { best = Some((identity, value)); }
        }
        let (identity, match_score) = best.unwrap_or_else(|| (new_identity(track, track.last_seen), 0.0));
        let sig = serde_json::to_value(current)?;
        let identity = if match_score == 0.0 {
            sqlx::query("INSERT INTO identities (id,object_class,signature,familiarity_score,familiarity,first_seen,last_seen,sightings) VALUES ($1,$2,$3,$4,$5,$6,$6,1)").bind(identity.id).bind(&identity.object_class).bind(sig).bind(identity.familiarity_score).bind(&identity.familiarity).bind(track.last_seen).execute(&self.pool).await?;
            identity
        } else {
            let sightings = identity.sightings + 1;
            sqlx::query("UPDATE identities SET signature=$2,last_seen=$3,sightings=$4,familiarity_score=$5,familiarity=$6 WHERE id=$1").bind(identity.id).bind(sig).bind(track.last_seen).bind(sightings).bind(score(sightings)).bind(familiarity(sightings)).execute(&self.pool).await?;
            Identity { sightings, last_seen: track.last_seen, familiarity_score: score(sightings), familiarity: familiarity(sightings).into(), ..identity }
        };
        sqlx::query("INSERT INTO identity_observations (identity_id,track_id,camera_id,similarity,observed_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (identity_id,track_id) DO NOTHING").bind(identity.id).bind(track.id).bind(track.camera_id).bind(match_score.max(1.0)).bind(track.last_seen).execute(&self.pool).await?;
        sqlx::query("INSERT INTO identity_statistics (identity_id,average_duration_ms,active_days) VALUES ($1,$2,1) ON CONFLICT (identity_id) DO UPDATE SET average_duration_ms=((identity_statistics.average_duration_ms + EXCLUDED.average_duration_ms)/2),active_days=identity_statistics.active_days+1").bind(identity.id).bind(track.duration_ms).execute(&self.pool).await?;
        Ok(identity)
    }

    pub async fn insert_adaptive_scores(&self, identity_id: Uuid, behaviour_id: Option<Uuid>, assessment: &AdaptiveAssessment, event_id: Option<Uuid>) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO identity_scores (identity_id,familiarity_score,confidence) VALUES ($1,$2,$3)").bind(identity_id).bind(assessment.familiarity_score).bind(assessment.familiarity_score).execute(&self.pool).await?;
        if let Some(behaviour_id) = behaviour_id {
            sqlx::query("INSERT INTO behaviour_scores (behaviour_id,anomaly_score,behaviour_level,confidence) VALUES ($1,$2,$3,$4)").bind(behaviour_id).bind(assessment.anomaly_score).bind(assessment.behaviour_level).bind(assessment.priority_score).execute(&self.pool).await?;
        }
        sqlx::query("INSERT INTO anomaly_events (event_id,identity_id,behaviour_id,anomaly_score,priority_score,priority) VALUES ($1,$2,$3,$4,$5,$6)").bind(event_id).bind(identity_id).bind(behaviour_id).bind(assessment.anomaly_score).bind(assessment.priority_score).bind(assessment.priority.as_str()).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_anomalies(&self, limit: i64) -> anyhow::Result<Vec<objexel_common::AnomalyEvent>> {
        let rows = sqlx::query("SELECT id,event_id,identity_id,behaviour_id,anomaly_score,priority_score,priority,created_at FROM anomaly_events ORDER BY priority_score DESC,created_at DESC LIMIT $1").bind(limit.clamp(1,500)).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(objexel_common::AnomalyEvent { id: row.try_get("id")?, event_id: row.try_get("event_id")?, identity_id: row.try_get("identity_id")?, behaviour_id: row.try_get("behaviour_id")?, anomaly_score: row.try_get("anomaly_score")?, priority_score: row.try_get("priority_score")?, priority: row.try_get("priority")?, created_at: row.try_get("created_at")? })).collect()
    }

    pub async fn delete_camera(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM cameras WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }
}

fn user_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<User> {
    Ok(User { id: row.try_get("id")?, username: row.try_get("username")?, email: row.try_get("email")?, role: Role::try_from(row.try_get::<String, _>("role")?.as_str())?, enabled: row.try_get("enabled")?, created_at: row.try_get("created_at")? })
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
        active_model_id: row.try_get("active_model_id")?,
        created_at: row.try_get("created_at")?,
        last_connected_at: row.try_get("last_connected_at")?,
        last_snapshot_at: row.try_get("last_snapshot_at")?,
        last_error: row.try_get("last_error")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn model_assignment_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<ModelAssignment> { Ok(ModelAssignment { id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, model_id: row.try_get("model_id")?, priority: row.try_get("priority")?, confidence_threshold: row.try_get("confidence_threshold")?, fps_limit: row.try_get("fps_limit")?, enabled: row.try_get("enabled")? }) }
fn fusion_result_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<FusionResult> { Ok(FusionResult { id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, detection_id: row.try_get("detection_id")?, source_model_ids: serde_json::from_value(row.try_get("source_model_ids")?)?, fused_confidence: row.try_get("fused_confidence")?, created_at: row.try_get("created_at")? }) }

fn model_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Model> {
    Ok(Model { id: row.try_get("id")?, name: row.try_get("name")?, version: row.try_get("version")?, model_type: row.try_get("model_type")?, path: row.try_get("path")?, input_width: row.try_get("input_width")?, input_height: row.try_get("input_height")?, class_list: serde_json::from_value(row.try_get("class_list")?)?, enabled: row.try_get("enabled")?, default_model: row.try_get("default_model")?, created_at: row.try_get("created_at")? })
}

fn benchmark_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<BenchmarkResult> { Ok(BenchmarkResult { id: row.try_get("id")?, model_id: row.try_get("model_id")?, fps: row.try_get("fps")?, average_inference_time_ms: row.try_get("average_inference_time_ms")?, gpu_memory_usage_mb: row.try_get::<Option<i64>, _>("gpu_memory_usage_mb")?.map(|value| value as u64), cpu_usage_percent: row.try_get("cpu_usage_percent")?, test_timestamp: row.try_get("test_timestamp")? }) }

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

fn zone_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Zone> {
    Ok(Zone { id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, name: row.try_get("name")?, polygon_coordinates: serde_json::from_value(row.try_get("polygon_coordinates")?)?, colour: row.try_get("colour")?, enabled: row.try_get("enabled")?, created_at: row.try_get("created_at")? })
}

fn zone_event_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<ZoneEvent> {
    let event_type = match row.try_get::<String, _>("event_type")?.as_str() { "entered" => ZoneEventType::Entered, "exited" => ZoneEventType::Exited, _ => ZoneEventType::Occupied };
    Ok(ZoneEvent { id: row.try_get("id")?, zone_id: row.try_get("zone_id")?, camera_id: row.try_get("camera_id")?, track_id: row.try_get("track_id")?, event_type, occurred_at: row.try_get("occurred_at")?, duration_ms: row.try_get("duration_ms")? })
}

fn zone_event_type_string(event_type: &ZoneEventType) -> &'static str { match event_type { ZoneEventType::Entered => "entered", ZoneEventType::Exited => "exited", ZoneEventType::Occupied => "occupied" } }

fn condition_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<RuleCondition> { Ok(RuleCondition { id: row.try_get("id")?, rule_id: row.try_get("rule_id")?, object_class: row.try_get("object_class")?, zone_id: row.try_get("zone_id")?, observation_type: row.try_get("observation_type")?, behaviour_type: row.try_get("behaviour_type")?, confidence_threshold: row.try_get("confidence_threshold")?, minimum_duration_ms: row.try_get("minimum_duration_ms")? }) }
fn severity_string(severity: &EventSeverity) -> &'static str { match severity { EventSeverity::Info => "info", EventSeverity::Warning => "warning", EventSeverity::Critical => "critical" } }
fn severity_from_string(value: &str) -> EventSeverity { match value { "warning" => EventSeverity::Warning, "critical" => EventSeverity::Critical, _ => EventSeverity::Info } }
fn event_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Event> { Ok(Event { id: row.try_get("id")?, rule_id: row.try_get::<Option<Uuid>, _>("rule_id")?.unwrap_or_else(Uuid::nil), camera_id: row.try_get("camera_id")?, track_id: row.try_get("track_id")?, observation_id: row.try_get("observation_id")?, event_type: row.try_get("event_type")?, summary: row.try_get("summary")?, severity: severity_from_string(&row.try_get::<String,_>("severity")?), created_at: row.try_get("created_at")? }) }

fn recording_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Recording> { Ok(Recording { id: row.try_get("id")?, camera_id: row.try_get("camera_id")?, start_time: row.try_get("start_time")?, end_time: row.try_get("end_time")?, file_path: row.try_get("file_path")?, file_size: row.try_get("file_size")?, mode: row.try_get("mode")? }) }
fn clip_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Clip> { Ok(Clip { id: row.try_get("id")?, event_id: row.try_get("event_id")?, recording_id: row.try_get("recording_id")?, clip_start: row.try_get("clip_start")?, clip_end: row.try_get("clip_end")?, clip_path: row.try_get("clip_path")? }) }
fn snapshot_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Snapshot> { Ok(Snapshot { id: row.try_get("id")?, event_id: row.try_get("event_id")?, camera_id: row.try_get("camera_id")?, image_path: row.try_get("image_path")?, timestamp: row.try_get("timestamp")? }) }

fn provider_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<NotificationProvider> { Ok(NotificationProvider { id: row.try_get("id")?, provider_type: row.try_get("type")?, enabled: row.try_get("enabled")?, configuration: row.try_get("configuration")?, created_at: row.try_get("created_at")? }) }
fn template_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<NotificationTemplate> { Ok(NotificationTemplate { id: row.try_get("id")?, name: row.try_get("name")?, subject: row.try_get("subject")?, body: row.try_get("body")?, html: row.try_get("html")?, created_at: row.try_get("created_at")? }) }
fn action_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Action> { Ok(Action { id: row.try_get("id")?, name: row.try_get("name")?, action_type: row.try_get("action_type")?, provider_id: row.try_get("provider_id")?, template_id: row.try_get("template_id")?, enabled: row.try_get("enabled")?, configuration: row.try_get("configuration")?, created_at: row.try_get("created_at")?, updated_at: row.try_get("updated_at")? }) }
fn execution_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<ActionExecution> { Ok(ActionExecution { id: row.try_get("id")?, action_id: row.try_get("action_id")?, event_id: row.try_get("event_id")?, status: row.try_get("status")?, execution_time_ms: row.try_get("execution_time_ms")?, error_message: row.try_get("error_message")?, created_at: row.try_get("created_at")? }) }
fn notification_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Notification> { Ok(Notification { id: row.try_get("id")?, event_id: row.try_get("event_id")?, title: row.try_get("title")?, body: row.try_get("body")?, read: row.try_get("read")?, created_at: row.try_get("created_at")? }) }

fn behaviour_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Behaviour> {
    Ok(Behaviour { id: row.try_get("id")?, track_id: row.try_get("track_id")?, camera_id: row.try_get("camera_id")?, object_class: row.try_get("object_class")?, behaviour_type: row.try_get("behaviour_type")?, confidence: row.try_get("confidence")?, summary: row.try_get("summary")?, start_time: row.try_get("start_time")?, end_time: row.try_get("end_time")?, recording_id: row.try_get("recording_id")?, clip_id: row.try_get("clip_id")? })
}

fn metric_rows(rows: Vec<sqlx::postgres::PgRow>, label_column: &str) -> anyhow::Result<Vec<AnalyticsMetric>> { rows.into_iter().map(|row| Ok(AnalyticsMetric { label: row.try_get(label_column)?, count: row.try_get("count")? })).collect() }

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
