use anyhow::Context;
use objexel_common::{Action, ActionExecution, BenchmarkResult, Camera, CameraStatus, CreateAction, CreateCamera, CreateModel, CreateNotificationProvider, CreateNotificationTemplate, UpdateNotificationProvider, CreateRule, CreateZone, Detection, Event, EventSeverity, Model, Notification, NotificationProvider, NotificationTemplate, Observation, Rule, RuleCondition, RuleConditionInput, Track, UpdateAction, UpdateCamera, UpdateRule, UpdateZone, Zone, ZoneEvent, ZoneEventType};
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
            sqlx::query("INSERT INTO rule_conditions (id,rule_id,object_class,zone_id,observation_type,confidence_threshold,minimum_duration_ms) VALUES ($1,$2,$3,$4,$5,$6,$7)")
                .bind(Uuid::new_v4()).bind(rule_id).bind(condition.object_class).bind(condition.zone_id).bind(condition.observation_type).bind(condition.confidence_threshold).bind(condition.minimum_duration_ms).execute(&self.pool).await?;
        }
        Ok(())
    }

    async fn rule_from_row(&self, row: sqlx::postgres::PgRow) -> anyhow::Result<Rule> {
        let id: Uuid = row.try_get("id")?;
        let condition_rows = sqlx::query("SELECT id,rule_id,object_class,zone_id,observation_type,confidence_threshold,minimum_duration_ms FROM rule_conditions WHERE rule_id=$1").bind(id).fetch_all(&self.pool).await?;
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
        active_model_id: row.try_get("active_model_id")?,
        created_at: row.try_get("created_at")?,
        last_connected_at: row.try_get("last_connected_at")?,
        last_snapshot_at: row.try_get("last_snapshot_at")?,
        last_error: row.try_get("last_error")?,
        updated_at: row.try_get("updated_at")?,
    })
}

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

fn condition_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<RuleCondition> { Ok(RuleCondition { id: row.try_get("id")?, rule_id: row.try_get("rule_id")?, object_class: row.try_get("object_class")?, zone_id: row.try_get("zone_id")?, observation_type: row.try_get("observation_type")?, confidence_threshold: row.try_get("confidence_threshold")?, minimum_duration_ms: row.try_get("minimum_duration_ms")? }) }
fn severity_string(severity: &EventSeverity) -> &'static str { match severity { EventSeverity::Info => "info", EventSeverity::Warning => "warning", EventSeverity::Critical => "critical" } }
fn severity_from_string(value: &str) -> EventSeverity { match value { "warning" => EventSeverity::Warning, "critical" => EventSeverity::Critical, _ => EventSeverity::Info } }
fn event_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Event> { Ok(Event { id: row.try_get("id")?, rule_id: row.try_get::<Option<Uuid>, _>("rule_id")?.unwrap_or_else(Uuid::nil), camera_id: row.try_get("camera_id")?, track_id: row.try_get("track_id")?, observation_id: row.try_get("observation_id")?, event_type: row.try_get("event_type")?, summary: row.try_get("summary")?, severity: severity_from_string(&row.try_get::<String,_>("severity")?), created_at: row.try_get("created_at")? }) }

fn provider_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<NotificationProvider> { Ok(NotificationProvider { id: row.try_get("id")?, provider_type: row.try_get("type")?, enabled: row.try_get("enabled")?, configuration: row.try_get("configuration")?, created_at: row.try_get("created_at")? }) }
fn template_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<NotificationTemplate> { Ok(NotificationTemplate { id: row.try_get("id")?, name: row.try_get("name")?, subject: row.try_get("subject")?, body: row.try_get("body")?, html: row.try_get("html")?, created_at: row.try_get("created_at")? }) }
fn action_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Action> { Ok(Action { id: row.try_get("id")?, name: row.try_get("name")?, action_type: row.try_get("action_type")?, provider_id: row.try_get("provider_id")?, template_id: row.try_get("template_id")?, enabled: row.try_get("enabled")?, configuration: row.try_get("configuration")?, created_at: row.try_get("created_at")?, updated_at: row.try_get("updated_at")? }) }
fn execution_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<ActionExecution> { Ok(ActionExecution { id: row.try_get("id")?, action_id: row.try_get("action_id")?, event_id: row.try_get("event_id")?, status: row.try_get("status")?, execution_time_ms: row.try_get("execution_time_ms")?, error_message: row.try_get("error_message")?, created_at: row.try_get("created_at")? }) }
fn notification_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<Notification> { Ok(Notification { id: row.try_get("id")?, event_id: row.try_get("event_id")?, title: row.try_get("title")?, body: row.try_get("body")?, read: row.try_get("read")?, created_at: row.try_get("created_at")? }) }

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
