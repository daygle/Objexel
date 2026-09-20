# Objexel

Objexel is a self-hosted, Linux-first AI camera platform for home servers and Proxmox. It combines a Rust/Axum backend, SvelteKit web interface, PostgreSQL, ONNX Runtime, YOLO-compatible ONNX models, RTSP/FFmpeg media handling, tracking, behaviour analysis, identity familiarity, rules, events, recordings, and notifications.

> **Status:** active development / controlled testing. The architecture is substantial, but production deployment still requires the verification checklist below.

## Stack

- **Backend:** Rust, Tokio, Axum, SQLx, Serde, tracing
- **Database:** PostgreSQL 16
- **AI:** ONNX Runtime with CUDA preference and CPU fallback; YOLO-compatible ONNX models
- **Frontend:** SvelteKit, TypeScript, Vite
- **Media:** RTSP, FFmpeg, continuous recordings, event clips, snapshots
- **Deployment:** Docker Compose, Debian 13, Ubuntu 24.04+, Proxmox VMs

## Current capabilities

- Camera registration, RTSP probing, snapshots, health monitoring, and reconnect backoff
- ONNX model discovery, registration, activation, per-camera assignment, reload, and benchmarking
- Detection, IoU tracking, observations, zones, behaviours, identities, anomaly scoring, and rules
- Events, searchable analytics, recordings, clips, snapshots, notifications, and WebSocket events
- Local users with Administrator, Operator, and Viewer roles
- Argon2id password hashing, revocable HttpOnly sessions, SameSite cookies, and CSRF checks for user administration
- Health, liveness, readiness, metrics, SQLx migrations, Docker restart policy, and graceful shutdown

## Quick start with Docker Compose

```bash
sudo apt update
sudo apt install -y ca-certificates curl git openssl
git clone https://github.com/daygle/Objexel.git
cd Objexel
mkdir -p models recordings clips snapshots backups
printf 'POSTGRES_PASSWORD=%s\nOBJEXEL_COOKIE_SECURE=0\n' "$(openssl rand -hex 24)" > .env
docker compose up -d --build
curl -fsS http://localhost:8080/liveness
```

For a network-exposed deployment, terminate HTTPS in a trusted reverse proxy and set `OBJEXEL_COOKIE_SECURE=1`. Do not expose PostgreSQL publicly.

## First run

1. Open `http://SERVER:8080/setup`.
2. Create the first Administrator account with a password of at least 12 characters.
3. Sign in at `http://SERVER:8080/login`.
4. Add and test a camera using its RTSP URL.
5. Copy ONNX files into `models/` (or `/models/yolov8`, `/models/yolov11`, or `/models/yolov26`).
6. Open **Models**, reload the registry, activate a model, and assign it to a camera.
7. Verify one detection, one event, and one recording clip before adding more cameras.

The setup endpoint rejects new setup attempts after the first user exists. Model files are currently installed manually; the application does not yet download models from an external registry.

## Useful endpoints

- `GET /health` — process health
- `GET /liveness` — container liveness
- `GET /readiness` and `GET /ready` — PostgreSQL readiness
- `GET /metrics` — runtime and camera metrics
- `GET /api/openapi.json` — generated API description
- `POST /api/auth/setup` — first Administrator creation
- `POST /api/auth/login` / `POST /api/auth/logout` / `GET /api/auth/me`
- `/api/cameras`, `/api/models`, `/api/detections`, `/api/tracks`, `/api/observations`, `/api/events`
- `/api/behaviours`, `/api/identities`, `/api/intelligence`, `/api/analytics`
- `/api/recordings`, `/api/clips`, `/api/snapshots`

All application API routes other than health, authentication entry points, and OpenAPI require an authenticated session.

## Repository layout

```text
crates/                 Rust workspace crates
  api/ auth/ camera/ database/ detector/ models/ ...
web/                    SvelteKit frontend
docker/                 API image definition
docker-compose.yml      PostgreSQL + API development deployment
migrations/             SQLx migrations
docs/                   Architecture and subsystem documentation
```

## Documentation

### Installation and operations

- [INSTALL.md](INSTALL.md)
- [OPERATIONS.md](OPERATIONS.md)
- [BACKUP.md](BACKUP.md)
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
- [UPGRADE.md](UPGRADE.md)

### Architecture

- [docs/architecture.md](docs/architecture.md)
- [docs/camera-ingestion.md](docs/camera-ingestion.md)
- [docs/model-management.md](docs/model-management.md)
- [docs/observation-pipeline.md](docs/observation-pipeline.md)
- [docs/behaviour.md](docs/behaviour.md)
- [docs/identity.md](docs/identity.md)
- [docs/rules-engine.md](docs/rules-engine.md)
- [docs/spatial-zones.md](docs/spatial-zones.md)

## Development

Required local tools are Rust stable, PostgreSQL 16, FFmpeg, Node.js, and npm.

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cd web
npm install
npm run check
npm run build
```

Pull requests are expected to pass the Rust, frontend, and Docker checks defined in `.github/workflows/ci.yml`.

## Production readiness checklist

Before calling an installation production-ready, verify:

- HTTPS and secure cookies are enabled.
- The first Administrator has been created and unused setup access is closed.
- PostgreSQL and media backups have been restored successfully in a test environment.
- At least one camera, model, detection, event, and clip have been tested.
- Disk retention and storage monitoring are configured.
- Upgrade and rollback procedures have been rehearsed.
- The deployment is restricted to trusted users and networks.

Objexel is not yet a packaged v1.0 release. Treat upgrades, model compatibility, GPU acceleration, and high camera counts as workload-specific until benchmarked on the target hardware.

## License

Apache-2.0
