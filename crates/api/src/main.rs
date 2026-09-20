use objexel_api::{broadcast_pipeline_result, live_channel, router, AppState};
use objexel_camera::{CameraService, FrameIngestor, IngestorConfig};
use objexel_detector::FrameTensor;
use objexel_pipeline::ObservationPipeline;
use objexel_playback::PlaybackService;
use objexel_recorder::{Recorder, RecorderConfig};
use objexel_models::ModelRegistry;
use objexel_database::Database;
use std::env;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};

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
                let recording_camera = camera.clone();
                let recording_service = recorder.clone();
                match recording_service.start_continuous(recording_camera.id, &recording_camera.rtsp_url).await {
                    Ok((recording, mut child)) => {
                        if let Err(error) = database.insert_recording(&recording).await { tracing::warn!(%error, "could not persist recording metadata"); }
                        tokio::spawn(async move { let _ = child.wait().await; });
                    }
                    Err(error) => tracing::warn!(camera_id = %recording_camera.id, %error, "continuous recording could not start"),
                }
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
    if let (Some(database), Some(pipeline)) = (&database, &pipeline) {
        let ingestor = FrameIngestor::new(IngestorConfig { fps: ingestor_fps, ..IngestorConfig::default() });
        for camera in database.list_cameras().await? {
            if !camera.enabled { continue; }
            let pipeline = pipeline.clone();
            let events = events.clone();
            let camera_id = camera.id;
            ingestor.spawn(camera_id, &camera.rtsp_url, move |frame| {
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
                        Ok(result) => {
                            broadcast_pipeline_result(&events, &result);
                            tracing::debug!(camera_id = %camera_id, detections = result.detections.len(), "live frame processed");
                        }
                        Err(error) => {
                            tracing::warn!(camera_id = %camera_id, %error, "live frame processing failed");
                        }
                    }
                }
            });
            tracing::info!(camera_id = %camera_id, fps = ingestor_fps, "live frame ingestor started");
        }
    }

    let app = router(AppState { database, camera_service, pipeline, recorder, playback, events });
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
