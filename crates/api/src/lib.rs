use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use objexel_common::{Camera, CreateCamera, HealthResponse, UpdateCamera};
use objexel_database::Database;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub database: Option<Database>,
}

#[derive(OpenApi)]
#[openapi(
    paths(health, ready, list_cameras, create_camera, get_camera, update_camera, delete_camera),
    components(schemas(Camera, CreateCamera, UpdateCamera, HealthResponse)),
    tags((name = "cameras", description = "Camera management"))
)]
pub struct ApiDoc;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/cameras", get(list_cameras).post(create_camera))
        .route(
            "/api/v1/cameras/:id",
            get(get_camera).patch(update_camera).delete(delete_camera),
        )
        .route("/api/v1/openapi.json", get(openapi))
        .route("/api/v1/events", get(events_socket))
        .with_state(Arc::new(state))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

#[utoipa::path(get, path = "/health", responses((status = 200, body = HealthResponse)))]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        service: "objexel-api".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

#[utoipa::path(get, path = "/ready", responses((status = 200), (status = 503)))]
async fn ready(State(state): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    match &state.database {
        Some(database) => match database.list_cameras().await {
            Ok(_) => (StatusCode::OK, Json(json!({"status": "ready", "database": "ok"}))),
            Err(error) => {
                tracing::warn!(%error, "database readiness check failed");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"status": "not_ready", "database": "unavailable"})),
                )
            }
        },
        None => (
            StatusCode::OK,
            Json(json!({"status": "ready", "database": "not_configured"})),
        ),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/cameras",
    tag = "cameras",
    responses((status = 200, body = [Camera]), (status = 503))
)]
async fn list_cameras(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Camera>>, (StatusCode, Json<Value>)> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| service_unavailable("database is not configured"))?;
    database
        .list_cameras()
        .await
        .map(Json)
        .map_err(internal_error)
}

#[utoipa::path(
    post,
    path = "/api/v1/cameras",
    tag = "cameras",
    request_body = CreateCamera,
    responses((status = 201, body = Camera), (status = 400), (status = 503))
)]
async fn create_camera(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateCamera>,
) -> Result<(StatusCode, Json<Camera>), (StatusCode, Json<Value>)> {
    validate_camera_input(&input.name, &input.rtsp_url)?;
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| service_unavailable("database is not configured"))?;
    database
        .create_camera(input)
        .await
        .map(|camera| (StatusCode::CREATED, Json(camera)))
        .map_err(internal_error)
}

#[utoipa::path(
    get,
    path = "/api/v1/cameras/{id}",
    tag = "cameras",
    params(("id" = Uuid, Path, description = "Camera identifier")),
    responses((status = 200, body = Camera), (status = 404), (status = 503))
)]
async fn get_camera(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Camera>, (StatusCode, Json<Value>)> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| service_unavailable("database is not configured"))?;
    match database.get_camera(id).await.map_err(internal_error)? {
        Some(camera) => Ok(Json(camera)),
        None => Err(not_found("camera not found")),
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/cameras/{id}",
    tag = "cameras",
    params(("id" = Uuid, Path, description = "Camera identifier")),
    request_body = UpdateCamera,
    responses((status = 200, body = Camera), (status = 400), (status = 404), (status = 503))
)]
async fn update_camera(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateCamera>,
) -> Result<Json<Camera>, (StatusCode, Json<Value>)> {
    if let Some(name) = &input.name {
        if name.trim().is_empty() {
            return Err(bad_request("name cannot be empty"));
        }
    }
    if let Some(rtsp_url) = &input.rtsp_url {
        if rtsp_url.trim().is_empty() {
            return Err(bad_request("rtsp_url cannot be empty"));
        }
    }
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| service_unavailable("database is not configured"))?;
    match database.update_camera(id, input).await.map_err(internal_error)? {
        Some(camera) => Ok(Json(camera)),
        None => Err(not_found("camera not found")),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/cameras/{id}",
    tag = "cameras",
    params(("id" = Uuid, Path, description = "Camera identifier")),
    responses((status = 204), (status = 404), (status = 503))
)]
async fn delete_camera(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| service_unavailable("database is not configured"))?;
    if database.delete_camera(id).await.map_err(internal_error)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found("camera not found"))
    }
}

async fn events_socket(ws: WebSocketUpgrade) -> impl axum::response::IntoResponse {
    ws.on_upgrade(|_socket| async move {
        tracing::debug!("event websocket connected");
    })
}

async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

fn validate_camera_input(name: &str, rtsp_url: &str) -> Result<(), (StatusCode, Json<Value>)> {
    if name.trim().is_empty() || rtsp_url.trim().is_empty() {
        Err(bad_request("name and rtsp_url are required"))
    } else {
        Ok(())
    }
}

fn bad_request(message: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"error": message})))
}

fn not_found(message: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({"error": message})))
}

fn service_unavailable(message: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": message})))
}

fn internal_error(error: anyhow::Error) -> (StatusCode, Json<Value>) {
    tracing::error!(%error, "request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "internal server error"})),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_endpoint_is_available() {
        let response = router(AppState { database: None })
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn camera_validation_rejects_blank_values() {
        assert!(validate_camera_input("", "rtsp://camera").is_err());
        assert!(validate_camera_input("Front door", "").is_err());
        assert!(validate_camera_input("Front door", "rtsp://camera").is_ok());
    }

    #[test]
    fn openapi_contains_camera_crud_paths() {
        let document = ApiDoc::openapi();
        assert!(document.paths.paths.contains_key("/api/v1/cameras"));
        assert!(document.paths.paths.contains_key("/api/v1/cameras/{id}"));
    }
}
