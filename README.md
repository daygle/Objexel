# Objexel

Self-hosted, Linux-first AI camera analytics for home servers and Proxmox. Objexel is being built incrementally as a Rust backend with a SvelteKit frontend, ONNX Runtime inference, RTSP/FFmpeg media handling, and PostgreSQL persistence.

## Phase 4

The backend now includes a single-process frame-to-observation pipeline: ONNX Runtime detection with CUDA/CPU fallback, persistent IoU-based tracking, observation generation, PostgreSQL storage, generated OpenAPI documentation, and SvelteKit views for observations, tracks, and detections.

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
docker compose up -d postgres
DATABASE_URL=postgres://objexel:objexel@localhost:5432/objexel cargo run -p objexel-api
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
- `POST /api/models/reload`
- `GET /api/detections` and `GET /api/detections/:id`
- `GET /api/tracks` and `GET /api/tracks/:id`
- `GET /api/observations` and `GET /api/observations/:id`
- `GET /api/zones`
- `GET /api/zones/:id`
- `POST /api/zones`
- `PUT /api/zones/:id`
- `DELETE /api/zones/:id`
- `GET /api/zone-events`

The `/api/v1/cameras` routes remain available as compatibility aliases. See [`docs/observation-pipeline.md`](docs/observation-pipeline.md) and [`docs/spatial-zones.md`](docs/spatial-zones.md) for pipeline and spatial architecture.

Run checks with:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace
```

## License

Apache-2.0
