#!/usr/bin/env python3
"""Offline vLLM 1-best KD decode into ANY target language.

vllm_decode.py is the same thing hardcoded to English and is the memoized uig KD
step — editing it would invalidate that chain's step keys mid-campaign, so this
is a separate script rather than a parameter added there.

1-best greedy: extract-best buys little at these gate rates and 1-best halves the
generated tokens. vLLM's continuous batching handles throughput and input order
is preserved, so gather can cat by shard index.

The `if __name__ == "__main__"` guard is REQUIRED: vLLM's v1 engine uses spawn,
which re-imports this module in each worker.

    vllm_kd.py SRC OUT TARGET_LANGUAGE [LIMIT=0]
"""

import os
import sys
import time


def main() -> None:
    from vllm import LLM, SamplingParams

    src, out, tgt = sys.argv[1], sys.argv[2], sys.argv[3]
    limit = int(sys.argv[4]) if len(sys.argv) > 4 else 0
    model = os.environ["HY_MODEL"]

    # Hy-MT2's own English instruction template (matches hy_mt2_gate.py exactly).
    template = ("Translate the following text into {tgt}. Note that you should only "
                "output the translated result without any additional explanation: {text}")

    lines = [l for l in open(src, encoding="utf-8").read().splitlines() if l.strip()]
    if limit:
        lines = lines[:limit]

    # enforce_eager: skip torch.compile + cudagraph capture. That warmup is
    # ~10-20min per box with no compile cache and can hang, which for a one-pass
    # bulk decode costs more than the ~10-20% eager penalty it saves.
    llm = LLM(model=model, trust_remote_code=True, max_model_len=2048,
              gpu_memory_utilization=0.92, enforce_eager=True)
    tok = llm.get_tokenizer()
    prompts = [
        tok.apply_chat_template(
            [{"role": "user", "content": template.format(tgt=tgt, text=s)}],
            add_generation_prompt=True, tokenize=False,
        )
        for s in lines
    ]

    t0 = time.time()
    outs = llm.generate(prompts, SamplingParams(temperature=0.0, max_tokens=512))
    dt = time.time() - t0

    with open(out, "w", encoding="utf-8") as f:
        for o in outs:
            f.write(o.outputs[0].text.strip().replace("\n", " ").replace("\t", " ") + "\n")
    print(f"THROUGHPUT {len(lines)} lines in {dt:.1f}s = {len(lines)/dt:.1f} l/s",
          file=sys.stderr)


if __name__ == "__main__":
    main()
