# Model management

Objexel supports local ONNX registration and an administrator-controlled verified catalog.

## Built-in default catalog

Objexel ships a built-in catalog so the **Models** page is populated out of the box with
one-click, checksum-verified downloads — no configuration required. It contains the full
YOLO11 and YOLO26 families (`n`, `s`, `m`, `l`, `x`), COCO-trained. Loading a model is:

1. Open **Models**, pick a size, and select **Download** (progress is checksum-verified).
2. **Enable** the downloaded model, then **Activate** it.

The default catalog is seeded on every startup from the embedded copy of
`models/catalog.json` and the upsert is idempotent, so a restart restores the shipped
definitions. Set `OBJEXEL_DISABLE_DEFAULT_CATALOG=1` to skip seeding.

### Hosting the ONNX assets (one-time)

Ultralytics distributes PyTorch (`.pt`) weights, but Objexel's detector consumes ONNX, so
the ONNX files are produced by `scripts/export_models.py` and hosted on a GitHub release.
The default catalog's download URLs point at the `models-v1` release of this repository:

```
https://github.com/daygle/Objexel/releases/download/models-v1/<model>.onnx
```

To activate one-click downloads, create that release once and upload the exported ONNX
files (their SHA-256 digests must match `models/catalog.json`). Until the assets exist the
entries appear in the catalog but downloads will fail the fetch.

Produce the files with `scripts/export_models.py`, which writes both the `*.onnx` assets
and a matching `models/catalog.json`. The committed manifest was generated with
`ultralytics==8.4.157` / `torch 2.14` / opset 12; using the same versions reproduces the
exact bytes and digests. If you export with different versions, just commit the
regenerated `models/catalog.json` — the manifest always matches the files the script
produced, and deployments reconcile via **Refresh catalog**.

### Updating models

Model weights are updated upstream over time. To publish a refresh without shipping a new
Objexel build:

1. Regenerate the family and manifest: `pip install ultralytics onnx && python scripts/export_models.py --out dist/models`.
2. Upload the new ONNX files to the release (a new `--release-tag` for a clean version, or replace assets in place).
3. Commit the regenerated `models/catalog.json`.
4. On each deployment, use **Refresh catalog** on the Models page, or set
   `OBJEXEL_MODEL_CATALOG_URL` so the latest manifest is pulled at boot. The default
   refresh source is `models/catalog.json` on the repository's default branch.

`models/catalog.json` is the single source of truth: it is embedded at build time and also
served over HTTP for the runtime refresh path.

## Catalog entries

Catalog entries are stored in PostgreSQL and contain:

- model name, version, and model type
- HTTPS download URL
- expected SHA-256 digest
- input dimensions
- class labels
- optional `tar.gz` or `zip` archive format

Only catalog entries already present in the database can be downloaded. There is no arbitrary URL field in the web UI. Catalog entries can be imported by an administrator through the API or at startup from `OBJEXEL_MODEL_CATALOG`.

The file is a JSON array:

```json
[
  {
    "id": "synthetic-yolo-v1",
    "name": "Synthetic YOLO",
    "version": "1.0.0",
    "model_type": "yolo",
    "download_url": "https://models.example.invalid/synthetic-yolo-v1.onnx",
    "sha256": "<64 hexadecimal SHA-256 characters>",
    "input_width": 640,
    "input_height": 640,
    "class_list": ["person", "car"],
    "archive_format": null
  }
]
```

Set `OBJEXEL_MODEL_CATALOG=/path/to/catalog.json` before starting the API, or POST the same array to `/api/models/catalog/import`. URLs, checksums, dimensions, and labels are validated before entries are stored.

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

Catalog metadata supplies the ONNX input dimensions and class labels used by the detector. Labels are passed into inference so detections can be named instead of returned as numeric class IDs. If a locally registered model has no labels, the detector falls back to `class_N` names. For standard COCO-trained YOLO exports, use model type `yolo-coco`; when no labels are supplied, Objexel automatically applies the standard COCO-80 ordering, including `cat` at class ID 15.

## Enable and activate

- **Enable** makes a registered model eligible for loading.
- **Activate** selects one enabled model as the default detector.
- **Reload sessions** loads all enabled registered models from `OBJEXEL_MODEL_DIR`.
- Each loaded model has a bounded ONNX session pool so cameras sharing a model can infer concurrently. Set `OBJEXEL_INFERENCE_SESSIONS` to a value from 1 to 16; the default is 2 sessions per model. Increase it only when memory and accelerator capacity allow.
- Disabling a model clears its default status but does not delete its file.

Model files should be kept on persistent storage and backed up with the application configuration. Do not delete a model file while it is active.

## API

- `GET /api/models`
- `PATCH /api/models/{id}` with `{ "enabled": true|false }`
- `POST /api/models/{id}/activate`
- `GET /api/models/catalog`
- `POST /api/models/catalog/import` with a JSON array of trusted catalog entries
- `POST /api/models/catalog/refresh` — re-fetch the hosted manifest (`OBJEXEL_MODEL_CATALOG_URL`, default `models/catalog.json` on the default branch) and upsert its entries
- `POST /api/models/catalog/{id}/download`
- `GET /api/models/downloads/{id}`
- `POST /api/models/reload`

All model-management routes require an authenticated session.
