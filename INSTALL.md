# Objexel Installation

## Requirements

- Debian 13 (recommended), Ubuntu 24.04+, or a Proxmox VM
- 4 CPU cores and 8 GB RAM minimum; add RAM for more cameras/models
- PostgreSQL 16, FFmpeg, and Docker Engine with Compose v2
- SSD storage for the database and model files; separate storage for recordings

## Docker Compose (recommended)

```sh
sudo apt update
sudo apt install -y ca-certificates curl git openssl
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker "$USER"
mkdir -p /opt/objexel
cd /opt/objexel
git clone https://github.com/daygle/Objexel.git .
mkdir -p config models recordings clips snapshots backups
sudo chown -R "$USER":"$USER" /opt/objexel
printf 'POSTGRES_PASSWORD=%s\nOBJEXEL_COOKIE_SECURE=1\n' "$(openssl rand -hex 24)" > .env
docker compose up -d --build
curl -fsS http://localhost:8080/liveness
```

The Compose file starts PostgreSQL, waits for its health check, applies SQLx migrations, and starts the API. The generated `.env` keeps PostgreSQL off the host network and enables secure cookies for HTTPS deployments. If TLS is terminated elsewhere, keep the API on a private network and preserve `OBJEXEL_COOKIE_SECURE=1`.

## First run

1. Open `http://SERVER:8080/setup`.
2. Create the administrator with a unique username and a password of at least 12 characters.
3. Sign in at `/login`.
4. Add a camera under Cameras using its RTSP URL.
5. Place ONNX files under `/opt/objexel/models` and reload Models.
6. Assign a model to the camera and verify `/readiness` and `/metrics`.
7. Put the API behind HTTPS before allowing access outside the trusted LAN.

The setup endpoint is permanently disabled after the first user is created.

## CPU-only and NVIDIA

CPU-only deployments need no extra runtime flags. NVIDIA deployments require a working NVIDIA driver, NVIDIA Container Toolkit, and a CUDA-compatible ONNX Runtime build. Verify the host with `nvidia-smi`, then pass GPU access through your deployment environment and benchmark the model before enabling high camera counts. Tesla P4, RTX 3060, and Intel N100 performance depends on model size, stream resolution, and frame rate; measure the actual workload.

## Native Debian

Install Rust stable, PostgreSQL 16, FFmpeg, and a service manager such as systemd. Create `/opt/objexel/{config,models,recordings,clips,snapshots,backups}`, set `DATABASE_URL`, `OBJEXEL_STORAGE_DIR`, and `OBJEXEL_MODEL_DIR`, then run `cargo build --release -p objexel-api`. Run the binary under a dedicated unprivileged `objexel` user with restart-on-failure and `/health`/`/readiness` checks.

## Layout

```text
/opt/objexel/config       deployment configuration (no secrets in git)
/opt/objexel/models       ONNX models
/opt/objexel/recordings   continuous recordings
/opt/objexel/clips        event clips
/opt/objexel/snapshots    event snapshots
/opt/objexel/backups      database and configuration backups
```
