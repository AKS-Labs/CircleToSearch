#!/usr/bin/env python3
"""Zero-shot instruction ug->en with an instruction-tuned decoder LLM, saving
outputs for eyeball. Mirrors hy_mt2_gate.py's exact prompt (Hy-MT2's English
template, greedy) but writes the hypotheses instead of only scoring.

    hy_mt2_run.py MODEL TEST_UG OUT_EN [LIMIT=100]
"""

import sys

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

model_id, test_ug, out_en = sys.argv[1:4]
limit = int(sys.argv[4]) if len(sys.argv) > 4 else 100

TEMPLATE = ("Translate the following text into English. Note that you should only "
            "output the translated result without any additional explanation: {text}")

tok = AutoTokenizer.from_pretrained(model_id, trust_remote_code=True)
model = AutoModelForCausalLM.from_pretrained(
    model_id, torch_dtype=torch.bfloat16, device_map="cuda", trust_remote_code=True
)
model.eval()

tests = [l for l in open(test_ug, encoding="utf-8").read().splitlines() if l.strip()][:limit]

outs = []
for i, s in enumerate(tests):
    prompt = tok.apply_chat_template(
        [{"role": "user", "content": TEMPLATE.format(text=s)}],
        add_generation_prompt=True, tokenize=False,
    )
    ids = tok(prompt, return_tensors="pt").to("cuda")
    with torch.no_grad():
        gen = model.generate(**ids, max_new_tokens=512, do_sample=False,
                             pad_token_id=tok.eos_token_id)
    text = tok.decode(gen[0][ids.input_ids.shape[1]:], skip_special_tokens=True).strip()
    outs.append(text.replace("\n", " "))
    if (i + 1) % 20 == 0:
        print(f"  {i+1}/{len(tests)}", file=sys.stderr, flush=True)

with open(out_en, "w", encoding="utf-8") as f:
    for t in outs:
        f.write(t + "\n")
print(f"DONE {len(outs)} -> {out_en}", file=sys.stderr)
