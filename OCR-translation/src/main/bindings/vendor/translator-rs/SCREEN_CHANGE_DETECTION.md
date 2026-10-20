# Live-screen change detection — per-box monitoring (Monitoring spec)

Status: **design / future**. This is the "Monitoring" state of the screen-translate
pipeline — what runs *after* an acquire while the overlay is shown. It decides, per
text box, when the text underneath changed, when new text appeared, and when the
whole view scrolled. It pairs with `LIVE_SCREEN_FUTURE_WORKER_SPLIT.md` (the async
worker + event loop this lives in).

The target behaviour is Google-Lens-style, generalized to video and games: keep
translations up over a live source, update each box in place when its text changes,
add boxes when new text appears, and only tear the overlay down when the whole view
actually scrolls. The overlay is **additive and per-box**, not a single thing shown
and hidden as a unit.

## Relationship to the current code (v1)

What exists today in `src/live_screen.rs` is a single global movement/settle signal:

- one coarse mean-abs-diff `G` over the whole 128-long-side gray per frame
  (`mean_abs_diff`), threshold `MOVE_THRESHOLD = 8.0`, settle quiet `120ms`;
- a three-state machine (`Settling` / `Acquiring` / `Idle`);
- a pill mask that excludes our own overlay cells from the diff (`build_pill_mask`);
- acquire is whole-screen and destructive — `run_screen_acquire` calls
  `reset_anchor_state` and re-detects over the full frame.

This handles "scroll a page → hide → stop → re-translate." It fails the two cases
this spec targets:

- **Subtitles on a still or playing video.** A changing subtitle is a tiny fraction
  of the coarse grid, so a global `G` washes it out and never re-acquires (stale
  translation); a playing video keeps `G` high forever and never settles (nothing
  ever shows).
- **Background motion, unchanged text.** A moving element crosses the global
  threshold and `Hide` drops the *entire* overlay, including translations whose
  underlying text never changed.

Both have the same root cause: a single global scalar cannot separate "the text I
care about changed" from "background pixels moved." This spec replaces the global
signal with per-box signals and an additive overlay.

## Guiding principles

- **Measure the text, not the pixels.** Every per-box signal is defined relative to
  the text strokes under the box, not raw frame deltas.
- **The capture includes our overlay.** MediaProjection and accessibility+screenshot
  both mirror the display, so our own opaque pills are in every captured frame. We do
  not assume access to a clean, overlay-free capture. (A platform that *did* hand us
  the pre-overlay frame would simplify this, but we do not design around one.)
- **Additive overlay.** A box persists until its own text is gone. Background motion
  never removes a box; only a real scroll tears the overlay down.
- **Cadence is a free motion signal.** The mirror emits frames only on change, so
  frames arriving = something moved, no frames = settled. Everything else is cheap
  math on frames we already receive.

## The probe: a global pinhole lattice

We sample the true screen content at a fixed, frame-space **lattice** of points (the
playtranslate spacing, ~3px, alternating rows). The lattice is global and per-frame —
a box does not own a lattice; it selects the subset of lattice points that fall
inside its region. The punched holes are exactly `global lattice ∩ pill coverage`:
where no pill covers, the lattice point is an ordinary pixel read; where a pill
covers, we punch a hole so the point is not fully occluded.

The holes are **50%-alpha** (PorterDuff `DST_OUT` on the Android canvas, or a stipple
in the GPU pill shader), not fully transparent — a transparent hole would be a
visible pinprick of the original, untranslated text poking through the pill. Through
a 50% hole the captured pixel is a blend, which we invert using the overlay we drew:

```
raw ≈ ½·screen + ½·overlay         (overlay is known — we rendered it)
screen_est = 2·raw − overlay       (recovered screen content at that point)
```

**There is no stored clean reference frame.** The old `cleanRef` (an overlay-free
capture taken during a clear → capture → show cycle) does not exist in an always-on
overlay. `screen_est` already divides the overlay out, so the change baseline is just
a recent snapshot of `screen_est`, rolled forward continuously from the same stream.

## The text mask: classify by behaviour, not appearance

The per-box change test compares only lattice points that sit on the box's text.
Deciding which points those are is the crux for video backgrounds, and it is built in
three layers, each bootstrapping the next.

1. **Contour (scope).** Detection already returns a `contour` polygon hugging the
   text run (no ascenders/descenders). It is the right per-box region — far tighter
   than the bounding rect — for assigning lattice points and bounding everything
   below. But it is a *filled area*: it includes inter-word spaces and the counters of
   letters like `o`, where a no-bar subtitle shows the moving video. So the contour
   alone is too generous for the change test.

2. **Binarize within the contour (bootstrap).** At each clean read we binarize the
   crop inside the contour to a per-pixel ink mask. The contour makes this reliable
   (local threshold, no high-contrast video elsewhere misread as ink). This gives a
   usable mask on the first frame. But binarization classifies by **appearance**, and
   appearance is fooled: white-fill + black-shadow text is bimodal at *both* luma
   extremes with the video in the middle, so no single threshold isolates the glyph,
   and a bright or dark patch of video inside the contour gets misread as ink and then
   *moves*.

3. **Temporal per-hole variance (the real mask).** What actually defines "the text
   under this pill" is **opaque coverage**: a glyph pixel (white fill, black shadow,
   outline, bar — any of them) stays put while the video behind it moves, because the
   glyph occludes the video there. That is a temporal property and it is
   polarity-agnostic. Over the first few frames after a clean read, accumulate the
   per-hole variance of `screen_est`; low-variance holes are glyph, high-variance
   holes track the video and are excluded. We get it for free from the always-on
   pinhole stream (no extra reads), bootstrapped by the binarize mask while it firms
   up over those frames.

Order of strength: contour scopes it, binarize bootstraps it, **temporal variance is
the mask**. Appearance can be mimicked by the video; behaviour cannot.

Residual failure modes, handled by coherence rather than a perfect mask:

- **Anti-aliased stroke edges** blend glyph and video sub-pixel. Erode the mask to
  stroke interiors and require a deviation magnitude above edge noise.
- **Glyph color matching the local background** (black shadow over a night scene,
  white fill over sky) makes that hole ambiguous. It is local and momentary; require
  **spatial coherence** — a majority of a box's on-glyph holes deviating, not one — so
  a few ambiguous holes never trip a box on their own.

There is no "blink the whole ROI every few seconds" backstop. Under-pill correctness
rests on the pinhole + temporal mask, which is what buys the no-flicker property
below.

## Per-frame signal and classification

Each frame, for every lattice point: recover `screen_est`, update its temporal
variance, and (on stable on-glyph holes) diff against the rolling baseline. Reduce to
a per-box **changed-stroke-hole count**. The spatial structure of the deviation field
across the whole lattice classifies the motion:

- **Globally coherent translation** (the whole field shifts by a consistent vector) →
  **scroll** → hide + full re-acquire (the v1 behaviour, correct here). The shift
  vector can be recovered by phase-correlating the lattice samples if we ever want to
  reposition instead of hide.
- **Deviation concentrated in one box's on-glyph holes** → that box's text changed →
  targeted re-OCR (mechanism A).
- **Diffuse, off-glyph, no coherent shift** → animated background → ignore.

This is why scroll detection survives a text-heavy page even though our pills cover
most of it: the pinholes see *through* the pills, so a scroll moves the recovered
content under every pill at once — a dense, page-wide signal exactly where the old
pill mask had blanked everything.

## The two monitoring mechanisms

The whole interface never blinks. The only thing that ever flickers is the specific
set of pills the pinhole flagged this frame.

### A. Under-pill change → batched targeted re-OCR (event-driven)

When the per-box scores trip for some set of pills **in a frame**, that set is the
batch: remove just those pills, take **one** snapshot, re-OCR them all together,
re-show, and refresh their temporal baseline from that clean read. Batching is "all
pills that tripped this frame," driven by the signal rather than a clock; a blink
rate cap (e.g. one blink per ~150–200ms) bounds flicker if many boxes churn. A
subtitle advancing to the next line is this case: the new line lands under the
existing pill → the box trips → that one pill re-OCRs.

### B. New text → masked periodic full-screen detect (no blink)

Every ~0.5s, run detection on the captured frame **with all overlays still up**, and
mask out the existing pill regions (drop detections overlapping current pills — we
already own those). What remains is text in the uncovered gaps, which is genuinely
visible in the capture, so it is OCR'd directly with **no blink**. A new region
becomes a new pill. The existing overlay is untouched.

The split is deliberate: **the pinhole owns under-pill correctness; the masked
periodic detect owns new-text discovery.** Because the periodic detect masks existing
pills, it never re-checks what is under a pill — that is the pinhole's job, and the
temporal mask is what makes the pinhole reliable enough to carry it.

## Regrouping and reconciliation

One-shot acquire grouped all detected lines at once (`group_lines_into_paragraphs`,
`ocr.rs:781`), so paragraph grouping was free. Incremental discovery breaks that:
grouping is a property of the line *set*, so any line-set change in a region makes the
local grouping stale. A second subtitle line appearing must join the first and
translate as one paragraph, not two independent lines.

Whenever lines are added/removed/changed in a region (from A or B):

- Assemble the affected **cluster** = the changed/new lines ∪ every existing block
  within the merge algorithm's own join tolerance (height / vertical-gap / column —
  the same criteria `group_lines_into_paragraphs` already encodes). An isolated new
  line is its own block and nothing else moves.
- Re-run `group_lines_into_paragraphs` over just that cluster. Scope is local, never
  the whole screen, so distant blocks do not churn.
- **Reconcile by `content_hash`** (already on `BlockSpec`): a regrouped block whose
  joined source text is unchanged keeps its translation; a new or changed group is
  re-translated. A regroup that turns one line into a two-line paragraph changes the
  translation unit, so it re-translates as a unit even though neither line's own text
  changed.

Mechanism B already has all current lines, so its periodic pass can re-run the full
grouping and reconcile globally for free; mechanism A regroups only the local cluster
it re-read.

**Data-model requirement.** Each resident block must retain its constituent
`TextLine`s (tight_box + recognized source text), not just the merged `display_text`,
so a neighbor's arrival can decompose the block back into lines and regroup without
re-OCR. Today the one-shot path discards the pre-merge lines; the incremental model
keeps them on the block. This is a `BlockSpec` change, not just orchestration.

## GPU implementation (GLES 3.1)

The captured frame already arrives as a GL texture (the compositor samples it), and
we drew the overlay, so the pinhole extraction belongs on the GPU and reads back only
a tiny per-box result — the same philosophy as the existing det-gray readback.

GLES 3.1 (compute shaders, SSBOs, atomics) makes the reduction clean:

1. One compute dispatch over the lattice points.
2. Each invocation samples the captured texture, computes `screen_est = 2·raw −
   overlay`, checks the per-box stroke-mask texture, and on a coherent deviation does
   `atomicAdd(scores[boxIndex], 1)` into an SSBO.
3. Read back the SSBO — one integer per box, not the frame.

Supporting GPU state:

- a **baseline `screen_est` texture** refreshed on each clean read and rolled forward
  between them (the GPU-resident replacement for the old CPU `cleanRef` buffer);
- a per-box **stroke-mask texture** (contour → binarize → eroded, then refined by
  temporal variance) plus a per-hole variance accumulator;
- the holes are punched in the composite pass, so the recovered values can be emitted
  alongside compositing rather than in a separate read of the frame.

The CPU runs the cheap classifier (scroll / box-local / diffuse) on the small
readback. GLES 3.1 is a device capability, not guaranteed by minSdk 23 alone, but the
screen path already does GPU work on the MediaProjection texture, so the target has
it.

## Cost

- **Per frame:** one compute dispatch over the lattice + a per-box-score readback (a
  handful of ints). Self-bounded by the on-change frame cadence — a fully static
  screen emits no frames and does no work.
- **Per clean read (mechanism A):** detect + rec on the blinked boxes only.
- **Per ~0.5s (mechanism B):** one full-screen detect, masked to the gaps, plus rec
  on genuinely new regions.
- No full-frame CPU readback anywhere in the steady state.

## Test ladder

The change logic is pure functions over lattice samples and per-box state; the OCR
runs on a CPU-built `LiveFrame`. Both are testable without a device.

- **Tier 0 — pure classifier / monitor (no models, no GPU).** Synthetic lattice +
  per-hole-variance fixtures asserting the action stream. "Subtitle text changes
  while a colored square moves" and "background moves, text still" live here; both
  reduce to lattice-sample sequences.
- **Tier 1 — real OCR on CPU frames.** Synthesize RGBA frames (still background +
  subtitle band whose text changes every M frames; an independently moving square),
  build a `LiveFrame` via `reset_owned`, and run the real `acquire_detect` /
  `acquire_rec_translate` (the CPU `ensure_oriented_with_rgb` path — no GPU). Assert
  the subtitle re-OCRs to the new string and the moving square never drops or
  re-translates the subtitle pill, and that a second line regroups + re-translates as
  one paragraph. Gated on the `ppocr` feature + model files, like
  `tests/live_overlay_sign.rs`.
- **Tier 2 — headless GPU composite + pinhole.** The surfaceless GLES context already
  stood up in `tests/gpu_compositor.rs` (EGL + llvmpipe): composite our overlay (with
  holes) onto a synthetic screen the way MediaProjection would capture it, feed that
  back as the captured frame, and assert the reduction recovers the under-pill content
  through the holes and the per-box scores trip correctly. This is how the
  Android-style "capture includes our overlay" condition is reproduced off-device.

For determinism, drive the crate-internal `acquire_detect` / `acquire_rec_translate`
directly (what the worker calls) or add a test-only synchronous `acquire_blocking`,
rather than the async `SlotWorker` dispatch.

## Phasing

- **v1 (shipped) — global movement + settle**, full destructive re-acquire. Handles
  page scroll; fails subtitles/video. Lives in `src/live_screen.rs`.
- **v2 — per-box pinhole + temporal mask + the two mechanisms + regroup.** The body
  of this spec: global lattice, `screen_est`, temporal stroke mask, batched targeted
  re-OCR, masked periodic detect, scoped regroup with content-hash reconciliation, GPU
  compute reduction. Build the pure classifier + Tier-0/1 tests first, then the GPU
  reduction + Tier-2.

All of it lives in the **Monitoring** state of the worker-split pipeline
(`LIVE_SCREEN_FUTURE_WORKER_SPLIT.md`), fed each `onFrameAvailable` by the captured
frame texture + the rendered overlay + the resident box state.
