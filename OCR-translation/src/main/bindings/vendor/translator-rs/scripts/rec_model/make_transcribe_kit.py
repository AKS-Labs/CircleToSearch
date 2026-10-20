"""Build a per-strip transcription kit for a golden-set image dir.

For each image: run the production det+dewarp, dump the line strips as a numbered
sheet (<name>.sheet.png) plus a blank {strip_idx: ""} template (<name>.template.json)
for a human/VLM to fill. Per-strip beats whole-image transcription — it constrains the
transcriber to one line at a time, which avoids the hallucination + granularity drift
that whole-sign prompting produces on dense boards (a VLM asked to read an 84-word
heritage paragraph will invent sentence boundaries and duplicate text; asked to read
one strip it just types that strip). Leave a strip blank if it's detector garbage
(tiny ottakshara/subscript fragments, background billboards) — those drop from scoring.

  python make_transcribe_kit.py <det.mnn> <rec.mnn> <keys> <script> <img_dir> <out_dir>
"""

import glob
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
GOLDEN_EVAL = os.path.normpath(os.path.join(HERE, "../../target/debug/golden_eval"))


def main() -> None:
    det, rec, keys, script, img_dir, out = sys.argv[1:7]
    os.makedirs(out, exist_ok=True)
    imgs = []
    for ext in ("jpg", "JPG", "jpeg", "png"):
        imgs += glob.glob(os.path.join(img_dir, f"*.{ext}"))
    for img in sorted(imgs):
        b = os.path.splitext(os.path.basename(img))[0]
        tmp = f"/tmp/kit_{b}"
        subprocess.run([GOLDEN_EVAL, det, rec, keys, script, tmp, img], check=True, capture_output=True)
        recj = json.load(open(f"{tmp}/{b}.rec.json"))
        json.dump({k: "" for k in sorted(recj)},
                  open(os.path.join(out, f"{b}.template.json"), "w"), ensure_ascii=False, indent=1)
        strips = sorted(glob.glob(f"{tmp}/strips/{b}-*.png"))
        if strips:
            subprocess.run(["montage", *strips, "-tile", "1x", "-geometry", "+2+2", "-background", "white",
                            "-bordercolor", "#888", "-border", "1", "-label", "%f",
                            os.path.join(out, f"{b}.sheet.png")], capture_output=True)
        print(f"  {b}: {len(recj)} strips")


if __name__ == "__main__":
    main()
