"""Smoke pipe.gate on a 100k slice before it sits between a corpus and 5 boxes.

The gate had never run. Not merely unwired — the hub's config.toml had no
[store] table at all, so the cross-run store it publishes `Filtered` into did not
exist, and every publish failed. That was the real reason uig round 1's Kazakh
contamination went unfiltered by the very module written to catch it.

This exercises the whole path on a slice: artstore publish, the typed
Raw -> Filtered wrappers, gate_rule's budget refusal, and filter_corpus.py with
latin_source=True — the flag that matters most here, because the Discuz junk
rule keys on a Latin letter+punct+alnum run and drops ~61% of clean English if
left enabled on a Latin corpus.

It is not expected to FIND anything: Philippine-confusable contamination in this
pool measured ~0.03%. The number worth reading is FERTILITY's false-positive rate
against known_good — how much real English the mojibake filter eats.

    pipe pin gateslice raw <digest>          (+ known_good, gold, vocab)
    pipe --run gateslice run gateslice
"""

from __future__ import annotations

from pipe.artstore import Aligned, EvalSet, Filtered, Raw
from pipe.gate import gate
from pipe.step import Run
from pipe.target import Bigserver

PREP = "prep:next"
BUDGET = 0.02


def main(run: Run, argv: list[str]) -> dict:
    raw = run.pinned("raw")
    known_good = run.pinned("known_good")
    gold = run.pinned("gold")
    vocab = run.pinned("vocab")

    filtered = gate(
        run,
        raw=raw,
        vocab=vocab,
        gold=gold,
        known_good=known_good,
        budget=BUDGET,
        image=PREP,
        target=Bigserver(cpus=8),
        # The corpus is Latin; leaving the Discuz rule on would drop ~61% of it.
        latin_source=True,
        label="entl-slice",
    )
    return {"filtered": filtered.stored.to_json()}
