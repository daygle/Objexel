# Model hosting — one-time setup (operator steps)

Objexel ships a built-in catalog of the YOLO11 and YOLO26 families so the **Models**
page offers one-click downloads. Because Ultralytics distributes PyTorch (`.pt`) weights
while Objexel runs ONNX, the ONNX files have to be produced once and hosted where your
Objexel server can reach them. The built-in catalog points at a GitHub release named
`models-v1` on this repository.

These are the steps **you** perform once. After this, every deployment gets one-click,
checksum-verified model downloads with no further action.

## Prerequisites

- Python 3.9+ with `pip`
- Push access to `daygle/Objexel` (to create the release)
- ~1 GB free disk for the exported models

## Step 1 — Export the ONNX model family

From a checkout of this repository:

```bash
pip install "ultralytics==8.4.157" onnx
python scripts/export_models.py --out dist/models
```

This writes ten files to `dist/models/` (`yolo11{n,s,m,l,x}.onnx`,
`yolo26{n,s,m,l,x}.onnx`) and regenerates `models/catalog.json` with each file's
SHA-256. Using the pinned `ultralytics==8.4.157` reproduces the exact digests already
committed in `models/catalog.json`.

## Step 2 — Create the `models-v1` release

Web UI: **Releases → Draft a new release → Tag: `models-v1` → target: `main` →
Publish**.

Or with the GitHub CLI:

```bash
gh release create models-v1 --repo daygle/Objexel \
  --title "Objexel model weights v1" \
  --notes "YOLO11 and YOLO26 ONNX exports for the built-in catalog."
```

## Step 3 — Upload the ONNX files as release assets

Web UI: open the `models-v1` release, **Edit**, and drag every file from
`dist/models/` into the assets area.

Or with the GitHub CLI:

```bash
gh release upload models-v1 dist/models/*.onnx --repo daygle/Objexel
```

The asset filenames must stay exactly `yolo11n.onnx`, `yolo11s.onnx`, … so the
download URLs in `models/catalog.json` resolve:

```
https://github.com/daygle/Objexel/releases/download/models-v1/<file>.onnx
```

## Step 4 — Commit the catalog (only if you re-exported)

If Step 1 changed `models/catalog.json` (e.g. you used a different Ultralytics
version), commit it so builds and the refresh endpoint agree with the uploaded files:

```bash
git add models/catalog.json
git commit -m "Update model catalog checksums"
git push
```

If the digests are unchanged, there is nothing to commit.

## Step 5 — Verify in the app

1. Open **Models**. The catalog lists YOLO11 and YOLO26 (`n · s · m · l · x`).
2. If you re-exported after the server started, click **Refresh catalog**.
3. Pick a model → **Download** (the progress bar verifies the checksum) →
   **Enable** → **Activate**.

A good default is **YOLO26n** (fastest) or **YOLO26m** (balanced).

## Updating models later

When you want to refresh to newer upstream weights:

1. Re-run Step 1 (optionally with a newer `ultralytics`).
2. Upload the new files to the release. To keep old versions, create a new tag
   (e.g. `models-v2`) and run `export_models.py --release-tag models-v2`, then
   commit the regenerated `models/catalog.json`.
3. On each deployment, click **Refresh catalog** (or set `OBJEXEL_MODEL_CATALOG_URL`
   so the latest `models/catalog.json` is pulled at boot).

`models/catalog.json` is always rewritten by the export script to match the files it
produced, so the manifest and the uploaded assets never drift.

## Related operator settings

- **Login over plain HTTP:** if you sign in and land back on the login page with no
  error, the browser dropped the `Secure` session cookie. Serve Objexel over HTTPS, or
  set `OBJEXEL_COOKIE_SECURE=0` for a plain-HTTP LAN deployment, then restart.
- **Model storage:** downloaded models are written under `OBJEXEL_MODEL_DIR`
  (default `/models`). Keep this on persistent storage.
- **Skip the built-in catalog:** set `OBJEXEL_DISABLE_DEFAULT_CATALOG=1`.
