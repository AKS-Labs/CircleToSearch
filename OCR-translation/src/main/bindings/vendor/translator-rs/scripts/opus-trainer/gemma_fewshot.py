#!/usr/bin/env python3
"""5-shot ug->en with a CPT/base LM via transformers (no vLLM, no SFT).

Runs on a rented GPU box. Exemplars and test are plain-text files (one sentence
per line), so we can point it at FLORES dev (exemplars) + FLORES devtest (test)
and compare the outputs head-to-head with the NLLB teacher on the SAME sentences.

    gemma_fewshot.py MODEL EXEMPLAR_UG EXEMPLAR_EN TEST_UG OUT_EN [NSHOT=5] [LIMIT=100]
"""

import sys

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

model_id, ex_ug, ex_en, test_ug, out_en = sys.argv[1:6]
nshot = int(sys.argv[6]) if len(sys.argv) > 6 else 5
limit = int(sys.argv[7]) if len(sys.argv) > 7 else 100

ex_src = open(ex_ug, encoding="utf-8").read().splitlines()[:nshot]
ex_tgt = open(ex_en, encoding="utf-8").read().splitlines()[:nshot]
tests = open(test_ug, encoding="utf-8").read().splitlines()[:limit]

tok = AutoTokenizer.from_pretrained(model_id, trust_remote_code=True)
model = AutoModelForCausalLM.from_pretrained(
    model_id, torch_dtype=torch.bfloat16, device_map="cuda", trust_remote_code=True
)
model.eval()

preamble = "".join(f"Uyghur: {s}\nEnglish: {t}\n\n" for s, t in zip(ex_src, ex_tgt))

outs = []
for i, src in enumerate(tests):
    prompt = f"{preamble}Uyghur: {src}\nEnglish:"
    ids = tok(prompt, return_tensors="pt").to("cuda")
    with torch.no_grad():
        gen = model.generate(**ids, max_new_tokens=256, do_sample=False,
                             pad_token_id=tok.eos_token_id)
    text = tok.decode(gen[0][ids.input_ids.shape[1]:], skip_special_tokens=True)
    # keep only the first completion line (the model may continue the few-shot pattern)
    text = text.split("\nUyghur:")[0].split("\n\n")[0].strip()
    outs.append(text)
    if (i + 1) % 20 == 0:
        print(f"  {i+1}/{len(tests)}", file=sys.stderr, flush=True)

with open(out_en, "w", encoding="utf-8") as f:
    for t in outs:
        f.write(t.replace("\n", " ") + "\n")
print(f"DONE {len(outs)} -> {out_en}", file=sys.stderr)
