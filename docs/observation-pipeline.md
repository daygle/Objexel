# Phase 4 observation pipeline

Objexel keeps intelligence in one Rust process. There are no brokers or microservices between stages:

```text
camera frame -> FrameTensor -> objexel-detector -> objexel-tracker -> objexel-observations -> PostgreSQL
```

## Crates

- `objexel-detector` owns ONNX Runtime sessions, model validation, active-model selection, hot reload, CUDA registration, and CPU fallback. Models are expected to emit YOLO-style rows: `x, y, width, height, confidence, class_id`.
- `objexel-tracker` performs bounded greedy IoU association per object class, preserves UUID identities, caps movement history at 64 points, and expires tracks after a configurable age.
- `objexel-observations` turns tracks that exceed the configured presence duration into semantic observation records.
- `objexel-pipeline` composes the stages and persists detections, tracks, and observations through the existing SQLx database layer.

ONNX Runtime uses the CUDA execution provider when available and falls back to CPU execution when provider registration is unavailable. Model reload replaces a model session atomically in the manager and does not require a process restart.

## API

- `GET /api/models`
- `POST /api/models/reload`
- `GET /api/detections` and `/api/detections/{id}`
- `GET /api/tracks` and `/api/tracks/{id}`
- `GET /api/observations` and `/api/observations/{id}`

The generated OpenAPI document is available at `/api/openapi.json`.

## Operational boundaries

This phase deliberately excludes recording, notifications, and automation rules. The observation table is the stable input contract for those later phases.
