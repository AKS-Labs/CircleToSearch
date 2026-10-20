#!/usr/bin/env python3
"""Phase 1: build cleaned en<->X parallel bitext + joint SentencePiece vocab.

Resolves OPUS moses resources for the pair (via `opus_get -l`), downloads every
corpus carrying a REGISTER (registers.py — that table is the allowlist), streams
each zip through cleaning (Moses de-escape, subtitle/markup artifact strip,
wiki-boilerplate fix, alpha/word-ratio + num/url mismatch filters ported from
mozilla/translations' OpusCleaner, length/ratio filters, fastText language ID on
both sides, then the register's own extra filters), deduplicates ACROSS
registers by precedence, and trains a joint unigram SPM. Designed to run on a
scratch box: raw shards are deleted after cleaning and everything is
gzip-streamed, so peak disk stays small.

Outputs under <out-dir> (the STEP's artifacts, complete — running this script
is running the whole step, by design; see finish()):
    pool.tsv            2-col src \t tgt, every register, pairing preserved
    pool.<register>.tsv  the same split by register, always all five
    vocab.spm           joint SPM, named for marian's extension sniffing

The per-register split is what lets the KD sampler hit absolute targets instead
of drawing proportionally, which is how UI text disappeared from the en->tl
corpus entirely (~4k lines of 10M against 63.5M lines of NLLB).

Setup (on the box):
    python3 -m venv venv
    ./venv/bin/pip install opustools sentencepiece fasttext-wheel "numpy<2"
    curl -O https://dl.fbaipublicfiles.com/fasttext/supervised-models/lid.176.ftz
"""

from __future__ import annotations

import argparse
import gzip
import os
import re
import shutil
import subprocess
import sys
import time
import unicodedata
import zipfile
from collections import deque
from multiprocessing import Pool
from pathlib import Path

import fasttext

from registers import PRECEDENCE, REGISTER, Register, apply_extra
URL_RE = re.compile(r"(https://\S+/OPUS-([^/]+)/\S+/moses/\S+\.txt\.zip)")
# opus_get prints this to stdout and exits 0 when the API call fails.
API_FAIL = "Unable to retrieve the data from"


def _opus_list(venv_bin: Path, src: str, tgt: str) -> tuple[dict[str, str], bool]:
    out = subprocess.run(
        [str(venv_bin / "opus_get"), "-s", src, "-t", tgt, "-p", "moses", "-l"],
        capture_output=True, text=True,
    )
    seen: dict[str, str] = {}
    for line in out.stdout.splitlines():
        m = URL_RE.search(line)
        if not m:
            continue
        url, name = m.group(1), m.group(2)
        if name in REGISTER and name not in seen:
            seen[name] = url
    return seen, API_FAIL in out.stdout


def resolve_urls(venv_bin: Path, src: str, tgt: str, tries: int = 25, wait: int = 45) -> list[tuple[str, str]]:
    """Allowlisted (name, url) pairs for a pair, or [] if OPUS genuinely has none.

    opus.nlpl.eu's API is intermittently overloaded: it serves a request now and
    then and RESETs the connection otherwise (measured 2026-07-16 — ~2 successes
    in ~12 calls over 45min, from both a container and the host, TLS completing
    before the reset). opus_get reports that failure on STDOUT and still exits 0,
    so a reset and a pair with no corpora both arrive here as an empty list —
    which is how a transient failure became "no OPUS moses corpora resolved for
    en-ug" and killed a prep run 135s in.

    Retries are a FIXED interval, not exponential backoff: the failure is not a
    cooldown we can wait out (a 30-min quiet gap still failed), it is a per-call
    coin flip, so what matters is the NUMBER of independent attempts, not the
    spacing. Only the distinguishable failure is retried, and an exhausted retry
    raises rather than returning a misleading [].
    """
    for attempt in range(tries):
        urls, api_failed = _opus_list(venv_bin, src, tgt)
        if urls:
            return list(urls.items())
        if not api_failed:
            return []  # the API answered; this pair really has nothing allowlisted
        if attempt < tries - 1:
            print(f"opus API unavailable; retry {attempt + 1}/{tries - 1} in {wait}s",
                  file=sys.stderr)
            time.sleep(wait)
    raise RuntimeError(
        f"opus API kept failing for {src}-{tgt} across {tries} tries / ~{tries * wait // 60}min; "
        "this is NOT the same as the pair having no corpora"
    )


def members(zf: zipfile.ZipFile, ext: str) -> str:
    hits = [n for n in zf.namelist() if n.endswith("." + ext)]
    if len(hits) != 1:
        raise ValueError(f"expected one *.{ext} member, got {hits}")
    return hits[0]


# Subtitle/markup artifacts (OpenSubtitles ♪ lyric markers, [Applause]/[Music]
# annotations, <i> tags) leak into the student and get emitted at inference
# ("Kumusta kayo" -> "♪ How are you ♪"). Strip them from the data here rather
# than filtering the model output at runtime.
ARTIFACTS = re.compile(r"[♩♪♫♬♭♮♯]|\[[^\]]{0,40}\]|</?[a-zA-Z][^>]*>")


def strip_artifacts(s: str) -> str:
    return re.sub(r"\s+", " ", ARTIFACTS.sub(" ", s)).strip()


# The filters below are ported from mozilla/translations' OpusCleaner chain
# (hplt-project/OpusCleaner). Mined OPUS data is noisier than we were treating
# it: escaped markup, wiki boilerplate, mostly-symbol junk lines, and pairs
# whose numbers/URLs don't line up (a misalignment tell). Cleaning the corpus
# is orthogonal to teacher quality — the teacher only fixes the target side.

# Moses special-char de-escaping. &amp; must be last so "&amp;lt;" un-escapes to
# "&lt;" rather than "<".
DEESCAPE = [
    ("&bar;", "|"), ("&#124;", "|"), ("&lt;", "<"), ("&gt;", ">"),
    ("&bra;", "["), ("&ket;", "]"), ("&quot;", '"'), ("&apos;", "'"),
    ("&#91;", "["), ("&#93;", "]"), ("&#39;", "'"), ("&nbsp;", " "),
    ("&amp;", "&"),
]
FOOTNOTE = re.compile(r"\[[0-9]+\]")
WIKILINK = re.compile(r"\[\[(?:.+?\|)?(.+?)\]\]")
HEADING = re.compile(r"(==+)(.+?)\1")
WIKI_EMPHASIS = re.compile(r"'{2,5}")
WIKI_CODE = re.compile(r"\.mw-parser-output")
URL = re.compile(
    r"https?://(?:www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b"
    r"[-a-zA-Z0-9()@:%_+.~#?&/=]*"
)
NUM_EXPR = re.compile(r"""
    (?:
        (?P<sign>(?:(?<=\s)|^)[-+])   # leading sign only when detached from a word
        |\b
    )
    (?:0*)                            # ignore leading zeros when comparing
    (?P<value>\d+(?:[.,:]\d+)*)
    \b
""", re.X)


def deescape(s: str) -> str:
    for esc, ch in DEESCAPE:
        s = s.replace(esc, ch)
    return s


def fix_wiki(s: str) -> str:
    s = FOOTNOTE.sub("", s)
    s = WIKILINK.sub(r"\1", s)
    s = HEADING.sub("", s)
    # ''italic'' / '''bold''' emphasis. Here rather than in the UI filter because
    # it is wiki markup, and wikimedia/WikiMatrix carry it too — 159 lines reached
    # pool.ui as "'''Error:''' no page title was specified."
    return WIKI_EMPHASIS.sub("", s)


def alpha_ratio_ok(s: str, word_ratio: float, alpha_ratio: float) -> bool:
    toks = s.split()
    if not toks:
        return False
    lettered = sum(1 for t in toks if any(unicodedata.category(c)[0] == "L" for c in t))
    if lettered / len(toks) < word_ratio:
        return False
    nonspace = len(s) - s.count(" ")
    letters = sum(1 for c in s if unicodedata.category(c)[0] == "L")
    return letters / nonspace >= alpha_ratio


def _nums(s: str) -> set[str]:
    return {(m["sign"] or "") + re.sub(r"[^\d]+", "*", m["value"]) for m in NUM_EXPR.finditer(s)}


def num_mismatch_ok(s: str, t: str, ratio: float) -> bool:
    a, b = _nums(s), _nums(t)
    if not a and not b:
        return True
    return (len(a & b) + 1) / (len(a ^ b) + 1) >= ratio


def url_mismatch_ok(s: str, t: str) -> bool:
    return set(URL.findall(s)) == set(URL.findall(t))


def max_word_ok(s: str, t: str, limit: int) -> bool:
    return not any(len(tok) > limit for field in (s, t) for tok in field.split(" "))


# A DECODE-HANG GUARD, not a quality rule, and the threshold is set accordingly.
# One 146-character run of a single letter made a vLLM decode batch loop until its
# 1800s timeout, taking 100 lines of KD with it. At a 9-character threshold the
# rule was only 22% precise against labelled data -- 32 of 41 hits were ordinary
# expressive text -- so it is set well above the length any emphasis reaches and
# only catches the pathological case it exists for.
DEGENERATE_RUN = re.compile(r"(\w)\1{15,}")
PUNCT = " \t.,;:!?\"'()[]{}<>«»„“”‘’-–—…"


# A Latin letter touching a non-Latin letter with nothing between them. Measured
# against 2,000 judged lines this fired on 0 of 1,373 clean ones while catching
# character-corrupted and script-mixed rows ("სანახავდName"). A case suffix
# attached to a Latin brand is normal and keeps its hyphen ("iPhone-ის"), which
# is why the rule requires DIRECT adjacency. On a Latin-script corpus it is inert.
SCRIPT_CLASH = re.compile(r"[A-Za-z][^\W\dA-Za-z_]|[^\W\dA-Za-z_][A-Za-z]")


def mixed_script(text: str) -> bool:
    return SCRIPT_CLASH.search(text) is not None


def corrupt_encoding(text: str) -> bool:
    """Decode damage: replacement characters, or C0/C1 controls in running text.

    The single highest-precision quality rule we have measured: 0 false positives
    on 1,373 judged-clean lines while catching 37% of the character-corrupted
    ones. Language-independent, because a replacement character is evidence of a
    decode that already failed.
    """
    return "\ufffd" in text or any(unicodedata.category(c) == "Cc" for c in text)


def degenerate(text: str) -> bool:
    if DEGENERATE_RUN.search(text):
        return True
    # A short PHRASE repeating back to back. Decoders fall into this loop
    # ("the All-compassionate, the All-compassionate, the All-compassionate")
    # and mined corpora carry it from the same failure upstream. Matching on
    # adjacent identical WORDS misses it, because the repeating unit there is a
    # bigram. A single word needs a longer run to qualify: subtitle dialogue
    # legitimately says "No, no, no!", and three is well inside normal speech.
    # Compared with surrounding punctuation stripped: the repeating unit is
    # usually punctuated on every copy but the last, so "A, A, A" is three
    # repeats of one word and not two of a bigram.
    words = [w.strip(PUNCT).lower() for w in text.split()]
    words = [w for w in words if w]
    for size, need in ((1, 5), (2, 3), (3, 3), (4, 3)):
        for i in range(len(words) - size * need + 1):
            unit = words[i:i + size]
            if all(words[i + k * size:i + (k + 1) * size] == unit for k in range(1, need)):
                return True
    return False


def clean_pair(s_raw: str, t_raw: str, args) -> tuple[str, str] | None:
    """All non-langid filtering + transforms. Returns the cleaned pair, or None
    to drop it. Pure and picklable-input, so it runs in worker processes."""
    s = deescape(s_raw.replace("\t", " "))
    t = deescape(t_raw.replace("\t", " "))
    if WIKI_CODE.search(s) or WIKI_CODE.search(t):
        return None
    s = strip_artifacts(fix_wiki(s))
    t = strip_artifacts(fix_wiki(t))
    if not s or not t or s == t:
        return None
    if degenerate(s) or degenerate(t):
        return None
    if corrupt_encoding(s) or corrupt_encoding(t):
        return None
    if mixed_script(s) or mixed_script(t):
        return None
    if not max_word_ok(s, t, args.max_word_len):
        return None
    sw, tw = s.split(), t.split()
    if not (args.min_tokens <= len(sw) <= args.max_tokens):
        return None
    if not (args.min_tokens <= len(tw) <= args.max_tokens):
        return None
    if max(len(sw), len(tw)) / min(len(sw), len(tw)) > args.ratio:
        return None
    if max(len(s), len(t)) > args.max_chars:
        return None
    if not alpha_ratio_ok(s, args.word_ratio, args.alpha_ratio):
        return None
    if not alpha_ratio_ok(t, args.word_ratio, args.alpha_ratio):
        return None
    if not num_mismatch_ok(s, t, args.num_ratio):
        return None
    if not url_mismatch_ok(s, t):
        return None
    return s, t


# One giant corpus (NLLB is ~6x all others combined) dominates the clean, so we
# shard by LINE across a worker pool, not by corpus. Each worker loads its own
# fastText model; the parent only reads/decompresses zips and writes survivors.
_LID = None
_WORKER_ARGS = None


def _init_worker(lid_model: str, args) -> None:
    global _LID, _WORKER_ARGS
    _LID = fasttext.load_model(lid_model)
    _WORKER_ARGS = args


def _clean_batch(task: tuple[str, list]) -> tuple[str, int, list[str]]:
    name, rows = task
    args = _WORKER_ARGS
    register = REGISTER[name]
    src_buf: list[str] = []
    tgt_buf: list[str] = []
    for bs, bt in rows:
        pair = clean_pair(bs.decode("utf-8", "replace"), bt.decode("utf-8", "replace"), args)
        if pair:
            pair = apply_extra(register, *pair)
        if pair:
            src_buf.append(pair[0])
            tgt_buf.append(pair[1])
    kept: list[str] = []
    if src_buf:
        sl, sp = _LID.predict(src_buf)
        tl, tp = _LID.predict(tgt_buf)
        want_s, want_t = "__label__" + args.src, "__label__" + args.lang
        for s, t, slab, spr, tlab, tpr in zip(src_buf, tgt_buf, sl, sp, tl, tp):
            # fastText LID is unreliable on 1-2 word lines and silently drops
            # nearly all of them, which is what starves the model of short-input
            # coverage; pass short pairs through on the non-LID filters alone.
            short = (
                args.lid_bypass_tokens > 0
                and len(s.split()) <= args.lid_bypass_tokens
                and len(t.split()) <= args.lid_bypass_tokens
            )
            if short or (slab[0] == want_s and tlab[0] == want_t and spr[0] >= args.min_lid and tpr[0] >= args.min_lid):
                kept.append(f"{s}\t{t}")
    return name, len(rows), kept


def read_batches(urls, raw: Path, pair: str, src: str, tgt: str, skip_download: bool, batch: int = 20000):
    for name, url in urls:
        zp = raw / f"{name}.{pair}.zip"
        if not (skip_download and zp.exists()):
            print(f"[{name}] downloading {url}", file=sys.stderr)
            subprocess.run(["curl", "-sL", "--fail", "-o", str(zp), url], check=True)
        with zipfile.ZipFile(zp) as zf:
            sf, tf = members(zf, src), members(zf, tgt)
            with zf.open(sf) as fs, zf.open(tf) as ft:
                rows: list = []
                for bs, bt in zip(fs, ft):
                    rows.append((bs, bt))
                    if len(rows) >= batch:
                        yield name, rows
                        rows = []
                if rows:
                    yield name, rows
        zp.unlink()  # reclaim disk immediately


def dedup_by_precedence(clean: Path, tmp: Path, sort_mem: str, jobs: int) -> dict[Register, Path]:
    """Deduplicate ACROSS registers, keeping the highest-precedence copy.

    Per-register `sort -u` would leave a pair present in both HUMAN and CRAWL to
    be drawn twice by the sampler, once from each. Which copy survives is not
    arbitrary: a human-checked line beats a mined copy of the same text, and a UI
    string beats the same string scraped into a crawl, so the rank rides along
    and the first row per (src, tgt) wins.
    """
    tagged = clean / "tagged.tsv"
    with open(tagged, "w", encoding="utf-8") as w:
        for rank, register in enumerate(PRECEDENCE):
            part = clean / f"all.{register.value}.tsv"
            if not part.exists():
                continue
            with open(part, encoding="utf-8") as f:
                for line in f:
                    w.write(f"{rank}\t{register.value}\t{line}")
            part.unlink()

    ranked = clean / "ranked.tsv"
    env = {**os.environ, "LC_ALL": "C"}
    with open(ranked, "w") as out:
        subprocess.run(
            ["sort", "-t", "\t", "-k3,3", "-k4,4", "-k1,1n", "-S", sort_mem,
             "--parallel", str(jobs), "-T", str(tmp), str(tagged)],
            check=True, stdout=out, env=env,
        )
    tagged.unlink()

    paths = {r: clean / f"dedup.{r.value}.tsv" for r in Register}
    handles = {r: open(p, "w", encoding="utf-8") for r, p in paths.items()}
    kept = {r: 0 for r in Register}
    try:
        previous = None
        with open(ranked, encoding="utf-8") as f:
            for line in f:
                parts = line.rstrip("\n").split("\t")
                if len(parts) != 4:
                    continue
                _, register, s, t = parts
                if (s, t) == previous:
                    continue
                previous = (s, t)
                r = Register(register)
                handles[r].write(f"{s}\t{t}\n")
                kept[r] += 1
    finally:
        for h in handles.values():
            h.close()
    ranked.unlink()

    for r in PRECEDENCE:
        print(f"[{r.value}] {kept[r]} pairs after cross-register dedup", file=sys.stderr)
    return paths


def finish(args, pair: str, src_gz: Path, tgt_gz: Path, spm_prefix: Path | None,
           register_files: dict[Register, Path] | None = None) -> None:
    """Emit the step's REAL outputs: the 2-col pool, the marian-loadable vocab.

    This used to live in prep_step.sh, which made a partial prep runnable — and
    on 2026-07-20 it was run that way, losing all three at once: the pool paste
    (so src/tgt PAIRING was gone and build_kd_source could not emit kd_ref), the
    `.spm` extension (marian rejects a `.model` name with "DefaultVocabulary must
    not contain empty lines"), and the intermediate cleanup.

    A step whose script does only part of the step is a step you can do wrong.
    The wrapper is now a pure argv adapter, so there is no partial form to run.
    """
    out = Path(args.out_dir) if args.out_dir else Path(args.workdir)
    out.mkdir(parents=True, exist_ok=True)

    pool = out / "pool.tsv"
    with pool.open("w", encoding="utf-8") as w, \
            gzip.open(src_gz, "rt", encoding="utf-8") as fs, \
            gzip.open(tgt_gz, "rt", encoding="utf-8") as ft:
        n = 0
        for s, t in zip(fs, ft):
            w.write(f"{s.rstrip(chr(10))}\t{t.rstrip(chr(10))}\n")
            n += 1
    print(f"  pool {pool} ({n} pairs)", file=sys.stderr)

    # Per-register pools alongside the combined one. A tag COLUMN on pool.tsv
    # would have been the other option, but every existing consumer reads it as
    # 2-col (build_kd_source.sh asserts NF == 2), and separate files make the
    # sampler a per-file draw instead of a filtered scan of 47M lines.
    # Always all five, empty ones included: a pair with no UI corpus must still
    # produce the file, or the step output is missing and the run fails on
    # something that is a legitimate (and recorded) outcome.
    for register in Register:
        dest = out / f"pool.{register.value}.tsv"
        source = (register_files or {}).get(register)
        if source is not None and source.exists():
            shutil.copyfile(source, dest)
        else:
            dest.write_text("", encoding="utf-8")
        print(f"  pool.{register.value} {sum(1 for _ in dest.open())} pairs", file=sys.stderr)

    if spm_prefix is not None:
        vocab = out / "vocab.spm"
        vocab.write_bytes(Path(f"{spm_prefix}.model").read_bytes())
        print(f"  vocab {vocab}", file=sys.stderr)

    if args.keep_intermediates:
        return
    # The WHOLE workdir, not just clean/ and spm/: raw zips live there too, and
    # the pool is the artifact — everything else is re-derivable. Removing only
    # some of it is the same partial-port mistake this function exists to undo.
    work, dest = Path(args.workdir).resolve(), out.resolve()
    # Refuse only when the workdir CONTAINS the outputs (deleting it would take
    # them with it). The normal layout is the opposite — work nested under out —
    # and an inverted check silently skips every cleanup.
    if work.is_dir() and work != dest and work not in dest.parents:
        shutil.rmtree(work, ignore_errors=True)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--lang", default="tl", help="the non-English side (OPUS code)")
    ap.add_argument("--src", default="en")
    ap.add_argument("--workdir", default=".")
    ap.add_argument("--lid-model", default="lid.176.ftz")
    ap.add_argument("--only", default="", help="comma list of corpora to restrict to (smoke test)")
    ap.add_argument("--skip-download", action="store_true")
    ap.add_argument("--min-tokens", type=int, default=1)
    ap.add_argument("--max-tokens", type=int, default=150)
    ap.add_argument("--max-chars", type=int, default=1000)
    ap.add_argument("--ratio", type=float, default=3.0)
    ap.add_argument("--max-word-len", type=int, default=150)
    ap.add_argument("--word-ratio", type=float, default=0.6, help="min fraction of tokens that contain a letter")
    ap.add_argument("--alpha-ratio", type=float, default=0.4, help="min fraction of non-space chars that are letters")
    ap.add_argument("--num-ratio", type=float, default=1.0, help="min numbers-overlap/mismatch ratio between sides")
    ap.add_argument("--min-lid", type=float, default=0.5)
    ap.add_argument("--lid-bypass-tokens", type=int, default=2,
                    help="skip the LID filter when both sides have <= this many tokens (0 disables)")
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4, help="clean worker processes")
    ap.add_argument("--skip-spm", action="store_true", help="skip joint SPM training (reuse an existing vocab)")
    ap.add_argument("--vocab-size", type=int, default=32000)
    ap.add_argument("--sort-mem", default="4G")
    ap.add_argument("--out-dir", default="", help="where pool.tsv + vocab.spm land (default: workdir)")
    ap.add_argument("--keep-intermediates", action="store_true",
                    help="keep clean/ and spm/; they are re-derivable, the pool is the artifact")
    args = ap.parse_args()

    src, tgt = args.src, args.lang
    pair = f"{src}{tgt}"
    wd = Path(args.workdir).resolve()
    raw, clean, spm, tmp = wd / "raw", wd / "clean", wd / "spm", wd / "tmp"
    for d in (raw, clean, spm, tmp):
        d.mkdir(parents=True, exist_ok=True)
    venv_bin = Path(sys.executable).parent

    urls = resolve_urls(venv_bin, src, tgt)
    if args.only:
        keep = set(args.only.split(","))
        urls = [(n, u) for n, u in urls if n in keep]
    if not urls:
        sys.exit(f"no OPUS moses corpora resolved for {src}-{tgt}")
    print(f"corpora for {src}-{tgt}: {', '.join(n for n, _ in urls)}", file=sys.stderr)

    kept_by: dict[str, int] = {n: 0 for n, _ in urls}
    seen_by: dict[str, int] = {n: 0 for n, _ in urls}
    batches = read_batches(urls, raw, pair, src, tgt, args.skip_download)
    max_inflight = args.jobs * 4  # bound queued batches so a huge corpus can't blow up RAM

    def collect(sinks, inflight, drain_all: bool) -> None:
        while inflight and (drain_all or len(inflight) >= max_inflight):
            name, seen, kept = inflight.popleft().get()
            seen_by[name] += seen
            kept_by[name] += len(kept)
            sink = sinks[REGISTER[name]]
            for line in kept:
                sink.write(line + "\n")
            print(f"  cleaned {sum(seen_by.values())}", end="\r", file=sys.stderr)

    sinks = {r: open(clean / f"all.{r.value}.tsv", "w", encoding="utf-8") for r in Register}
    try:
        with Pool(args.jobs, initializer=_init_worker, initargs=(args.lid_model, args)) as pool:
            inflight: deque = deque()
            for task in batches:
                inflight.append(pool.apply_async(_clean_batch, (task,)))
                collect(sinks, inflight, drain_all=False)
            collect(sinks, inflight, drain_all=True)
    finally:
        for h in sinks.values():
            h.close()
    for name in kept_by:
        print(f"[{name}] ({REGISTER[name].value}) kept {kept_by[name]}/{seen_by[name]}", file=sys.stderr)
    total_kept, total_seen = sum(kept_by.values()), sum(seen_by.values())

    print(f"dedup {total_kept} pairs across registers", file=sys.stderr)
    register_files = dedup_by_precedence(clean, tmp, args.sort_mem, os.cpu_count() or 4)

    src_gz = clean / f"train.{pair}.{src}.gz"
    tgt_gz = clean / f"train.{pair}.{tgt}.gz"
    n_final = 0
    with gzip.open(src_gz, "wt", encoding="utf-8") as fs, \
            gzip.open(tgt_gz, "wt", encoding="utf-8") as ft:
        for register in PRECEDENCE:
            with open(register_files[register], encoding="utf-8") as f:
                for line in f:
                    parts = line.rstrip("\n").split("\t")
                    if len(parts) != 2:
                        continue
                    fs.write(parts[0] + "\n")
                    ft.write(parts[1] + "\n")
                    n_final += 1

    if args.skip_spm:
        finish(args, pair, src_gz, tgt_gz, spm_prefix=None, register_files=register_files)
        print(f"\nDONE {pair}: {n_final} pairs (from {total_seen} raw), SPM skipped", file=sys.stderr)
        return

    print(f"training joint SPM ({args.vocab_size}) on {n_final} pairs", file=sys.stderr)
    import sentencepiece as spmlib
    spm_prefix = spm / f"vocab.{pair}"
    corpus_txt = tmp / f"spm_input.{pair}.txt"
    with open(corpus_txt, "w", encoding="utf-8") as w, \
            gzip.open(src_gz, "rt", encoding="utf-8") as fs, \
            gzip.open(tgt_gz, "rt", encoding="utf-8") as ft:
        for line in fs:
            w.write(line)
        for line in ft:
            w.write(line)
    # split_digits: one piece per digit, so copying a figure is a fixed
    # one-piece-per-digit operation instead of a segmentation the decoder has to
    # guess ("2387" as ▁23+87 or ▁2+387). A joint vocab without it learned a
    # short-digit-run prior from label-style finetune data and truncated longer
    # figures (ka_findings.md §32-§33).
    spmlib.SentencePieceTrainer.train(
        input=str(corpus_txt), model_prefix=str(spm_prefix), vocab_size=args.vocab_size,
        model_type="unigram", character_coverage=1.0, byte_fallback=True, split_digits=True,
        input_sentence_size=10_000_000, shuffle_input_sentence=True,
        num_threads=os.cpu_count() or 4, bos_id=-1, eos_id=0, unk_id=1, pad_id=-1,
    )
    corpus_txt.unlink()

    finish(args, pair, src_gz, tgt_gz, spm_prefix, register_files)
    print(f"\nDONE {pair}: {n_final} pairs (from {total_seen} raw)", file=sys.stderr)


if __name__ == "__main__":
    main()
