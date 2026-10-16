"""Where does synth-gen time go at the training config (stream, reuse 12)?
tottime = time in the function itself (excl. subcalls) -> finds the hot leaves
(PIL glyph raster, raqm shaping, numpy composite/degrade).

  python3 profile_gen.py
"""

import cProfile
import io
import pstats
import random

import gen_data as g

rng = random.Random(0)
gen = g.stream(rng, 320, 12)  # reuse 12, as in training

for _ in range(60):  # warm font cache, dict, lru_caches
    next(gen)

pr = cProfile.Profile()
pr.enable()
for _ in range(1500):
    next(gen)
pr.disable()

for sort in ("tottime", "cumtime"):
    s = io.StringIO()
    pstats.Stats(pr, stream=s).sort_stats(sort).print_stats(22)
    print(f"\n================= sorted by {sort} =================")
    print(s.getvalue())
