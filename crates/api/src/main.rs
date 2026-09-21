use async_trait::async_trait;
use objexel_api::{broadcast_pipeline_result, live_channel, router, validate_model_catalog_entry, AppState, CameraRuntime};
use objexel_camera::{CameraService, FrameIngestor, IngestorConfig};
use objexel_detector::FrameTensor;
use objexel_pipeline::ObservationPipeline;
use objexel_playback::PlaybackService;
use objexel_recorder::{Recorder, RecorderConfig};
use objexel_models::ModelRegistry;
use objexel_database::Database;
use std::{collections::HashMap, env, sync::Arc};
use tokio::{net::TcpListener, sync::{Mutex, oneshot}, task::JoinHandle};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

struct RecordingHandle {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

struct LiveCameraRuntime {
    ingestor: FrameIngestor,
    pipeline: Option<ObservationPipeline>,
    events: objexel_api::LiveEventSender,
    recorder: Recorder,
    database: Option<Database>,
    handles: Mutex<HashMap<Uuid, JoinHandle<()>>>,
    recording_handles: Mutex<HashMap<Uuid, RecordingHandle>>,
}

impl LiveCameraRuntime {
    async fn stop_recording(&self, camera_id: Uuid) {
        if let Some(handle) = self.recording_handles.lock().await.remove(&camera_id) {
            let _ = handle.stop.send(());
            let _ = handle.task.await;
            tracing::info!(camera_id = %camera_id, "continuous recording stopped");
        }
    }

    async fn start_recording(&self, camera: &objexel_common::Camera) {
        let Some(database) = &self.database else { return; };
        let recorder = self.recorder.clone();
        let database = database.clone();
        let camera_id = camera.id;
        let rtsp_url = camera.rtsp_url.clone();
        let (stop, mut stop_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut retry_delay = std::time::Duration::from_secs(2);
            loop {
                match recorder.start_continuous(camera_id, &rtsp_url).await {
                    Ok((recording, mut child)) => {
                        if let Err(error) = database.insert_recording(&recording).await {
                            tracing::warn!(camera_id = %camera_id, %error, "could not persist recording metadata");
                        }
                        tracing::info!(camera_id = %camera_id, "continuous recording started");
                        tokio::select! {
                            result = child.wait() => {
                                if let Err(error) = result { tracing::warn!(camera_id = %camera_id, %error, "continuous recording exited"); }
                                else { tracing::warn!(camera_id = %camera_id, "continuous recording exited unexpectedly"); }
                            }
                            _ = &mut stop_rx => {
                                if let Err(error) = child.kill().await { tracing::debug!(camera_id = %camera_id, %error, "recording process was already stopped"); }
                                let _ = child.wait().await;
                                return;
                            }
                        }
                        retry_delay = std::time::Duration::from_secs(2);
                    }
                    Err(error) => tracing::warn!(camera_id = %camera_id, %error, "continuous recording could not start"),
                }
                tokio::select! {
                    _ = tokio::time::sleep(retry_delay) => {},
                    _ = &mut stop_rx => return,
                }
                retry_delay = std::cmp::min(retry_delay.saturating_mul(2), std::time::Duration::from_secs(60));
            }
        });
        self.recording_handles.lock().await.insert(camera.id, RecordingHandle { stop, task });
    }
}

#[async_trait]
impl CameraRuntime for LiveCameraRuntime {
    async fn apply(&self, camera: &objexel_common::Camera) -> anyhow::Result<()> {
        self.remove(camera.id).await?;
        if !camera.enabled { return Ok(()); }
        self.start_recording(camera).await;
        let Some(pipeline) = &self.pipeline else { return Ok(()); };
        let pipeline = pipeline.clone();
        let events = self.events.clone();
        let camera_id = camera.id;
        let handle = self.ingestor.spawn(camera_id, &camera.rtsp_url, move |frame| {
            let pipeline = pipeline.clone();
            let events = events.clone();
            async move {
                let tensor = FrameTensor {
                    camera_id,
                    observed_at: chrono::Utc::now(),
                    shape: vec![1, 3, frame.height as usize, frame.width as usize],
                    data: frame.data.iter().map(|&pixel| pixel as f32 / 255.0).collect(),
                };
                match pipeline.process_frame(tensor).await {
                    Ok(result) => broadcast_pipeline_result(&events, &result),
                    Err(error) => tracing::warn!(camera_id = %camera_id, %error, "live frame processing failed"),
                }
            }
        });
        self.handles.lock().await.insert(camera.id, handle);
        tracing::info!(camera_id = %camera.id, "live camera runtime started");
        Ok(())
    }

    async fn remove(&self, camera_id: Uuid) -> anyhow::Result<()> {
        self.stop_recording(camera_id).await;
        if let Some(handle) = self.handles.lock().await.remove(&camera_id) {
            handle.abort();
            tracing::info!(camera_id = %camera_id, "live camera runtime stopped");
        }
        Ok(())
    }
}

/// Fetch and parse a hosted model catalog manifest (used for the optional
/// boot-time refresh when `OBJEXEL_MODEL_CATALOG_URL` is set).
async fn fetch_remote_catalog(url: &str) -> anyhow::Result<Vec<objexel_common::ModelCatalogEntry>> {
    let response = reqwest::Client::new().get(url).send().await?.error_for_status()?;
    let body = response.text().await?;
    Ok(serde_json::from_str::<Vec<objexel_common::ModelCatalogEntry>>(&body)?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).init();

    let database = match env::var("DATABASE_URL") {
        Ok(url) => {
            let database = loop {
                match Database::connect(&url).await {
                    Ok(database) => break database,
                    Err(error) => {
                        tracing::warn!(%error, "database connection failed; retrying");
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                }
            };
            if let Err(error) = database.migrate().await {
                tracing::error!(%error, "database migration failed");
                return Err(error);
            }
            // Seed the built-in catalog so the Models page is populated with one-click,
            // checksum-verified YOLO11/YOLO26 downloads out of the box. Entries are keyed
            // by a stable id, so this upsert is idempotent and any operator edits made
            // through the API are refreshed to the shipped definition on restart. Set
            // OBJEXEL_DISABLE_DEFAULT_CATALOG=1 to skip seeding.
            if !matches!(env::var("OBJEXEL_DISABLE_DEFAULT_CATALOG").as_deref(), Ok("1") | Ok("true")) {
                for entry in objexel_api::default_model_catalog() {
                    if let Err(error) = database.upsert_model_catalog(&entry).await {
                        tracing::warn!(catalog_id = %entry.id, %error, "could not seed default model catalog entry");
                    }
                }
            }
            if let Ok(path) = env::var("OBJEXEL_MODEL_CATALOG") {
                match tokio::fs::read_to_string(&path).await {
                    Ok(contents) => match serde_json::from_str::<Vec<objexel_common::ModelCatalogEntry>>(&contents) {
                        Ok(entries) => {
                            for entry in entries {
                                if let Err(error) = validate_model_catalog_entry(&entry) {
                                    tracing::warn!(catalog_id = %entry.id, %error, "invalid model catalog entry");
                                    continue;
                                }
                                if let Err(error) = database.upsert_model_catalog(&entry).await {
                                    tracing::warn!(catalog_id = %entry.id, %error, "could not import model catalog entry");
                                }
                            }
                        }
                        Err(error) => tracing::warn!(%error, path = %path, "could not parse model catalog file"),
                    },
                    Err(error) => tracing::warn!(%error, path = %path, "could not read model catalog file"),
                }
            }
            // When a hosted manifest URL is configured, pull the latest catalog at boot so
            // deployments track new model versions without a rebuild. Failures are
            // non-fatal: the embedded default catalog remains available.
            if let Ok(url) = env::var("OBJEXEL_MODEL_CATALOG_URL") {
                match fetch_remote_catalog(&url).await {
                    Ok(entries) => {
                        for entry in entries {
                            if let Err(error) = validate_model_catalog_entry(&entry) {
                                tracing::warn!(catalog_id = %entry.id, %error, "invalid remote model catalog entry");
                                continue;
                            }
                            if let Err(error) = database.upsert_model_catalog(&entry).await {
                                tracing::warn!(catalog_id = %entry.id, %error, "could not import remote model catalog entry");
                            }
                        }
                    }
                    Err(error) => tracing::warn!(%error, %url, "could not refresh model catalog from URL"),
                }
            }
            Some(database)
        }
        Err(_) => {
            tracing::warn!("DATABASE_URL is not configured; starting in health-only mode");
            None
        }
    };
    let storage_root = env::var("OBJEXEL_STORAGE_DIR").unwrap_or_else(|_| "/var/lib/objexel".into());
    let recorder = Recorder::new(RecorderConfig { storage_root: storage_root.clone().into(), ..RecorderConfig::default() });
    let playback = PlaybackService::new(storage_root);
    let cleanup_recorder = recorder.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 60 * 60));
        loop {
            interval.tick().await;
            if let Err(error) = cleanup_recorder.cleanup_paths(chrono::Utc::now()).await { tracing::warn!(%error, "recording retention cleanup failed"); }
        }
    });
    let camera_service = CameraService::default();
    if let Some(database) = &database {
        for camera in database.list_cameras().await? {
            if camera.enabled {
                let health_database = database.clone();
                camera_service.spawn_monitor(camera, move |result| {
                    let health_database = health_database.clone();
                    async move {
                        let connected_at = (result.status == objexel_common::CameraStatus::Online).then(chrono::Utc::now);
                        let _ = health_database
                            .update_camera_health(
                                result.camera_id,
                                result.status,
                                connected_at,
                                None,
                                result.error.as_deref(),
                            )
                            .await;
                    }
                });
            }
        }
    }
    let ingestor_fps: f64 = env::var("OBJEXEL_INGEST_FPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5.0);

    let pipeline = database.as_ref().map(|database| ObservationPipeline::new(database.clone(), recorder.clone()));
    if let Some(pipeline) = &pipeline {
        let model_root = env::var("OBJEXEL_MODEL_DIR").unwrap_or_else(|_| "/models".into());
        let registry = ModelRegistry::new(model_root, pipeline.models.clone());
        if let Some(database) = &database {
            match registry.discover().await {
                Ok(discovered) => {
                    let existing = database.list_models().await.unwrap_or_default();
                    for (index, config) in discovered.into_iter().enumerate() {
                        if existing.iter().any(|model| model.path == config.path.to_string_lossy()) { continue; }
                        let input = objexel_common::CreateModel {
                            name: config.name,
                            version: config.version,
                            model_type: config.model_type,
                            path: config.path.to_string_lossy().into_owned(),
                            input_width: config.input_width,
                            input_height: config.input_height,
                            class_list: config.labels,
                            enabled: true,
                            default_model: existing.is_empty() && index == 0,
                        };
                        if let Err(error) = database.create_model(input).await {
                            tracing::warn!(%error, "could not register discovered model");
                        }
                    }
                    match database.list_models().await {
                        Ok(models) => {
                            if let Err(error) = registry.load_registered(&models).await {
                                tracing::warn!(%error, "registered model loading failed; models can still be reloaded through the API");
                            }
                        }
                        Err(error) => tracing::warn!(%error, "could not read registered models during startup"),
                    }
                }
                Err(error) => tracing::warn!(%error, "model discovery failed; models can still be loaded through the API"),
            }
        }
    }
    // Live-events broadcast channel: pipeline results published here fan out to every
    // connected /api/v1/events WebSocket subscriber. Created unconditionally so the API
    // exposes the endpoint even in health-only mode (it simply never emits).
    let events = live_channel(256);
    // --- Live frame ingestion ---------------------------------------------------
    // Keep camera handles in a shared runtime so API-created, edited, disabled, and
    // deleted cameras take effect immediately instead of requiring a restart.
    let camera_runtime: Option<Arc<dyn CameraRuntime>> = Some(Arc::new(LiveCameraRuntime {
        ingestor: FrameIngestor::new(IngestorConfig { fps: ingestor_fps, ..IngestorConfig::default() }),
        pipeline: pipeline.clone(),
        events: events.clone(),
        recorder: recorder.clone(),
        database: database.clone(),
        handles: Mutex::new(HashMap::new()),
        recording_handles: Mutex::new(HashMap::new()),
    }));
    if let Some(database) = &database {
        if let Some(runtime) = &camera_runtime {
            for camera in database.list_cameras().await? {
                runtime.apply(&camera).await?;
            }
        }
    }

    let app = router(AppState { database, camera_runtime, camera_service, pipeline, recorder, playback, events });
    let web_dir = env::var("OBJEXEL_WEB_DIR").unwrap_or_else(|_| "/usr/local/share/objexel/web".into());
    let app = app.fallback_service(ServeDir::new(&web_dir).not_found_service(ServeFile::new(format!("{web_dir}/index.html"))));
    let address = env::var("OBJEXEL_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = TcpListener::bind(&address).await?;
    tracing::info!(%address, "Objexel API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received; stopping Objexel gracefully");
        })
        .await?;
    Ok(())
}
