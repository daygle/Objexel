use axum::{extract::{State, WebSocketUpgrade}, http::StatusCode, routing::get, Json, Router};
use objexel_common::{Camera, CreateCamera, HealthResponse};
use objexel_database::Database;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
pub struct AppState {
    pub database: Option<Database>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/cameras", get(list_cameras).post(create_camera))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/api/v1/events", get(events_socket))
        .with_state(Arc::new(state))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok", service: "objexel-api", version: env!("CARGO_PKG_VERSION") })
}

async fn ready(State(state): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    match &state.database {
        Some(database) => match database.list_cameras().await {
            Ok(_) => (StatusCode::OK, Json(json!({"status": "ready", "database": "ok"}))),
            Err(error) => {
                tracing::warn!(%error, "database readiness check failed");
                (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"status": "not_ready", "database": "unavailable"})))
            }
        },
        None => (StatusCode::OK, Json(json!({"status": "ready", "database": "not_configured"}))),
    }
}

async fn list_cameras(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Camera>>, (StatusCode, Json<Value>)> {
    let database = state.database.as_ref().ok_or_else(|| service_unavailable("database is not configured"))?;
    database.list_cameras().await.map(Json).map_err(internal_error)
}

async fn create_camera(State(state): State<Arc<AppState>>, Json(input): Json<CreateCamera>) -> Result<(StatusCode, Json<Camera>), (StatusCode, Json<Value>)> {
    if input.name.trim().is_empty() || input.rtsp_url.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "name and rtsp_url are required"}))));
    }
    let database = state.database.as_ref().ok_or_else(|| service_unavailable("database is not configured"))?;
    database.create_camera(input).await.map(|camera| (StatusCode::CREATED, Json(camera))).map_err(internal_error)
}

async fn events_socket(ws: WebSocketUpgrade) -> impl axum::response::IntoResponse {
    ws.on_upgrade(|_socket| async move {
        tracing::debug!("event websocket connected");
    })
}

async fn openapi() -> Json<Value> {
    Json(json!({"openapi": "3.0.3", "info": {"title": "Objexel API", "version": env!("CARGO_PKG_VERSION")}, "paths": {"/health": {"get": {}}, "/ready": {"get": {}}, "/api/v1/cameras": {"get": {}, "post": {}}, "/api/v1/events": {"get": {"description": "WebSocket event stream"}}}}))
}

fn service_unavailable(message: &str) -> (StatusCode, Json<Value>) { (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": message}))) }
fn internal_error(error: anyhow::Error) -> (StatusCode, Json<Value>) { tracing::error!(%error, "request failed"); (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"}))) }

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}, Router};
    use tower::ServiceExt;
    #[tokio::test]
    async fn health_endpoint_is_available() {
        let response = router(AppState { database: None }).oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
