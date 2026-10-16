//! Per-box screen-monitoring core (the v2 "Monitoring" logic from
//! `SCREEN_CHANGE_DETECTION.md`).
//!
//! Pure classification over a global pinhole lattice. Each frame the GPU
//! reduction recovers the screen content under our overlay (`screen_est`) at a
//! fixed lattice of points and hands this module one byte per point; this module
//! decides whether the view scrolled, which boxes' text changed, or nothing
//! happened. No IO and no GPU live here — only the math that turns lattice
//! samples + resident box state into a [`FrameClassification`].
//!
//! The signal is defined relative to each box's text strokes, not raw pixels
//! ("measure the text, not the pixels"). A glyph stroke is opaque, so a hole that
//! sits on it stays put while the background behind the box moves, and only moves
//! when the text itself changes. Which holes are "on a stroke" is decided by
//! behaviour (low temporal variance) intersected with appearance (the binarize
//! bootstrap), because the background can mimic ink's appearance but not its
//! stability — see the three-layer mask in the spec.

use translator_core::ocr::OrientedRect;

/// A point in the global sampling lattice, in canonical (overlay) coords.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatticePoint {
    pub x: f32,
    pub y: f32,
}

/// The fixed per-frame sampling lattice for a canonical `w×h` frame. Built once
/// per capture geometry; every frame supplies exactly one `screen_est` byte per
/// point, in `points()` order.
pub struct Lattice {
    points: Vec<LatticePoint>,
    cols: u32,
    rows: u32,
    spacing: f32,
    /// Centre offset of the first point: `spacing / 2`. A point's canonical
    /// position is `(origin + col*spacing + row_phase, origin + row*spacing)`,
    /// where `row_phase = row % spacing` staggers each row (see `build`).
    origin: f32,
}

impl Lattice {
    /// A `spacing`-pitch grid (canonical px, fractional allowed — e.g. 2.5 ≈ 5
    /// display px) with each row shifted right by `row % spacing`, so the holes
    /// form a sheared lattice rather than solid columns. A straight grid beats
    /// against the display's pixel grid into a visible axis-aligned moiré; the
    /// per-row stagger rotates that beat off the vertical axis. The step is 1
    /// canonical px/row (2 display px after the present's 2× upscale), which walks
    /// a fresh column phase each row, cycling every `spacing` rows. The renderer's
    /// punch and recovery shaders apply the identical shift and snap to the mirror
    /// texel grid, so a fractional pitch still samples each hole pixel-exact.
    /// Points are emitted row-major (y outer, x inner), so index `row*cols + col`
    /// — the order the GPU readback grid is read in.
    pub fn build(canon_w: u32, canon_h: u32, spacing: f32) -> Self {
        let spacing = spacing.max(1.0);
        let half = spacing * 0.5;
        let cols = ((canon_w.saturating_sub(1) as f32) / spacing).floor() as u32 + 1;
        let rows = ((canon_h.saturating_sub(1) as f32) / spacing).floor() as u32 + 1;
        let mut points = Vec::with_capacity((cols * rows) as usize);
        for row in 0..rows {
            let y = half + row as f32 * spacing;
            let phase = (row as f32) % spacing;
            for col in 0..cols {
                let x = half + col as f32 * spacing + phase;
                points.push(LatticePoint { x, y });
            }
        }
        Lattice {
            points,
            cols,
            rows,
            spacing,
            origin: half,
        }
    }

    /// Grid `(cols, rows)` for a `canon_w×canon_h` frame at `spacing`, without
    /// allocating the points — the same counts [`build`](Self::build) produces, so
    /// the GPU readback can be sized to match before a `Lattice` exists.
    pub fn dims(canon_w: u32, canon_h: u32, spacing: f32) -> (u32, u32) {
        let spacing = spacing.max(1.0);
        let cols = ((canon_w.saturating_sub(1) as f32) / spacing).floor() as u32 + 1;
        let rows = ((canon_h.saturating_sub(1) as f32) / spacing).floor() as u32 + 1;
        (cols, rows)
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn points(&self) -> &[LatticePoint] {
        &self.points
    }

    pub fn cols(&self) -> u32 {
        self.cols
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    pub fn spacing(&self) -> f32 {
        self.spacing
    }

    pub fn origin(&self) -> f32 {
        self.origin
    }

    /// Indices of the lattice points inside oriented rect `r`. Callers use this to
    /// assign a box its hole subset and to align an externally-computed bootstrap
    /// mask to the same order.
    pub fn holes_in_rect(&self, r: &OrientedRect) -> Vec<usize> {
        self.points
            .iter()
            .enumerate()
            .filter(|(_, p)| point_in_rect(p.x, p.y, r))
            .map(|(i, _)| i)
            .collect()
    }

    /// Indices of the lattice points inside the detection `contour` (flattened
    /// `[x0,y0,x1,y1,…]`). Tighter than the bounding rect — it hugs the text run, so
    /// the hole set isn't padded with the box's background margin.
    pub fn holes_in_polygon(&self, contour: &[f32]) -> Vec<usize> {
        if contour.len() < 6 {
            return Vec::new();
        }
        self.points
            .iter()
            .enumerate()
            .filter(|(_, p)| point_in_polygon(p.x, p.y, contour))
            .map(|(i, _)| i)
            .collect()
    }
}

/// Ray-casting point-in-polygon for a flattened `[x0,y0,…]` contour.
fn point_in_polygon(px: f32, py: f32, poly: &[f32]) -> bool {
    let n = poly.len() / 2;
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[2 * i], poly[2 * i + 1]);
        let (xj, yj) = (poly[2 * j], poly[2 * j + 1]);
        if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn point_in_rect(px: f32, py: f32, r: &OrientedRect) -> bool {
    let dx = px - r.cx;
    let dy = py - r.cy;
    let cos = r.angle_radians.cos();
    let sin = r.angle_radians.sin();
    let lx = (dx * cos + dy * sin).abs();
    let ly = (-dx * sin + dy * cos).abs();
    lx <= r.width * 0.5 && ly <= r.height * 0.5
}

/// One recovered sample per lattice point: the screen RGB under the overlay (or the
/// raw screen at a gap point). Per-channel so an isoluminant change — different colour
/// at the same brightness — still registers, which a luma-only delta would miss.
pub type Rgb = [u8; 3];

/// Max per-channel absolute delta between two samples.
pub fn channel_delta(a: Rgb, b: Rgb) -> u32 {
    (0..3)
        .map(|i| (a[i] as i32 - b[i] as i32).unsigned_abs())
        .max()
        .unwrap_or(0)
}

/// Tunables for the classifier. Distances are per-channel RGB units (0..255).
///
/// One signal over a box's contour holes vs baseline: the fraction whose per-channel
/// RGB delta exceeds `hard_threshold`. RGB (not luma) so an isoluminant change still
/// registers; one signal so there's nothing to balance against — a scroll/replacement
/// moves the strokes (and a coloured background wholesale), a benign sub-threshold
/// drift scores ~0.
#[derive(Debug, Clone, Copy)]
pub struct MonitorConfig {
    /// Frames after an acquire before a box is judged. The baseline is re-snapshotted
    /// at the end of this window — by then the overlay has settled into the captured
    /// mirror — so a stable overlay reads as no-change.
    pub warmup_frames: u32,
    /// Per-hole `|current − baseline|` luma delta that counts as a *hard* swing. High
    /// enough that coherent video motion doesn't cross it, low enough that a gap↔stroke
    /// crossing does.
    pub hard_threshold: u8,
    /// Fraction of a box's holes that must swing hard for the box to trip.
    pub hard_frac: f32,
    /// Fraction of judged boxes tripping at once that reads as a scroll (anchored text
    /// moving together) rather than a per-box edit.
    pub scroll_frac: f32,
    /// Scroll needs at least this many judged boxes — a single box can't be told apart
    /// from an in-place text change.
    pub scroll_min_boxes: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            warmup_frames: 6,
            hard_threshold: 110,
            hard_frac: 0.25,
            scroll_frac: 0.7,
            scroll_min_boxes: 2,
        }
    }
}

/// What the monitor concludes for one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameClassification {
    /// Nothing of interest changed — leave the overlay as-is.
    Quiet,
    /// The whole view translated (anchored text moved together) — hide + full
    /// re-acquire.
    Scroll,
    /// These boxes' underlying text changed — batched targeted re-OCR.
    BoxesChanged(Vec<u64>),
}

struct BoxMonitor {
    id: u64,
    /// Lattice indices inside this box's contour. Parallel to `baseline`.
    holes: Vec<usize>,
    /// Recovered screen RGB baseline, re-snapshotted at warmup end.
    baseline: Vec<Rgb>,
    /// Frames observed since the baseline was set.
    frames: u32,
}

impl BoxMonitor {
    /// One signal over the box's contour holes vs the baseline: the fraction whose
    /// per-channel RGB delta exceeds `hard_threshold`. A scroll / content replacement
    /// moves the strokes (and, for a coloured background, the whole patch); a benign
    /// uniform drift under the threshold scores ~0. Not judged until warmup elapses
    /// (baseline re-snapshotted at warmup end, once the overlay has settled).
    fn deviation(&self, samples: &[Rgb], cfg: &MonitorConfig) -> BoxDeviation {
        if self.frames < cfg.warmup_frames || self.holes.is_empty() {
            return BoxDeviation::not_judged(self.id);
        }
        let mut hard = 0usize;
        let mut max_delta = 0u32;
        let mut sum_delta = 0u64;
        for (k, &h) in self.holes.iter().enumerate() {
            let delta = channel_delta(samples[h], self.baseline[k]);
            max_delta = max_delta.max(delta);
            sum_delta += delta as u64;
            if delta > cfg.hard_threshold as u32 {
                hard += 1;
            }
        }
        BoxDeviation {
            id: self.id,
            frac: hard as f32 / self.holes.len() as f32,
            mean_delta: sum_delta as f32 / self.holes.len() as f32,
            max_delta,
            judged: true,
        }
    }
}

struct BoxDeviation {
    id: u64,
    /// Fraction of holes swinging hard this frame.
    frac: f32,
    /// Mean per-channel delta over all holes — diagnostic only (logged).
    mean_delta: f32,
    /// Largest single-hole per-channel delta, for debug.
    max_delta: u32,
    /// False while the box is still in its warmup window.
    judged: bool,
}

impl BoxDeviation {
    fn not_judged(id: u64) -> Self {
        BoxDeviation {
            id,
            frac: 0.0,
            mean_delta: 0.0,
            max_delta: 0,
            judged: false,
        }
    }

    fn changed(&self, cfg: &MonitorConfig) -> bool {
        self.judged && self.frac > cfg.hard_frac
    }
}

/// Holds the resident box state and classifies each frame. Stateful shell around the
/// pure [`classify`] core.
pub struct ScreenMonitor {
    lattice: Lattice,
    boxes: Vec<BoxMonitor>,
    cfg: MonitorConfig,
    /// Diagnostic summary of the last `observe`: (judged boxes, changed boxes, max
    /// hard-fraction across boxes, as a percent).
    last_stats: (usize, usize, usize),
    /// The judged box with the highest hard-fraction last observe:
    /// (id, holes, hard_fraction_pct).
    last_top: Option<(u64, usize, usize)>,
    /// Per-box `(id, holes, hard_frac_pct, max_delta, changed)` from the last observe,
    /// for debug logging joined with each block's text.
    last_devs: Vec<(u64, usize, usize, u32, u32, bool)>,
}

impl ScreenMonitor {
    pub fn new(lattice: Lattice, cfg: MonitorConfig) -> Self {
        ScreenMonitor {
            lattice,
            boxes: Vec::new(),
            cfg,
            last_stats: (0, 0, 0),
            last_top: None,
            last_devs: Vec::new(),
        }
    }

    /// Per-box `(id, holes, hard_frac_pct, max_delta, changed)` from last observe.
    pub fn debug_boxes(&self) -> &[(u64, usize, usize, u32, u32, bool)] {
        &self.last_devs
    }

    /// (judged boxes, changed boxes, max hard-fraction pct) from the last observe.
    pub fn debug_last_stats(&self) -> (usize, usize, usize) {
        self.last_stats
    }

    /// The judged box with the highest hard-fraction last observe:
    /// (id, holes, hard_frac_pct).
    pub fn debug_top_box(&self) -> Option<(u64, usize, usize)> {
        self.last_top
    }

    pub fn lattice(&self) -> &Lattice {
        &self.lattice
    }

    /// Register or replace a box after an acquire. `holes` are the lattice indices
    /// inside the box's contour; `clean_samples` is the full-lattice `screen_est` of
    /// the clean read, from which the per-hole baseline is snapshotted.
    pub fn set_box(&mut self, id: u64, holes: Vec<usize>, clean_samples: &[Rgb]) {
        assert_eq!(
            clean_samples.len(),
            self.lattice.len(),
            "clean_samples must cover the whole lattice"
        );
        let baseline: Vec<Rgb> = holes.iter().map(|&h| clean_samples[h]).collect();
        let monitor = BoxMonitor {
            id,
            holes,
            baseline,
            frames: 0,
        };
        match self.boxes.iter_mut().find(|b| b.id == id) {
            Some(slot) => *slot = monitor,
            None => self.boxes.push(monitor),
        }
    }

    pub fn remove_box(&mut self, id: u64) {
        self.boxes.retain(|b| b.id != id);
    }

    /// Drop all boxes (the resident overlay was replaced); per-hole stats are left
    /// to be reset by the next `set_box`.
    pub fn clear_boxes(&mut self) {
        self.boxes.clear();
    }

    /// Mark `mask[h] = true` for every lattice hole any box covers (our overlay's
    /// footprint). Used to exclude our own pills from the global-motion signal so
    /// the overlay's own redraw isn't read as screen motion.
    pub fn fill_covered(&self, mask: &mut [bool]) {
        for b in &self.boxes {
            for &h in &b.holes {
                if let Some(m) = mask.get_mut(h) {
                    *m = true;
                }
            }
        }
    }

    pub fn box_ids(&self) -> Vec<u64> {
        self.boxes.iter().map(|b| b.id).collect()
    }

    /// One monitoring frame. `samples` is the recovered screen RGB, one per lattice
    /// point in `points()` order.
    pub fn observe(&mut self, samples: &[Rgb]) -> FrameClassification {
        assert_eq!(
            samples.len(),
            self.lattice.len(),
            "samples must cover the whole lattice"
        );
        let cfg = self.cfg;
        for b in self.boxes.iter_mut() {
            b.frames += 1;
            // Re-baseline once, at warmup end: the initial baseline was captured before
            // our overlay reached the captured mirror, so snapshot the now-settled frame.
            if b.frames == cfg.warmup_frames {
                for (k, &h) in b.holes.iter().enumerate() {
                    b.baseline[k] = samples[h];
                }
            }
        }
        let devs: Vec<BoxDeviation> = self
            .boxes
            .iter()
            .map(|b| b.deviation(samples, &cfg))
            .collect();
        let judged = devs.iter().filter(|d| d.judged).count();
        let changed_n = devs.iter().filter(|d| d.changed(&cfg)).count();
        let max_frac = devs.iter().map(|d| d.frac).fold(0.0f32, f32::max);
        self.last_stats = (judged, changed_n, (max_frac * 100.0).round() as usize);
        self.last_top = devs
            .iter()
            .filter(|d| d.judged)
            .max_by(|a, b| a.frac.total_cmp(&b.frac))
            .map(|d| {
                let holes = self
                    .boxes
                    .iter()
                    .find(|b| b.id == d.id)
                    .map_or(0, |b| b.holes.len());
                (d.id, holes, (d.frac * 100.0).round() as usize)
            });
        self.last_devs = self
            .boxes
            .iter()
            .zip(&devs)
            .map(|(b, d)| {
                (
                    d.id,
                    b.holes.len(),
                    (d.frac * 100.0).round() as usize,
                    d.max_delta,
                    d.mean_delta.round() as u32,
                    d.changed(&cfg),
                )
            })
            .collect();
        classify(&devs, &cfg)
    }
}

/// Pure decision from the per-box deviations. Scroll is tested first (anchored
/// text across many boxes all moving at once), then per-box change.
fn classify(devs: &[BoxDeviation], cfg: &MonitorConfig) -> FrameClassification {
    let judged = devs.iter().filter(|d| d.judged).count();
    let changed: Vec<u64> = devs
        .iter()
        .filter(|d| d.changed(cfg))
        .map(|d| d.id)
        .collect();
    // Many boxes changing together is the whole view moving (scroll / navigation),
    // not a per-pill edit.
    if changed.len() >= cfg.scroll_min_boxes
        && judged > 0
        && changed.len() as f32 >= cfg.scroll_frac * judged as f32
    {
        return FrameClassification::Scroll;
    }
    if changed.is_empty() {
        FrameClassification::Quiet
    } else {
        FrameClassification::BoxesChanged(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: u8 = 100; // resting background luma
    const HARD: u8 = 255; // a full swing vs BG (delta 155 > hard_threshold)

    fn rect(cx: f32, cy: f32, w: f32, h: f32) -> OrientedRect {
        OrientedRect {
            cx,
            cy,
            width: w,
            height: h,
            angle_radians: 0.0,
        }
    }

    /// Grayscale samples: every point `[v,v,v]`, with per-hole overrides.
    fn samples(len: usize, default: u8, overrides: &[(usize, u8)]) -> Vec<Rgb> {
        let mut v = vec![[default; 3]; len];
        for &(i, val) in overrides {
            v[i] = [val; 3];
        }
        v
    }

    fn cfg() -> MonitorConfig {
        MonitorConfig {
            warmup_frames: 4,
            hard_threshold: 110,
            // Between the 10% "video edge" (held) and a ~20% sparse-ink scroll (trips).
            hard_frac: 0.15,
            scroll_frac: 0.7,
            scroll_min_boxes: 2,
        }
    }

    fn warm(mon: &mut ScreenMonitor, clean: &[Rgb]) {
        for _ in 0..cfg().warmup_frames + 1 {
            mon.observe(clean);
        }
    }

    /// A frame (over a `BG` field) where `frac` of `holes` swing hard to `HARD`.
    fn swing(len: usize, holes: &[usize], frac: f32) -> Vec<Rgb> {
        let n = (holes.len() as f32 * frac).round() as usize;
        samples(
            len,
            BG,
            &holes[..n.min(holes.len())]
                .iter()
                .map(|&h| (h, HARD))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn lattice_grid_and_membership() {
        let lat = Lattice::build(100, 100, 10.0);
        assert_eq!(lat.len(), 100);
        let band = rect(50.0, 80.0, 80.0, 20.0);
        let holes = lat.holes_in_rect(&band);
        assert_eq!(holes.len(), 16);
    }

    #[test]
    fn holes_in_polygon_hugs_the_contour() {
        let lat = Lattice::build(100, 100, 10.0);
        // A diamond centred at (50,50): its bounding rect includes corners the polygon
        // excludes, so the polygon hole set is strictly smaller.
        let diamond = [50.0, 20.0, 80.0, 50.0, 50.0, 80.0, 20.0, 50.0];
        let poly = lat.holes_in_polygon(&diamond);
        let bbox = lat.holes_in_rect(&rect(50.0, 50.0, 60.0, 60.0));
        assert!(!poly.is_empty());
        assert!(poly.len() < bbox.len(), "polygon excludes the rect corners");
        for &h in &poly {
            let p = lat.points()[h];
            assert!((p.x - 50.0).abs() + (p.y - 50.0).abs() <= 30.5);
        }
    }

    #[test]
    fn static_and_moderate_drift_held() {
        let lat = Lattice::build(100, 100, 10.0);
        let holes = lat.holes_in_rect(&rect(50.0, 50.0, 80.0, 80.0));
        let mut mon = ScreenMonitor::new(lat, cfg());
        let len = mon.lattice().len();
        let clean = samples(len, BG, &[]);
        mon.set_box(1, holes, &clean);
        warm(&mut mon, &clean);
        // Unchanged.
        assert_eq!(mon.observe(&clean), FrameClassification::Quiet);
        // Uniform moderate shift (delta 50 < hard 110) — background brightening or
        // temporally-coherent video — never crosses the hard threshold → held.
        assert_eq!(
            mon.observe(&samples(len, 150, &[])),
            FrameClassification::Quiet,
        );
    }

    #[test]
    fn hard_swing_fraction_trips() {
        let lat = Lattice::build(100, 100, 10.0);
        let holes = lat.holes_in_rect(&rect(50.0, 50.0, 80.0, 80.0));
        let len = lat.len();
        let mut mon = ScreenMonitor::new(lat, cfg());
        let clean = samples(mon.lattice().len(), BG, &[]);
        mon.set_box(1, holes.clone(), &clean);
        warm(&mut mon, &clean);
        // A few hard holes (a moving video edge) — below the fraction → held.
        assert_eq!(
            mon.observe(&swing(len, &holes, 0.10)),
            FrameClassification::Quiet,
        );
        // Many holes swing hard (a scroll sweeping gap↔stroke) → trip.
        assert_eq!(
            mon.observe(&swing(len, &holes, 0.40)),
            FrameClassification::BoxesChanged(vec![1]),
        );
    }

    #[test]
    fn all_boxes_swinging_hard_is_a_scroll() {
        let lat = Lattice::build(100, 100, 10.0);
        let h1 = lat.holes_in_rect(&rect(30.0, 30.0, 40.0, 40.0));
        let h2 = lat.holes_in_rect(&rect(70.0, 70.0, 40.0, 40.0));
        let mut mon = ScreenMonitor::new(lat, cfg());
        let clean = samples(mon.lattice().len(), BG, &[]);
        mon.set_box(1, h1.clone(), &clean);
        mon.set_box(2, h2.clone(), &clean);
        warm(&mut mon, &clean);

        let mut f = clean.clone();
        for &h in &h1[..(h1.len() * 2 / 5)] {
            f[h] = [HARD; 3];
        }
        for &h in &h2[..(h2.len() * 2 / 5)] {
            f[h] = [HARD; 3];
        }
        assert_eq!(mon.observe(&f), FrameClassification::Scroll);
    }

    #[test]
    fn one_box_swinging_is_per_box_not_scroll() {
        let lat = Lattice::build(100, 100, 10.0);
        let h1 = lat.holes_in_rect(&rect(30.0, 30.0, 40.0, 40.0));
        let h2 = lat.holes_in_rect(&rect(70.0, 70.0, 40.0, 40.0));
        let mut mon = ScreenMonitor::new(lat, cfg());
        let clean = samples(mon.lattice().len(), BG, &[]);
        mon.set_box(1, h1.clone(), &clean);
        mon.set_box(2, h2.clone(), &clean);
        warm(&mut mon, &clean);

        let mut f = clean.clone();
        for &h in &h1[..(h1.len() * 2 / 5)] {
            f[h] = [HARD; 3];
        }
        assert_eq!(mon.observe(&f), FrameClassification::BoxesChanged(vec![1]));
    }

    #[test]
    fn re_acquire_resets_baseline() {
        let lat = Lattice::build(100, 100, 10.0);
        let holes = lat.holes_in_rect(&rect(50.0, 50.0, 80.0, 80.0));
        let len = lat.len();
        let mut mon = ScreenMonitor::new(lat, cfg());
        let clean_a = samples(len, BG, &[]);
        mon.set_box(1, holes.clone(), &clean_a);
        warm(&mut mon, &clean_a);

        // New content after re-OCR (half the holes now bright): re-baseline, then the
        // same frame reads as no-change.
        let clean_b = swing(len, &holes, 0.5);
        mon.set_box(1, holes, &clean_b);
        warm(&mut mon, &clean_b);
        assert_eq!(mon.observe(&clean_b), FrameClassification::Quiet);
    }

    #[test]
    fn black_on_white_scroll_trips() {
        // The sticky-label case: black-on-white. Most holes are white background and
        // never move; only the sparse ink strokes change on a scroll. With the single
        // RGB hard-fraction signal this works because `hard_frac` is low enough that
        // the ~20% ink crossing clears it — no ink/NCC heuristic needed.
        const WHITE: u8 = 255;
        const INK: u8 = 20;
        let lat = Lattice::build(100, 100, 10.0);
        let holes = lat.holes_in_rect(&rect(50.0, 50.0, 80.0, 80.0));
        let len = lat.len();
        let ink_n = holes.len() / 5; // ~20% ink
        let inked: Vec<(usize, u8)> = holes[..ink_n].iter().map(|&h| (h, INK)).collect();
        let baseline = samples(len, WHITE, &inked);
        let mut mon = ScreenMonitor::new(lat, cfg());
        mon.set_box(1, holes.clone(), &baseline);
        warm(&mut mon, &baseline);

        // Static page: ink holds in place → held.
        assert_eq!(mon.observe(&baseline), FrameClassification::Quiet);

        // Scroll: the strokes move on, so the old ink holes read white now → the ~20%
        // that swing hard clears the (low) hard fraction.
        let scrolled = samples(len, WHITE, &[]);
        assert_eq!(
            mon.observe(&scrolled),
            FrameClassification::BoxesChanged(vec![1]),
        );
    }

    #[test]
    fn isoluminant_colour_change_trips() {
        // Content scrolls onto a different colour at ~the same brightness: a luma-only
        // delta would be ~0 (blind), but the per-channel RGB delta is large.
        let lat = Lattice::build(100, 100, 10.0);
        let holes = lat.holes_in_rect(&rect(50.0, 50.0, 80.0, 80.0));
        let len = lat.len();
        // hard_threshold 60 so the Δ70 colour swing counts; the point is the channel
        // delta, not the magnitude.
        let cfg = MonitorConfig {
            hard_threshold: 60,
            ..cfg()
        };
        let base = vec![[100u8, 100, 100]; len];
        let mut mon = ScreenMonitor::new(lat, cfg);
        mon.set_box(1, holes.clone(), &base);
        for _ in 0..cfg.warmup_frames + 1 {
            mon.observe(&base);
        }
        // [30,150,70]: luma ≈ 105 (Δ5 — a luma monitor shrugs), channel Δ = 70.
        let recoloured = vec![[30u8, 150, 70]; len];
        assert_eq!(
            mon.observe(&recoloured),
            FrameClassification::BoxesChanged(vec![1]),
            "an isoluminant colour change should trip on the per-channel RGB delta",
        );
    }
}
