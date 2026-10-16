"""Score a golden_eval run against the human/VLM ground truth, by CONTENT matching.

golden_eval writes <img>.rec.json ({idx: model_text}); the GT lives at <gt_dir>/<img>.json
({idx: truth}). The det box COUNT drifts between GT-creation and now (more/fewer boxes), so
the reading-order index is NOT stable — instead, for each GT line we take the model's
best-matching output line (min edit distance) from the same image. That decouples the
recognition score from det box-count drift; identical metric for every model → fair A/B.
Indic is LTR — no bidi.

  python compare_rec.py <gt_dir> <rec_dir>
"""

import glob
import json
import os
import sys


def lev(a, b):
    m, n = len(a), len(b)
    if m == 0 or n == 0:
        return max(m, n)
    d = list(range(n + 1))
    for i in range(1, m + 1):
        prev, d[0] = d[0], i
        for j in range(1, n + 1):
            t = d[j]
            d[j] = min(d[j] + 1, d[j - 1] + 1, prev + (a[i - 1] != b[j - 1]))
            prev = t
    return d[n]


def main():
    gt_dir, rec_dir = sys.argv[1], sys.argv[2]
    te = tl = we = wl = exact = nlines = 0
    for gp in sorted(glob.glob(os.path.join(gt_dir, "*.json"))):
        name = os.path.splitext(os.path.basename(gp))[0]
        gt = json.load(open(gp, encoding="utf-8"))
        rp = os.path.join(rec_dir, f"{name}.rec.json")
        rec = json.load(open(rp, encoding="utf-8")) if os.path.exists(rp) else {}
        cands = [v for v in rec.values() if v]
        for g in gt.values():
            p = min(cands, key=lambda c: lev(c, g)) if cands else ""
            nlines += 1
            exact += (p == g)
            te += lev(p, g)
            tl += len(g)
            we += lev(p.split(), g.split())
            wl += len(g.split())
    tag = os.path.basename(os.path.normpath(gt_dir))
    print(f"{tag:10} CER {te / max(tl, 1):.3f}  WER {we / max(wl, 1):.3f}  exact {exact}/{nlines}  ({tl} chars)")


if __name__ == "__main__":
    main()
