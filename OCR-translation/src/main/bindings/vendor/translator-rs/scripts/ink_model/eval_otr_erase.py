"""Score the ink erase against OTR's erased ground truth.

For each OTR image: run our det+contour-dewarp (viz_pipeline, deskewed stage) to get the
per-box strips, erase them with the ink model (erase_full.erase_from_strips), then compare
the result to `gt_image`. PSNR is reported both globally (collateral damage to non-text)
and over the text regions (the union of OTR word boxes — the erase-quality signal), with
the before (overlay vs gt) value alongside so the gain is visible.

    python eval_otr_erase.py --data data/otr --n 30 --ckpt ckpt/ink-base16-1x1-prod.pt \
        --viz ../../target/release/viz_pipeline

det recall is a shared confound (text our det misses is never erased), but it's held
constant across ink models, so this is valid for A/B; absolute numbers undercount matte
quality. Use the same --n/seed when comparing checkpoints.
"""

import argparse
import json
import os
import subprocess
import tempfile

import numpy as np
from PIL import Image, ImageDraw

from erase_full import erase_from_strips, load_ink_model

DEFAULT_MODEL_DIR = os.path.expanduser("~/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5")


def psnr(a: np.ndarray, b: np.ndarray, mask: np.ndarray | None = None) -> float:
    if mask is None:
        mse = float(np.mean((a - b) ** 2))
    else:
        m = mask.astype(bool)
        if m.sum() == 0:
            return float("nan")
        mse = float(np.mean((a[m] - b[m]) ** 2))
    return 99.0 if mse <= 1e-9 else float(10.0 * np.log10(1.0 / mse))


def text_mask(bboxes, h: int, w: int, pad: int = 3) -> np.ndarray:
    mask = np.zeros((h, w), dtype=bool)
    for bb in bboxes:
        xs = [bb[0], bb[2]]; ys = [bb[1], bb[3]]
        x0 = max(0, int(min(xs)) - pad); x1 = min(w, int(max(xs)) + pad)
        y0 = max(0, int(min(ys)) - pad); y1 = min(h, int(max(ys)) + pad)
        mask[y0:y1, x0:x1] = True
    return mask


def run_det_dewarp(viz: str, image: str, out_dir: str, model_dir: str) -> str:
    subprocess.run(
        [viz, image, "--out", out_dir, "--script", "latin", "--stages", "deskewed",
         "--model-dir", model_dir],
        check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    return os.path.join(out_dir, "deskewed")


def erase_one(model, viz, model_dir, dilate, img_path, img):
    with tempfile.TemporaryDirectory() as td:
        strips_dir = run_det_dewarp(viz, img_path, td, model_dir)
        erased, alpha, raw, quads = erase_from_strips(model, img, strips_dir, dilate)
    return erased, alpha, raw, len(quads)


def spot_panel(img, erased, alpha, gt_ink, path, title):
    """[overlay | removed (red = erase alpha) | true ink (green = GT diff) | erased].
    Over-marking shows as red beyond the green; a miss shows as green without red."""
    h, w = img.shape[:2]

    def tint(mask, color, a=0.6):
        o = img.copy(); m = mask.astype(bool)
        o[m] = (1 - a) * o[m] + a * np.array(color, np.float32)
        return o

    panels = [img, tint(alpha > 0.5, [1, 0, 0]), tint(gt_ink, [0, 1, 0]), np.clip(erased, 0, 1)]
    sheet = np.ones((h + 16, w * 4 + 18, 3), np.float32)
    for i, p in enumerate(panels):
        sheet[16:16 + h, i * (w + 6):i * (w + 6) + w] = p
    pim = Image.fromarray((sheet * 255).astype(np.uint8))
    ImageDraw.Draw(pim).text((2, 2), title, fill=(200, 0, 0))
    pim.save(path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="data/otr")
    ap.add_argument("--n", type=int, default=30, help="images per split")
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--viz", default="../../target/release/viz_pipeline")
    ap.add_argument("--model-dir", default=DEFAULT_MODEL_DIR)
    ap.add_argument("--dilate", type=int, default=7)
    ap.add_argument("--gt-tau", type=float, default=0.002,
                    help="|image-gt| channel-max above which a pixel is true ink. OTR background is "
                         "~lossless so this is effectively diff>0 (0.002 < 1/255, catches any real "
                         "difference, excludes exact-zero background)")
    ap.add_argument("--out", default="data/otr/eval")
    ap.add_argument("--spot", type=int, default=6, help="lowest-precision panels to dump for spot-check")
    args = ap.parse_args()

    model = load_ink_model(args.ckpt)
    ann = {json.loads(l)["id"]: json.loads(l) for l in open(os.path.join(args.data, "ann.jsonl"))}
    os.makedirs(args.out, exist_ok=True)

    def load_pair(split, rid):
        img = np.asarray(Image.open(os.path.join(args.data, split, "img", f"{rid}.png"))
                         .convert("RGB"), dtype=np.float32) / 255.0
        gt = np.asarray(Image.open(os.path.join(args.data, split, "gt", f"{rid}.png"))
                        .convert("RGB"), dtype=np.float32) / 255.0
        return img, gt

    rows = []
    for split in ("easy", "hard"):
        ids = sorted(os.path.basename(p)[:-4]
                     for p in os.scandir(os.path.join(args.data, split, "img"))
                     if p.name.endswith(".png"))[: args.n]
        for rid in ids:
            img, gt = load_pair(split, rid)
            h, w = img.shape[:2]
            img_path = os.path.join(args.data, split, "img", f"{rid}.png")
            erased, alpha, raw, nboxes = erase_one(model, args.viz, args.model_dir, args.dilate, img_path, img)
            tm = text_mask(ann[rid]["word_bboxes"], h, w)
            # Inpaint-free matte score: GT ink = where overlay and erased-GT differ. Two views:
            #   *erase* (alpha>0.5) = what actually got removed — but dilated/hardened, so its
            #     precision is capped by the fixed halo regardless of background quality;
            #   *raw matte* (raw>0.5) = the model's undilated ink decision — isolates background
            #     over-marking (the thing OTR backgrounds target) from the dilation halo.
            gt_ink = np.abs(img - gt).max(axis=2) > args.gt_tau
            ngt = max(int(gt_ink.sum()), 1)

            def pr(mask):
                inter = float((mask & gt_ink).sum())
                return (inter / ngt, inter / max(int(mask.sum()), 1),
                        inter / max(int((mask | gt_ink).sum()), 1))

            recall, prec, iou = pr(alpha > 0.5)
            # raw matte, no dilation/closing — scored exactly as the rendered masks look.
            r_recall, r_prec, r_iou = pr(raw > 0.5)
            rows.append({
                "id": rid, "split": split, "nboxes": nboxes, "det0": nboxes == 0,
                "g_before": psnr(img, gt), "g_after": psnr(erased, gt),
                "t_before": psnr(img, gt, tm), "t_after": psnr(erased, gt, tm),
                "recall": recall, "prec": prec, "iou": iou,
                "r_recall": r_recall, "r_prec": r_prec, "r_iou": r_iou,
                "gt_ink_px": int(gt_ink.sum()),
            })
            tag = " [DET0, dropped]" if nboxes == 0 else ""
            print(f"  {rid}: erase rec {recall:.2f}/prec {prec:.2f} | raw-matte rec {r_recall:.2f}/"
                  f"prec {r_prec:.2f} boxes {nboxes}{tag}", flush=True)

    with open(os.path.join(args.out, "metrics.jsonl"), "w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")

    kept = [r for r in rows if not r["det0"]]
    print("\n=== OTR erase eval (current ink) — det-0 images dropped ===")
    print(f"{'split':<6}{'n':>4}{'det0':>6}{'RAW-MATTE iou':>14}{'rec':>7}{'prec':>7}"
          f"{'erase iou':>11}{'gPSNR':>7}")
    for split in ("easy", "hard", "all"):
        sub = kept if split == "all" else [r for r in kept if r["split"] == split]
        d0 = sum(1 for r in rows if r["det0"] and (split == "all" or r["split"] == split))
        m = lambda key: float(np.nanmean([r[key] for r in sub]))  # noqa: E731
        print(f"{split:<6}{len(sub):>4}{d0:>6}{m('r_iou'):>14.3f}{m('r_recall'):>7.3f}"
              f"{m('r_prec'):>7.3f}{m('iou'):>11.3f}{m('g_after'):>7.1f}")

    # Spot-check: re-process the lowest-precision kept images and dump tinted panels.
    worst = sorted(kept, key=lambda r: r["prec"])[: args.spot]
    for r in worst:
        img, gt = load_pair(r["split"], r["id"])
        img_path = os.path.join(args.data, r["split"], "img", f"{r['id']}.png")
        erased, alpha, _raw, _ = erase_one(model, args.viz, args.model_dir, args.dilate, img_path, img)
        gt_ink = np.abs(img - gt).max(axis=2) > args.gt_tau
        spot_panel(img, erased, alpha, gt_ink,
                   os.path.join(args.out, f"spot-prec{r['prec']:.2f}-{r['id']}.png"),
                   f"{r['id']} prec={r['prec']:.2f} recall={r['recall']:.2f}")
    print(f"\nwrote metrics + {len(worst)} low-precision spot panels -> {args.out}")


if __name__ == "__main__":
    main()
