"""Build a local OTR eval set (cyberagent/OTR, CC-BY-4.0) for the ink erase eval.

OTR ships overlay text composited on complex backgrounds with clean erased GT and
per-word boxes — closer to the screenshot/document case than scene-text sets, and the
only real-with-GT option that's openly downloadable (SCUT-EnsText/Flickr-ST are gated).

Downloads one easy + one hard parquet shard (~1GB, resumable) and extracts a per-split
sample to disk as image/gt PNGs + an annotations jsonl, so the Rust det path can dewarp
strips from `image` and the erase can be scored against `gt_image`.

    python build_otr_eval.py --out data/otr --n 500
"""

import argparse
import io
import json
import os

import pyarrow.parquet as pq
from huggingface_hub import hf_hub_download
from PIL import Image

REPO = "cyberagent/OTR"
NSHARDS = {"easy": 12, "hard": 15}  # total shard counts in the repo


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="data/otr")
    ap.add_argument("--n", type=int, default=500, help="images per split (easy/hard)")
    ap.add_argument("--shard-idx", type=int, default=0,
                    help="which parquet shard per split (0 = pack1/training; use 1 for a held-out set)")
    args = ap.parse_args()

    manifest = []
    for split, ns in NSHARDS.items():
        shard = f"data/OTR_{split}-{args.shard_idx:05d}-of-{ns:05d}.parquet"
        path = hf_hub_download(REPO, shard, repo_type="dataset")
        img_dir = os.path.join(args.out, split, "img")
        gt_dir = os.path.join(args.out, split, "gt")
        os.makedirs(img_dir, exist_ok=True)
        os.makedirs(gt_dir, exist_ok=True)
        pf = pq.ParquetFile(path)
        count = 0
        for batch in pf.iter_batches(batch_size=64):
            d = batch.to_pydict()
            for i in range(len(d["id"])):
                rid = f"{split}-{d['id'][i]}"
                Image.open(io.BytesIO(d["image"][i]["bytes"])).convert("RGB").save(
                    os.path.join(img_dir, f"{rid}.png"))
                Image.open(io.BytesIO(d["gt_image"][i]["bytes"])).convert("RGB").save(
                    os.path.join(gt_dir, f"{rid}.png"))
                manifest.append({
                    "id": rid, "split": split, "class": d["class"][i],
                    "words": d["words"][i], "word_bboxes": d["word_bboxes"][i],
                })
                count += 1
                if count >= args.n:
                    break
            if count >= args.n:
                break
        print(f"{split}: extracted {count} images")

    with open(os.path.join(args.out, "ann.jsonl"), "w") as f:
        for m in manifest:
            f.write(json.dumps(m) + "\n")
    print(f"wrote {len(manifest)} entries -> {os.path.join(args.out, 'ann.jsonl')}")


if __name__ == "__main__":
    main()
