# End-to-End Validation - Live Detection Pipeline

## Overview

This document describes how to validate the complete detection pipeline from RTSP
camera through to email notification delivery. It includes test infrastructure
setup, scenario execution, and expected results.

---

## Prerequisites

| Requirement | Version | Purpose |
|---|---|---|
| Docker | 24+ | Run PostgreSQL and test RTSP server |
| Docker Compose | 2.20+ | Orchestrate test infrastructure |
| FFmpeg | 6.0+ | RTSP stream decode and test stream generation |
| Rust toolchain | 1.75+ | Build the application |
| An RTSP camera | Any H.264 | Real camera for live tests (or use test stream) |

---

## Test Infrastructure

### Docker Compose - `docker-compose.validation.yml`

The repository includes a runnable validation stack with PostgreSQL, MediaMTX, a synthetic RTSP camera, and Mailpit. Use that file directly; the abbreviated example below describes the services it provides.

```yaml
version: "3.9"
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: objexel
      POSTGRES_USER: objexel
      POSTGRES_PASSWORD: objexel
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U objexel"]
      interval: 2s
      retries: 10

  # Fake RTSP camera - generates a test pattern with a moving object
  rtsp-camera-1:
    image: alpine:3.19
    depends_on: []
    command: >
      sh -c "apk add --no-cache ffmpeg &&
             ffmpeg -re -f lavfi
             -i testsrc2=size=640x480:rate=15,
             drawbox=x='mod(t*40,640)':y=200:w=80:h=80:color=red:t=fill
             -c:v libx264 -preset ultrafast -tune zerolatency
             -f rtsp rtsp://0.0.0.0:8554/camera1"
    ports:
      - "8554:8554"

  rtsp-camera-2:
    image: alpine:3.19
    command: >
      sh -c "apk add --no-cache ffmpeg &&
             ffmpeg -re -f lavfi
             -i testsrc2=size=640x480:rate=15,
             drawbox=x='mod(t*30,640)':y=100:w=60:h=60:color=blue:t=fill
             -c:v libx264 -preset ultrafast -tune zerolatency
             -f rtsp rtsp://0.0.0.0:8554/camera2"
    ports:
      - "8555:8554"

  rtsp-camera-3:
    image: alpine:3.19
    command: >
      sh -c "apk add --no-cache ffmpeg &&
             ffmpeg -re -f lavfi
             -i testsrc2=size=640x480:rate=15,
             drawbox=x='mod(t*50,640)':y=300:w=100:h=100:color=green:t=fill
             -c:v libx264 -preset ultrafast -tune zerolatency
             -f rtsp rtsp://0.0.0.0:8554/camera3"
    ports:
      - "8556:8554"
```

### Start Infrastructure

```bash
docker compose -f docker-compose.validation.yml up -d
# Wait for PostgreSQL healthcheck to pass
docker compose -f docker-compose.validation.yml ps
sleep 5
```

---

## Scenario 1: Single Camera

### Setup

```bash
# Set environment
export DATABASE_URL="postgres://objexel:objexel@localhost:5432/objexel"
export OBJEXEL_INGEST_FPS=5
export RUST_LOG=info,objexel_pipeline=debug,objexel_camera=debug
export OBJEXEL_STORAGE_DIR="$PWD/.validation-storage"
export OBJEXEL_MODEL_DIR="$PWD/models"

# Build and run
cargo run --release -p objexel-api
```

### API Calls - Bootstrap Camera + Model + Rule

```bash
# 1. Create admin user
curl -s -X POST http://localhost:8080/api/auth/setup \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"test1234","role":"administrator"}' \
  -c cookies.txt

# 2. Add a camera
CAMERA=$(curl -s -X POST http://localhost:8080/api/cameras \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"Test Camera","rtsp_url":"rtsp://localhost:8554/camera1","enabled":true}')
echo "$CAMERA" | jq .
CAMERA_ID=$(echo "$CAMERA" | jq -r .id)

# 3. Test camera connection
curl -s -X POST "http://localhost:8080/api/cameras/$CAMERA_ID/test" \
  -b cookies.txt | jq .

# 4. List models (should have auto-discovered models if /models dir has .onnx files)
curl -s http://localhost:8080/api/models -b cookies.txt | jq .

# 5. Assign a model to the camera (replace MODEL_ID with actual model UUID)
MODEL_ID=$(curl -s http://localhost:8080/api/models -b cookies.txt | jq -r '.[0].id')
curl -s -X POST "http://localhost:8080/api/cameras/$CAMERA_ID/model/$MODEL_ID" \
  -b cookies.txt | jq .

# 6. Create a "cat" detection rule
RULE=$(curl -s -X POST http://localhost:8080/api/rules \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{
    "name": "Cat Detected",
    "enabled": true,
    "description": "Alert when a cat is detected",
    "cooldown_seconds": 60,
    "conditions": [{
      "object_class": "cat",
      "confidence_threshold": 0.25,
      "minimum_duration_ms": 3000
    }]
  }')
echo "$RULE" | jq .
RULE_ID=$(echo "$RULE" | jq -r .id)

# 7. Create an email notification provider
PROVIDER=$(curl -s -X POST http://localhost:8080/api/notification-providers \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{
    "provider_type": "email",
    "enabled": true,
    "configuration": {
      "host": "127.0.0.1",
      "port": 1025,
      "from": "alerts@example.com",
      "to": "you@example.com"
    }
  }')
echo "$PROVIDER" | jq .
PROVIDER_ID=$(echo "$PROVIDER" | jq -r .id)

# 8. Create a notification template
TEMPLATE=$(curl -s -X POST http://localhost:8080/api/notification-templates \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{
    "name": "Cat Alert",
    "subject": "Cat Detected!",
    "body": "A cat was detected by {{camera_name}} at {{timestamp}}.",
    "html": false
  }')
echo "$TEMPLATE" | jq .
TEMPLATE_ID=$(echo "$TEMPLATE" | jq -r .id)

# 9. Link action to rule (email action via the provider + template)
#    Note: This is done through the rule's action_ids or a separate action entity.
#    Create an action that sends email when the rule fires:
ACTION=$(curl -s -X POST http://localhost:8080/api/actions \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d "{
    \"name\": \"Email on Cat Detection\",
    \"action_type\": \"email\",
    \"provider_id\": \"$PROVIDER_ID\",
    \"template_id\": \"$TEMPLATE_ID\",
    \"enabled\": true,
    \"configuration\": {}
  }")
echo "$ACTION" | jq .
```

### Observe

```bash
# Watch the logs for pipeline activity
# Expected output within 30 seconds:
#   camera_id=<UUID> fps=5 live frame ingestor started
#   camera_id=<UUID> detections=1 live frame processed
#   camera_id=<UUID> detections=1 tracks=1 observations=0 zone_events=0 events=0 frame processed

# Check detections are being stored
curl -s "http://localhost:8080/api/detections?limit=5" -b cookies.txt | jq '.[].object_class'

# Check tracks
curl -s "http://localhost:8080/api/tracks?limit=5" -b cookies.txt | jq '.[].object_class'

# Check events (will appear once a cat is detected for 3+ seconds)
curl -s "http://localhost:8080/api/events?limit=5" -b cookies.txt | jq '.[].summary'

# Check action executions
curl -s "http://localhost:8080/api/action-executions?limit=5" -b cookies.txt | jq '.[].status'
```

### Expected Results - Single Camera

| Metric | Expected |
|---|---|
| Camera status | Online |
| Frames decoded | ~5/sec (OBJEXEL_INGEST_FPS) |
| Detections | 1+ per frame (if model detects objects) |
| Tracks | 1+ persistent tracks with growing `duration_ms` |
| Observations | Generated after track exceeds 3s minimum |
| Events | Created when rule conditions are met |
| Action execution | `status: "success"` |
| Email | Delivered to configured address |

### Logs to Capture

```bash
# Save full logs for the validation run
cargo run --release -p objexel-api 2>&1 | tee validation-scenario1.log

# Key log lines to look for:
grep "live frame ingestor started" validation-scenario1.log
grep "live frame processed" validation-scenario1.log | head -5
grep "frame processed" validation-scenario1.log | head -5
grep "action execution finished" validation-scenario1.log
```

---

## Scenario 2: Three Simultaneous Cameras

### Setup

```bash
# Start all three test RTSP streams
docker compose -f docker-compose.validation.yml up -d

# Add cameras 2 and 3
CAMERA2=$(curl -s -X POST http://localhost:8080/api/cameras \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"Test Camera 2","rtsp_url":"rtsp://localhost:8555/camera2","enabled":true}')
CAMERA2_ID=$(echo "$CAMERA2" | jq -r .id)

CAMERA3=$(curl -s -X POST http://localhost:8080/api/cameras \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"Test Camera 3","rtsp_url":"rtsp://localhost:8556/camera3","enabled":true}')
CAMERA3_ID=$(echo "$CAMERA3" | jq -r .id)

# Assign models to all cameras
curl -s -X POST "http://localhost:8080/api/cameras/$CAMERA2_ID/model/$MODEL_ID" -b cookies.txt
curl -s -X POST "http://localhost:8080/api/cameras/$CAMERA3_ID/model/$MODEL_ID" -b cookies.txt
```

### Observe

```bash
# Watch for all three cameras in logs
grep "live frame ingestor started" validation-scenario2.log
# Should show 3 entries with different camera_ids

# Verify per-camera track isolation
curl -s "http://localhost:8080/api/tracks?limit=20" -b cookies.txt | \
  jq 'group_by(.camera_id) | map({camera_id: .[0].camera_id, count: length})'

# Check metrics endpoint
curl -s http://localhost:8080/metrics -b cookies.txt | jq .
# Expected: cameras_total=3, cameras_online=3, active_tracks=3+
```

### Expected Results - Three Cameras

| Metric | Expected |
|---|---|
| Cameras online | 3 |
| Ingestors running | 3 (one per camera) |
| Total frames/sec | ~15 (3 cameras × 5 fps) |
| Tracks per camera | 1+ each (isolated, no cross-camera contamination) |
| Total active tracks | 3+ |
| No track leakage | Camera A tracks ≠ Camera B tracks |

---

## Validation Checklist

### Frame Processing

- [ ] FFmpeg subprocess starts for each enabled camera
- [ ] Frames are decoded at the configured FPS
- [ ] `FrameTensor` is created with correct shape `[1, 3, H, W]`
- [ ] Pixel values are normalized to `[0.0, 1.0]`
- [ ] `process_frame()` is called for every decoded frame

### Detection

- [ ] ONNX model loads successfully at startup
- [ ] `detect_with_model()` returns `Vec<Detection>` for each frame
- [ ] Detections have valid `bounding_box` coordinates
- [ ] Detections are persisted to database

### Tracking

- [ ] Per-camera tracker instances are created on first frame
- [ ] Same object class + overlapping bbox → same track ID
- [ ] Different cameras → different track IDs (even for same class)
- [ ] Track `duration_ms` increases over time
- [ ] Track `movement_path` accumulates bounding boxes
- [ ] Expired tracks are cleaned up after `max_age`

### Observations

- [ ] Tracks shorter than 3s → no observation
- [ ] Tracks ≥ 3s → observation generated with type "presence"
- [ ] Zone events generate additional observations

### Rules

- [ ] Rule with `object_class: "cat"` matches observations with that class
- [ ] `confidence_threshold` is respected
- [ ] `minimum_duration_ms` is respected
- [ ] Cooldown prevents duplicate events within `cooldown_seconds`
- [ ] Rule lock is released before DB operations (concurrency)

### Events

- [ ] Event is created with correct `rule_id`, `camera_id`, `track_id`
- [ ] Event is persisted to database
- [ ] Notification record is created

### Actions

- [ ] Action is looked up via `actions_for_rule()`
- [ ] Provider and template are resolved
- [ ] `ActionDispatcher::execute()` is called
- [ ] Execution record shows `status: "success"`
- [ ] Email is delivered to configured recipient

---

## Performance Metrics

### Measurement Commands

```bash
# Frames processed per second (from logs)
grep "live frame processed" validation.log | \
  awk '{print $1}' | sort | uniq -c | sort -rn | head

# Pipeline latency per frame (from debug logs)
grep "frame processed" validation.log | \
  grep -oP 'detections=\d+ tracks=\d+' | head -20

# Database write throughput
psql "$DATABASE_URL" -c "
  SELECT date_trunc('minute', created_at) AS minute,
         COUNT(*) AS detections
  FROM detections
  WHERE created_at > now() - interval '5 minutes'
  GROUP BY 1 ORDER BY 1 DESC;
"

# Track lifecycle
psql "$DATABASE_URL" -c "
  SELECT object_class, COUNT(*) AS count,
         AVG(duration_ms) AS avg_duration_ms
  FROM tracks
  GROUP BY object_class;
"
```

### Expected Performance (N100 / 4-core CPU)

| Metric | Single Camera | Three Cameras |
|---|---|---|
| Ingest FPS | 5 fps | 5 fps × 3 |
| Frame decode latency | ~5 ms | ~5 ms × 3 |
| ONNX inference (CPU) | ~200-400 ms | ~200-400 ms × 3 (parallel) |
| Tracker update | <1 ms | <1 ms × 3 (per-camera) |
| DB writes per frame | ~3-5 ms | ~3-5 ms × 3 |
| Total pipeline latency | ~250-450 ms | ~250-450 ms (parallel) |
| End-to-end (frame → email) | ~3-10 s | ~3-10 s |

### Expected Performance (GPU - CUDA)

| Metric | Single Camera | Three Cameras |
|---|---|---|
| ONNX inference (GPU) | ~20-50 ms | ~20-50 ms × 3 (parallel) |
| Total pipeline latency | ~30-60 ms | ~30-60 ms (parallel) |
| Max sustainable FPS | 10-15 fps | 5-10 fps × 3 |

---

## Troubleshooting

### Camera shows Offline

```bash
# Test RTSP connectivity manually
ffprobe -v error -rtsp_transport tcp -show_entries stream=width,height \
  rtsp://localhost:8554/camera1

# Check if FFmpeg is installed
which ffmpeg && ffmpeg -version | head -1
```

### No Detections Generated

```bash
# Check model is loaded
curl -s http://localhost:8080/api/models -b cookies.txt | jq '.[].name'

# Check model assignment
curl -s "http://localhost:8080/api/cameras/$CAMERA_ID" -b cookies.txt | jq '.active_model_id'

# Run a benchmark
curl -s -X POST "http://localhost:8080/api/models/$MODEL_ID/benchmark" -b cookies.txt | jq .
```

### No Events Created

```bash
# Check rules exist and are enabled
curl -s http://localhost:8080/api/rules -b cookies.txt | jq '.[].enabled'

# Check observations exist (prerequisite for rule evaluation)
curl -s http://localhost:8080/api/observations?limit=5 -b cookies.txt | jq .

# Check if track duration exceeds minimum_duration_ms
curl -s http://localhost:8080/api/tracks?limit=5 -b cookies.txt | \
  jq '.[].duration_ms'
```

### Email Not Delivered

```bash
# Check action executions for errors
curl -s http://localhost:8080/api/action-executions?limit=5 -b cookies.txt | \
  jq '.[] | {status, error_message}'

# Test notification manually (view messages at http://localhost:8025)
curl -s -X POST http://localhost:8080/api/test-notification \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d "{\"provider_id\":\"$PROVIDER_ID\"}" | jq .
```

---

## Cleanup

```bash
docker compose -f docker-compose.validation.yml down -v
rm -f cookies.txt validation-scenario1.log validation-scenario2.log
```
