"""Hammer the full sample pipeline single-process to reproduce the worker stall.
Prints a heartbeat with the RNG state so a hang/crash can be localized.

    python3 stress.py 8000
"""

import random
import signal
import sys
import time

import gen_data as g


def handler(_s, _f):
    raise TimeoutError("sample hang")


signal.signal(signal.SIGALRM, handler)

n = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
g.font_paths()
rng = random.Random(0)
t = time.time()
for i in range(n):
    if i % 250 == 0:
        print(f"{i} ok {time.time() - t:.0f}s", flush=True)
    try:
        signal.alarm(15)
        g.sample(rng, width=320)
        signal.alarm(0)
    except Exception as e:
        signal.alarm(0)
        print(f"FAIL at {i}: {type(e).__name__}: {e}", flush=True)
print("ALL OK", flush=True)
