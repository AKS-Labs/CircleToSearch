"""Run the Hebrew MNN recognizer over a directory of pre-cropped line strips.

For strips produced by `viz_pipeline` (deskewed/squashed box-NNN.png): each is a
straightened ~48px line, so no segmentation — just resize to h48 at native width,
BGR-normalize, run MNN, CTC-decode, and print the logical-order text.

  python rec_strips.py <model.mnn> <keys.txt> <strips_dir> [min_score]
"""

import glob
import sys

import MNN
import numpy as np
from bidi import get_display
from PIL import Image

model, keys_path, strips_dir = sys.argv[1:4]

keys = [k for k in open(keys_path, encoding="utf-8").read().split("\n")]
if keys and keys[-1] == "":
    keys = keys[:-1]
charlist = [""] + keys + [" "]

itp = MNN.Interpreter(model)
sess = itp.createSession()
inp = itp.getSessionInput(sess)


def decode(a):  # T,C — already softmax probabilities
    idx = a.argmax(1)
    out, prev, scores = [], -1, []
    for t, i in enumerate(idx):
        if i != prev and i != 0:
            out.append(charlist[i])
            scores.append(float(a[t, i]))
        prev = i
    conf = float(np.mean(scores)) if scores else 0.0
    return "".join(out), conf


def run(path):
    im = Image.open(path).convert("RGB")
    w, h = im.size
    W = max(16, int(round(48 * w / h)) // 8 * 8)
    arr = np.asarray(im.resize((W, 48), Image.BILINEAR), "float32")[:, :, ::-1]
    x = np.ascontiguousarray(np.transpose((arr / 255.0 - 0.5) / 0.5, (2, 0, 1))[None])
    shp = (1, 3, 48, W)
    itp.resizeTensor(inp, shp)
    itp.resizeSession(sess)
    inp.copyFrom(MNN.Tensor(shp, MNN.Halide_Type_Float, x, MNN.Tensor_DimensionType_Caffe))
    itp.runSession(sess)
    o = itp.getSessionOutput(sess)
    oshp = tuple(o.getShape())
    ot = MNN.Tensor(oshp, MNN.Halide_Type_Float, np.zeros(oshp, "float32"), MNN.Tensor_DimensionType_Caffe)
    o.copyToHostTensor(ot)
    a = np.array(ot.getData()).reshape(oshp)
    return decode(a[0] if a.ndim == 3 else a)


paths = sorted(glob.glob(f"{strips_dir}/*.png"))
for p in paths:
    pred, conf = run(p)
    logical = get_display(pred, base_dir="R")
    if logical.strip():
        print(f"{conf:.2f}  {p.split('/')[-1]:18s}  {logical}")
