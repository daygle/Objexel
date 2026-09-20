# Objexel Operations

Objexel is designed to run as a single API process with PostgreSQL and FFmpeg. Use the supplied Compose deployment for a restartable baseline.

## Health and observability

- `/health` is a process-level health response.
- `/liveness` confirms the process can serve requests.
- `/readiness` (also `/ready`) verifies the database is reachable.
- `/metrics` exposes uptime, camera counts, track/detection counts, and database state as JSON.

Set `RUST_LOG=info` for normal operation. Use `RUST_LOG=objexel_api=debug,objexel_camera=debug` while diagnosing a camera, then return to `info`.

## Routine checks

```sh
docker compose ps
curl -fsS http://localhost:8080/liveness
curl -fsS http://localhost:8080/readiness
curl -fsS http://localhost:8080/metrics
```

Inspect container logs with `docker compose logs --tail=200 api`. Do not put passwords, RTSP URLs, or API keys in log messages or audit details.

## Recovery

The API waits for PostgreSQL at startup, applies migrations, and exits on migration failure rather than serving a partially upgraded schema. Compose restarts both services after a crash. Camera monitors use bounded exponential reconnect delays. A clean SIGINT/SIGTERM lets the HTTP server stop accepting work before exit.

If storage is exhausted, stop recording, preserve the database, free space according to [BACKUPS.md](BACKUPS.md), and restart the API. Never delete the PostgreSQL volume as a first response.

## Capacity validation

Before production, measure one, five, ten, and twenty-five cameras using the actual codecs, resolution, model, and storage policy. Record CPU, memory, inference latency, dropped streams, and disk growth for at least 30 minutes per scenario. Results are hardware and model dependent; do not treat a benchmark from another host as a capacity guarantee.
