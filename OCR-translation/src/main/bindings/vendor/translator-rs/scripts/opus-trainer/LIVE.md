# Live packs

What is actually served from the bucket per pair, and which experiments and
fixes are inside it. Update this when `publish_pack.sh` runs; the line-by-line
publish record is `data/DEPLOY_STATE.md`.

## ka-en — `lmt60_distill_20260902` (published 2026-09-02)

- Model: the arm C int8 checkpoint that shipped on 2026-09-01, unchanged.
  sha256 `24540168630f034b2e246474368a308d16deef44c3d55ffb1e6885f6110f6644`.
  KD student (LMT-60-8B teacher, 4M pairs) finetuned on 127,628 rows
  (`ft3/armC.train.tsv`: 113k human/generated short pairs + 8.4k one-word +
  3.1k paradigm rows), quantize-aware. See `ka_findings.md` §20, §23.
- Shortlist: rebuild-3 recipe (`ka_findings.md` §28): KD 1-in-4 sample + the
  arm C finetune corpus ×10, target side encoded deterministic + sampled
  (alpha 0.5), `marian-conv 200 100 0`. Built at bigserver
  `/nvme2/prom/enka2/sl4.kaen/out/lex.200.100.s2t.bin`, sha256 prefix
  `3cec6f0737c92a03`. Replaces the KD-only table that caused the
  "extingerier" / "Error 44" class of errors.
- Catalog metrics (int8, with table, FLORES devtest): chrF++ 48.61, COMET22 0.826.
- Not in this pack: the ft4/ft5/ft6 finetunes (luna tranche + rehearsal; §29,
  §30, and the ft6 note below). Arm D (paradigm tier, §24) also beats arm C on isolated words with
  its own rebuild-3 table but part of that lead is memorisation (§29); not
  shipped.
- Why ft6 was not shipped (2026-09-02). ft6 is the corpus-only iteration §31
  asked for: the luna tranche's English rewritten to mirror the Georgian row's
  figures and currency (4,328 rows), its title-cased UI labels sentence-cased,
  and the rehearsal's long numeric band tripled inside the same 1:1 budget. It
  did what it was aimed at and still lost. Against live it gains flores +0.36
  (COMET22 82.60 -> 82.73), ted +0.74, crawl +0.86, probes +6.05 and holds the
  one-word band (44.3% exact against 44.2%), and it repairs most of ft5's ui
  loss (source-only stratum 63.69 -> 64.95, blind A/B net -14 -> -3 rows). It
  loses on the two clauses that decide: `number_fidelity.py` against the
  Georgian source counts 7 lines with a lost or changed figure on `numbers`
  against live's 5 and 11 against live's 3 on `numbers_ho` (new corruptions
  2387 -> 237, 1201000 -> 120,000, postal code 7002 -> 702, ISO 10012 -> 101012),
  and the blind source-only A/B is net -18 rows of 500 on subtitles. The
  conventions themselves did move to the source's side (₾ and decimal commas
  are back at the reference's counts, GEL down 21 -> 14 and 22 -> 8), so the
  corpus rewrite worked and the figure loss is not what the conventions were
  hiding. `pack.kaen.ft6` on bigserver has no PACK_OK; the extended
  `gate_pack.sh` refused it.
- Previous label `lmt60_distill_20260901` (arm C + KD-only table) stays addressable.

## en-ka — `lmt60_distill_20260902` (published 2026-09-02)

- Model: ft5 checkpoint (`ka_findings.md` §30), sha256
  `201049a418490e5bfb5bf23ef61faee125a2eed33380540745b4ae6568e25a43`.
  KD student (LMT-60-4B teacher, 4M pairs, §11, §15) finetuned on the ft2 set
  (123k luna/sonnet-translated short + long pairs) + the full 132,969-row luna
  sign/label/menu/UI/notice tranche from `gen_pairs.py` + a 1:1 KD rehearsal
  sample, after `exclude_eval.py --eval-pair` removed 2,709 eval-overlapping
  rows (including the 42 check lines that had leaked into ft2 and the rows
  overlapping the valid set). Quantize-aware, OpusTrainer case modifiers off.
- Shortlist: rebuild-3 recipe over KD sample + the ft5 finetune corpus ×10
  (`/nvme2/prom/enka2/sl6.enka`).
- Catalog metrics (int8, with table, FLORES devtest): chrF++ 47.36, COMET22 0.8389.
  67 referenced signs: 68.55 chrF++ (was 60.86 on the 20260831 pack).
- Previous label `lmt60_distill_20260831` (FT v2 + KD-only table, §15) stays addressable.

## Runtime requirements

- slimt digit rule (slimt `2beb403`, slimt-sys `1cef6a6`, translator-rs Cargo.lock
  bumped 2026-09-02): every digit-only target piece is admitted to the
  shortlist candidates whenever the source carries a digit piece. Without it,
  the ka-en table above still drops digits on inputs like `შეცდომა 404`.
