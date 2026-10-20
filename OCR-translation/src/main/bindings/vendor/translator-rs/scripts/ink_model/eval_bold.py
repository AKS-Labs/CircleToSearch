"""Quick visual eval of the 2-channel ink model: image | bold label | bold pred.

The bold label/pred are the continuous stroke-width target in [0,1] (brighter = thicker),
not a binary mask.

  python3 eval_bold.py ckpt/ink-latest.pt /tmp/bold_eval.png
"""

import random
import sys

import numpy as np
import torch
from PIL import Image

from gen_data import sample
from model import InkUNet

ckpt_path = sys.argv[1] if len(sys.argv) > 1 else "ckpt/ink-latest.pt"
out_path = sys.argv[2] if len(sys.argv) > 2 else "/tmp/bold_eval.png"

ck = torch.load(ckpt_path, map_location="cpu")
model = InkUNet(base=ck["base"], levels=ck["levels"], bold_from=ck.get("bold_from", 1),
                bold_head=ck.get("bold_head", "dilated"))
model.load_state_dict(ck["model"])
model.eval()

rng = random.Random(7)
g2rgb = lambda a: np.repeat(a[..., None], 3, axis=2)  # noqa: E731
rows = []
for _ in range(10):
    img, cov, bold, _ = sample(rng, width=320)
    x = torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1)))[None]
    with torch.no_grad():
        out = torch.sigmoid(model(x))[0].numpy()  # (2, 48, W)
    bold_pred = out[1]
    pad = np.ones((48, 6, 3), np.float32)
    # image | ground-truth bold | predicted bold
    row = np.concatenate([img, pad, g2rgb(bold), pad, g2rgb(bold_pred)], axis=1)
    rows.append(row)
    rows.append(np.ones((6, row.shape[1], 3), np.float32))
sheet = np.concatenate(rows, axis=0)
Image.fromarray((sheet * 255).astype(np.uint8)).save(out_path)
print("saved", out_path, sheet.shape)
