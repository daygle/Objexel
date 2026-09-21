# Objexel Installation

## Requirements

- Debian 13 (recommended), Ubuntu 24.04+, or a Proxmox VM
- 4 CPU cores and 8 GB RAM minimum; add RAM for more cameras/models
- Docker Engine with the Compose v2 plugin (installation steps below) for the recommended deployment
- SSD storage for the database and model files; separate storage for recordings

With the Docker Compose deployment, PostgreSQL and FFmpeg run inside containers, so
you do not install them on the host. They are only required for a native install
(see [Native Debian](#native-debian)).

## Install Docker Engine

Use Docker's official apt repository (recommended for production). The commands below
are for Debian; on Ubuntu, replace `debian` with `ubuntu` in the GPG key and
repository URLs.

```sh
# 1. Remove any distro-provided packages that conflict with Docker's
for pkg in docker.io docker-doc docker-compose podman-docker containerd runc; do
  sudo apt-get remove -y "$pkg" 2>/dev/null || true
done

# 2. Install prerequisites and add Docker's official GPG key
sudo apt-get update
sudo apt-get install -y ca-certificates curl git openssl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc

# 3. Add the Docker apt repository for this release
echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian \
  $(. /etc/os-release && echo "${VERSION_CODENAME}") stable" | \
  sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
sudo apt-get update

# 4. Install Docker Engine, the CLI, containerd, and the Buildx + Compose plugins
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

# 5. Allow your user to run Docker without sudo (log out and back in to apply)
sudo usermod -aG docker "$USER"

# 6. Verify the installation
docker --version
docker compose version
```

For a quick, non-production host you can instead use Docker's convenience script:
`curl -fsSL https://get.docker.com | sudo sh`.

## Deploy Objexel with Docker Compose (recommended)

```sh
sudo mkdir -p /opt/objexel
sudo chown -R "$USER":"$USER" /opt/objexel
cd /opt/objexel
if [ -d .git ]; then
  git pull --autostash
else
  git clone https://github.com/daygle/Objexel.git .
fi
mkdir -p config models recordings clips snapshots backups
printf 'POSTGRES_PASSWORD=%s\nOBJEXEL_COOKIE_SECURE=0\n' "$(openssl rand -hex 24)" > .env
docker compose up -d --build
curl -fsS http://localhost:8080/liveness
```

The Compose file starts PostgreSQL, waits for its health check, applies SQLx migrations, and starts the API. The API container also serves the built web UI and its static assets on port 8080. The generated `.env` keeps PostgreSQL off the host network. The default `OBJEXEL_COOKIE_SECURE=0` works with plain HTTP on your LAN. **When you put the API behind HTTPS, set `OBJEXEL_COOKIE_SECURE=1` in `.env` and redeploy so browsers send the session cookie over the encrypted connection.**

## First run

1. Open `http://SERVER:8080/setup`.
2. Create the administrator with a unique username and a password of at least 12 characters.
3. Sign in at `/login`.
4. Add a camera under Cameras using its RTSP URL.
5. Place ONNX files under `/opt/objexel/models` and reload Models.
6. Assign a model to the camera and verify `/readiness` and `/metrics`.
7. Put the API behind HTTPS before allowing access outside the trusted LAN, and set `OBJEXEL_COOKIE_SECURE=1` in `.env`.

The setup endpoint is permanently disabled after the first user is created.

## CPU-only and NVIDIA

The default Compose build compiles an ONNX Runtime CPU binary that runs on any x86-64 host without special runtime flags.

NVIDIA GPU deployments require a working NVIDIA driver and NVIDIA Container Toolkit on the host (`nvidia-smi`). Build the API with CUDA enabled by adding the GPU override file, then benchmark the model before enabling high camera counts:

```sh
docker compose -f docker-compose.yml -f docker-compose.gpu.yml up -d --build
```

The override passes `--features cuda` to the build (compiling a CUDA-enabled ONNX Runtime) and exposes the GPU through `deploy.resources`. Machines building without the override get the CPU build, so one repo serves both host types. Tesla P4, RTX 3060, and Intel N100 performance depends on model size, stream resolution, and frame rate; measure the actual workload.

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
