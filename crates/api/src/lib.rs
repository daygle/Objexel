use axum::{
    body::Body,
    extract::{Path, State, WebSocketUpgrade},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};
use chrono::Utc;
use objexel_camera::{CameraManager, CameraService};
use objexel_common::{Camera, CameraStatus, CameraTestResult, CreateCamera, HealthResponse, StreamMetadata, UpdateCamera};
use objexel_database::Database;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub database: Option<Database>,
    pub camera_service: CameraService,
}

#[derive(OpenApi)]
#[openapi(
    paths(health, ready, list_cameras, create_camera, get_camera, update_camera, delete_camera, test_camera, snapshot_camera, camera_status),
    components(schemas(Camera, CreateCamera, UpdateCamera, CameraStatus, CameraTestResult, StreamMetadata, HealthResponse)),
    tags((name = "cameras", description = "Camera management and RTSP ingestion"))
)]
pub struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let camera_routes = Router::new()
        .route("/api/cameras", get(list_cameras).post(create_camera))
        .route("/api/cameras/:id", get(get_camera).put(update_camera).patch(update_camera).delete(delete_camera))
        .route("/api/cameras/:id/test", axum::routing::post(test_camera))
        .route("/api/cameras/:id/snapshot", axum::routing::post(snapshot_camera))
        .route("/api/cameras/:id/status", get(camera_status))
        .route("/api/v1/cameras", get(list_cameras).post(create_camera))
        .route("/api/v1/cameras/:id", get(get_camera).put(update_camera).patch(update_camera).delete(delete_camera))
        .route("/api/v1/cameras/:id/test", axum::routing::post(test_camera))
        .route("/api/v1/cameras/:id/snapshot", axum::routing::post(snapshot_camera))
        .route("/api/v1/cameras/:id/status", get(camera_status));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/openapi.json", get(openapi))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/api/v1/events", get(events_socket))
        .merge(camera_routes)
        .with_state(Arc::new(state))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

#[utoipa::path(get, path = "/health", responses((status = 200, body = HealthResponse)))]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".into(), service: "objexel-api".into(), version: env!("CARGO_PKG_VERSION").into() })
}

#[utoipa::path(get, path = "/ready", responses((status = 200), (status = 503)))]
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

#[utoipa::path(get, path = "/api/cameras", tag = "cameras", responses((status = 200, body = [Camera]), (status = 503)))]
async fn list_cameras(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Camera>>, ErrorResponse> {
    let database = database(&state)?;
    database.list_cameras().await.map(Json).map_err(internal_error)
}

#[utoipa::path(post, path = "/api/cameras", tag = "cameras", request_body = CreateCamera, responses((status = 201, body = Camera), (status = 400), (status = 503)))]
async fn create_camera(State(state): State<Arc<AppState>>, Json(input): Json<CreateCamera>) -> Result<(StatusCode, Json<Camera>), ErrorResponse> {
    validate_camera_input(&input.name, &input.rtsp_url)?;
    let database = database(&state)?;
    database.create_camera(input).await.map(|camera| (StatusCode::CREATED, Json(camera))).map_err(internal_error)
}

#[utoipa::path(get, path = "/api/cameras/{id}", tag = "cameras", params(("id" = Uuid, Path, description = "Camera identifier")), responses((status = 200, body = Camera), (status = 404), (status = 503)))]
async fn get_camera(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Camera>, ErrorResponse> {
    match database(&state)?.get_camera(id).await.map_err(internal_error)? {
        Some(camera) => Ok(Json(camera)),
        None => Err(not_found("camera not found")),
    }
}

#[utoipa::path(put, path = "/api/cameras/{id}", tag = "cameras", params(("id" = Uuid, Path, description = "Camera identifier")), request_body = UpdateCamera, responses((status = 200, body = Camera), (status = 400), (status = 404), (status = 503)))]
async fn update_camera(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateCamera>) -> Result<Json<Camera>, ErrorResponse> {
    if let Some(name) = &input.name { if name.trim().is_empty() { return Err(bad_request("name cannot be empty")); } }
    if let Some(rtsp_url) = &input.rtsp_url { if rtsp_url.trim().is_empty() { return Err(bad_request("rtsp_url cannot be empty")); } }
    match database(&state)?.update_camera(id, input).await.map_err(internal_error)? {
        Some(camera) => Ok(Json(camera)),
        None => Err(not_found("camera not found")),
    }
}

#[utoipa::path(delete, path = "/api/cameras/{id}", tag = "cameras", params(("id" = Uuid, Path, description = "Camera identifier")), responses((status = 204), (status = 404), (status = 503)))]
async fn delete_camera(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> {
    if database(&state)?.delete_camera(id).await.map_err(internal_error)? { Ok(StatusCode::NO_CONTENT) } else { Err(not_found("camera not found")) }
}

#[utoipa::path(post, path = "/api/cameras/{id}/test", tag = "cameras", params(("id" = Uuid, Path, description = "Camera identifier")), responses((status = 200, body = CameraTestResult), (status = 404), (status = 503)))]
async fn test_camera(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<CameraTestResult>, ErrorResponse> {
    let database = database(&state)?;
    let camera = database.get_camera(id).await.map_err(internal_error)?.ok_or_else(|| not_found("camera not found"))?;
    let result = state.camera_service.test_connection(&camera).await;
    let connected_at = (result.status == CameraStatus::Online).then(Utc::now);
    database.update_camera_health(id, result.status.clone(), connected_at, None, result.error.as_deref()).await.map_err(internal_error)?;
    Ok(Json(result))
}

#[utoipa::path(post, path = "/api/cameras/{id}/snapshot", tag = "cameras", params(("id" = Uuid, Path, description = "Camera identifier")), responses((status = 200, description = "JPEG snapshot"), (status = 404), (status = 502), (status = 503)))]
async fn snapshot_camera(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Response, ErrorResponse> {
    let database = database(&state)?;
    let camera = database.get_camera(id).await.map_err(internal_error)?.ok_or_else(|| not_found("camera not found"))?;
    let bytes = match state.camera_service.snapshot(&camera).await {
        Ok(bytes) => bytes,
        Err(error) => {
            let message = error.to_string();
            database.update_camera_health(id, CameraStatus::Offline, None, None, Some(&message)).await.map_err(internal_error)?;
            return Err(bad_gateway("snapshot capture failed"));
        }
    };
    let now = Utc::now();
    database.update_camera_health(id, CameraStatus::Online, Some(now), Some(now), None).await.map_err(internal_error)?;
    Response::builder().status(StatusCode::OK).header(header::CONTENT_TYPE, "image/jpeg").body(Body::from(bytes)).map_err(|error| internal_error(anyhow::anyhow!(error)))
}

#[utoipa::path(get, path = "/api/cameras/{id}/status", tag = "cameras", params(("id" = Uuid, Path, description = "Camera identifier")), responses((status = 200, body = Camera), (status = 404), (status = 503)))]
async fn camera_status(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Camera>, ErrorResponse> {
    match database(&state)?.get_camera(id).await.map_err(internal_error)? { Some(camera) => Ok(Json(camera)), None => Err(not_found("camera not found")) }
}

async fn events_socket(ws: WebSocketUpgrade) -> impl axum::response::IntoResponse { ws.on_upgrade(|_socket| async move { tracing::debug!("event websocket connected"); }) }
async fn openapi() -> Json<utoipa::openapi::OpenApi> { Json(ApiDoc::openapi()) }

type ErrorResponse = (StatusCode, Json<Value>);

fn database(state: &AppState) -> Result<&Database, ErrorResponse> { state.database.as_ref().ok_or_else(|| service_unavailable("database is not configured")) }
fn validate_camera_input(name: &str, rtsp_url: &str) -> Result<(), ErrorResponse> { if name.trim().is_empty() || rtsp_url.trim().is_empty() { Err(bad_request("name and rtsp_url are required")) } else { Ok(()) } }
fn bad_request(message: &str) -> ErrorResponse { (StatusCode::BAD_REQUEST, Json(json!({"error": message}))) }
fn bad_gateway(message: &str) -> ErrorResponse { (StatusCode::BAD_GATEWAY, Json(json!({"error": message}))) }
fn not_found(message: &str) -> ErrorResponse { (StatusCode::NOT_FOUND, Json(json!({"error": message}))) }
fn service_unavailable(message: &str) -> ErrorResponse { (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": message}))) }
fn internal_error(error: anyhow::Error) -> ErrorResponse { tracing::error!(%error, "request failed"); (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"}))) }

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt;

    fn test_state() -> AppState { AppState { database: None, camera_service: CameraService::default() } }

    #[tokio::test]
    async fn health_endpoint_is_available() {
        let response = router(test_state()).oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn requested_camera_routes_exist_without_database() {
        let response = router(test_state()).oneshot(Request::builder().uri("/api/cameras").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn openapi_contains_ingestion_paths() {
        let document = ApiDoc::openapi();
        assert!(document.paths.paths.contains_key("/api/cameras/{id}/snapshot"));
        assert!(document.paths.paths.contains_key("/api/cameras/{id}/test"));
    }
}
