#!/usr/bin/env python3
"""Resumable translation-pair generator via `claude -p` (no API key needed).

Reads a source corpus, translates it in batches through the Claude CLI, and
writes one file per batch so it can be stopped and resumed at any time.
Re-running skips batches already completed and validated.

The translation spec (model, language pair, script, batch size, system prompt)
comes from a --config JSON, a small file that makes one pair reproducible and
keeps the invocation to just --src/--out; any field can be overridden on the CLI.

Sources (--src) may be:
  - a .txt file, one sentence per line (e.g. a corpus column like kd_src)
  - a .json list of strings, or of {"text": ...} objects

Example (config-driven, plain-text input):
  gen_sft.py --config configs/gen.ug2en.json --src uig.10k.txt --out out/ug2en

Config (configs/gen.ug2en.json):
  {"model": "sonnet", "from": "Uyghur", "to": "English",
   "script": "Perso-Arabic, Xinjiang usage", "batch": 200}

Outputs under --out:
  batches/batch_00000.json ...   per-batch target lists (resume unit)
  targets.json                   all targets concatenated, in order
  pairs.jsonl                    {"src":..., "tgt":...} one per line
  pairs.tsv                      src \t tgt (align.sh / finetune input)
  cost.log                       running USD-equivalent per batch
"""

import argparse
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed

DEFAULT_SYS = "You are a professional machine translation engine. Follow the output format exactly and emit nothing else."


def load_sources(p: str) -> list[str]:
    # Plain text (one sentence per line) is the default; only .json is parsed as
    # JSON. Keying on ".txt" alone rejected corpus columns like `flores.ug`.
    path = pathlib.Path(p)
    if path.suffix == ".json":
        raw = json.load(open(p, encoding="utf-8"))
        if raw and isinstance(raw[0], dict):
            return [r["text"] for r in raw]
        return list(raw)
    return [ln for ln in path.read_text(encoding="utf-8").splitlines() if ln.strip()]


def strip_fences(t: str) -> str:
    t = t.strip()
    t = re.sub(r"^```(?:json)?\s*", "", t)
    t = re.sub(r"\s*```$", "", t)
    return t.strip()


def call_claude(model: str, system: str, prompt: str, effort: str) -> dict:
    # Effort is passed explicitly so the run is reproducible, NOT as an economy:
    # measured on 200 lines, low and medium cost the same to within 1% ($0.00146
    # vs $0.00145/line), because translation does not trigger much reasoning at
    # any setting. The cost here is output tokens, and Georgian runs near one
    # token per character, so slice size is the only real lever.
    cmd = [
        "claude", "-p", "--model", model, "--effort", effort,
        "--output-format", "json",
        "--allowedTools", "", "--exclude-dynamic-system-prompt-sections",
        "--system-prompt", system, prompt,
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=1200)
    if r.returncode != 0:
        # claude -p reports its error on stdout (as JSON), not stderr, so surface both.
        detail = (r.stderr.strip() or r.stdout.strip() or "(no output)")[:400]
        raise RuntimeError(f"claude exited {r.returncode}: {detail}")
    return json.loads(r.stdout)


def call_codex(model: str, system: str, prompt: str, effort: str) -> dict:
    # `codex exec` has no system-prompt flag, so the system text becomes the first
    # block of the user prompt. Tools stay unused: read-only sandbox plus --cd into
    # an empty scratch dir means a generation that tries to touch the filesystem
    # finds nothing, and stdin is closed so nothing is appended to the prompt.
    workdir = pathlib.Path(tempfile.mkdtemp(prefix="gen_sft_codex_"))
    cmd = [
        "codex", "exec", "--model", model,
        # TOML-quoted, which is what -c expects. Bare words are accepted too --
        # the effort sweep scaled reasoning tokens 0/76/1886/17018/42412 across
        # none..xhigh with them, so they do reach the server -- but quoting is the
        # documented form and survives a stricter parser.
        "-c", f'model_reasoning_effort="{effort}"',
        # Pinned rather than inherited: a data run must not change behaviour
        # because the operator's ~/.codex/config.toml differs or was edited.
        "-c", 'service_tier="default"', "-c", "fast_default_opt_out=true",
        "--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral",
        "--cd", str(workdir), "--json", f"{system}\n\n{prompt}",
    ]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=1800,
                           stdin=subprocess.DEVNULL)
    finally:
        shutil.rmtree(workdir, ignore_errors=True)
    text, usage, err = "", {}, ""
    for line in r.stdout.splitlines():
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        item = ev.get("item", {})
        if ev.get("type") == "item.completed" and item.get("type") == "agent_message":
            text = item.get("text", "")
        elif ev.get("type") == "turn.completed":
            usage = ev.get("usage", {})
        elif ev.get("type") in ("error", "turn.failed"):
            err = json.dumps(ev)[:400]
    if not text:
        # codex reports API rejections (bad model, unsupported effort) as JSONL
        # error events, not on stderr, so the event is the useful detail.
        detail = err or r.stderr.strip()[:400] or "(no output)"
        raise RuntimeError(f"codex exited {r.returncode} without a message: {detail}")
    if usage:
        print(f"[usage] in={usage.get('input_tokens', 0)} "
              f"cached={usage.get('cached_input_tokens', 0)} "
              f"out={usage.get('output_tokens', 0)} "
              f"reasoning={usage.get('reasoning_output_tokens', 0)}",
              file=sys.stderr, flush=True)
    # codex reports token usage but no price, so there is no cost to return.
    return {"result": text, "total_cost_usd": 0.0}


BACKENDS = {"claude": call_claude, "codex": call_codex}


def translate_batch(spec: dict, srcs: list[str]) -> tuple[list[str], float]:
    script = f" ({spec['script']})" if spec.get("script") else ""
    instr = (
        f"Translate each {spec['from']} sentence in this JSON array into natural, fluent "
        f"{spec['to']}{script}. Output ONLY a JSON array of exactly {len(srcs)} "
        f"{spec['to']} strings, in the same order, one per input. No commentary, no "
        f"romanization, no markdown fences.\n\n" + json.dumps(srcs, ensure_ascii=False)
    )
    last_err = ""
    call = BACKENDS[spec["backend"]]
    for attempt in range(2):
        resp = call(
            spec["model"], spec["system"],
            instr if attempt == 0 else instr + "\n\nReturn a bare JSON array only.",
            spec["effort"],
        )
        cost = float(resp.get("total_cost_usd", 0.0))
        try:
            out = json.loads(strip_fences(resp["result"]))
        except json.JSONDecodeError as e:
            last_err = f"parse: {e}"
            continue
        if not isinstance(out, list) or len(out) != len(srcs):
            last_err = f"count: got {len(out) if isinstance(out, list) else type(out)} want {len(srcs)}"
            continue
        return [str(x) for x in out], cost
    raise RuntimeError(f"batch failed after retries ({last_err})")


def resolve_spec(a: argparse.Namespace) -> dict:
    """Config file provides defaults; any CLI flag that was set overrides it."""
    spec = {"model": None, "from": None, "to": None, "script": "", "batch": 200,
            "effort": "low", "system": DEFAULT_SYS, "backend": "claude"}
    if a.config:
        cfg = json.load(open(a.config, encoding="utf-8"))
        spec.update({k: cfg[k] for k in ("model", "from", "to", "script", "batch", "effort", "backend") if k in cfg})
        if "system_prompt" in cfg:
            spec["system"] = cfg["system_prompt"]
    for cli_key, spec_key in (("model", "model"), ("effort", "effort"),
                              ("src_lang", "from"), ("tgt_lang", "to"),
                              ("script", "script"), ("batch", "batch"), ("system_prompt", "system"),
                              ("backend", "backend")):
        v = getattr(a, cli_key)
        if v is not None:
            spec[spec_key] = v
    missing = [k for k in ("model", "from", "to") if not spec[k]]
    if missing:
        sys.exit(f"missing required spec fields (set in --config or CLI): {missing}")
    return spec


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--src", required=True, help=".txt (one per line) or .json list")
    ap.add_argument("--out", required=True)
    ap.add_argument("--config", help="JSON spec: model/from/to/script/batch/system_prompt")
    ap.add_argument("--model", help="opus | sonnet | haiku | full id (overrides config)")
    ap.add_argument("--from", dest="src_lang", help="source language name (overrides config)")
    ap.add_argument("--to", dest="tgt_lang", help="target language name (overrides config)")
    ap.add_argument("--script", help="target script/usage note (overrides config)")
    ap.add_argument("--system-prompt", dest="system_prompt", help="override the system prompt")
    ap.add_argument("--batch", type=int, help="sentences per claude call (overrides config)")
    ap.add_argument("--backend", choices=tuple(BACKENDS),
                    help="CLI that runs the model: claude (default) or codex")
    ap.add_argument("--effort", choices=("none", "minimal", "low", "medium", "high", "xhigh", "max"),
                    help="reasoning effort; default low, because translation does not "
                         "benefit from thinking and the default level is expensive")
    ap.add_argument("--workers", type=int, default=1, help="concurrent claude -p calls")
    ap.add_argument("--uniq", action=argparse.BooleanOptionalAction, default=True,
                    help="drop duplicate source sentences before generating (--no-uniq to keep)")
    ap.add_argument("--limit", type=int, default=0, help="cap total sources (0 = all)")
    ap.add_argument("--max-cost", type=float, default=0.0, metavar="USD",
                    help="stop dispatching new batches once cumulative cost reaches "
                         "this (0 = unlimited). Lets a quota-limited run end on a "
                         "batch boundary instead of losing every in-flight batch")
    a = ap.parse_args()

    spec = resolve_spec(a)
    srcs = load_sources(a.src)
    if a.uniq:
        # Duplicate inputs waste generation and produce duplicate output pairs; a
        # finetune corpus wants them gone. Order-preserving so resume stays stable.
        seen: set[str] = set()
        deduped = [s for s in srcs if not (s in seen or seen.add(s))]
        if len(deduped) != len(srcs):
            print(f"uniq: {len(srcs)} -> {len(deduped)} sources ({len(srcs)-len(deduped)} dups dropped)",
                  file=sys.stderr)
        srcs = deduped
    if a.limit:
        srcs = srcs[:a.limit]
    batch = spec["batch"]
    out = pathlib.Path(a.out)
    (out / "batches").mkdir(parents=True, exist_ok=True)
    (out / "failures").mkdir(exist_ok=True)
    print(f"{spec['from']}->{spec['to']} via {spec['backend']}/{spec['model']}, {len(srcs)} sentences, "
          f"batch {batch}, {a.workers} workers", flush=True)

    n = len(srcs)
    log_lock = threading.Lock()

    def run_batch(start: int) -> tuple[str, int, float]:
        """Translate one batch (or skip if already done). Batch files are keyed by
        the GLOBAL offset, so concurrent workers never collide and ordered assembly
        below is unaffected. Returns (status, start, cost)."""
        chunk = srcs[start:start + batch]
        bf = out / "batches" / f"batch_{start:06d}.json"
        if bf.exists():
            try:
                if len(json.load(open(bf))) == len(chunk):
                    return "skip", start, 0.0
            except Exception:
                pass
        try:
            tgts, cost = translate_batch(spec, chunk)
        except Exception as e:
            (out / "failures" / f"batch_{start:06d}.txt").write_text(str(e))
            print(f"[FAIL] {start}: {e}", file=sys.stderr, flush=True)
            return "fail", start, 0.0
        json.dump(tgts, open(bf, "w"), ensure_ascii=False)
        with log_lock:
            with open(out / "cost.log", "a") as f:
                f.write(f"{start}\t{len(chunk)}\t{cost:.4f}\n")
        return "ok", start, cost

    total_cost = 0.0
    starts = list(range(0, n, batch))
    stopped = False
    with ThreadPoolExecutor(max_workers=a.workers) as ex:
        futs = [ex.submit(run_batch, s) for s in starts]
        for fut in as_completed(futs):
            if fut.cancelled():
                continue
            status, start, cost = fut.result()
            total_cost += cost
            if status != "skip":
                print(f"[{status}] {start}-{min(start+batch,n)}  cost={cost:.3f}  cum={total_cost:.3f}", flush=True)
            # Stop DISPATCHING rather than stop working: cancel() only takes
            # futures that have not started, so the in-flight batches finish and
            # are written. Being killed instead discards every in-flight batch,
            # which on a quota-limited plan is spend with nothing to show.
            if a.max_cost and total_cost >= a.max_cost and not stopped:
                stopped = True
                pending = sum(f.cancel() for f in futs)
                print(f"\n[BUDGET] cum={total_cost:.2f} reached --max-cost {a.max_cost}; "
                      f"cancelled {pending} unstarted batches, letting in-flight ones finish. "
                      f"Re-run the same command to resume.", flush=True)

    # assemble in order; only if fully complete
    targets, pairs, complete = [], [], True
    for start in range(0, n, batch):
        bf = out / "batches" / f"batch_{start:06d}.json"
        if not bf.exists():
            complete = False
            break
        chunk = srcs[start:start + batch]
        tg = json.load(open(bf))
        targets += tg
        pairs += [{"src": s, "tgt": t} for s, t in zip(chunk, tg)]
    if complete:
        json.dump(targets, open(out / "targets.json", "w"), ensure_ascii=False)
        with open(out / "pairs.jsonl", "w") as f:
            for p in pairs:
                f.write(json.dumps(p, ensure_ascii=False) + "\n")
        with open(out / "pairs.tsv", "w", encoding="utf-8") as f:
            for p in pairs:
                s, t = p["src"].replace("\t", " "), p["tgt"].replace("\t", " ")
                f.write(f"{s}\t{t}\n")
        print(f"[DONE] {len(pairs)} pairs -> {out}/pairs.tsv  total_cost={total_cost:.3f}", flush=True)
    else:
        print(f"[PARTIAL] some batches missing; rerun to continue. total_cost={total_cost:.3f}", flush=True)


if __name__ == "__main__":
    main()
