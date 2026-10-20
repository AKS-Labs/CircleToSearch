//! Sliding-window surface map: per-line strip identity and storage.
//!
//! Today's pipeline runs OCR once per acquire and re-OCRs only on a
//! fresh acquire of the surface. The surface map breaks that
//! constraint: every detection that lands on the active surface gets
//! merged into a persistent **map of lines**, keyed by physical line
//! identity. Pan reveals more of a line → its extent in the map
//! grows. Zoom out brings new lines into view → they're added. The
//! user always sees the live camera with overlays projected from the
//! map through the active homography.
//!
//! This module is the first piece: the data structure for the map
//! and the `same_line` predicate that decides whether a new
//! detection refers to an existing line or is a fresh one. Pure;
//! not yet wired into the live pipeline (the acquire path keeps
//! re-creating overlays from scratch).
//!
//! All coordinates are **surface (root-canonical)** coords. The
//! tracker is responsible for converting per-frame detections into
//! surface coords before they reach this layer. See
//! FUTURE_SURFACE_MAP.md for the broader design.

use translator_core::ocr::OrientedRect;

/// Stable identifier of a line in the surface map. Assigned at
/// creation, never reused.
pub type SurfaceLineId = u64;

/// One physical line on the surface, as known to the map. Created
/// from the first observation and grown by subsequent matching
/// observations.
#[derive(Clone, Debug)]
pub struct SurfaceLine {
    pub id: SurfaceLineId,
    /// The line's footprint in surface coords. Width grows as new
    /// observations extend the visible extent; height is the max
    /// observed (line x-height doesn't drift across observations of
    /// the same physical line). Angle is the average over
    /// observations, weighted by width.
    pub bbox: OrientedRect,
    /// Source-language text recognized for this line. Latest
    /// observation wins for now; later the map will track
    /// multiple observations and pick the highest-quality one
    /// (see surface-map progressive-OCR phase).
    pub source_text: String,
    /// Translated text for this line. Mirrors `source_text` policy.
    pub translated_text: String,
    /// Per-word bold byte ranges over `source_text` (trimmed), pooled from the ink bold
    /// channel + CTC firings at the last rec. Cached so lines that skip re-rec (held camera)
    /// keep their per-word weight instead of falling back to a whole-line estimate.
    pub source_bold_ranges: Vec<translator_core::ocr::StyleRange>,
    /// BCP-47 source language tag (e.g. "nl", "en"), or empty when
    /// the source language wasn't recorded.
    pub source_language: String,
    /// How many observations have been folded into this line, for
    /// debugging and future "trust this line more" policies.
    pub observation_count: u32,
    /// `(u_min, u_max)` along the text direction at the moment the
    /// last recognition fired against this line. `None` until the
    /// caller marks the first rec done via `record_rec_extent`.
    /// The trigger rule compares this against new observations'
    /// extents to decide whether the line is "stale and panned-in
    /// past where rec saw" (re-rec) vs "same view as before"
    /// (skip rec).
    pub last_rec_extent: Option<(f32, f32)>,
}

impl SurfaceLine {
    /// Snapshot the current bbox's u-extent as "this is where rec
    /// just saw." Future observations whose merged u-range extends
    /// past this by ≥ ½ height become recognition-eligible again.
    /// Call after each successful rec for this line.
    pub fn record_rec_extent(&mut self) {
        self.last_rec_extent = Some(bbox_u_range(&self.bbox));
    }
}

/// `(u_min, u_max)` along the rect's own text-direction axis. Pure;
/// uses the rect's `angle_radians` to define the basis.
fn bbox_u_range(b: &OrientedRect) -> (f32, f32) {
    let c = b.angle_radians.cos();
    let s = b.angle_radians.sin();
    let mut u_min = f32::INFINITY;
    let mut u_max = f32::NEG_INFINITY;
    for (x, y) in b.corners() {
        let u = x * c + y * s;
        if u < u_min {
            u_min = u;
        }
        if u > u_max {
            u_max = u;
        }
    }
    (u_min, u_max)
}

/// One incoming observation from the acquire pipeline. The map
/// decides whether it merges into an existing `SurfaceLine` or
/// creates a new one.
#[derive(Clone, Debug)]
pub struct SurfaceLineObservation {
    pub bbox: OrientedRect,
    pub source_text: String,
    pub translated_text: String,
    pub source_language: String,
}

/// What happened when `SurfaceMap::add_or_merge` consumed an
/// observation, and whether the caller should run recognition on
/// the affected line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddResult {
    /// A new line was created with this id. Recognition required
    /// (the line has no text yet).
    Created(SurfaceLineId),
    /// The observation matched the line with this id; the merged
    /// bbox extended past `last_rec_extent` by ≥ ½ line height (or
    /// the line was never rec'd). Caller should queue rec — the
    /// new extent contains glyphs the recognizer hasn't seen yet.
    MergedAndExtended(SurfaceLineId),
    /// The observation matched the line with this id and the
    /// merged bbox didn't meaningfully extend past
    /// `last_rec_extent`. Caller should reuse cached text — same
    /// view, nothing new.
    MergedUnchanged(SurfaceLineId),
}

impl AddResult {
    pub fn id(self) -> SurfaceLineId {
        match self {
            AddResult::Created(id)
            | AddResult::MergedAndExtended(id)
            | AddResult::MergedUnchanged(id) => id,
        }
    }

    /// True when the caller should run recognition for this line.
    pub fn needs_rec(self) -> bool {
        matches!(
            self,
            AddResult::Created(_) | AddResult::MergedAndExtended(_)
        )
    }
}

/// Threshold on u-extent growth, expressed as a fraction of line
/// height, that promotes a `MergedUnchanged` to `MergedAndExtended`.
/// Per FUTURE_SURFACE_MAP.md: "if D.surface_bbox extends
/// L.observed_extent (by ≥ ½ x-height)…".
///
/// The doc proposed 0.5, but at typical phone zoom PaddleOCR's
/// frame-to-frame contour fluctuation can clear 0.5 × height on a
/// held camera (a 40 px line tolerates 20 px boundary jitter
/// trivially). With 0.5, refresh on a still hand still triggers
/// `MergedAndExtended` → bbox grows → re-raster. 1.0 × height ≈ a
/// short word's width — well above detector noise, below "an
/// actually new word visible at the edge."
const EXTENT_GROWTH_TRIGGER_FRACTION: f32 = 1.0;

/// Collection of lines on one surface, keyed by physical line
/// identity. Built up across observations; queried by the renderer
/// to produce per-frame overlays.
#[derive(Clone, Debug, Default)]
pub struct SurfaceMap {
    next_id: SurfaceLineId,
    lines: Vec<SurfaceLine>,
}

impl SurfaceMap {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            lines: Vec::new(),
        }
    }

    pub fn lines(&self) -> &[SurfaceLine] {
        &self.lines
    }

    /// Drop cached lines whose bbox overlaps any of `rects`. The screen monitor
    /// calls this when content under a pill changed (or on a scroll) so the next
    /// detection at that geometry can't be a `MergedUnchanged` cache hit that
    /// re-shows stale OCR text.
    pub fn remove_overlapping(&mut self, rects: &[OrientedRect]) {
        if rects.is_empty() {
            return;
        }
        let targets: Vec<translator_core::ocr::Rect> = rects.iter().map(|r| r.to_aabb()).collect();
        self.lines.retain(|line| {
            let lb = line.bbox.to_aabb();
            !targets.iter().any(|t| {
                lb.left < t.right && t.left < lb.right && lb.top < t.bottom && t.top < lb.bottom
            })
        });
    }

    pub fn get(&self, id: SurfaceLineId) -> Option<&SurfaceLine> {
        self.lines.iter().find(|l| l.id == id)
    }

    pub fn get_mut(&mut self, id: SurfaceLineId) -> Option<&mut SurfaceLine> {
        self.lines.iter_mut().find(|l| l.id == id)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// First existing line whose footprint matches `obs`'s under the
    /// `same_line` predicate, or `None`. Linear scan — the map size
    /// is bounded by the visible-text-on-one-surface working set
    /// (dozens at most for live UX), so a linear scan stays cheap.
    pub fn find_matching(&self, obs: &OrientedRect) -> Option<SurfaceLineId> {
        self.lines
            .iter()
            .find(|line| same_line(&line.bbox, obs))
            .map(|line| line.id)
    }

    /// Consume an observation. Merge into an existing line if its
    /// bbox matches via `same_line`; otherwise create a new line.
    /// The returned variant tells the caller whether to run
    /// recognition — see [`AddResult::needs_rec`].
    pub fn add_or_merge(&mut self, obs: SurfaceLineObservation) -> AddResult {
        if let Some(id) = self.find_matching(&obs.bbox) {
            let line = self
                .lines
                .iter_mut()
                .find(|l| l.id == id)
                .expect("find_matching returned an id not in the map");
            // Tentatively compute the merged bbox so we can ask
            // `needs_rec_after_merge` whether it extends past
            // `last_rec_extent`. We only commit the merge in the
            // **extended** case. Without this guard, noisy detections
            // and tracker drift creep the bbox forward observation by
            // observation: u-extent unions monotonically, cx slides
            // toward the union centre, and the overlay visibly shifts
            // on every refresh even when the camera is held still.
            let candidate = merge_bbox(&line.bbox, &obs.bbox, line.observation_count);
            let extended = {
                let probe = SurfaceLine {
                    bbox: candidate.clone(),
                    ..line.clone()
                };
                needs_rec_after_merge(&probe)
            };
            if extended {
                line.bbox = candidate;
            }
            line.observation_count = line.observation_count.saturating_add(1);
            // Note: observation's source_text / translated_text are
            // *not* written through here. Acquire-time observations
            // carry empty text by convention; the rec / translate
            // stages call `get_mut` directly to write results.
            if !obs.source_language.is_empty() {
                line.source_language = obs.source_language;
            }
            return if extended {
                AddResult::MergedAndExtended(id)
            } else {
                AddResult::MergedUnchanged(id)
            };
        }
        let id = self.next_id;
        self.next_id += 1;
        self.lines.push(SurfaceLine {
            id,
            bbox: obs.bbox,
            source_text: obs.source_text,
            translated_text: obs.translated_text,
            source_bold_ranges: Vec::new(),
            source_language: obs.source_language,
            observation_count: 1,
            last_rec_extent: None,
        });
        AddResult::Created(id)
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        // next_id keeps incrementing — never reuse ids.
    }
}

/// True if the merged line's current u-extent extends past the
/// recorded `last_rec_extent` by ≥ `EXTENT_GROWTH_TRIGGER_FRACTION
/// × bbox.height` on either side, OR if the line has never been
/// rec'd. Used to gate "should the caller queue recognition?"
fn needs_rec_after_merge(line: &SurfaceLine) -> bool {
    let (new_min, new_max) = bbox_u_range(&line.bbox);
    match line.last_rec_extent {
        None => true,
        Some((rec_min, rec_max)) => {
            let leftward = rec_min - new_min;
            let rightward = new_max - rec_max;
            let max_growth = leftward.max(rightward);
            max_growth >= EXTENT_GROWTH_TRIGGER_FRACTION * line.bbox.height
        }
    }
}

/// Decide whether two oriented rects in shared surface coords refer
/// to the same physical line on the underlying plane.
///
/// Predicate from FUTURE_SURFACE_MAP.md:
///
/// ```text
/// same_line(a, b) :=
///     angle_diff(a, b) < ~5°                  AND
///     |Δv| < 0.3 × avg(a.h, b.h)              AND   # baseline coincides
///     x_ranges_overlap_or_touch(a, b)               # along-text axis
/// ```
///
/// "v" is the cross-line axis (perpendicular to the average text
/// direction); "x" is the along-text axis. Both projections are
/// done in the **rotated basis** defined by the average of the two
/// rects' angles — *not* in image-axis coords, which would give
/// wrong answers on tilted scenes.
pub fn same_line(a: &OrientedRect, b: &OrientedRect) -> bool {
    const MAX_ANGLE_DIFF_RAD: f32 = 0.0872665; // 5°
    const BASELINE_TOL_FRAC: f32 = 0.3;

    let mut angle_diff = a.angle_radians - b.angle_radians;
    let pi = std::f32::consts::PI;
    while angle_diff > pi {
        angle_diff -= 2.0 * pi;
    }
    while angle_diff < -pi {
        angle_diff += 2.0 * pi;
    }
    if angle_diff.abs() > MAX_ANGLE_DIFF_RAD {
        return false;
    }

    let theta = 0.5 * (a.angle_radians + b.angle_radians);
    let sin_t = theta.sin();
    let cos_t = theta.cos();
    let dx = b.cx - a.cx;
    let dy = b.cy - a.cy;
    let delta_v = -dx * sin_t + dy * cos_t;
    let avg_h = 0.5 * (a.height + b.height);
    if avg_h <= 0.0 {
        return false;
    }
    if delta_v.abs() > BASELINE_TOL_FRAC * avg_h {
        return false;
    }

    let u_range = |r: &OrientedRect| -> (f32, f32) {
        let mut u_min = f32::INFINITY;
        let mut u_max = f32::NEG_INFINITY;
        for (x, y) in r.corners() {
            let u = x * cos_t + y * sin_t;
            if u < u_min {
                u_min = u;
            }
            if u > u_max {
                u_max = u;
            }
        }
        (u_min, u_max)
    };
    let (a_min, a_max) = u_range(a);
    let (b_min, b_max) = u_range(b);
    a_max >= b_min && b_max >= a_min
}

/// Merge two same-line bounding boxes into one. The "existing"
/// `line` already represents `prev_count` observations; the new
/// `obs` is a single observation. The result:
/// - angle: width-weighted running mean.
/// - height: max of the two (preserve x-height on the largest
///   reliable observation).
/// - along-text extent (u-range): union of both.
/// - cross-text position: existing line's cy (don't drift the
///   baseline; new observations with different cy are clamped
///   into the existing line's position).
fn merge_bbox(existing: &OrientedRect, obs: &OrientedRect, prev_count: u32) -> OrientedRect {
    let prev_weight = (prev_count.max(1) as f32) * existing.width.max(1.0);
    let obs_weight = obs.width.max(1.0);
    let total_weight = prev_weight + obs_weight;
    let new_angle = if total_weight > 0.0 {
        let sin_sum =
            prev_weight * existing.angle_radians.sin() + obs_weight * obs.angle_radians.sin();
        let cos_sum =
            prev_weight * existing.angle_radians.cos() + obs_weight * obs.angle_radians.cos();
        sin_sum.atan2(cos_sum)
    } else {
        existing.angle_radians
    };
    let sin_t = new_angle.sin();
    let cos_t = new_angle.cos();
    let u_range = |r: &OrientedRect| -> (f32, f32) {
        let mut u_min = f32::INFINITY;
        let mut u_max = f32::NEG_INFINITY;
        for (x, y) in r.corners() {
            let u = x * cos_t + y * sin_t;
            if u < u_min {
                u_min = u;
            }
            if u > u_max {
                u_max = u;
            }
        }
        (u_min, u_max)
    };
    let (a_min, a_max) = u_range(existing);
    let (b_min, b_max) = u_range(obs);
    let u_left = a_min.min(b_min);
    let u_right = a_max.max(b_max);
    let new_width = (u_right - u_left).max(existing.width.max(obs.width));
    let u_centre = 0.5 * (u_left + u_right);
    // v-coord (cross-line) of the existing line, used to keep the
    // baseline stable while u shifts.
    let v_existing = -existing.cx * sin_t + existing.cy * cos_t;
    let new_cx = u_centre * cos_t - v_existing * sin_t;
    let new_cy = u_centre * sin_t + v_existing * cos_t;
    OrientedRect {
        cx: new_cx,
        cy: new_cy,
        width: new_width,
        height: existing.height.max(obs.height),
        angle_radians: new_angle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use translator_core::ocr::OrientedRect;

    fn rect(cx: f32, cy: f32, w: f32, h: f32, angle_deg: f32) -> OrientedRect {
        OrientedRect {
            cx,
            cy,
            width: w,
            height: h,
            angle_radians: angle_deg.to_radians(),
        }
    }

    // ---- same_line ----

    #[test]
    fn same_line_identical_rects() {
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        assert!(same_line(&a, &a));
    }

    #[test]
    fn same_line_overlapping_extent() {
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let b = rect(150.0, 50.0, 200.0, 30.0, 0.0);
        assert!(same_line(&a, &b), "overlapping x-ranges should match");
    }

    #[test]
    fn same_line_touching_extent() {
        // a: [0..200], b: [200..400] → touch at x=200.
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let b = rect(300.0, 50.0, 200.0, 30.0, 0.0);
        assert!(
            same_line(&a, &b),
            "touching x-ranges should match (overlap-or-touch predicate)"
        );
    }

    #[test]
    fn same_line_disjoint_extent() {
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let b = rect(500.0, 50.0, 200.0, 30.0, 0.0);
        assert!(!same_line(&a, &b), "non-touching x-ranges should not match");
    }

    #[test]
    fn same_line_baseline_too_far() {
        // Height 30 → 0.3 * 30 = 9 px tolerance. cy delta = 20 > 9.
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let b = rect(100.0, 70.0, 200.0, 30.0, 0.0);
        assert!(!same_line(&a, &b), "different baselines should not match");
    }

    #[test]
    fn same_line_baseline_within_tolerance() {
        // 0.3 * 30 = 9 px tolerance. cy delta = 5 < 9.
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let b = rect(100.0, 55.0, 200.0, 30.0, 0.0);
        assert!(same_line(&a, &b));
    }

    #[test]
    fn same_line_angle_too_different() {
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let b = rect(100.0, 50.0, 200.0, 30.0, 10.0);
        assert!(!same_line(&a, &b), "10° angle difference should not match");
    }

    #[test]
    fn same_line_small_angle_diff_ok() {
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let b = rect(100.0, 50.0, 200.0, 30.0, 3.0);
        assert!(same_line(&a, &b), "3° angle diff should match");
    }

    /// On a 30°-tilted line, the "baseline tolerance" is on the
    /// cross-line axis (rotated basis), NOT on image y. Two same-line
    /// observations on the tilted line have very different image-y
    /// values but tiny cross-line offset.
    #[test]
    fn same_line_tilted_rotated_basis() {
        let theta = 30.0_f32.to_radians();
        let h = 30.0;
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        let a = OrientedRect {
            cx: 100.0,
            cy: 100.0,
            width: 200.0,
            height: h,
            angle_radians: theta,
        };
        // Move along the text direction by 80 px. Result: cx shifts by
        // 80·cos(30°) ≈ 69, cy shifts by 80·sin(30°) = 40. Cross-line
        // delta should be 0 (within tolerance).
        let b = OrientedRect {
            cx: 100.0 + 80.0 * cos_t,
            cy: 100.0 + 80.0 * sin_t,
            width: 200.0,
            height: h,
            angle_radians: theta,
        };
        assert!(
            same_line(&a, &b),
            "tilted same-line observations should match in rotated basis"
        );
    }

    #[test]
    fn same_line_tilted_different_baseline() {
        // Same tilt but one line is "below" the other in the rotated
        // basis (cross-line offset).
        let theta = 30.0_f32.to_radians();
        let h = 30.0;
        let a = OrientedRect {
            cx: 100.0,
            cy: 100.0,
            width: 200.0,
            height: h,
            angle_radians: theta,
        };
        // Shift by 40 px perpendicular to the line.
        let b = OrientedRect {
            cx: 100.0 - 40.0 * theta.sin(),
            cy: 100.0 + 40.0 * theta.cos(),
            width: 200.0,
            height: h,
            angle_radians: theta,
        };
        assert!(
            !same_line(&a, &b),
            "different cross-line baselines on a tilted text should not match"
        );
    }

    // ---- SurfaceMap::add_or_merge ----

    fn obs(rect: OrientedRect, src: &str, tgt: &str) -> SurfaceLineObservation {
        SurfaceLineObservation {
            bbox: rect,
            source_text: src.to_string(),
            translated_text: tgt.to_string(),
            source_language: "nl".to_string(),
        }
    }

    #[test]
    fn add_or_merge_creates_then_merges() {
        let mut map = SurfaceMap::new();
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let r0 = map.add_or_merge(obs(a, "Hallo", "Hello"));
        assert!(matches!(r0, AddResult::Created(_)));
        assert!(r0.needs_rec());
        assert_eq!(map.len(), 1);

        // Same line, extended to the right.
        let b = rect(180.0, 50.0, 240.0, 30.0, 0.0);
        let r1 = map.add_or_merge(obs(b, "Hallo wereld", "Hello world"));
        // Never recorded a rec extent → second merge still needs rec.
        assert!(matches!(r1, AddResult::MergedAndExtended(_)));
        assert!(r1.needs_rec());
        assert_eq!(r0.id(), r1.id(), "same line should keep its id");
        assert_eq!(map.len(), 1);

        let line = map.get(r1.id()).unwrap();
        assert_eq!(line.observation_count, 2);
        let merged_aabb = line.bbox.to_aabb();
        assert!(merged_aabb.left as f32 <= 0.0_f32 + 1.0);
        assert!(merged_aabb.right as f32 >= 300.0);
    }

    #[test]
    fn merged_unchanged_after_rec_when_no_growth() {
        let mut map = SurfaceMap::new();
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let r0 = map.add_or_merge(obs(a, "", ""));
        let id = r0.id();
        // Simulate rec completion: record extent.
        map.get_mut(id).unwrap().record_rec_extent();
        // Same view again: same bbox, no extension.
        let r1 = map.add_or_merge(obs(a, "", ""));
        assert!(matches!(r1, AddResult::MergedUnchanged(_)));
        assert!(!r1.needs_rec(), "no growth → no re-rec needed");
    }

    #[test]
    fn merged_extended_when_growth_past_last_rec_extent() {
        let mut map = SurfaceMap::new();
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0); // u in [0, 200]
        let r0 = map.add_or_merge(obs(a, "", ""));
        map.get_mut(r0.id()).unwrap().record_rec_extent();
        // Threshold is 1.0 * height = 30 px. Extend right by 40 px.
        let b = rect(140.0, 50.0, 200.0, 30.0, 0.0); // u in [40, 240]
        let r1 = map.add_or_merge(obs(b, "", ""));
        assert!(matches!(r1, AddResult::MergedAndExtended(_)));
        assert!(r1.needs_rec(), "40px > 30px threshold → re-rec");
    }

    #[test]
    fn merged_unchanged_when_growth_under_threshold() {
        let mut map = SurfaceMap::new();
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let r0 = map.add_or_merge(obs(a, "", ""));
        map.get_mut(r0.id()).unwrap().record_rec_extent();
        // 1.0 * height = 30 px threshold. Extend right by only 20 px.
        let b = rect(110.0, 50.0, 200.0, 30.0, 0.0); // u in [10, 210]
        let r1 = map.add_or_merge(obs(b, "", ""));
        assert!(matches!(r1, AddResult::MergedUnchanged(_)));
        assert!(!r1.needs_rec(), "20px < 30px threshold → skip re-rec");
    }

    #[test]
    fn merged_extended_when_never_rec_yet() {
        let mut map = SurfaceMap::new();
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        map.add_or_merge(obs(a, "", ""));
        // No record_rec_extent → second observation must re-rec
        // even if it's identical.
        let r = map.add_or_merge(obs(a, "", ""));
        assert!(matches!(r, AddResult::MergedAndExtended(_)));
        assert!(r.needs_rec());
    }

    #[test]
    fn add_or_merge_different_lines_stay_separate() {
        let mut map = SurfaceMap::new();
        let line_a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let line_b = rect(100.0, 100.0, 200.0, 30.0, 0.0);
        let line_c = rect(100.0, 150.0, 200.0, 30.0, 0.0);
        map.add_or_merge(obs(line_a, "A", "A_t"));
        map.add_or_merge(obs(line_b, "B", "B_t"));
        map.add_or_merge(obs(line_c, "C", "C_t"));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn add_or_merge_extends_in_rotated_basis() {
        // Tilted line, second observation shifted 80 px along the
        // text direction. a's u-range is [u₀-50, u₀+50]; b's is
        // [u₀+30, u₀+130]; they overlap by 20 px. Merged u-range
        // width = 180.
        let theta = 25.0_f32.to_radians();
        let h = 30.0;
        let mut map = SurfaceMap::new();
        let a = OrientedRect {
            cx: 100.0,
            cy: 100.0,
            width: 100.0,
            height: h,
            angle_radians: theta,
        };
        map.add_or_merge(obs(a, "Hi", "Hi"));
        let b = OrientedRect {
            cx: 100.0 + 80.0 * theta.cos(),
            cy: 100.0 + 80.0 * theta.sin(),
            width: 100.0,
            height: h,
            angle_radians: theta,
        };
        let r = map.add_or_merge(obs(b, "Hi there", "Hi there"));
        // Never recorded a rec extent → second merge needs re-rec.
        assert!(matches!(r, AddResult::MergedAndExtended(_)));
        let line = map.get(r.id()).unwrap();
        assert!(
            line.bbox.width >= 175.0 && line.bbox.width <= 185.0,
            "merged width {} not in [175, 185]",
            line.bbox.width
        );
    }

    #[test]
    fn add_or_merge_preserves_baseline_under_jitter() {
        // Two observations of the same line with slight baseline noise.
        // The merged baseline should stay near the first observation;
        // shouldn't drift toward the noisier second.
        let mut map = SurfaceMap::new();
        let a = rect(100.0, 50.0, 200.0, 30.0, 0.0);
        let b = rect(100.0, 52.0, 200.0, 30.0, 0.0); // 2 px y jitter
        let r0 = map.add_or_merge(obs(a, "A", "A"));
        map.add_or_merge(obs(b, "A", "A"));
        let line = map.get(r0.id()).unwrap();
        assert!(
            (line.bbox.cy - 50.0).abs() < 1.0,
            "merged cy drifted: {}",
            line.bbox.cy
        );
    }
}
