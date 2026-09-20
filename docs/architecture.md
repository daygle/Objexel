# Objexel architecture

## Current boundary

Objexel establishes a Rust workspace with clear service boundaries, a PostgreSQL persistence layer, and an HTTP API that can be deployed on a Linux home server or Proxmox VM. The API starts in health-only mode when `DATABASE_URL` is absent, which keeps local development and container orchestration straightforward. Camera ingestion uses Tokio-managed `ffprobe` and `ffmpeg` processes with timeout and reconnect backoff.

- `common`: serializable domain types shared by services.
- `database`: SQLx pool, embedded migrations, and camera persistence.
- `camera`: Tokio-based RTSP probe/snapshot service backed by system FFmpeg binaries, with bounded commands and reconnect backoff.
- `api`: Axum HTTP boundary, readiness checks, complete camera CRUD endpoints, generated OpenAPI documentation, and the event WebSocket boundary.

The API is intentionally stateless. PostgreSQL is the source of truth; future workers will communicate through Tokio channels and publish events over WebSockets without coupling media processing to HTTP handlers.

## Runtime topology

```text
RTSP cameras -> camera worker -> detector/tracker workers -> event engine
                                      |                         |
                                      v                         v
                               PostgreSQL + clips        notifications
                                      ^
                                      |
                             Axum REST/WebSocket API
```

## Operational principles

1. Keep blocking media and inference work off Tokio's async executor.
2. Bound queues and batch inference to protect low-memory hosts.
3. Store timestamps in UTC and use UUIDs for externally visible identifiers.
4. Run migrations at API startup in controlled deployments.
5. Treat camera credentials as secrets; RTSP URLs are persisted for the self-hosted deployment but are not written to structured logs.

## Roadmap

- **Phase 5:** zones, event rules, and richer WebSocket observation events.
- **Phase 6:** recordings, retention, notifications, authentication, and production deployment manifests.
