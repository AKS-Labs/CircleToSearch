# Pipeline cleanup plan — bandaid inventory

Catalogue of "fix the output because the input was noisy" patches in
the planar / OCR pipeline, with a recommended path for each. The
trigger for writing this down was landing the EKF on `H_anchor→view`
(see `analysis.md` § "EKF on H") and realising the per-element P-EMA
and the older `live_tracker_pipeline.rs::smooth_homography` are now
two redundant smoothers stacked downstream of a principled one.

For each entry: **what** the patch is, **root cause** it masks,
**status** (band-aid vs principled), and **action**.

---

## 1. Three temporal smoothers stacked in series on H

**What.** The per-frame `H` flows through three temporal filters
before composite:

```
RANSAC fit
   ↓
EKF on H_anchor→view              (planar_engine.rs::apply_h_ekf, new)
   ↓
chain compose → H_root→view       (chain_homography)
   ↓
per-element P-EMA                 (planar_engine.rs::smooth_emit_h)
   ↓
smooth_homography                 (live_tracker_pipeline.rs:1307)
   ↓
composite
```

**Root cause masked.** Per-frame RANSAC noise on `H` (and
chain-composition noise on `H_root→view`).

**Status.** The EKF is principled (per-DoF noise budget set by
measurement geometry). The two downstream smoothers are scalar
low-passes on a 9-vector — strictly weaker filters that the EKF
subsumes.

Both downstream smoothers also have an adaptive-α motion gate
tuned against raw RANSAC delta magnitudes. With the EKF in front,
the upstream delta is much smaller, so the gates engage more
aggressively than designed and add lag without a measurement-side
justification.

**Action.**

- [x] Phase 1: flip both defaults to passthrough.
  - `EngineConfig.p_ema_min_alpha`: 0.15 → 1.0
  - `live_tracker_pipeline.rs::DISABLE_SMOOTH_H`: false → true
- [x] Phase 2: delete the now-dead code.
  - In `smooth_emit_h`, drop the per-element EMA block. Keep the
    `BlendState` anchor-switch lerp — different job.
  - Delete the `p_ema_*` fields from `EngineConfig`.
  - Delete `smooth_homography`, the `SMOOTH_*` constants, the
    `DISABLE_SMOOTH_H` toggle, and rename `SmoothedHomography` →
    `LastEmittedH` in `live_tracker_pipeline.rs`. The struct still
    earns its keep for the `LOSS_HIDE_AFTER_FRAMES` grace window.

The `BlendState` lerp inside `smooth_emit_h` stays — it smooths
chain-composition discontinuity at anchor switch, which the EKF
does not touch (the chain matrix `H_root→canonical` is static).

---

## 2. Sanity gates that mask matcher cliffs

**What.** Two distinct gates on the engine:

- `apply_sanity_gate` (`planar_engine.rs:1722`): when accepted
  inlier count drops below `sanity_gate_drop_ratio · EMA`,
  substitute the previous-frame `H` for up to 3 consecutive
  frames, then fall through to Lost.
- `homography_delta_is_sane` (300 px corner-jump cap): hard-reject
  any RANSAC fit that moves a viewport corner more than 300 px vs
  the previous accepted H.

**Root cause masked.** The matcher cliff at scale/perspective
drift past BRIEF's invariance, plus RANSAC's wrong-basin failure
mode on repetitive-feature scenes.

**Status.**

- The freeze is pragmatic: it buys 3 frames for the matcher to
  recover before the engine commits to Lost. The real fix is
  descriptor replacement (BEBLID / multi-scale FAST / perspective-
  rectified patches), all already on the analysis.md queue. Keep
  the freeze.
- The 300 px corner-jump cap may now be partially obsolete: the
  EKF provides temporal continuity at the measurement level
  (small Kalman gain on huge innovations), so wrong-basin RANSAC
  outputs are absorbed rather than locked into. **Caveat:** the
  EKF's sequential update can still be pulled by a self-consistent
  wrong-basin set (all inliers internally agreeing on the wrong
  H), so a hard pre-filter is defensible.

**Action.**

- [x] Instrument `LivePlanarEngine` with per-event counters so
  smoke runs can show whether the gates still fire meaningfully
  with the EKF active:
  - `sanity_gate_freeze_count`
  - `sanity_gate_reject_count`
  - `delta_sanity_reject_count`
- [x] Surface the counters via a public getter and emit them in
  the smoke harness JSONL summary.
- [x] Measured. Across book / magn / gintonic with EKF + Phase-2-
  smoothers-deleted: `delta_cap = 0`, `h_sanity = 0` on all three.
  `sanity_freeze = 6` and `sanity_reject = 2` on gintonic, `0` on
  the others — the sanity-gate freeze is still earning its keep
  on slow-tilt; the hard caps are confirmed dead with EKF on.
- [ ] Phase 3: delete `homography_delta_is_sane` and
  `homography_is_sane` (confirmed 0 fires across the three clips).
  Deferred — wait until on-device telemetry from a longer session
  shows no fires before pulling the safety nets.
- [ ] Phase 3: consider lowering `sanity_gate_max_consecutive`
  from 3 → 1 (the EKF already provides multi-frame smoothing).
  Deferred for the same reason.

---

## 3. Chain-composition stabilizers

**What.** Five distinct knobs all working around the same
underlying issue:

- `handoff_cooldown_ns` (500 ms throttle between handoffs)
- `max_chain_depth = 1` (refuse to chain a grandchild)
- `anchor_switch_blend_frames` (5-frame H lerp on leaf switch)
- `handoff_max_median_residual_px` (1.5 px gate on spawn-frame
  fit quality)
- `handoff_min_inlier_ratio` (0.4 gate on spawn-frame inlier
  ratio)

**Root cause masked.** Spawning a handoff bakes a *single-frame*
RANSAC fit into the chain matrix `H_root→canonical_new`, which
then biases every downstream frame for the lifetime of that
anchor. The five knobs are independent attempts to bound the
damage from that single noisy fit (throttle, depth-cap, smooth
the visible jump, reject the worst spawn frames).

**Status.** Band-aids on a structural problem. The EKF doesn't
help here — it operates on `H_anchor→view`, while the noise lives
in the static chain matrix.

**Action.** Infrastructure landed; effective on the three smoke
clips it's a silent no-op because the handoff trigger fires *after*
the parent's descriptors have degraded, so re-tracking the parent on
subsequent frames either fails the inlier floor (no observation
contributed) or produces a biased fit (rejected by the delta gate).

The implementation:

- `EngineConfig.use_chain_refine` / `chain_refine_frames` defaults.
- `ChainRefineState { new_anchor_id, parent_id, parent_h_root_to_canonical,
  frames_remaining, chain_sum, count }` seeded with the spawn-frame
  value (count = 1).
- `start_chain_refine` fires at every successful `spawn_handoff`.
- `step_chain_refine` runs each accepted Locked frame: re-track the
  parent against the current frame, derive a candidate
  `h_root_to_canonical_new = inv(H_new_to_view) · H_parent_to_view ·
  h_root_to_parent`, gate it against the running mean (max 5 px
  corner delta — biased post-degradation fits typically fail this),
  fold into the running mean, write back to the cached anchor's
  chain matrix.

The two safeguards (inlier floor + delta gate) ensure no regression:
either an observation is healthy (rare on these clips) or it's
dropped silently. On the three smoke clips: metrics bit-identical
vs the pre-refinement baseline.

The refinement *does* deliver value in the structural sense: when a
parent stays healthy enough across the post-handoff window to pass
both gates, the chain matrix tracks the running mean rather than
the single spawn-frame fit. This is the right behaviour for future
clips with proactive handoff dynamics (parent still healthy at
spawn time) or for the eventual descriptor improvements (BEBLID /
multi-scale FAST) that would extend the parent's effective range
past the spawn frame.

What's still deferred: the five chain-stabilizer knobs
(`handoff_cooldown_ns`, `max_chain_depth`, etc.) — relaxing them
requires the refinement to be demonstrably reducing chain noise on
real-device traces first.

---

## 4. Overlay swap clear+flash

**What.** `live_session.rs::clear_anchor_state_for_relock` wipes
the surface map and all overlay items before dispatching async
OCR, producing a ~150 ms flash where overlays are absent until
the new OCR result arrives.

**Root cause masked.** No transactional view of "the new overlay
set as a whole". `run_post_detect` upserts overlays one-by-one
streaming in, with no notion of "this is one atomic refresh".

**Status.** Documented in detail at `analysis.md` § "Overlay swap
on refresh". Three pairwise-bbox heuristics (IoU, containment,
displace-on-overlap) were each tried and each failed on a
different legitimate case. The clear+flash is the deliberate
chosen position.

**Action.** Deferred — implementing the proper staged atomic swap
crosses `live_session.rs::run_post_detect`,
`live_tracker_pipeline.rs::run_refresh_inner`, and every caller
of `upsert_block_overlay_bitmap`, plus needs a lock-ordering
review (the compositor takes `overlay_items.lock()` to render).

Not a candidate for this cleanup pass.

---

## 5. OCR route smoothing (NOT a bandaid)

**What.** `ocr_runtime.rs::smooth_dominant_routes` folds rare
script classifications into the dominant when the dominant ratio
crosses a threshold.

**Root cause.** Per-strip PaddleOCR script classifier is noisy.

**Status.** **Principled, not a bandaid.** The classifier is a
separate trained model — output aggregation is the right tool
when you can't change the model. Leave alone.

## 6. Quadrant estimation fallback chain (NOT a bandaid)

**What.** `estimate_canonical_quadrant` runs per-strip
classification with majority + confidence gates, then falls back
to `last_known_quadrant`, then to `default_canonical_quadrant`.

**Status.** **Principled.** Same shape as #5 — the underlying
per-strip estimate is noisy because the model is what it is.
Aggregation + fallback chain is the right design. Leave alone.

---

## Summary

| # | Category | Action this pass |
|---|---|---|
| 1 | Three H smoothers stacked | Phase 2 done: dead code deleted |
| 2 | Sanity gates / delta cap | Instrumented + measured (delta cap dead; freeze still earns its keep) |
| 3 | Chain-composition stabilizers | In progress (sub-pixel FAST already landed in `b8ae35a`) |
| 4 | Overlay swap clear+flash | Deferred (cross-cutting refactor) |
| 5 | OCR route smoothing | Leave (principled) |
| 6 | Quadrant fallback chain | Leave (principled) |

Phase-2 deletions for #1 and #2 are gated on smoke + on-device
validation that the defaults change does what's expected.
