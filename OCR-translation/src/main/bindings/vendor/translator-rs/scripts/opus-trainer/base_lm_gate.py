#!/usr/bin/env python3
"""Gate a BASE / CPT language model as an MT teacher via few-shot prompting, on
FLORES-200 devtest (chrF++/spBLEU) — same harness/output as nllb_gate.py so the
numbers line up. Exemplars come from FLORES dev; eval is FLORES devtest.

Base models don't follow instructions, so we do N-shot in-context completion with
stop sequences and take the first output line.

    pip install vllm sacrebleu
    python base_lm_gate.py --pairs eng_Latn-uig_Arab,uig_Arab-eng_Latn \
        --model pkupie/gemma-3-4b-ug-cpt --nshot 5 --limit 300
"""

import argparse

import sacrebleu
from vllm import LLM, SamplingParams

from nllb_gate import CACHE, flores_devtest  # reuse cache + devtest loader
import tarfile
import urllib.request
from pathlib import Path

NAME = {"eng_Latn": "English", "uig_Arab": "Uyghur", "zho_Hans": "Chinese",
        "kaz_Cyrl": "Kazakh", "bod_Tibt": "Tibetan", "khk_Cyrl": "Mongolian"}

FLORES_URL = "https://dl.fbaipublicfiles.com/nllb/flores200_dataset.tar.gz"


def flores_dev(code: str) -> list[str]:
    root = CACHE / "flores200_dataset"
    if not root.exists():
        CACHE.mkdir(exist_ok=True)
        tar = CACHE / "flores200_dataset.tar.gz"
        if not tar.exists():
            urllib.request.urlretrieve(FLORES_URL, tar)
        with tarfile.open(tar) as t:
            t.extractall(CACHE)
    return (root / "dev" / f"{code}.dev").read_text(encoding="utf-8").splitlines()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pairs", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--nshot", type=int, default=5)
    ap.add_argument("--limit", type=int, default=300)
    ap.add_argument("--max-tokens", type=int, default=256)
    ap.add_argument("--max-model-len", type=int, default=4096)
    ap.add_argument("--gpu-mem", type=float, default=0.90)
    args = ap.parse_args()

    llm = LLM(model=args.model, dtype="bfloat16", trust_remote_code=True,
              max_model_len=args.max_model_len, gpu_memory_utilization=args.gpu_mem)

    for pair in args.pairs.split(","):
        s_code, t_code = pair.split("-")
        s_name, t_name = NAME[s_code], NAME[t_code]
        shots = list(zip(flores_dev(s_code)[: args.nshot], flores_dev(t_code)[: args.nshot]))
        preamble = "".join(f"{s_name}: {a}\n{t_name}: {b}\n\n" for a, b in shots)
        src = flores_devtest(s_code)[: args.limit]
        ref = flores_devtest(t_code)[: args.limit]
        prompts = [f"{preamble}{s_name}: {s}\n{t_name}:" for s in src]
        sp = SamplingParams(temperature=0.0, max_tokens=args.max_tokens,
                            stop=["\n\n", f"\n{s_name}:", f"\n{t_name}:"])
        outs = llm.generate(prompts, sp)
        hyps = [o.outputs[0].text.strip() for o in outs]
        for h in hyps[:2]:
            print(f"  [sample {s_code}->{t_code}] {h[:90]}")
        chrf = sacrebleu.corpus_chrf(hyps, [ref], word_order=2)
        bleu = sacrebleu.corpus_bleu(hyps, [ref], tokenize="flores200")
        print(f"{s_code}->{t_code}: chrF++ {chrf.score:.2f}  spBLEU {bleu.score:.2f}  (n={len(hyps)}, {args.nshot}-shot)")


if __name__ == "__main__":
    main()
