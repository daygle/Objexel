# Model management

Objexel uses a registry of local ONNX model files. It does not currently download models from Hugging Face, Ultralytics, GitHub, or another external catalog. This is intentional until model provenance, licensing, checksums, compatibility metadata, and safe storage limits are defined.

## Install a model manually

With Docker Compose, place compatible `.onnx` files in one of these directories:

```text
models/yolov8/
models/yolov11/
models/yolov26/
```

The API mounts `models/` read-only at `/models`. On startup, or after choosing **Reload registry** in the Models page, Objexel discovers ONNX files and registers them in PostgreSQL. A model must exist on disk before it can be registered through `POST /api/models`.

## Select a model

- `GET /api/models` lists registered models.
- `POST /api/models/{id}/activate` selects the global default.
- `POST /api/cameras/{camera_id}/model/{model_id}` assigns a camera override.
- A camera assignment takes precedence over the global default.
- `POST /api/models/{id}/benchmark` measures loaded-session inference performance.

The Models page is available at `/models`. Use **Reload registry**, activate a model, then assign it from the camera workflow. Benchmark each model on the target CPU/GPU before increasing camera count or inference FPS.

## Compatibility

Models must be ONNX files with dimensions and class labels that match their detector configuration. ONNX Runtime performs the authoritative graph validation. CUDA is preferred when available and CPU is used as fallback; actual performance depends on model size, stream resolution, and frame rate.

## Planned catalog support

A future model catalog can add verified downloads, SHA-256 checksums, license metadata, version pinning, resumable progress, and safe enable/rollback operations. Until then, download models through a trusted operator-controlled process and record their source and checksum in deployment documentation.
