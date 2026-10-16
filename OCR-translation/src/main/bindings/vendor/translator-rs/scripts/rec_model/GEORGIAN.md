# Georgian recognizer (`georgian` slot)

Plan for a PP-OCRv6 small rec fine-tune covering Georgian (Mkhedruli + Mtavruli)
with mixed Latin. Third script after Hebrew and the merged Indic model.

The generic method — how to add any new script — lives in `NEW_SCRIPT.md`. This
document records only what is specific to Georgian: the charset decisions, the
Mtavruli question, the font constraint, and the per-round results.

## Shape of the problem

Georgian is the Hebrew pipeline with the RTL wrinkle removed. Single script,
left-to-right, no bidi, no conjuncts, no pre-base matras, no cluster reordering.
In `recgen.Spec` terms:

- `reorder=False` — HarfBuzz is forced LTR, the caller pre-ordered the string.
- `gen_pair` returns `(text, text)`; render string and label are the same object.
- No `python-bidi`, no downstream visual→logical pass. `PpocrScript::Georgian`
  stays out of `is_rtl()`.

That makes it the least risky of the three fine-tunes so far. The difficulty
sits entirely in data (Mtavruli coverage) and fonts, covered below.

## Standalone slot, not merged into `indic`

`indic` is one model over Bengali, Gujarati, Kannada and Malayalam. Georgian
does not belong in it:

- No shared glyphs and no shared corpus, so the merge buys no transfer.
- `build_corpus.py`'s balance stage equalizes line counts across the scripts
  returned by `line_script()`. Adding a fifth script would re-cut every existing
  script's share of the corpus, and Bengali was already halved once.
- A merge forces a retrain and a re-eval of a shipping model to add a script
  that shares nothing with it.

Georgian gets its own slot, its own corpus, its own keys, its own MNN file. The
Hebrew slot is the precedent.

## Charset

| Range | Contents | Handling |
|---|---|---|
| `U+10D0–10FA` | Mkhedruli, 33 modern letters + 10 archaic (ჱჲჳჴჵჶჷჸჹჺ) | include the whole range |
| `U+1C90–1CBF` | Mtavruli capitals | include, see below |
| ASCII | Latin letters, digits, common punctuation | same `BASE` as `gen_hebrew.py` |
| `KEEP_SET` | `₾` (U+20BE lari), `჻` (U+10FB), `[]%+&@#` | curated, kept regardless of corpus frequency |

The archaic Mkhedruli letters need no special handling. `build_corpus.py`'s trim
stage keeps a non-curated glyph only when it appears at least `--min-count`
(default 10) times in the raw corpus, so archaic letters that modern Georgian
does not use drop out of `keys.txt` on their own. Declaring the full block and
letting the corpus decide is the same pattern `gen_indic.py` uses for the ~20%
unassigned holes in the Indic blocks.

Expected size is roughly 150–170 classes, between Hebrew (114) and the merged
Indic model (391).

The lari sign is worth calling out. It appears on essentially every price in
Georgia and is rare in cleaned wiki prose, which is exactly the profile the
`KEEP_SET` mechanism exists for. It goes in the keep set and gets synth-filled
to the floor, the way `₪` does for Hebrew.

## Mtavruli: keep it as its own classes

**Decision: Mtavruli codepoints stay distinct in the label. The label is what is
on the page, never case-normalized.**

Reasoning:

- Mtavruli is a separate glyph design, with uniform cap height and no ascenders
  or descenders. The recognizer sees two shape sets whichever way the label is
  written; the only question is whether the label preserves the distinction the
  pixels already carry.
- Latin keeps `a` and `A` as separate CTC classes for the same reason. Georgian
  case behaves the same way at the codepoint level, so it should be labelled the
  same way.
- Case is semantic. All-caps signage is a style signal the overlay wants, and
  the StyleRange/emphasis path can only re-render an all-caps sign as all-caps if
  the recognizer reported it. Normalizing at the label destroys that information
  before anything downstream can use it.
- The cost is 33 extra classes on a ~150-class head. Immaterial.

If some downstream consumer ever wants one canonical form, it normalizes in
`ppocr.rs` after decode. Same call as the Malayalam atomic-chillu note in
`gen_indic.py`: the recognizer emits what it sees, and consumers with an opinion
apply it themselves.

### Downstream check for Mtavruli

The `unicode-script` side is confirmed. Version 0.5.8 is what `Cargo.lock` pins,
and its tables map `U+1C90–1CBA` and `U+1CBD–1CBF` to `Script::Georgian`, so the
existing `U::Georgian => Script::Georgian` arm in `crates/translator-core/src/script.rs`
already covers Mtavruli. Itemization and `text_shape.rs`'s `Script::Georgian =>
script::GEORGIAN` need no change.

The open check is the **font provider**, which is a trait implemented by each
consumer app rather than by this repo. `FontProvider` returns a preference-ordered
chain and the writer picks the first face whose cmap covers the codepoints at
hand. A platform whose Georgian font predates Unicode 11 will not cover
`U+1C90–1CBF`, and Mtavruli text would fall through the chain to tofu. Verify on
device before shipping: render a Mtavruli string through the image renderer and
confirm glyphs appear. This affects rendering translated output and the
overlay's source-text draw, and it does not affect recognition.

## Mtavruli coverage is a data problem

Leipzig wiki and newscrawl prose is close to 100% Mkhedruli. A model trained on
that corpus alone is blind to Mtavruli. Signage and headlines, which is the live
camera case, are heavily Mtavruli. Training on the corpus as-is would produce a
model that reads Georgian books and fails on Georgian shop fronts.

`str.upper()` maps Mkhedruli to Mtavruli correctly and round-trips through
`.lower()` — verified on the dev box, Python's Unicode tables carry the case
mapping added in Unicode 11. So the generator uppercases a fraction of lines and
a fraction of individual words, and the label follows.

This gives Mtavruli coverage in real word contexts drawn from the corpus, which
is what `IMPROVEMENTS.md` argues the coverage stream must do. Repeated synthetic
placements corrupt the sequence prior that the NRTR head learns, and the Hebrew
`synth_tail` final-form incident is the recorded example of that failure. Do not
route Mtavruli coverage through `synth_tail`. Uppercase real lines instead.

Mixing ratio is a round-1 knob. Start with something like a quarter of lines
fully uppercased plus occasional single-word uppercasing inside otherwise
lowercase lines, and read the round-1 golden-set errors to decide whether
Mtavruli is under- or over-represented.

## Fonts: the headline risk

Measured on the dev box:

```
$ fc-list :lang=ka file | wc -l
109
```

109 files, but only **9 families**: DejaVu Sans, DejaVu Serif, DejaVu Sans Mono,
FreeSans, FreeSerif, FreeMono, Hack, Noto Sans Georgian, Noto Serif Georgian.
The 109 is mostly weight and width variants of the two Noto families.

Worse, only **2 of the 9 cover Mtavruli** — Noto Sans Georgian and Noto Serif
Georgian, accounting for 72 of the 109 files. Every Mtavruli line in the training
set would render in one of two designs, and Mtavruli is precisely the signage
case the model most needs to generalize on.

Worse again, those two are Debian subset builds carrying **no Latin, no digits
and almost no punctuation** — only `-`, `჻` and `₾` beyond the letters. The
families that cover Mtavruli and the families that cover Latin and digits are
disjoint sets. Coverage per line type, measured:

| line | files | families |
|---|---|---|
| Mkhedruli only | 109 | 9 |
| Mkhedruli + Latin, digits, or an email | 37 | 7 |
| Mkhedruli + `₾` | 9 | 3 |
| Mtavruli only | 72 | 2 |
| Mtavruli + Latin, digits, or `₾` | **0** | **0** |

So the gap is not confined to digits and not confined to the uppercase form.
Mkhedruli mixed with Latin loses both Noto families, which are the only two
purpose-built Georgian faces in the set — DejaVu, FreeSans and Hack are
pan-Unicode generalists whose Georgian is an afterthought. `ქუჩა 25₾`, an
ordinary lowercase price line, is down to three families. `ᲥᲣᲩᲐ 25₾` and
`ᲗᲑᲘᲚᲘᲡᲘ 2025` cannot be rendered at all.

**This is fixed in `recgen.plan_runs` (mixed-font rendering), not by fonts
alone.** A line no single face covers is now split into runs by greedy set
cover and each run drawn in its own font, which is what a real renderer does
when a script font carries no Latin. Before the change, 6.3% of generated
Georgian lines were silently dropped by `sample()`'s retry loop, and of 3000
emitted samples the 407 carrying Mtavruli contained zero digits or Latin
characters. After it: nothing is dropped, and Mtavruli co-occurs with Latin and
digits at the rate the corpus produces. A single covering face still yields a
single run, so Hebrew and Indic generation is unchanged (verified: 3000/3000
lines single-run for both).

Font harvest is still needed, for a different reason — Mtavruli *design*
variety stays at two families until it is done, which is the display and
signage weakness `IMPROVEMENTS.md` flags across every script trained so far.

This is the Hebrew round-1 font gap in a more severe form. Round 1 for Hebrew
produced a confusable-pair failure (מ/ח, ס/ם, ב/כ) that was root-caused to a
stock-font pool under-covering traditional print, and round 2 fixed it by
installing authentic faces (Culmus, SIL Ezra). Doing the Georgian font harvest
after round 1 repeats a mistake that is already written down.

**Do the harvest before the first training run**, since round 1 is what the
Mtavruli decision is being tested on and two designs will not answer it.

- Harvest Google Fonts Georgian faces the way `setup_indic.sh` does. Use **both**
  `U+10D0` (Mkhedruli AN) and `U+1C90` (Mtavruli AN) as representative
  codepoints, and count them separately so Mtavruli coverage is visible rather
  than assumed. A harvest keyed only on `U+10D0` would report healthy coverage
  while leaving the Mtavruli pool at two families.
- Get BPG faces if licensing allows. BPG is what most Georgian print and street
  signage actually uses, and its absence is the closest analogue to the missing
  Culmus faces in Hebrew round 1.
- Include real heavy and display weights (`IMPROVEMENTS.md` recognizer-data item
  3). Big stylized sign titles are the documented cross-script real-world gap,
  seen on both Hebrew banners and Malayalam shop signs.
- Never synth-bold by coverage dilation. Hebrew round 3 tried it and regressed
  net; dilation distorts glyph geometry rather than emulating a heavier weight.

Gate the bootstrap on the result. `setup_indic.sh` hard-fails when any of its
four scripts has zero fonts, since generation would otherwise hang. The Georgian
equivalent needs a `fc-list :lang=ka` guard plus a separate Mtavruli-coverage
guard that fails when fewer than some threshold of families render `U+1C90`.

## Corpus

```sh
python prep_corpus.py --charset-from gen_georgian --download \
    --out data/georgian_corpus.txt \
    --names kat_wikipedia_2021_100K,kat_newscrawl_2017_100K
```

Leipzig's ISO code for Georgian is `kat`. Confirm the exact tarball names against
the Leipzig download index at run time; the corpus year and size suffixes vary by
language and `prep_corpus.py` fetches by exact name.

No `--strip-marks`. Georgian has no combining-mark layer to strip, unlike the
Hebrew niqqud case.

```sh
python build_corpus.py --module gen_georgian --raw data/georgian_corpus.txt \
    --out-corpus data/georgian_corpus.bal.txt \
    --out-keys paddle/georgian_latin_dict.txt
```

With a single script, `line_script()` returns one bucket and the balance stage is
a no-op — the same situation `gen_hebrew.py` is in. Trim and fill do all the
work: trim drops archaic letters and dead classes below `min_count`, fill lifts
the lari sign, the paragraph separator, bracket punctuation and any rare-but-real
Mtavruli letters to the `--floor` (default 300).

Watch the `WARNING: N glyphs still under floor` line from `build_corpus.py`. If
Mtavruli letters appear there, the uppercase transform in `gen_pair` is running
too rarely, or `synth_tail` is being asked to cover something it should not.

## Training config and bootstrap

`paddle/georgian_finetune.yml`, derived from `paddle/indic_finetune.yml`:

- `Global.model_name`, `Global.save_model_dir`, `Global.save_res_path`
- `Global.character_dict_path` → `georgian_latin_dict.txt`
- `Train.dataset.data_dir` / `Eval.dataset.data_dir` → the Georgian `/dev/shm` path
- Keep `use_amp: true` / `amp_level: O2` / `amp_dtype: bfloat16`
- Keep `RecAug.tia_prob: 0.0` — the MLS warp is pure-Python per-pixel and ate
  ~85% of dataloader CPU; geometry comes from the generator's cv2 warp
- Keep `RecConAug.ext_data_num: 0` — the generator already supplies length and
  combination variety
- **20 epochs.** Synthetic val saturates around epoch 7, and the cosine-LR tail
  from 10 to 20 is what teaches styled and low-quality text. Hebrew round 2 read
  white-on-red banners correctly only at epoch 20, with no banner-specific data
  added. Budget 20 and do not stop at val convergence.

`setup_georgian.sh`, derived from `setup_indic.sh`:

- Swap the font apt line for Georgian packages, plus the Google Fonts harvest
  described above
- Swap `LEIPZIG` for the `kat_*` corpus names
- Replace the four-script font guard with a single `fc-list :lang=ka` guard and
  the Mtavruli-coverage guard
- Everything else carries over: cu129 paddle, `/dev/shm` dataset, sharded
  generation by `--seed`, detached `nohup` train

No negatives. `--neg-frac` was tried in Hebrew round 2 and dropped in round 3;
the score gate already separates garbage (≤0.6) from text (≥0.93).

## Export

```
tools/export_model.py  ->  inference.json + inference.pdiparams
paddle2onnx (opset 14) ->  georgian_rec.onnx        (build-time intermediate)
MNNConvert -f ONNX --weightQuantBits 8  ->  georgian_rec_int8.mnn   (shipped)
```

The `.onnx` exists only because MNNConvert reads ONNX; it is a scratch file in
the work dir. The runtime loads MNN and nothing else, so the only artifacts that
reach the bucket are `georgian_rec_int8.mnn` and `georgian_keys.txt`.

`convert_ppocr_v6_mnn.py` is not the path here: its `MODELS` table is keyed to
upstream release tars and it names outputs `PP-OCRv6_{tier}_{kind}_int8.mnn`,
which cannot produce `georgian_rec_int8.mnn`. Run the same two commands it wraps
(`run_paddle2onnx` and `run_mnnconvert`) directly against the fine-tune's export
directory, as the Hebrew and Indic models were converted — `out/hebrew_v2_warp/
hebrew_rec.onnx` and `out/indic/indic_now.onnx` are the leftovers of that.

`georgian_keys.txt` is the training dict (`paddle/georgian_latin_dict.txt`)
under its shipping name, not a `write_keys` extraction from `inference.yml`.
The two must be byte-identical or every class index shifts. See `NEW_SCRIPT.md`
§I for the generic form.

## Evaluation

The real-photo golden set is the score that matters. Synthetic val is a sanity
check on optimization, and it is structurally blind to the failure mode that
actually limits these models: val is generated from the same corpus, fonts and
degradations as train, so it cannot see the gap between the synthetic world and
the real one. A high synthetic val next to a real-world ceiling is the signature
of a data problem, not a contradiction. See `IMPROVEMENTS.md` for the full
argument.

Process:

1. Collect real photos into `data/georgian/` — books, street signs, shop fronts,
   screenshots, price lists. Cover Mtavruli deliberately, since that is the
   coverage question round 1 is testing.
2. Build per-strip GT with `make_transcribe_kit.py`. Per-strip beats whole-image
   transcription; it constrains the transcriber to one line and avoids the
   hallucination and granularity drift that whole-sign prompting produces.
3. Score with `run_golden.py`.

Code touchpoints for the harness:

- `scripts/rec_model/run_golden.py:20` — add `"georgian": ["georgian"]` to `FAMILIES`
- `src/bin/golden_eval.rs:23` — add `"georgian" => PpocrScript::Georgian` to `script_from_slug`
- `src/bin/viz_pipeline.rs` — add the same arm to its slug match

## Ship-side wiring — done

All of the below is landed and verified; kept as the record of what a script's
ship step touches.

Rust:

- `crates/translator-core/src/catalog_model.rs` — `PpocrScript::Georgian`, plus
  `as_slug` / `from_slug` arms. Deliberately absent from `is_rtl()`.
- `src/bin/golden_eval.rs` and `src/bin/viz_pipeline.rs` — `"georgian"` arms in
  their slug matches.
- `scripts/rec_model/run_golden.py` — `"georgian": ["georgian"]` in `FAMILIES`.

Catalog, `~/AndroidStudioProjects/Translator/catalog_ppocr.py`:

- `PPOCR_V6_FILENAMES` — `georgian_rec_int8.mnn`, `georgian_keys.txt`
- `PPOCR_V6_NATIVE_RECOGNIZER_FILENAMES` — the `(model, keys)` pair
- `PPOCR_RECOGNIZER_SLUGS` — `"georgian"`
- `ISO_SCRIPT_TO_PPOCR` — `"Geor": "georgian"`

The `assert set(ISO_SCRIPT_TO_PPOCR.values()) | {"eslav"} == set(PPOCR_RECOGNIZER_SLUGS)`
at the bottom of that file catches a half-done edit.

Bucket and index: the two files were copied to `bucket/ocr/1/PP-OCRv6/`, then
`generate_index.py` was run in `internal` mode and again in `public` mode, and
the result copied to `bucket/index_v6.json` so the served index and the app's
bundled asset stay identical. Regenerating to a scratch path and diffing against
the live index first is worth the extra step — it confirmed the only change was
one added pack, `ka` gaining its `ocr` asset, and correct `sizeBytes`, with no
existing pack touched.

### Verifying the wiring rather than assuming it

Two checks, both cheap, both catching failures that are otherwise silent:

- `catalog_tests.rs::georgian_recognizer_resolves_for_ka` parses the real
  shipping index and asserts `ka` resolves to a PPOCR recognizer whose slug is
  `georgian` and which reports LTR. A slug typo or a missing `from_slug` arm
  otherwise surfaces only at runtime as "no ppocr pack for language ka". The
  test skips when the bucket checkout is absent.
- Re-run the golden set with the real slug and diff against the run made with
  the `latin` stand-in. For an LTR script the outputs must be byte-identical,
  since the slug only gates the bidi pass; all nine images matched. For an RTL
  script this diff is the check that the bidi pass actually engaged.

## Open items

### PULC cannot name Georgian

The script classifier has ten fixed classes (`crates/translator-ocr/src/ppocr.rs:107`):
Arabic, Chinese, Cyrillic, Devanagari, Japanese, Kannada, Korean, Tamil, Telugu,
Latin. Georgian is not among them, so a Georgian strip classifies as whichever of
those ten it most resembles — Latin or Cyrillic in practice.

Georgian therefore reaches its recognizer only through a forced source language
or the dominant-pack fallback in `route_ppocr_predictions`
(`crates/translator-ocr/src/ocr_runtime.rs`). This is the same limitation Bengali,
Gujarati and Malayalam already ship with; the comment at
`ocr_runtime.rs:538` records that PULC's Kannada class is the only one of the four
merged Indic scripts it can name.

Acceptable for launch on the same terms as the Indic three. Fixing it properly
means retraining PULC with a Georgian class, which is separate work with its own
data requirements.

### No `ka` translation model

`index_v6.json` carries Georgian as TTS-only: the `ka` entry has
`"assets": {}` and a Piper voice under `tts`. `bucket/translation/1/models` runs
`ar-en … az-en … ug-en` with no `ka-en` or `en-ka`.

So a Georgian recognizer would read text that has nothing to translate it into.
The user intends to build that model, so this is a tracked dependency for the
user-visible feature rather than a blocker on the recognizer work. The OCR side
can be built, evaluated and merged independently; shipping the language to users
waits on the translation pair.

## Rounds

### Round 1 — trained, and what the mid-run probe got wrong

300K synthetic lines, 20 epochs, ~70 min on a rented RTX 4080 Super. Synthetic val
0.99287. Real-photo results on the 8-image golden set below.

**Read this section for the methodological lesson before the results.** An
epoch-3 checkpoint was probed against real photos while training continued, and
its errors were diagnosed at length: `Ზ` decoded as `%` on two signs, `Ბ` as `Გ`
on a third. Two causal accounts were built on that — first a Mtavruli exposure
deficit (`Ზ` at 4752 instances against `%` at 5213), then, when `Ბ` turned out to
have failed at 15805, a display-typeface gap (Mtavruli has 5 font designs to
Mkhedruli's 26). Glyph similarity was measured, corpus contexts were counted,
per-letter exposure was pulled off the box, the font pool was re-measured.

Every one of those errors disappeared by epoch 20, with no change to data, fonts
or config:

| epoch 3 | epoch 20 | |
|---|---|---|
| `Გ%Ა ᲛᲨᲕᲘᲓᲝᲑᲘᲡᲐ` | `ᲒᲖᲐ ᲛᲨᲕᲘᲓᲝᲑᲘᲡᲐ` | `Ზ` |
| `ᲐᲑᲐᲜᲝᲡ ᲡᲐᲮᲣᲠᲐᲕ%Ე ᲐᲡᲕᲚᲐ` | `…ᲡᲐᲮᲣᲠᲐᲕᲖᲔ…` | `Ზ` |
| `ᲛᲣ%ᲔᲣᲛᲘ` | `ᲛᲣᲖᲔᲣᲛᲘ` | `Ზ` |
| `ᲡᲐᲒᲭᲝᲗᲐ` | `ᲡᲐᲑᲭᲝᲗᲐ` | `Ბ` |
| `აზი% სლოევები` | `აზიზ სლოევები` | Mkhedruli `ზ` |
| `TSCHUNVALI` | `TSCHINVALI` | Latin |
| `აკადემაური` | `აკადემიური` | Mkhedruli |

The errors were **optimization error, not data error** — `IMPROVEMENTS.md`
category 1 misfiled as category 3. The model had not converged. Nothing about
exposure or fonts was involved, and the two hypotheses were elaborate
explanations for a checkpoint that was simply 13 epochs early.

**The rule this earns: do not diagnose data from an unconverged checkpoint.** A
mid-run probe is useful for confirming the pipeline runs end to end and that the
script is being read at all. It cannot distinguish "the data taught this wrong"
from "training has not finished", because both present identically as confident
wrong characters on real photos. Wait for the final model, or state explicitly
that any finding is provisional and do not build on it.

This also reverses the round-2 plan that the probe motivated. The Mtavruli
coverage stream was justified entirely by the `Ზ` error; with that error gone
there is nothing for it to fix, and the exposure change costs ~13% of Mkhedruli
labels while pushing the caps share of labels from 19.2% to 28.9%. The machinery
built for it is still correct on its own merits — preferring real corpus text
over `synth_tail`, and a group-relative floor instead of an absolute 300 — and it
is worth keeping for a script whose corpus genuinely starves a class. It is not
worth spending on this one.

### Round 1 residuals (final model)

Real regressions against epoch 3, all minor: `ს-1` → `Ს-1` (case flip on a route
badge that epoch 3 read correctly), `OCCUPATION` → `OCCUPATIOx`, a spurious space
before a full stop, a duplicated `-`.

The one worth tracking: strips that returned **empty** at epoch 3 now emit short
garbage (`@/`, `U`, `T-`, `V`) on detector false positives — ornament and
non-text boxes. This is the hallucination-on-false-positives behaviour Hebrew
round 1 showed. The runtime score gate is documented as separating garbage
(≤0.6) from text (≥0.93), so it likely never surfaces in production, but
`golden_eval` does not apply that gate and so cannot confirm it. Check box scores
on those strips before assuming it is harmless.

### Round 1 golden-set score

Nine real photographs, 98 scorable strips, 3620 characters. Ground truth written
per-strip against the images; ornament, pictogram and Russian strips excluded
(the dict has no Cyrillic classes, so ignoring them is the designed behaviour).

```
TOTAL                     98 strips   CER 0.72%   folded 0.30%   exact 90/98
  georgian only (folded)  64 strips   CER 0.34%                  exact 60/64
  latin only (folded)     34 strips   CER 0.24%                  exact 31/34
```

"folded" maps Mtavruli onto Mkhedruli, isolating letter identity from case. The
gap between 0.72% and 0.30% is entirely two case decisions, so case is worth
tracking as its own metric:

```
Georgian-bearing strips   66     (11 Mtavruli-dominant, 55 Mkhedruli)
Pure case-error strips     1     1.5%
Georgian characters     2007
Wrong on case alone       15     0.75%    always over-predicting Mtavruli
```

Case errors are **whole-line, not per-character** — the model committed an entire
running head to caps rather than mixing within the line. That is the right shape
of behaviour, since a printed line is typographically uniform, but it means each
case error costs a full line of characters rather than one.

Per image, worst to best:

| image | strips | CER (folded) | exact |
|---|---|---|---|
| bathhouse sign (trilingual) | 4 | 1.27% | 3/4 |
| Maro Tarkhnishvili plaque | 14 | 1.08% | 11/14 |
| Bakhcho & Aziz Sloevs plaque | 8 | 1.01% | 7/8 |
| museum banner | 5 | 0.93% | 4/5 |
| book page (Lay of Leithian) | 35 | 0.09% | 33/35 |
| Erkomaishvili plaque | 17 | 0.00% | 17/17 |
| motorway sign, Olympic plaque, HAPPY JOURNEY | 15 | 0.00% | 15/15 |

The book page is the useful result: bendy paper, an italic serif, dense verse,
and it produced the best per-character score in the set. Dense Mkhedruli body
text is both the dominant real-world case and the model's strongest surface.

Remaining errors across the whole set, in full: one running-head case flip, one
em dash rendered `--`, one duplicated dash, one spurious `.` (a bolt head on a
plaque — arguably the ground truth is wrong), one `N` clipped by a sticker
overlapping the sign, one `s` lost to glare, one `on`/`On`, and one genuinely
hard low-contrast strip. No systematic failure mode is visible.

**Read the score with its bias in mind.** The ground truth was written by the
same model that is being scored, so a systematic misreading would be invisible
and the absolute figure is optimistic. It is sound for tracking movement between
rounds, which is what it was built for. A native reader spot-checking a sample
would be needed to anchor it as an absolute number.

Sample skew worth noting: only 11 of 66 Georgian strips are Mtavruli-dominant,
so the case decision — the design question this script exists to test — is mostly
being exercised on Mkhedruli. More Mtavruli signage would strengthen the set.

### Confusables: measured candidates, deliberately not acted on yet

Glyph geometry alone (mean IoU of baseline-aligned rasters across the font pool,
computed from the font files — no model involved) nominates these:

| within Georgian | | Latin leaking into Georgian | |
|---|---|---|---|
| ს/ხ, Ს/Ხ | 0.76, 0.78 | d → ძ | 0.465 |
| მ/ძ, Მ/Ძ | 0.77, 0.77 | b → ხ | 0.451 |
| ე/ქ, ე/ვ | 0.75, 0.75 | 6 → ნ | 0.425 |
| ო/რ, ნ/ხ | 0.74, 0.69 | o → ი | 0.422 |

No repair rule was added for these, in `script_normalize.rs` or anywhere else.
The Cyrillic repair that module implements is justified by `А`/`A` being
*pixel-identical*, so no amount of training can separate them and only word
context can. Georgian's pairs are merely similar, the recognizer has real signal
to learn from, and the NRTR head is an autoregressive LM over the alphabet that
should already disprefer a lone ASCII `d` between Georgian letters. A rewrite
table built before the model exists would mask an error we have not confirmed
and would corrupt genuine Latin in mixed signage.

Derive the real list from round-1 golden-set errors, as Hebrew's `PAIRS` in
`lex_correct.py` were ("observed in the real-photo eval"). Use the table above
only to check whether an observed error was predictable. If a repair does turn
out to be needed, `script_normalize.rs` is where it belongs — dispatched from
`normalize_rec_text_for_script` in `ppocr.rs` — not in a Python post-process.
