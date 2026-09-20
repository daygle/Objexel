# Objexel

Objexel is a self-hosted, Linux-first AI camera platform for object detection, tracking, behaviour analysis, and event intelligence. It is designed for home servers and Proxmox deployments, with PostgreSQL persistence, ONNX Runtime inference, FFmpeg/RTSP media handling, and a SvelteKit web interface.

## Current stack

- **Backend:** Rust, Tokio, Axum, SQLx, Serde, tracing
- **Database:** PostgreSQL
- **AI:** ONNX Runtime with CPU/CUDA execution providers and YOLO-compatible ONNX models
- **Media:** FFmpeg, RTSP, recordings, clips, and snapshots
- **Web:** SvelteKit, TypeScript, and a dark operator dashboard
- **Deployment:** Docker Compose, with Debian/Ubuntu and Proxmox as primary targets

## Quick start

The recommended deployment is Docker Compose on Debian 13 or Ubuntu 24.04+.

```sh
mkdir -p /opt/objexel
cd /opt/objexel
git clone https://github.com/daygle/Objexel.git .
mkdir -p config models recordings clips snapshots backups
printf 'POSTGRES_PASSWORD=%s\nOBJEXEL_COOKIE_SECURE=1\n' "$(openssl rand -hex 24)" > .env
docker compose up -d --build
curl -fsS http://localhost:8080/liveness
```

Open `http://SERVER:8080/setup` to create the first administrator. The setup route is permanently disabled after the first account exists. Then sign in, add a camera, and configure a model.

See [INSTALL.md](INSTALL.md) for CPU-only, NVIDIA, Tesla P4, native Debian, and Proxmox guidance.

## Models

Objexel supports:

- local ONNX model registration and discovery
- model versions, input dimensions, and class-label metadata
- CPU and CUDA inference fallback
- model enable/disable and default activation
- verified catalog downloads with streamed progress
- SHA-256 verification before registration
- safe `tar.gz` and `zip` extraction with path traversal rejection
- benchmark results and per-camera model assignments

Catalog downloads are administrator-controlled and register disabled. Review, enable, benchmark, and activate models from **Models**. See [docs/model-management.md](docs/model-management.md).

## Updates

Updates are deliberately operator-controlled:

1. Check for a release in the dashboard or review GitHub release notes.
2. Back up PostgreSQL, configuration, and media metadata.
3. Pull the versioned image or check out the release tag.
4. Apply migrations through the normal API startup.
5. Verify health, login, one camera, one model, one detection, one event, and one clip.

Tagged releases publish versioned Docker images to GitHub Container Registry and generate release notes through GitHub Actions. See [UPGRADE.md](UPGRADE.md), [BACKUP.md](BACKUP.md), and [OPERATIONS.md](OPERATIONS.md).

## Authentication

Objexel uses local accounts with Argon2id password hashing, secure session cookies, CSRF tokens for state-changing user operations, and Administrator, Operator, and Viewer roles. The first-run setup creates the administrator; later user management is available to administrators at `/users`.

## API

The API exposes authenticated REST endpoints under `/api`, a generated OpenAPI document at `/api/openapi.json`, health endpoints at `/health`, `/liveness`, and `/readiness`, JSON metrics at `/metrics`, and a WebSocket event endpoint at `/api/v1/events`.

## Development

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cd web
npm install
npm run check
npm run build
```

CI runs Rust checks, SvelteKit checks/builds, and the API Docker build on pushes and pull requests. The project is still under active development; verify your actual camera, model, GPU, storage, and retention workload before calling a deployment production-ready.
