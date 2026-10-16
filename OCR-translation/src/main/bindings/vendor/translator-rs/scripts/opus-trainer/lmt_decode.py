#!/usr/bin/env python3
"""Decode a source file with NiuTrans LMT-60, into a target language.

The LMT half of the probe/gate eval, mirroring nllb_decode.py's interface so
every teacher's hypotheses land in the same shape for side-by-side reading.

LMT-60 is a decoder-only Qwen3-family model prompted with FULL LANGUAGE NAMES
through its chat template, so languages are passed as names, not FLORES codes.
Greedy by default, matching hy_mt2_gate.py: a gate that is re-run must be
reproducible. Pass a beam width to use the model card's beam search instead.

    lmt_decode.py NiuTrans/LMT-60-8B probes.en English Georgian out.ka cuda [beam]
"""

import sys

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

TEMPLATE = "Translate the following text from {src} into {tgt}:\n{src}: {text}\n{tgt}:"


def main() -> None:
    model_id, src_p, src_name, tgt_name, out_p, device = sys.argv[1:7]
    beam = int(sys.argv[7]) if len(sys.argv) > 7 else 1
    lines = [l for l in open(src_p, encoding="utf-8").read().splitlines() if l.strip()]

    tok = AutoTokenizer.from_pretrained(model_id, padding_side="left")
    dtype = torch.bfloat16 if device.startswith("cuda") else torch.float32
    model = AutoModelForCausalLM.from_pretrained(model_id, dtype=dtype).eval().to(device)

    prompts = [
        tok.apply_chat_template(
            [{"role": "user", "content": TEMPLATE.format(src=src_name, tgt=tgt_name, text=l)}],
            tokenize=False, add_generation_prompt=True,
        )
        for l in lines
    ]

    hyps: list[str] = []
    batch = 16
    with torch.no_grad():
        for i in range(0, len(prompts), batch):
            enc = tok(prompts[i : i + batch], return_tensors="pt", padding=True,
                      add_special_tokens=False).to(device)
            gen = model.generate(**enc, max_new_tokens=256, do_sample=False,
                                 num_beams=beam,
                                 pad_token_id=tok.pad_token_id or tok.eos_token_id)
            for row, out in zip(enc.input_ids, gen):
                hyps.append(tok.decode(out[len(row):], skip_special_tokens=True).strip())
            print(f"  {min(i + batch, len(prompts))}/{len(prompts)}", end="\r", file=sys.stderr)

    with open(out_p, "w", encoding="utf-8") as f:
        for h in hyps:
            f.write(h.replace("\n", " ") + "\n")


if __name__ == "__main__":
    main()
