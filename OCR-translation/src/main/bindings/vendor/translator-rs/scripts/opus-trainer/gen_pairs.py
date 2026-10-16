#!/usr/bin/env python3
"""Generate bilingual en<->XX pairs for the short/sign/label register, resumably.

The shipped en-ka student fails on the text a camera is actually pointed at:
short signs, labels, menus, dosages and device screens. It inverts senses ("Sign
out" -> "sign in"), takes the wrong sense of a polysemous English label ("Dead
end", "Serves 4", "Yield"), garbles safety wording, and drops figures ("140 over
90" losing the 90). The KD teacher is weak on the same register, so the repair
has to be generated rather than mined, and generated as PAIRS in one call -- see
pair_spec.py for why the setting has to stay attached to the row.

Language-specific material lives in a spec under configs/, so a new language is a
new spec rather than a new script.

    gen_pairs.py gen   --spec configs/gen_pairs.ka.json --out data/gen_pairs/ka \
                       --rounds 1 --workers 4
    gen_pairs.py build --spec configs/gen_pairs.ka.json --out data/gen_pairs/ka \
                       --known data/short.en-ka.v1.tsv ... \
                       --exclude probes/check.en data/eval_exclude.sha256 ... \
                       --tsv-order en,ka --judge-sample 0.05

`gen` widens the grid one round at a time and each round names what the previous
rounds already wrote, so re-running with a higher --rounds is how the set grows.
`build` is pure re-derivation from the job files: gates, exclusions and dedup can
be changed and re-applied without spending another call.
"""

from __future__ import annotations

import argparse
import collections
import json
import pathlib
import random
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

from gen_sft import call_codex, strip_fences
from pair_spec import (
    ColumnOrder,
    GateReason,
    Job,
    NumberPolicy,
    PairRow,
    Spec,
    build_jobs,
    gate,
    load_spec,
    norm,
    parse_rows,
    sha,
)

JUDGE_DEFECTS = ("none", "mistranslation", "sense_flip", "unnatural", "gloss",
                 "number", "script", "english_leak", "wrong_register", "truncated")


# ------------------------------------------------------------------ generate


def load_have(jobs_dir: pathlib.Path, spec: Spec) -> dict[str, list[str]]:
    """category -> English text already written for it, plus a global list at "".

    The global list holds the text that turns up across many categories: it is
    what a fresh ask returns first no matter which setting it is pointed at, so
    every job carries a slice of it.
    """
    per_cat: dict[str, collections.Counter] = collections.defaultdict(collections.Counter)
    spread: collections.Counter = collections.Counter()
    for path in sorted(jobs_dir.glob("*.json")):
        payload = json.loads(path.read_text(encoding="utf-8"))
        category = payload["job"]["category"]
        for row in payload["rows"]:
            per_cat[category][row["en"]] += 1
            spread[row["en"]] += 1
    have = {c: [w for w, _ in cnt.most_common()] for c, cnt in per_cat.items()}
    have[""] = [w for w, n in spread.most_common(spec.have_cap_global * 4) if n > 1]
    return have


def run_job(job: Job, spec: Spec, model: str, effort: str,
            jobs_dir: pathlib.Path) -> int:
    path = jobs_dir / f"{job.key}.json"
    last = ""
    for attempt in (0, 1):
        prompt = job.prompt if attempt == 0 else job.prompt + "\n\nReturn a bare JSON array only."
        resp = call_codex(model, spec.system, prompt, effort)
        try:
            rows, malformed = parse_rows(json.loads(strip_fences(resp["result"])), spec)
        except (json.JSONDecodeError, ValueError) as e:
            last = str(e)[:200]
            continue
        if not rows:
            last = "no usable objects in response"
            continue
        # Written aside and renamed, because `build` is run against this
        # directory while generation is still going: a reader must see either
        # the whole cell or no cell, never half a JSON file.
        tmp = path.with_suffix(".json.part")
        tmp.write_text(json.dumps({
            "job": {"key": job.key, "register": job.register, "category": job.category,
                    "form": job.form, "band": job.band, "n": job.n,
                    "numbers": str(job.numbers)},
            "gen": model, "effort": effort,
            "prompt_version": spec.prompt_version, "spec_sha": spec.sha,
            "malformed": malformed,
            "rows": [{"en": r.en, spec.code: r.target} for r in rows],
        }, ensure_ascii=False), encoding="utf-8")
        tmp.rename(path)
        return len(rows)
    raise RuntimeError(f"batch failed after retry ({last})")


def cmd_gen(a: argparse.Namespace) -> None:
    spec = load_spec(a.spec)
    jobs_dir = a.out / "jobs"
    jobs_dir.mkdir(parents=True, exist_ok=True)

    have = load_have(jobs_dir, spec)
    written = sum(len(v) for k, v in have.items() if k)
    print(f"spec {spec.language}/{spec.code} sha={spec.sha} prompt={spec.prompt_version}; "
          f"{written} rows already written across {max(len(have) - 1, 0)} categories",
          flush=True)

    jobs = build_jobs(
        spec, a.rounds, have,
        registers=[x for x in a.registers.split(",") if x],
        forms=[x for x in a.forms.split(",") if x],
        bands=[x for x in a.bands.split(",") if x],
        per_cell=a.per_cell,
    )
    if a.shuffle_seed:
        random.Random(a.shuffle_seed).shuffle(jobs)
    shard_i, _, shard_n = a.shard.partition("/")
    shard_i, shard_n = int(shard_i), int(shard_n or 1)
    if shard_n > 1:
        jobs = [j for k, j in enumerate(jobs) if k % shard_n == shard_i]
    todo = [j for j in jobs if not (jobs_dir / f"{j.key}.json").exists()]
    if a.limit:
        todo = todo[: a.limit]
    print(f"{len(jobs) - len(todo)} of {len(jobs)} cells done, {len(todo)} to fetch "
          f"({sum(j.n for j in todo)} rows asked) via {a.model}/{a.effort}, "
          f"{a.workers} workers", flush=True)

    ok = fail = got = 0
    with ThreadPoolExecutor(max_workers=a.workers) as pool:
        futs = {pool.submit(run_job, j, spec, a.model, a.effort, jobs_dir): j for j in todo}
        for f in as_completed(futs):
            job = futs[f]
            try:
                n = f.result()
                ok += 1
                got += n
                print(f"  [ok] {job.key}: {n}  ({ok + fail}/{len(todo)}, {got} rows)", flush=True)
            except Exception as e:
                fail += 1
                print(f"  [FAIL] {job.key}: {str(e)[:300]}", flush=True)
    print(f"\n{ok} cells ok ({got} rows), {fail} failed. Re-run to retry failures.",
          flush=True)


# --------------------------------------------------------------------- build


def load_known(paths: list[pathlib.Path]) -> set[str]:
    """Normalised text already in the corpus, both columns in one set.

    One set rather than one per language: a normalised English string and a
    normalised Georgian string never collide, so the column order of each TSV
    stops mattering and there is no per-file flag to get wrong.
    """
    known: set[str] = set()
    for path in paths:
        with path.open(encoding="utf-8") as f:
            for line in f:
                for col in line.rstrip("\n").split("\t"):
                    if col.strip():
                        known.add(norm(col))
    return known


def load_exclusions(paths: list[pathlib.Path]) -> tuple[set[str], set[str]]:
    """(sha256 digests, normalised text) that must never enter the training set.

    A `.sha256` file is a digest list of held-out English and is compared as
    stored; any other file is text whose lines are held out both as digests and
    normalised, because an eval line that comes back with different casing or a
    stripped full stop is the same contamination.
    """
    digests: set[str] = set()
    text: set[str] = set()
    for path in paths:
        if path.suffix == ".sha256":
            digests |= set(path.read_text(encoding="utf-8").split())
            continue
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            digests.add(sha(line))
            text.add(norm(line))
    return digests, text


def cmd_build(a: argparse.Namespace) -> None:
    spec = load_spec(a.spec)
    if a.tsv_order not in (f"en,{spec.code}", f"{spec.code},en"):
        sys.exit(f"--tsv-order must be en,{spec.code} or {spec.code},en")
    order = (ColumnOrder.EN_FIRST if a.tsv_order == f"en,{spec.code}"
             else ColumnOrder.TARGET_FIRST)

    known = load_known(a.known)
    ex_digests, ex_text = load_exclusions(a.exclude)
    print(f"{len(known)} known normalised strings, "
          f"{len(ex_digests)} eval digests + {len(ex_text)} eval lines held out",
          flush=True)

    stats: collections.Counter = collections.Counter()
    # Kept, raw and duplicate counts per register: the duplicate rate is the
    # signal for whether another round is still buying rows in that register or
    # only re-asking for what it already holds.
    per_register: collections.Counter = collections.Counter()
    raw_register: collections.Counter = collections.Counter()
    dup_register: collections.Counter = collections.Counter()
    seen: set[str] = set()
    kept: list[dict] = []
    for path in sorted((a.out / "jobs").glob("*.json")):
        payload = json.loads(path.read_text(encoding="utf-8"))
        job = payload["job"]
        band = spec.bands[job["band"]]
        numbers = NumberPolicy(job["numbers"])
        stats["malformed"] += payload["malformed"]
        for raw in payload["rows"]:
            stats["raw"] += 1
            raw_register[job["register"]] += 1
            row = PairRow(en=raw["en"], target=raw[spec.code])
            reason = gate(spec, row, band, numbers)
            if reason is not None:
                stats[f"gate:{reason}"] += 1
                continue
            keys = (norm(row.en), norm(row.target))
            if any(sha(c) in ex_digests or k in ex_text
                   for c, k in ((row.en, keys[0]), (row.target, keys[1]))):
                stats["eval_excluded"] += 1
                continue
            if any(k in known for k in keys):
                stats["dup_known"] += 1
                dup_register[job["register"]] += 1
                continue
            if any(k in seen for k in keys):
                stats["dup_run"] += 1
                dup_register[job["register"]] += 1
                continue
            seen.update(keys)
            kept.append({
                "en": row.en, spec.code: row.target,
                "register": job["register"], "category": job["category"],
                "form": job["form"], "band": job["band"],
                "gen": payload["gen"], "effort": payload["effort"],
                "prompt_version": payload["prompt_version"],
                "spec_sha": payload["spec_sha"],
            })
            per_register[job["register"]] += 1

    if not stats["raw"]:
        sys.exit(f"no rows under {a.out / 'jobs'}; run `gen` first")

    jsonl = a.out / "pairs.jsonl"
    with jsonl.open("w", encoding="utf-8") as f:
        for row in kept:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")
    tsv = a.out / f"pairs.{a.tsv_order.replace(',', '-')}.tsv"
    with tsv.open("w", encoding="utf-8") as f:
        for row in kept:
            left, right = ((row["en"], row[spec.code]) if order is ColumnOrder.EN_FIRST
                           else (row[spec.code], row["en"]))
            f.write(f"{left}\t{right}\n")

    print(f"\nraw {stats['raw']}  (malformed rows dropped at gen: {stats['malformed']})")
    for reason in GateReason:
        if stats[f"gate:{reason}"]:
            print(f"  gate {reason:16s} -{stats[f'gate:{reason}']:>6}")
    print(f"  {'eval excluded':21s} -{stats['eval_excluded']:>6}")
    print(f"  {'dup vs known':21s} -{stats['dup_known']:>6}")
    print(f"  {'dup within run':21s} -{stats['dup_run']:>6}")
    print(f"  {'KEPT':21s}  {len(kept):>6}  "
          f"({100 * len(kept) / stats['raw']:.1f}% of raw)")
    print(f"\n{'register':12s} {'kept':>7} {'raw':>7} {'dup':>7}  dup rate")
    for reg, n in per_register.most_common():
        dup = dup_register[reg]
        print(f"{reg:12s} {n:>7} {raw_register[reg]:>7} {dup:>7}  "
              f"{100 * dup / raw_register[reg]:>5.1f}%")
    print(f"\n-> {jsonl}\n-> {tsv}")

    if a.judge_sample > 0 and kept:
        judge(kept, spec, a)


# --------------------------------------------------------------------- judge


JUDGE_SYSTEM = (
    "You are a bilingual corpus annotator. You judge data, you do not translate it "
    "or improve it. Follow the output format exactly and emit nothing else."
)

JUDGE_INSTRUCTIONS = """\
You are auditing generated {language}<->English training pairs for a translation
model. Each item is one pair: text as it would be printed on a sign, label, menu,
package or device screen.

Give TWO verdicts per item.

1. `ok` - the decision. Would a competent bilingual reader accept this pair as a
   correct, usable training example?
   yes - the two sides mean the same thing in the same setting, and each side is
         what that language would really print. Terse label style, a missing
         article, an unusual but plausible sign: still yes.
   no  - the meaning differs, is inverted or is a neighbouring sense; a figure
         differs; one side is not what that language prints there; one side is a
         grammar gloss rather than a sign; either side is broken text.

2. `defect` - what is wrong, INDEPENDENT of `ok`. Exactly one, the most prominent:
   none          - nothing wrong
   mistranslation- the sides do not mean the same thing
   sense_flip    - the sides mean opposite or neighbouring senses
   unnatural     - understandable but not how that language writes it here
   gloss         - one side renders the other's grammar instead of the notice
   number        - a figure, unit, time or date differs between the sides
   script        - wrong script, wrong casing system, or broken characters
   english_leak  - English left untranslated inside the {language}
   wrong_register- prose where a label belongs, or a description of a sign
   truncated     - cut off mid-thought

Output STRICT JSON: an array of objects with keys `i`, `ok`, `defect`. One object
per input item, same order, same count. No prose, no code fences.
"""


def judge_batch(items: list[dict], spec: Spec, model: str, effort: str) -> list[dict]:
    lines = [JUDGE_INSTRUCTIONS.format(language=spec.language), "",
             f"Judge these {len(items)} pairs.", ""]
    for n, it in enumerate(items):
        lines += [f"--- item {n} ---", f"en: {it['en']}", f"{spec.code}: {it[spec.code]}"]
    resp = call_codex(model, JUDGE_SYSTEM, "\n".join(lines), effort)
    out = json.loads(strip_fences(resp["result"]))
    if not isinstance(out, list) or len(out) != len(items):
        raise ValueError(f"got {len(out) if isinstance(out, list) else type(out)} verdicts, "
                         f"want {len(items)}")
    if [o.get("i") for o in out] != list(range(len(items))):
        raise ValueError("verdicts out of order or missing `i`")
    for o, it in zip(out, items):
        if o.get("ok") not in ("yes", "no"):
            raise ValueError(f"bad ok {o.get('ok')!r}")
        if o.get("defect") not in JUDGE_DEFECTS:
            raise ValueError(f"bad defect {o.get('defect')!r}")
        o["en"], o[spec.code] = it["en"], it[spec.code]
        o["register"], o["form"] = it["register"], it["form"]
    return out


def judge(kept: list[dict], spec: Spec, a: argparse.Namespace) -> None:
    rng = random.Random(a.judge_seed)
    sample = rng.sample(kept, max(1, round(len(kept) * a.judge_sample)))
    batches = [(i, sample[i:i + a.judge_batch]) for i in range(0, len(sample), a.judge_batch)]
    bdir = a.out / "judge" / "batches"
    bdir.mkdir(parents=True, exist_ok=True)
    print(f"\njudging {len(sample)} of {len(kept)} kept rows "
          f"({100 * a.judge_sample:.0f}%) in {len(batches)} batches", flush=True)

    def run(task: tuple[int, list[dict]]) -> tuple[int, str]:
        off, items = task
        f = bdir / f"batch_{off:06d}.json"
        if f.exists() and len(json.loads(f.read_text(encoding="utf-8"))) == len(items):
            return off, "cached"
        for attempt in (1, 2, 3):
            try:
                got = judge_batch(items, spec, a.model, a.effort)
                f.write_text(json.dumps(got, ensure_ascii=False), encoding="utf-8")
                return off, "ok"
            except Exception as e:
                if attempt == 3:
                    return off, f"FAIL {str(e)[:200]}"
        return off, "FAIL"

    done = 0
    with ThreadPoolExecutor(max_workers=a.judge_workers) as pool:
        for fut in as_completed([pool.submit(run, t) for t in batches]):
            off, status = fut.result()
            done += 1
            print(f"  [{done}/{len(batches)}] {off} {status}", flush=True)

    labels = []
    for off, _ in batches:
        f = bdir / f"batch_{off:06d}.json"
        if f.exists():
            labels.extend(json.loads(f.read_text(encoding="utf-8")))
    out = a.out / "judge" / "labels.jsonl"
    out.write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in labels),
                   encoding="utf-8")
    if not labels:
        print("no verdicts returned")
        return
    good = sum(1 for r in labels if r["ok"] == "yes")
    print(f"\njudged {len(labels)}: ok={good} ({100 * good / len(labels):.1f}%) -> {out}")
    defects = collections.Counter(r["defect"] for r in labels if r["defect"] != "none")
    for d, n in defects.most_common():
        print(f"  {d:16s} {n:>5} ({100 * n / len(labels):.1f}%)")
    by_reg = collections.Counter(r["register"] for r in labels if r["ok"] == "no")
    for reg, n in by_reg.most_common():
        print(f"  rejected in {reg:20s} {n:>5}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    g = sub.add_parser("gen", help="fetch cells of the register grid")
    g.add_argument("--spec", required=True, type=pathlib.Path)
    g.add_argument("--out", required=True, type=pathlib.Path)
    g.add_argument("--model", default="gpt-5.6-luna")
    g.add_argument("--effort", default="low")
    g.add_argument("--workers", type=int, default=4)
    g.add_argument("--rounds", type=int, default=1,
                   help="how many times each cell is asked; a later round names "
                        "what the earlier ones wrote, so raise it to widen the set")
    g.add_argument("--per-cell", type=int, default=0, help="override the spec's rows per cell")
    g.add_argument("--registers", default="", help="comma-separated subset")
    g.add_argument("--forms", default="", help="comma-separated subset")
    g.add_argument("--bands", default="", help="comma-separated subset")
    g.add_argument("--limit", type=int, default=0, help="cap cells fetched this run")
    # Two processes on one job directory would otherwise walk the same todo list
    # in the same order and spend the second process re-fetching what the first
    # is already fetching. Sharding by index makes the split disjoint.
    g.add_argument("--shard", default="0/1", help="i/n: take cells where index %% n == i")
    # Cells are built register by register, so a run stopped short covers the
    # first categories at every form and the last ones not at all. Shuffling
    # makes any prefix a spread across the whole grid instead.
    g.add_argument("--shuffle-seed", type=int, default=0)
    g.set_defaults(func=cmd_gen)

    b = sub.add_parser("build", help="gate, exclude, dedup and emit the corpus")
    b.add_argument("--spec", required=True, type=pathlib.Path)
    b.add_argument("--out", required=True, type=pathlib.Path)
    b.add_argument("--known", nargs="*", type=pathlib.Path, default=[],
                   help="existing TSV corpora; a row matching either column is dropped")
    b.add_argument("--exclude", nargs="*", type=pathlib.Path, default=[],
                   help="held-out eval: .sha256 digest lists, or text files")
    b.add_argument("--tsv-order", default="en,ka", help="en,<code> or <code>,en")
    b.add_argument("--judge-sample", type=float, default=0.0,
                   help="fraction of kept rows to send through the bilingual judge")
    b.add_argument("--judge-seed", type=int, default=42)
    b.add_argument("--judge-batch", type=int, default=20)
    b.add_argument("--judge-workers", type=int, default=4)
    b.add_argument("--model", default="gpt-5.6-luna")
    b.add_argument("--effort", default="low")
    b.set_defaults(func=cmd_build)

    a = ap.parse_args()
    a.func(a)


if __name__ == "__main__":
    main()
