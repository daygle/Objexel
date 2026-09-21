# Model hosting — one-time setup (operator steps)

Objexel ships a built-in catalog of the YOLO11 and YOLO26 families so the **Models**
page offers one-click downloads. Because Ultralytics distributes PyTorch (`.pt`) weights
while Objexel runs ONNX, the ONNX files have to be produced once and hosted where your
Objexel server can reach them. The built-in catalog points at a GitHub release named
`models-v1` on this repository.

These are the steps **you** perform once. After this, every deployment gets one-click,
checksum-verified model downloads with no further action.

## Option A — publish from GitHub Actions (no Python, recommended)

The **Publish model weights** workflow (`.github/workflows/publish-models.yml`) exports the
whole family and uploads it to the release for you — nothing to install locally.

1. Go to the repo's **Actions** tab → **Publish model weights** → **Run workflow**.
2. Leave the tag as `models-v1` (or enter another) and start it.
3. When it finishes, the `models-v1` release exists with all ten ONNX assets attached, and
   the workflow has reconciled `models/catalog.json` to match.

You can also trigger it by pushing a tag, e.g. `git tag models-v1 && git push origin models-v1`.

Then [verify in the app](#verify-in-the-app).

## Option B — export locally (advanced)

Only if you'd rather build the files on your own machine (air-gapped, custom weights, or
no Actions access). Needs Python 3.9+ and ~1 GB disk. From a checkout of this repository:

```bash
pip install "ultralytics==8.4.157" onnx
python scripts/export_models.py --out dist/models
```

This writes the ten ONNX files to `dist/models/` and rewrites `models/catalog.json` with
their digests. Then create a `models-v1` release, upload every `dist/models/*.onnx` as
assets — keeping the exact filenames so the `models/catalog.json` URLs resolve — and
commit the manifest if it changed (a different `ultralytics`/`torch` build can produce
different digests). The `gh` equivalents are `gh release create models-v1` and
`gh release upload models-v1 dist/models/*.onnx`.

## Verify in the app

1. Open **Models**. The catalog lists YOLO11 and YOLO26 (`n · s · m · l · x`).
2. If you re-exported after the server started, click **Refresh catalog**.
3. Pick a model → **Download** (the progress bar verifies the checksum) →
   **Enable** → **Activate**.

A good default is **YOLO26n** (fastest) or **YOLO26m** (balanced).

## Updating models later

When you want to refresh to newer upstream weights:

1. Re-run the **Publish model weights** workflow. To keep the old versions, run it with a
   new tag (e.g. `models-v2`); the workflow exports, uploads, and reconciles
   `models/catalog.json` for that tag.
2. On each deployment, click **Refresh catalog** (or set `OBJEXEL_MODEL_CATALOG_URL`
   so the latest `models/catalog.json` is pulled at boot).

`models/catalog.json` is always rewritten by the export step to match the files it
produced, so the manifest and the uploaded assets never drift.

## Related operator settings

- **Login over plain HTTP:** if you sign in and land back on the login page with no
  error, the browser dropped the `Secure` session cookie. Serve Objexel over HTTPS, or
  set `OBJEXEL_COOKIE_SECURE=0` for a plain-HTTP LAN deployment, then restart.
- **Model storage:** downloaded models are written under `OBJEXEL_MODEL_DIR`
  (default `/models`). Keep this on persistent storage.
- **Skip the built-in catalog:** set `OBJEXEL_DISABLE_DEFAULT_CATALOG=1`.
