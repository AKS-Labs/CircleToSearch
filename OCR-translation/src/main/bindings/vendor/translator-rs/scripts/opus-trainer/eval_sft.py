import json

import sacrebleu
import torch
from unsloth import FastModel
from unsloth.chat_templates import get_chat_template

model, tok = FastModel.from_pretrained(model_name="merged_16bit", max_seq_length=1024, load_in_4bit=True)
FastModel.for_inference(model)
tok = get_chat_template(tok, chat_template="gemma-3")
tokenizer = getattr(tok, "tokenizer", tok)
tokenizer.padding_side = "left"

NAME = {"en": "English", "ug": "Uyghur"}


def load(dom):
    return json.load(open(f"milic/{dom}_test.json", encoding="utf-8"))


def translate(srcs, s, t):
    out = []
    for i in range(0, len(srcs), 16):
        batch = srcs[i:i + 16]
        prompts = [tok.apply_chat_template(
            [{"role": "user", "content": f"Translate {NAME[s]} to {NAME[t]}:\n{x}"}],
            tokenize=False, add_generation_prompt=True) for x in batch]
        enc = tokenizer(prompts, return_tensors="pt", padding=True).to("cuda")
        with torch.no_grad():
            gen = model.generate(**enc, max_new_tokens=256, do_sample=False)
        for g in gen:
            out.append(tokenizer.decode(g[enc["input_ids"].shape[1]:], skip_special_tokens=True).strip())
        print(f"  {s}->{t} {len(out)}/{len(srcs)}", flush=True)
    return out


print("=== SFT model, MiLiC first-100, chrF++ ===", flush=True)
for dom in ["article", "dialogue"]:
    d = load(dom)[:100]
    for s, t in [("en", "ug"), ("ug", "en")]:
        srcs = [r[s] for r in d]
        refs = [r[t] for r in d]
        hyp = translate(srcs, s, t)
        chrf = sacrebleu.corpus_chrf(hyp, [refs], word_order=2).score
        print(f"[SFT] {dom} {s}->{t}: chrF++ {chrf:.2f}", flush=True)
        print(f"   sample: {hyp[0][:90]}", flush=True)
