# Phase 7 model management

Objexel now treats model loading as a registry concern rather than tying the detector to one YOLO release. `objexel-models` discovers ONNX files under `/models/yolov8`, `/models/yolov11`, and `/models/yolov26`, registers new files in PostgreSQL at startup, validates extensions and dimensions, and delegates graph integrity validation to ONNX Runtime. The embedded `migrations/0001_initial.sql` migration creates the model registry, camera assignments, and benchmark history tables.

The detector uses one common `ModelConfig` interface for YOLOv8, YOLOv11, YOLOv26, and future ONNX-compatible model families. Model sessions are hot-reloadable and CUDA is preferred with CPU fallback.

## Selection

- The default model is stored in `models.default_model`.
- A camera override is stored in `camera_models` and wins over the global default.
- The pipeline selects the camera model before each frame, preserving existing detection/tracking/observation behavior.
- `Fast`, `Balanced`, and `Accurate` profiles expose confidence policy defaults for preprocessing/runtime tuning; Balanced is used for registered models by default.
- The benchmark endpoint executes zero-filled inference tensors against the requested loaded ONNX session and stores measured FPS and latency, rather than timing a metadata lookup.

## API

- `GET /api/models`
- `GET /api/models/{id}`
- `POST /api/models`
- `DELETE /api/models/{id}`
- `POST /api/models/reload`
- `POST /api/models/{id}/activate`
- `POST /api/models/{id}/benchmark`
- `GET /api/benchmarks`
- `POST /api/cameras/{id}/model/{model_id}`

The Models page is available at `/models`; `/cameras` provides per-camera model routing. Benchmarks record FPS, average inference latency, and optional accelerator/CPU metrics. Notifications, recordings, and automation remain outside this phase.
