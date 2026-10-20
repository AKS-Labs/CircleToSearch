"""One-command golden eval: det -> rec (per model) -> content-match -> CER/WER table.

The ONLY manual input is the GT (data/<script>/<image>.json); everything downstream is
scripted and deterministic. For each model it runs the Rust golden_eval (real det + dewarp
+ rec) into a per-script out dir (per-script avoids the <stem>.rec.json cross-script name
collision: sign-1 exists in bengali AND kannada), then scores every GT line by its
best-matching rec line (det-box-count drift proof). Indic is LTR; Hebrew RTL is handled
inside golden_eval (bidi reverse), so GT and rec are both logical order here.

  python run_golden.py <det.mnn> indic  old=mnn:keys  new=mnn:keys
  python run_golden.py <det.mnn> hebrew old=mnn:keys  v4=mnn:keys
"""

import glob
import json
import os
import subprocess
import sys

FAMILIES = {"indic": ["bengali", "gujarati", "kannada", "malayalam"], "hebrew": ["hebrew"], "georgian": ["georgian"]}
HERE = os.path.dirname(os.path.abspath(__file__))
GOLDEN_EVAL = os.path.normpath(os.path.join(HERE, "../../target/debug/golden_eval"))


def lev(a, b):
    m, n = len(a), len(b)
    if m == 0 or n == 0:
        return max(m, n)
    d = list(range(n + 1))
    for i in range(1, m + 1):
        p, d[0] = d[0], i
        for j in range(1, n + 1):
            t = d[j]
            d[j] = min(d[j] + 1, d[j - 1] + 1, p + (a[i - 1] != b[j - 1]))
            p = t
    return d[n]


def gt_images(script):
    out = []
    for gp in sorted(glob.glob(f"data/{script}/*.json")):
        b = os.path.splitext(os.path.basename(gp))[0]
        for ext in (".jpg", ".JPG", ".jpeg", ".png"):
            if os.path.exists(f"data/{script}/{b}{ext}"):
                out.append(f"data/{script}/{b}{ext}")
                break
    return out


def run_eval(det, family, mnn, keys, images, out):
    subprocess.run([GOLDEN_EVAL, det, mnn, keys, family, out, *images],
                   check=True, capture_output=True)


def score(script, rec_dir):
    te = tl = we = wl = exact = nlines = 0
    for gp in sorted(glob.glob(f"data/{script}/*.json")):
        name = os.path.splitext(os.path.basename(gp))[0]
        gt = json.load(open(gp, encoding="utf-8"))
        rp = os.path.join(rec_dir, f"{name}.rec.json")
        cands = [v for v in json.load(open(rp, encoding="utf-8")).values() if v] if os.path.exists(rp) else []
        for g in gt.values():
            p = min(cands, key=lambda c: lev(c, g)) if cands else ""
            nlines += 1
            exact += (p == g)
            te += lev(p, g)
            tl += len(g)
            we += lev(p.split(), g.split())
            wl += len(g.split())
    return te / max(tl, 1), we / max(wl, 1), exact, nlines, tl


def main():
    det, family = sys.argv[1], sys.argv[2]
    models = [a.split("=", 1) for a in sys.argv[3:]]  # [(label, "mnn:keys"), ...]
    if family not in FAMILIES:
        sys.exit(f"family must be one of {list(FAMILIES)}")
    if not os.path.exists(GOLDEN_EVAL):
        sys.exit(f"build it first: cargo build --features ppocr --bin golden_eval (missing {GOLDEN_EVAL})")

    results = {}  # (label, script) -> (cer, wer, exact, nlines, chars)
    for label, spec in models:
        mnn, keys = spec.split(":", 1)
        for script in FAMILIES[family]:
            imgs = gt_images(script)
            if not imgs:
                continue
            out = f"/tmp/rg_{label}_{script}"
            run_eval(det, family, mnn, keys, imgs, out)
            results[(label, script)] = score(script, out)

    labels = [l for l, _ in models]
    w = 14
    hdr = f"{'script':12} {'chars':>6} " + " ".join(f"{l:>{w}}" for l in labels)
    print(hdr)
    print("-" * len(hdr))
    for script in FAMILIES[family]:
        if not any((l, script) in results for l in labels):
            continue
        chars = next(results[(l, script)][4] for l in labels if (l, script) in results)
        cells = []
        for l in labels:
            if (l, script) in results:
                cer, wer, ex, nl, _ = results[(l, script)]
                cells.append(f"{cer:.3f}/{wer:.3f} {ex}/{nl}".rjust(w))
            else:
                cells.append("-".rjust(w))
        print(f"{script:12} {chars:>6} " + " ".join(cells))
    print("\n(cells = CER/WER exact/lines)")


if __name__ == "__main__":
    main()
