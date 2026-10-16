"""Run the Hebrew MNN recognizer on real photos/scans, no detector.

Segments text lines by horizontal ink projection (works on clean horizontal
text), crops each line, runs the int8 MNN rec at native width, CTC-decodes, and
writes an annotated montage (crop + logical-order prediction) per image. This is
a rough real-world read, not a substitute for det->rec.

  python infer_real.py <model.mnn> <keys.txt> <image> [y0 y1]   # y0,y1 = frac band to scan
"""

import sys

import MNN
import numpy as np
from bidi import get_display
from PIL import Image, ImageDraw, ImageFont

model, keys_path, img_path = sys.argv[1:4]
yfrac = (float(sys.argv[4]), float(sys.argv[5])) if len(sys.argv) > 5 else (0.0, 1.0)

keys = [k for k in open(keys_path, encoding="utf-8").read().split("\n")]
if keys and keys[-1] == "":
    keys = keys[:-1]
charlist = [""] + keys + [" "]

itp = MNN.Interpreter(model)
sess = itp.createSession()
inp = itp.getSessionInput(sess)


def decode(a):
    idx = a.argmax(1)
    out, prev = [], -1
    for i in idx:
        if i != prev and i != 0:
            out.append(charlist[i])
        prev = i
    return "".join(out)


def run(strip_rgb):  # HxWx3 uint8 RGB
    h, w, _ = strip_rgb.shape
    W = max(16, int(round(48 * w / h)) // 8 * 8)
    im = Image.fromarray(strip_rgb).resize((W, 48), Image.BILINEAR)
    arr = np.asarray(im, "float32")[:, :, ::-1]  # RGB->BGR to match training
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


def segment_lines(gray, y0, y1):
    """Horizontal-projection line bands within the [y0,y1) row range."""
    band = gray[y0:y1]
    ink = (band < band.mean() - 0.10 * 255).sum(axis=1).astype("float32")  # dark px per row
    on = ink > ink.max() * 0.18
    lines, s = [], None
    for i, v in enumerate(list(on) + [False]):
        if v and s is None:
            s = i
        elif not v and s is not None:
            if i - s >= 8:  # min line height
                lines.append((y0 + s, y0 + i))
            s = None
    return lines


img = Image.open(img_path).convert("RGB")
rgb = np.asarray(img)
gray = np.asarray(img.convert("L"))
H = rgb.shape[0]
y0, y1 = int(yfrac[0] * H), int(yfrac[1] * H)
bands = segment_lines(gray, y0, y1)

cells, results = [], []
font = ImageFont.load_default(size=22)
for (a, b) in bands:
    pad = max(2, (b - a) // 6)
    row = gray[max(0, a - pad):b + pad]
    cols = np.where((row < row.mean() - 0.10 * 255).any(axis=0))[0]
    if cols.size < 4:
        continue
    x0, x1 = max(0, cols[0] - pad), min(rgb.shape[1], cols[-1] + pad)
    crop = rgb[max(0, a - pad):b + pad, x0:x1]
    pred = run(crop)
    logical = get_display(pred, base_dir="R")
    results.append(logical)
    cw = max(crop.shape[1], 360)
    cell = np.full((crop.shape[0] + 30, cw, 3), 255, "uint8")
    cell[30:, :crop.shape[1]] = crop
    pim = Image.fromarray(cell)
    ImageDraw.Draw(pim).text((2, 2), logical or "(empty)", fill=(200, 0, 0), font=font)
    cells.append(np.asarray(pim.resize((720, int(pim.height * 720 / pim.width)))))

if cells:
    out = img_path.rsplit(".", 1)[0] + "_pred.png"
    Image.fromarray(np.concatenate(cells, axis=0)).save(out)
    print(f"{len(cells)} lines -> {out}")
for r in results:
    print("  ", r)
