"""Turn a raw (single-script or merged) corpus into a balanced, glyph-floored training
corpus plus the matching keys.txt — one pass at the data boundary, so the corpus the
generators read and the class list the model trains on can never drift apart.

Three stages, driven entirely by a per-script generator module (gen_indic / gen_hebrew)
that exposes candidate_charset(), BASE, KEEP_SET, line_script() and synth_tail():

  1. trim    — keep = (BASE | KEEP_SET | {g : corpus_count[g] >= min_count}) & candidate.
               Drop every line bearing a non-kept glyph. This removes the dead/archaic
               CTC classes (unassigned holes, vedic marks, vocalic-L/RR, fractions) that
               only act as confusable sinks; a real letter clears min_count easily.
  2. balance — equalize line counts across scripts (line_script) so a corpus-heavy script
               (e.g. Bengali wiki+newscrawl at 2x) does not dominate the merged model.
  3. fill    — lift every kept glyph to its target count, preferring REAL corpus lines
               (coverage_lines) and synthesizing (synth_tail) only for glyphs no corpus
               supplies at all — `@`, `#`, brackets. Real text is strictly better filler:
               a glyph the corpus never supplies takes 100% of its exposure from this
               stage, so invented contexts become the model's whole definition of it, and
               Hebrew round 1 corrupted the samekh/final-mem boundary exactly that way.

Two optional module hooks, resolved with getattr so existing generators need no change:

  glyph_targets(counts, kept, floor) -> {glyph: target}. Per-glyph exposure the module
                     wants, overriding the absolute floor. An absolute floor cannot express
                     "this letter is outnumbered by a punctuation mark" — Georgian Mtavruli
                     zen sat at 4752 against `%` at 5213 while clearing a floor of 300
                     seventeen times over. The obvious generalisation, a fraction of the
                     glyph's group median, fails too: the starved classes are the majority
                     of their own group and drag its median to zero. Which classes compete,
                     and what counts as enough, is script knowledge the module holds.
  coverage_lines(glyph, corpus, rng, kept) -> real lines containing `glyph`.

  python build_corpus.py --module gen_indic --raw data/indic_corpus.txt \
      --out-corpus data/indic_corpus.bal.txt --out-keys paddle/indic_latin_dict.txt
"""

import argparse
import collections
import importlib
import os
import random
import sys


def _counts(lines: list[str]) -> collections.Counter:
    c = collections.Counter()
    for ln in lines:
        c.update(ln)
    return c


def _targets(m, kept: frozenset, counts: collections.Counter, floor: int) -> dict[str, int]:
    """The count each glyph must reach: the absolute floor, overridden by the module.

    A relative floor cannot be computed generically here. The obvious formula — a fraction
    of the glyph's group median — fails exactly where it is needed: Georgian's alphabet group
    is majority Mtavruli, prose carries almost no Mtavruli, so the median sits at zero and
    every target collapses back to the floor. Which classes compete with which, and what
    counts as enough, is script knowledge, so the module answers it directly.
    """
    target = {g: floor for g in kept if g != " "}
    override = getattr(m, "glyph_targets", None)
    if override:
        for g, n in override(counts, kept, floor).items():
            target[g] = max(target.get(g, floor), n)
    return target


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--module", required=True, help="generator module exposing the corpus API (gen_indic / gen_hebrew)")
    ap.add_argument("--raw", required=True, help="raw cleaned corpus (one sentence per line)")
    ap.add_argument("--out-corpus", required=True)
    ap.add_argument("--out-keys", required=True, help="keys.txt for training + inference (the model's class list)")
    ap.add_argument("--min-count", type=int, default=10, help="keep a non-curated glyph only if it appears >= this in the raw corpus")
    ap.add_argument("--floor", type=int, default=300, help="absolute minimum instances for every kept glyph")
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()

    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    m = importlib.import_module(args.module)
    rng = random.Random(args.seed)

    raw = [ln.rstrip("\n") for ln in open(args.raw, encoding="utf-8")]
    raw = [ln for ln in raw if ln.strip()]
    cand = m.candidate_charset()
    base = frozenset(c for c in m.BASE if c in cand)
    keep = frozenset(c for c in m.KEEP_SET if c in cand)

    # --- trim ---
    c_raw = _counts(raw)
    kept = base | keep | frozenset(ch for ch in cand if c_raw.get(ch, 0) >= args.min_count)
    allow = kept | {" "}
    trimmed = [ln for ln in raw if set(ln) <= allow]

    # --- balance ---
    buckets = collections.defaultdict(list)
    for ln in trimmed:
        buckets[m.line_script(ln)].append(ln)
    real = {k: v for k, v in buckets.items() if k is not None}
    if len(real) > 1:
        per_script = min(len(v) for v in real.values())
        balanced = [ln for v in real.values() for ln in rng.sample(v, per_script)]
        none_lines = buckets.get(None, [])  # latin/punct-only lines ride along, capped to target
        balanced += rng.sample(none_lines, min(len(none_lines), per_script))
    else:
        balanced = list(trimmed)

    # --- fill the tail ---
    coverage_lines = getattr(m, "coverage_lines", None)
    c_bal = _counts(balanced)
    target = _targets(m, kept, c_bal, args.floor)

    running = collections.Counter(c_bal)
    used: set[str] = set()
    covered: list[str] = []
    synth: list[str] = []
    # Rarest first: one real line carries many glyphs of its script, so filling the most
    # starved glyph first lets its lines pay down the rest of the tail for free. The count
    # is re-read after every line for the same reason — a static per-glyph deficit would
    # ignore what earlier lines already supplied and over-add by a wide margin.
    for g in sorted((x for x in kept if x != " "), key=lambda x: c_bal.get(x, 0)):
        if running[g] >= target[g]:
            continue
        pool = coverage_lines(g, balanced, rng, kept) if coverage_lines else []
        for line in pool:
            if running[g] >= target[g]:
                break
            if line in used:
                continue
            used.add(line)
            covered.append(line)
            running.update(line)
        guard = target[g] * 6
        while running[g] < target[g] and guard > 0:
            line = m.synth_tail(g, rng, kept)
            synth.append(line)
            running.update(line)
            guard -= 1

    out = balanced + covered + synth
    rng.shuffle(out)

    os.makedirs(os.path.dirname(os.path.abspath(args.out_corpus)) or ".", exist_ok=True)
    os.makedirs(os.path.dirname(os.path.abspath(args.out_keys)) or ".", exist_ok=True)
    with open(args.out_corpus, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")
    keys = [g for g in sorted(kept) if g != " "]  # space excluded: use_space_char appends it
    with open(args.out_keys, "w", encoding="utf-8") as f:
        f.write("\n".join(keys) + "\n")

    per_script = {k: len(v) for k, v in real.items()} if len(real) > 1 else {m.line_script(raw[0]) if raw else "?": len(trimmed)}
    print(f"raw={len(raw)} trimmed={len(trimmed)} balanced={len(balanced)} covered={len(covered)} synth={len(synth)} out={len(out)}")
    print(f"coverage stream = {100 * len(covered) / max(len(out), 1):.1f}% of the corpus")
    print(f"candidate={len(cand)} kept(keys)={len(keys)} dropped={len(cand) - len(kept) + (1 if ' ' in kept else 0)}")
    print(f"per-script (post-trim): {per_script}")
    c_out = _counts(out)
    under = sorted((c_out.get(g, 0), g) for g in keys if c_out.get(g, 0) < target[g])
    if under:
        print(f"WARNING: {len(under)} glyphs still under floor (unfillable): {[(n, hex(ord(g))) for n, g in under[:20]]}")


if __name__ == "__main__":
    main()
