use objexel_api::{router, AppState};
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
    let app = router(AppState { database });
    let address = env::var("OBJEXEL_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = TcpListener::bind(&address).await?;
    tracing::info!(%address, "Objexel API listening");
    axum::serve(listener, app).await?;
    Ok(())
}
