"""Zero-shot synthetic eval on connected/cursive scripts the model never trained on
(it's Latin + CJK only). Renders properly *shaped* Arabic (RTL, contextual forms),
Devanagari/Tamil (Indic conjuncts + headline bar), and Thai via PIL+raqm, composites
strips, runs the ink matte, and writes orig|matte|overlay rows per script.

    python eval_scripts.py --ckpt ckpt/ink-v10.pt --out /tmp/script-eval
"""

import argparse
import os
import subprocess

import numpy as np
import torch
from PIL import Image, ImageDraw, ImageFont

from model import InkUNet

RAQM = ImageFont.Layout.RAQM
H = 48

SCRIPTS = {
    "arabic": (["naskharabic-regular", "notosansarabic-regular", "arabic"],
               ["مرحبا", "السلام", "كتاب", "قهوة", "مدينة", "شكرا"]),
    "devanagari": (["notosansdevanagari-regular", "devanagari"],
                   ["नमस्ते", "किताब", "पानी", "धन्यवाद", "भारत", "विद्यालय"]),
    "tamil": (["notosanstamil-regular", "tamil"],
              ["வணக்கம்", "புத்தகம்", "தண்ணீர்", "நன்றி", "பள்ளி"]),
    "thai": (["notosansthai-regular", "thai"],
             ["สวัสดี", "ขอบคุณ", "หนังสือ", "กาแฟ", "โรงเรียน"]),
}


def find_font(lang_hint, prefer):
    lang = {"arabic": "ar", "devanagari": "hi", "tamil": "ta", "thai": "th"}[lang_hint]
    out = subprocess.check_output(["fc-list", f":lang={lang}", "file"], text=True)
    paths = [ln.split(":")[0].strip() for ln in out.splitlines() if ln.strip()]
    paths = [p for p in paths if p.lower().endswith((".ttf", ".otf"))]
    for pref in prefer:
        for p in paths:
            if pref in p.lower().replace("-", "").replace("ui", ""):
                return p
    return sorted(paths)[0]


def render_coverage(text, font_path, nh=40):
    font = ImageFont.truetype(font_path, nh, layout_engine=RAQM)
    tmp = ImageDraw.Draw(Image.new("L", (8, 8)))
    left, top, right, bottom = tmp.textbbox((0, 0), text, font=font)
    pad = 12
    w, h = (right - left) + 2 * pad, (bottom - top) + 2 * pad
    canvas = Image.new("L", (w, h), 0)
    ImageDraw.Draw(canvas).text((pad - left, pad - top), text, font=font, fill=255)
    return np.asarray(canvas, np.float32) / 255.0


def matte(model, img):
    h, w = img.shape[:2]
    sw = max(16, round(w * H / h))
    sw += (-sw) % 8
    small = np.asarray(
        Image.fromarray((img * 255).astype(np.uint8)).resize((sw, H), Image.BILINEAR),
        np.float32) / 255.0
    with torch.no_grad():
        x = torch.from_numpy(np.ascontiguousarray(small.transpose(2, 0, 1)))[None]
        m = torch.sigmoid(model(x))[0, 0].numpy()
    return small, m


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    state = torch.load(args.ckpt, map_location="cpu")
    model = InkUNet(base=state.get("base", 16), levels=state.get("levels", 2))
    model.load_state_dict(state["model"])
    model.eval()

    for script, (prefer, words) in SCRIPTS.items():
        fp = find_font(script, prefer)
        rows = []
        for i, word in enumerate(words):
            cov = render_coverage(word, fp, nh=40)
            ink = np.array([0.1, 0.1, 0.12], np.float32)        # dark ink
            bg = np.full((*cov.shape, 3), 0.85, np.float32)     # light bg
            img = cov[..., None] * ink + (1 - cov[..., None]) * bg
            small, m = matte(model, img)
            over = small * 0.45 + np.stack([m, m * 0.2, m * 0.8], -1) * 0.55
            gap = np.ones((H, 6, 3), np.float32)
            rows.append(np.concatenate(
                [small, gap, np.repeat(m[..., None], 3, -1), gap, over], 1))
        wmax = max(r.shape[1] for r in rows)
        sheet = np.concatenate(
            [np.pad(r, ((0, 0), (0, wmax - r.shape[1]), (0, 0)), constant_values=0.3) for r in
             sum(([r, np.ones((4, wmax, 3))] for r in rows), [])[:-1]], 0)
        Image.fromarray((np.clip(sheet, 0, 1) * 255).astype(np.uint8)).save(
            os.path.join(args.out, f"{script}.png"))
        print(f"{script}: {os.path.basename(fp)} -> {script}.png")


if __name__ == "__main__":
    main()
