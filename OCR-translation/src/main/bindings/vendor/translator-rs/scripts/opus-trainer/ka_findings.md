# en↔ka (Georgian) — teacher gate findings

Measured 2026-08-30 across four rented 4090s, $1.64 of GPU total. Scripts added
for this: `madlad_decode.py`, `lmt_decode.py`, `eval_slices.py`, `mojibake.py`,
`mojibake_filter.py`, `gen_short_en.py`. Data produced:
`data/short.en-ka.gen.jsonl` (10,863 short pairs) and `probes/check.ka.gen.jsonl`
(67 camera-path lines), both model-generated and awaiting native review.

Verdict: teacher is NiuTrans LMT-60, both directions. MADLAD-400 is
unusable for Georgian and the reason is section 2.

## 1. South-Slavic mojibake contaminates the Georgian side of mined corpora

~80% of the Georgian side of OPUS OpenSubtitles en-ka is South-Slavic
subtitles, not Georgian. CP1251 Cyrillic bytes were painted with the glyphs of
an 8-bit Georgian font and then stored as the Georgian codepoints, so every
character lands inside the Mkhedruli block. It survives codepoint-range checks,
"does this contain Georgian script" checks, and fastText `lid.176`, all of which
see a well-formed Georgian string.

The content is mostly Bulgarian but includes Macedonian, Serbian and Russian, so
treat it as Slavic rather than one language.

The substitution is the 32 CP1251 lowercase Cyrillic letters laid position by
position onto the first 32 letters of the TRADITIONAL 38-letter Georgian
alphabet. The archaic letters ჱ ჲ ჳ carry з и х because they sit at traditional
positions 7, 14 and 21, which modern Georgian skips. Aligning onto the 30-letter
Bulgarian alphabet instead is wrong past щ: it mis-maps ჩ ც ძ წ, and წ=я is among
the most frequent letters in the corrupt text.

    გთზ რჲგა.      ->  виж това.      (Look at that.)
    ვი, მამჲ.      ->  ей, мамо.      (Hey, Mama.)

Detection ladder, drop-only and never keep-only, following the uig/kk precedent
in `script_lid.py`:

1. Archaic Mkhedruli letters `U+10F1-10FA`. ჱ ჲ ჳ are the images of Cyrillic
   з и х, which are common in Slavic and effectively absent from modern
   Georgian, so the false-positive risk is near zero. This also drops genuine
   Mingrelian and Svan, which use ჷ and ჸ.
2. Invert the map, then run LID. A demapped mojibake line is fluent
   Bulgarian and LID says so; a demapped genuine Georgian line is Cyrillic noise
   that LID will not confidently label. This needs no mined marker list.
3. A character-trigram likelihood ratio for the residue: score each line
   under a Georgian model and its demapped form under a Slavic model trained on
   the corpus's own tier-1 hits, and drop when Slavic wins by a margin. Training
   the Slavic side on the corpus itself costs nothing and needs no extra data.

Tier 1 alone is not enough. A third of the archaic-free OpenSubtitles lines are
also corrupt, so a codepoint rule by itself leaves tens of thousands of Slavic
pairs in the pool.

Shipped as `mojibake_filter.py` at `--margin 0.5`: 98.5% catch on a hand-labelled
residue, and 23 false positives across 91,700 known-clean lines (0.025%), which
are `.kgm` place names and printf format strings. Measured drops: OpenSubtitles
76.67%, KDE4 0.09%, translatewiki 0.02%, TED2020 0.004%, GNOME/ELRC/QED and the
generated short set 0.00%.

Demap-then-LID was tried and rejected: `lid.176` labels 104 of 134 genuine
Georgian lines as `ru`, because demapped Georgian is still Cyrillic and `ru` is
the block's default answer.

Known residue: roughly 7-8% of the kept OpenSubtitles lines are still Slavic.
All are 1-3 words ("ეა."=да, "ნვ."=не), too short for trigram context. A minimum
length filter on the pool is the cheaper fix than lowering the threshold.

Scale, measured on the full prepared pool rather than on OpenSubtitles alone.
An early reading had this confined to OpenSubtitles because the corpus sample it
used excluded NLLB, which is 76% of the bitext. It is not confined:

| corpus | archaic-bearing ka lines | share |
|---|---|---|
| OpenSubtitles | 163,112 | 66.80% |
| NLLB | 110,847 | 1.00% |
| WikiMatrix / wikimedia / CCAligned / XLEnt | 117 total | ~0.01% |
| HPLT, MultiHPLT, and every clean corpus | 0-48 | 0.00% |

NLLB's raw rate looked alarming at 110,847 lines, but almost none of it is
Slavic. Demapping the 80,902 lines the filter dropped from the crawl register
finds 195 Slavic and 80,707 not (0.2% / 99.8%). The rest is Old Georgian,
where ჲ ჳ ჵ are doing their real historical job:

    EN  29 As they were going out of Jericho, a great crowd followed him.
    KA  29. და ვითარ გამოვიდოდეს იგინი იერიქოჲთ, შეუდგა მას ერი მრავალი.

That is a correct translation in archaic orthography, mixed with misaligned
liturgical text and junk. Dropping it is still defensible, because archaic forms
would teach the student ჰრქუა where modern Georgian wants უთხრა, but that is a
REGISTER decision and not a contamination one, and tier 1's calibration never
covered this population: the clean references it was tuned against contain no
biblical text.

So the Slavic contamination really is confined to OpenSubtitles: 122,781
dialogue lines against 195 in crawl.

The vocab shows the cost directly. A joint 32k SPM trained on the unfiltered
pool put 133 of its 32,000 pieces on subwords containing archaic Mkhedruli:
they are learned units from Slavic-in-Georgian-letters, not byte fallback, and
`გთზ რჲგა.` tokenizes as `['▁გთ', 'ზ', '▁რჲგა', '.']`. Filter before training the
vocab.

## 2. MADLAD-400's Georgian is the same mojibake

MADLAD-400 scores ~15-17 chrF on en→ka in its own paper across FLORES and NTREX,
against ~47 for NLLB, and the paper offers no explanation. The mechanism is that
it emits Bulgarian in Georgian letters:

    <2ka> I love pizza!   ->  ჲბთფამ ოთუა!   =  обичам пица!
    <2ka> No Smoking      ->  ნვ ოსქთ.        =  не пуши.

Measured over 300 FLORES lines, the fraction of each model's own Georgian output
that carries the mojibake signature:

| model | mojibake |
|---|---|
| madlad400-3b-mt | 59.0% |
| nllb-200-distilled-1.3B | 5.7% |
| nllb-200-distilled-600M | 0.7% |
| LMT-60-8B | 0.0% |
| opus-mt-synthetic-en-ka | 0.0% |

Our own en→ka FLORES chrF++ for madlad-3b is 16.12, reproducing the published
number. So the anomaly replicates and now has a cause: the corrupted Georgian
web text reached the model's training data. NLLB carries a trace of the same
contamination, and the 1.3B carries eight times more of it than the 600M.

MADLAD is unusable as an en→ka teacher, and filtering the mojibake does not
rescue it. Restricting the comparison to the lines where madlad emitted real
Georgian, so it is scored only where it did not fail this way (chrF++ / spBLEU):

| slice | n | madlad-3b | LMT-60-8B |
|---|---|---|---|
| flores | 123 | 24.64 / 11.23 | 49.29 / 35.77 |
| signs | 29 | 40.42 / 28.19 | 59.17 / 50.12 |
| ui | 142 | 51.82 / 35.90 | 45.02 / 27.13 |

The clean rate is itself register-dependent, and it collapses on exactly the
register the app cares about: FLORES 41% clean, signs 43%, ui 95%, ted 3%,
subtitles 1.3%.

Reading the clean lines shows a second failure mode underneath the first. Where
madlad emits real Georgian it frequently ignores the source and generates
fluent unrelated prose, recycling one template across different inputs: three
separate FLORES sources about Shark Tank, a shopping channel and an antibody
trial all came back as a Georgian sentence about a band releasing an album. A
third mode leaves English proper nouns untransliterated and breaks word order.
The same hallucination appears in ka→en, which is why madlad carries the highest
repetition defect count of any model there. So the 48.20 ka→en FLORES score is
an average over correct lines and invented ones.

The `ui` win is training-set overlap rather than capability. Those references
come from KDE4, GNOME and translatewiki, which MADLAD-400 trained on, and madlad
reproduces several of them verbatim. LMT loses that slice partly because it
reads the accelerator `&` as the word "and".

A teacher whose failure mode is fluent invention cannot be repaired downstream.
Mojibake is detectable and could be filtered; hallucination that is grammatical,
plausible and off-topic is the hard case, so there is no filter that makes this
model safe to distil from in either direction.

## 3. When a model emits garbage, check that library defaults still match the spec

The first madlad run produced Chinese, Thai and timestamps for every input. That
looked like confirmation of the model's known weakness and it was a loading bug:
a config flag that the repo sets one way was being defaulted the other way by a
newer version of the library, and nothing errored.

The lesson is procedural. Before believing a bad result about a model, decode
the model card's own documented example and check you reproduce its documented
output. Here `<2pt> I love pizza!` should return `Eu adoro pizza!`; when it did
not, the fault was ours and not the model's. A control that costs one line
separates "this model is bad at our language" from "we loaded it wrong", and
those two conclusions lead to opposite decisions.

Pin the library version across every model in one comparison. Different versions
in the same table is a confound, not a detail.

## 4. Mtavruli belongs at the MT boundary, not in the trainer

FLORES `kat_Geor` is 122,570 Mkhedruli characters and zero Mtavruli, and the six
clean en-ka corpora are the same. Web text is Mkhedruli, so no teacher has been
gated on the all-caps form. The recognizer deliberately emits Mtavruli
codepoints, because `rec_model/GEORGIAN.md` keeps the label faithful to the page.

Decision: normalize Mtavruli to Mkhedruli at the MT input call site, not in
`ppocr.rs`, because OCR output also feeds the per-word box and copy paths, which
want what was actually on the sign. `str.lower()` performs the map, verified to
land every character back in `U+10D0-10FF`.

Shipped as `configs/opustrainer.student.ka.yml` with `UpperCase: 0.0`,
selected through the new `OPUSTRAINER_CFG` override on `train_student.sh` and
`finetune_student.sh` so the shared config keeps working for the Latin-script
pairs. `TitleCase` stays at 0.05: `.title()` is a no-op on Mkhedruli, so it only
ever title-cases the English side.

The cost is real and needs paying back elsewhere. Turning `UpperCase` off also
removes the ALL-CAPS ENGLISH perturbation, and English signage is exactly what
an en→ka camera path photographs. The fix belongs in the finetune set rather
than the trainer: uppercase a fraction of the ENGLISH side of the 10,863 short
pairs while leaving the Georgian in Mkhedruli, which teaches caps-in to
Mkhedruli-out directly instead of coupling the two sides the way the modifier
does.

Do not reach for OpusTrainer's `UpperCase` to teach the student Mtavruli. It
uppercases both sides of the pair, so it couples source casing to target
casing, and the SPM vocab is trained before the modifier runs. The vocab would
carry no Mtavruli pieces, byte-fallback would fire at roughly three pieces per
character, and `check_vocab.py` cannot see it because it inspects the pool
rather than the augmented stream. `TitleCase` is a no-op on Georgian, which has
no titlecase mapping.

## 5. Teacher gate, both directions

chrF++ / COMET22. `signs` is 67 hand-built camera-path lines; `subtitles`,
`ted` and `ui` are held-out human OPUS slices; `adversarial` is reference-free.

en→ka:

| slice | nllb-600M | nllb-1.3B | madlad-3b | opus-synth | LMT-60-8B |
|---|---|---|---|---|---|
| flores | 46.60 / 85.18 | 46.27 / 83.36 | 16.12 / 41.98 | 44.01 / 80.11 | 49.10 / 88.29 |
| signs | 46.40 / 83.62 | 49.54 / 81.14 | 21.05 / 55.10 | 39.69 / 75.06 | 58.43 / 88.00 |
| subtitles | 23.36 / 56.10 | 16.94 / 46.70 | 8.72 / 35.74 | 37.94 / 77.29 | 40.21 / 81.97 |
| ted | 43.53 / 77.62 | 43.10 / 75.03 | 10.53 / 32.38 | — | 45.28 / 84.32 |
| ui | 38.07 / 76.48 | 40.61 / 77.48 | 49.33 / 80.52 | — | 44.31 / 81.25 |

ka→en:

| slice | nllb-600M | nllb-1.3B | madlad-3b | LMT-60-8B |
|---|---|---|---|---|
| flores | 52.50 / 85.20 | 55.38 / 86.69 | 48.20 / 80.08 | 56.09 / 87.84 |
| subtitles | 44.71 / 78.63 | 46.26 / 79.73 | 46.93 / 81.71 | 44.77 / 80.69 |
| ted | 49.53 / 82.57 | 51.75 / 83.35 | 50.90 / 83.77 | 52.27 / 84.82 |
| ui | 32.34 / 64.91 | 38.47 / 72.05 | 51.33 / 84.19 | 47.39 / 81.53 |

Harness validation: our NLLB-600M FLORES numbers are 46.60 en→ka and 52.50
ka→en against Meta's published 46.6 and 52.6.

Readings:

- LMT-60-8B wins both directions and takes COMET on every en→ka slice. It is
  Apache-2.0 and Qwen3-shaped, so it serves through the existing vLLM KD path.
- FLORES hides the register cliff. NLLB-600M drops 46.60 to 23.36 from FLORES
  to subtitles; LMT drops 49.10 to 40.21. On FLORES alone the two look three
  points apart, and on conversational text they are seventeen apart.
- Scaling NLLB does not help en→ka and hurts conversation. The 1.3B loses to
  the 600M on subtitles by 6.4 chrF, consistent with its eight-times-higher
  mojibake rate.
- The 60M synthetic model beats both NLLBs on conversation (37.94 against
  23.36 and 16.94) while losing on FLORES. It was trained only on GPT-4o
  forward-translated Europarl, so it is clean by construction. This is evidence
  for the synthetic route on en→ka rather than for that particular checkpoint.
- madlad's ka→en `ui` and `subtitles` wins are partly copy-through, so read
  them next to the defect counts rather than alone.

## 6. Size and decoding

Size and decoding were measured together because they trade against each other.
chrF++ / COMET22, en→ka:

| config | flores | signs | subtitles |
|---|---|---|---|
| LMT-4B greedy | 47.51 / 88.13 | 56.77 / 86.01 | 40.70 / 82.12 |
| LMT-4B beam 5 | 49.10 / 89.00 | 58.54 / 88.39 | 40.81 / 82.58 |
| LMT-8B greedy | 49.10 / 88.29 | 58.43 / 88.00 | 40.21 / 81.97 |
| LMT-8B beam 5 | 49.87 / 88.91 | 60.08 / 88.50 | — |

ka→en FLORES: 4B greedy 55.22 / 87.34, 4B beam 5 55.68 / 87.66, 8B greedy
56.09 / 87.84.

4B with beam 5 matches 8B greedy on chrF and beats it on COMET, at half the
weights. On quality alone that is the configuration to run, and it recovers what
was lost by LMT having no FP8 checkpoint: 4B in bf16 is ~8GB, the footprint an
8B FP8 build would have had, so a 24GB card keeps ~16GB for KV cache instead of
~8GB.

Throughput kills it. Measured on a 4090 under vLLM 0.28.0, bf16, eager,
decoding FLORES-length lines:

| config | lines/s | 8M lines on 6 boxes | cost |
|---|---|---|---|
| 4B greedy | 44.9 | 8.3 h | $17 |
| 8B greedy | 29.2 | 12.7 h | $26 |
| 4B beam 5 | 1.04 | 356 h | $727 |
| 8B beam 5 | 0.54 | 682 h | $1,391 |

Beam is 43x (4B) to 54x (8B) slower than greedy, and it is not a mis-call or a
CPU fallback: the GPU sits at 100% and 380-420W throughout. vLLM 0.28 removed
`SamplingParams(use_beam_search=...)` and the only remaining path,
`llm.beam_search()`, is a Python per-token loop that rebuilds each beam's full
prefix and resubmits it as a fresh single-token request, so beam-5 over 200
prompts is 256 synchronous engine round-trips of 1,000 requests. Wall time is
therefore set by `max_tokens` rather than by the actual output length: the same
batch took 191.9s at 30 output tokens and 194.3s at 144.

So beam is out, and the remaining choice is size and sampling. On quality per
line the 8B leads the 4B by +1.66 chrF and +1.99 COMET on `signs`; on speed the
4B leads by 1.82x once cudagraphs are on (53.1 vs 29.2 lines/s).

The replacement for beam is `SamplingParams(n=5)` plus a rerank. In vLLM V1
that is native parallel sampling: `ParentRequest` expands one n>1 request into n
children submitted together with prefill shared through prefix caching, inside
the engine. It costs 3.3x (short lines) to 4.0x (long) greedy, and output
tok/s goes UP under it because the engine has more parallel work in flight,
which is the structural opposite of beam.

Quality, 8B en→ka, chrF++ gain over the same harness's own greedy baseline:

| slice | beam 5 | n=5 t=0.3 | n=5 t=0.7 |
|---|---|---|---|
| flores | +0.77 | +0.55 | +0.42 |
| signs | +1.65 | +1.72 | +2.21 |

It recovers ~71% of beam's FLORES gain and beats beam outright on `signs`, for
12x less than beam. Rerank on MEAN logprob, not cumulative, or the shortest
candidate always wins. Sampling has a runaway tail greedy does not: at t=0.7,
183 of 4000 long candidates hit the length cap, so a length guard is needed.
Set a seed or the draw is not reproducible.

Tuning notes, measured with a control run rather than assumed:

- Cudagraphs help the 4B and hurt the 8B. On the 4B, `enforce_eager=False`
  gives 44.9 -> 53.1 lines/s (+18%). On the 8B they consume 34% of the KV cache
  (35,008 -> 22,160 tokens) to buy kernel-launch savings it does not need, for a
  net -12%. Keep `enforce_eager=True` on the 8B.
- Lowering `max_model_len` does nothing for the 8B. Concurrency rose 34x to
  106x and throughput moved -3%: at 16GB of weights on a 24GB card the 8B is
  bandwidth-bound, not concurrency-starved. An early guess that these numbers
  were a 1.3-1.8x floor was wrong and is withdrawn; only the 4B moves.

That KV pressure is also the argument for FP8 on the 8B specifically. At 8GB of
weights it would have room for both cudagraphs and a deep KV cache, turning the
current -12% into a gain. `llm-compressor` with `FP8_DYNAMIC` needs no
calibration data.

Beam matters more than expected, and most on the shortest inputs: +1.65 chrF on
`signs` for the 8B against +0.77 on `flores`. A short output has a small search
space, so greedy's first-token commitment costs more there. Greedy costs about
0.9 COMET.

That also explains the published numbers. The paper reports 89.02 COMET for 4B
en→ka; we measure 89.00 with beam 5 and 88.13 greedy. So NiuTrans's figures are
beam-search figures, and the 4B→8B gap of +0.15 they report is real and tiny.

Open question before the KD run: vLLM's beam search support is weak, and the
throughput figures we plan against (~126 l/s for a 7B class model) are greedy
figures. Measure 4B beam 5 throughput under the serving stack on one box before
renting a fleet, and if beam is unavailable there, the choice becomes 4B greedy
against 8B greedy rather than the table above.

## 7. LMT-60-8B spot-check: wins the gate, still not clean

Read by hand over the 30 safety-critical `signs` lines, a sample of the 179
reference-free adversarial lines, and ka→en subtitles. The reviewer reads
Georgian well enough to judge meaning, numbers, entities and register, and not
well enough to certify idiomatic naturalness, so the native pass still stands.

The error profile is consistent and it matters for this app: LMT handles full
sentences well and degrades on short context-free strings, which is what a
camera sees. FLORES, subtitles, TED and postal addresses come back accurate and
fluent, with entities properly transliterated. Signage and labels are where it
slips, because a two-word string carries no context to disambiguate a word sense.

Real semantic errors found on the safety lines, roughly seven in thirty:

- *power button* → "ძალაუფლების ღილაკს", the button of political power
- *Do not immerse in water* → "არ გაახვიოთ", do not wrap
- *tire pressure* → "საჭის წნევა", steering wheel pressure, and *psi* → "ფუნტი/სმ2",
  pounds per square centimetre. The digit 32 survives, so the numeric check
  passes while the unit is wrong.
- *Allow the engine to cool* → "დაალაგეთ ძრავა", tidy up the engine; and
  *radiator cap* → "ქუდი", a hat
- *before disconnecting the hose* → "არ დაკავშირებამდე", inserting a spurious
  negation, before not connecting
- *Rinse thoroughly* → "საკმარისია…", turning an emergency imperative into an
  assertion that rinsing is sufficient
- *Contains nuts* → "შეიცავს თხილს", contains hazelnut specifically, losing
  the allergen's generality

From the adversarial list: *Right* → "მართალია" (correct/true, not the
direction), *Check please* → "გთხოვთ, შეამოწმეთ" (please verify, not the bill),
*Construction ahead* → "მშენებლობა წინ მიდის" (construction is progressing).
Several English terms come back as transliterations that are not Georgian words,
including "სეალი" for seal and "ფირმვერსია" for firmware.

ka→en is solid and sometimes more complete than the loose subtitle references.
One systematic limit is worth planning around rather than fixing: Georgian `ის`
is gender-neutral, so any ka→en system guesses English pronoun gender and will
be wrong about half the time.

Bearing on the decision: LMT still wins clearly, since NLLB-600M dropped a
negation outright, inverted *Sign out*, and degenerated into character loops,
none of which LMT did anywhere. But a student distilled from LMT inherits this
short-string weakness, and that is the app's dominant input. Teacher choice does
not solve it. The lever is a curated sign, menu and label set used as finetune
data, which is what the check set should grow into.

## 8. Why LMT fails the way it does, and what that means for tuning it

`NiuTrans/LMT-60-sft-data` is public, so the teacher's Georgian supervision can
be inspected directly. `en-ka.jsonl` is 1.65 MB, about 2,960 pairs. Of 716
sampled, 716 are FLORES dev sentences and none appear in devtest. Median
length is 20 English words and no sampled line is 4 words or shorter.

For scale, `en-de` is 10.2 MB, and Georgian costs ~3 bytes per character in
UTF-8 against German's ~1.1, so German carries roughly ten times the pairs.
Georgian sits with `en-ug` (1.4 MB) and `en-tl` (1.0 MB) in the low-resource
tier.

This matches the measured error profile exactly. The model is strong on news
because news is what it was tuned on, loses register on dialogue, and fails on
signage because it has never been supervised on a short Georgian string.

It also means our FLORES devtest number for LMT should be read as slightly
optimistic, since devtest is the sibling split of its own SFT data. The slices
that decided the gate, `signs` and `subtitles`, are unaffected.

On fine-tuning the teacher: data volume is not the constraint. Our ~100k clean
human prose pairs and ~44k UI strings already exceed what NiuTrans used by a
large factor. The constraint is that this data is abundant in the register the
model already handles and scarce in the register where it fails, so tuning on it
would mostly reinforce an existing strength. The only data that moves the
failure class is a curated short-string set, which is also what a student
finetune would want, so the decision is where to spend one scarce dataset rather
than whether it exists.

The asymmetry that settles the sequencing is that any change to the teacher
invalidates the whole KD decode, costing a re-decode and a full re-gate per
iteration, while a student finetune is a few GPU-hours and reversible. The
uig escalation to CPT→SFT was triggered by a teacher that was blocked; this
teacher clears in both directions, so that trigger is not met. Run KD with LMT
as-is, measure the student, then decide.

## 9. Metrics stay blind to the failure that decides shippability

NLLB-600M scores 46.40 chrF++ and 83.62 COMET on `signs` while rendering
*No Smoking* as "სიგარეტის მოწევა", which reads as *Smoking cigarettes*. The
negation is gone and both metrics are comfortable. Other cases from the same
decode: *Sign out* becomes *register*, *Out of Order* becomes *incorrect
sequence*, and *Spicy or non-spicy?* degenerates into a repeated-character loop.

The mechanical checks in `probe_check.py` caught the loop and the length blowup.
They cannot catch the dropped negation, and no reference-based metric ranked it
low. Read the pairs.

## 10. Corpus gotchas for en-ka

- ELRC-5218-Georgian_Legal_MT contains no legal text. 16 of its 1,001 English
  lines carry any legal keyword; the content is encyclopedic prose. There is no
  legal or government eval slice available from OPUS for this pair.
- KDE4 leaks `msgctxt` into the Georgian side on 42.6% of otherwise-clean
  pairs, for example *Toronto* becoming "ტორონტოcanada. kgm". Training on KDE4
  en-ka without stripping these teaches the student to emit trailing English.
- GNOME en-ka is 89% duplicates, 4,601 raw pairs collapsing to 386 unique.
- QED alignment is poor, with 47.8% of pairs outside a 0.4-2.5 length ratio
  against 0.75% for TED2020.
- No CCMatrix and no ParaCrawl exist for en-ka. NLLB alone is 76% of the
  available mined bitext.

## 11. The KD plan

Decided after the gate, and deliberately staged so the expensive question is
answered by measurement rather than on spec.

Iteration 1: LMT-60-4B, greedy, cudagraphs on, en→ka only, 4M lines.
DONE 2026-08-30: 4.9h wall on 4 boxes, $6.1, 67-76 lines/s per box against the
45-55 projected. Every chunk exactly 1,000,000 lines with zero empty outputs, and
the concatenated source re-hashes to the pinned artifact's first 4M
(`726b70ee...`), so the corpus is provably its prefix rather than merely the same
size.

Defect rates over the 4M: empty 0.000%, control_chars 0.000%, copy_through
0.097%, wrong_script 0.103%, too_long 0.127%, length ratio median 1.09. Read per
register they are not defects at all -- entity sits at 2.28% wrong_script against
crawl's 0.05%, which is XLEnt passing brand names and acronyms through as it
should. `verify_kd.py --registers` exists so that distinction is visible rather
than averaged away.

Why the 4B and not the 8B, given the 8B is better per line: the student is
expected to land 5-6 chrF below its teacher, which is larger than the entire
4B-to-8B teacher gap. So that gap may not survive distillation at all, and one
cheap iteration says whether teacher quality is worth paying for here. Running
the 8B first would answer a question we do not have yet.

Why greedy and not `n=5`, given `n=5` is measured and affordable: the first
iteration exists to isolate one variable. With both size and sampling changed at
once, a student that handles signs well would not tell us which choice did it.
`n=5` is now a measured upgrade with known cost, available when the student's
numbers justify it.

Why en→ka and not the cheaper ka→en: every piece of curated data we have is
English-source — the 67-line `signs` set and the 10,863 short pairs — so the
finetune experiment can only run in this direction. It is also the harder
generation problem and the more expensive to decode, because Georgian runs at
roughly one token per character in the Qwen3 vocabulary against 2.6 characters
per token for English.

Pinned. `PIPE_ROOT=/nvme2/prom/pipe` on bigserver, run `kakd`, beside
`tlkd`, `swenv3` and `uigr2`: `kd_src` and `kd_ref` at 10,000,000 lines
(`921e9b48...`, `2854315c...`), plus `vocab` and `mix` blobs. The digests match
the sha256 taken before transfer, so bigserver to store is byte-verified. The
durable working copy with `MANIFEST.md` is `/nvme2/prom/enka2/kd.en2ka/`.

Realized mix: human 30,357 / ui 21,049 / dialogue 37,500 / entity 98,381 /
crawl 9,812,713. Deduplicating on the English column costs less than it might:
crawl 11,255,501 unique pairs becomes 10,713,303 unique English sides.

Draw 10M, pin it, decode 4M. The artifact is the full 10M draw from
`sample_mix.py`, not the slice being decoded. Drawing 4M now and 10M later would
only be additive while the pool stays byte-identical, because
`shuf --random-source` re-permutes everything when a single line changes.
Pinning the whole draw removes that dependency: the line list is fixed, the pool
may then change, and extending to the next chunk is a split of a fixed file
rather than a re-draw that has to be proven equivalent.

Split the pinned draw into 1M chunks and decode the first four. Chunks 5-10 are
additive later at no re-cost, and the same pinned source makes an 8B re-decode a
controlled comparison rather than a different experiment.

What iteration 1 answers. Score the student on the same slices as the
teachers and read the per-slice teacher-to-student delta:

- tracks the teacher within 1-2 across the board: the pipeline is healthy and
  the 8B's +1.66 is worth buying.
- drops 5-6 uniformly: KD compression dominates, teacher choice is close to
  irrelevant, and the 8B re-decode is wasted money.
- drops uniformly except `signs`, which falls further: the short-string
  hypothesis is confirmed, and the fix is the curated finetune set rather than a
  bigger teacher. Test it in the same iteration by finetuning the KD checkpoint
  on the 10,863 short pairs and scoring again.

Recovering the dialogue register for en→ka. The mojibake filter drops
122,781 dialogue pairs, which is most of the conversational data, and there is
no other Georgian dialogue corpus on OPUS. For en→ka that loss is recoverable:
the KD source is the ENGLISH side and the teacher regenerates the Georgian, so a
pair whose only defect is a corrupt ka side is still a usable source line. Its
English is ordinary subtitle text. That takes en→ka dialogue from 40,087 to
162,962. Those lines have no valid `kd_ref`, so they cannot feed extract-best or
ce-filter, which costs nothing for a greedy 1-best run.

The ceiling this cannot lift is teacher style. LMT loses the T-V distinction on
dialogue and literalizes idioms, and the tl experiment already showed that more
teacher-labeled data reinforces teacher style rather than closing the
human-reference gap. More dialogue KD makes the student a better imitator of a
mediocre dialogue teacher, which still beats the near-zero exposure that made
`Right` and `Pull` vanish from the tl student, but it is not a fix.

ka→en has no such recovery, since its source must be Georgian. The 40,087 clean
lines are what exists; HPLT monolingual Georgian would add volume through
`segment_mono.py` but it is web prose, not conversation.

A launch trap from the pipe notes. The step key covers the script digest and
inputs but neither argv nor `configs/*.yml`, so swapping the teacher from 4B to
8B without changing an input hashes identically and can be memoised away. Pass
the teacher through `args={...}`.

Settings. 4B: `enforce_eager=False` (+18%). 8B: `enforce_eager=True`, and do
not lower `max_model_len` — both cost throughput on that model. If the 8B is
ever run, FP8-quantise it first: at 8GB of weights it has room for cudagraphs
and a deep KV cache together, which is what the bf16 build cannot afford.

## 12. Operational notes for the next run

- `PIPE_ROOT=/nvme2/prom/pipe`, on bigserver. It holds all 31 prior
  campaigns. `piped` had been running since a network outage on 3 August with
  400 `NameResolutionError`s against `console.vast.ai` and no log line since;
  DNS resolves fine now, so restart it before trusting it to orchestrate.
- Kill `piped` by explicit PID. `pgrep -f "pipe.cli piped"` matches the
  `bash -c` line carrying it over ssh and kills the session (exit 255). The
  bracket trick is not enough when the same command also launches the process.
- Filter vast offers on `cuda_vers>=12.8`. Three boxes failed at start
  across two runs, from three different causes: `unresolvable CDI devices`,
  `failed to create task for container`, and CUDA driver error 803 on a host
  advertising `cuda_max_good` 12.5. Only the last is predictable from the offer
  listing, and filtering on it is free. Two of the three were Romanian hosts.
  Budget one re-roll regardless.
- Rented price drifts above the cheapest listed offer. A 4-box fleet came in
  at $0.333-0.392/h against a $0.282 headline, because the cheapest offers go
  first. Size the budget on the blended rate.
- bigserver can push to a rented box DIRECTLY, at ~96 Mbit/s. The standing
  note says to relay bigserver→laptop→vast because bigserver "can't scp to vast".
  That conflates inbound with outbound: bigserver is not publicly reachable, so
  vast cannot pull FROM it, but bigserver's own outbound works fine and it
  initiates the connection happily. The laptop uplink is ~1.5 Mbit/s, so
  relaying a 378MB training corpus through it takes ~26 HOURS against ~30
  SECONDS direct. Push from bigserver; only the return leg needs thought.
- Pull decode images from Docker Hub, not ghcr.io. A decode box died on
  `TOOMANYREQUESTS` pulling our own `ghcr.io/.../hy-kd:cu129p` and never finished
  the pull. `vastai/vllm:v<version>-cuda-12.9` off Docker Hub avoids the rate
  limit entirely, is faster to obtain, needs no custom image, and pins the exact
  vLLM the teacher gate ran on.
- `Permission denied (publickey)` is not always key propagation. A host can
  provision `/root/.ssh/authorized_keys` with wrong ownership or modes, and sshd
  then rejects a correctly-installed key. From the client the two are
  indistinguishable and waiting never fixes the second. Read `vastai logs <id>`:
  the real cause appears there as `bad ownership or modes`. `vastai execute` only
  works on stopped instances, and a reboot re-runs the same provisioning.
  Destroy and re-roll.
- A stalled image pull is diagnosable in ~90 seconds. Sample `status_msg`
  twice and compare the layer digest: an unchanged digest is a stall, a changing
  one is a slow host. Elapsed time alone is the wrong signal — `pipe`'s 600 s
  `loading` budget would have killed a box that took 22 minutes to pull and then
  decoded fine. Give a box that just finished a layer a few minutes' grace, then
  destroy.
- Do not stop on a stall. The en→ka shipped checkpoint came after two. That
  run stalled at Up.40000 and again at Up.42000, then Up.44000 delivered
  1.09659 → 1.05091, a 4.2% gain that became its final best and the model that
  shipped. Marian's `early-stopping` counts only CONSECUTIVE non-improvements, so
  the standing advice to stop manually at a plateau is right about the epsilon
  grind and wrong if applied to a couple of stalls. Any automatic stopper belongs
  at the pathological zero-gain case, not near convergence: replayed over that
  history, a rule of "ten consecutive new bests gaining under 0.5% together" saw
  a 15.7% gain and correctly declined to fire.
- To test whether a process is alive, match argv[0], not the command line.
  `pgrep -f`, and equally a grep over whole `/proc/*/cmdline`, match any process
  whose arguments contain the pattern -- including the checking pipeline itself
  and the ssh `bash -c` wrapper carrying it. This produced a false "the trainer
  restarted under a new PID" alarm, and separately a guard that reported a
  watchdog which did not exist. Bracketing the first character does not help when
  the wrapper also holds the unbracketed string. Match the executable instead
  (`argv[0].endswith("/marian.cuda")`), or write a PID file.
- A watchdog needs a claim file, not a cleverer pgrep. Three separate
  watchdog collisions happened in one campaign, including two sessions putting
  three watchdogs on one box. `pgrep -f '[c]ollect.sh <id>'` does not protect
  you: the `bash -c` wrapper carrying the command contains the unbracketed
  pattern, so the guard matches itself and reports a watchdog that does not
  exist. Write a claim file holding the instance id next to the log, so a
  watchdog can tell it is pointed at a box someone else already collected.
- Train on a 4090 or Ampere, never a 5090. That is where the CUDA
  11.8 / sm_89 constraint on `marian.cuda` actually binds. It does not bind on
  the decode boxes, which run vLLM, so a Blackwell card is legitimate there and
  is the better value per unit of decode throughput.

## 13. The short-text corpus — harvest what exists, generate only what does not

Cross-language asset, not a Georgian one. The English side is frozen once at
`data/short.en.v1.en` (100,000 lines) and reused for every pair, so adding a
language is a translation job rather than a curation job, and every pair ends up
scored on translations of the same source.

The instinct to generate short text is mostly wrong, and one measurement
settles it. Unique English lines by length in OpenSubtitles en-ka alone:

| band | available |
|---|---|
| 1 word | 5,584 |
| 2-4 words | 62,458 |
| 5-8 words | 83,400 |
| 9-15 words | 45,519 |
| 16-25 words | 9,022 |

The short bands are the LARGEST, not the smallest. An earlier plan to generate
the 2-8 word bands would have paid to invent 57k lines that one subtitle corpus
supplies 145,858 of, already real rather than a model's idea of how people talk.

The distinction that matters is that short is not the same as signage:

- short CONVERSATIONAL ("Yes.", "What?", "Come on.") is abundant and harvestable
- short SIGNAGE ("Emergency Exit", "Wet Floor") appears in no corpus, because no
  film says it, and that is the only part worth generating

Generation is also bounded by how much genuinely exists. Asking for 2,800
single-word signs returned 1,784 unique, a 36% duplicate rate: real one-word
signage is a small closed set that runs out in the low thousands. Budgeting 8k
there, as an early draft did, would have bought padding and near-duplicates.

Frozen composition, 12,641 generated against 87,359 harvested:

| band | total | generated | harvested |
|---|---|---|---|
| w01 | 7,500 | 1,784 | 5,716 |
| w02_04 | 30,000 | 6,819 | 23,181 |
| w05_08 | 34,000 | 4,038 | 29,962 |
| w09_15 | 21,000 | 0 | 21,000 |
| w16_25 | 7,500 | 0 | 7,500 |

Weighted toward what the KD pool lacks: an en-ka draw is 96% crawl, so long
prose needs nothing and short text needs everything.

Order is a seeded shuffle across bands, so any prefix is a stratified sample.
Translation is bought in 20k slices as quota allows, and stopping after two
leaves a balanced corpus rather than nothing but signs.

Harvest must apply the eval exclusion list. `subtitles.300` came from
OpenSubtitles, so harvesting it unfiltered would put eval lines into a finetune
set. `data/eval_exclude.sha256` holds the 1,000 digests; the run that built v1
caught 635 of them.

The two halves buy different things, which matters when reading results. Signage
is NEW COVERAGE, absent from everything the student sees. Dialogue is a
TARGET-QUALITY UPGRADE on lines the student already meets in KD with LMT targets
that lose the T-V distinction and flatten idioms, so a frontier target on the
same source corrects a specific measured error. That argues against engineering
the overlap away.

Scripts: `harvest_short_en.py`, `gen_short_en.py` (`--words 1-1` for the
single-word band), `build_short_corpus.py`. Measured cost to translate is about
$0.0013-0.0017 per line, so roughly $30 per 20k slice and $150 per language.

This serves en→X only. ka→en needs Georgian source text and has no equivalent.

## 14. Split sentences BEFORE the KD decode, not after

The app sentence-splits at inference: slimt feeds the model exactly one sentence
per call. The training corpora do not match that. Measured, with an
abbreviation-aware splitter:

| corpus | single-sentence | multi, counts agree | ka fewer | ka more |
|---|---|---|---|---|
| short.en-ka.v1 | 89.6% | 7.9% | 0.4% | 2.1% |
| long.en-ka.v1 | 87.4% | 8.4% | 0.8% | 3.4% |
| KD 4M | 90.2% | 4.5% | 0.5% | 4.7% |

Two separate readings, and only the second is a problem.

Translation fidelity is fine. It is N sentences in, N sentences out: ~97% of
pairs either are single sentences or preserve the count, and the residual splits
about evenly in both directions. A first measurement suggested 56% of
multi-sentence KD pairs disagreed; that was an artifact of a naive splitter
breaking English on `Mr.` and `Ms.` and then blaming the Georgian side. Any
sentence-count check on this data MUST carry a non-breaking-prefix list or it
will manufacture a defect that is not there.

The distribution mismatch is real. Roughly 10% of training pairs are
multi-sentence while 100% of inference input is single-sentence, so that capacity
is spent on a case that never occurs, and it plausibly skews the model's length
prior — the finetuned student dropped "Dalhousie University" from a long FLORES
sentence the KD student rendered completely.

For en→ka this was found after the KD decode, when re-splitting would
invalidate the pinned artifact, its alignment and its digests. It was left alone
there.

For ka→en, split before the decode. The order that costs nothing is: build
the pool, split sentences, THEN draw the KD source, decode, align. Splitting
after alignment means redoing the alignment; splitting after the draw breaks the
pinned artifact's prefix property.

Split with the SAME rule the runtime uses, not an approximation of it. The Rust
side's `NONBREAKING_PREFIXES` is the authority; a Python splitter for corpus prep
should consume that same list so that a sentence boundary in training is a
sentence boundary in production. Where the two sides' counts agree, a
multi-sentence pair splits 1:1 and yields MORE training pairs than it consumed;
where they disagree, drop the pair rather than guessing an alignment.

## 15. Results, and what the finetune actually buys

chrF++ / COMET22 on the six slices. `v1` is the first KD student, trained with a
config that perturbed 5% of lines into Mtavruli; `v2 KD` is the retrain after
that was fixed; `FT v1` finetunes v2 on 99k short-only pairs; `FT v2` on 123k
after adding long-form and sentence-splitting; `int8` is `FT v2` quantized,
which is what actually ships.

| slice | teacher | v2 KD | FT v1 | FT v2 | int8 SHIPPED |
|---|---|---|---|---|---|
| flores | 47.51 / 88.13 | 47.36 / 85.04 | 45.89 / 83.06 | 46.60 / 83.26 | 46.86 / 83.21 |
| signs | 56.77 / 86.01 | 52.97 / 84.47 | 65.44 / 90.51 | 64.53 / 89.13 | 63.50 / 89.30 |
| subtitles | 40.70 / 82.12 | 40.97 / 81.55 | 41.98 / 81.10 | 41.62 / 80.96 | 41.87 / 80.87 |
| ted | 44.89 / 84.23 | 44.18 / 82.54 | 42.67 / 81.44 | 42.30 / 82.32 | 42.25 / 82.45 |
| ui | 43.73 / 81.11 | 44.38 / 81.35 | 41.19 / 79.12 | 41.76 / 79.87 | 41.72 / 79.77 |

The KD student reaches teacher parity, which the tl history did not predict.
v2 KD is within 0.15 chrF of its teacher on flores and ahead on subtitles and
ui. The documented expectation was 5-6 chrF below (tl was -5.7). The likely
difference is `sample_mix.py`'s absolute per-register targets, which exist
because tl's short registers were diluted to nothing by a proportional draw.

Removing the Mtavruli perturbation improved training, not just output. v2
beat v1 on 4 of 5 slices, reached a better validation floor (1.017 vs 1.051) and
trained 26k updates longer before stalling. 5% of lines byte-falling-back at ~3
pieces per character was real noise, not a cosmetic defect.

The finetune is a fixed trade, and composition is not the lever. FT v1 bought
+12.47 chrF on `signs` for -1.47 flores, -1.51 ted, -3.19 ui. The obvious
diagnosis was a length-distribution mismatch, so FT v2 added 29% long-form and
made the corpus single-sentence. Everything moved under a point, in both
directions. Finetuning on ~120k pairs after 4M shifts the model toward those
120k more or less regardless of what is in them. Budget the trade rather than
trying to engineer it away.

Frontier references buy what human references were supposed to. The tl
doctrine is that a student only exceeds its teacher on human references, and
that more teacher-generated data merely reinforces teacher style. The finetuned
student beats its teacher by 8.67 chrF on `signs` using luna/sonnet-generated
Georgian. Reading all 67 lines confirms these are real corrections -- *wet
storey* to *wet floor*, *payment reduced* to *payment declined*, an untranslated
"Fire Extinguisher" -- not agreement with the reference's style.

Quantization is free here. int8 costs -1.03 chrF on `signs` and moves three
slices slightly UP. The quantization-aware finetune (`quantize-bits: 8`) is why.
Score the int8 artifact, not the fp32 checkpoint: slimt loads the former.

Shipped pack: `model.enka.intgemm.alphas.bin` 31,561,697 bytes, sha256
`41f5bffa...`, with a subword shortlist and the joint vocab. Total GPU cost for
the whole pair, gate through pack: about $12.

## 16. The TitleCase trap

`UpperCase: 0` is not sufficient for a caseless script. OpusTrainer's
`TitleCaseModifier` does `word[0].upper() + word[1:]`, not Python's `.title()`.
On Mkhedruli `.title()` is genuinely a no-op -- Unicode gives Georgian no
titlecase mapping -- so testing the stdlib function says the modifier is safe,
and it is not: `.upper()` maps Mkhedruli into Mtavruli.

Left at 0.05 it put Mtavruli initials on 5% of training lines and the student
emitted "Არ Არის Პარკინგი" on 6% of signs and 7.3% of UI strings. Georgian has
no Title Case at all, so that output is simply wrong, and chrF barely notices
(+0.15 when normalized) because the metric sees near-identical characters.

Verify the MODIFIER's source, not the stdlib function that shares its name. Both
are 0.0 in `configs/opustrainer.student.ka.yml`.

## 17. The runtime splitter never split any caseless script

`split_sentences` guarded a lone `.` with `first.is_uppercase()`. No caseless
script has an uppercase, so Georgian, Hebrew, Arabic, Devanagari, Bengali, Thai
and Japanese never split at a period; only `!` and `?` did, which hid it in
dialogue while prose stayed unsplit. Georgian is the subtle case: Unicode gives
Mkhedruli lowercase status with an uppercase mapping into Mtavruli, so
`is_uppercase()` is false for ordinary prose.

Fixed with a unicameral-script table, the terminators `।॥։۔؟።፧`, and Georgian
non-breaking prefixes. Affects every shipped X→en pair with a caseless source.

Two traps in the prefix list:

- An entry must be symmetric with the other language's list. `ა.შ` ("and so on")
  is the Georgian `etc.`, which is deliberately absent from the Latin list
  because it ends a sentence. On the 1,788 pool pairs where both sides use the
  construction, dropping `ა.შ` kept 68.6% against 17.7% for keeping it.
- `e.g` and `i.e` were missing from the Latin list, costing English-side splits
  on every pair the app ships. Masked whenever the next word was lowercase.

Split results: 11,459,301 → 10,604,811 pairs, 11.73% mismatch. That rate is a
corpus property, not a splitter artifact — the prefix fixes moved it 0.1pp. 92.8%
of crawl mismatches have more sentences on the Georgian side and 70% are exactly
one-into-two: web translators rendering one English sentence as two Georgian ones.

Still open: 1,697 dialogue pairs whose Georgian side begins with the untranslated
English source, which `nearid.py` misses at ~50% identity, and 1,175 in entity.

## 18. The two directions of a pair do not hold the same amount of data

The ka→en draw asked for 10M and realized 6,308,923. `--kd-col` dedups on the
source column, so the two directions dedup different columns of the same corpus.

| crawl pool | lines | unique en | unique ka |
|---|---|---|---|
| after split | 10,409,293 | 9,875,791 | 6,132,386 |
| before split | 11,255,501 | 10,713,303 | 6,689,244 |

Mined en-ka crawl is many-to-one on the Georgian side; the repetition is list
enumerators and site furniture (`1.` occurs 2,135 times). Splitting costs a
further 8.3% of unique sources, because fragments collide — worth paying only
because §17 made the runtime actually split Georgian.

Measure unique counts per COLUMN before budgeting a direction. Extending past
6.31M needs monolingual through `segment_mono.py`.

## 19. What the ka→en teacher actually does wrong

Chunk 00 passed every automated gate: no defect class above 0.16%, zero empties,
length ratio median 0.99. Reading it found two failures the gate cannot see.

Obscure entities are invented. `მიხელ სააკაშვილმა` → "Mikheil Saakashvili" is
correct, but `ანდრე კოლინგბა` → "Andre Collinba" when the man is André Kolingba.
Same gist-faithful, entity-weak shape the uig KD run recorded.

On degraded input the teacher does not degrade with it. Where the source is
garbled machine translation, the output is fluent, confident English carrying
substituted facts — one medical line acquired "commonly known as Femoral Head and
Neck Ostectomy", naming a surgery absent from the source. A student learns to
sound authoritative exactly where it should be uncertain, and every
fluency-shaped metric moves the wrong way on these pairs.

This is why `kd_ref` is pinned alongside `kd_src`: extract-best is the only lever
that sees it. Orthographic filters cannot — an §19 line is well spaced, correctly
punctuated, real words, and scores at the median of its length bucket. Verified
by reading 60+ lines across every band of `source_quality.py`'s distribution
without surfacing one.

Judge these from the FULL line. The `verify_kd.py` sample cuts at 110 characters,
and one suspected fabrication turned out to be present in the source once read
whole.

## 20. ka→en results

4M KD pairs from LMT-60-8B, one RTX 4090, 2h42m, $0.97. KD best valid ce
1.75998 at Up.48000, stopped Up.60000 on six stalls; finetune on 113,217 aligned
pairs.

int8 pack, the artifact that ships:

| slice | chrF++ | COMET22 | int8 vs fp32 |
|---|---|---|---|
| flores | 48.89 | 82.28 | −0.71 |
| signs | 59.03 | 85.79 | −0.32 |
| subtitles | 65.88 | 86.72 | −1.26 |
| ted | 54.94 | 84.71 | −0.75 |
| ui | 64.34 | 88.52 | −1.09 |
| crawl | 39.41 | 68.22 | −0.24 |

7.2 below the teacher's 56.09 on FLORES, inside the documented KD-compression
band, and 6.5 in fp32.

The finetune had no trade here. fp32 fp deltas: signs +3.94, ui +2.66,
subtitles +2.57, ted +0.31, flores +0.10, crawl +0.03 — every slice up. §15's
en→ka finetune bought +11.56 on signs and paid −0.76 flores, −1.51 ted, −3.19 ui,
and concluded the trade was fixed. It is not fixed; see below for why this one
was luckier rather than better.

The finetune stage needs its own valid set, drawn from the finetune
distribution. This run validated stage 2 against the same held-out TED/human
prose as stage 1. Adapting toward short text makes that metric worse BY
CONSTRUCTION, and it did, monotonically — 1.75739, 1.80160, 1.86362, 1.94520 —
while training cost fell to 0.25. Early stopping therefore selected Up.500, about
12 epochs against en→ka's ~47.

That accident produced the better result: stopping early captured the short-text
gain before the general-quality cost arrived, so the mismatched yardstick acted
as a regulariser. Do not read that as vindication. The curve was uninformative by
construction, so nothing in the setup could have indicated the stopping point was
a good one. `ce` measured against a mismatched valid set can neither condemn
nor bless a checkpoint. Both points are generic pipeline lessons, not Georgian
ones.

The one-word band is the weak spot, and no metric sees it. Bare single-word
signs come back malformed: `მოქაჩეთ` (Pull) → "Stret", `ცეცხლმაქრი` (Fire
Extinguisher) → "fire extinger". `შეცდომა 404` → "Error 44" silently dropped a
digit, which is worse than a garbled word because it stays plausible and the
number IS the meaning in error codes, prices and dosages. chrF is blind to all of
it — "extinger" scores well against "Extinguisher" on character n-grams. Judge
one-word output against the SOURCE, not the reference, which separates model
errors from inherited ones: "Emergency Exit" → "Common exit" traces to a
generated Georgian source that said "spare exit".

## 21. Why isolated words fail, and why more short data is not the fix

The §20 one-word failure looked like mozilla's short-input degeneracy (#210/#215),
whose root cause is that cleaners strip short pairs until 1-word input is
out-of-distribution. It is not that. The KD draw is not short-starved: 29.89% of
the 4M is 1-4 Georgian words.

The problem is what the short examples TEACH. Composition of the 106,432 lines
holding exactly one Georgian word:

| register | share | what it teaches |
|---|---|---|
| crawl | 57.6% | `Maps სტატისტიკა` → "Maps Statistics", `5000kg ჰორიზონტალური turntable` — mixed-script, Latin passed through |
| entity | 34.1% | `ფინეთი` → Finland, `დუფალაკი` → Duphalac — proper nouns, i.e. romanize |
| ui | 5.4% | `ლიბერიაukraine. kgm` → "Liberiaukraine. kgm" — KDE geo-file artifacts |
| dialogue | 2.6% | |
| human | 0.2% | |

Counting Georgian words alone overstates it. Requiring no Latin and no digits
leaves 36,803 lines, 0.92% of the draw, 29,942 unique forms — and a large
share of those are the entity block's proper nouns.

So the student's prior for an isolated Georgian word is "this is probably a name,
romanize it", which is what `ქანჩი` → "kanchi" is, and "Drine" / "cluff" /
"doughnt" are that prior failing on words it does not know. Common nouns as
labels — the register a camera photographs — are close to absent.

Measure the short band by REGISTER, not by count. A pool can be 30% short and
still teach nothing about labels. Before concluding a model is short-starved,
check what its short examples are: proper nouns and mixed-script fragments are
short text that teaches the wrong behaviour for this app.

The entity register is actively harmful at w01 for a camera pair. It is
included as short-pair supply (mozilla's fix for short-input degeneracy) and for
this direction it installs a romanize-on-sight prior. Consider reducing its share
in a future draw rather than raising it.

### Numbers are copied unreliably at both lengths

Not a short-text problem, and worse on prose:

| slice | number-bearing lines | wrong | rate |
|---|---|---|---|
| probes | 14 | 2 | 14.3% |
| flores | 84 | 9 | 10.7% |
| crawl | 138 | 6 | 4.3% |
| ted | 133 | 1 | 0.8% |
| signs / subtitles / ui | 4 / 5 / 3 | 0 | 0% |

Two mechanisms. Omission, where a clause is dropped and takes its number with
it (`Water is spilling over the levee in a section 100 feet wide` → "Water flows
around the germ"). Corruption, where the number survives but is wrong:
`2039` → `2019` turning a pension forecast into history, `7004` → `7074` on a
legal citation, `404` → `44`. Corruption is the dangerous class because the
output stays fluent and plausible.

Score number preservation as a set comparison, and allow correct word forms —
`2-ჯერ` → "twice" is right, and a naive digit match counts it as an error. Note
also that signs/ui/subtitles carry almost no numbers in these eval slices, so the
registers where numbers matter most to the app are the ones the eval barely tests.

### Order of attack

The finetune's w01 band is the right register (29% ui, 28% dialogue, 26% crawl,
14% signage; entity excluded) but it trained for only ~12 epochs because of the
valid-set mismatch above. Overturning a prior installed by 4M pairs needs more
than that. So: redo the finetune against a finetune-distribution valid set first,
~$0.15, and only re-draw KD with a label-register short block if that is not
enough. More short data of the same kind is not the fix.

## 22. Wiktionary glosses are unreliable; declension tables are useful

The survey for the §21 isolated-word failure found that `en.wiktionary` translation tables
give 29,756 sense-labelled ka→en pairs, 46,718 unioned with kaikki (30,705 unique
Georgian), CC BY-SA 4.0. PanLex is gone — `api.panlex.org` is NXDOMAIN, the
snapshot page is dead, and it was CC BY-NC-SA rather than CC0. No kat-eng pair
exists in FreeDict, Apertium or MUSE. Wikidata has 76 Georgian lexemes total.

Do not import the glosses. Of 5,000 concrete sign-plausible candidates, 40%
were already in `kd_src` or the finetune set, and dedup bit hardest at the common
end; the survivors survived by being rare, which is the same property that keeps
a word off a shelf edge. Only 31.4% of lexicon headwords occur in observed text
at all. Worse, the single-sense filter meant to skip un-makeable disambiguation
selects for thinly covered entries — a common word carries many senses and is
dropped — so what passes is rare and often wrong: `გადარეული → nut` (the
crazy-person sense), `ცხარე → hot` (spicy, not temperature), `ფართე → full`
(ფართო is *wide*). In isolated-word position the model has no context to override
a wrong sense, so this deepens the failure it was meant to repair.

The declension tables are the asset. 26,778 of 26,792 kaikki Georgian entries
carry them: 702,567 tagged form strings, median 9 per entry, tagged for case,
number and archaic. Ground truth rather than a model's guess — component (b)'s
generator is currently inventing inflected forms unverified.

Headwords are saturated, surface forms are not. Measured in (b): citation-form
generation produced 2,003 distinct words of which 628 survived cross-dedup (69%
already known), against inflected generation's 2,964 of which ~2,000 survived.
That matches the mechanism behind §21 — an isolated inflected form segments into
subwords the decoder has only seen mid-word, and it falls back to transliteration.

Use it by expanding a gloss trusted from elsewhere across the attested paradigm,
not by importing Wiktionary's own translation. Multiplying a gloss across ~26
forms multiplies any error by 26, so gloss trust gates harder than before, and
the postpositional forms carry meaning English must express (`-ისკენ` = *toward
X*, `-ში` = *in X*) — those need templating, while nominative, ergative, dative
and genitive can share one gloss.

## 23. Three finetune arms: what training length bought, and what it did not

The three data mixes use the same KD checkpoint and finetune-distribution valid
set (§20). The run used one box and cost $0.425.

| slice | existing FT | A (114k) | B (+8.8k) | C (+16.6k) |
|---|---|---|---|---|
| oneword (288) | 38.38 *(47.08)* | 48.64 *(49.19)* | 52.08 | 51.47 *(53.26)* |
| oneword holdout (400) | 39.32 *(48.62)* | 46.33 *(49.68)* | 51.79 | 53.27 *(57.19)* |
| numbers (300) | 68.20 | 70.09 | 74.84 | 74.90 |
| signs | 59.35 | 69.69 | 71.24 | 70.77 |
| ui | 65.43 | 71.86 | 71.31 | 70.86 |
| probes | 55.49 | 59.39 | 59.90 | 59.98 |
| ted | 55.69 | 54.88 | 54.68 | 54.54 |
| flores | 49.60 | 49.43 | 49.02 | 49.22 |

*(case-folded in brackets where it changes the reading)*

The valid-set fix alone was worth a lot. Against the correct distribution the
curve improves monotonically (0.90046, 0.79568, 0.76346) where the old TED/prose
set climbed (1.75739, 1.80160, 1.86362), so arm A trains to ~37 epochs against the
old run's ~12. That bought signs +10.34, ui +6.43, probes +3.90, subtitles +1.46
for −0.81 ted / −0.17 flores / −0.56 crawl. The old stopping point was an artefact
of the yardstick, not convergence.

But longer training did not fix isolated words, and the raw chrF++ says
otherwise. Arm A's +10.26 on `oneword` is +2.11 case-folded — the rest is
capitalisation, because the old finetune emitted lowercase against Title Case
references. The two casing-immune measures both refuse the story: romanize rate
11.1% → 11.5%, moving the wrong way, and number drop 9.1% → 8.1%. Arm A still
emits `ურეცეპტო` → "Unreceptable", `ტრაინიკი` → "Trainki", `დაუთოების` →
"Dutching" where B and C both produce "Ironing".

The generated data is what fixes the named failures:

| | existing | A | B | C |
|---|---|---|---|---|
| number drop | 9.1% | 8.1% | 2.0% | 3.4% |
| romanize rate, holdout | 13.0% | 14.0% | 12.0% | 8.8% |
| one-word exact match | 37.5% | 35.5% | 42.2% | 47.0% |

So §21's diagnosis holds: the romanize-on-sight prior is a data gap, not a
training-length artefact, and no amount of epochs on prose-register finetune data
displaces it.

Arm C was shipped. Against the previously-packed finetune it wins 8 of 11 slices,
and every failure §20 named moved: number corruption 9.1% → 3.4%, romanize
13.0% → 8.8%, one-word exact match 37.5% → 47.0%. Roughly one isolated word in
eleven still comes back romanized, which is a much better model than the one
nearly shipped that morning rather than a solved problem.

Re-measure the int8 pack on `oneword` specifically, not just on chrF.
Quantization's ordinary cost here is 0.3-1.3 chrF++ and is not interesting. The
risk that is: int8 disproportionately affects rare-token behaviour, and rare
tokens are the exact mechanism behind romanization — a decoder falling back on
subword pieces it has only seen mid-word. A pack can lose its one-word fix while
its headline scores look normal, so measure the romanize and number-drop rates on
the quantized artifact and not only on the checkpoint.

B-vs-C shows the curve has not flattened — the
8,443-row one-word component beats the 3,836-row one by +1.48 chrF++ on the
holdout and 3.2 points of romanize rate. More of this data is still buying
something. Excluding the raw `dictionary` set (§22) cost nothing measurable.

The B→C step moved two variables at once — `oneword` 3,836 → 8,443 and the
addition of 3,142 paradigm pairs — so the gain is unattributed. If the paradigms
carry it, that is the cheap lever for every future language, since declension
tables are ground truth where generation is not. Run an ablation before
assuming more generated one-word data is the answer.

## 24. Arms D and E: the paradigm tier carries the gain, one-word scale is done

Section 23 could not attribute B→C because two things changed at once. This round
separates them, from arm C as the shared origin, on the same KD checkpoint and
the same finetune-distribution valid set. Both new components are increments:
v1 is carried into v2 byte-for-byte, so each step moves exactly one variable.

| arm | one-word | paradigms | rows | best ce |
|---|---|---|---|---|
| C | 8,443 | 3,142 | 127,628 | 0.75611 |
| E | 30,297 | 3,142 | 149,001 | 0.76012 |
| D | 30,297 | 24,130 | 166,606 | 0.75388 |

The 688-row one-word band combines both evaluation slices. The measures below
are not affected by casing:

| | existing | A | B | C | E | D |
|---|---|---|---|---|---|---|
| chrF++ case-folded | 48.43 | 49.93 | 54.32 | 55.82 | 56.55 | 57.24 |
| exact match | 38.2% | 37.4% | 42.4% | 45.9% | 45.9% | 48.3% |
| romanize rate | 11.9% | 12.9% | 10.8% | 7.6% | 8.7% | 7.0% |

Tripling the generated one-word component bought nothing. C→E is +22k pairs
of exactly the material sec 23 credited with the fix, and exact match does not
move (45.9% to 45.9%) while the romanize rate goes the wrong way, 7.6% to 8.7%.
The case-folded chrF++ gain of +0.73 without an exact-match gain is the same
signature sec 23 read on arm A's longer training: the output looks more like a
label without being the right label more often.

The paradigm tier carries all of it. E→D holds one-word fixed and takes
Wiktionary's declension tables from 3,142 to 24,130: exact match +2.4 points,
romanize −1.7, case-folded chrF++ +0.69. Against arm C the total is +2.4 exact
and −0.6 romanize, and every point of it is attributable to the paradigms.

That is the cheap lever sec 22 predicted, and it generalises: a declension table
is attested morphology, free for any language wiktextract covers, and it needs
only a gloss trusted from elsewhere. Generated one-word vocabulary is the
expensive half and it saturates in the low tens of thousands.

Off the one-word band nothing moved: probes +1.65 over C, subtitles/signs/ui
within ±0.5, ted −0.06, flores −0.28, crawl −0.19, and number drop is unchanged
because the numbers component is byte-identical across C, D and E.

Reading twenty holdout outputs says the residual failure is lexical, not
morphological. Every inflected item in the sample is now right across all
three arms (`gatskhelebistvis` → "For heating", `desertebis` → "Desserts",
`bileTebistvis` → "Tickets"). Arm D alone fixes `dabalze` → "Low" where C and E
both say "Lower", and `sakhvevi` → "Dressings" where C says "Twilight". What
remains is rare and loan vocabulary: `barkali` → "Barkali", `kandeli` →
"Candel", `loferi` → "Loffer" are still transliterations, and `dasvrili` →
"Crushed" and `daberva` → "Aging" are near-homograph senses. Paradigms cannot
reach any of those, and neither can more of the same generation.

### Generation yield decays fast, and naming what you already have is the lever

Round 3 covered 186 categories (92 old, 94 new) across 7 morphological modes,
2,603 jobs, 144,232 raw pairs, for 21,854 net-new after dedup: an 85% drop, of
which 75 points is duplication within the round itself. Listing the words a
category already holds in its own prompt, capped at 260 plus a 200-word global
list, costs about 1,200 prompt tokens and is the only thing that measurably
raised the yield.

The paradigm side inverts that ratio: 22,894 glossed forms produced 20,988 kept
pairs, an 8% drop, because the source side is attested rather than invented and
cannot collide with itself.

Sec 23's postpositional defect is fixed at source rather than filtered. Naming
the failure in the ask ("a bare preposition plus a bare noun is a gloss of the
Georgian suffix, not a sign") took the case-gloss rate in new material from
5.39% to 0.00%; the gate that catches the residue is case-sensitive on the noun
so that "From Above" survives and "From account" does not.

Two operational notes. `pgrep -f gloss_forms.py` in a wait loop matched a
monitoring shell that carried the same string in its own command line and hung
the chain for twenty minutes — the sec 12 trap, met again from the other side.
And an eval-slice sweep found 4,133 of the 24,130 paradigm forms already present
in the generated one-word set, so the two components overlap by 17% and the
increment must be deduplicated against the other component, not only against
its own previous round.

## 25. An fp32 gain is not a shipped gain

Arm D beat arm C on every isolated-word measure as a checkpoint and lost as a
pack. The comparison uses the same band, harness, and 688 rows:

| | case-folded chrF++ | exact | romanize |
|---|---|---|---|
| arm C fp32 | 55.82 | 45.9% | 7.6% |
| arm D fp32 | 57.24 | 48.3% | 7.0% |
| arm C int8 | 52.13 | 39.4% | 10.2% |
| arm D int8 | 50.29 | 39.7% | 9.0% |

Quantization costs arm C 3.69 case-folded chrF++ and arm D 6.95. D enters
1.42 ahead and leaves 1.84 behind; int8-D loses on 9 of 11 slices and wins none.
The +2.4 exact-match gain §24 credited to the paradigm tier becomes +0.3.

Every arm comparison in §23 and §24 was measured on fp32 checkpoints, and the
artifact that ships is int8. A gain can fail to survive quantization, and the
loss is not uniform across checkpoints — so a fp32 ranking does not license a ship
decision. Score the packed artifact before choosing between checkpoints, not
after.

Two candidate explanations remained to test.

### Calibration mismatch

`quantize_export.sh` feeds `devtest.ka` to
`--dump-quantmult`, and that file is 3,000 lines of crawl prose from beyond line
4M of the KD source. The deployment distribution is signs, labels and UI strings.
Arm D is the more specialised checkpoint — 24,130 paradigm pairs pushing it toward
isolated words — so a prose calibration set mismatches it further, which would
explain roughly double the sensitivity and would mean the advantage is
recoverable.

### The evaluation metric may be wrong for this band

On single words, chrF++ measures
character overlap, and a near-miss is no more useful on a sign than a wild miss.
Arm D wins exact match and romanize while losing chrF++, and its residual misses
land farther from the reference — which chrF++ punishes and a user does not care
about. If usability and chrF++ disagree in direction here, the gate was measuring
the wrong thing.

## 26. Why arm D lost, and why the eval said the wrong thing

The two hypotheses from §25 were tested, and neither explained the result.

Calibration does not recover the loss. Five calibration sets (crawl control; short/label
at 1k, 3k, 9k; mixed) against both arms: every arm D variant lands within ±0.4
case-folded chrF++ of the crawl control and none beats it, and arm C is slightly
worse under short calibration. Size is irrelevant. Swapping crawl prose for
one-word labels moves the median quantizer alpha by 4%, and int8 weight round-trip
error is 0.00329 against 0.00328 — neither half of the quantizer distinguishes the
arms. There is no free improvement here for any pack, en→ka included.

The sensitivity is to beam narrowing, not to quantization.

| band, case-folded chrF++ | fp32 b6 | fp32 b1 | int8 | beam cost | quant cost |
|---|---|---|---|---|---|
| arm C | 55.80 | 54.43 | 52.13 | −1.37 | −2.30 |
| arm D | 57.24 | 54.81 | 50.29 | −2.43 | −4.52 |

Arm D loses twice as much to beam-1 before quantization. Its top-1 margins on this
band are thinner, plausibly because 24k paradigm pairs flattened the output
distribution on multi-token targets. That is the thing to chase; re-quantizing
cannot reach it. Note slimt decodes greedily and escalates beam only on degenerate
output, so the beam-1 column is what ships.

chrF++ was scoring how wrong the wrong answers were. Decomposing the gap: 35.5%
of rows both exactly right, 32.0% both wrong with identical output, and 24.4% both
wrong with differing output — that last class contributes 144% of the net gap. The
exact-answer swap slightly favours arm D. Reading all 168 differing-both-wrong rows
and judging usability on a sign: 44 usable each, exclusive wins 13-13. Netted over
the band arm D is +2 usable rows of 688 while chrF++ awards arm C 1.84 points.
`უგაზო` → C "Ugazo" (useless) vs D "Gas-free" (correct); `უსრიალო` → C "Sliding"
(the opposite, on a safety label) vs D "Slide-free".

But the band was mis-constructed, and that is what decided it. 150 of its 688
rows have multi-word English references — 21.8% of rows but 30.4% of reference
characters, and corpus chrF++ is character-weighted:

| reference | n | arm C int8 | arm D int8 |
|---|---|---|---|
| one word | 538 | 59.03 / 43.3% | 59.60 / 45.0% |
| 2+ words | 150 | 45.42 / 25.3% | 40.36 / 20.7% |

Arm D wins the rows the paradigm tier was built for, under all five calibrations,
and loses the rest hard enough to flip the pooled sign. Some of that loss is §24's
case-gloss fix being punished by references written before it ("Tickets" against
*For tickets*), but not all — "Metallodetictor", "Witters" for *Display Cases* are
regressions.

Arm C stays live. Before this is revisited: split the eval so the 538 genuine
one-word rows are reported separately on exact match and romanize, and chase the
beam-1 margin rather than the quantizer.

## 27. Proposed: scan the corpus for romanization instead of guessing at it

Not run. Written down because the three-word anecdote §22 and §26 lean on is a bad
instrument and this replaces it.

### Question

Is the romanization failure a short head or a long flat tail? If
a few hundred word types produce most of it, a targeted lexicon is worth building.
If it is flat, no lexicon of any size helps and the register-sourcing route (§26)
is the only option. Nothing measured so far answers this: §21 established the
cause, §24 showed which lever moves it, but the size and shape of the failing
vocabulary is unknown.

### Method

Run the shipped int8 pack through slimt over a large Georgian sample.
Extract output tokens, keep those absent from an English dictionary, count by
frequency. No reference needed — romanization is detectable monolingually, which
is what allows this to run over millions of lines where no gold exists.

Run it over three populations and compare, because the failure is
length-conditioned: `kd_src` (prose baseline, and `kd_ref` gives a reference-based
cross-check on the same lines), the HPLT short extract (19k one-word and 295k
2-4 word candidates already pulled), and the pool's `ui` and `dialogue` registers.
A non-dictionary rate of ~1% on prose against ~8% on short text would confirm the
conditioning.

Proper nouns are supposed to romanize. `ფრანჩიაკორტა` → "Franciacorta" is
correct, `ტრაინიკი` → "Trainic" is not, and Georgian is unicameral so there is no
case signal. Frequency is the separator available: a non-English token appearing
hundreds of times is a common noun the model cannot translate, while proper nouns
spread thinly across many distinct types. The type/token distribution therefore
does double duty — it answers the head-vs-tail question and filters the class we
do not care about.

Cross-reference against corpus occurrence to split "never seen" from "seen and
not learned". Both numbers are available in the same pass, and §26 showed the
distinction matters: dictionary coverage tracks corpus frequency monotonically, so
a word absent from `kd_src` is also absent from Wiktionary.

The cost is CPU only on bigserver. Measure the slimt rate on 10k lines before
committing to millions.

### Decision

A short head justifies a few hundred to a few thousand
targeted terms, glossed in sign register by a frontier model — affordable, and
free of Wiktionary's unreliable senses. A flat tail closes the lexical route for
good and points at Georgian retail catalogues and matsne's official bilingual
codes (§26).

## 28. The shortlist was the "quantization cost", and it was built from the wrong corpus

Found 2026-09-01 by decoding the shipped int8 packs with and without their
`lex.50.50.*.s2t.bin`. The same model, same quantization, read by hand:

| input | with shortlist | without |
|---|---|---|
| ცეცხლმაქრი | Fire extingerier | Fire extinguisher |
| შეცდომა 404 | Error 44 | Error 404 |
| Fire Extinguisher | ცეცხლმასაშენი (non-word) | ცეცხლმაქრი |
| Emergency Exit | საავარაო გასასვლელი (misspelt) | საევაკუაციო გასასვლელი |
| Push | დაასაბუთეთ ("prove") | დააჭირეთ |
| On | გათიშულია ("off") | ჩართულია |
| Keep out of reach of children | არ მიუახლოვდეთ ბავშვებს ("do not approach children") | შორს იქონიეთ ბავშვებისგან |

`flows/pack.py` hands `shortlist.sh` the aligned KD corpus and caps it at 2M lines
(`SHORTLIST_MAX_LINES`), so the table was built from a 1-in-4 or 1-in-5 sample of
the 4M KD decode and nothing else. The finetune corpus, which is where every
sign, one-word and paradigm pair lives, was never in it. The table then offers
only pieces that fast_align saw aligned in KD text. For the pieces of ცეცხლმაქრი
the top-50 targets in `lex.s2t.clean` contain `ex`, `ting`, `ish` and never `u`,
so "extinguisher" cannot be emitted and the decoder assembles the nearest
reachable string. Digit pieces are thin for the same reason.

Measured on the int8 packs, chrF++ with shortlist / without: ka→en oneword_ho
48.05 / 53.01, oneword 48.64 / 52.35, signs 67.97 / 70.08, ui 68.47 / 70.18,
probes 60.40 / 61.93, flores 47.92 / 48.79; en→ka signs 60.86 / 64.71, flores
46.36 / 46.70. The loss is largest on exactly the registers the app serves. The
no-shortlist int8 numbers match the fp32 arm C figures in §23 within about 0.3,
so what §25 and §26 chased as quantization and beam-1 sensitivity was the
shortlist. Arm D lost as a pack for the same reason: its 24k paradigm pairs are
the population the table had never seen. Arm D should be re-scored with a
rebuilt table before it is judged.

The Mozilla en-de pack loses only a little to its shortlist ("Notauszug" for
"Notausgang") because its table covers the whole training corpus, finetune
included.

Rebuild 1: KD 1-in-4 sample plus the finetune corpus repeated five times, same
`50 50 0` parameters. ka→en oneword_ho 48.05 → 51.00 (none 53.01), one-word
exact match 38.3% → 43.0% (none 46.3%), signs 67.97 → 69.55, ui 68.47 → 69.46;
en→ka flores 46.36 → 46.54. Six of eight en→ka probe lines now match the
no-shortlist output. Decode time and memory are unchanged against the old table.
Rebuild 2, finetune ×10 and firstNum 200 with bestNum 100: oneword_ho 51.77,
one-word exact match 43.8%, "On" → ჩართულია fixed, decode time unchanged. Digits
were still wrong: "შეცდომა 404" → "Error 44", "ოთახი 404" → "Room 440", while
403, 500, 4040 and 45.99 were fine.

The digit failure is a segmentation mismatch, not a frequency problem. The
student trained with `sentencepiece-alphas: [0.5, 0.5]`, and under that sampling
"404" is segmented ▁4+04 about 40% of the time. The shortlist corpus was encoded
deterministically, where "404" is always ▁40+4, so the piece `04` (id 6070) never
aligned to source `4` and is unreachable under any table. The decoder starts its
preferred ▁4 path, finds `04` missing, and takes `4`: "44". The same mechanism
puts every alternative segmentation the sampled trainer learned out of the
table's reach, digits are only the visible case. Rebuild 3 encodes the target
side of the alignment corpus twice, deterministic and sampled at alpha 0.5, so
the table learns the pieces the decoder actually emits.

Independently of the table, slimt now admits every digit-only target piece
whenever the source carries a digit piece (`ShortlistGenerator::generate`,
after the frequency top-up so it never displaces the top-up budget). With the
rebuild-2 table that gives "Error 404", "Room 404", "Pressure 140/90" and moves
numbers +0.15 and probes +0.26 with every other slice unchanged; regression
cases for the three inputs live in `slimt-sys/tests/regression.rs` (pair
`kaen`). Adding the digits inside the top-up budget instead had shifted two
en→es cases ("Náufrago" → "Násteula"), which is why the order matters.

Rebuild 3, target side encoded twice (deterministic and sampled at alpha 0.5,
`--nbest_size=-1`, the equals form; the space form fails gflags parsing) and
aligned to the same deterministic source: the best table measured. ka→en
oneword_ho 51.67, oneword 50.92, signs 69.91, ui 69.91, probes 61.83, flores
48.61; one-word exact match 43.8% holdout / 44.8%; decode time unchanged
against the old table, 3.7 MB against 2.1 MB. Within noise of `none` on every
slice except the one-word band, where `none` keeps 53.01 / 46.2%.

What the residual is. On the one-word holdout the two decodes differ on 62 of
400 rows; `none` alone hits the reference on 11 of those, the table alone on 1.
Reading the 62: the table's exclusive losses are a missing piece inside a word
the model otherwise knows (ბორდინგი → "Bording", სილანტი → "Silant", დოზატორები
→ "Dosers", სუნები → "Soams"), and the rest are rare vocabulary both decodes get
wrong in different ways (ფუგა, საფაღარათოები, ნივთმტკიცებები). So the corrected
table costs about one isolated word in forty a truncated spelling, and the
larger one-word problem is the model's own vocabulary, which the table cannot
touch and the finetune data can.

Collapse-triggered fallback, tried and reverted. The deficit router that
already re-decodes low-confidence rows with beam was changed to re-decode them
over the full vocabulary. It flagged only 65 of 288 one-word rows and 68 of
400 holdout rows, because a starved decode is usually confident: the wrong
piece wins cleanly and the deficit never spikes. It flagged 61% of FLORES rows
and 87% of en→ka FLORES rows, where the deficit accumulates over length and the
table was fine. Net: +0.2 to +0.7 points of one-word exact match for ted 5.2 →
7.9 s and en→ka flores 15 → 25 s (none: 10.9 s and 29 s). Not shipped. The
router does catch the starvation cases that do collapse, and beam on the
shortlist still helps the confident-early-token-then-collapse case, so the
existing behaviour stays.

Arm D re-scored with its own rebuild-3 table (KD 1-in-4 + armD.train.tsv ×10,
sampled target). On the 538 isolated-word rows arm D beats arm C 52.2% vs 49.1%
exact match, 5.9% vs 7.6% romanize, 64.71 vs 62.47 case-folded chrF++; on the
150 multi-word rows D still trails (24.7% vs 26.7%); off-band slices are within
noise (signs and ui favour C by ~1.5 and 0.5, probes favours D by 1.9). The
§25/§26 conclusion that arm D lost as a pack was the table, not the checkpoint.
Arm D is the base for the next ka→en finetune.

Decoding without any shortlist costs 1.7-2.3× wall time on CPU. The remaining
choice for this pair is the rebuild-3 table with the runtime digit rule against
`none`, and the difference is the one-word residue above.

Two eval defects found while reading the same outputs:

- 42 of the 175 lines in `probes/check.en`, including most of the 67 `signs`
  lines, are verbatim in the shipped en→ka finetune corpus (`ft2/ft.src`). The
  generated signage set that the check lines were drawn from never went through
  the eval exclusion list, which was only applied to the harvested band. The §15
  signs gain is partly memorisation, and the model still gets several of those
  training lines wrong ("Sign out" → შესვლა). The ka→en slices are clean against
  `ft3/armC.train.tsv` (0 of 4,000 rows overlap).
- The `slimt_load_test --align` display slices source text by byte offset while
  slimt reports character offsets, so Georgian alignments look like garbage in
  that tool while the runtime (`dom_translate.rs`, char-based) is fine.

How the packs were assessed, so it can be repeated: decode every eval slice with
`slimt_load_test` twice (shortlist and `none`), write side-by-side files
`SRC / REF / SL / NONE`, read the short slices whole (probes 67, signs sample 60,
one-word 80, en→ka signs 175) and treat chrF++ only as direction. Every finding
above came from a pair that scored fine.

## 29. Finetune v3: luna signage tranche, KD rehearsal, and an enforced eval gate

2026-09-02, one RTX 3090, 89 min, $0.31. Corpora on bigserver at `ft4.enka/` and
`ft4.kaen/`: shipped finetune set (ft2 for en→ka, arm D for ka→en) + 46,448 luna
round-1 rows from `gen_pairs.py` (65,278 raw, 71.2% kept, judge 93.9% ok) + a
1:1 KD rehearsal sample, everything passed through `exclude_eval.py`, which
`finetune_student.sh` now requires. Shortlists rebuilt with the §28 recipe over
the new finetune corpora (`sl5.*`), packs at `pack.enka.ft4` and `pack.kaen.ft4`.
Nothing published.

What the gate found. 1,759 rows of the shipped en→ka training corpus matched
`ft2/valid.ft2.tsv`, so the previous en→ka early stopping was partly measured on
memorised rows; 528 matched the one-word adversarial list ("No", "Yes",
"Right"). On ka→en, arm D scored 56-64% exact match on one-word rows whose
answer was in its training data against 38% on rows whose answer was not, a
twenty-point memorisation gap. The one-word gains credited in §23 and §24 were
substantially measured on contaminated rows and should be read down accordingly.

en→ka, int8: flores 46.62 → 46.93, the 67 referenced signs 64.02 → 65.58, and the
table now costs 0.13 against `none` where the old one cost 0.69. Read pairs: "Keep
out of reach of children" is now the reference exactly (was "do not approach
children"), "Connect to network" moved to the imperative, "Iced coffee" to ცივი
ყავა, "Firmware" dropped the ფირმულური calque. New losses: "Allow the engine to
cool" came back as ჩართეთ ძრავა ("turn on the engine"), a sense inversion, and
"Tighten the bolts" is still wrong in a new way. Net better; ship candidate.

ka→en, int8, against arm D with its corrected table: flores 48.27 → 49.35, ted
54.18 → 55.27, crawl 38.40 → 39.58, probes 63.73 → 64.87; signs 68.27 → 67.21,
ui 69.38 → 67.80, subtitles 68.04 → 67.43, numbers_ho 76.30 → 74.02, one-word
exact 46.2% → 43.0%, romanize 7.7% → 7.4%. Splitting the short slices by whether
the gate removed that row's answer from training: on rows whose answer was in
arm D's data the new model is 5-8 chrF lower (memorisation gone); on clean rows
signs −0.5, ui −1.3, numbers_ho −2.1, oneword +1.5. Read pairs: მოქაჩეთ → "Pull"
(was "Mottor"), "Payment declined", "Do not touch", "before disconnecting the
hose" all fixed; losses are "Expiry date" → "Shelf life" and rare one-word items.
The rehearsal bought the prose gain without the previous trade.

One defect in the gate as run: the either-column rule dropped 435 arm D rows
whose English target ("Ironing", "Boil") is also the reference of a different
Georgian word in `oneword_ho`. Pair-level leakage on that slice is zero, so those
were legitimate training rows, and the ka→en one-word and numbers slices paid for
their absence. For X→en holdouts the rule should be source-side or pair match,
not either column. Rebuilding the ka→en corpus with that rule and retraining
costs about $0.15 and no re-decode; do that before deciding on the ka→en pack.

Rounds 2 and 3 of the tranche (~60k more rows) were still generating when this
ran and are the input to the next iteration.

## 30. Finetune v4 (ft5): full tranche, pair-aware gate; en→ka published, ka→en held

2026-09-02, two RTX 3090s in parallel, $0.28. Same recipe as §29 with the full
132,969-row luna tranche (three rounds, judge 92.8% ok) and `exclude_eval.py
--eval-pair`, which matches a held-out reference only together with its source.
On ka→en the pair rule dropped 262 rows against 1,204 under either-column and
found zero pair-level leaks, so the 435 arm D one-word rows §29 flagged were
legitimate and are back in. Corpora 506,848 (en→ka) and 598,434 (ka→en) rows.

en→ka, int8 with its table: FLORES 46.36 shipped → 46.93 ft4 → 47.36 ft5,
COMET22 82.93 → 83.89, the 67 referenced signs 60.86 → 68.55. Read: "Sign out" →
გამოდით (shipped and ft4 both said "sign in"), "On" → ჩართულია (shipped said off),
"Keep out of reach of children", "Fire Extinguisher", "Beware of dog", "Detour",
"Pull", "Iced coffee", "Expiry date" all fixed; "Push" → დააჩქარეთ ("hurry") is a
new loss and some UI labels drift to imperatives. No prose slice regressed.
Published as `en-ka/lmt60_distill_20260902` via the new `publish_pack.sh`;
the 20260831 pack stays addressable.

ka→en, ft5 against the arm C pack WITH THE OLD TABLE (the comparison the run
used; §31 corrects it against live): flores +1.56, ted +1.05, crawl +0.78,
probes +8.63, one-word exact 39.4% → 44.8%, but subtitles −1.10, ui −1.15,
numbers_ho −3.30 and COMET22 82.60 → 82.45, the lowest of the four candidates.
Fails the selection rule (no prose slice may lose more than ~0.3, no safety
regression). Not published. Against live (arm C + rebuild-3 table) the picture
is worse: signs −1.60, ui −2.60, numbers_ho −3.94, and the one-word gain
disappears, because that gain was the table.

The cheapest ka→en win is not a checkpoint: arm C read through its rebuild-3
table beats the shipped pack on every slice (signs 67.97 → 69.91, ui 68.47 →
69.91, oneword_ho 48.05 → 51.67, flores 47.92 → 48.61) and arm D with its own
rebuild-3 table does the same with a larger one-word lead, part of which is the
§29 memorisation. Neither is packed. That decision is open.

Why ft5 loses ka→en subtitles and numbers while gaining prose and probes is
not yet read; the candidates are the 1:1 rehearsal shifting the length prior
and the tranche's ₾/decimal conventions. Read before the next iteration.

## 31. Why ft5 lost ka→en short slices, and what the eval was really measuring

Read 2026-09-02 from the differing rows, a blind source-only luna A/B over every
differing row, and corpus statistics on bigserver. Notes in the session
scratchpad `kaen_ft5_investigation.md`.

The ui and subtitles slices are not held out. Normalised match against the 4M
KD training corpus (`train.ka2en/aligned/train.tsv`): ui 293 of 300 sources
and 166 of 300 full pairs are in it, subtitles 416 of 500 and 157 of 500;
numbers, numbers_ho and flores 0. Arm D's finetune corpus also held 16 ui and
15 subtitles eval pairs verbatim, which ft5's pair-aware gate removed. On the 40
worst-regressing rows the live pack equals the reference exactly on 23 (ui), 21
(subtitles) and 22 (numbers_ho). Those slices measure KD reproduction, and a
1-1.5 chrF move there should not veto a pack. Rebuild them from text outside
the pool, or report only the clean stratum (ui n=7, subtitles n=84).

Blind A/B, luna judging source against the two hypotheses with no reference,
order randomised: subtitles live 77 / ft5 69 / tie 99 of 245 differing rows
(net −8 of 500); ui 46 / 32 / 67 of 145 (net −14 of 300, real but a third of
the chrF gap); numbers 42 / 46 / 101; numbers_ho 64 / 46 / 125; probes 1 / 11 /
24. Register drift is measurable on ui: mid-sentence capitals 10 in the
references, 13 live, 30 ft5 ("Color Palette", "Read Directory A"); lower-casing
recovers 0.53 of ui's 2.60. Length ratios move by at most 0.012 on any slice.

Numbers, split: currency written GEL where live wrote ₾ or lari (16 / 22 rows),
decimal comma → point (7 / 7), thousands separators added (8 / 6), digit set
changed (11 / 12), other wording (153 / 194). Digit fidelity against the SOURCE
on numbers_ho: live 384 ok / 5 omitted / 1 corrupted, ft5 379 / 10 / 3, with
the real cases "198 volts" → "18 volts", train 832 dropped, 1201000 →
"120,1000", 2387 → "237", "250 °C" dropped. On prose ft5 is better (flores 78 →
82 ok). The conventions come from the tranche: its English side writes GEL
4,421 times against lari 207 and ₾ 53, point decimals 3,824 against comma 3,
while the references mirror the Georgian ("8,75 ₾" stays "8,75 ₾"). The app
overlays onto a photographed price tag, so the source's own symbol and
separator are what belongs in the target; the tranche should change, not the
references. Normalising both sides recovers only ~0.5 chrF; the rest of the
numbers_ho gap is on long rows (>5 words −4.13, ≤5 words −2.47; numbers ≤5
words +1.42).

Hypotheses. Rehearsal ratio, falsified: English-token share of rehearsal is
64.3% in ft4 and 64.1% in ft5; what changed is luna 7.8% → 16.2%. The tranche is
29.7% digit-bearing but only 1.5% digit-and-long, so the finetune's
short-numeric to long-numeric ratio went 3.0:1 → 5.7:1, which is the mechanism
for the long-row omissions. The swapped tranche matches the short slices in
length (5.46 words mean vs ui 4.02, subtitles 4.61) and mismatches them in
target style: a bigram-LM probe puts the probe and signs references on the
luna side and subtitles on the OPUS side, and ft5 moved toward luna on every
short slice, so probes rewards what ui and subtitles punish. The valid set
(`ft2/valid.ft.ka2en.tsv`, OpenSubtitles/TED/KDE, 6.4 words mean) is biased
toward the incumbent style and still picked this checkpoint; it holds no long
number-dense rows and cannot see the numbers regression.

Probes +7 against live: of 36 changed rows read against the source, 11-13 are
substance ("before disconnecting the hose", "500 working hours", "Untreated
water, do not drink", "Payment declined"), 9 are luna-style agreement ("Do not
touch" vs "Don't touch it"), the rest ties. Direction real, magnitude inflated.

Next iteration for ka→en, corpus only plus one ~$0.15 finetune: rewrite the
tranche's English number conventions at build time to mirror the Georgian row
(currency symbol and separators copied, ~4,400 rows); add a long-numeric band
(over-sample the rehearsal's 20,724 digit-and-long rows ~3× or generate a
10-20k band); sentence-case UI labels in the prompt; leave the rehearsal ratio
alone. Expected against ft5: numbers +1.2 to +1.7, numbers_ho +2.5 to +3.5, ui
+0.5, subtitles unchanged, probes and prose held. Eval: score digits as a
multiset after stripping separators, accept ₾/lari/GEL and comma/point as
equivalent, and gate on source-vs-hypothesis digit fidelity directly, which is
what caught the one real ft5 defect chrF buried.

## 32. ft6: the number conventions were never the cause; the gate refused it

2026-09-02, one RTX 3090, 43 min, $0.11. The §31 recipe applied as reusable
build steps: `build_tranche.py` rewrote 4,328 tranche rows so the English
figure, separator and currency mirror the Georgian row (71 rows dropped where
the two sides already disagreed on digits, 36 UI labels sentence-cased);
`sample_rehearsal.py` tripled the rehearsal's digit-and-long rows (26,331 →
78,993) inside an unchanged 1:1 budget; 598,341 rows. Same valid set as ft5.
Pack `pack.kaen.ft6` with the `sl7.kaen` rebuild-3 table. Not published.

Against live (arm C + rebuild-3 table), int8 with own table: flores +0.36, ted
+0.74, crawl +0.86, COMET22 82.73 vs 82.60 (best of the finetunes), probes
+6.05, one-word exact 44.3% vs 44.2%; subtitles −1.83, signs −1.65, ui −1.38,
numbers −2.39, numbers_ho −3.90. Blind source-only A/B: ui net −3 of 300 (ft5
was −14, so the casing fix worked), subtitles −18 of 500, numbers_ho −23 of 400,
probes +8 of 67.

The conventions moved: on numbers_ho ft6 writes ₾ 3× and lari 25× where the
references write 3 and 25 (ft5: 0 and 6), decimal commas 7 (reference 7), GEL
22 → 8. The figure loss did not move with them: source-vs-hypothesis digit
fidelity on numbers_ho is live 3 bad lines, ft5 9, ft6 11, with 2387 → 237,
1201000 → 120,000, 7002 → 702, ISO 10012:2003 → 101012:2003, 13 საათზე → "1
a.m."; identical with shortlist=none, so it is the checkpoint. The long-numeric
band recovered a few omissions ("every 24 months") and cost more than it bought.

The new `gate_pack.sh` requires a `number_fidelity.py` report for the candidate
and for live on the same slices and refuses when the candidate omits or corrupts
more figures on any slice; it refused ft5 and ft6. `score_strata.py` reports
each slice split into pair-leaked / source-only / clean against the KD corpus.

Reading: every finetune that contains the luna tranche re-segments digit runs,
in proportion to tranche size (ft4 → ft5 → ft6), while arm C never saw that data
and keeps 4-7 digit strings intact. The tranche's numeric form is 36k rows of
one-to-three-digit figures, so the decoder learned a short-digit-run prior and
emits too few pieces on longer runs; "2387" is not long, it is longer than the
prior. Adding a band of long figures would move the prior rather than remove it.
The fix under test in ft7 is an offline augmentation (`perturb_numbers.py`)
that rewrites every figure identically on both sides of numeric pairs, K
variants per row with long-tailed lengths, so the digits can only be copied
from the source; measured by the same fidelity gate.

## 33. ft7: number perturbation fixes the digits and the way it was applied diluted the labels

2026-09-02, one RTX 3090 Ti, 47 min, $0.15. `perturb_numbers.py` rewrites every
figure in a numeric pair identically on both sides, keyed on the canonical
value so "8,75 ₾" and "8.75 GEL" agree, re-printed in each occurrence's own
shape, with hour/day/month/year roles respected and whitespace token counts
held so the alignment column stays valid; lengths long-tailed toward 4-7
digits. Applied at K=3 to the ft6 finetune rows: 48,343 eligible → 143,928
variants, every one verified to carry identical digit multisets on both sides;
4-8 digit runs went from 9.3% to 30.6% of the finetune band. Fresh 1:1
rehearsal, same valid set. Pack `pack.kaen.ft7`, table `sl8.kaen`. Not published.

Digits: source-vs-hypothesis bad lines on numbers live 5 / ft6 7 / ft7 5, on
numbers_ho 3 / 11 / 6 (one of the six a scoring artefact on "2754 03:25"); six
of the eight §32 lines repaired (2387, 7002, ISO 10012:2003, "13 o'clock", the
eleven-digit id, 198 volts), 1201000 still wrong, train 832 kept but attached to
the wrong noun. Blind A/B on numbers_ho went from −23 (ft6) to +4. Prose and
probes gained again: flores +0.90, ted +1.25, crawl +1.01, probes +6.55 over
live, COMET22 82.88.

Cost: signs −3.27, ui −3.54, subtitles −2.13, one-word holdout −3.3 against
live, and the A/B attributes ui (−16) and subtitles (−12) to lexical and meaning
changes, not style. Mechanism measured rather than guessed: the variants add no
new content, so the non-numeric share of the finetune half fell from 83% to
56%, and every short-label register was diluted. The gate refused the pack on
numbers_ho (6 vs 3) and the A/B clause.

So the hypothesis holds and the dose was wrong. Next: the same perturbation
without expansion (K=1 in place, or perturb only the long-numeric band, or
repeat the non-numeric rows to hold the share at 83%), about $0.15. If that
keeps the digit gain with labels flat, it is the ka→en pack to ship. The result
also argues for a streaming OpusTrainer Numbers modifier, which perturbs a
probability of batches at read time and leaves the corpus mix untouched, the
variable that broke this run; open that upstream only with a shipped result
behind it.

Disk: the shortlist intermediates of passes 1-3 and the ft4 artefacts were
deleted on 2026-09-02 (46 GB); the tables in `sl*/out/*.bin`, build logs and
scripts were kept.

## 34. ft8: the perturbation in place holds the labels and does not buy the pack

2026-09-02, one RTX 3090, 46 min, $0.13. `perturb_numbers.py` gained a second
dose. `Emit.APPEND` is what ft7 ran, the row plus K perturbed copies;
`Emit.REPLACE` emits ONE perturbed row in place of the original for a `--share`
of the eligible rows and leaves the rest as written. At `--share 0.7` over the
same ft6 aligned finetune material, 48,343 eligible rows produced 33,541
replacements (258 dropped because they would have moved a column's whitespace
token count), and the finetune half came out at ft6's 299,242 rows, 83.0%
non-numeric, short-to-long numeric 2.44:1 — all three identical to the input,
which is what ft7 could not do. Digit runs of 4-8 digits are 27.4% of it against
ft6's 9.3% and ft7's 30.6%, so the digit dose is nine tenths of ft7's with the
register mix held fixed. Append mode still reproduces ft7's corpus byte for
byte from its own seed. Fresh 1:1 rehearsal at the ft6 boost, same valid set,
598,350 rows; pack `pack.kaen.ft8`, table `sl9.kaen`. Not published.

Against live (arm C + rebuild-3 table), int8 with own table, with ft7 alongside:

    slice          live     ft7     ft8   ft8-live  ft8-ft7
    flores        48.61   49.51   49.64     +1.02     +0.12
    ted           54.23   55.48   55.35     +1.12     -0.13
    crawl         38.69   39.70   39.46     +0.77     -0.24
    subtitles     68.29   66.17   66.61     -1.68     +0.45
    signs         69.91   66.64   67.01     -2.89     +0.37
    ui            69.91   66.38   67.36     -2.56     +0.98
    oneword_ho    51.67   48.35   48.28     -3.39     -0.07
    numbers       74.10   73.40   71.60     -2.50     -1.80
    numbers_ho    76.56   75.14   72.34     -4.23     -2.80
    probes        61.83   68.38   66.06     +4.23     -2.32

COMET22 on FLORES 82.86 (live 82.60, ft7 82.88). One-word band exact 43.3%
against live's 44.2% and ft7's 44.3%.

Digits held. `number_fidelity.py` against the Georgian source: numbers 3 bad
(live 5, ft7 5), flores 3 (live 5, ft7 4), probes 0, numbers_ho 7 (live 3, ft7
6); identical with shortlist=none, so it is the checkpoint. Seven of the eight
ft6 failure lines are right — 2387, postal code 7002, ISO 10012:2003, 13
საათზე, the 01012034567 id, 198 volts, and train 832's digits — and 1201000
still comes back "120,1000". Two of the seven numbers_ho counts are measurement
artefacts the live pack shares ("რეისი 8529 23:05" reads as one numeral to the
measure; "420 ათასი დოლარი" written "$420,000" is correct English), so the real
count is five against live's two.

The dilution was real and it was not the whole cost. Every register ft7 diluted
came back part of the way — ui +0.98 over ft7 and its blind A/B −16 → −7,
subtitles +0.45 and −12 → −11, signs +0.37 with a blind A/B of +26 of 500,
oneword +0.77 — while prose held. What moved the other way is the number-dense
prose: numbers −1.80 against ft7, numbers_ho −2.80, and the numbers_ho blind
A/B +4 → −26, decided on lexical and grammar rather than on figures. Reading
the strata, ft8 is above ft7 on every clean stratum it shares (signs 67.12 vs
66.70, subtitles 67.96 vs 66.27, ui source-only 63.99 vs 62.32) and below live
on all of them.

The gate refuses it on numbers_ho, 7 lines against live's 3, and the selection
rule fails again on the ui and subtitles A/B and on the one-word band. Three
runs now say the same thing: the perturbation fixes the digit copying at any
dose that reaches the numeric rows, and every corpus-side way of delivering it
costs the short-label registers something, because 33,541 rows of the finetune
half are now figures nobody wrote. The corpus is the wrong place for this. The
next lever is the streaming OpusTrainer Numbers modifier, which perturbs a
probability of batches at read time and leaves both the corpus and its mix
alone; ft7 and ft8 together are the shipped-adjacent result that argues for
opening it upstream.
