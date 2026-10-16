"""Find fonts that crash or hang the data pipeline. Prints each font BEFORE testing it
(flushed), so a native segfault leaves the culprit as the last TEST line; an in-Python
hang is caught by SIGALRM. Render + stroke-measure each latin/shaped/cjk font.

    python3 font_check.py            # writes /tmp/fontcheck.txt
"""

import random
import signal
import sys

from PIL import Image, ImageDraw, ImageFont

import gen_data as g


def handler(_s, _f):
    raise TimeoutError("alarm")


signal.signal(signal.SIGALRM, handler)


def test_font(path, script):
    layout = ImageFont.Layout.RAQM if script in g.SHAPED else ImageFont.Layout.BASIC
    font = ImageFont.truetype(path, 64, layout_engine=layout)
    txt = g._ref_text(script)
    im = Image.new("L", (64 * (len(txt) + 4), 200), 0)
    ImageDraw.Draw(im).text((20, 40), txt, font=font, fill=255)
    g._font_stroke_ratio(path, script)


def main():
    rng = random.Random(0)
    pools = [("latin", g.font_paths())]
    for s in g.SHAPED:
        pools.append((s, list(g.shaped_fonts(s))))
    pools.append(("cjk", list(g.cjk_font_paths())))

    out = open("/tmp/fontcheck.txt", "w")
    bad = []
    for script, paths in pools:
        for i, p in enumerate(paths):
            out.write(f"TEST {script} {i} {p}\n")
            out.flush()
            try:
                signal.alarm(8)
                test_font(p, script)
                signal.alarm(0)
            except Exception as e:
                signal.alarm(0)
                bad.append((script, p, f"{type(e).__name__}: {e}"))
                out.write(f"  BAD {p} -> {type(e).__name__}: {e}\n")
                out.flush()
    out.write(f"\nDONE. {len(bad)} bad fonts:\n")
    for s, p, e in bad:
        out.write(f"  [{s}] {p} -> {e}\n")
    out.close()
    print(f"done, {len(bad)} bad")


if __name__ == "__main__":
    main()
