#!/usr/bin/env python3
"""Score one hypothesis file against one reference file with the SAME metrics as
the teacher gates (chrF++ word_order=2, spBLEU flores200 tokenizer), so student
numbers line up directly against nllb_gate.py / hy_mt2_gate.py output.

Pass the SOURCE file as a 4th arg to also get COMET22 (wmt22-comet-da, x100).
chrF/ce are blind to confident content-word errors ("rain of rain" costs ~2
chrF); COMET is a neural adequacy metric that catches them — this is the
brittleness blind-angle fix, use it whenever the source file is available.

    python chrf_score.py <label> <hyp> <ref> [src]
"""

import sys

import sacrebleu


def load_comet():
    """Load wmt22-comet-da. Call ONCE from the caller's startup, pass the result in.

    The checkpoint is ~2.3GB of XLM-R and takes ~40s to load, which dwarfs scoring
    a few hundred segments — teacher_metrics.py loading it per call spent 10m30s
    on work that is one load plus sixteen short passes. It is an explicit argument
    rather than a module-level/lazy cache so that nothing initializes at import.
    """
    from comet import download_model, load_from_checkpoint

    return load_from_checkpoint(download_model("Unbabel/wmt22-comet-da"))


def comet22(model, srcs: list[str], hyps: list[str], refs: list[str]) -> float:
    import torch

    data = [{"src": s, "mt": h, "ref": r} for s, h, r in zip(srcs, hyps, refs)]
    gpus = 1 if torch.cuda.is_available() else 0
    return 100 * model.predict(data, batch_size=32, gpus=gpus).system_score


def main() -> None:
    label, hyp_path, ref_path = sys.argv[1], sys.argv[2], sys.argv[3]
    hyps = open(hyp_path, encoding="utf-8").read().splitlines()
    refs = open(ref_path, encoding="utf-8").read().splitlines()
    if len(hyps) != len(refs):
        sys.exit(f"{label}: line-count mismatch hyp={len(hyps)} ref={len(refs)}")
    chrf = sacrebleu.corpus_chrf(hyps, [refs], word_order=2)
    bleu = sacrebleu.corpus_bleu(hyps, [refs], tokenize="flores200")
    line = f"{label}: chrF++ {chrf.score:.2f}  spBLEU {bleu.score:.2f}"
    if len(sys.argv) > 4:
        srcs = open(sys.argv[4], encoding="utf-8").read().splitlines()
        if len(srcs) != len(hyps):
            sys.exit(f"{label}: line-count mismatch src={len(srcs)} hyp={len(hyps)}")
        line += f"  COMET22 {comet22(load_comet(), srcs, hyps, refs):.2f}"
    print(f"{line}  (n={len(hyps)})")


if __name__ == "__main__":
    main()
