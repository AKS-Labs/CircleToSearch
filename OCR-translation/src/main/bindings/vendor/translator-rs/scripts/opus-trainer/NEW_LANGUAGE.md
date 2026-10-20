# Add a language pair

This is the canonical guide for adding and training an `en↔X` pair. It covers
language-specific checks, data preparation, teacher selection, distillation,
student training, evaluation, and packaging.

Use [`NOTES.infra.md`](NOTES.infra.md) for machine, container, and Vast.ai
details. Keep measurements from a particular pair in its findings or run record.

## Workflow

Complete the stages in order:

1. Define the language, production inputs, and release requirements.
2. Inspect writing-system, normalization, segmentation, and tokenization behavior.
3. Inventory and qualify the available corpora.
4. Gate a teacher independently for each direction.
5. Prepare cleaned data, held-out data, and the joint vocabulary.
6. Decode the KD source and select or filter teacher outputs.
7. Align every training block and train the student with a curriculum.
8. Evaluate the quantized artifact on every relevant slice.
9. Package the model and complete the release checks.

Before paid compute, record the language profile, corpus inventory, contamination
checks, teacher decision, evaluation sets, and budget. Stop when a required gate
fails.

## 1. Define the pair

Record:

- the dataset language code and any teacher-specific language codes
- the two translation directions
- the target script and any script variants
- the definition of production inputs
- the available native-speaker review
- the model size, latency, and package constraints

Use separate decisions for `en→X` and `X→en`. Data availability, teacher
quality, morphology, and evaluation behavior can differ by direction.

### Production inputs

Define production inputs as the range of text encountered during ordinary daily
use. This is deliberately broad. It can include a short sign or menu label, a
casual message, a Wikipedia article, or a government letter.

This definition describes coverage rather than priority. No category is assumed
to dominate the others. Evaluate both short and long inputs explicitly because a
model can handle longer prose while treating short input as out of distribution,
then looping, copying, or hallucinating.

## 2. Inspect the language behavior

### Writing system and casing

Check whether the language has case distinctions and whether the script contains
case variants or display forms. Read the actual OpusTrainer modifiers before
choosing augmentation settings.

For a caseless language or a script whose transformed characters leave the
training vocabulary, disable the affected case modifiers in a pair-specific
configuration. Verify the training log reports the intended modifier weights.

Check normalization at the translation input boundary. Preserve the original OCR
text for copy and per-word paths when normalization is only required by MT.

### Sentence segmentation

Run the runtime splitter on representative text containing at least two
sentences. Check full stops, question marks, exclamation marks, abbreviations,
ellipses, and non-breaking prefixes.

The splitter used for corpus preparation must match the runtime splitter. Split
sentences before the KD draw so source and reference rows remain aligned and the
draw remains reproducible.

### Vocabulary

Measure teacher output tokens per character for both directions. Use the result
when estimating KD cost and decode time. For a causal-LM teacher it is also a
coverage signal: a language that runs at about one token per character in the
teacher's vocabulary was nearly absent from its pretraining, and lexical errors
on that side should be expected before any decode runs.

Train a pair-specific joint SentencePiece vocabulary with `split_digits`, so
every digit is its own piece. Without it a figure segments differently depending
on its neighbours ("2387" as ▁23+87 or ▁2+387) and copying it becomes a guess
the decoder makes from its training prior; a student finetuned on label-style
data with many short figures then truncates longer ones ("2387" → "237",
"7002" → "702") while every digit piece is available. `prep_data.py` sets the
flag and `check_vocab.py` refuses a vocabulary whose figures segment into
multi-digit pieces. Changing this on an existing pair means a new vocabulary and
therefore a KD re-decode and student retrain; bundle it with the next re-decode
rather than running one for it. Check piece count, unknown tokens, encode/decode
round trips, and fertility for each side. A low unknown rate does not prove that
the vocabulary represents the script efficiently because byte fallback can hide
a missing script.

### Evaluation behavior

Choose metrics after inspecting the language. Morphology, tokenization, word
order, and script properties affect the relationship between surface metrics and
translation quality.

Use chrF and spBLEU where they provide useful comparisons. Use COMET only with
the language-coverage and calibration limitations understood. Treat metrics as
system-ranking tools when human-judgment coverage is limited.

Create production-input probes for signs, menus, warnings, dosages, entities,
short inputs, and content words. Score numbers as a digit multiset against the
SOURCE after stripping separators, treating currency spellings and decimal
conventions as equivalent, and gate on that fidelity directly: chrF++ buries a
dropped or altered digit under formatting differences the references charge
for, and the target should mirror the source's own symbol and separator when
the output is overlaid on a photographed price or label. Include adversarial cases for numbers,
negations, copying, repetition, and length blowup. Review the translations by
kind rather than reducing the probe set to one score.

Evaluate the teacher on short inputs as a separate slice. Use one-word, short
sentence, conversational, and other production-input examples. Check whether the
teacher preserves meaning, stops at the right point, and handles entities,
numbers, and negation. A news benchmark does not establish short-input quality.

## 3. Inventory and qualify the data

Start with the OPUS inventory:

```text
curl -sL "https://opus.nlpl.eu/opusapi/?source=en&target=<lang>&preprocessing=moses&version=latest"
```

Compare the result with the corpus allowlist in `registers.py`. Record the
available pair counts, the largest contributors, missing sources, unique source
and target lines, and the amount of usable human text.

The standard preparation filters remove formatting artifacts, malformed wiki
markup, invalid character or word ratios, number and URL mismatches, excessive
word lengths, and wrong-language rows. Inspect the filter implementation before
changing a default, and report what each filter removes by corpus and register.

Qualify the non-English source independently of pair alignment. A mined pair can
be well aligned while its non-English side is itself an upstream machine
translation: correctly encoded and spaced, yet calqued, ungrammatical, or
semantically unreliable. Mechanical rules can remove invalid encoding, broken
script mixtures, and other surface damage, but cannot establish that apparently
fluent text was written naturally in the language.

Draw a uniform sample from each important corpus and register before committing
to KD. Have a native reviewer, or a capable model with native review of its
calibration set, classify the source side for native quality and translationese.
Use that sample to estimate the dirty rate and decide whether the corpus remains
worth keeping; do not infer the rate from rows selected by a proposed filter. If
trusted native monolingual data is available, prefer decoding it with the teacher
to trying to repair a corpus whose source was machine-generated upstream.

When no replacement corpus is available, train an acceptability/translationese
classifier to rank the existing source. It needs reviewed clean and degraded
examples: a clean-only language model may help mine suspicious candidates, but
will also rank names, rare terminology, and legitimate register differences as
unusual. Calibrate the classifier's cutoff on a separate representative reviewed
sample, then retain only the confidence range whose false-positive cost is
acceptable.

Measure a defect rate before building a filter for it. Estimate it from a
uniform random sample judged directly. A rate read off the filter's own output
measures the filter. Keep any deliberately skewed sample separate and use it only
to evaluate a proposed cut.

Character and token models detect orthographic damage. They do not detect a
source that is fluent but wrong, which is correctly spelled and scores near the
median of its length. Detecting that requires the reference side of the pair.
State which of the two an instrument detects, and do not let it be cited as the
other.

Where a defect has no negative class to train against, emit a score and calibrate
it against labelled lines rather than thresholding it directly. An absolute
perplexity cut rejects proper nouns, loanwords and short lines, and a
length-normalised score ranked globally is a short-line filter. Rank within
length buckets and stratify any labelling set by length.

Record the licence of every source alongside its size, and decide eligibility
before measuring quality. The app is non-commercial, so CC BY-NC sources are
usable.

Classify corpora by role and register. Keep sentence-level prose, UI strings,
named-entity data, subtitles, spoken text, and religious or historical text
separate when deciding what to train on.

### KD scale and composition

Plan for roughly 4–10 million usable KD sentence pairs per direction. Choose the
point in this range from teacher quality, token cost, language availability, and
student behavior. Record the reason when the available data or teacher makes a
different scale appropriate.

Vary the source by corpus and register. Track the share contributed by each
source, and check whether one news or wiki crawl dominates the pool. Most
teachers are strongest on news and web prose, so a large crawl pool can still
leave the student weak on short and conversational inputs. The composition of the
pool is part of the training design, not a property to inspect only after a run.

Measure the short band by REGISTER, not by count. A length histogram says whether
short text is present, not what it teaches, and the two can point opposite ways.
A pool that is 30% short can still contain almost no common nouns as labels: most
of its short lines may be named entities, which teach the student to romanize
whatever it does not recognise, or mixed-script fragments, which teach it to pass
foreign tokens through. Both are short text that installs the wrong behaviour for
a camera pair, and a student can then fail on isolated words while the histogram
says it saw millions of them. Count the short band with foreign-script tokens and
digits excluded, then break it down by source register, and check what the
examples actually are before concluding anything about coverage.

Verify an eval slice measures what its name says. A slice commissioned as
"isolated words" turned out to have multi-word references on 22% of its rows, and
because corpus chrF is character-weighted those rows carried 30% of the weight and
reversed the verdict — the candidate won the genuine one-word rows and lost the
pooled score. Check the reference side's shape, not just the source side's, and
report subsets separately when a slice mixes them.

Decompose a metric gap before trusting it. Split the rows into both-right,
both-wrong-identical, both-wrong-differing, and the exact-answer swaps. If most of
the gap comes from rows where NEITHER system is right, the metric is scoring how
wrong the wrong answers are, which may not be what you care about. One comparison
had 144% of its net gap in that class while a hand-read of usability came out
level.

Separate beam sensitivity from quantization sensitivity. Decode fp32 at the
training beam, fp32 at the deployed beam, then the packed artifact. A checkpoint
can be unusually fragile to ANY narrowing of the decoder rather than to the
quantizer specifically — one candidate lost twice as much to beam-1 as its rival
before quantization ever ran, which no amount of re-quantizing could fix.

Compare the artifact you ship, not the checkpoint you trained. Quantization loss
is not uniform across checkpoints: two finetunes of the same base lost 3.7 and 7.0
chrF++ respectively on the same band, so a fp32 ranking reversed once both were
packed. The more specialised checkpoint lost more, which is worth suspecting
whenever a candidate has been pushed hard toward a narrow register. Score the
packed int8 artifact before choosing between checkpoints, and note that the
calibration set given to the quantizer is part of that result — a set drawn from
a different distribution than deployment is a plausible cause rather than a
detail.

A chrF gain with no exact-match gain means the output looks more like the target
register without being right more often. Track both: on isolated words or labels,
a checkpoint can learn the shape of a label — casing, length, article-free
phrasing — while its accuracy is unchanged. Two separate runs showed that
signature, one from training longer and one from adding more generated data of a
kind already saturated.

Generated short-text data saturates; attested morphology does not. Once a
language's headword vocabulary is covered, adding more model-generated one-word
pairs stops paying: an increment of ~22k moved exact match by zero and made
transliteration slightly worse. An increment of the same size drawn from a
lexicon's DECLENSION TABLES moved exact match +2.4 and cut transliteration. The
mechanism is that an isolated inflected form segments into subwords the decoder
has only seen mid-word, and a paradigm supplies those forms as ground truth
rather than as a model's guess at them. Deduplicate an increment against the
OTHER component too, not only its own prior round — the two overlapped by 17%
here.

Case-fold before believing a gain on short or label-like text, and keep at least
one metric casing cannot reach. Sign and UI references are conventionally Title
Case while a model may emit lowercase, so a checkpoint that learned only
capitalisation can post a large chrF++ gain on exactly the slice you are trying to
fix. One run showed +10.26 on isolated words that became +2.11 case-folded, while
two casing-immune measures — the share of outputs closer to a transliteration of
the source than to the reference, and the share dropping or corrupting a number —
showed no improvement and one regression. Decide which of those you are measuring
before reading the table.

A filter that avoids ambiguity selects for sparse data, not for unambiguous data.
Keeping only dictionary entries with a single sense sounds like skipping the
disambiguation you cannot do; what it actually keeps is the thinly-documented
entries, because a well-covered common word carries many senses and gets dropped.
The survivors are rare words with one under-described gloss, and those glosses are
often wrong. The same shape appears whenever a quality filter keys on an
attribute that correlates with coverage. Check what a filter's survivors have in
common before trusting them, especially for isolated-word training data, where
the model has no context to override a wrong sense.

Where a lexicon does pay for a morphologically rich language is its INFLECTION
tables rather than its glosses. Headword vocabulary saturates quickly against a
mined pool; attested surface forms do not, and an isolated inflected form is
exactly what a decoder mis-segments into pieces it has only seen mid-word. Expand
a gloss you trust from elsewhere across an attested paradigm rather than
importing the lexicon's own translations. Two constraints: multiplying a gloss
across N forms multiplies any error in it by N, and case or postpositional forms
that carry meaning the target language must express need templating, not copying.

That also makes a named-entity corpus a judgement call rather than a default.
It is the standard supply of short pairs and it is the right fix for short-input
degeneracy in general, but for a camera pair it installs a romanize-on-sight
prior on exactly the inputs the app photographs most.

If OpenSubtitles is available, use a cleaned portion to add conversational and
short inputs. Subtitle data often combines several sentences and contains
formatting artifacts. Run `split_sentences.py` before sampling, alignment, or KD
decoding, and verify that its behavior matches the runtime splitter. Inspect
abbreviations, ellipses, and subtitle markers specifically.

If suitable conversational or short bitext is unavailable, generate a small,
explicitly labeled finetune set with a capable low-cost SOTA model. For example,
use `gpt-5.6-luna-low` when it is available and meets the current quality and
cost requirements. Generate from production inputs, deduplicate by source,
review the outputs, and record the model and prompt version. Keep this set
separate from human bitext and use it to address the short-input gap. It does not
replace the KD corpus scale requirement.

`gen_pairs.py` is the generator for sign, label, menu, UI and notice registers.
It is language-parametric: the grid, target-language notes and gates live in a
per-language spec under `configs/gen_pairs.<lang>.json`, and both sides of each
pair come out of one call so the register that disambiguates a short label stays
attached to it. It hash-excludes every eval file passed with `--exclude`,
deduplicates against existing sets passed with `--known`, gates script, digits
and length, and `--judge-sample` sends a fraction of the kept rows back to the
model as a faithfulness judge. Run it in rounds; each round reads what earlier
rounds wrote so duplicates fall rather than rise.

When the generating model judges cleaner than the teacher on the short registers
by a wide margin, use it as the teacher for those registers rather than only as
a finetune set: decode the dialogue, UI and label sources of the KD draw with
it and leave the bulk crawl prose to the cheaper teacher. Short lines carry few
tokens, so the cost is small. Judge every generated row rather than a sample
and drop the rows judged bad, because a block that is upweighted in training
repeats each bad row many times more than a bad crawl row. Both sides of a
generated pair are synthetic, so it is neither KD nor back-translation; it is
the substitute for human bitext in registers where no native corpus exists in
any language.

### Back-translation

KD fills the target column with teacher output, so everything the student
knows about writing the target language comes from the teacher: its calques,
its invented transliterations and its register. Back-translation is the only
step that puts native target-language text into that column. Decode
monolingual target-language text into English with the reverse-direction
teacher and add the pairs as their own training block beside the KD block. KD
pairs teach the student to understand the source; back-translated pairs teach
it to write the target. The two failure domains are disjoint.

The reverse decode only needs to be gist-faithful. Noise on the source side of
a training pair costs little as long as the target is right, so use the
reverse teacher and do not gate on its accuracy. Back-translation is gated by
monolingual supply instead: it applies whenever more native text exists than
bitext, which for any language with a Wikipedia and a web presence is by a wide
margin. It targets non-words, calques and unnatural phrasing on the target
side. It does not fix short-label sense errors, which come from understanding
the source, and the back-translated source is the noisy side.

Run the monolingual text through the same contamination and mojibake filters
as the bitext before decoding; a web crawl carries the same sibling-language
and legacy-encoding damage. Give the block a real share of training, on the
order of the generated block or larger, and taper it in the final stage of the
curriculum below.

Use these corpus roles:

- **KD source:** the source column used for teacher translation. The teacher
  creates the target, so source-side quality is the primary concern.
- **Finetune data:** aligned source and target text. Use curated or high-confidence
  human bitext, or explicitly labeled SOTA-generated data when human data is
  unavailable. Semantic misalignment directly teaches incorrect mappings.
- **Validation data:** a frozen, in-distribution golden set in the direction
  being trained, on the order of a few hundred lines, spanning every register
  the model serves and hash-excluded from every training source. Early
  stopping selects the checkpoint by this set, so a set drawn from one
  register cannot see a regression in another: a finetune toward short text
  validated on held-out TED prose gets worse by construction on that set and
  stops at an arbitrary point.
- **Evaluation data:** held-out references and production-input probes that are
  excluded from training by content hash, including from the KD draw: a slice
  carved from the same OPUS pool the KD source was drawn from measures KD
  reproduction, and a finetune that stops reproducing the teacher looks like a
  regression there. Check every eval slice against the KD training corpus, not
  only the finetune corpus, and report the clean stratum separately. Apply the exclusion to every finetune
  source, generated sets included, since a generated set is usually built from
  the same prompts that produced the check set. `exclude_eval.py` does this for
  any pair (raw-hash and normalised match on every text column against text,
  TSV, jsonl and sha256 sources, with a JSON report) and `finetune_student.sh`
  refuses a training TSV without its report. Include the early-stopping valid
  set among the sources; a valid set that overlaps training selects a memorised
  checkpoint. For an X→en holdout of short items, match on the source column or
  the pair rather than either column, since a common English target ("Boil") is
  legitimately the translation of many source words. Before scoring any
  finetuned checkpoint, count exact overlaps between each eval slice and the
  actual training TSV; a gain on an overlapping slice is memorisation until
  proven otherwise, and a drop after cleaning is the memorisation leaving.

Prepare the KD source and its original reference column together. Use
`build_kd_source.sh` rather than independently sorting and sampling the two
columns.

Bicleaner measures source-reference alignment. Use its score to gate
reference-based n-best selection and to salvage high-confidence human bitext for
finetuning or the backward model. It does not replace source-side cleaning for
the KD pool, because the teacher regenerates KD targets.

## 4. Check contamination

Check the following classes before accepting a corpus:

1. Sibling languages that share the script.
2. Encoding or legacy-font corruption that remains inside the expected Unicode
   block.
3. Archaic, liturgical, or otherwise unsuitable registers.
4. Register imbalance introduced by filtering.

Use script and character checks, language identification, frequency analysis,
and character or token models as appropriate. A keep-only rule is unsafe when
absence of a marker does not prove language membership. Prefer drop-only rules
for ambiguous sibling-language signals.

Validate every filter on known-clean data. Record its false-positive rate and the
retained and removed counts by corpus and register. If filtering removes the only
conversational or short-input source, decide whether the loss is acceptable before
training.

Keep a corrupt target row as a KD source when the source is usable and the teacher
will regenerate the target. Do not use that row as human finetune data.

Measure corpus quality by usability, and read before trusting a rate. A judge
that counts any detectable imperfection reports several times the damage of one
that asks whether a translator could render the line, so decide `keep`
separately from the defect type. Protocol: `judge_mono.py` over uniform samples
of a few hundred lines per source, the source field only; a hand read of a few
dozen source, teacher-output and reference triples per direction; and a
per-register breakdown, since entity and UI rows behave differently from crawl.
Judge the reference column as well: mined bitext can carry a large share of
unrelated references, which disqualifies reference-based selection even where
the source is fine. Orthographic features find spam and shattered text and are
blind to fluent machine-translated source, which is the class that makes a
teacher fabricate; reference-free quality estimation over the decoded pairs is
the instrument for that class. Keep the judged samples, they are the validation
set for every cheap filter built afterwards.

## 5. Gate the teacher

Run a loading smoke test before interpreting a poor score. Reproduce the model
card example, pin the model and library versions, and confirm the tokenizer,
language codes, direction, and output format.

Evaluate every candidate in both directions when both directions are intended for
production use. Use FLORES for a common reference point, then use the production-input
check set, held-out register slices, and the probe set. A teacher that passes a
news benchmark can still fail on short signs, entities, numbers, or negations.

Choose a teacher per direction using the complete evidence. Do not apply one
absolute chrF threshold to languages whose surface metrics are known to differ.
Record the teacher, model version, decoding settings, evaluation inputs, scores,
and review decision.

If references were generated by a model family used in training, check for style
agreement. A gain on generated references must also appear on human-referenced
slices and in manual review.

## 6. Prepare the training artifacts

Run the data preparation script on a local CPU-only host when available. Use the
project's uv-managed Python environment:

```text
prep_data.py --lang <lang> --workdir <workdir> --jobs <n>
```

The first run trains the joint vocabulary. Use `--skip-spm` only when reusing a
verified vocabulary for a separate data role. The vocabulary passed to Marian
must have the `.spm` extension.

Create, verify, and retain:

- cleaned pair data
- the joint vocabulary
- the KD source and matching reference column
- the held-out validation file
- the source-only devtest file used by quantization
- content hashes for excluded evaluation rows
- any bicleaner scores or contamination decisions used later

Run the vocabulary checks before renting a training box. A vocabulary failure is
cheaper to correct before alignment and KD artifacts depend on it.

## 7. Decode and select KD data

Shard the KD source before teacher decoding. Keep shard order, source rows,
references, scores, and output rows together.

Select a teacher with a supported gate and decoding path. The pipeline has used
NLLB, Hy-MT2, and OPUS-MT teachers. NLLB and OPUS-MT use `distill_data.py`; a
causal language model such as Hy-MT2 uses the corresponding vLLM decoding path.

For an NLLB or OPUS-MT teacher, use `distill_data.py` with the corresponding
teacher language codes:

```text
distill_data.py --model <model> --src <shard> --out <targets> \
  --src-lang <src_code> --tgt-lang <tgt_code> \
  --beam <beam> --nbest <n>
```

Use n-best selection only when the source-reference pair is trustworthy. Gate
reference-based selection with a pair-quality score. Use rank one when the
reference is below the selection gate. Keep the reference-selection threshold
separate from the stricter threshold used to salvage human finetune data.

Score every teacher output with the backward model. The backward model is the
shallow RNN from `backward.s2s.yml`, trained in the reverse direction on human
or high-confidence aligned bitext only, so it is independent of the teacher
(`train_backward.sh`, then `cefilter_score.sh` and `cefilter_cut.sh`). For each
pair it reports the per-token cross-entropy of the original source given the
teacher's target. A target that carries the meaning makes the source
predictable and scores low; a dropped clause, a fabricated entity, a copied
source or an inverted sense makes the source surprising and scores high. The
reverse direction is the point: any forward score, the teacher's own
confidence included, rewards fluent fabrication, and this is the class that
orthographic filters cannot see. Rank within length buckets, since per-token
cross-entropy favours short lines, drop only the configured worst fraction,
preserve the row order, and record the filter coverage. It is a ranking, not a
judge: it separates gross failures reliably and nuance poorly, and a model
trained on thin bitext still separates the gross failures.

The same model selects between teachers. Where the short registers are decoded
by more than one teacher, keep the candidate with the lowest backward score
per line. This lifts label quality above any single teacher without changing
the teacher.

Do not assume that a moved n-best hypothesis improved the translation. Compare
the resulting student against the rank-one control using all evaluation slices
and the probes.

## 8. Align and train the student

Remove empty or zero-token pairs before alignment. Run both alignment directions,
symmetrize them, and fail the step when either direction contains an unexpected
number of empty alignments.

Create the guided-alignment TSV with:

```text
align.sh <source> <target> <out_dir> <tools_dir>
```

Run the CPU-only stages on a local host when available before renting a GPU. Use
Docker for binaries that require an isolated system environment. The production
student uses the base-memory configuration unless a documented experiment selects
another size.

Train with `train_student.sh` and a two-column validation file. Keep the best
checkpoint and stop according to validation behavior. Use the pair-specific
OpusTrainer configuration when script casing, punctuation, or normalization
requires it.

Use guided alignment for the production student. The OpusTrainer stream supplies
the augmented three-column corpus to Marian, and the model's alignment output is
used for format and bold transfer. Verify that the training configuration,
vocabulary, and alignment mode are the intended pair-specific versions.

Train once, with a curriculum, rather than a KD-only run followed by a
finetune. OpusTrainer reads several named corpora and fills every batch from
them in the proportions a stage gives; a stage ends after a stated number of
passes over one named corpus and the next stage changes the mix. Marian sees
one stream whose composition shifts. Give the KD block, the back-translated
block, the human bitext and the generated block their own datasets, keep the
curated blocks present from the first update, and raise their share in the
last stage. The proportions are per batch, so a small corpus with a large
share cycles more often; that is how the curated data is upweighted rather
than diluted. A block that is only a few percent of the rows on disk can be a
third of every end-stage batch.

This replaces train-then-finetune because a finetune on a few hundred
thousand rows after millions of KD rows moves the whole model toward those
rows, regardless of their composition, and every subsequent addition to the
finetune reshuffles a ratio nobody is steering. One pair went through five
finetune iterations that way, each fixing its symptom and regressing a
register the previous one had fixed, and a small set repeated for dozens of
epochs memorised its rows before it generalised. In the curriculum there is
nothing to overwrite later. This is the structure the browsermt pipeline
uses; its first measurement in this pipeline is the next retrain of that
pair, so compare against the train-then-finetune pack on the clean strata
and record the result here.

The stage that remains after training is a short quantization-aware
continuation with `quantize-bits: 8` on the end-stage mix, which is what makes
the int8 pack lossless. Any further finetune is an explicit experiment: record
the model and prompt version of generated inputs, evaluate every slice, and
retain the base checkpoint as the comparison.

Run the multi-GPU efficiency check before committing to a multi-GPU training job.
The measured throughput must be evaluated on a representative corpus and include
the effective batch and learning-rate settings.

## 9. Quantize, evaluate, and package

Quantize the selected checkpoint with the browsermt conversion path:

```text
quantize_export.sh <model.npz> <vocab.spm> <devtest.src> <out_dir>
```

Build the shortlist from SentencePiece subwords with:

```text
shortlist.sh <source> <target> <vocab.spm> <out_dir> <tools_dir>
```

The shortlist restricts the output projection to the pieces fast_align saw
aligned in the corpus it was built from, plus a fixed number of frequent pieces.
Build it over every corpus the student trained on, not the KD corpus alone. A
table built from a KD sample cannot emit vocabulary that only the finetune stage
taught, and the loss lands on the short registers as non-words, misspellings and
dropped digits. Concatenate the KD sample with the finetune corpus repeated
enough times to be a large share of the alignment input, and segment the target
side the way the model was trained: a student trained under SentencePiece
sampling emits alternative segmentations of the same word, and a table built
from one deterministic segmentation cannot reach them. Encode the target side
once deterministically and once with `--output_format sample_piece` at the
training alpha, aligned to the same deterministic source. The runtime admits
every digit piece whenever the source carries one, so numbers do not depend on
the table; other pieces do.

Benchmark the quantized model without a shortlist first. Then benchmark the
shortlisted package on the same slices and read the short-input outputs side by
side. A shortlist loss looks like a quantization loss until the pack is decoded
with `none`, so measure both before attributing a gap to the quantizer. Judge the
table on the rows where the two decodes differ: count exact hits each side wins
and read them. A table whose exclusive losses are pieces missing from words the
model knows is still starving the decoder; rebuild it. A residue of rare
vocabulary both decodes miss is a model problem the table cannot fix. A package
that still loses to `none` on the short slices after a correct rebuild is a
speed-for-quality call to make explicitly, not a default. Do not expect the
low-confidence re-decode to rescue a starved shortlist: a decoder missing the
piece it wants usually picks a wrong piece confidently, so the deficit router
does not fire on most of those rows. Compare
the model and package against the teacher, the previous package, held-out
slices, and probes.

Package only after the artifact passes the release checks. Use a new package
directory for each model update and verify hashes, sizes, vocabulary, shortlist,
and index references before publishing. `gate_pack.sh` writes the `PACK_OK`
marker only when the scored pack passes the selection rule, and
`publish_pack.sh <infix> <pack_dir> <dated_label> --confirm` refuses a pack
without it, refuses an existing label, re-measures size and hash after transfer,
regenerates the catalog and index, syncs, fetches every live URL and re-hashes
the download, logging each step to `data/DEPLOY_STATE.md`. Publish one direction
at a time; a direction that fails the rule stays on its previous label.

## Completion checklist

A language pair is ready for release when:

- the language profile and pair-specific configuration are recorded
- corpus roles, contamination checks, and held-out hashes are recorded
- the KD corpus has an appropriate scale and a measured source/register mix
- the back-translated, human and generated blocks are recorded with their
  stage weights, and every teacher output passed the backward-model filter
- short-input behavior has been evaluated for the teacher and student
- the vocabulary passes script, round-trip, unknown-token, and fertility checks
- each teacher passes the load, reference, production-input, and probe checks
- KD, alignment, filtering, and training artifacts are reproducible and verified
- the quantized model and shortlist pass independent package tests, and the
  shortlisted pack matches the no-shortlist decode on the short slices
- every eval slice has zero exact overlap with the training TSVs
- all evaluation slices and probes have been reviewed
- the release decision and remaining limitations are recorded
