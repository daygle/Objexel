# Objexel

Self-hosted, Linux-first AI camera analytics for home servers and Proxmox. Objexel is being built incrementally as a Rust backend with a SvelteKit frontend, ONNX Runtime inference, RTSP/FFmpeg media handling, and PostgreSQL persistence.

## Phase 7

Objexel now supports model-agnostic inference profiles with ONNX model registration, discovery under `/models/yolov8`, `/models/yolov11`, and `/models/yolov26`, per-camera model assignments, hot reload, activation, and benchmark results. See [`docs/model-management.md`](docs/model-management.md).

The backend now includes a single-process intelligence pipeline: ONNX Runtime detection with CUDA/CPU fallback, persistent IoU-based tracking, zone awareness, observation generation, rules, event generation, PostgreSQL storage, generated OpenAPI documentation, and SvelteKit views for observations, tracks, detections, zones, rules, and events. Events are the final output of this phase.

## API

This repository currently contains the production-oriented foundation:

- Rust Cargo workspace with `common`, `database`, `camera`, and `api` crates.
- SQLx PostgreSQL connection pool and embedded migrations.
- Camera schema plus core schemas for models, detections, events, recordings, notification rules, and accounts.
- Axum health/readiness endpoints and initial camera REST endpoints.
- Docker Compose environment for PostgreSQL and the API.
- Architecture documentation in [`docs/architecture.md`](docs/architecture.md).

## Run locally

```bash
# Start PostgreSQL, then run the API with DATABASE_URL configured in your shell:
export POSTGRES_PASSWORD='replace-with-a-long-random-password'
docker compose up -d postgres
DATABASE_URL="postgres://objexel:${POSTGRES_PASSWORD}@localhost:5432/objexel" cargo run -p objexel-api
```

The API listens on `0.0.0.0:8080` by default. `GET /health` does not require a database; `GET /ready` reports PostgreSQL readiness. SQLx applies all files in `migrations/` at startup. The REST surface is:

- `GET /health`
- `GET /ready`
- `GET /api/cameras`
- `POST /api/cameras`
- `GET /api/cameras/:id`
- `PUT /api/cameras/:id`
- `DELETE /api/cameras/:id`
- `POST /api/cameras/:id/test`
- `POST /api/cameras/:id/snapshot`
- `GET /api/cameras/:id/status`
- `GET /api/openapi.json` (generated with `utoipa`)
- `GET /api/models`
- `GET /api/models/:id`
- `POST /api/models`
- `DELETE /api/models/:id`
- `POST /api/models/reload`
- `POST /api/models/:id/activate`
- `POST /api/models/:id/benchmark`
- `GET /api/benchmarks`
- `POST /api/cameras/:id/model/:model_id`
- `GET /api/detections` and `GET /api/detections/:id`
- `GET /api/tracks` and `GET /api/tracks/:id`
- `GET /api/observations` and `GET /api/observations/:id`
- `GET /api/zones`
- `GET /api/zones/:id`
- `POST /api/zones`
- `PUT /api/zones/:id`
- `DELETE /api/zones/:id`
- `GET /api/zone-events`
- `GET /api/rules`
- `GET /api/rules/:id`
- `POST /api/rules`
- `PUT /api/rules/:id`
- `DELETE /api/rules/:id`
- `GET /api/events`
- `GET /api/events/:id`

The `/api/v1/cameras` routes remain available as compatibility aliases. See [`docs/observation-pipeline.md`](docs/observation-pipeline.md), [`docs/spatial-zones.md`](docs/spatial-zones.md), and [`docs/rules-engine.md`](docs/rules-engine.md). Notifications, recordings, and automation actions are intentionally deferred.

Run checks with:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace
```

## Production setup

See [`INSTALL.md`](INSTALL.md) for Debian 13, Ubuntu 24.04+, Proxmox VM, Docker Compose, CPU-only, and NVIDIA deployment instructions. On first start, open `/setup` to create the administrator account; subsequent setup attempts are rejected once a user exists. Authenticate through `/login`.

Authentication uses Argon2id password hashes and revocable, HttpOnly, SameSite session cookies. Administrator user changes require the CSRF token returned by login/setup in the `X-CSRF-Token` header. Set `OBJEXEL_COOKIE_SECURE=1` behind HTTPS; local plain-HTTP development can leave it unset.

Operational procedures are documented in [`OPERATIONS.md`](OPERATIONS.md), [`BACKUP.md`](BACKUP.md), [`TROUBLESHOOTING.md`](TROUBLESHOOTING.md), and [`UPGRADE.md`](UPGRADE.md). The Compose deployment keeps PostgreSQL on the internal Compose network; expose it only through an explicit, protected operator override.

## License

Apache-2.0
