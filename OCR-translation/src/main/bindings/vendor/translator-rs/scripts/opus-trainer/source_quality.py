#!/usr/bin/env python3
"""Score the SOURCE side of a KD draw for "is this plausible Georgian at all".

The failure this exists for is not misalignment. Where the Georgian source is
itself degraded -- machine translation of machine translation, OCR spill, subtitle
noise -- the teacher does not degrade with it. It emits fluent, confident English
carrying substituted facts (ka_findings.md S19), so a student distilled on those
pairs learns to sound authoritative exactly where it should be uncertain. Every
fluency-shaped metric moves the WRONG WAY on those lines, which is why bicleaner
and ce-filter cannot find them: both score the PAIR, and a degraded source
usually sits in a correctly-aligned pair.

So the instrument is monolingual and looks only at the source.

Emits a SCORE, never a verdict. mojibake_filter.py's docstring records why: an
absolute Georgian perplexity cut "eats transliterated place names, UI
abbreviations and loanwords", which is why that filter used a Georgian-vs-Slavic
RATIO instead. Garbled text has no such negative class to train against, so the
threshold has to be calibrated against labelled lines rather than guessed, and
this script deliberately stops one step short of choosing it.

Two independent features, reported side by side because they disagree
informatively. A rare loanword is unusual to the n-gram model but is a real word;
a garbled string is usually neither. Ranking on their disagreement is what the
calibration step is for.

    source_quality.py --clean-ref clean.ka --src kd_src --out scores.tsv
"""

import argparse
import math
import re
import sys
import unicodedata
from collections import Counter
from pathlib import Path

ORDER = 5
# Any run this long of one character is degenerate rather than emphatic. A
# 146-character run of the same letter in the pool made a decode batch loop until
# its 1800s timeout, so this is a real observed shape and not a hypothetical.
MAX_RUN = 8
LATIN = re.compile(r"[A-Za-z]")
# Punctuation with no space after it, then a letter. The other half of the
# space-damage class that frac_short catches.
GLUED = re.compile(r"[,.;:!?](?=\w)")
# Length buckets in Georgian words. The n-gram score is a MEAN PER CHARACTER, so
# short lines pay for their line-initial context and a global percentile cut is
# to first order a short-line filter: 13.13% of 1-word lines fall in the global
# bottom 1% against 0.06% of 13-25 word lines. Rank within bucket, never across.
BUCKETS = ((1, 1), (2, 3), (4, 6), (7, 12), (13, 25), (26, 10**6))


def bucket_of(n: int) -> str:
    for lo, hi in BUCKETS:
        if lo <= n <= hi:
            return f"w{lo:02d}_{hi:02d}" if hi < 10**6 else f"w{lo:02d}p"
    return "w00"


def token_shape(text: str, words: list[str], script: frozenset[str]) -> tuple[float, float]:
    """Fraction of Georgian tokens that are 1-2 chars, and glued-punctuation rate.

    Space shattering ("ნა პო ლე ონ მა" for ნაპოლეონმა) is the most common genuine
    defect in the damaged lines and is INVISIBLE to the n-gram model, which scores
    a shattered line about as well as a clean one because the character sequence
    is barely changed. It is also orthogonal to word-oddity: shattered and
    name-dense lines have near-identical odd-word rates and are separated only
    here.
    """
    toks = [t for t in text.split() if any(c in script for c in t)]
    short = sum(1 for t in toks if len(t) <= 2) / len(toks) if toks else 0.0
    glued = len(GLUED.findall(text)) / len(words) if words else 0.0
    return short, glued


def script_of(reference: list[str], coverage: float = 0.999) -> frozenset[str]:
    """The set of letters the language actually writes in, learned from clean text.

    Derived rather than configured, so this file carries no language of its own:
    hardcoding a script's Unicode ranges here is what makes a data-quality tool
    single-use, and these defects are not language-specific. Takes the smallest
    set of alphabetic characters covering `coverage` of the reference's letters,
    which drops the long tail of foreign characters in loanwords and citations
    without needing a block list.
    """
    counts = Counter(c for line in reference for c in line if c.isalpha())
    if not counts:
        raise SystemExit("clean reference contains no alphabetic characters")
    keep, seen, target = set(), 0, sum(counts.values()) * coverage
    for char, n in counts.most_common():
        keep.add(char)
        seen += n
        if seen >= target:
            break
    return frozenset(keep)


class CharNgram:
    """Interpolated character n-gram model, backing off to order 1.

    Mirrors mojibake_filter.py's trigram model at higher order: order 3 separates
    two LANGUAGES, which is a coarse question, while separating fluent Georgian
    from Georgian-shaped noise needs enough context to notice broken morphology.
    """

    def __init__(self, script: frozenset[str], order: int = ORDER) -> None:
        self.script = script
        self.order = order
        self.counts: list[Counter[str]] = [Counter() for _ in range(order + 1)]
        self.total = 0

    def __init_script__(self, script: frozenset[str]) -> None:
        self.script = script

    def _stream(self, text: str) -> str:
        return "^" * (self.order - 1) + "".join(
            c if c in self.script or c.isspace() else "~" for c in text.lower()
        )

    def train(self, texts) -> None:
        for text in texts:
            s = self._stream(text)
            for i in range(self.order - 1, len(s)):
                for n in range(1, self.order + 1):
                    self.counts[n][s[i - n + 1:i + 1]] += 1
            self.total += max(0, len(s) - self.order + 1)

    def unseen(self, text: str, n: int = 4) -> tuple[int, int]:
        """Count character n-grams never observed in the clean reference.

        Hard evidence rather than a soft score, and the reason it is worth having
        separately: `logprob` is a MEAN PER CHARACTER, so its low band is
        dominated by short lines and stratifying on it oversamples "short" rather
        than "damaged". A zero-count n-gram does not care how long the line is.

        Derived empirically instead of from hand-written phonotactic rules,
        because Georgian tolerates consonant clusters that look illegal and are
        not -- გვფრცქვნი is a real word.
        """
        stream = self._stream(text)
        hits = miss = 0
        for i in range(self.order - 1, len(stream)):
            gram = stream[i - n + 1:i + 1]
            if "~" in gram or "^" in gram:
                continue
            hits += 1
            if not self.counts[n][gram]:
                miss += 1
        return miss, hits

    def logprob(self, text: str) -> float | None:
        """Mean log-probability per scored character, or None if nothing scored."""
        s = self._stream(text)
        scored, total = 0, 0.0
        for i in range(self.order - 1, len(s)):
            if s[i] == "~":
                continue
            p, weight = 0.0, 0.0
            for n in range(self.order, 0, -1):
                gram, ctx = s[i - n + 1:i + 1], s[i - n + 1:i]
                denom = self.counts[n - 1][ctx] if n > 1 else self.total
                if denom:
                    w = 0.5 ** (self.order - n)
                    p += w * (self.counts[n][gram] + 0.1) / (denom + 10.0)
                    weight += w
            if weight:
                total += math.log(p / weight)
                scored += 1
        return total / scored if scored else None


def words_of(text: str, script: frozenset[str]) -> list[str]:
    return [w for w in re.split(r"[^\w]+", text) if w and any(c in script for c in w)]


def tier1(text: str, script: frozenset[str]) -> str | None:
    """Deterministic rejects. Returns a reason code, or None if the line passes."""
    s = text.strip()
    if not s:
        return "empty"
    if not any(c in script for c in s):
        return "no_target_script"
    if any(unicodedata.category(c) == "Cc" for c in s):
        return "control_chars"
    run, prev = 1, ""
    for c in s:
        run = run + 1 if c == prev else 1
        prev = c
        if run > MAX_RUN and not c.isspace():
            return "degenerate_run"
    letters = sum(c.isalpha() for c in s)
    if letters < len(s) * 0.4:
        return "low_letter_ratio"
    if len(LATIN.findall(s)) > letters * 0.5:
        return "latin_dominant"
    return None


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--clean-ref", required=True, type=Path, action="append",
                    help="one-per-line known-clean Georgian; repeatable. Use the "
                         "human and ui registers plus generated signage -- the same "
                         "clean set mojibake_filter.py was calibrated against")
    ap.add_argument("--src", required=True, type=Path, help="one Georgian line per line")
    ap.add_argument("--out", required=True, type=Path, help="TSV: idx, tier1, ngram, oov, words")
    ap.add_argument("--lexicon", type=Path,
                    help="optional word list; without it the oov column is computed "
                         "against the vocabulary of --clean-ref, which is small and "
                         "will over-report. Report it, do not threshold on it.")
    args = ap.parse_args()

    clean: list[str] = []
    for p in args.clean_ref:
        clean.extend(l.strip() for l in p.read_text(encoding="utf-8").splitlines() if l.strip())
    print(f"clean reference: {len(clean)} lines", file=sys.stderr)

    script = script_of(clean)
    print(f"script learned from reference: {len(script)} characters", file=sys.stderr)

    model = CharNgram(script)
    model.train(clean)
    print(f"trained order-{ORDER} model, {sum(len(c) for c in model.counts)} grams", file=sys.stderr)

    vocab = set()
    if args.lexicon is not None:
        vocab = {w.strip().lower() for w in args.lexicon.read_text(encoding="utf-8").splitlines() if w.strip()}
    for line in clean:
        vocab.update(w.lower() for w in words_of(line, script))
    print(f"vocabulary: {len(vocab)} word types", file=sys.stderr)

    reasons: Counter[str] = Counter()
    scores: list[float] = []
    with args.src.open(encoding="utf-8") as fin, args.out.open("w", encoding="utf-8") as fout:
        fout.write("idx\ttier1\tngram\toov\twords\tbucket\tfrac_short\tfrac_glued\tunseen\tunseen_n\n")
        for idx, raw in enumerate(fin):
            text = raw.rstrip("\n")
            why = tier1(text, script)
            if why is not None:
                reasons[why] += 1
                fout.write(f"{idx}\t{why}\t\t\t\t\t\t\t\t\n")
                continue
            reasons["pass"] += 1
            words = words_of(text, script)
            oov = sum(w.lower() not in vocab for w in words) / len(words) if words else 1.0
            lp = model.logprob(text)
            if lp is not None:
                scores.append(lp)
            short, glued = token_shape(text, words, script)
            ng = f"{lp:.4f}" if lp is not None else ""
            miss, seen = model.unseen(text)
            fout.write(f"{idx}\tok\t{ng}\t{oov:.4f}\t{len(words)}\t{bucket_of(len(words))}"
                       f"\t{short:.4f}\t{glued:.4f}\t{miss}\t{seen}\n")

    n = sum(reasons.values())
    print(f"\n{n} lines", file=sys.stderr)
    for why, c in reasons.most_common():
        print(f"  {why:18s} {c:>9d}  {100 * c / n:6.3f}%", file=sys.stderr)
    if scores:
        scores.sort()
        pct = [1, 5, 10, 25, 50, 75, 95]
        print("\nngram logprob percentiles (lower = less like clean Georgian):", file=sys.stderr)
        for p in pct:
            print(f"  p{p:<3d} {scores[min(len(scores) - 1, p * len(scores) // 100)]:8.4f}", file=sys.stderr)
        print("\nNo threshold is applied, and a GLOBAL percentile on ngram would be a\n"
              "short-line filter, not a quality filter. Rank within `bucket`, and\n"
              "stratify the calibration set by length or it will relearn 'short'.",
              file=sys.stderr)


if __name__ == "__main__":
    main()
