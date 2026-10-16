#!/usr/bin/env python3
"""Extract a Galician word -> VITS-token-id lexicon from cotovia + sabela.

Each word is fed to the AhoTTS binary terminated by a period (one onnx Run per
word). An LD_PRELOAD interposer (ort_intercept.so) dumps the int64 `input`
tensor cotovia feeds the model. A stub onnx is used in place of the real voice
so the VITS vocoder is skipped (~18x faster); cotovia's phoneme->id mapping is
independent of the onnx weights, so the captured ids are identical.

Captured ids look like `0 p1 0 p2 0 ... 0 3 0`: blank id 0 interspersed, EOS id
3 terminal. We store per word the bare [p1..pn] (drop blanks and the trailing
EOS); the runtime re-adds EOS once per utterance and re-intersperses the blank.
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

BLANK_ID = 0
EOS_ID = 3


def parse_ids_line(line: str) -> list[int]:
    ids = [int(x) for x in line.split()[1:]]  # drop the "IDS" prefix
    content = [x for x in ids if x != BLANK_ID]
    if content and content[-1] == EOS_ID:
        content = content[:-1]
    return content


def run_chunk(words: list[str], ahotts_dir: Path, voice: Path, interpose: Path,
              dump: Path) -> list[list[int]]:
    if dump.exists():
        dump.unlink()
    env = dict(os.environ)
    env["LD_LIBRARY_PATH"] = str(ahotts_dir)
    env["LD_PRELOAD"] = str(interpose)
    env["ORT_DUMP_FILE"] = str(dump)
    stdin = "".join(f"{w}.\n" for w in words)
    wav = dump.with_suffix(".wav")  # per-worker, discarded
    subprocess.run(
        [str(ahotts_dir / "ahotts/tts"), "-Lang=gl", "-Method=Vits",
         f"-HDicDB={ahotts_dir}/ahotts/dicts/gl/cotovia",
         f"-voice_path={voice}", str(wav)],
        input=stdin, text=True, env=env,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    lines = [l for l in dump.read_text().splitlines() if l.startswith("IDS")]
    return [parse_ids_line(l) for l in lines]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ahotts-dir", type=Path, required=True)
    ap.add_argument("--wordlist", type=Path, required=True)
    ap.add_argument("--voice", type=Path, required=True,
                    help="voice dir holding vits.onnx (use the stub for speed)")
    ap.add_argument("--interpose", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--chunk", type=int, default=2000)
    ap.add_argument("--dump", type=Path,
                    default=Path(f"/tmp/ort_extract_dump_{os.getpid()}.txt"),
                    help="per-worker interposer dump file; must be unique when running in parallel")
    args = ap.parse_args()

    words = [w.strip() for w in args.wordlist.read_text().splitlines() if w.strip()]
    dump = args.dump
    lexicon: dict[str, list[int]] = {}
    misaligned: list[str] = []

    for start in range(0, len(words), args.chunk):
        chunk = words[start:start + args.chunk]
        ids = run_chunk(chunk, args.ahotts_dir, args.voice, args.interpose, dump)
        if len(ids) == len(chunk):
            for w, seq in zip(chunk, ids):
                if seq:
                    lexicon[w] = seq
        else:
            # a word in this chunk split into !=1 Run; fall back to per-word
            misaligned.extend(chunk)
        done = min(start + args.chunk, len(words))
        print(f"\r{done}/{len(words)}  (misaligned so far: {len(misaligned)})",
              end="", file=sys.stderr, flush=True)
    print(file=sys.stderr)

    for w in misaligned:
        ids = run_chunk([w], args.ahotts_dir, args.voice, args.interpose, dump)
        if len(ids) == 1 and ids[0]:
            lexicon[w] = ids[0]

    with args.out.open("w") as f:
        for w in sorted(lexicon):
            f.write(f"{w}\t{' '.join(map(str, lexicon[w]))}\n")
    print(f"wrote {len(lexicon)} entries to {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
