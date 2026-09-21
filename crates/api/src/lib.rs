use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{ws::Message, Path, Query, State, WebSocketUpgrade, Request},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use objexel_actions::ActionDispatcher;
use objexel_notifications::NotificationService;
use objexel_auth::{digest_token, generate_csrf_token, generate_token, hash_password, verify_password, AuthResponse, CreateUser, LoginRequest, Role, SessionUser, UpdateUser, User};
use objexel_analytics::AnalyticsSummary;
use objexel_common::{AnomalyEvent, Behaviour, BehaviourScore, CreateModelAssignment, IdentityScore, FusionResult, Identity, IdentityObservation, IdentityStatistics, IntelligenceSummary, ModelAssignment, ModelCatalogEntry, ModelDownload, UpdateIdentity, UpdateInfo, UpdateModelEnabled};
use objexel_camera::{CameraManager, CameraService};use objexel_common::{
    Action, ActionExecution, BenchmarkResult, Camera, CameraStatus, CameraTestResult, Clip,
 CreateAction, CreateCamera, CreateModel, CreateNotificationProvider, CreateNotificationTemplate, CreateRule, CreateZone, Detection, Event, HealthResponse, Model, Notification, NotificationProvider, NotificationTemplate, Observation, Recording, Rule, Snapshot, StreamMetadata, Track, UpdateAction, UpdateCamera, UpdateNotificationProvider, UpdateRule, UpdateZone, Zone, ZoneEvent};
use objexel_pipeline::{ObservationPipeline, PipelineResult};
use objexel_models::ModelRegistry;
use objexel_playback::PlaybackService;
use objexel_recorder::Recorder;
use objexel_search::{SearchFilters, SearchResult};
use objexel_zones::validate_polygon;
use objexel_database::Database;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};use std::{
    collections::HashSet,
    sync::{
atomic::{AtomicU64, Ordering}, Arc}, time::Instant};
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use uuid::Uuid;

#[async_trait]
pub trait CameraRuntime: Send + Sync {
    async fn apply(&self, camera: &Camera) -> AnyhowResult<()>;
    async fn remove(&self, camera_id: Uuid) -> AnyhowResult<()>;
}

#[derive(Clone)]
pub struct AppState {
    pub database: Option<Database>,
    pub camera_runtime: Option<Arc<dyn CameraRuntime>>,
    pub camera_service: CameraService,
    pub pipeline: Option<ObservationPipeline>,
    pub recorder: Recorder,
    pub playback: PlaybackService,
    /// Broadcast channel for the live-events WebSocket. Producers publish serialized
    /// pipeline results here; each `/api/v1/events` subscriber receives them.
    pub events: LiveEventSender,
}

/// Sender half of the live-events broadcast channel. Each subscriber (WebSocket client)
/// gets a receiver; messages are pre-serialized JSON envelopes.
pub type LiveEventSender = broadcast::Sender<String>;

/// Create a live-events broadcast channel with the given buffer capacity. Slow subscribers
/// that fall behind drop the oldest buffered messages (a `Lagged` skip) rather than block producers.
pub fn live_channel(capacity: usize) -> LiveEventSender {
    broadcast::channel(capacity).0
}

/// Publish the meaningful signals from a processed frame to live-events subscribers.
/// Raw detections are intentionally omitted — they are high-volume and would flood clients;
/// events, observations, and zone crossings are the actionable feed. When no client is
/// connected `send` returns an error, which is ignored.
pub fn broadcast_pipeline_result(sender: &LiveEventSender, result: &PipelineResult) {
    if sender.receiver_count() == 0 { return; }
    for event in &result.events {
        let _ = sender.send(json!({ "kind": "event", "data": event }).to_string());
    }
    for observation in &result.observations {
        let _ = sender.send(json!({ "kind": "observation", "data": observation }).to_string());
    }
    for zone_event in &result.zone_events {
        let _ = sender.send(json!({ "kind": "zone_event", "data": zone_event }).to_string());
    }
}

#[derive(Clone)]
pub struct RuntimeMetrics {
    started_at: Instant,
    requests: Arc<AtomicU64>,
}

static RUNTIME_METRICS: std::sync::OnceLock<RuntimeMetrics> = std::sync::OnceLock::new();

fn runtime_metrics() -> &'static RuntimeMetrics {
    RUNTIME_METRICS.get_or_init(RuntimeMetrics::default)
}

impl Default for RuntimeMetrics {
    fn default() -> Self { Self { started_at: Instant::now(), requests: Arc::new(AtomicU64::new(0)) } }
}

impl RuntimeMetrics {
    fn snapshot(&self) -> Value {
        json!({ "uptime_seconds": self.started_at.elapsed().as_secs(), "http_requests_total": self.requests.load(Ordering::Relaxed) })
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(health, ready, liveness, readiness, metrics, list_cameras, create_camera, get_camera, update_camera, delete_camera, test_camera, snapshot_camera, camera_status, assign_camera_model, list_models, list_model_catalog, import_model_catalog, refresh_model_catalog, download_model, get_model_download, get_model, create_model, set_model_enabled, delete_model, reload_models, activate_model, benchmark_model, list_benchmarks, list_actions, get_action, create_action, update_action, delete_action, list_executions, list_notifications, list_providers, create_provider, update_provider, validate_provider, list_templates, create_template, test_notification, global_search, search_events, search_observations, search_tracks, search_recordings, search_detections, search_behaviours, list_behaviours, get_behaviour, list_identities, get_identity, update_identity, identity_history, intelligence_summary, list_anomalies, list_fusion, list_model_assignments, create_model_assignment, analytics_summary, analytics_cameras, analytics_zones, analytics_models, list_recordings, get_recording, list_clips, get_clip, clip_media, download_clip, list_snapshots, get_snapshot, snapshot_media, list_detections, get_detection, list_tracks, get_track, list_observations, get_observation, list_zones, create_zone, get_zone, update_zone, delete_zone, list_zone_events, list_rules, get_rule, create_rule, update_rule, delete_rule, list_events, get_event),
    components(schemas(Camera, CreateCamera, CreateModel, UpdateCamera, CameraStatus, CameraTestResult, StreamMetadata, HealthResponse, Model, ModelCatalogEntry, ModelDownload, UpdateInfo, UpdateModelEnabled, BenchmarkResult, Action, ActionExecution, CreateAction, UpdateAction, Notification, NotificationProvider, NotificationTemplate, CreateNotificationProvider, UpdateNotificationProvider, CreateNotificationTemplate, Recording, Clip, Snapshot, SearchResult, AnalyticsSummary, Behaviour, Identity, IdentityObservation, IdentityStatistics, UpdateIdentity, IdentityScore, BehaviourScore, AnomalyEvent, IntelligenceSummary, ModelAssignment, CreateModelAssignment, FusionResult, Detection, Track, Observation, Zone, CreateZone, UpdateZone, ZoneEvent, Rule, CreateRule, UpdateRule, Event)),
    tags((name = "cameras", description = "Camera management and RTSP ingestion"))
)]
pub struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let camera_routes = Router::new()
        .route("/api/cameras", get(list_cameras).post(create_camera))
        .route("/api/cameras/{id}", get(get_camera).put(update_camera).patch(update_camera).delete(delete_camera))
        .route("/api/cameras/{id}/test", axum::routing::post(test_camera))
        .route("/api/cameras/{id}/snapshot", get(snapshot_camera).post(snapshot_camera))
        .route("/api/cameras/{id}/status", get(camera_status))
        .route("/api/cameras/{id}/model/{model_id}", axum::routing::post(assign_camera_model))
        .route("/api/v1/cameras", get(list_cameras).post(create_camera))
        .route("/api/v1/cameras/{id}", get(get_camera).put(update_camera).patch(update_camera).delete(delete_camera))
        .route("/api/v1/cameras/{id}/test", axum::routing::post(test_camera))
        .route("/api/v1/cameras/{id}/snapshot", get(snapshot_camera).post(snapshot_camera))
        .route("/api/v1/cameras/{id}/status", get(camera_status))
        .route("/api/v1/cameras/{id}/model/{model_id}", axum::routing::post(assign_camera_model));

    let shared_state = Arc::new(state);
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/readiness", get(readiness))
        .route("/liveness", get(liveness))
        .route("/metrics", get(metrics))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/setup", post(setup_admin))
        .route("/api/auth/me", get(me))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", axum::routing::put(update_user).delete(delete_user))
        .route("/api/openapi.json", get(openapi))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/api/v1/events", get(events_socket))
        .route("/api/onvif/discover", post(discover_onvif))
        .route("/api/onvif/profiles", post(onvif_profiles))
        .route("/api/models", get(list_models).post(create_model))
        .route("/api/models/catalog", get(list_model_catalog))
        .route("/api/models/catalog/import", post(import_model_catalog))
        .route("/api/models/catalog/refresh", post(refresh_model_catalog))
        .route("/api/models/catalog/{id}/download", axum::routing::post(download_model))
        .route("/api/models/downloads/{id}", get(get_model_download))
        .route("/api/updates", get(update_info))
        .route("/api/updates/check", post(check_update))
        .route("/api/models/{id}", get(get_model).delete(delete_model).patch(set_model_enabled))
        .route("/api/models/reload", axum::routing::post(reload_models))
        .route("/api/models/{id}/activate", axum::routing::post(activate_model))
        .route("/api/models/{id}/benchmark", axum::routing::post(benchmark_model))
        .route("/api/benchmarks", get(list_benchmarks))
        .route("/api/actions", get(list_actions).post(create_action))
        .route("/api/actions/{id}", get(get_action).put(update_action).delete(delete_action))
        .route("/api/action-executions", get(list_executions))
        .route("/api/notifications", get(list_notifications))
        .route("/api/notification-providers", get(list_providers).post(create_provider))
        .route("/api/notification-providers/{id}", axum::routing::put(update_provider))
        .route("/api/notification-providers/{id}/validate", post(validate_provider))
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
        .route("/api/behaviours/{id}", get(get_behaviour))
        .route("/api/identities", get(list_identities))
        .route("/api/identities/{id}", get(get_identity).put(update_identity))
        .route("/api/identities/{id}/history", get(identity_history))
        .route("/api/intelligence", get(intelligence_summary))
        .route("/api/anomalies", get(list_anomalies))
        .route("/api/fusion", get(list_fusion))
        .route("/api/model-assignments", get(list_model_assignments).post(create_model_assignment))
        .route("/api/analytics", get(analytics_summary))
        .route("/api/analytics/cameras", get(analytics_cameras))
        .route("/api/analytics/zones", get(analytics_zones))
        .route("/api/analytics/models", get(analytics_models))
        .route("/api/recordings", get(list_recordings))
        .route("/api/recordings/{id}", get(get_recording))
        .route("/api/clips", get(list_clips))
        .route("/api/clips/{id}", get(get_clip))
        .route("/api/clips/{id}/media", get(clip_media))
        .route("/api/clips/{id}/download", axum::routing::post(download_clip))
        .route("/api/snapshots", get(list_snapshots))
        .route("/api/snapshots/{id}", get(get_snapshot))
        .route("/api/snapshots/{id}/media", get(snapshot_media))
        .route("/api/detections", get(list_detections))
        .route("/api/detections/{id}", get(get_detection))
        .route("/api/tracks", get(list_tracks))
        .route("/api/tracks/{id}", get(get_track))
        .route("/api/observations", get(list_observations))
        .route("/api/observations/{id}", get(get_observation))
        .route("/api/zones", get(list_zones).post(create_zone))
        .route("/api/zones/{id}", get(get_zone).put(update_zone).delete(delete_zone))
        .route("/api/zone-events", get(list_zone_events))
        .route("/api/rules", get(list_rules).post(create_rule))
        .route("/api/rules/{id}", get(get_rule).put(update_rule).delete(delete_rule))
        .route("/api/events", get(list_events))
        .route("/api/events/{id}", get(get_event))
        .merge(camera_routes)
        .layer(middleware::from_fn_with_state(shared_state.clone(), require_session))
        .with_state(shared_state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

#[utoipa::path(get, path = "/health", responses((status = 200, body = HealthResponse)))]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".into(), service: "objexel-api".into(), version: env!("CARGO_PKG_VERSION").into() })
}

#[utoipa::path(get, path = "/liveness", responses((status = 200)))]
async fn liveness() -> Json<Value> { Json(json!({"status": "alive"})) }

#[utoipa::path(get, path = "/readiness", responses((status = 200), (status = 503)))]
async fn readiness(State(state): State<Arc<AppState>>) -> (StatusCode, Json<Value>) { ready(State(state)).await }

#[utoipa::path(get, path = "/metrics", responses((status = 200)))]
async fn metrics(State(state): State<Arc<AppState>>) -> Json<Value> {
    let mut value = runtime_metrics().snapshot();
    if let Some(database) = &state.database {
        let cameras = database.list_cameras().await.unwrap_or_default();
        let tracks = database.list_tracks(10_000).await.unwrap_or_default();
        let detections = database.list_detections(10_000).await.unwrap_or_default();
        value["cameras_total"] = json!(cameras.len());
        value["cameras_online"] = json!(cameras.iter().filter(|camera| camera.status == CameraStatus::Online).count());
        value["active_tracks"] = json!(tracks.len());
        value["recent_detections"] = json!(detections.len());
        value["database"] = json!("ok");
    } else { value["database"] = json!("not_configured"); }
    if let Some(pipeline) = &state.pipeline {
        value["pipeline_cameras"] = json!(pipeline.metrics_snapshot().await.into_iter().map(|metrics| json!({
            "camera_id": metrics.camera_id,
            "frames_processed": metrics.frames_processed,
            "frames_in_flight": metrics.frames_in_flight,
            "processing_errors": metrics.processing_errors,
            "total_processing_ms": metrics.total_processing_ms,
            "last_processing_ms": metrics.last_processing_ms,
        })).collect::<Vec<_>>());
    }
    Json(value)
}

const SESSION_COOKIE: &str = "objexel_session";
// Readable (non-HttpOnly) companion cookie holding the CSRF token, so the SPA can
// echo it back in the x-csrf-token header for the double-submit check. It shares the
// session cookie's lifetime, which lets a valid session recover the token in a fresh
// tab (sessionStorage does not survive across tabs).
const CSRF_COOKIE: &str = "objexel_csrf";
const SESSION_DAYS: i64 = 7;

fn public_path(path: &str) -> bool {
    matches!(path, "/health" | "/ready" | "/readiness" | "/liveness" | "/api/auth/login" | "/api/auth/setup" | "/api/openapi.json" | "/api/v1/openapi.json")
}

fn is_write_method(method: &Method) -> bool {
    *method == Method::POST || *method == Method::PUT || *method == Method::PATCH || *method == Method::DELETE
}

/// Mutating requests that any authenticated user may make regardless of role
/// (still CSRF-protected): signing out, and downloading a clip (a read served over POST).
fn write_role_exempt(path: &str) -> bool {
    path == "/api/auth/logout" || (path.starts_with("/api/clips/") && path.ends_with("/download"))
}

fn cookie_secure_suffix() -> &'static str {
    let secure = std::env::var("OBJEXEL_COOKIE_SECURE").map(|value| value != "0").unwrap_or(false);
    if secure { "; Secure" } else { "" }
}

fn auth_failure(status: StatusCode, message: &str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"error": message}).to_string()))
        .expect("valid authentication response")
}

async fn require_session(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    if public_path(request.uri().path()) { return next.run(request).await; }
    let database = match state.database.as_ref() {
        Some(database) => database,
        None => return auth_failure(StatusCode::SERVICE_UNAVAILABLE, "database is not configured"),
    };
    let token = match cookie_value(request.headers(), SESSION_COOKIE) {
        Some(token) => token,
        None => return auth_failure(StatusCode::UNAUTHORIZED, "authentication required"),
    };
    match database.session_user(&digest_token(&token)).await {
        Ok(Some(session)) => {
            if is_write_method(request.method()) {
                let supplied = request.headers().get("x-csrf-token").and_then(|value| value.to_str().ok());
                if !supplied.is_some_and(|token| digest_token(token) == session.csrf_token_hash) {
                    return auth_failure(StatusCode::FORBIDDEN, "invalid or missing CSRF token");
                }
                if !session.user.role.can_write() && !write_role_exempt(request.uri().path()) {
                    return auth_failure(StatusCode::FORBIDDEN, "write access requires the operator or administrator role");
                }
            }
            let _ = database.update_session_seen(session.session_id).await;
            next.run(request).await
        }
        Ok(None) => auth_failure(StatusCode::UNAUTHORIZED, "invalid or expired session"),
        Err(error) => {
            tracing::error!(%error, "session validation failed");
            auth_failure(StatusCode::INTERNAL_SERVER_ERROR, "session validation failed")
        }
    }
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(header::COOKIE)?.to_str().ok()?.split(';').find_map(|part| { let (key, value) = part.trim().split_once('=')?; (key == name).then(|| value.to_owned()) })
}

async fn authenticated(state: &Arc<AppState>, headers: &HeaderMap) -> Result<SessionUser, ErrorResponse> {
    let token = cookie_value(headers, SESSION_COOKIE).ok_or_else(|| bad_request("authentication required"))?;
    let session = database(state)?.session_user(&digest_token(&token)).await.map_err(internal_error)?.ok_or_else(|| bad_request("invalid or expired session"))?;
    let _ = database(state)?.update_session_seen(session.session_id).await;
    Ok(session)
}

fn require_admin(session: &SessionUser) -> Result<(), ErrorResponse> { if session.user.role.can_manage_users() { Ok(()) } else { Err(bad_request("administrator role required")) } }

fn require_csrf(session: &SessionUser, headers: &HeaderMap) -> Result<(), ErrorResponse> {
    let token = headers.get("x-csrf-token").and_then(|value| value.to_str().ok()).ok_or_else(|| bad_request("CSRF token required"))?;
    if digest_token(token) == session.csrf_token_hash { Ok(()) } else { Err(bad_request("invalid CSRF token")) }
}

fn session_response(auth: AuthResponse, token: &str) -> Result<(HeaderMap, Json<AuthResponse>), ErrorResponse> {
    let mut headers = HeaderMap::new();
    let secure_flag = cookie_secure_suffix();
    let max_age = SESSION_DAYS * 86_400;
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure_flag}")).map_err(|_| internal_error(anyhow::anyhow!("invalid session cookie")))?);
    headers.append(header::SET_COOKIE, HeaderValue::from_str(&format!("{CSRF_COOKIE}={}; Path=/; SameSite=Strict; Max-Age={max_age}{secure_flag}", auth.csrf_token)).map_err(|_| internal_error(anyhow::anyhow!("invalid csrf cookie")))?);
    Ok((headers, Json(auth)))
}

fn cleared_auth_cookies() -> Result<HeaderMap, ErrorResponse> {
    let mut headers = HeaderMap::new();
    let secure_flag = cookie_secure_suffix();
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure_flag}")).map_err(|_| internal_error(anyhow::anyhow!("invalid session cookie")))?);
    headers.append(header::SET_COOKIE, HeaderValue::from_str(&format!("{CSRF_COOKIE}=; Path=/; SameSite=Strict; Max-Age=0{secure_flag}")).map_err(|_| internal_error(anyhow::anyhow!("invalid csrf cookie")))?);
    Ok(headers)
}

async fn login(State(state): State<Arc<AppState>>, Json(input): Json<LoginRequest>) -> Result<(HeaderMap, Json<AuthResponse>), ErrorResponse> {
    let database = database(&state)?;
    let (user, password_hash) = database.get_user_credentials(&input.username).await.map_err(internal_error)?.ok_or_else(|| bad_request("invalid username or password"))?;
    if !user.enabled || !verify_password(&input.password, &password_hash) { return Err(bad_request("invalid username or password")); }
    let token = generate_token(); let csrf = generate_csrf_token();
    database.insert_session(Uuid::new_v4(), user.id, &digest_token(&token), &digest_token(&csrf), Utc::now() + chrono::Duration::days(SESSION_DAYS)).await.map_err(internal_error)?;
    let _ = database.audit(Some(user.id), "auth.login", "user", Some(user.id), json!({})).await;
    session_response(AuthResponse { user, csrf_token: csrf }, &token)
}

async fn setup_admin(State(state): State<Arc<AppState>>, Json(input): Json<CreateUser>) -> Result<(HeaderMap, Json<AuthResponse>), ErrorResponse> {
    let database = database(&state)?;
    if database.user_count().await.map_err(internal_error)? != 0 { return Err(bad_request("initial setup is already complete")); }
    if input.role != Role::Administrator { return Err(bad_request("first user must be an administrator")); }
    let password_hash = hash_password(&input.password).map_err(|error| bad_request(&error.to_string()))?;
    let user = database.create_user(Uuid::new_v4(), &input.username, input.email.as_deref(), &password_hash, Role::Administrator).await.map_err(internal_error)?;
    let token = generate_token(); let csrf = generate_csrf_token();
    database.insert_session(Uuid::new_v4(), user.id, &digest_token(&token), &digest_token(&csrf), Utc::now() + chrono::Duration::days(SESSION_DAYS)).await.map_err(internal_error)?;
    let _ = database.audit(Some(user.id), "auth.setup", "user", Some(user.id), json!({})).await;
    session_response(AuthResponse { user, csrf_token: csrf }, &token)
}

async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<(HeaderMap, StatusCode), ErrorResponse> {
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE) {
        if let Some(database) = &state.database {
            let token_hash = digest_token(&token);
            if let Some(session) = database.session_user(&token_hash).await.map_err(internal_error)? {
                require_csrf(&session, &headers)?;
                database.delete_session(&token_hash).await.map_err(internal_error)?;
                let _ = database.audit(Some(session.user.id), "auth.logout", "user", Some(session.user.id), json!({})).await;
            }
        }
    }
    Ok((cleared_auth_cookies()?, StatusCode::NO_CONTENT))
}

async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<User>, ErrorResponse> { Ok(Json(authenticated(&state, &headers).await?.user)) }

async fn list_users(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Vec<User>>, ErrorResponse> { let session = authenticated(&state, &headers).await?; require_admin(&session)?; database(&state)?.list_users().await.map(Json).map_err(internal_error) }

async fn create_user(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(input): Json<CreateUser>) -> Result<(StatusCode, Json<User>), ErrorResponse> {
    let session = authenticated(&state, &headers).await?; require_admin(&session)?; require_csrf(&session, &headers)?;
    let hash = hash_password(&input.password).map_err(|error| bad_request(&error.to_string()))?;
    let database = database(&state)?; let user = database.create_user(Uuid::new_v4(), &input.username, input.email.as_deref(), &hash, input.role).await.map_err(internal_error)?;
    database.audit(Some(session.user.id), "user.created", "user", Some(user.id), json!({})).await.map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_user(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Uuid>, Json(input): Json<UpdateUser>) -> Result<Json<User>, ErrorResponse> {
    let session = authenticated(&state, &headers).await?; require_admin(&session)?; require_csrf(&session, &headers)?;
    let hash = input.password.as_deref().map(hash_password).transpose().map_err(|error| bad_request(&error.to_string()))?;
    let database = database(&state)?; let user = database.update_user(id, Some(input.email.as_deref()), hash.as_deref(), input.role, input.enabled).await.map_err(internal_error)?.ok_or_else(|| not_found("user not found"))?;
    database.audit(Some(session.user.id), "user.updated", "user", Some(id), json!({})).await.map_err(internal_error)?;
    Ok(Json(user))
}

async fn delete_user(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> {
    let session = authenticated(&state, &headers).await?; require_admin(&session)?; require_csrf(&session, &headers)?; if id == session.user.id { return Err(bad_request("cannot delete the current administrator")); }
    let database = database(&state)?; if !database.delete_user(id).await.map_err(internal_error)? { return Err(not_found("user not found")); }
    database.audit(Some(session.user.id), "user.deleted", "user", Some(id), json!({})).await.map_err(internal_error)?; Ok(StatusCode::NO_CONTENT)
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct OnvifProfilesRequest {
    device_service_url: String,
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct OnvifProfileSummary {
    token: String,
    name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    rtsp_uri: Option<String>,
}

#[utoipa::path(post, path = "/api/onvif/discover", tag = "cameras", responses((status = 200), (status = 503)))]
async fn discover_onvif() -> Result<Json<Value>, ErrorResponse> {
    let devices = oxvif::discovery::probe(std::time::Duration::from_secs(3)).await;
    Ok(Json(json!({ "devices": devices })))
}

#[utoipa::path(post, path = "/api/onvif/profiles", tag = "cameras", request_body = OnvifProfilesRequest, responses((status = 200), (status = 400), (status = 502)))]
async fn onvif_profiles(Json(input): Json<OnvifProfilesRequest>) -> Result<Json<Value>, ErrorResponse> {
    if input.device_service_url.trim().is_empty() || input.username.trim().is_empty() {
        return Err(bad_request("device_service_url and username are required"));
    }
    let session = oxvif::OnvifSession::builder(input.device_service_url.trim())
        .with_credentials(input.username.trim(), &input.password)
        .with_clock_sync()
        .build()
        .await
        .map_err(|error| bad_gateway(format!("ONVIF connection failed: {error}")))?;
    let profiles = session.get_profiles().await
        .map_err(|error| bad_gateway(format!("ONVIF profile query failed: {error}")))?;
    let mut result = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let uri = session.get_stream_uri(&profile.token).await.ok().map(|stream| stream.uri);
        result.push(OnvifProfileSummary {
            token: profile.token,
            name: Some(profile.name),
            width: None,
            height: None,
            rtsp_uri: uri,
        });
    }
    Ok(Json(json!({ "profiles": result })))
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
    let camera = database.create_camera(input).await.map_err(internal_error)?;
    if let Some(runtime) = &state.camera_runtime {
        runtime.apply(&camera).await.map_err(internal_error)?;
    }
    database.audit(None, "camera.created", "camera", Some(camera.id), json!({})).await.map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(camera)))
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
    let database = database(&state)?;
    match database.update_camera(id, input).await.map_err(internal_error)? {
        Some(camera) => {
            if let Some(runtime) = &state.camera_runtime {
                runtime.apply(&camera).await.map_err(internal_error)?;
            }
            database.audit(None, "camera.updated", "camera", Some(id), json!({})).await.map_err(internal_error)?;
            Ok(Json(camera))
        },
        None => Err(not_found("camera not found")),
    }
}

#[utoipa::path(delete, path = "/api/cameras/{id}", tag = "cameras", params(("id" = Uuid, Path, description = "Camera identifier")), responses((status = 204), (status = 404), (status = 503)))]
async fn delete_camera(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> {
    let database = database(&state)?;
    if database.delete_camera(id).await.map_err(internal_error)? {
        if let Some(runtime) = &state.camera_runtime {
            runtime.remove(id).await.map_err(internal_error)?;
        }
        database.audit(None, "camera.deleted", "camera", Some(id), json!({})).await.map_err(internal_error)?;
        Ok(StatusCode::NO_CONTENT)
    } else { Err(not_found("camera not found")) }
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

#[utoipa::path(get, path = "/api/updates", tag = "updates", responses((status = 200, body = UpdateInfo), (status = 503)))]
async fn update_info(State(state): State<Arc<AppState>>) -> Result<Json<UpdateInfo>, ErrorResponse> { let current=env!("CARGO_PKG_VERSION").to_string(); let mut info=database(&state)?.update_info().await.map_err(internal_error)?.unwrap_or(UpdateInfo{current_version:current.clone(),latest_version:None,update_available:false,release_url:None,notes:None}); info.current_version=current.clone(); info.update_available=info.latest_version.as_ref().is_some_and(|v| v.trim_start_matches('v') != current.trim_start_matches('v')); Ok(Json(info)) }

#[utoipa::path(post, path = "/api/updates/check", tag = "updates", responses((status = 200, body = UpdateInfo), (status = 502), (status = 503)))]
async fn check_update(State(state): State<Arc<AppState>>) -> Result<Json<UpdateInfo>, ErrorResponse> { let repo=std::env::var("OBJEXEL_UPDATE_REPO").unwrap_or_else(|_| "daygle/Objexel".into()); let endpoint=format!("https://api.github.com/repos/{repo}/releases/latest"); let response=reqwest::Client::new().get(&endpoint).header("user-agent","objexel").send().await.map_err(|e| bad_gateway(&e.to_string()))?.error_for_status().map_err(|e| bad_gateway(&e.to_string()))?; let release: Value=response.json().await.map_err(|e| bad_gateway(&e.to_string()))?; let tag=release.get("tag_name").and_then(Value::as_str).ok_or_else(|| bad_gateway("release response did not contain tag_name"))?; let url=release.get("html_url").and_then(Value::as_str); let notes=release.get("body").and_then(Value::as_str); database(&state)?.save_update_info(tag,url,notes).await.map_err(internal_error)?; update_info(State(state)).await }

#[utoipa::path(get, path = "/api/models/catalog", tag = "models", responses((status = 200, body = [ModelCatalogEntry]), (status = 503)))]
async fn list_model_catalog(State(state): State<Arc<AppState>>) -> Result<Json<Vec<ModelCatalogEntry>>, ErrorResponse> { database(&state)?.list_model_catalog().await.map(Json).map_err(internal_error) }

#[utoipa::path(post, path = "/api/models/catalog/import", tag = "models", request_body = [ModelCatalogEntry], responses((status = 200), (status = 400), (status = 503)))]
async fn import_model_catalog(State(state): State<Arc<AppState>>, Json(entries): Json<Vec<ModelCatalogEntry>>) -> Result<Json<Value>, ErrorResponse> {
    if entries.is_empty() || entries.len() > 100 { return Err(bad_request("catalog import must contain between 1 and 100 entries")); }
    let mut ids = HashSet::new();
    for entry in &entries {
        validate_model_catalog_entry(entry).map_err(|error| bad_request(&error))?;
        if !ids.insert(&entry.id) { return Err(bad_request("catalog import contains duplicate ids")); }
    }
    let database = database(&state)?;
    for entry in &entries { database.upsert_model_catalog(entry).await.map_err(internal_error)?; }
    Ok(Json(json!({ "imported": entries.len() })))
}

#[utoipa::path(post, path = "/api/models/catalog/refresh", tag = "models", responses((status = 200), (status = 400), (status = 502), (status = 503)))]
async fn refresh_model_catalog(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ErrorResponse> {
    let database = database(&state)?;
    let url = std::env::var("OBJEXEL_MODEL_CATALOG_URL").unwrap_or_else(|_| DEFAULT_CATALOG_URL.to_string());
    let response = reqwest::Client::new().get(&url).send().await.map_err(|error| bad_gateway(format!("could not fetch catalog: {error}")))?;
    if !response.status().is_success() { return Err(bad_gateway(format!("catalog source returned status {}", response.status()))); }
    let body = response.text().await.map_err(|error| bad_gateway(format!("could not read catalog: {error}")))?;
    let entries: Vec<ModelCatalogEntry> = serde_json::from_str(&body).map_err(|error| bad_request(&format!("catalog is not valid JSON: {error}")))?;
    if entries.is_empty() || entries.len() > 100 { return Err(bad_request("catalog must contain between 1 and 100 entries")); }
    let mut ids = HashSet::new();
    for entry in &entries {
        validate_model_catalog_entry(entry).map_err(|error| bad_request(&error))?;
        if !ids.insert(&entry.id) { return Err(bad_request("catalog contains duplicate ids")); }
    }
    for entry in &entries { database.upsert_model_catalog(entry).await.map_err(internal_error)?; }
    Ok(Json(json!({ "imported": entries.len(), "source": url })))
}

fn coco80_labels() -> Vec<String> {
    [
        "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat", "traffic light",
        "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat", "dog", "horse", "sheep", "cow",
        "elephant", "bear", "zebra", "giraffe", "backpack", "umbrella", "handbag", "tie", "suitcase", "frisbee",
        "skis", "snowboard", "sports ball", "kite", "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket", "bottle",
        "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple", "sandwich", "orange",
        "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch", "potted plant", "bed",
        "dining table", "toilet", "tv", "laptop", "mouse", "remote", "keyboard", "cell phone", "microwave", "oven",
        "toaster", "sink", "refrigerator", "book", "clock", "vase", "scissors", "teddy bear", "hair drier", "toothbrush",
    ].into_iter().map(str::to_owned).collect()
}

/// Default location of the hosted catalog manifest used by the "Refresh catalog"
/// action. Editing `models/catalog.json` on the default branch (and uploading any
/// new ONNX assets to the matching release) publishes model updates to every
/// deployment without shipping a new Objexel build. Override with
/// `OBJEXEL_MODEL_CATALOG_URL` to track a different manifest.
pub const DEFAULT_CATALOG_URL: &str = "https://raw.githubusercontent.com/daygle/Objexel/main/models/catalog.json";

/// The catalog shipped with Objexel, embedded at build time from `models/catalog.json`.
/// Seeded on startup so the Models page offers one-click, checksum-verified YOLO11 and
/// YOLO26 downloads without any configuration. `models/catalog.json` is the single source
/// of truth: the same file is served over HTTP for the runtime refresh path.
pub fn default_model_catalog() -> Vec<ModelCatalogEntry> {
    const EMBEDDED: &str = include_str!("../../../models/catalog.json");
    serde_json::from_str(EMBEDDED).unwrap_or_default()
}

pub fn validate_model_catalog_entry(entry: &ModelCatalogEntry) -> Result<(), String> {
    if entry.id.trim().is_empty() || entry.name.trim().is_empty() || entry.version.trim().is_empty() || entry.model_type.trim().is_empty() { return Err("catalog id, name, version, and model_type are required".into()); }
    let url = reqwest::Url::parse(&entry.download_url).map_err(|_| "download_url must be a valid URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") { return Err("download_url must use http or https".into()); }
    if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) { return Err("sha256 must be a 64-character hexadecimal digest".into()); }
    if entry.input_width == 0 || entry.input_width > 8192 || entry.input_height == 0 || entry.input_height > 8192 { return Err("model dimensions must be between 1 and 8192".into()); }
    if entry.class_list.len() > 10_000 || entry.class_list.iter().any(|label| label.trim().is_empty()) { return Err("class_list contains an invalid label".into()); }
    Ok(())
}

#[utoipa::path(post, path = "/api/models/catalog/{id}/download", tag = "models", params(("id" = String, Path)), responses((status = 202, body = ModelDownload), (status = 404), (status = 400)))]
async fn download_model(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<(StatusCode, Json<ModelDownload>), ErrorResponse> {
    let database = database(&state)?; let entry = database.get_model_catalog(&id).await.map_err(internal_error)?.ok_or_else(|| not_found("catalog entry not found"))?; let download = database.create_model_download(&id).await.map_err(internal_error)?; let database_clone = database.clone(); let root = std::env::var("OBJEXEL_MODEL_DIR").unwrap_or_else(|_| "/models".into());
    let _ = database.update_model_download(download.id, "downloading", 0, 0, None, None, None).await;
    tokio::spawn(async move {
        let registry = ModelRegistry::new(root, objexel_detector::ModelManager::default());
        let progress_database = database_clone.clone();
        let progress_id = download.id;
        let result = registry.download_catalog_entry_with_progress(&entry, move |progress, bytes, total| {
            let database = progress_database.clone();
            tokio::spawn(async move { let _ = database.update_model_download(progress_id, "downloading", progress, bytes, total, None, None).await; });
        }).await;
        match result { Ok(path) => { let input = CreateModel { name: entry.name, version: entry.version, model_type: entry.model_type.clone(), path: path.to_string_lossy().into_owned(), input_width: entry.input_width, input_height: entry.input_height, class_list: if entry.class_list.is_empty() && matches!(entry.model_type.as_str(), "coco" | "yolo-coco") { coco80_labels() } else { entry.class_list }, enabled: false, default_model: false }; let _ = database_clone.update_model_download(download.id, "downloading", 100, 0, None, None, None).await; match database_clone.create_model(input).await { Ok(model) => { let _=database_clone.update_model_download(download.id,"completed",100,0,None,None,Some(model.id)).await; }, Err(error) => { let _=database_clone.update_model_download(download.id,"failed",0,0,None,Some(&error.to_string()),None).await; } } }, Err(error) => { let message=error.to_string(); let _=database_clone.update_model_download(download.id,"failed",0,0,None,Some(&message),None).await; } }
    });
    Ok((StatusCode::ACCEPTED, Json(download)))
}

#[utoipa::path(get, path = "/api/models/downloads/{id}", tag = "models", params(("id" = Uuid, Path)), responses((status = 200, body = ModelDownload), (status = 404)))]
async fn get_model_download(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<ModelDownload>, ErrorResponse> { database(&state)?.get_model_download(id).await.map_err(internal_error)?.map(Json).ok_or_else(|| not_found("download not found")) }

#[utoipa::path(get, path = "/api/models", tag = "models", responses((status = 200, body = [Model]), (status = 503)))]
async fn list_models(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Model>>, ErrorResponse> {
    database(&state)?.list_models().await.map(Json).map_err(internal_error)
}

#[utoipa::path(get, path = "/api/models/{id}", tag = "models", params(("id" = Uuid, Path)), responses((status = 200, body = Model), (status = 404), (status = 503)))]
async fn get_model(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Model>, ErrorResponse> { match database(&state)?.get_model(id).await.map_err(internal_error)? { Some(model) => Ok(Json(model)), None => Err(not_found("model not found")) } }

#[utoipa::path(post, path = "/api/models", tag = "models", request_body = CreateModel, responses((status = 201, body = Model), (status = 400), (status = 503)))]
async fn create_model(State(state): State<Arc<AppState>>, Json(input): Json<CreateModel>) -> Result<(StatusCode, Json<Model>), ErrorResponse> {
    let mut input = input;
    if input.class_list.is_empty() && matches!(input.model_type.as_str(), "coco" | "yolo-coco") { input.class_list = coco80_labels(); }
    let model_root = std::env::var("OBJEXEL_MODEL_DIR").unwrap_or_else(|_| "/models".into());
    ModelRegistry::validated_path(std::path::Path::new(&model_root), input.path.as_ref(), input.input_width, input.input_height)
        .map_err(|error| bad_request(&error.to_string()))?;
    let database = database(&state)?;
    let model = database.create_model(input).await.map_err(internal_error)?;
    database.audit(None, "model.created", "model", Some(model.id), json!({})).await.map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(model)))
}

#[utoipa::path(patch, path = "/api/models/{id}", tag = "models", params(("id" = Uuid, Path)), request_body = UpdateModelEnabled, responses((status = 200, body = Model), (status = 404), (status = 503)))]
async fn set_model_enabled(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateModelEnabled>) -> Result<Json<Model>, ErrorResponse> { database(&state)?.set_model_enabled(id, input.enabled).await.map_err(internal_error)?.map(Json).ok_or_else(|| not_found("model not found")) }

#[utoipa::path(delete, path = "/api/models/{id}", tag = "models", params(("id" = Uuid, Path)), responses((status = 204), (status = 404), (status = 503)))]
async fn delete_model(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> {
    let database = database(&state)?;
    if database.delete_model(id).await.map_err(internal_error)? {
        database.audit(None, "model.deleted", "model", Some(id), json!({})).await.map_err(internal_error)?;
        Ok(StatusCode::NO_CONTENT)
    } else { Err(not_found("model not found")) }
}

#[utoipa::path(post, path = "/api/models/{id}/activate", tag = "models", params(("id" = Uuid, Path)), responses((status = 200), (status = 404), (status = 503)))]
async fn activate_model(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Value>, ErrorResponse> { if database(&state)?.activate_model(id).await.map_err(internal_error)? { if let Some(pipeline) = &state.pipeline { let _ = pipeline.models.set_active(id).await; } Ok(Json(json!({"active": id}))) } else { Err(not_found("model not found or disabled")) } }

#[utoipa::path(post, path = "/api/models/{id}/benchmark", tag = "models", params(("id" = Uuid, Path)), responses((status = 200, body = BenchmarkResult), (status = 404), (status = 503)))]
async fn benchmark_model(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<BenchmarkResult>, ErrorResponse> {
    let model = database(&state)?.get_model(id).await.map_err(internal_error)?.ok_or_else(|| not_found("model not found"))?;
    let pipeline = state.pipeline.as_ref().ok_or_else(|| service_unavailable("inference pipeline is not configured"))?;
    let model_root = std::env::var("OBJEXEL_MODEL_DIR").unwrap_or_else(|_| "/models".into());
    let registry = ModelRegistry::new(model_root, pipeline.models.clone());
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
async fn create_provider(State(state): State<Arc<AppState>>, Json(input): Json<CreateNotificationProvider>) -> Result<(StatusCode, Json<NotificationProvider>), ErrorResponse> {
    let candidate = NotificationProvider { id: Uuid::nil(), provider_type: input.provider_type.clone(), enabled: input.enabled, configuration: input.configuration.clone(), created_at: Utc::now() };
    NotificationService::validate_provider(&candidate).map_err(|error| bad_request(&error.to_string()))?;
    database(&state)?.create_provider(input).await.map(|provider| (StatusCode::CREATED, Json(provider))).map_err(internal_error)
}

#[utoipa::path(put, path = "/api/notification-providers/{id}", tag = "notifications", params(("id" = Uuid, Path)), request_body = UpdateNotificationProvider, responses((status = 200, body = NotificationProvider), (status = 404), (status = 503)))]
async fn update_provider(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateNotificationProvider>) -> Result<Json<NotificationProvider>, ErrorResponse> {
    let database = database(&state)?;
    let current = database.provider(id).await.map_err(internal_error)?.ok_or_else(|| not_found("provider not found"))?;
    let candidate = NotificationProvider { configuration: input.configuration.clone().unwrap_or(current.configuration), enabled: input.enabled.unwrap_or(current.enabled), ..current };
    NotificationService::validate_provider(&candidate).map_err(|error| bad_request(&error.to_string()))?;
    database.update_provider(id, input).await.map_err(internal_error)?.map(Json).ok_or_else(|| not_found("provider not found"))
}

#[utoipa::path(post, path = "/api/notification-providers/{id}/validate", tag = "notifications", params(("id" = Uuid, Path)), responses((status = 200), (status = 400), (status = 404)))]
async fn validate_provider(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Value>, ErrorResponse> {
    let provider = database(&state)?.provider(id).await.map_err(internal_error)?.ok_or_else(|| not_found("provider not found"))?;
    NotificationService::validate_provider(&provider).map_err(|error| bad_request(&error.to_string()))?;
    Ok(Json(json!({ "status": "valid", "provider_type": provider.provider_type })))
}

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
    let model_root = std::env::var("OBJEXEL_MODEL_DIR").unwrap_or_else(|_| "/models".into());
    let registry = ModelRegistry::new(model_root, pipeline.models.clone());
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
    let database = database(&state)?;
    let rule = database.create_rule(input).await.map_err(internal_error)?;
    database.audit(None, "rule.created", "rule", Some(rule.id), json!({})).await.map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(rule)))
}

#[utoipa::path(put, path = "/api/rules/{id}", tag = "rules", params(("id" = Uuid, Path)), request_body = UpdateRule, responses((status = 200, body = Rule), (status = 400), (status = 404), (status = 503)))]
async fn update_rule(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(input): Json<UpdateRule>) -> Result<Json<Rule>, ErrorResponse> {
    if let Some(name) = &input.name { if name.trim().is_empty() { return Err(bad_request("rule name cannot be empty")); } }
    if input.cooldown_seconds.is_some_and(|value| value < 0) || input.suppression_seconds.is_some_and(|value| value < 0) { return Err(bad_request("cooldown and suppression cannot be negative")); }
    let database = database(&state)?;
    match database.update_rule(id, input).await.map_err(internal_error)? {
        Some(rule) => {
            database.audit(None, "rule.updated", "rule", Some(id), json!({})).await.map_err(internal_error)?;
            Ok(Json(rule))
        }
        None => Err(not_found("rule not found")),
    }
}

#[utoipa::path(delete, path = "/api/rules/{id}", tag = "rules", params(("id" = Uuid, Path)), responses((status = 204), (status = 404), (status = 503)))]
async fn delete_rule(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<StatusCode, ErrorResponse> {
    let database = database(&state)?;
    if database.delete_rule(id).await.map_err(internal_error)? {
        database.audit(None, "rule.deleted", "rule", Some(id), json!({})).await.map_err(internal_error)?;
        Ok(StatusCode::NO_CONTENT)
    } else { Err(not_found("rule not found")) }
}

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

async fn events_socket(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl axum::response::IntoResponse {
    let mut receiver = state.events.subscribe();
    ws.on_upgrade(move |mut socket| async move {
        tracing::debug!("live event websocket connected");
        if socket.send(Message::Text("{\"kind\":\"connected\"}".to_string().into())).await.is_err() { return; }
        loop {
            tokio::select! {
                broadcasted = receiver.recv() => match broadcasted {
                    Ok(payload) => { if socket.send(Message::Text(payload.into())).await.is_err() { break; } }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => { tracing::debug!(skipped, "live event subscriber lagged"); }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                incoming = socket.recv() => match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                },
            }
        }
        tracing::debug!("live event websocket disconnected");
    })
}
async fn openapi() -> Json<utoipa::openapi::OpenApi> { Json(ApiDoc::openapi()) }

type ErrorResponse = (StatusCode, Json<Value>);

fn database(state: &AppState) -> Result<&Database, ErrorResponse> { state.database.as_ref().ok_or_else(|| service_unavailable("database is not configured")) }
fn validate_camera_input(name: &str, rtsp_url: &str) -> Result<(), ErrorResponse> { if name.trim().is_empty() || rtsp_url.trim().is_empty() { Err(bad_request("name and rtsp_url are required")) } else { Ok(()) } }
fn bad_request(message: &str) -> ErrorResponse { (StatusCode::BAD_REQUEST, Json(json!({"error": message}))) }
fn bad_gateway(message: impl Into<String>) -> ErrorResponse { (StatusCode::BAD_GATEWAY, Json(json!({"error": message.into()}))) }
fn not_found(message: &str) -> ErrorResponse { (StatusCode::NOT_FOUND, Json(json!({"error": message}))) }
fn service_unavailable(message: &str) -> ErrorResponse { (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": message}))) }
fn internal_error(error: anyhow::Error) -> ErrorResponse { tracing::error!(%error, "request failed"); (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"}))) }

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt;

    fn test_state() -> AppState { AppState { database: None, camera_runtime: None, camera_service: CameraService::default(), pipeline: None, recorder: Recorder::default(), playback: PlaybackService::new("/var/lib/objexel"), events: live_channel(16) } }

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

    #[test]
    fn only_mutating_methods_require_write_guards() {
        assert!(is_write_method(&Method::POST));
        assert!(is_write_method(&Method::PUT));
        assert!(is_write_method(&Method::PATCH));
        assert!(is_write_method(&Method::DELETE));
        assert!(!is_write_method(&Method::GET));
        assert!(!is_write_method(&Method::HEAD));
    }

    #[tokio::test]
    async fn broadcast_delivers_observations_to_subscribers() {
        let sender = live_channel(16);
        let mut receiver = sender.subscribe();
        let result = PipelineResult {
            detections: vec![], tracks: vec![],
            observations: vec![Observation { id: Uuid::new_v4(), camera_id: Uuid::new_v4(), track_id: Uuid::new_v4(), observation_type: "zone_entered".into(), summary: "person entered yard".into(), created_at: Utc::now() }],
            zone_events: vec![], events: vec![],
        };
        broadcast_pipeline_result(&sender, &result);
        let message = receiver.recv().await.expect("subscriber receives the observation");
        assert!(message.contains("\"kind\":\"observation\""));
        assert!(message.contains("person entered yard"));
    }

    #[test]
    fn write_role_exemptions_cover_logout_and_clip_downloads() {
        assert!(write_role_exempt("/api/auth/logout"));
        assert!(write_role_exempt("/api/clips/2f1c6b0e-0000-0000-0000-000000000000/download"));
        // Creating or deleting resources is never exempt from the write-role check.
        assert!(!write_role_exempt("/api/cameras"));
        assert!(!write_role_exempt("/api/clips/2f1c6b0e-0000-0000-0000-000000000000"));
        assert!(!write_role_exempt("/api/models/2f1c6b0e-0000-0000-0000-000000000000/activate"));
    }
}
