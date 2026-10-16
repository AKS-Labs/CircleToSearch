#!/usr/bin/env python3
"""Judge Georgian lines for USABILITY as KD source, monolingually.

Replaces the `source` half of judge_lines.py, which measured the wrong thing
twice over.

First, it was run on monolingual text with its `en`/`ref` fields blanked, and the
judge read the empty English as evidence against the Georgian -- its own notes say
so ("incorrect case ... ; en is empty"). Blanking the fields moved the same 400
lines from 24.5% to 34.0%. Here the English does not exist in the prompt at all,
which is also the honest shape of the task: the KD input IS the Georgian.

Second, and worse, it had no severity. A Wikipedia sentence with one transposed
letter scored `garbled`, in the same bucket as keyboard mash. That is how every
source measured 12-35% "dirty" -- the number counted detectable imperfections, not
unusable lines, and a 3% typo rate in a volunteer encyclopedia is both real and
harmless. The primary verdict here is therefore `keep`, which asks whether a
competent translator would produce correct English from the line. The defect type
is recorded second, for describing what is wrong rather than deciding it.

    judge_mono.py --src sample.jsonl --out labels/mono --batch 20 --workers 4
"""

import argparse
import json
import pathlib
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

from gen_sft import call_codex, strip_fences

DEFECTS = ("none", "typo", "awkward", "shattered", "garbled", "boilerplate",
           "not_georgian", "truncated")

SYSTEM = (
    "You are a Georgian corpus annotator. You judge data, you do not translate it or "
    "improve it. Follow the output format exactly and emit nothing else."
)

INSTRUCTIONS = """\
You are auditing the SOURCE side of a Georgian->English machine-translation
training corpus. Every item is one Georgian line. There is no English anywhere in
this task and none is expected; do not treat its absence as a problem.

Give TWO verdicts per item.

1. `keep` - the decision. Would this line be USEFUL as a training example, i.e.
   could a competent human translator read it and produce a correct English
   translation?
   yes - it carries real meaning a translator could render. This includes lines
         that are imperfect: a typo that does not obscure the word, clumsy or
         translated-sounding phrasing, a fragment, a headline, a single word, a
         name, colloquial speech, profanity, an unusual register. Imperfect
         Georgian that a reader understands is USEFUL training data.
   no  - the meaning is not recoverable, or there is no meaning to translate:
         corruption that destroys words, text that is not Georgian, pure
         navigation furniture or timestamps with no sentence, or a fragment cut
         off so early that what it was saying cannot be told.

   When you are genuinely unsure, answer yes. This audit exists to find lines that
   would actively harm a translation model, not to grade Georgian prose. Most
   ordinary text, including mediocre text, is `yes`.

2. `defect` - what is imperfect, INDEPENDENT of `keep`. A line can be keep=yes
   with a defect. Use exactly one, the most prominent:
   none         - nothing wrong
   typo         - one or two wrong/missing/transposed letters, word still readable
   awkward      - grammatical but unnatural: calqued syntax, wrong case or
                  agreement, phrasing no Georgian writer would choose
   shattered    - word-internal spacing broken, or spaces missing after punctuation
   garbled      - character-level noise destroying words
   boilerplate  - navigation, timestamps, cookie notices, site furniture
   not_georgian - predominantly another language
   truncated    - cut off mid-thought

3. `note` - at most one short clause, ONLY when keep is no or defect is not none.
   Name the specific word or construction. Georgian or English, your choice.

Output STRICT JSON: an array of objects with keys `i`, `keep`, `defect`, `note`.
One object per input item, same order, same count. No prose, no code fences.
"""


def build_prompt(items: list[dict]) -> str:
    lines = [INSTRUCTIONS, "", f"Judge these {len(items)} items.", ""]
    for n, it in enumerate(items):
        lines.append(f"--- item {n} ---")
        lines.append(f"ka: {it['ka']}")
    return "\n".join(lines)


def judge_batch(items: list[dict], model: str, effort: str) -> list[dict]:
    resp = call_codex(model, SYSTEM, build_prompt(items), effort)
    out = json.loads(strip_fences(resp["result"]))
    if not isinstance(out, list) or len(out) != len(items):
        raise ValueError(f"got {len(out) if isinstance(out, list) else type(out)} verdicts, want {len(items)}")
    if [o.get("i") for o in out] != list(range(len(items))):
        raise ValueError("verdicts out of order or missing `i`")
    for o, it in zip(out, items):
        if o.get("keep") not in ("yes", "no"):
            raise ValueError(f"bad keep {o.get('keep')!r}")
        if o.get("defect") not in DEFECTS:
            raise ValueError(f"bad defect {o.get('defect')!r}")
        o["ka"], o["id"] = it["ka"], it.get("id")
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--src", required=True, type=pathlib.Path, help="jsonl with a `ka` field")
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--batch", type=int, default=20)
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--model", default="gpt-5.6-luna")
    ap.add_argument("--effort", default="low")
    args = ap.parse_args()

    rows = [json.loads(l) for l in args.src.read_text(encoding="utf-8").splitlines() if l.strip()]
    batches = [(i, rows[i:i + args.batch]) for i in range(0, len(rows), args.batch)]
    bdir = args.out / "batches"
    bdir.mkdir(parents=True, exist_ok=True)

    def run(task):
        off, items = task
        f = bdir / f"batch_{off:06d}.json"
        if f.exists():
            try:
                if len(json.loads(f.read_text(encoding="utf-8"))) == len(items):
                    return off, "cached"
            except Exception:
                pass
        for attempt in (1, 2, 3):
            try:
                got = judge_batch(items, args.model, args.effort)
                f.write_text(json.dumps(got, ensure_ascii=False), encoding="utf-8")
                return off, "ok"
            except Exception as e:
                if attempt == 3:
                    return off, f"FAIL {e}"
        return off, "FAIL"

    done = 0
    with ThreadPoolExecutor(max_workers=args.workers) as ex:
        for fut in as_completed([ex.submit(run, t) for t in batches]):
            off, status = fut.result()
            done += 1
            print(f"[{done}/{len(batches)}] {off} {status}", flush=True)

    got = []
    for off, items in batches:
        f = bdir / f"batch_{off:06d}.json"
        if f.exists():
            got.extend(json.loads(f.read_text(encoding="utf-8")))
    (args.out / "labels.jsonl").write_text(
        "\n".join(json.dumps(r, ensure_ascii=False) for r in got) + "\n", encoding="utf-8")
    keep = sum(1 for r in got if r["keep"] == "yes")
    print(f"\n{len(got)} judged, keep={keep} ({100 * keep / len(got):.2f}%)" if got else "nothing judged")


if __name__ == "__main__":
    main()
