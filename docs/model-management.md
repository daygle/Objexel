# Model management

Objexel supports local ONNX registration and an administrator-controlled verified catalog.

## Catalog entries

Catalog entries are stored in PostgreSQL and contain:

- model name, version, and model type
- HTTPS download URL
- expected SHA-256 digest
- input dimensions
- class labels
- optional `tar.gz` or `zip` archive format

Only catalog entries already present in the database can be downloaded. There is no arbitrary URL field in the web UI.

## Download workflow

1. Open **Models** and review a catalog entry.
2. Select **Download**.
3. Monitor the progress percentage and byte count in the download panel.
4. Objexel streams the response to a temporary file and calculates SHA-256 while receiving it.
5. The digest must match before extraction or registration begins.
6. For archives, paths containing parent-directory traversal are rejected and only an ONNX file is accepted as the resulting model.
7. The downloaded model is registered disabled. Enable and activate it only after reviewing its metadata and benchmarking it.

Failed downloads never become selectable models. Temporary files are removed after checksum failure or unsupported archive formats.

## Metadata and labels

Catalog metadata supplies the ONNX input dimensions and class labels used by the detector. Labels are passed into inference so detections can be named instead of returned as numeric class IDs. If a locally registered model has no labels, the detector falls back to `class_N` names.

## Enable and activate

- **Enable** makes a registered model eligible for loading.
- **Activate** selects one enabled model as the default detector.
- **Reload sessions** loads all enabled registered models from `OBJEXEL_MODEL_DIR`.
- Disabling a model clears its default status but does not delete its file.

Model files should be kept on persistent storage and backed up with the application configuration. Do not delete a model file while it is active.

## API

- `GET /api/models`
- `PATCH /api/models/{id}` with `{ "enabled": true|false }`
- `POST /api/models/{id}/activate`
- `GET /api/models/catalog`
- `POST /api/models/catalog/{id}/download`
- `GET /api/models/downloads/{id}`
- `POST /api/models/reload`

All model-management routes require an authenticated session.
