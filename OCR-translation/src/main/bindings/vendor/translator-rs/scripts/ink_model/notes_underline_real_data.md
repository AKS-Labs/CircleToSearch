# Ink model: underline channel + real-data training — session notes (2026-06-25)

Working notes (untracked). Goal: (1) fix "underline not erased", (2) improve the matte.

## Results (held-out OTR pack2, raw-matte IoU, no dilation/closing, golden tau≈0)

| model | matte IoU | recall | prec | notes |
|---|---|---|---|---|
| prod (base16, shipped 2-ch) | 0.577 | 0.637 | 0.856 | baseline |
| rule-v1 (base16 + rule) | 0.570 | 0.591 | 0.923 | rule added, no matte regression |
| rule-otr-v1 (+ OTR backgrounds) | 0.570 | 0.591 | 0.923 | precision↑ but recall↓ (over-conservative) |
| rule-otr-tv (+ matte-tversky 1.0) | 0.641 | 0.669 | 0.915 | recall-bias recovers recall; beats prod |
| **rule-otr-real (+ --real-frac 0.3)** | **0.816** | **0.914** | 0.859 | **real OTR strips = the big lever** |
| rule-otr-b8 (base8 variant) | 0.542 | 0.571 | 0.893 | −0.06 capacity cost; erase IoU still ≈ base16 |

## What worked
- **Rule channel (under/strike/over) = ONE channel.** ch2 fires on rule pixels; the *type* is
  read at runtime from vertical position vs baseline/x-height. Converged well; detects underlines
  crisply. Drawn into both the matte (ch0, so it erases) and the rule label (ch2).
- **Erase = matte ∪ rule.** The matte (ch0) only weakly learns the underline at RULE_FRAC=0.22;
  the dedicated ch2 head learns it strongly → union is what makes rules erase. (`erase_full.erase_ink`)
- **matte-tversky (recall-biased FN≫FP)** fixed the over-conservatism OTR backgrounds induced.
- **REAL OTR strips were the decisive win** (recall 0.669→0.914). Generalizes OOD: badink screenshot
  + menu.jpg (real camera) both matte cleanly. Real text+bg+correct-label >> synth-text-on-real-bg.
- **det ∩ ink filter** (drop boxes below ink-floor) keeps det false-positives out of training data.
- **Self-describing model:** output channel count (1/2/3) = matte / +bold / +rule. Graceful downgrade.

## What didn't / caveats
- **OTR backgrounds alone** (synth text on OTR plates, no tversky): over-corrected → recall crash on
  busy/low-contrast bg (1019-class), matte IoU flat. Needed recall protection.
- **base8**: real matte −0.06 IoU (capacity). BUT dilated erase IoU ≈ base16 — fine for erase, worse
  for matte-derived typography (x-height/baseline/bold).
- **Metric pitfalls (cost real time, several wrong diagnoses corrected by *looking*):**
  - erase precision is **halo-capped ~0.5** by the ×1.8 + margin dilation → measures the halo, not
    the model. Use **raw-matte ch0>0.5** as the real metric.
  - golden must be **binary diff>~0 (tau≈0)**; the soft |img−gt| magnitude renders hollow/misleading.
  - thin-stroke **boundary harshness**: 1px tolerance +0.11, matched-threshold +0.08 — ~0.6 aggregate
    is mostly this, not model badness. But thick-stroke busy-bg (1019) failures are REAL (tolerance
    doesn't rescue). STR/matting literature uses PSNR/SSIM + leftover-detect or soft SAD, never IoU.
  - projected-matte **coordmap-scatter speckle** deflates ~0.02 (minor).
- **0.816 is largely IN-DISTRIBUTION** (trained OTR, eval OTR diff split). OOD (badink/menu) says it
  generalizes, but need a proper non-OTR OOD eval (PDF/scene/screenshot) before full confidence.
- **HierText has NO ink masks** (word/line/para polygons only) — that's why the prior HierText ink
  model was bad (region≠strokes). Path: pseudo-label within polygons with rule-otr-real + use
  outside-polygon as real negatives (esp. for non-Latin, which OTR lacks).
- Each vast run **hung in dataloader teardown after `done`** (held GPU until killed; kill the pgid).
- Throughput trap: `train.py --reuse` defaulted to 1 → GPU starved (~500 vs ~1100 strips/s). Now
  reuse defaults 16 + run-defining args are REQUIRED (no silent defaults).

## Pipeline / artifacts built
- `gen_data.py`: rule rendering (RULE_FRAC) + OTR real-background plates (INK_OTR_PLATES/FRAC).
- `model.py`: optional `rule`/`rule_head` (off by default → 2-ch loads strict).
- `train.py`: rule loss; `--real-dir`/`--real-frac` mix (real = matte-only, **bold+rule loss masked**
  via `real_mask`); required run-defining args; `--pin-memory`; reuse default 16.
- `src/bin/extract_strips.rs`: threaded batch det+dewarp (`--features viz`), builds on laptop, runs
  on the box. `build_otr_real.py`: storage-bounded shard loop → 28,044 real `(strip,matte)` pairs
  (`data/otr_real/`, also on the box). `eval_otr_erase.py`: held-out matte-IoU harness.
- Checkpoints: `ckpt/ink-rule-otr-real.pt` is the ship candidate (also -tv, -v1, -b8, -otr-v1).

## Next
1. **Rust runtime wiring — DONE (erase side):** `InkStrip.rule` from ch2 (`chans>=3`);
   `InkStrip.erase_mask()` = matte∪rule; `ocr_runtime` uses a separate `erase_masks` for the union
   (keeps `ink_masks` pure matte for metrics); `pooled_bold` excludes rule pixels. 2-ch model →
   `rule=None` → identical to today (graceful, self-describing via output channel count). Builds.
   STILL TODO: per-word rule TYPE (under/strike/over by vertical position) for *re-rendering* the
   underline in the translation (render/overlay side); `bold_profile` rule-exclusion (live path,
   gated off — needs a `text_metrics` signature change).
2. **DONE:** `convert_ink_mnn.py` made rule-aware; `ink-rule-otr-real.pt` → `ink_bold_line_int8.mnn`
   (3-ch, int8 weight-quant, 1.98MB) staged in bucket `PP-OCRv6/` next to `ink_bold_int8.mnn`.
   Validated: onnx out=3ch, Rust loader runs it. NOT active yet — point the INK role
   (`catalog_ppocr.py`/pack index) at it to activate; app build must include the ch2 wiring to
   erase underlines (old app + this model = matte+bold only, graceful).
3. **Non-OTR OOD eval** (menu/PDF/scene) — decides shippability honestly.
4. **HierText pseudo-label** pipeline (non-Latin + real diversity) — the next big app-quality lever.
5. Full 74k OTR extraction (reusable dataset); derive bold-from-strokes for real samples.
