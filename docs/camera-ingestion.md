# Camera ingestion API

Phase 3 uses the system `ffprobe` binary for RTSP metadata/health checks and `ffmpeg` for one-frame JPEG snapshots. Both commands run through Tokio's process API with bounded timeouts, so media operations never block the async executor.

## Endpoints

All endpoints are available under `/api/cameras`; the `/api/v1/cameras` aliases remain for compatibility.

- `GET /api/cameras` — list cameras and persisted health fields.
- `POST /api/cameras` — create a camera with `name`, `rtsp_url`, and optional `enabled`.
- `GET /api/cameras/{id}` — retrieve one camera.
- `PUT /api/cameras/{id}` — replace supplied camera fields; `PATCH` is also accepted.
- `DELETE /api/cameras/{id}` — remove a camera.
- `POST /api/cameras/{id}/test` — run an RTSP probe and return status, latency, and stream metadata.
- `POST /api/cameras/{id}/snapshot` — capture and return `image/jpeg` bytes.
- `GET /api/cameras/{id}/status` — retrieve persisted status and timestamps.
- `GET /api/openapi.json` — generated OpenAPI document.

## Health and reconnect behavior

The API starts a Tokio monitor task for each enabled camera after migrations complete. A failed probe marks the in-memory connection offline and retries with exponential backoff from two seconds up to sixty seconds. A successful probe resets the delay and resumes a thirty-second health interval. Explicit test and snapshot requests persist `status`, `last_connected_at`, `last_snapshot_at`, and `last_error` in PostgreSQL.

Camera RTSP URLs are never logged. Configure alternate binaries and timeouts by constructing `FfmpegConfig` in an embedding service; the default API uses `ffmpeg` and `ffprobe` from `PATH`.
