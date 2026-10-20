#!/usr/bin/env python3
"""Gate teachers on MiLiC-Eval en<->ug (human-translated), chrF++/spBLEU, for both
domains (article, dialogue) and both directions. Two backends:
  nllb    - transformers seq2seq (NLLB-200), beam 4
  fewshot - base/CPT LM via vLLM, N-shot in-context (exemplars from train_1.json)

    pip install vllm transformers sacrebleu sacremoses datasets
    python milic_gate.py --backend nllb    --model facebook/nllb-200-distilled-1.3B
    python milic_gate.py --backend fewshot --model pkupie/gemma-3-4b-ug-cpt --nshot 5
"""

import argparse
import json

import sacrebleu
from huggingface_hub import hf_hub_download

DOMAINS = ["article", "dialogue"]
DIRS = [("en", "ug"), ("ug", "en")]


def load(domain: str, split: str) -> list[dict]:
    p = hf_hub_download("pkupie/milic-eval", f"translation_{domain}/ug/{split}.json", repo_type="dataset")
    return json.load(open(p, encoding="utf-8"))


def score(tag: str, hyps: list[str], refs: list[str]) -> None:
    chrf = sacrebleu.corpus_chrf(hyps, [refs], word_order=2)
    bleu = sacrebleu.corpus_bleu(hyps, [refs], tokenize="flores200")
    print(f"{tag}: chrF++ {chrf.score:.2f}  spBLEU {bleu.score:.2f}  (n={len(hyps)})", flush=True)


def run_nllb(model_id: str, device: str, limit: int) -> None:
    import torch
    from transformers import AutoModelForSeq2SeqLM, AutoTokenizer

    tok = AutoTokenizer.from_pretrained(model_id)
    model = AutoModelForSeq2SeqLM.from_pretrained(model_id).eval().to(device)
    CODE = {"en": "eng_Latn", "ug": "uig_Arab"}

    def translate(srcs, s, t):
        tok.src_lang = CODE[s]
        bos = tok.convert_tokens_to_ids(CODE[t])
        out = []
        for i in range(0, len(srcs), 16):
            enc = tok(srcs[i:i + 16], return_tensors="pt", padding=True, truncation=True, max_length=256).to(device)
            with torch.no_grad():
                g = model.generate(**enc, forced_bos_token_id=bos, num_beams=4, max_length=256)
            out += tok.batch_decode(g, skip_special_tokens=True)
        return out

    for dom in DOMAINS:
        d = load(dom, "test")[:limit]
        cols = {k: [r[k] for r in d] for k in ("en", "ug")}
        for s, t in DIRS:
            score(f"{model_id} {dom} {s}->{t}", translate(cols[s], s, t), cols[t])


def run_fewshot(model_id: str, nshot: int, limit: int) -> None:
    from vllm import LLM, SamplingParams

    llm = LLM(model=model_id, dtype="bfloat16", trust_remote_code=True,
              max_model_len=4096, gpu_memory_utilization=0.90)
    NAME = {"en": "English", "ug": "Uyghur"}

    for dom in DOMAINS:
        shots = load(dom, "train_1")[:nshot]
        test = load(dom, "test")[:limit]
        for s, t in DIRS:
            pre = "".join(f"{NAME[s]}: {r[s]}\n{NAME[t]}: {r[t]}\n\n" for r in shots)
            prompts = [f"{pre}{NAME[s]}: {r[s]}\n{NAME[t]}:" for r in test]
            refs = [r[t] for r in test]
            sp = SamplingParams(temperature=0.0, max_tokens=256,
                                stop=["\n\n", f"\n{NAME[s]}:", f"\n{NAME[t]}:"])
            hyps = [o.outputs[0].text.strip() for o in llm.generate(prompts, sp)]
            print(f"  [sample {dom} {s}->{t}] {hyps[0][:80]}", flush=True)
            score(f"{model_id} {dom} {s}->{t} ({nshot}-shot)", hyps, refs)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--backend", required=True, choices=["nllb", "fewshot"])
    ap.add_argument("--model", required=True)
    ap.add_argument("--nshot", type=int, default=5)
    ap.add_argument("--limit", type=int, default=10000)
    ap.add_argument("--device", default="cuda")
    a = ap.parse_args()
    if a.backend == "nllb":
        run_nllb(a.model, a.device, a.limit)
    else:
        run_fewshot(a.model, a.nshot, a.limit)


if __name__ == "__main__":
    main()
