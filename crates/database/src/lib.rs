use anyhow::Context;
use objexel_common::{BenchmarkResult, Camera, CameraStatus, CreateCamera, CreateModel, CreateRule, CreateZone, Detection, Event, EventSeverity, Model, Observation, Rule, RuleCondition, RuleConditionInput, Track, UpdateCamera, UpdateRule, UpdateZone, Zone, ZoneEvent, ZoneEventType};
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
        self.get_rule(id).await?.context("rule was not returned after insert")
    }

    pub async fn update_rule(&self, id: Uuid, input: UpdateRule) -> anyhow::Result<Option<Rule>> {
        let severity = input.severity.as_ref().map(severity_string);
        let result = sqlx::query("UPDATE rules SET name=COALESCE($2,name), enabled=COALESCE($3,enabled), description=COALESCE($4,description), cooldown_seconds=COALESCE($5,cooldown_seconds), suppression_seconds=COALESCE($6,suppression_seconds), severity=COALESCE($7,severity), updated_at=NOW() WHERE id=$1")
            .bind(id).bind(input.name).bind(input.enabled).bind(input.description).bind(input.cooldown_seconds.map(|value| value.max(0))).bind(input.suppression_seconds.map(|value| value.max(0))).bind(severity).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Ok(None); }
        if let Some(conditions) = input.conditions { self.replace_rule_conditions(id, conditions).await?; }
        self.get_rule(id).await
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
        Ok(Rule { id, name: row.try_get("name")?, enabled: row.try_get("enabled")?, description: row.try_get("description")?, cooldown_seconds: row.try_get("cooldown_seconds")?, suppression_seconds: row.try_get("suppression_seconds")?, severity: severity_from_string(&row.try_get::<String,_>("severity")?), conditions, created_at: row.try_get("created_at")?, updated_at: row.try_get("updated_at")? })
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
