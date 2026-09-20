use objexel_api::{router, AppState};
use objexel_camera::CameraService;
use objexel_database::Database;
use std::env;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).init();
    let database = match env::var("DATABASE_URL") {
        Ok(url) => {
            let database = Database::connect(&url).await?;
            database.migrate().await?;
            Some(database)
        }
        Err(_) => {
            tracing::warn!("DATABASE_URL is not configured; starting in health-only mode");
            None
        }
    };
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
    let app = router(AppState { database, camera_service });
    let address = env::var("OBJEXEL_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = TcpListener::bind(&address).await?;
    tracing::info!(%address, "Objexel API listening");
    axum::serve(listener, app).await?;
    Ok(())
}
