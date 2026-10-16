#!/usr/bin/env python3
"""Generate short English camera-path phrases, the source side of a finetune set.

The camera path is dominated by 2-8 word noun phrases on signs, labels, menus and
device screens, and no public corpus contains them, which is why every teacher we
gated mistranslates them. This writes the ENGLISH side; gen_sft.py then
translates it into the target language.

Phrases by default, because a bare word carries the ambiguity that breaks these
models ("power", "check", "right") while a phrase reads one way ("power button",
"check please", "turn right"). `--words 1-1` overrides that for signage, where
single words genuinely dominate the page -- PUSH, EXIT, ARRIVALS -- and the
sign context supplies the sense the word lacks. Run it separately and expect a
few hundred, not thousands: real one-word signage is a small closed set, unlike
the 2-8 band which has room for tens of thousands.

One file per category so a killed run resumes, and so a category that comes back
thin can be re-asked without touching the rest.

    gen_short_en.py --out out/short_en --per-category 150 --workers 6
"""

import argparse
import json
import pathlib
import re
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

from gen_sft import call_claude, strip_fences

SYS = ("You write realistic short text that appears on physical signs, labels, "
       "packaging, menus and device screens. You output JSON arrays and nothing else.")

# Ordered roughly by how often a traveller's camera meets them. Menus are split
# finely because dish names are the open-ended tail of this vocabulary.
CATEGORIES = [
    "road and traffic signs", "parking and vehicle notices", "pedestrian and crossing signs",
    "building entrance and exit signs", "restroom and facility signs",
    "fire safety and emergency equipment", "evacuation and assembly notices",
    "electrical and machinery hazard warnings", "construction and roadworks notices",
    "prohibition notices", "warning notices about surfaces and steps",
    "opening hours and closure notices", "shop and retail signs",
    "price, payment and checkout signs", "queue and service counter signs",
    "train and metro station signs", "bus stop and coach signs",
    "airport terminal and gate signs", "border, customs and immigration signs",
    "hotel and accommodation signs", "hospital and clinic department signs",
    "pharmacy and medicine counter signs", "medicine dosage instructions",
    "medicine warnings and side effects", "food allergen declarations",
    "food storage and expiry labels", "nutrition and ingredient labels",
    "cleaning product hazard labels", "cosmetic and toiletry labels",
    "menu section headings", "menu breakfast dishes", "menu appetisers and starters",
    "menu main courses", "menu side dishes", "menu desserts",
    "menu hot drinks", "menu cold drinks and alcohol",
    "cafe and takeaway counter phrases", "restaurant service phrases",
    "supermarket aisle and section signs", "market stall and produce signs",
    "appliance control labels and buttons", "phone and computer interface labels",
    "error and status messages on devices", "wifi and network notices",
    "ATM and banking machine prompts", "ticket machine prompts",
    "museum and gallery labels", "park and nature reserve signs",
    "beach and swimming notices", "gym and sports facility signs",
    "school and university signs", "office and workplace signs",
    "lift and stairwell signs", "delivery and postal notices",
    "waste and recycling bin labels", "pet and animal notices",
    "weather and road condition warnings", "tool and hardware labels",
    "clothing care and size labels",
    # Added after a gate found the teacher mistranslating exactly these: a power
    # button read as political power, tire as steering wheel, a radiator cap as a
    # hat, psi converted to the wrong unit, and a restaurant check as "verify".
    "vehicle dashboard warnings and controls",
    "car maintenance, fluid and tyre labels",
    "device power, battery and charging controls",
    "restaurant bill and payment phrases",
    "tree nut, peanut and seafood allergen notices",
    "first aid, eyewash and emergency shower instructions",
    "plumbing, gas and water valve labels",
    "left, right and directional wayfinding signs",
    "torque, pressure and measurement instructions",
    "tamper seal and packaging integrity notices",
]


def ask(model: str, category: str, n: int, exemplars: list[str],
        lo: int, hi: int) -> list[str]:
    length = (f"- EXACTLY one word each. Only words that name one thing "
              f"unambiguously on a sign.\n" if hi == 1 else
              f"- {lo} to {hi} words each.\n")
    prompt = (
        f"List {n} DISTINCT short English phrases of the kind actually printed on "
        f"real-world {category}.\n\n"
        "Rules:\n"
        + length +
        "- Each phrase must be self-contained and have ONE obvious reading out of "
        "context. Avoid phrases whose meaning flips depending on setting.\n"
        "- Real wording as printed, not a description of the sign.\n"
        "- No duplicates, no numbering, no explanations.\n"
        "- Vary the grammatical shape: noun phrases, imperatives, short statements.\n\n"
        "Examples of the register (from other categories):\n"
        + "\n".join(f"- {e}" for e in exemplars) + "\n\n"
        f"Output ONLY a JSON array of {n} strings."
    )
    resp = call_claude(model, SYS, prompt)
    out = json.loads(strip_fences(resp["result"]))
    if not isinstance(out, list):
        raise RuntimeError(f"{category}: expected a JSON array")
    return [str(x).strip() for x in out if str(x).strip()]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--model", default="sonnet")
    ap.add_argument("--per-category", type=int, default=150)
    ap.add_argument("--workers", type=int, default=6)
    ap.add_argument("--words", default="2-8", metavar="LO-HI",
                    help="word-count band to ask for. The single-word band is a "
                         "separate run because real one-word signage runs out in "
                         "the hundreds, while 2-8 has thousands of room")
    ap.add_argument("--exemplars", type=pathlib.Path,
                    help="file of example phrases to anchor the register (e.g. probes/adversarial.en)")
    args = ap.parse_args()

    lo, hi = (int(x) for x in args.words.split("-"))

    exemplars: list[str] = []
    if args.exemplars is not None:
        lines = [l.strip() for l in args.exemplars.read_text(encoding="utf-8").splitlines() if l.strip()]
        exemplars = [l for l in lines if lo <= len(l.split()) <= hi][:12] or lines[:12]

    batch_dir = args.out / "categories"
    batch_dir.mkdir(parents=True, exist_ok=True)

    todo = [c for c in CATEGORIES if not (batch_dir / (re.sub(r"\W+", "_", c) + ".json")).exists()]
    print(f"{len(CATEGORIES) - len(todo)} categories already done, {len(todo)} to fetch "
          f"via {args.model}", file=sys.stderr)

    def run(category: str) -> tuple[str, int]:
        phrases = ask(args.model, category, args.per_category, exemplars, lo, hi)
        (batch_dir / (re.sub(r"\W+", "_", category) + ".json")).write_text(
            json.dumps(phrases, ensure_ascii=False, indent=1), encoding="utf-8")
        return category, len(phrases)

    if todo:
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            futures = {pool.submit(run, c): c for c in todo}
            for f in as_completed(futures):
                category = futures[f]
                try:
                    _, n = f.result()
                    print(f"  [ok] {category}: {n}", file=sys.stderr)
                except Exception as e:
                    print(f"  [FAIL] {category}: {e}", file=sys.stderr)

    seen: dict[str, str] = {}
    for p in sorted(batch_dir.glob("*.json")):
        for phrase in json.loads(p.read_text(encoding="utf-8")):
            if lo <= len(phrase.split()) <= hi:
                seen.setdefault(phrase.lower(), phrase)

    merged = args.out / "short.en"
    merged.write_text("\n".join(seen.values()) + "\n", encoding="utf-8")
    print(f"\n{len(seen)} unique phrases -> {merged}", file=sys.stderr)


if __name__ == "__main__":
    main()
