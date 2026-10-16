#!/usr/bin/env python3
"""Gate Hy-MT2 (Tencent) as a distillation teacher on FLORES-200 devtest (chrF++/spBLEU).

Decoder-only LLM served with vLLM. Reuses nllb_gate.py's FLORES cache + the same
metrics + output line format so scores sit directly next to the NLLB numbers.
Hy-MT2 prompts with the ENGLISH template using FULL LANGUAGE NAMES (not codes), so
FLORES codes are mapped to names here. Greedy by default for a reproducible gate;
--sample switches to Tencent's recommended sampling.

    pip install vllm sacrebleu
    python hy_mt2_gate.py --pairs eng_Latn-uig_Arab,uig_Arab-eng_Latn \
        --model tencent/Hy-MT2-1.8B --limit 200
"""

import argparse
from pathlib import Path

import sacrebleu
from vllm import LLM, SamplingParams

from nllb_gate import flores_devtest, save_pair

# FLORES-200 code -> English language name for the Hy-MT2 prompt (Hy-MT2's 33 langs).
NAME = {
    "eng_Latn": "English", "uig_Arab": "Uyghur", "zho_Hans": "Chinese",
    "zho_Hant": "Traditional Chinese", "yue_Hant": "Cantonese", "tgl_Latn": "Filipino",
    "urd_Arab": "Urdu", "arb_Arab": "Arabic", "pes_Arab": "Persian",
    "hin_Deva": "Hindi", "ben_Beng": "Bengali", "guj_Gujr": "Gujarati",
    "tam_Taml": "Tamil", "tel_Telu": "Telugu", "mar_Deva": "Marathi",
    "heb_Hebr": "Hebrew", "kaz_Cyrl": "Kazakh", "khk_Cyrl": "Mongolian",
    "bod_Tibt": "Tibetan", "khm_Khmr": "Khmer", "mya_Mymr": "Burmese",
    "tha_Thai": "Thai", "vie_Latn": "Vietnamese", "zsm_Latn": "Malay",
    "ind_Latn": "Indonesian", "ukr_Cyrl": "Ukrainian", "rus_Cyrl": "Russian",
    "tur_Latn": "Turkish", "kor_Hang": "Korean", "jpn_Jpan": "Japanese",
    "deu_Latn": "German", "fra_Latn": "French", "spa_Latn": "Spanish",
    "por_Latn": "Portuguese", "ita_Latn": "Italian", "nld_Latn": "Dutch",
    "pol_Latn": "Polish", "ces_Latn": "Czech",
}

TEMPLATE = ("Translate the following text into {tgt}. Note that you should only "
            "output the translated result without any additional explanation: {text}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pairs", required=True, help="comma list of FLORES pairs src_code-tgt_code")
    ap.add_argument("--model", default="tencent/Hy-MT2-1.8B")
    ap.add_argument("--limit", type=int, default=200)
    ap.add_argument("--max-tokens", type=int, default=512)
    ap.add_argument("--max-model-len", type=int, default=2048)
    ap.add_argument("--gpu-mem", type=float, default=0.90)
    ap.add_argument("--sample", action="store_true",
                    help="use Tencent's recommended sampling (default: greedy, reproducible)")
    ap.add_argument("--out-dir", type=Path,
                    help="also write src/hyp/ref per pair, for chrf_score.py (COMET)")
    args = ap.parse_args()

    codes = {c for pair in args.pairs.split(",") for c in pair.split("-")}
    missing = codes - NAME.keys()
    if missing:
        ap.error(f"no language name for FLORES code(s) {sorted(missing)}; add to NAME")

    llm = LLM(model=args.model, dtype="bfloat16", trust_remote_code=True,
              max_model_len=args.max_model_len, gpu_memory_utilization=args.gpu_mem)
    tok = llm.get_tokenizer()
    sp = (SamplingParams(temperature=0.7, top_p=0.6, top_k=20, repetition_penalty=1.05,
                         max_tokens=args.max_tokens, seed=0)
          if args.sample else
          SamplingParams(temperature=0.0, max_tokens=args.max_tokens))

    for pair in args.pairs.split(","):
        src_code, tgt_code = pair.split("-")
        src = flores_devtest(src_code)[: args.limit]
        ref = flores_devtest(tgt_code)[: args.limit]
        prompts = [
            tok.apply_chat_template(
                [{"role": "user", "content": TEMPLATE.format(tgt=NAME[tgt_code], text=s)}],
                add_generation_prompt=True, tokenize=False,
            )
            for s in src
        ]
        outs = llm.generate(prompts, sp)
        hyps = [o.outputs[0].text.strip() for o in outs]
        if args.out_dir is not None:
            save_pair(args.out_dir, src_code, tgt_code, src, hyps, ref)
        chrf = sacrebleu.corpus_chrf(hyps, [ref], word_order=2)
        bleu = sacrebleu.corpus_bleu(hyps, [ref], tokenize="flores200")
        print(f"{src_code}->{tgt_code}: chrF++ {chrf.score:.2f}  spBLEU {bleu.score:.2f}  (n={len(hyps)})")


if __name__ == "__main__":
    main()
