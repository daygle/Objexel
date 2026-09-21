#!/usr/bin/env python3
"""Export the Objexel default YOLO model family to ONNX and regenerate models/catalog.json.

Objexel's detector consumes ONNX, while Ultralytics publishes PyTorch (`.pt`) weights,
so the ONNX files that back the built-in catalog are produced here and hosted on a
GitHub release. Run this whenever you want to refresh to newer upstream weights:

    pip install "ultralytics==8.4.157" onnx    # pin for byte-reproducible exports
    python scripts/export_models.py --out dist/models

Then upload every *.onnx in the output directory to the release named by --release-tag
(default: models-v1) on the repository named by --repo, and commit the regenerated
models/catalog.json. Deployments pick up the new checksums via the "Refresh catalog"
button (or OBJEXEL_MODEL_CATALOG_URL at boot).

This script is the source of the ONNX assets: the SHA-256 digests it writes into
models/catalog.json are exactly what Objexel verifies after download, so the files you
upload to the release must be the ones this run produced. Because the script rewrites
models/catalog.json to match its own output, the manifest and the uploaded files stay
consistent even if a newer ultralytics/torch changes the bytes — just commit the
regenerated catalog.json. The committed catalog.json in this repo was produced with
ultralytics 8.4.157 / torch 2.14 / opset 12.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

# The families and sizes shipped in the default catalog.
FAMILIES = {
    "YOLO11": [f"yolo11{size}" for size in ("n", "s", "m", "l", "x")],
    "YOLO26": [f"yolo26{size}" for size in ("n", "s", "m", "l", "x")],
}
INPUT_SIZE = 640
OPSET = 12


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="dist/models", help="Directory for exported ONNX files")
    parser.add_argument("--repo", default="daygle/Objexel", help="GitHub owner/repo hosting the release assets")
    parser.add_argument("--release-tag", default="models-v1", help="Release tag the ONNX assets are uploaded to")
    parser.add_argument(
        "--catalog",
        default=str(Path(__file__).resolve().parent.parent / "models" / "catalog.json"),
        help="Path to write the regenerated catalog manifest",
    )
    args = parser.parse_args()

    try:
        from ultralytics import YOLO
    except ImportError:
        print("ultralytics is required: pip install ultralytics onnx", file=sys.stderr)
        return 1

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    base_url = f"https://github.com/{args.repo}/releases/download/{args.release_tag}"

    entries = []
    for family, models in FAMILIES.items():
        for name in models:
            print(f"Exporting {name} …", flush=True)
            model = YOLO(f"{name}.pt")
            exported = Path(model.export(format="onnx", opset=OPSET, simplify=False))
            target = out_dir / f"{name}.onnx"
            if exported.resolve() != target.resolve():
                target.write_bytes(exported.read_bytes())
            digest = sha256_of(target)
            print(f"  {target}  sha256={digest}  bytes={target.stat().st_size}")
            entries.append({
                "id": f"{name}-coco",
                "name": name.replace("yolo11", "YOLO11").replace("yolo26", "YOLO26"),
                "version": "coco",
                "model_type": "yolo-coco",
                "download_url": f"{base_url}/{name}.onnx",
                "sha256": digest,
                "input_width": INPUT_SIZE,
                "input_height": INPUT_SIZE,
                "class_list": [],
            })

    catalog_path = Path(args.catalog)
    catalog_path.write_text(json.dumps(entries, indent=2) + "\n")
    print(f"\nWrote {len(entries)} entries to {catalog_path}")
    print(f"Upload every ONNX in {out_dir} to the '{args.release_tag}' release on {args.repo}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
