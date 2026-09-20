# Objexel architecture

## Phase 1 boundary

Phase 1 establishes a Rust workspace with clear service boundaries, a PostgreSQL persistence layer, and an HTTP API that can be deployed on a Linux home server or Proxmox VM. The API starts in health-only mode when `DATABASE_URL` is absent, which keeps local development and container orchestration straightforward.

- `common`: serializable domain types shared by services.
- `database`: SQLx pool, embedded migrations, and camera persistence.
- `camera`: camera lifecycle boundary; RTSP and FFmpeg adapters will be added later.
- `api`: Axum HTTP boundary, readiness checks, camera endpoints, and a minimal OpenAPI document.

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
5. Treat camera credentials as secrets; Phase 2 will add secret references rather than returning RTSP URLs from public APIs.

## Roadmap

- **Phase 2:** camera registry CRUD, RTSP health worker, FFmpeg snapshots, and SvelteKit shell.
- **Phase 3:** ONNX Runtime detector with CPU/CUDA execution providers.
- **Phase 4:** tracker, zones, event rules, and WebSocket events.
- **Phase 5:** recordings, retention, notifications, authentication, and production deployment manifests.
