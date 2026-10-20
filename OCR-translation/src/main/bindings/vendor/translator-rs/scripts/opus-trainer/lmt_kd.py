#!/usr/bin/env python3
"""Offline vLLM 1-best KD decode with NiuTrans LMT-60.

lmt_decode.py is the transformers-backed gate/probe path; this is the bulk one.
Same chat template, so a KD corpus decoded here matches what the gate scored.

Every input line produces exactly one output line, in input order: a KD corpus is
consumed positionally against its source, so a dropped or reordered line silently
mis-pairs everything after it. Blank input is rejected rather than skipped, for
the same reason.

Decodes in blocks and appends, so an interrupted run leaves usable output and can
resume from the output's own line count; the block boundary also gives a periodic
measured rate instead of one number at the end.

    LMT_MODEL=NiuTrans/LMT-60-8B lmt_kd.py SRC OUT Georgian English
"""

import os
import sys
import time

TEMPLATE = "Translate the following text from {src} into {tgt}:\n{src}: {text}\n{tgt}:"


def main() -> None:
    from vllm import LLM, SamplingParams

    src_p, out_p, src_name, tgt_name = sys.argv[1:5]
    model = os.environ["LMT_MODEL"]
    max_model_len = int(os.environ.get("LMT_MAX_MODEL_LEN", "4096"))
    max_tokens = int(os.environ.get("LMT_MAX_TOKENS", "1024"))
    block = int(os.environ.get("LMT_BLOCK", "50000"))
    # Cudagraphs cost the 8B 34% of its KV cache for kernel-launch savings it does
    # not need (net -12%); they win on the 4B. Measured 2026-08-30, ka_findings 6.
    eager = os.environ.get("LMT_EAGER", "1") == "1"

    lines = open(src_p, encoding="utf-8").read().splitlines()
    blank = [i for i, l in enumerate(lines) if not l.strip()]
    if blank:
        sys.exit(f"{src_p} has {len(blank)} blank lines, first at {blank[0]}; "
                 "a KD source must be one non-empty line per pair")

    done = 0
    if os.path.exists(out_p):
        done = sum(1 for _ in open(out_p, encoding="utf-8"))
        print(f"resuming: {done} lines already decoded", file=sys.stderr, flush=True)
    if done >= len(lines):
        sys.exit(f"{out_p} already has {done} of {len(lines)} lines")

    llm = LLM(model=model, max_model_len=max_model_len,
              gpu_memory_utilization=0.92, enforce_eager=eager)
    tok = llm.get_tokenizer()
    params = SamplingParams(temperature=0.0, max_tokens=max_tokens)

    t_start = time.time()
    with open(out_p, "a", encoding="utf-8") as f:
        for lo in range(done, len(lines), block):
            chunk = lines[lo : lo + block]
            prompts = [
                tok.apply_chat_template(
                    [{"role": "user", "content": TEMPLATE.format(
                        src=src_name, tgt=tgt_name, text=l)}],
                    add_generation_prompt=True, tokenize=False,
                )
                for l in chunk
            ]
            t0 = time.time()
            outs = llm.generate(prompts, params)
            dt = time.time() - t0
            if len(outs) != len(chunk):
                sys.exit(f"vLLM returned {len(outs)} outputs for {len(chunk)} prompts")
            for o in outs:
                f.write(o.outputs[0].text.strip().replace("\n", " ").replace("\t", " ") + "\n")
            f.flush()
            os.fsync(f.fileno())
            n = lo + len(chunk)
            print(f"BLOCK {lo}-{n} {dt:.1f}s = {len(chunk) / dt:.1f} l/s | "
                  f"cumulative {n - done} lines in {time.time() - t_start:.0f}s = "
                  f"{(n - done) / (time.time() - t_start):.1f} l/s",
                  file=sys.stderr, flush=True)

    total = sum(1 for _ in open(out_p, encoding="utf-8"))
    print(f"DONE {total} lines written to {out_p} (source {len(lines)})", file=sys.stderr)
    if total != len(lines):
        sys.exit(f"line-count mismatch: wrote {total}, source has {len(lines)}")


if __name__ == "__main__":
    main()
