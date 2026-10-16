#!/usr/bin/env python3
"""Convert the PP-OCRv6 release tars into the bucket's MNN files.

Takes the upstream inference tars (PIR format: inference.json +
inference.pdiparams), converts via paddle2onnx -> MNNConvert with int8
weight quantization, folds the tiny det's DBNet head to 1/4-resolution
output (see convert_det_lowres_mnn.py), and extracts each rec tier's
character dictionary from its inference.yml — the tiers have different
charsets (tiny has no kana: it drops Japanese, 49 vs 50 languages).

PIR conversion needs paddle2onnx>=2.1 with paddlepaddle installed
(e.g. python3 -m venv venv && venv/bin/pip install paddle2onnx paddlepaddle
packaging onnx pyyaml).

Outputs, named as the catalog expects (catalog_ppocr.py in the app repo):
    PP-OCRv6_tiny_det_int8.mnn
    PP-OCRv6_tiny_det_half_int8.mnn      <- shipping candidate (score-safe)
    PP-OCRv6_tiny_det_quarter_int8.mnn   <- stills-only: drops box scores ~0.11
    PP-OCRv6_tiny_rec_int8.mnn + PP-OCRv6_tiny_keys.txt   <- latin slot
    PP-OCRv6_small_rec_int8.mnn + PP-OCRv6_small_keys.txt <- cj slot
"""

import argparse
import subprocess
import sys
import tarfile
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
from convert_det_lowres_mnn import clear_output_dims, replace_deconv

import onnx

MODELS = {
    "PP-OCRv6_tiny_det_infer": {"kind": "det", "tier": "tiny"},
    "PP-OCRv6_tiny_rec_infer": {"kind": "rec", "tier": "tiny"},
    "PP-OCRv6_small_rec_infer": {"kind": "rec", "tier": "small"},
}
# Folded det variants: half (fold the final deconv only) is exact and
# preserves box scores; quarter additionally folds the first deconv, which
# on the tiny tier lowers box scores by ~0.11 (noisy logits + the BN/ReLU
# approximation) — enough to break the live-path box-score gate on camera
# frames. Ship half; quarter stays available for still-only experiments.
FOLD_VARIANTS = {
    "half": ["ConvTranspose.2"],
    "quarter": ["ConvTranspose.2", "ConvTranspose.0"],
}


def run_paddle2onnx(paddle2onnx: Path, model_dir: Path, out: Path) -> None:
    subprocess.run(
        [
            paddle2onnx,
            "--model_dir", model_dir,
            "--model_filename", "inference.json",
            "--params_filename", "inference.pdiparams",
            "--save_file", out,
            "--opset_version", "14",
        ],
        check=True,
        capture_output=True,
    )


def run_mnnconvert(mnnconvert: Path, onnx_path: Path, mnn_path: Path) -> None:
    subprocess.run(
        [
            mnnconvert,
            "-f", "ONNX",
            "--modelFile", onnx_path,
            "--MNNModel", mnn_path,
            "--bizCode", "biz",
            "--weightQuantBits", "8",
        ],
        check=True,
        capture_output=True,
    )


def write_keys(yml_path: Path, keys_path: Path) -> int:
    config = yaml.safe_load(yml_path.read_text())
    charset = config["PostProcess"]["character_dict"]
    with open(keys_path, "w") as f:
        for ch in charset:
            f.write(str(ch) + "\n")
    return len(charset)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tar-dir", type=Path, default=Path.home() / "Downloads")
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=Path.home() / "AndroidStudioProjects/bucket/ocr/1/PP-OCRv6",
    )
    parser.add_argument("--work-dir", type=Path, default=Path("/tmp/ppocr-v6-convert"))
    parser.add_argument("--paddle2onnx", type=Path, default=Path("paddle2onnx"))
    parser.add_argument(
        "--mnnconvert",
        type=Path,
        default=Path(__file__).resolve().parent.parent.parent
        / "mnn-sys/3rd_party/MNN/build-convert/MNNConvert",
    )
    args = parser.parse_args()

    args.work_dir.mkdir(parents=True, exist_ok=True)
    args.out_dir.mkdir(parents=True, exist_ok=True)

    for stem, spec in MODELS.items():
        tar_path = args.tar_dir / f"{stem}.tar"
        with tarfile.open(tar_path) as tar:
            tar.extractall(args.work_dir)
        model_dir = args.work_dir / stem
        onnx_path = args.work_dir / f"{stem}.onnx"
        run_paddle2onnx(args.paddle2onnx, model_dir, onnx_path)

        tier, kind = spec["tier"], spec["kind"]
        mnn_path = args.out_dir / f"PP-OCRv6_{tier}_{kind}_int8.mnn"
        run_mnnconvert(args.mnnconvert, onnx_path, mnn_path)
        print(mnn_path)

        if kind == "det":
            for variant, deconvs in FOLD_VARIANTS.items():
                model = onnx.load(onnx_path)
                for deconv in deconvs:
                    replace_deconv(model.graph, deconv, f"folded_{deconv}")
                clear_output_dims(model.graph)
                folded_onnx = args.work_dir / f"{stem}_{variant}.onnx"
                onnx.save(model, folded_onnx)
                folded_mnn = args.out_dir / f"PP-OCRv6_{tier}_{kind}_{variant}_int8.mnn"
                run_mnnconvert(args.mnnconvert, folded_onnx, folded_mnn)
                print(folded_mnn)

        if kind == "rec":
            keys_path = args.out_dir / f"PP-OCRv6_{tier}_keys.txt"
            count = write_keys(model_dir / "inference.yml", keys_path)
            print(f"{keys_path} ({count} entries)")


if __name__ == "__main__":
    main()
