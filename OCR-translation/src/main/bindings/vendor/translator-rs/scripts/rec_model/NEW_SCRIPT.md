# Adding a new PP-OCRv6 recognizer script

The script-agnostic playbook for `scripts/rec_model/`. Hebrew, the merged Indic
model (Bengali + Gujarati + Kannada + Malayalam), and Georgian are the worked
examples; the per-round history lives in `train_paddle_rec.md`, the quality
diagnosis in `IMPROVEMENTS.md`, and the current Georgian plan in `GEORGIAN.md`.

This is ordered as the decision sequence you actually walk. Each learning is
attached to the decision it bears on, so the section you are standing in tells
you what was already tried.

---

## Step 0 — before you start

Three checks that decide whether the work is worth doing at all.

1. **Is there a translation pair?** A recognizer with no `xx-en` / `en-xx` model
   reads text into a dead end. Check `~/AndroidStudioProjects/bucket/translation/1/models`
   for the language code before writing a line of generator. Georgian tripped
   this: `ka` is in the catalog as TTS-only, with `assets: {}`.
2. **Can PULC name the script?** The classifier has ten fixed classes
   (`PULC_CLASSES`, `crates/translator-ocr/src/ppocr.rs`): arabic, chinese,
   cyrillic, devanagari, japanese, kannada, korean, tamil, telugu, latin.
   Anything outside that list reaches its recognizer only through a forced
   source language or the dominant-pack fallback in `route_ppocr_predictions`.
   Hebrew and three of the four merged Indic scripts already live with this.
3. **Is there a usable corpus?** Leipzig coverage is the default assumption
   (§D). A script with no Leipzig sentence file needs a sourcing plan before
   anything else, because synthetic-only word construction cannot calibrate the
   sequence prior (§D, tail coverage).

---

## A. Decide the model's shape

One new standalone recognizer slot, or a merge into an existing model?

Criteria, in the order they actually decide it:

| Criterion | Merge | Standalone |
|---|---|---|
| Shared glyph inventory with the incumbent | Large overlap favours merge | Disjoint blocks favour standalone |
| Effect on the incumbent's data | `build_corpus.py` equalizes to the *smallest* script bucket, so a merge costs the incumbent lines | No effect on a shipping model |
| Label-order convention | Same convention (both logical, or both visual) | Different convention forces a split |
| Retrain cost | The incumbent must be retrained and re-evaluated | Independent |
| CTC head capacity | Non-issue at these sizes | Non-issue at these sizes |
| Visual distinctness | Distinct scripts confuse cheaply | — |

Capacity is not the constraint. The merged Indic model carries 391 classes
against a CJK model's ~6000, and PP-OCRv6 small ships 50 languages in one
recognizer. Class count has never been the thing that broke a round here.

The balance stage is the real cost of merging. `build_corpus.py` equalizes line
counts across `line_script()` buckets so a corpus-heavy script does not dominate,
which means adding a corpus-poor script to a merged model truncates every
incumbent script down to the newcomer's size. `IMPROVEMENTS.md` records this as
one of the three roots of the Malayalam floor: script-balancing halved Bengali
and shrank effective Malayalam variety.

Settled outcomes so far:

- **The Indic four merged well.** Visually distinct, comparable Leipzig
  coverage, one shared label convention (logical order), one MNN slot, one
  training run. Per-script real-photo CER validated the merge rather than
  assuming it.
- **Hebrew was kept separate deliberately.** Its labels are visual-order and the
  Indic labels are logical-order, so a merged dataset would carry two
  incompatible conventions in one CTC head. Directionality is the hard split.
- **Georgian goes standalone.** No glyph overlap with the Indic four, and
  merging would re-truncate four shipping scripts to Georgian's corpus size for
  no shared benefit. See `GEORGIAN.md`.

Rule of thumb: merge when the scripts share a label convention *and* comparable
corpus depth. Split on directionality without further argument.

---

## B. Classify the script's complexity

This step determines the generator, the label convention, and the downstream
Rust work. Walk every axis; a script can sit on several at once.

### B.1 Directionality

**LTR.** Nothing to do. `Spec.reorder=False`, `render_text == label`, no
downstream pass. Georgian and Latin sit here.

**RTL with whole-line reordering.** A monotonic CTC head reads glyphs in the
order they appear on the strip and cannot learn a whole-line reversal from
logical-order labels. The settled convention:

- Train on **visual-order labels**. `gen_hebrew.py` runs `python-bidi`'s
  `get_display()` over the logical string and renders that string with
  `Spec.reorder=False`, which pins HarfBuzz to `direction = "ltr"` so it applies
  no further reordering. The model then trains exactly like an LTR script.
- Recover logical order **downstream in Rust**, gated by
  `PpocrScript::is_rtl()`. `reverse_visual_to_logical()` in `ppocr.rs` groups
  contiguous ASCII-alphanumeric runs (plus `: * . / % + -` and space) as single
  units, then reverses the unit list, so embedded Latin and digit runs survive
  intact. The same gate drives per-word box carving and
  `order_words_visually()`.
- **Never reverse the logical string** as a whole. That scatters Latin and digit
  runs to the wrong end of the line.
- **Keep `"arabic"` out of the dict filename.** PaddleOCR's
  `BaseRecLabelDecode` keys its auto-reverse off the substring `"arabic"` in
  `character_dict_path`, so a filename containing it double-reverses our
  already-visual labels in any Paddle-side eval.

**Mixed bidi content.** Handled by the run-grouping above rather than by a
second mechanism. Generate visual order directly and let the run grouping put
the Latin back.

### B.2 Local reordering (pre-base matras, conjuncts)

Indic scripts render a dependent vowel sign to the left of its consonant while
storing it after the consonant in Unicode. This looks like the RTL problem and
resolves the opposite way.

The reorder is **local to a syllable cluster** and stays inside the CNN
receptive field, so the model can absorb it. `gen_indic.py` therefore trains on
**logical-order labels** with `Spec.reorder=True`, letting HarfBuzz produce the
visual layout, and does no downstream reordering at inference. HarfBuzz keeps
clusters in logical order, which is what makes the label well-defined.

Verified before generating data, via the HB cluster probe recorded in
`train_paddle_rec.md`. Repeat that verification for any new abugida rather than
assuming the Indic answer transfers.

The contrast is worth holding explicitly:

| | Scope of reorder | Label order | `Spec.reorder` | Downstream pass |
|---|---|---|---|---|
| RTL (Hebrew, Arabic) | Whole line | Visual | `False` | Run-grouped bidi, gated by `is_rtl()` |
| Abugida (Indic) | One syllable cluster | Logical | `True` | None |

### B.3 Casing and dual alphabets

Scripts where display text uses a *different codepoint range* than running
prose. Georgian is the worked example: Mkhedruli `U+10D0–10FA` for prose,
Mtavruli `U+1C90–1CBF` for all-caps display. Cherokee and Deseret have the same
shape of problem.

The trap is a data trap rather than a modelling one. Wikipedia and news prose
carry only the prose form, and signage — the live-camera use case — carries the
display form. A corpus-only model is blind to half the script.

- **Synthesize the missing form via the case transform over real word
  contexts.** Python's `str.upper()` maps Mkhedruli to Mtavruli and round-trips
  cleanly. Uppercasing a fraction of real corpus lines and words gives the
  display form in genuine contexts, which is what §D's tail-coverage rule
  demands. Do not lean on `synth_tail` for it.
- **Keep both ranges as separate CTC classes.** The label is what is on the
  page. Latin keeps `a` and `A` as distinct classes for the same reason, and
  collapsing loses case information the overlay may want for style matching.
  Override only if a downstream consumer proves it needs one form.
- **Verify the second range routes to the right `Script`** in
  `crates/translator-core/src/script.rs`, so font selection and the renderer do
  not treat display-form codepoints as unknown.
- **Check font coverage of the second range separately** (§E). Georgian's
  distro font set covers Mkhedruli in every file and Mtavruli in only two
  families.

### B.4 Invisible control characters

ZWJ and ZWNJ (`U+200C` / `U+200D`) steer conjunct formation and render nothing.
A model cannot predict them from pixels, so they are stripped from both text and
labels and the canonical output form is accepted. `gen_indic.py` carries a `ZW`
translate table for this, and `prep_corpus.py` strips them during cleaning.

Combining marks that are optional in real print get stripped at prep with
`--strip-marks`. Hebrew niqqud (`0591-05C7`) is the example. The Hebrew model
still recovered consonants on pointed text it never trained on, so stripping
optional marks is cheap.

### B.5 Positional and contextual forms

Scripts with allographs whose form depends on position in the word. Hebrew final
forms (`ךםןףץ`) are the worked example, and the lesson is sharp enough to state
as a rule:

> Positionally-constrained glyphs enter labels **only through real corpus
> words**, never through synthetic contexts.

Hebrew round 1 floored rare glyphs by generating `word + glyph + word[::-1]`
contexts. Seeding final forms mid-word taught the model that a final mem can sit
word-internally, which corrupted the ס/ם (samekh / final-mem) decision boundary.
The fix that was meant to help the confusable made it worse. `gen_hebrew.py`'s
`synth_tail` now draws its wrapping letters from non-final forms only.

Generalize to Arabic initial/medial/final forms and any script with positional
allographs: exclude them from the synth pool, and get their coverage from the
corpus.

### B.6 Ligature-heavy scripts

HarfBuzz shapes clusters into single ligated forms, so one misread visual unit
corrupts several label characters at once. CER inflates mechanically, and the
number is not comparable to a script with one glyph per character. The Malayalam
~10% CER in `IMPROVEMENTS.md` is diagnosed partly this way. Set the expectation
before the round starts rather than treating the number as a regression.

---

## C. Build the charset

The generator's `candidate_charset()` is the single source of truth for what the
model could legitimately emit. Every consumer — `prep_corpus.py`'s line filter,
`build_corpus.py`'s trim, font coverage, `keys.txt` — reads it from there.

Construction:

```
candidate = (BASE | KEEP_SET | <script Unicode block range>)
            ∩ {assigned codepoints}
            ∩ {codepoints at least one discovered font renders}
```

Both intersections matter. A raw block range is roughly a fifth unassigned holes
and unrenderable characters, and each one becomes a dead CTC class that acts as
a confusable sink without ever appearing in training data. `gen_indic.py` filters
with `unicodedata.name()` for assignment and `recgen._covered()` for font
coverage.

**`BASE` versus `KEEP_SET`.** `BASE` is the script's letters plus Latin, digits,
and common punctuation. `KEEP_SET` is the curated set of glyphs that are rare in
cleaned prose but real on signs, prices, and documents — currency marks,
brackets, script-specific punctuation, abbreviation marks. `build_corpus.py`
keeps `BASE | KEEP_SET` unconditionally and applies the frequency trim only to
everything else, because the trim would otherwise drop exactly the glyphs a
camera meets most.

Worked examples: Hebrew keeps `₪ € $ [ ] % + & @ #` plus maqaf, geresh, and
gershayim. Indic keeps the danda and double danda, `₹ ৳ ૱`, and brackets.

**Archaic and liturgical letters need no hand-curation.** `build_corpus.py`'s
trim keeps a non-curated glyph only if it appears at least `--min-count` (10)
times in the raw corpus, so a generator can include an entire block range and
let the corpus prune it. Georgian's ten archaic Mkhedruli letters fall out this
way without a list.

**Space is excluded from `keys.txt`.** The training config sets
`use_space_char: true`, which appends space as the final class. The runtime CTC
layout is blank at index 0, the dict at 1..N, and space last — see
`validate_mnn.py`. A space line in `keys.txt` shifts every class and produces
garbage.

---

## D. Prepare the corpus

Two scripts, run in order.

```sh
# 1. raw: download, clean, filter to the charset
python prep_corpus.py --charset-from gen_<script> --download \
    --out data/<script>_corpus.txt \
    --names <leipzig names> [--strip-marks 0591-05C7]

# 2. balanced + keys, emitted together
python build_corpus.py --module gen_<script> --raw data/<script>_corpus.txt \
    --out-corpus data/<script>_corpus.bal.txt \
    --out-keys paddle/<script>_dict.txt
```

`prep_corpus.py` downloads the named Leipzig tarballs, strips the leading
`id\t`, NFC-normalizes, maps typographic punctuation to ASCII, drops lines under
two words or carrying any character outside `candidate_charset()`, and dedups.

`build_corpus.py` runs three stages:

1. **trim** — keep `BASE | KEEP_SET | {glyphs at or above min-count}`, then drop
   every line carrying a non-kept glyph.
2. **balance** — equalize line counts across `line_script()` buckets. A
   single-script generator returns one constant bucket and this is a no-op.
3. **fill** — append `synth_tail()` lines for every kept glyph still under
   `--floor` (300).

**Corpus and `keys.txt` emit from the same pass on purpose.** The corpus the
generator reads and the class list the model trains on cannot drift apart if
they are produced together. The committed keys file is the canonical class list
for the bucket and the Rust side; the training box regenerates an identical one.

**Leipzig naming.** ISO 639-3 prefix plus source and year:
`heb_wikipedia_2021_100K`, `ben_newscrawl_2017_100K`, `kat_wikipedia_2021_100K`.
Base URL is `https://downloads.wortschatz-leipzig.de/corpora/`. Confirm the
exact tarball names exist before scripting them into a bootstrap.

### Learnings

**Prep that normalizes typographic marks to ASCII destroys real glyphs.**
Hebrew geresh (`׳`) and gershayim (`״`) sat at zero occurrences because
`prep_corpus.py`'s `TRANSLATE` table mapped them to ASCII apostrophe and quote.
The recognizer then dropped them on real photos, and because the NRTR head is an
autoregressive LM over the alphabet, it actively pulled correct pixels toward a
geresh-free decode. Audit `TRANSLATE` against the new script's charset before
running, and keep any mark the script genuinely uses.

**Tail coverage comes from distinct real sentences.** The recognizer models
cross-word sequence context — SVTR mixes globally across the strip and the NRTR
training head is an autoregressive decoder over the output alphabet — and
recognition is per-line, so that implicit LM is cross-word. Fake contexts
corrupt the sequence prior. The framing from `IMPROVEMENTS.md`:

- A **natural-frequency stream** calibrates the LM.
- A **coverage stream** guarantees each glyph appears N times and trains the
  shapes.
- Mix roughly 80/20.

Being two or three times off on a word's frequency is harmless. A glyph at zero,
or a glyph appearing in exactly one fabricated context, is not. Never repeat a
single word to hit a count, and never place glyphs in random order.

**`synth_tail` is the floor-filler of last resort.** It exists for glyphs a real
corpus genuinely starves — native digits, danda, currency, abbreviation marks —
and its output must be structurally plausible. Route by `unicodedata.category()`
so a matra attaches to a consonant, a digit joins a digit run, an independent
vowel stands as its own syllable, currency follows a number, and punctuation
wraps a word. Respect §B.5: positionally-constrained glyphs stay out of the
synth pool entirely.

### Give every symbol its real syntactic slot, and refuse to guess

A glyph the corpus never supplies takes **100% of its exposure** from
`synth_tail`. Whatever position that function picks is not one example among
many, it is the entire definition of the symbol as far as the model is
concerned. A generic "drop it between two random words" branch therefore teaches
a rule the language does not have.

This is the same failure as the Hebrew final-form incident (§B.5), one level up:
there it was a letter placed in a position it never occupies, here it is a
symbol. Both were introduced by a fill rule that looked plausible.

Audit it by counting, not by reading the code. For each key, count occurrences
in the trimmed corpus and list everything under the floor — those are exactly the
glyphs whose grammar you are about to invent. Measured on the two generators:

| generator | zero-corpus glyphs sent to the catch-all | what it taught |
|---|---|---|
| `gen_georgian` (before fix) | `#` `@` `[` `]` | `თვე#ყავა`, `უნივერსიტეტი@ქვეყანა`, unpaired brackets |
| `gen_hebrew` (still) | `#` `@` `[` `]` `€` | `{word}{glyph}{word[::-1]}` — symbol between a word and a *reversed* word |

Rules worth encoding, from `gen_georgian.py`:

- `%` and currency trail an amount; they never sit between two words.
- `@` exists only inside an address; generate `user@host.tld`.
- `#` leads the thing it numbers.
- Brackets and quotes come in **pairs** — emit both halves in one line, so
  whichever half hit the floor is covered and neither is ever seen unclosed.
- `.!?` close a clause and take a following space; `,;:` bind to the preceding
  word.
- `-` joins words or spans a numeric range; `/` separates alternatives or date
  parts; `+` leads a dial code.
- A **letter** goes inside a word, not welded between two of them.

Then make the gap loud: end the dispatch with a `raise`, not a generic branch. An
unhandled symbol should stop the build and force someone to decide where it
belongs, because the silent alternative is a confident wrong answer that only
shows up on real photos months later.

---

## E. Fonts

Treat this as step zero of data generation. Font variety separates confusable
glyphs, and the two rounds that regressed or plateaued both traced back here.

Mechanics: `recgen.discover_fonts(lang)` shells out to `fc-list :lang=XX file`,
`recgen._covered(path, charset)` reports per-font coverage via FreeType,
`fonts_for()` picks only fonts covering every character in the line,
`plan_runs()` falls back to a multi-font split when no single face covers it
(below), and `shape_render()` returns `None` when HarfBuzz emits glyph id 0, so a
`.notdef` sample is rejected rather than training the model on tofu. fontconfig
must be installed on the box; bare containers lack it and `fc-list` returns
nothing, which makes generation hang or abort.

### Check the script font's Latin and digit coverage before anything else

Distribution builds of script fonts are frequently subsets that carry the script
and nothing else. Debian's Noto Sans Georgian and Noto Serif Georgian have no
Latin, no digits and almost no punctuation, and they are the only two families
on the box covering Georgian's uppercase Mtavruli range. The families covering
Mtavruli and the families covering Latin were disjoint, so `ᲥᲣᲩᲐ 25₾` — an
all-caps price sign, the single most common Georgian shop front — had zero
covering faces and could not be synthesized at all.

The failure is silent and it is not confined to the obvious case. `sample()`
retries and moves on, so an uncoverable line leaves no trace: 6.3% of Georgian
lines were being dropped before anyone looked. Mixed lines that *were* generated
lost the purpose-built faces and fell back to pan-Unicode generalists. And the
consequence reaches past coverage into the model's language prior — with zero
Mtavruli-plus-digit samples, the NRTR head learns that the pair never occurs and
actively suppresses correct decodes of `ᲐᲤᲗᲘᲐᲥᲘ 24/7`.

Measure it directly for a new script, per line type, counting families:

```python
for name, line in {"script": "…", "script+digits": "… 25",
                   "script+latin": "… WiFi", "script+currency": "… 25₾"}.items():
    print(name, len({fam for fam, face in faces
                     if all(face.get_char_index(ord(c)) for c in line if c != " ")}))
```

### Mixed-font rendering is the fix, and it is more faithful than the alternative

`recgen.plan_runs()` splits a line no single face covers into `(text, font)` runs
by greedy set cover, capped at `MAX_FALLBACK_FONTS`, and `shape_render()` shapes
each run separately and lays them out left to right on one baseline at one em
size. When one face does cover the line it stays a single run drawn exactly as
before, so adding this changed nothing for Hebrew or Indic (verified: 3000/3000
lines single-run for both).

This is not a workaround for a thin font pool. A real renderer facing a Georgian
font with no Latin substitutes another font for the Latin run, so `<script>
<email> <script>` and all-caps-plus-price lines reach a real page as multi-font
renders. Synthesizing them any other way would misrepresent them. The metric
mismatch between a script face and its Latin fallback is part of what the model
needs to tolerate. It also buys Latin variety for free: the Latin run draws from
the whole `discover_fonts("en")` pool rather than from the handful of faces that
happen to cover both, provided the generator unions that pool into `Spec.fonts`
the way `gen_indic` and `gen_georgian` do.

Harvest good fonts anyway. Mixed-font rendering makes a line renderable; it does
not give the script's own glyphs more than the design variety on the box.

### Learnings

**Count families, not files.** A distribution's font list is mostly weight and
width variants of two or three designs. Georgian measures 109 files across 9
families here, and 7 of those families are pan-Unicode fallbacks (DejaVu,
FreeSans, Hack) rather than Georgian type designs. Hebrew's confusable errors
(מ/ח, ס/ם, ב/כ, ד/ר) were rooted in the same gap, and the fix was installing
authentic faces — `fonts-culmus` (David, Frank Ruehl, Miriam CLM) and
`fonts-sil-ezra`.

**Check every sub-range separately.** A font covering the main alphabet may not
cover a secondary or display range. Of Georgian's 109 files, 72 cover Mtavruli,
and those 72 come from only 2 families. Measure per range before assuming the
`fc-list :lang=` count means coverage.

**Harvest Google Fonts when the distro set is thin.** `setup_indic.sh` has the
pattern: probe each candidate TTF with FreeType for a representative codepoint
per script and copy the matches into `/usr/share/fonts/`. Prefer a locally
staged set scp'd from the checkout over cloning `google/fonts` on the box; the
clone is ~2.5 GB and unreliable.

**Seek the faces the script's real print culture uses.** Culmus for Hebrew, BPG
for Georgian, Lohit and Samyak for Indic. Stock Noto alone under-covers
traditional print, which is what the Hebrew round-2 font fix demonstrated.

**Real signage needs display and heavy weights.** Stylized sign titles garbling
while normal-weight text on the same sign reads perfectly is the recurring
real-world failure across every script trained so far — Hebrew banners, and
Malayalam and Gujarati shop-sign headers. The harvested Google Fonts set skews
to text weights. Add real heavy and display faces.

**Never synth-bold by coverage dilation.** Hebrew round 3 added a MaxFilter
stroke dilation and was a net regression: dilation warps glyph geometry (מ read
as ק or ז) and broke text round 2 read correctly, for marginal ס/ם help. Use
real bold font weights instead. The round was reverted; the saturated-colour and
variable-word-spacing changes from it were geometry-safe and kept.

---

## F. Generator module contract

A `gen_<script>.py` is a thin module over `recgen.py`. It must expose exactly
this surface, because `build_corpus.py` imports it by name and `recgen.run_cli`
drives it.

| Symbol | Consumer | Contract |
|---|---|---|
| `candidate_charset()` | prep, build, spec | `frozenset` of every emittable glyph (§C). Cache with `lru_cache`. |
| `BASE` | `build_corpus` | Letters, Latin, digits, common punctuation. Kept unconditionally. |
| `KEEP_SET` | `build_corpus` | Curated rare-but-real glyphs. Kept unconditionally. |
| `line_script(line)` | `build_corpus` balance | Bucket key, or a constant for a single-script model. `None` means Latin/punct-only. |
| `synth_tail(glyph, rng, kept)` | `build_corpus` fill | One plausible same-script line containing `glyph`, drawn only from `kept`. |
| `gen_pair(rng, corpus, vocab)` | `recgen` sampling | Returns `(render_text, label)`. |
| `_build_spec()` | `__main__` | Returns `recgen.Spec`. |

Entry point is `recgen.run_cli(_build_spec())`.

### `Spec` fields

- `name` — dataset filename prefix.
- `fonts` — candidate font paths from `discover_fonts()`. A merged model unions
  every member script's fonts with `discover_fonts("en")` so mixed lines can be
  rendered (`gen_indic.py`). A single-script model can take the script's fonts
  alone when those faces already cover Latin (`gen_hebrew.py`); confirm that
  with `_covered()` rather than assuming it.
- `charset` — `"".join(sorted(candidate_charset()))`, used for coverage tests.
- `reorder` — HarfBuzz shaping mode. `False` forces `direction = "ltr"` with no
  reordering, for a caller that already produced a visual-order string. `True`
  uses natural per-script shaping and lets HarfBuzz reorder glyphs, for logical
  labels. See §B.1 and §B.2.
- `gen_pair` — the script's text generator.
- `vocab` — filled by `run_cli` from the required `--dict` argument, so a
  generator's random fallbacks can never synthesize an out-of-dict glyph. The
  corpus path is already kept-only after `build_corpus.py`, so this guards the
  random branches specifically.

### `gen_pair` conventions

`render_text` is fed to the shaper and `label` is written to the dataset. They
differ only for RTL, where both are the visual-order string. Every other script
returns the logical string twice.

Budget the label with `recgen.join_to_budget(rng, words, budget)` against
`recgen.MAX_LABEL_LEN` (25), which matches `max_text_length` in the training
config. `recgen.STRIP_HEIGHT` (48) matches `REC_TARGET_HEIGHT` in
`crates/translator-ocr/src/ppocr.rs`, so generated strips are drop-in.

**Mix about 25% Latin lines.** The PP-OCRv6 small base already carries Latin and
46 Latin-script languages, and a fine-tune without a Latin stream forgets it
catastrophically. Mixed Latin-in-script content then works within the one model,
which is the unified-model design the v6 paper argues for. Both `gen_hebrew.py`
and `gen_indic.py` use `rng.random() < 0.25`.

Draw corpus lines from a random word offset rather than always the line start,
so the model sees mid-sentence fragments the way a detector box crops them.

### Everything else lives in `recgen.py`

Procedural backgrounds, ink colour sampling including the low-contrast band and
saturated banner colours, the mild geometric warp (`synth_core.warp_maps`:
±2.5° rotation, parabolic baseline bend, small keystone), JPEG and blur and
noise degradation, the `legible()` gate that drops strips whose degraded ink no
longer contrasts with its background, the native-height floor of 30 px and em
floor of 20 px, the whitespace cap, negatives, and the dataset and inspect CLI.
A new script should need none of it changed.

### CLI

```sh
# dataset shard (parallelize by --seed / --prefix)
python gen_<script>.py --out /dev/shm/<script> --n 18000 --seed 0 --prefix w0 \
    --corpus <script>_corpus.bal.txt --dict <script>_dict.txt

# QA sheet: annotated strips, read it before launching a 300K-sample run
python gen_<script>.py --out /tmp/insp --n 32 --inspect \
    --corpus <script>_corpus.bal.txt --dict <script>_dict.txt
```

`--inspect` writes `inspect.png` with each strip captioned by its label. Read it
for the new script's specific failure modes: wrong label order, dropped
conjuncts, missing display forms, tofu that slipped the `.notdef` check.

---

## G. Train

Base weights:

```
https://paddle-model-ecology.bj.bcebos.com/paddlex/official_pretrained_model/PP-OCRv6_small_rec_pretrained.pdparams
```

Fine-tune the **small** tier. Tiny is distilled and needs a dict-matched medium
teacher, which doubles the work; small carries the full Latin dictionary, keeps
the `LightSVTR` neck, and is ground-truth supervised. The reasoning is in
`train_paddle_rec.md`.

Derive `paddle/<script>_finetune.yml` from `paddle/indic_finetune.yml`, changing
`model_name`, `save_model_dir`, `character_dict_path`, and the data directories.
Derive `setup_<script>.sh` from `setup_georgian.sh` (the most recently repaired
one), changing the Leipzig names and the font install line.

### Box bootstrap: the traps that have cost real wall-clock

Every script run so far — Hebrew, Indic, Georgian — has burned time on the same
class of failure. The money is trivial; discovering after fifteen idle minutes
that nothing started is what hurts. Check these before blaming the recipe.

**Ubuntu 24.04 broke what worked on 22.04.** Fresh vast images are noble now:

- `python3 -m pip install <x>` fails with `externally-managed-environment`
  (PEP 668). Install uv through its standalone script — no pip, no system
  Python involvement — then work inside a venv.
- `uv venv --seed` seeds **pip only** on Python 3.12; uv dropped setuptools and
  wheel seeding there. `import paddle` then dies in
  `utils/cpp_extension/cpp_extension.py`, which imports setuptools
  unconditionally. Install `setuptools` explicitly.

**`pkill -f` matches the shell that is launching the thing.** The bracket trick
(`pkill -f "[s]etup.sh"`) stops the pattern matching *itself*, but a remote
`ssh 'bash -c ... setup.sh'` command line legitimately contains the script name,
so the kill takes out the launcher and ssh returns 255 with nothing started.
Never run the kill and the launch through the same command.

**Do not fuse launch, sleep and poll into one shell command.** The tool call
times out, the exit status is lost, and the next log read shows a stale failure
that reads like progress. Launch the bootstrap as a tracked background job so
completion is an event, and keep status checks to their own short call.

**Verify a fix on the box before re-running the whole bootstrap.** One
`ssh '<venv>/bin/python -c "import paddle"'` costs seconds and settles whether
the next twenty minutes are worth starting.

**`nproc` lies.** It reports the host's cores, not the container's share. Read
`cpu.max` (cgroup v2) or the offer's `cpu_cores_effective`; a box advertising 128
may give 25. Generation is CPU-bound, so `N_WORKERS` follows the real number.

**Order the script so cheap checks fail first.** Font coverage, dict generation
and the renderability gate all run before the 300K-sample generation, so a
missing font pool costs seconds rather than an hour.

### Settled hyperparameters

| Setting | Value | Reason |
|---|---|---|
| `pretrained_model` | v6 small | CTC and NRTR heads reinit on a dict-size change while `PPLCNetV4` backbone and `lightsvtr` neck transfer. PaddleOCR warns and trains the heads fresh; this is intended. |
| `epoch_num` | **20** | Synthetic val saturates around epoch 7, and the cosine-LR tail from 10 to 20 is what teaches styled and low-quality text. Hebrew banners read correctly only at epoch 20. |
| `use_amp` / `amp_level` / `amp_dtype` | `true` / `O2` / `bfloat16` | bf16 keeps fp32 exponent range, so no loss scaling. Native on Blackwell. Set `use_dynamic_loss_scaling: false`. |
| Optimizer | Adam (0.9, 0.999), Cosine LR 3e-4, `warmup_epoch: 2`, L2 3e-5 | Mirrors the paper's rec recipe. |
| `d2s_train_image_shape` | `[3, 48, 320]` | Height 48 matches `REC_TARGET_HEIGHT` in `ppocr.rs`. |
| `MultiScaleSampler.scales` | `[[320,32],[320,48],[320,64]]` | Multi-scale training augmentation. Inference height is always 48. |
| `max_text_length` | 25 | Must match `recgen.MAX_LABEL_LEN`. |
| `use_space_char` | `true` | Appends space as the last class; `keys.txt` must not contain it. |
| `RecAug.tia_prob` | **0.0** | PaddleOCR's tia MLS warp is pure-Python per-pixel and ate ~85% of dataloader CPU on the vast container. The generator already supplies cheap vectorized cv2 warp geometry via `synth_core.warp_maps`. |
| `RecConAug.ext_data_num` | **0** | Same CPU bottleneck. The generator already supplies length and combination variety. |
| Batch size | 512 on a 5090 | Adjust to the box. |

Keep the multi-head. NRTR is train-only, dropped at inference, and acts as an
implicit language model worth about a point on the paper's ablation.

**Negatives are unnecessary.** Hebrew round 1 hallucinated text on detector
false-positives, and `--neg-frac` was added to emit empty-label non-text strips.
The score gate already separates garbage (≤0.6) from text (≥0.93), so this was
confirmed as not needed and is not used. Note also that PaddleOCR's
`BaseRecLabelEncode` returns `None` for empty labels and the dataset silently
drops them, so negatives require a patch to work at all.

Generate into `/dev/shm` and parallelize by `--seed`. Reclaim leaked
`paddle_*` shm segments from prior killed runs; they fill the tmpfs.

---

## H. Evaluate

### Do not diagnose data from an unconverged checkpoint

Tempting, because a checkpoint exists at epoch 3 and the box is busy anyway. It
cost most of an evening on the Georgian round.

The epoch-3 model read `Ზ` as `%` on two signs and `Ბ` as `Გ` on a third. That
produced two full causal investigations — a Mtavruli exposure deficit, then a
display-typeface gap — with glyph-similarity measurements, corpus context counts,
per-letter exposure pulled off the training box, and a re-measure of the font
pool. A round-2 training run was designed around the conclusion.

Every one of those errors was gone at epoch 20, with no change to data, fonts or
config. It was optimization error (§ the three-way split at the top of
`IMPROVEMENTS.md`) misfiled as data error.

**An under-trained model and a badly-taught model look identical from outside:**
both emit confident wrong characters on real photos, and both do it on the hard
surfaces first. Nothing in the output distinguishes them. Only convergence does.

So a mid-run probe is worth running for exactly two things — confirming the
export→convert→infer path works end to end, and confirming the model is reading
the target script at all. Any error it shows is provisional. Do not measure
against it, do not build a hypothesis on it, and do not design the next round
around it. Wait for the final checkpoint, which on this recipe is 20 epochs and
about an hour.

A corollary worth internalising: the cosine tail is where styled and low-quality
text gets learned (the Hebrew round-3 note says the same). Display faces, banners
and unusual signage are exactly the cases that look broken at epoch 3 and are
fixed by epoch 20, which is precisely the material a real-photo golden set is
made of.

### Synthetic val is blind to the failure that matters

Model error splits into optimization error, approximation error, and irreducible
data error. Val is generated from the *same* corpus, fonts, and degradations as
train, so it can see the first two and never the third. Training drives
optimization and approximation error to near zero, val looks excellent, and the
gap between the synthetic world and the real one goes unmeasured.

A high synthetic val score sitting next to a real-world ceiling is the signature
of a data problem. Treat the real-photo golden set as the score and let
synthetic val be a sanity check. The full argument is in `IMPROVEMENTS.md`.

### Golden-set harness

```sh
# 1. per-strip GT kit: det + dewarp, numbered sheet + blank template per image
python make_transcribe_kit.py <det.mnn> <rec.mnn> <keys> <script> <img_dir> <out_dir>

# 2. det -> rec -> content-match -> CER/WER table, one column per model
python run_golden.py <det.mnn> <family> old=<mnn>:<keys> new=<mnn>:<keys>
```

Per-strip transcription beats whole-image prompting. A VLM asked to read a dense
board invents sentence boundaries and duplicates text; asked to read one strip it
transcribes that strip. Leave a strip blank when it is detector garbage, and it
drops from scoring.

`run_golden.py` scores every GT line by its best-matching rec line, which is
robust to detector box-count drift. It writes into a per-script output directory
because `<stem>.rec.json` names collide across scripts.

Wire the new script in three places:

- `FAMILIES` in `run_golden.py` — maps a family to its data directories.
- The slug match arm in `src/bin/golden_eval.rs`.
- The slug match arm in `src/bin/viz_pipeline.rs`.

Collect real photos across the surfaces that matter: book or document pages,
street and shop signage, and screenshots. Signage is where every script has
failed so far (§E).

`validate_mnn.py` is the fast MNN-runtime sanity check on val images, replicating
the PaddleOCR rec preprocessing and CTC layout. Run it after conversion to catch
a keys-file offset before the golden run.

---

## I. Ship

### Export and convert

`tools/export_model.py` produces PIR `inference.json` plus `inference.pdiparams`.
Convert with `paddle2onnx` (opset 14, `--model_filename inference.json
--params_filename inference.pdiparams`) then `MNNConvert -f ONNX
--weightQuantBits 8`. `scripts/convert_ppocr_v6_mnn.py` holds the invocations and
the upstream-release `MODELS` table; the fine-tunes are converted with the same
two commands against the fine-tune's own output directory.

The ONNX step exists only because MNNConvert reads ONNX. It is a scratch file in
the work dir, never a deliverable — the runtime loads MNN and nothing else.

Ship `<script>_rec_int8.mnn` plus `<script>_keys.txt`.

### Wiring checklist

**Rust — `crates/translator-core/src/catalog_model.rs`:**

- Add the `PpocrScript` variant with a doc comment naming what it covers.
- Add arms to `as_slug()` and `from_slug()`.
- Add to `is_rtl()` only for a visual-order recognizer (§B.1).

**Catalog — `~/AndroidStudioProjects/Translator/catalog_ppocr.py`:**

- `PPOCR_V6_FILENAMES` — both the model and the keys file, so they resolve to
  the v6 bucket and install paths.
- `PPOCR_V6_NATIVE_RECOGNIZER_FILENAMES` — `slug: (model, keys)` for a fine-tune
  with no v5 base.
- `PPOCR_RECOGNIZER_SLUGS` — the stable output order.
- `ISO_SCRIPT_TO_PPOCR` — the ISO 15924 code to slug mapping. The assertion at
  the bottom of the file catches a missed entry.

**Recognizer and keys alternates must stay priority-paired.** The engine picks
the keys file whose priority matches the chosen recognizer's, so a
half-downloaded upgrade falls back to the older complete pair. Never bump one
without the other.

Then upload the files to the bucket and regenerate the index.

### The two recurring gaps

**PULC cannot name most new scripts.** Ten classes, listed in §0. A script
outside them reaches its recognizer through `recognizer_script_for_language()`
when the user forces a source language, or through the dominant-pack fallback in
`route_ppocr_predictions()` when another strip in the batch classified. Auto
script detection on a single-script photo of an unnamed script routes wrong.
Decide whether to accept it (Hebrew and three Indic scripts do) or budget a PULC
retrain. Note the routing constants while you are there:
`PPOCR_ROUTE_DOMINANT_MIN_RATIO` 0.55, `PPOCR_ROUTE_MINOR_KEEP_RATIO` 0.20,
`PPOCR_ROUTE_SMOOTH_MIN_CLASSIFIED` 8, plus the rule that Latin strips in an
otherwise single non-Latin batch fold into that non-Latin script.

**A recognizer without a translation pair reads into a dead end.** Check
`bucket/translation/1/models` at step 0, not at ship time.

---

## J. Lessons banked

The negatives, collected so a future round does not re-run them.

- **No coverage-dilation bold.** Dilation warps glyph geometry and regressed
  Hebrew round 3. Use real bold font weights.
- **20 epochs, not 10.** The cosine-LR tail teaches styled and low-quality text
  long after synthetic val saturates.
- **No negatives.** The score gate already separates garbage from text, and
  PaddleOCR silently drops empty labels anyway.
- **Positionally-constrained glyphs never enter synthetic contexts.** Seeding
  Hebrew final forms mid-word corrupted the ס/ם boundary.
- **Do not normalize real punctuation away in prep.** Geresh and gershayim sat
  at zero because `TRANSLATE` mapped them to ASCII.
- **Count font families, not files.** Nine families behind 109 Georgian files,
  two of them covering the display range.
- **Check that the script font carries Latin and digits.** Distribution builds
  are often script-only subsets. Georgian's two Mtavruli-capable families had
  neither, which made all-caps price signs unrenderable and cost 6.3% of lines
  with no error message.
- **An uncoverable line is dropped silently.** `sample()` retries and moves on,
  so a coverage hole shows up as a missing co-occurrence in the data rather than
  as a failure. Count what the generator emits, not what it was asked for.
- **Do not trust synthetic val.** It is generated from the same corpus, fonts,
  and degradations as train, so it cannot see the error that actually caps the
  model.
- **More capacity and more epochs are the wrong knobs when the ceiling is
  data.** The ink experiments showed added capacity hurting the bold head; when
  more capacity hurts, capacity is not the constraint.
- **Balancing a merged corpus truncates every script to the smallest bucket.**
  Weigh this before merging a corpus-poor script into a shipping model.
- **Verify the label-order convention against a reference shaper before
  generating 300K samples.** Hebrew was checked against PIL+RAQM, Indic against
  the HarfBuzz cluster probe.
