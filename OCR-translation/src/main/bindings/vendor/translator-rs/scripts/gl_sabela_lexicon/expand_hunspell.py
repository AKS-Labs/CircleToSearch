#!/usr/bin/env python3
"""Depth-1 expansion of a hunspell dict (FLAG num) into surface forms.

Applies each stem's SFX/PFX rules one level (the core inflectional paradigm:
plurals, verb conjugations, etc.) and the PFX×SFX cross product where the group
allows it. Continuation flags on produced affixes are intentionally NOT followed
— in Galician they generate enclitic-pronoun chains (cantándollelo…) that balloon
into millions of rare forms better left to the runtime g2p fallback.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

WORD_RE = re.compile(r"^[A-Za-záéíóúüñçÁÉÍÓÚÜÑÇ·]{2,}$")


def parse_aff(path: Path):
    groups: dict[tuple[str, str], dict] = {}
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    i = 0
    while i < len(lines):
        parts = lines[i].split()
        if parts and parts[0] in ("SFX", "PFX") and len(parts) >= 4 and parts[2] in ("Y", "N"):
            kind, flag, cross, count = parts[0], parts[1], parts[2] == "Y", int(parts[3])
            rules = []
            for j in range(1, count + 1):
                r = lines[i + j].split()
                # KIND flag strip affix cond [morph...]
                strip = r[2]
                affix = r[3].split("/")[0]  # drop continuation flags
                cond = r[4] if len(r) > 4 else "."
                rules.append((strip, affix, cond))
            groups[(kind, flag)] = {"cross": cross, "rules": rules}
            i += count + 1
        else:
            i += 1
    return groups


def apply_sfx(word, strip, affix, cond):
    if not re.search(cond + "$", word):
        return None
    if strip != "0":
        if not word.endswith(strip):
            return None
        base = word[: -len(strip)]
    else:
        base = word
    return base + (affix if affix != "0" else "")


def apply_pfx(word, strip, affix, cond):
    if not re.match(cond, word):
        return None
    if strip != "0":
        if not word.startswith(strip):
            return None
        base = word[len(strip):]
    else:
        base = word
    return (affix if affix != "0" else "") + base


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--aff", type=Path, required=True)
    ap.add_argument("--dic", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    groups = parse_aff(args.aff)
    out: set[str] = set()

    dic_lines = args.dic.read_text(encoding="utf-8", errors="replace").splitlines()[1:]
    for line in dic_lines:
        token = line.split(None, 1)[0] if line.strip() else ""
        if not token or "\\" in token:
            continue
        if "/" in token:
            word, flagstr = token.split("/", 1)
            flags = flagstr.split(",")
        else:
            word, flags = token, []
        if not WORD_RE.match(word):
            continue
        out.add(word)

        sfx_forms = []
        for f in flags:
            g = groups.get(("SFX", f))
            if not g:
                continue
            for strip, affix, cond in g["rules"]:
                form = apply_sfx(word, strip, affix, cond)
                if form and WORD_RE.match(form):
                    out.add(form)
                    if g["cross"]:
                        sfx_forms.append(form)

        for f in flags:
            g = groups.get(("PFX", f))
            if not g:
                continue
            for strip, affix, cond in g["rules"]:
                form = apply_pfx(word, strip, affix, cond)
                if form and WORD_RE.match(form):
                    out.add(form)
                    if g["cross"]:
                        for s in sfx_forms:
                            cp = apply_pfx(s, strip, affix, cond)
                            if cp and WORD_RE.match(cp):
                                out.add(cp)

    args.out.write_text("\n".join(sorted(out)) + "\n", encoding="utf-8")
    print(f"expanded to {len(out)} surface forms -> {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
