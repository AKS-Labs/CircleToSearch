"""Lexicon post-correction for confusable Hebrew letter pairs.

A context-free CTC recognizer confuses visually-similar letters (ס/ם, ב/כ, ...).
Most such errors turn a real word into a non-word fixable by a single swap. Build a
wordlist from the corpus; for any out-of-vocab Hebrew token, try one confusable
swap and accept the most-frequent valid word. Numbers/Latin/in-vocab tokens are
left alone. This is the inference-side complement to the font fix (which handles
the styled-banner multi-error case the lexicon can't).

  rec_strips.py ... | python lex_correct.py <corpus.txt>
"""

import re
import sys
from collections import Counter

# Bidirectional confusable pairs observed in the real-photo eval.
PAIRS = ["סם", "בכ", "דר", "מח", "טמ", "ונ", "ןך", "ךף", "תה", "וי", "הח"]
SWAP: dict[str, list[str]] = {}
for a, b in PAIRS:
    SWAP.setdefault(a, []).append(b)
    SWAP.setdefault(b, []).append(a)

HEB = re.compile(r"[א-ת]")
STRIP = "\"'.,:;!?()[]/%-+&@#־׳״"


def build_vocab(path):
    c = Counter()
    for line in open(path, encoding="utf-8"):
        for tok in line.split():
            t = tok.strip(STRIP)
            if len(t) >= 2 and HEB.search(t):
                c[t] += 1
    return c


MIN_FREQ = 5  # candidate must be a reasonably common word, else leave the token alone


def correct_token(tok, vocab):
    core = tok.strip(STRIP)
    if len(core) < 2 or not HEB.search(core) or core in vocab:
        return tok
    best, best_f = None, MIN_FREQ - 1
    for i, ch in enumerate(core):
        for repl in SWAP.get(ch, []):
            f = vocab.get(core[:i] + repl + core[i + 1:], 0)
            if f > best_f:
                best, best_f = core[:i] + repl + core[i + 1:], f
    return tok.replace(core, best, 1) if best else tok


def correct_line(line, vocab):
    return " ".join(correct_token(t, vocab) for t in line.split())


if __name__ == "__main__":
    vocab = build_vocab(sys.argv[1])
    print(f"# vocab: {len(vocab)} words", file=sys.stderr)
    for line in sys.stdin:
        line = " ".join(line.split())  # normalize whitespace so diffs are real
        fixed = correct_line(line, vocab)
        if fixed != line:
            print(f"{line}\n  -> {fixed}")
        else:
            print(line)
