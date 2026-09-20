# Objexel Troubleshooting

## API will not start

Check `docker compose logs api` and `/readiness`. A missing or unreachable `DATABASE_URL` causes startup retries. Migration errors are fatal by design; correct the migration or database state and restart without deleting data.

## Cameras reconnect repeatedly

Check the camera network route, RTSP credentials, codec support, and `ffprobe` errors in logs. The monitor backs off up to 60 seconds. Test one camera at a time before increasing concurrency.

## No detections

Check that an enabled model exists, the model path is mounted, the model is valid ONNX, and the selected camera has an assignment or default model. Compare inference latency and CPU/GPU utilization rather than increasing frame rate first.

## Recordings stop

Inspect FFmpeg errors and disk usage. Confirm the storage directory is writable and has free space. Do not repeatedly restart a full disk; clean only files covered by the retention policy or restore from backup.

## Database unavailable

Verify the PostgreSQL healthcheck, credentials, and volume. The API readiness endpoint should return 503 while the database is unavailable. Do not expose PostgreSQL publicly in production.

## Safe evidence collection

Collect versions, redacted logs, `/metrics`, and health responses. Redact RTSP URLs, passwords, tokens, and provider configuration before sharing diagnostics.
