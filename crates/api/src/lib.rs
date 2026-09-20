use axum::{
    body::Body,
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};
use chrono::Utc;
use objexel_actions::ActionDispatcher;
use objexel_analytics::AnalyticsSummary;
use objexel_common::{AnomalyEvent, Behaviour, BehaviourScore, CreateModelAssignment, IdentityScore, FusionResult, Identity, IdentityObservation, IdentityStatistics, IntelligenceSummary, ModelAssignment, UpdateIdentity};
use objexel_camera::{CameraManager, CameraService};
use objexel_common::{Action, ActionExecution, BenchmarkResult, Camera, CameraStatus, CameraTestResult, Clip, CreateAction, CreateCamera, CreateModel, CreateNotificationProvider, CreateNotificationTemplate, CreateRule, Detection, Event, HealthResponse, Model, Notification, NotificationProvider, NotificationTemplate, Observation, Recording, Rule, Snapshot, StreamMetadata, Track, UpdateAction, UpdateCamera, UpdateNotificationProvider, UpdateRule, UpdateZone, Zone, ZoneEvent};
use objexel_pipeline::ObservationPipeline;
use objexel_models::ModelRegistry;
use objexel_playback::PlaybackService;
use objexel_recorder::Recorder;
use objexel_search::{SearchFilters, SearchResult};
use objexel_zones::validate_polygon;
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
    pub pipeline: Option<ObservationPipeline>,
    pub recorder: Recorder,
    pub playback: PlaybackService,
}

#[derive(OpenApi)]
#[openapi(
    paths(health, ready, list_cameras, create_camera, get_camera, update_camera, delete_camera, test_camera, snapshot_camera, camera_status, assign_camera_model, list_models, get_model, create_model, delete_model, reload_models, activate_model, benchmark_model, list_benchmarks, list_actions, get_action, create_action, update_action, delete_action, list_executions, list_notifications, list_providers, create_provider, update_provider, list_templates, create_template, test_notification, global_search, search_events, search_observations, search_tracks, search_recordings, search_detections, search_behaviours, list_behaviours, get_behaviour, list_identities, get_identity, update_identity, identity_history, intelligence_summary, list_anomalies, list_fusion, list_model_assignments, create_model_assignment, analytics_summary, analytics_cameras, analytics_zones, analytics_models, list_recordings, get_recording, list_clips, get_clip, clip_media, download_clip, list_snapshots, get_snapshot, snapshot_media, list_detections, get_detection, list_tracks, get_track, list_observations, get_observation, list_zones, create_zone, get_zone, update_zone, delete_zone, list_zone_events, list_rules, get_rule, create_rule, update_rule, delete_rule, list_events, get_event),
    components(schemas(Camera, CreateCamera, CreateModel, UpdateCamera, CameraStatus, CameraTestResult, StreamMetadata, HealthResponse, Model, BenchmarkResult, Action, ActionExecution, CreateAction, UpdateAction, Notification, NotificationProvider, NotificationTemplate, CreateNotificationProvider, UpdateNotificationProvider, CreateNotificationTemplate, Recording, Clip, Snapshot, SearchResult, AnalyticsSummary, Behaviour, Identity, IdentityObservation, IdentityStatistics, UpdateIdentity, IdentityScore, BehaviourScore, AnomalyEvent, IntelligenceSummary, ModelAssignment, CreateModelAssignment, FusionResult, Detection, Track, Observation, Zone, CreateZone, UpdateZone, ZoneEvent, Rule, CreateRule, UpdateRule, Event)),
    tags((name = "cameras", description = "Camera management and RTSP ingestion"))
)]
pub struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let camera_routes = Router::new()
        .route("/api/cameras", get(list_cameras).post(create_camera))
        .route("/api/cameras/:id", get(get_camera).put(update_camera).patch(update_camera).delete(delete_camera))
        .route("/api/cameras/:id/test", axum::routing::post(test_camera))
        .route("/api/cameras/:id/snapshot", get(snapshot_camera).post(snapshot_camera))
        .route("/api/cameras/:id/status", get(camera_status))
        .route("/api/cameras/:id/model/:model_id", axum::routing::post(assign_camera_model))
        .route("/api/v1/cameras", get(list_cameras).post(create_camera))
        .route("/api/v1/cameras/:id", get(get_camera).put(update_camera).patch(update_camera).delete(delete_camera))
        .route("/api/v1/cameras/:id/test", axum::routing::post(test_camera))
        .route("/api/v1/cameras/:id/snapshot", get(snapshot_camera).post(snapshot_camera))
        .route("/api/v1/cameras/:id/status", get(camera_status))
        .route("/api/v1/cameras/:id/model/:model_id", axum::routing::post(assign_camera_model));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/openapi.json", get(openapi))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/api/v1/events", get(events_socket))
        .route("/api/models", get(list_models).post(create_model))
        .route("/api/models/:id", get(get_model).delete(delete_model))
        .route("/api/models/reload", axum::routing::post(reload_models))
        .route("/api/models/:id/activate", axum::routing::post(activate_model))
        .route("/api/models/:id/benchmark", axum::routing::post(benchmark_model))
        .route("/api/benchmarks", get(list_benchmarks))
        .route("/api/actions", get(list_actions).post(create_action))
        .route("/api/actions/:id", get(get_action).put(update_action).delete(delete_action))
        .route("/api/action-executions", get(list_executions))
        .route("/api/notifications", get(list_notifications))
        .route("/api/notification-providers", get(list_providers).post(create_provider))
        .route("/api/notification-providers/:id", axum::routing::put(update_provider))
        .route("/api/notification-templates", get(list_templates).post(create_template))
        .route("/api/test-notification", axum::routing::post(test_notification))
        .route("/api/search", get(global_search))
        .route("/api/search/events", get(search_events))
        .route("/api/search/observations", get(search_observations))
        .route("/api/search/tracks", get(search_tracks))
        .route("/api/search/recordings", get(search_recordings))
        .route("/api/search/detections", get(search_detections))
        .route("/api/search/behaviours", get(search_behaviours))
        .route("/api/behaviours", get(list_behaviours))
        .route("/api/behaviours/:id", get(get_behaviour))
        .route("/api/identities", get(list_identities))
        .route("/api/identities/:id", get(get_identity).put(update_identity))
        .route("/api/identities/:id/history", get(identity_history))
        .route("/api/intelligence", get(intelligence_summary))
        .route("/api/anomalies", get(list_anomalies))
        .route("/api/fusion", get(list_fusion))
        .route("/api/model-assignments", get(list_model_assignments).post(create_model_assignment))
        .route("/api/analytics", get(analytics_summary))
        .route("/api/analytics/cameras", get(analytics_cameras))
        .route("/api/analytics/zones", get(analytics_zones))
        .route("/api/analytics/models", get(analytics_models))
        .route("/api/recordings", get(list_recordings))
        .route("/api/recordings/:id", get(get_recording))
        .route("/api/clips", get(list_clips))
        .route("/api/clips/:id", get(get_clip))
        .route("/api/clips/:id/media", get(clip_media))
        .route("/api/clips/:id/download", axum::routing::post(download_clip))
        .route("/api/snapshots", get(list_snapshots))
        .route("/api/snapshots/:id", get(get_snapshot))
        .route("/api/snapshots/:id/media", get(snapshot_media))
        .route("/api/detections", get(list_detections))
        .route("/api/detections/:id", get(get_detection))
        .route("/api/tracks", get(list_tracks))
        .route("/api/tracks/:id", get(get_track))
        .route("/api/observations", get(list_observations))
        .route("/api/observations/:id", get(get_observation))
        .route("/api/zones", get(list_zones).post(create_zone))
        .route("/api/zones/:id", get(get_zone).put(update_zone).delete(delete_zone))
        .route("/api/zone-events", get(list_zone_events))
        .route("/api/rules", get(list_rules).post(create_rule))
        .route("/api/rules/:id", get(get_rule).put(update_rule).delete(delete_rule))
        .route("/api/events", get(list_events))
        .route("/api/events/:id", get(get_event))
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

#[utoipa::path(post, path = "/api/cameras/{id}/model/{model_id}", tag = "models", params(("id" = Uuid, Path), ("model_id" = Uuid, Path)), responses((status = 200), (status = 404), (status = 503)))]
async fn assign_camera_model(State(state): State<Arc<AppState>>, Path((id, model_id)): Path<(Uuid, Uuid)>) -> Result<Json<Value>, ErrorResponse> {
    if database(&state)?.get_camera(id).await.map_err(internal_error)?.is_none() { return Err(not_found("camera not found")); }
    let model = database(&state)?.get_model(model_id).await.map_err(internal_error)?.ok_or_else(|| not_found("model not found"))?;
    if !model.enabled { return Err(bad_request("cannot assign a disabled model")); }
    database(&state)?.assign_camera_model(id, model_id).await.map_err(internal_error)?;
    Ok(Json(json!({"camera_id": id, "model_id": model_id})))
}

#[derive(Debug, serde::Deserialize)]
struct LimitQuery { limit: Option<i64> }

#[utoipa::path(get, path = "/api/models", tag = "models", responses((status = 200, body = [Model]), (status = 503)))]
async fn list_models(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Model>>, ErrorResponse> {
    database(&state)?.list_models().await.map(Json).map_err(internal_error)
}

#[utoipa::path(get, path = "/api/models/{id}", tag = "models", params(("id" = Uuid, Path)), responses((status = 200, body = Model), (status = 404), (status = 503)))]
async fn get_model(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Model>, ErrorResponse> { match database(&state)?.get_model(id).await.map_err(internal_error)? { Some(model) => Ok(Json(model)), None => Err(not_found("model not found")) } }

#[utoipa::path(post, path = "/api/models", tag = "models", request_body = CreateModel, responses((status = 201, body = Model), (status = 400), (status = 503)))]
async fn create_model(State(state): State<Arc<AppState>>, Json(input): Json<CreateModel>) -> Result<(StatusCode, Json<Model>), ErrorResponse> {
    ModelRegistry::validate(input.path.as_ref(), input.input_width, input.input_height).await.map_err(|error| bad_request(&error.to_string()))?;
    database(&state)?.create_model(input).await.map(|model| (StatusCode::CREATED, Json(model))).map_err(internal_error)
}

#[utoipa::path(delete, path = "/api/models/{id}", tag = "models", params(("id" = Uuid, Path)), responses((status = 204), (status = 404), (status = 503)))]
async fn delete_model(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> { if database(&state)?.delete_model(id).await.map_err(internal_error)? { Ok(StatusCode::NO_CONTENT) } else { Err(not_found("model not found")) } }

#[utoipa::path(post, path = "/api/models/{id}/activate", tag = "models", params(("id" = Uuid, Path)), responses((status = 200), (status = 404), (status = 503)))]
async fn activate_model(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Value>, ErrorResponse> { if database(&state)?.activate_model(id).await.map_err(internal_error)? { if let Some(pipeline) = &state.pipeline { let _ = pipeline.models.set_active(id).await; } Ok(Json(json!({"active": id}))) } else { Err(not_found("model not found or disabled")) } }

#[utoipa::path(post, path = "/api/models/{id}/benchmark", tag = "models", params(("id" = Uuid, Path)), responses((status = 200, body = BenchmarkResult), (status = 404), (status = 503)))]
async fn benchmark_model(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<BenchmarkResult>, ErrorResponse> {
    let model = database(&state)?.get_model(id).await.map_err(internal_error)?.ok_or_else(|| not_found("model not found"))?;
    let pipeline = state.pipeline.as_ref().ok_or_else(|| service_unavailable("inference pipeline is not configured"))?;
    let registry = ModelRegistry::new("/models", pipeline.models.clone());
    let result = registry.benchmark(&model, 10).await.map_err(|error| bad_request(&error.to_string()))?;
    database(&state)?.insert_benchmark(&result).await.map_err(internal_error)?;
    Ok(Json(result))
}

#[utoipa::path(get, path = "/api/benchmarks", tag = "models", responses((status = 200, body = [BenchmarkResult]), (status = 503)))]
async fn list_benchmarks(State(state): State<Arc<AppState>>) -> Result<Json<Vec<BenchmarkResult>>, ErrorResponse> { database(&state)?.list_benchmarks(None).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/actions", tag = "notifications", responses((status = 200, body = [Action]), (status = 503)))]
async fn list_actions(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Action>>, ErrorResponse> { database(&state)?.list_actions().await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/actions/{id}", tag = "notifications", params(("id" = Uuid, Path)), responses((status = 200, body = Action), (status = 404), (status = 503)))]
async fn get_action(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Action>, ErrorResponse> { match database(&state)?.get_action(id).await.map_err(internal_error)? { Some(action) => Ok(Json(action)), None => Err(not_found("action not found")) } }

#[utoipa::path(post, path = "/api/actions", tag = "notifications", request_body = CreateAction, responses((status = 201, body = Action), (status = 400), (status = 503)))]
async fn create_action(State(state): State<Arc<AppState>>, Json(input): Json<CreateAction>) -> Result<(StatusCode, Json<Action>), ErrorResponse> {
    if input.name.trim().is_empty() { return Err(bad_request("action name cannot be empty")); }
    database(&state)?.create_action(input).await.map(|action| (StatusCode::CREATED, Json(action))).map_err(internal_error)
}

#[utoipa::path(put, path = "/api/actions/{id}", tag = "notifications", params(("id" = Uuid, Path)), request_body = UpdateAction, responses((status = 200, body = Action), (status = 404), (status = 503)))]
async fn update_action(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateAction>) -> Result<Json<Action>, ErrorResponse> { match database(&state)?.update_action(id, input).await.map_err(internal_error)? { Some(action) => Ok(Json(action)), None => Err(not_found("action not found")) } }

#[utoipa::path(delete, path = "/api/actions/{id}", tag = "notifications", params(("id" = Uuid, Path)), responses((status = 204), (status = 404), (status = 503)))]
async fn delete_action(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> { if database(&state)?.delete_action(id).await.map_err(internal_error)? { Ok(StatusCode::NO_CONTENT) } else { Err(not_found("action not found")) } }

#[utoipa::path(get, path = "/api/action-executions", tag = "notifications", responses((status = 200, body = [ActionExecution]), (status = 503)))]
async fn list_executions(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<ActionExecution>>, ErrorResponse> { database(&state)?.list_executions(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/notifications", tag = "notifications", responses((status = 200, body = [Notification]), (status = 503)))]
async fn list_notifications(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<Notification>>, ErrorResponse> { database(&state)?.list_notifications(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/notification-providers", tag = "notifications", responses((status = 200, body = [NotificationProvider]), (status = 503)))]
async fn list_providers(State(state): State<Arc<AppState>>) -> Result<Json<Vec<NotificationProvider>>, ErrorResponse> { database(&state)?.list_providers().await.map(Json).map_err(internal_error) }

#[utoipa::path(post, path = "/api/notification-providers", tag = "notifications", request_body = CreateNotificationProvider, responses((status = 201, body = NotificationProvider), (status = 503)))]
async fn create_provider(State(state): State<Arc<AppState>>, Json(input): Json<CreateNotificationProvider>) -> Result<(StatusCode, Json<NotificationProvider>), ErrorResponse> { database(&state)?.create_provider(input).await.map(|provider| (StatusCode::CREATED, Json(provider))).map_err(internal_error) }

#[utoipa::path(put, path = "/api/notification-providers/{id}", tag = "notifications", params(("id" = Uuid, Path)), request_body = UpdateNotificationProvider, responses((status = 200, body = NotificationProvider), (status = 404), (status = 503)))]
async fn update_provider(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateNotificationProvider>) -> Result<Json<NotificationProvider>, ErrorResponse> { match database(&state)?.update_provider(id, input).await.map_err(internal_error)? { Some(provider) => Ok(Json(provider)), None => Err(not_found("provider not found")) } }

#[utoipa::path(get, path = "/api/notification-templates", tag = "notifications", responses((status = 200, body = [NotificationTemplate]), (status = 503)))]
async fn list_templates(State(state): State<Arc<AppState>>) -> Result<Json<Vec<NotificationTemplate>>, ErrorResponse> { database(&state)?.list_templates().await.map(Json).map_err(internal_error) }

#[utoipa::path(post, path = "/api/notification-templates", tag = "notifications", request_body = CreateNotificationTemplate, responses((status = 201, body = NotificationTemplate), (status = 503)))]
async fn create_template(State(state): State<Arc<AppState>>, Json(input): Json<CreateNotificationTemplate>) -> Result<(StatusCode, Json<NotificationTemplate>), ErrorResponse> { database(&state)?.create_template(input).await.map(|template| (StatusCode::CREATED, Json(template))).map_err(internal_error) }

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
struct TestNotification { provider_id: Uuid, template_id: Option<Uuid> }

#[utoipa::path(post, path = "/api/test-notification", tag = "notifications", request_body = TestNotification, responses((status = 200), (status = 400), (status = 503)))]
async fn test_notification(State(state): State<Arc<AppState>>, Json(input): Json<TestNotification>) -> Result<Json<Value>, ErrorResponse> {
    let database = database(&state)?;
    let provider = database.provider(input.provider_id).await.map_err(internal_error)?.ok_or_else(|| not_found("provider not found"))?;
    let template = match input.template_id { Some(id) => database.template(id).await.map_err(internal_error)?, None => None };
    let event = Event { id: Uuid::new_v4(), rule_id: Uuid::nil(), camera_id: Uuid::nil(), track_id: None, observation_id: None, event_type: "test_notification".into(), summary: "Objexel test notification".into(), severity: objexel_common::EventSeverity::Info, created_at: Utc::now() };
    let execution = ActionDispatcher::new().execute(&Action { id: Uuid::new_v4(), name: "test notification".into(), action_type: provider.provider_type.clone(), provider_id: Some(provider.id), template_id: input.template_id, enabled: true, configuration: serde_json::json!({}), created_at: Utc::now(), updated_at: Utc::now() }, Some(&provider), template.as_ref(), &event).await;
    let success = execution.status == "success";
    if !success { return Err(bad_request(&execution.error_message.unwrap_or_else(|| "notification failed".into()))); }
    Ok(Json(json!({"status":"sent"})))
}

#[utoipa::path(get, path = "/api/search", tag = "search", responses((status = 200, body = [SearchResult]), (status = 503)))]
async fn global_search(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<Vec<SearchResult>>, ErrorResponse> {
    let database = database(&state)?;
    let mut results = database.search_detections(&filters).await.map_err(internal_error)?;
    results.extend(database.search_events(&filters).await.map_err(internal_error)?);
    results.extend(database.search_observations(&filters).await.map_err(internal_error)?);
    results.extend(database.search_tracks(&filters).await.map_err(internal_error)?);
    results.extend(database.search_recordings(&filters).await.map_err(internal_error)?);
    results.extend(database.search_behaviours(&filters).await.map_err(internal_error)?);
    results.sort_by(|left, right| right.occurred_at.cmp(&left.occurred_at));
    results.truncate(filters.limit() as usize);
    Ok(Json(results))
}

#[utoipa::path(get, path = "/api/search/detections", tag = "search", responses((status = 200, body = [SearchResult]), (status = 503)))]
async fn search_detections(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<Vec<SearchResult>>, ErrorResponse> { database(&state)?.search_detections(&filters).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/search/events", tag = "search", responses((status = 200, body = [SearchResult]), (status = 503)))]
async fn search_events(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<Vec<SearchResult>>, ErrorResponse> { database(&state)?.search_events(&filters).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/search/observations", tag = "search", responses((status = 200, body = [SearchResult]), (status = 503)))]
async fn search_observations(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<Vec<SearchResult>>, ErrorResponse> { database(&state)?.search_observations(&filters).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/search/tracks", tag = "search", responses((status = 200, body = [SearchResult]), (status = 503)))]
async fn search_tracks(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<Vec<SearchResult>>, ErrorResponse> { database(&state)?.search_tracks(&filters).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/search/recordings", tag = "search", responses((status = 200, body = [SearchResult]), (status = 503)))]
async fn search_recordings(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<Vec<SearchResult>>, ErrorResponse> { database(&state)?.search_recordings(&filters).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/search/behaviours", tag = "search", responses((status = 200, body = [SearchResult]), (status = 503)))]
async fn search_behaviours(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<Vec<SearchResult>>, ErrorResponse> { database(&state)?.search_behaviours(&filters).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/behaviours", tag = "behaviour", responses((status = 200, body = [Behaviour]), (status = 503)))]
async fn list_behaviours(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<Behaviour>>, ErrorResponse> { database(&state)?.list_behaviours(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/behaviours/{id}", tag = "behaviour", params(("id" = Uuid, Path)), responses((status = 200, body = Behaviour), (status = 404), (status = 503)))]
async fn get_behaviour(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Behaviour>, ErrorResponse> { match database(&state)?.get_behaviour(id).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("behaviour not found")) } }

#[utoipa::path(get, path = "/api/identities", tag = "identity", responses((status = 200, body = [Identity]), (status = 503)))]
async fn list_identities(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<Identity>>, ErrorResponse> { database(&state)?.list_identities(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/identities/{id}", tag = "identity", params(("id" = Uuid, Path)), responses((status = 200, body = Identity), (status = 404), (status = 503)))]
async fn get_identity(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Identity>, ErrorResponse> { match database(&state)?.get_identity(id).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("identity not found")) } }

#[utoipa::path(put, path = "/api/identities/{id}", tag = "identity", params(("id" = Uuid, Path)), request_body = UpdateIdentity, responses((status = 200, body = Identity), (status = 404), (status = 503)))]
async fn update_identity(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateIdentity>) -> Result<Json<Identity>, ErrorResponse> { match database(&state)?.update_identity(id, input).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("identity not found")) } }

#[utoipa::path(get, path = "/api/identities/{id}/history", tag = "identity", params(("id" = Uuid, Path)), responses((status = 200, body = [IdentityObservation]), (status = 503)))]
async fn identity_history(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<IdentityObservation>>, ErrorResponse> { database(&state)?.identity_history(id, query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/fusion", tag = "fusion", responses((status = 200, body = [FusionResult]), (status = 503)))]
async fn list_fusion(State(state): State<Arc<AppState>>, Query(query): Query<MediaQuery>) -> Result<Json<Vec<FusionResult>>, ErrorResponse> { database(&state)?.list_fusion_results(query.camera_id, query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/model-assignments", tag = "fusion", responses((status = 200, body = [ModelAssignment]), (status = 503)))]
async fn list_model_assignments(State(state): State<Arc<AppState>>, Query(query): Query<CameraQuery>) -> Result<Json<Vec<ModelAssignment>>, ErrorResponse> { database(&state)?.list_model_assignments(query.camera_id).await.map(Json).map_err(internal_error) }

#[utoipa::path(post, path = "/api/model-assignments", tag = "fusion", request_body = CreateModelAssignment, responses((status = 201, body = ModelAssignment), (status = 400), (status = 503)))]
async fn create_model_assignment(State(state): State<Arc<AppState>>, Json(input): Json<CreateModelAssignment>) -> Result<(StatusCode, Json<ModelAssignment>), ErrorResponse> {
    if !(0.0..=1.0).contains(&input.confidence_threshold) || input.fps_limit.is_some_and(|value| value <= 0.0) { return Err(bad_request("confidence threshold must be 0..1 and fps_limit must be positive")); }
    if database(&state)?.get_camera(input.camera_id).await.map_err(internal_error)?.is_none() { return Err(not_found("camera not found")); }
    if database(&state)?.get_model(input.model_id).await.map_err(internal_error)?.is_none() { return Err(not_found("model not found")); }
    database(&state)?.create_model_assignment(input).await.map(|item| (StatusCode::CREATED, Json(item))).map_err(internal_error)
}

#[derive(Debug, serde::Deserialize)]
struct CameraQuery { camera_id: Option<Uuid> }

#[utoipa::path(get, path = "/api/analytics", tag = "analytics", responses((status = 200, body = AnalyticsSummary), (status = 503)))]
async fn analytics_summary(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<AnalyticsSummary>, ErrorResponse> { database(&state)?.analytics(filters.from, filters.to).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/analytics/cameras", tag = "analytics", responses((status = 200, body = AnalyticsSummary), (status = 503)))]
async fn analytics_cameras(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<AnalyticsSummary>, ErrorResponse> { database(&state)?.analytics(filters.from, filters.to).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/analytics/zones", tag = "analytics", responses((status = 200, body = AnalyticsSummary), (status = 503)))]
async fn analytics_zones(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<AnalyticsSummary>, ErrorResponse> { database(&state)?.analytics(filters.from, filters.to).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/analytics/models", tag = "analytics", responses((status = 200, body = AnalyticsSummary), (status = 503)))]
async fn analytics_models(State(state): State<Arc<AppState>>, Query(filters): Query<SearchFilters>) -> Result<Json<AnalyticsSummary>, ErrorResponse> { database(&state)?.analytics(filters.from, filters.to).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/recordings", tag = "recordings", responses((status = 200, body = [Recording]), (status = 503)))]
async fn list_recordings(State(state): State<Arc<AppState>>, Query(query): Query<MediaQuery>) -> Result<Json<Vec<Recording>>, ErrorResponse> { database(&state)?.list_recordings(query.camera_id, query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/recordings/{id}", tag = "recordings", params(("id" = Uuid, Path)), responses((status = 200, body = Recording), (status = 404), (status = 503)))]
async fn get_recording(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Recording>, ErrorResponse> { match database(&state)?.get_recording(id).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("recording not found")) } }

#[utoipa::path(get, path = "/api/clips", tag = "recordings", responses((status = 200, body = [Clip]), (status = 503)))]
async fn list_clips(State(state): State<Arc<AppState>>, Query(query): Query<MediaQuery>) -> Result<Json<Vec<Clip>>, ErrorResponse> { database(&state)?.list_clips(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/clips/{id}", tag = "recordings", params(("id" = Uuid, Path)), responses((status = 200, body = Clip), (status = 404), (status = 503)))]
async fn get_clip(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Clip>, ErrorResponse> { match database(&state)?.get_clip(id).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("clip not found")) } }

#[utoipa::path(post, path = "/api/clips/{id}/download", tag = "recordings", params(("id" = Uuid, Path)), responses((status = 200), (status = 404), (status = 410)))]
async fn download_clip(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Response, ErrorResponse> {
    let clip = database(&state)?.get_clip(id).await.map_err(internal_error)?.ok_or_else(|| not_found("clip not found"))?;
    media_response(&state.playback, &clip.clip_path, "attachment").await
}

#[utoipa::path(get, path = "/api/clips/{id}/media", tag = "recordings", params(("id" = Uuid, Path)), responses((status = 200), (status = 404), (status = 410)))]
async fn clip_media(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Response, ErrorResponse> {
    let clip = database(&state)?.get_clip(id).await.map_err(internal_error)?.ok_or_else(|| not_found("clip not found"))?;
    media_response(&state.playback, &clip.clip_path, "inline").await
}

#[utoipa::path(get, path = "/api/snapshots", tag = "recordings", responses((status = 200, body = [Snapshot]), (status = 503)))]
async fn list_snapshots(State(state): State<Arc<AppState>>, Query(query): Query<MediaQuery>) -> Result<Json<Vec<Snapshot>>, ErrorResponse> { database(&state)?.list_snapshots(query.camera_id, query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/snapshots/{id}", tag = "recordings", params(("id" = Uuid, Path)), responses((status = 200, body = Snapshot), (status = 404), (status = 503)))]
async fn get_snapshot(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Snapshot>, ErrorResponse> { match database(&state)?.get_snapshot(id).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("snapshot not found")) } }

#[utoipa::path(get, path = "/api/snapshots/{id}/media", tag = "recordings", params(("id" = Uuid, Path)), responses((status = 200), (status = 404), (status = 410)))]
async fn snapshot_media(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Response, ErrorResponse> {
    let snapshot = database(&state)?.get_snapshot(id).await.map_err(internal_error)?.ok_or_else(|| not_found("snapshot not found"))?;
    media_response(&state.playback, &snapshot.image_path, "inline").await
}

async fn media_response(playback: &PlaybackService, path: &str, disposition: &str) -> Result<Response, ErrorResponse> {
    let bytes = playback.read_file(path).await.map_err(|error| (StatusCode::GONE, Json(json!({"error": error.to_string()}))))?;
    Response::builder().status(StatusCode::OK).header(header::CONTENT_TYPE, PlaybackService::content_type(path)).header(header::CONTENT_DISPOSITION, format!("{disposition}; filename=\"{}\"", std::path::Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or("media"))).body(Body::from(bytes)).map_err(|error| internal_error(anyhow::anyhow!(error)))
}

#[derive(Debug, serde::Deserialize)]
struct MediaQuery { camera_id: Option<Uuid>, limit: Option<i64> }

#[utoipa::path(post, path = "/api/models/reload", tag = "models", responses((status = 200), (status = 400), (status = 503)))]
async fn reload_models(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ErrorResponse> {
    let pipeline = state.pipeline.as_ref().ok_or_else(|| service_unavailable("inference pipeline is not configured"))?;
    let models = database(&state)?.list_models().await.map_err(internal_error)?;
    let registry = ModelRegistry::new("/models", pipeline.models.clone());
    let loaded = registry.load_registered(&models).await.map_err(|error| bad_request(&error.to_string()))?;
    if let Some(default_model) = models.iter().find(|model| model.default_model && model.enabled) {
        pipeline.models.set_active(default_model.id).await.map_err(|error| bad_request(&error.to_string()))?;
    }
    Ok(Json(json!({"loaded": loaded})))
}

#[utoipa::path(get, path = "/api/detections", tag = "observations", responses((status = 200, body = [Detection]), (status = 503)))]
async fn list_detections(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<Detection>>, ErrorResponse> {
    database(&state)?.list_detections(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error)
}

#[utoipa::path(get, path = "/api/detections/{id}", tag = "observations", params(("id" = Uuid, Path)), responses((status = 200, body = Detection), (status = 404), (status = 503)))]
async fn get_detection(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Detection>, ErrorResponse> {
    match database(&state)?.get_detection(id).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("detection not found")) }
}

#[utoipa::path(get, path = "/api/tracks", tag = "observations", responses((status = 200, body = [Track]), (status = 503)))]
async fn list_tracks(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<Track>>, ErrorResponse> {
    database(&state)?.list_tracks(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error)
}

#[utoipa::path(get, path = "/api/tracks/{id}", tag = "observations", params(("id" = Uuid, Path)), responses((status = 200, body = Track), (status = 404), (status = 503)))]
async fn get_track(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Track>, ErrorResponse> {
    match database(&state)?.get_track(id).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("track not found")) }
}

#[utoipa::path(get, path = "/api/observations", tag = "observations", responses((status = 200, body = [Observation]), (status = 503)))]
async fn list_observations(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<Observation>>, ErrorResponse> {
    database(&state)?.list_observations(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error)
}

#[utoipa::path(get, path = "/api/observations/{id}", tag = "observations", params(("id" = Uuid, Path)), responses((status = 200, body = Observation), (status = 404), (status = 503)))]
async fn get_observation(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Observation>, ErrorResponse> {
    match database(&state)?.get_observation(id).await.map_err(internal_error)? { Some(item) => Ok(Json(item)), None => Err(not_found("observation not found")) }
}

#[utoipa::path(get, path = "/api/zones", tag = "zones", responses((status = 200, body = [Zone]), (status = 503)))]
async fn list_zones(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Zone>>, ErrorResponse> {
    database(&state)?.list_zones(None).await.map(Json).map_err(internal_error)
}

#[utoipa::path(post, path = "/api/zones", tag = "zones", request_body = CreateZone, responses((status = 201, body = Zone), (status = 400), (status = 503)))]
async fn create_zone(State(state): State<Arc<AppState>>, Json(input): Json<CreateZone>) -> Result<(StatusCode, Json<Zone>), ErrorResponse> {
    validate_polygon(&input.polygon_coordinates).map_err(|error| bad_request(&error.to_string()))?;
    if input.name.trim().is_empty() { return Err(bad_request("zone name cannot be empty")); }
    database(&state)?.create_zone(input).await.map(|zone| (StatusCode::CREATED, Json(zone))).map_err(internal_error)
}

#[utoipa::path(get, path = "/api/zones/{id}", tag = "zones", params(("id" = Uuid, Path)), responses((status = 200, body = Zone), (status = 404), (status = 503)))]
async fn get_zone(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Zone>, ErrorResponse> {
    match database(&state)?.get_zone(id).await.map_err(internal_error)? { Some(zone) => Ok(Json(zone)), None => Err(not_found("zone not found")) }
}

#[utoipa::path(put, path = "/api/zones/{id}", tag = "zones", params(("id" = Uuid, Path)), request_body = UpdateZone, responses((status = 200, body = Zone), (status = 400), (status = 404), (status = 503)))]
async fn update_zone(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateZone>) -> Result<Json<Zone>, ErrorResponse> {
    if let Some(polygon) = &input.polygon_coordinates { validate_polygon(polygon).map_err(|error| bad_request(&error.to_string()))?; }
    match database(&state)?.update_zone(id, input).await.map_err(internal_error)? { Some(zone) => Ok(Json(zone)), None => Err(not_found("zone not found")) }
}

#[utoipa::path(delete, path = "/api/zones/{id}", tag = "zones", params(("id" = Uuid, Path)), responses((status = 204), (status = 404), (status = 503)))]
async fn delete_zone(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> {
    if database(&state)?.delete_zone(id).await.map_err(internal_error)? { Ok(StatusCode::NO_CONTENT) } else { Err(not_found("zone not found")) }
}

#[utoipa::path(get, path = "/api/zone-events", tag = "zones", responses((status = 200, body = [ZoneEvent]), (status = 503)))]
async fn list_zone_events(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<ZoneEvent>>, ErrorResponse> {
    database(&state)?.list_zone_events(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error)
}

#[utoipa::path(get, path = "/api/rules", tag = "rules", responses((status = 200, body = [Rule]), (status = 503)))]
async fn list_rules(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Rule>>, ErrorResponse> { database(&state)?.list_rules().await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/rules/{id}", tag = "rules", params(("id" = Uuid, Path)), responses((status = 200, body = Rule), (status = 404), (status = 503)))]
async fn get_rule(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Rule>, ErrorResponse> { match database(&state)?.get_rule(id).await.map_err(internal_error)? { Some(rule) => Ok(Json(rule)), None => Err(not_found("rule not found")) } }

#[utoipa::path(post, path = "/api/rules", tag = "rules", request_body = CreateRule, responses((status = 201, body = Rule), (status = 400), (status = 503)))]
async fn create_rule(State(state): State<Arc<AppState>>, Json(input): Json<CreateRule>) -> Result<(StatusCode, Json<Rule>), ErrorResponse> {
    validate_rule(&input.name, input.cooldown_seconds, input.suppression_seconds)?;
    database(&state)?.create_rule(input).await.map(|rule| (StatusCode::CREATED, Json(rule))).map_err(internal_error)
}

#[utoipa::path(put, path = "/api/rules/{id}", tag = "rules", params(("id" = Uuid, Path)), request_body = UpdateRule, responses((status = 200, body = Rule), (status = 400), (status = 404), (status = 503)))]
async fn update_rule(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateRule>) -> Result<Json<Rule>, ErrorResponse> {
    if let Some(name) = &input.name { if name.trim().is_empty() { return Err(bad_request("rule name cannot be empty")); } }
    if input.cooldown_seconds.is_some_and(|value| value < 0) || input.suppression_seconds.is_some_and(|value| value < 0) { return Err(bad_request("cooldown and suppression cannot be negative")); }
    match database(&state)?.update_rule(id, input).await.map_err(internal_error)? { Some(rule) => Ok(Json(rule)), None => Err(not_found("rule not found")) }
}

#[utoipa::path(delete, path = "/api/rules/{id}", tag = "rules", params(("id" = Uuid, Path)), responses((status = 204), (status = 404), (status = 503)))]
async fn delete_rule(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> { if database(&state)?.delete_rule(id).await.map_err(internal_error)? { Ok(StatusCode::NO_CONTENT) } else { Err(not_found("rule not found")) } }

#[utoipa::path(get, path = "/api/events", tag = "events", responses((status = 200, body = [Event]), (status = 503)))]
async fn list_events(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<Event>>, ErrorResponse> { database(&state)?.list_events(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/events/{id}", tag = "events", params(("id" = Uuid, Path)), responses((status = 200, body = Event), (status = 404), (status = 503)))]
async fn get_event(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Event>, ErrorResponse> { match database(&state)?.get_event(id).await.map_err(internal_error)? { Some(event) => Ok(Json(event)), None => Err(not_found("event not found")) } }

#[utoipa::path(get, path = "/api/anomalies", tag = "intelligence", responses((status = 200, body = [AnomalyEvent]), (status = 503)))]
async fn list_anomalies(State(state): State<Arc<AppState>>, Query(query): Query<LimitQuery>) -> Result<Json<Vec<AnomalyEvent>>, ErrorResponse> { database(&state)?.list_anomalies(query.limit.unwrap_or(100)).await.map(Json).map_err(internal_error) }

#[utoipa::path(get, path = "/api/intelligence", tag = "intelligence", responses((status = 200, body = IntelligenceSummary), (status = 503)))]
async fn intelligence_summary(State(state): State<Arc<AppState>>) -> Result<Json<IntelligenceSummary>, ErrorResponse> {
    let database = database(&state)?;
    let identities = database.list_identities(100).await.map_err(internal_error)?;
    let anomalies = database.list_anomalies(100).await.map_err(internal_error)?;
    Ok(Json(IntelligenceSummary { identities, highest_priority: anomalies.iter().take(10).cloned().collect(), anomalies }))
}

fn validate_rule(name: &str, cooldown_seconds: i64, suppression_seconds: i64) -> Result<(), ErrorResponse> { if name.trim().is_empty() { return Err(bad_request("rule name cannot be empty")); } if cooldown_seconds < 0 || suppression_seconds < 0 { return Err(bad_request("cooldown and suppression cannot be negative")); } Ok(()) }

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

    fn test_state() -> AppState { AppState { database: None, camera_service: CameraService::default(), pipeline: None, recorder: Recorder::default(), playback: PlaybackService::new("/var/lib/objexel") } }

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
