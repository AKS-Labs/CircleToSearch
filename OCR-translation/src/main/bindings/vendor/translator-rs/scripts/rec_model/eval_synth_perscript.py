"""Per-script CER on a PaddleOCR label_file, bucketed by Unicode block. Used to get
a held-out synthetic number per Indic script (incl. Kannada, which has no real set)
and to tell training-quality (synthetic CER) apart from real-world generalization.

  python eval_synth_perscript.py <model.mnn> <keys.txt> <label_file> <data_dir> [n]
"""
import collections
import difflib
import sys
import unicodedata

import MNN
import numpy as np
from PIL import Image

model, keys_path, label_file, data_dir = sys.argv[1:5]
n = int(sys.argv[5]) if len(sys.argv) > 5 else 3000

keys = [k for k in open(keys_path, encoding="utf-8").read().split("\n")]
if keys and keys[-1] == "":
    keys = keys[:-1]
charlist = [""] + keys + [" "]
BLOCKS = {"beng": (0x0980, 0x09FF), "gujr": (0x0A80, 0x0AFF), "knda": (0x0C80, 0x0CFF), "mlym": (0x0D00, 0x0D7F)}


def script_of(s):
    for ch in s:
        for nm, (lo, hi) in BLOCKS.items():
            if lo <= ord(ch) <= hi:
                return nm
    return "latin"


def norm(s):
    return unicodedata.normalize("NFC", s).replace("‌", "").replace("‍", "")


itp = MNN.Interpreter(model)
sess = itp.createSession()
inp = itp.getSessionInput(sess)


def run(path):
    im = Image.open(path).convert("RGB")
    w, h = im.size
    W = max(16, int(round(48 * w / h)) // 8 * 8)
    a = np.asarray(im.resize((W, 48), Image.BILINEAR), "float32")[:, :, ::-1]
    x = np.ascontiguousarray(np.transpose((a / 255 - 0.5) / 0.5, (2, 0, 1))[None])
    itp.resizeTensor(inp, (1, 3, 48, W))
    itp.resizeSession(sess)
    inp.copyFrom(MNN.Tensor((1, 3, 48, W), MNN.Halide_Type_Float, x, MNN.Tensor_DimensionType_Caffe))
    itp.runSession(sess)
    o = itp.getSessionOutput(sess)
    shp = tuple(o.getShape())
    ot = MNN.Tensor(shp, MNN.Halide_Type_Float, np.zeros(shp, "float32"), MNN.Tensor_DimensionType_Caffe)
    o.copyToHostTensor(ot)
    arr = np.array(ot.getData()).reshape(shp)
    arr = arr[0] if arr.ndim == 3 else arr
    idx = arr.argmax(1)
    out, prev = [], -1
    for i in idx:
        if i != prev and i != 0:
            out.append(charlist[i])
        prev = i
    return "".join(out)


cnt, exact, num, den = (collections.Counter() for _ in range(4))
for ln in [x for x in open(label_file, encoding="utf-8")][:n]:
    rel, lab = ln.rstrip("\n").split("\t")
    if not lab:
        continue
    sc = script_of(lab)
    pred, lab = norm(run(f"{data_dir}/{rel}")), norm(lab)
    cnt[sc] += 1
    exact[sc] += pred == lab
    sm = difflib.SequenceMatcher(None, lab, pred)
    den[sc] += len(lab)
    num[sc] += len(lab) - sum(b.size for b in sm.get_matching_blocks())
for sc in ["beng", "gujr", "knda", "mlym", "latin"]:
    if cnt[sc]:
        print(f"{sc}: n={cnt[sc]} exact={exact[sc]/cnt[sc]:.0%} CER={num[sc]/max(den[sc],1):.1%}")
