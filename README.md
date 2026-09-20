# Objexel

Self-hosted, Linux-first AI camera analytics for home servers and Proxmox. Objexel is being built incrementally as a Rust backend with a SvelteKit frontend, ONNX Runtime inference, RTSP/FFmpeg media handling, and PostgreSQL persistence.

## Phase 1

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
- `GET /api/v1/cameras`
- `POST /api/v1/cameras`
- `GET /api/v1/cameras/:id`
- `PATCH /api/v1/cameras/:id`
- `DELETE /api/v1/cameras/:id`
- `GET /api/v1/openapi.json` (generated with `utoipa`)

Run checks with:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace
```

## License

Apache-2.0
