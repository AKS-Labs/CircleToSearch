//! Edge-snap refinement for the coarse quad returned by the doc-align model.
//!
//! The ML model gives a region roughly bounding the document, but its corners drift by tens of
//! pixels around the true paper/sign edges — small perturbations in the input image (zoom, pan)
//! shift its output noticeably. That noise hurts perspective rectification because the homography
//! is fully determined by four corners; even a few pixels of corner error shears the warped
//! output enough to degrade OCR.
//!
//! This module post-processes the model quad by snapping each side to the nearest strong image
//! gradient running parallel to that side, then re-intersecting the four refined lines.
//!
//! Pipeline per side:
//!   1. Sample N points along the model-predicted side.
//!   2. At each sample, search ±band perpendicular to the side for the pixel whose Sobel gradient
//!      projects most strongly onto the side's normal direction. That pixel is a candidate
//!      "true edge" point.
//!   3. RANSAC line fit through the candidates (rejecting outliers from clutter, occluders).
//!   4. If the RANSAC line has too few inliers, keep the model's side instead.
//!
//! Then intersect adjacent refined lines to recover corners. Final sanity check: if any corner
//! moved by more than `MAX_CORNER_DELTA_FRAC` of the image diagonal, the refinement is suspect
//! and we fall back to the model quad untouched.

use crate::doc_align::{DocumentPoint, DocumentQuad};

const N_SAMPLES_PER_SIDE: usize = 80;
const BAND_FRAC_OF_DIAG: f32 = 0.025;
const RANSAC_ITERS: usize = 80;
const RANSAC_INLIER_THRESH_PX: f32 = 2.0;
const MIN_INLIERS: usize = 12;
const GRAD_MIN_MAGNITUDE: f32 = 30.0;
const MAX_CORNER_DELTA_FRAC: f32 = 0.08;

/// Coverage analysis: split each side into N_COVERAGE_BUCKETS segments along its length, and
/// require ≥MIN_CANDIDATES_PER_BUCKET candidate edge points in at least MIN_SUPPORTED_BUCKETS
/// of them for the side to count as "supported." This catches the failure mode where the model
/// predicts a side that lies on a real edge for only one segment (the rest extending through
/// unrelated scene content) — total inlier count alone passes that case, coverage rejects it.
const N_COVERAGE_BUCKETS: usize = 3;
const MIN_CANDIDATES_PER_BUCKET: usize = 3;
const MIN_SUPPORTED_BUCKETS: usize = 2;

/// Quality of the refined quad — how much of the model's predicted boundary actually lies on
/// real image edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum QuadQuality {
    /// All 4 sides have edge support spread along their length. The refined quad is
    /// trustworthy.
    Good,
    /// 3 of 4 sides are supported; one is occluded or low-contrast. Usually still close, but
    /// callers may want to surface "double-check this" to the user.
    Weak,
    /// 2 or fewer sides have spread edge support. The model's quad does not trace real image
    /// edges — likely a hallucination. Callers should typically suppress the pre-fill and let
    /// the user draw the quad manually.
    Bad,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RefinementResult {
    pub quad: DocumentQuad,
    pub quality: QuadQuality,
    /// How many of the 4 model-predicted sides had edge support spread across ≥2 of 3 segments.
    pub supported_sides: u8,
}

/// Refine `model_quad` and report quality. See [`RefinementResult`] for the meaning of
/// `quality` / `supported_sides`. The returned `quad` is the refined quad (or the model quad
/// if refinement was rejected because it would have moved corners too far); use `quality` to
/// decide whether to use it at all.
pub fn refine_quad_with_quality(
    rgba: &[u8],
    width: u32,
    height: u32,
    model_quad: &DocumentQuad,
) -> RefinementResult {
    let fallback = |q: DocumentQuad, supported: u8| RefinementResult {
        quad: q,
        quality: classify(supported),
        supported_sides: supported,
    };

    if width < 4 || height < 4 {
        return fallback(model_quad.clone(), 0);
    }
    let expected = (width as usize) * (height as usize) * 4;
    if rgba.len() != expected {
        return fallback(model_quad.clone(), 0);
    }

    let gray = rgba_to_gray(rgba, width, height);
    let (gx, gy) = sobel(&gray, width as usize, height as usize);

    let diag = ((width as f32).powi(2) + (height as f32).powi(2)).sqrt();
    let band = (diag * BAND_FRAC_OF_DIAG).round().max(6.0) as i32;

    let corners = model_quad.corners();
    // Sides in (start, end) corner-index order, matching the corner array
    // [TL, TR, BR, BL]: top TL→TR, right TR→BR, bottom BR→BL, left BL→TL.
    let side_pairs = [(0, 1), (1, 2), (2, 3), (3, 0)];

    let mut refined_sides = [Line::ZERO; 4];
    let mut supported_sides: u8 = 0;
    for (i, (a_idx, b_idx)) in side_pairs.iter().enumerate() {
        let a = corners[*a_idx];
        let b = corners[*b_idx];
        let baseline = Line::through_two_points(a, b);
        let candidates = collect_edge_candidates(&gx, &gy, width, height, a, b, band);
        if side_is_supported(&candidates, a, b) {
            supported_sides += 1;
        }
        let refined = if candidates.len() >= MIN_INLIERS {
            ransac_line_fit(
                &candidates,
                &baseline,
                RANSAC_INLIER_THRESH_PX,
                RANSAC_ITERS,
            )
            .unwrap_or(baseline)
        } else {
            baseline
        };
        refined_sides[i] = refined;
    }

    // Intersect adjacent refined sides to recover corners.
    let Some(tl) = intersect(&refined_sides[3], &refined_sides[0]) else {
        return fallback(model_quad.clone(), supported_sides);
    };
    let Some(tr) = intersect(&refined_sides[0], &refined_sides[1]) else {
        return fallback(model_quad.clone(), supported_sides);
    };
    let Some(br) = intersect(&refined_sides[1], &refined_sides[2]) else {
        return fallback(model_quad.clone(), supported_sides);
    };
    let Some(bl) = intersect(&refined_sides[2], &refined_sides[3]) else {
        return fallback(model_quad.clone(), supported_sides);
    };

    let refined_corners = [tl, tr, br, bl];
    let max_delta = diag * MAX_CORNER_DELTA_FRAC;
    for i in 0..4 {
        let dx = refined_corners[i].x - corners[i].x;
        let dy = refined_corners[i].y - corners[i].y;
        if (dx * dx + dy * dy).sqrt() > max_delta {
            return fallback(model_quad.clone(), supported_sides);
        }
    }

    fallback(DocumentQuad::from_corners(refined_corners), supported_sides)
}

/// Backwards-compatible thin wrapper: refine and return just the quad, discarding quality.
/// Most callers should use [`refine_quad_with_quality`] so they can suppress the pre-fill on
/// `QuadQuality::Bad`.
pub fn refine_quad(
    rgba: &[u8],
    width: u32,
    height: u32,
    model_quad: &DocumentQuad,
) -> DocumentQuad {
    refine_quad_with_quality(rgba, width, height, model_quad).quad
}

fn classify(supported_sides: u8) -> QuadQuality {
    match supported_sides {
        4 => QuadQuality::Good,
        3 => QuadQuality::Weak,
        _ => QuadQuality::Bad,
    }
}

/// A side is "supported" when candidate edge points exist along most of its length, not just
/// bunched at one end. Project each candidate onto the side's tangent direction to get its
/// fractional position along the side, bucket into `N_COVERAGE_BUCKETS` segments, and require
/// ≥`MIN_CANDIDATES_PER_BUCKET` candidates in ≥`MIN_SUPPORTED_BUCKETS` buckets.
fn side_is_supported(candidates: &[(f32, f32)], a: DocumentPoint, b: DocumentPoint) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1.0 {
        return false;
    }
    let mut bucket_counts = [0usize; N_COVERAGE_BUCKETS];
    for (px, py) in candidates {
        // Fractional position along the side: t = ((p - a) · (b - a)) / |b - a|²
        let t = ((px - a.x) * dx + (py - a.y) * dy) / len_sq;
        if !(0.0..=1.0).contains(&t) {
            continue;
        }
        let idx = ((t * N_COVERAGE_BUCKETS as f32) as usize).min(N_COVERAGE_BUCKETS - 1);
        bucket_counts[idx] += 1;
    }
    let populated = bucket_counts
        .iter()
        .filter(|&&c| c >= MIN_CANDIDATES_PER_BUCKET)
        .count();
    populated >= MIN_SUPPORTED_BUCKETS
}

fn rgba_to_gray(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let n = (width as usize) * (height as usize);
    let mut gray = vec![0u8; n];
    for i in 0..n {
        let r = rgba[i * 4] as u32;
        let g = rgba[i * 4 + 1] as u32;
        let b = rgba[i * 4 + 2] as u32;
        // BT.601 luma; integer-rounded.
        gray[i] = ((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8;
    }
    gray
}

fn sobel(gray: &[u8], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let mut gx = vec![0.0f32; w * h];
    let mut gy = vec![0.0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let tl = gray[(y - 1) * w + x - 1] as f32;
            let tt = gray[(y - 1) * w + x] as f32;
            let tr = gray[(y - 1) * w + x + 1] as f32;
            let ll = gray[y * w + x - 1] as f32;
            let rr = gray[y * w + x + 1] as f32;
            let bl = gray[(y + 1) * w + x - 1] as f32;
            let bb = gray[(y + 1) * w + x] as f32;
            let br = gray[(y + 1) * w + x + 1] as f32;
            gx[y * w + x] = -tl - 2.0 * ll - bl + tr + 2.0 * rr + br;
            gy[y * w + x] = -tl - 2.0 * tt - tr + bl + 2.0 * bb + br;
        }
    }
    (gx, gy)
}

/// For each of `N_SAMPLES_PER_SIDE` points along segment a→b, search ±band along the side's
/// normal direction for the pixel whose gradient projects most strongly onto that normal.
/// Candidates with magnitude below `GRAD_MIN_MAGNITUDE` are dropped.
fn collect_edge_candidates(
    gx: &[f32],
    gy: &[f32],
    width: u32,
    height: u32,
    a: DocumentPoint,
    b: DocumentPoint,
    band: i32,
) -> Vec<(f32, f32)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return Vec::new();
    }
    let tx = dx / len;
    let ty = dy / len;
    // Normal is perpendicular to (tx, ty); sign doesn't matter (we use abs of projection).
    let nx = -ty;
    let ny = tx;

    let w = width as i32;
    let h = height as i32;
    let stride = width as usize;

    let mut out = Vec::with_capacity(N_SAMPLES_PER_SIDE);
    for i in 0..N_SAMPLES_PER_SIDE {
        let t = (i as f32 + 0.5) / N_SAMPLES_PER_SIDE as f32;
        let cx = a.x + t * dx;
        let cy = a.y + t * dy;

        let mut best_score = GRAD_MIN_MAGNITUDE;
        let mut best_point: Option<(f32, f32)> = None;
        for s in -band..=band {
            let s_f = s as f32;
            let px = cx + s_f * nx;
            let py = cy + s_f * ny;
            let ix = px.round() as i32;
            let iy = py.round() as i32;
            if ix < 1 || iy < 1 || ix >= w - 1 || iy >= h - 1 {
                continue;
            }
            let idx = (iy as usize) * stride + ix as usize;
            let proj = (gx[idx] * nx + gy[idx] * ny).abs();
            if proj > best_score {
                best_score = proj;
                best_point = Some((px, py));
            }
        }
        if let Some(p) = best_point {
            out.push(p);
        }
    }
    out
}

/// Implicit-form line: a·x + b·y + c = 0, with a² + b² = 1 (so distance(p) = a*x + b*y + c is
/// signed perpendicular distance).
#[derive(Clone, Copy, Debug)]
struct Line {
    a: f32,
    b: f32,
    c: f32,
}

impl Line {
    const ZERO: Line = Line {
        a: 1.0,
        b: 0.0,
        c: 0.0,
    };

    fn through_two_points(p1: DocumentPoint, p2: DocumentPoint) -> Self {
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        let len = (dx * dx + dy * dy).sqrt().max(1e-6);
        let a = -dy / len;
        let b = dx / len;
        let c = -(a * p1.x + b * p1.y);
        Line { a, b, c }
    }

    fn distance(&self, p: (f32, f32)) -> f32 {
        self.a * p.0 + self.b * p.1 + self.c
    }
}

/// RANSAC line fit. `baseline` is the model-predicted side; we use it to break ties and reject
/// hypotheses that flip orientation (a line orthogonal to the side is never the right answer
/// even if it has many edge inliers).
fn ransac_line_fit(
    pts: &[(f32, f32)],
    baseline: &Line,
    inlier_thresh: f32,
    iters: usize,
) -> Option<Line> {
    if pts.len() < 2 {
        return None;
    }
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut best_count = 0usize;
    let mut best_inliers: Vec<(f32, f32)> = Vec::new();
    let n = pts.len();
    for _ in 0..iters {
        let i1 = (next_rand(&mut state) as usize) % n;
        let mut i2 = (next_rand(&mut state) as usize) % n;
        if i2 == i1 {
            i2 = (i2 + 1) % n;
        }
        let p1 = DocumentPoint {
            x: pts[i1].0,
            y: pts[i1].1,
        };
        let p2 = DocumentPoint {
            x: pts[i2].0,
            y: pts[i2].1,
        };
        let cand = Line::through_two_points(p1, p2);
        // Reject hypotheses that point in a different direction than the model side. dot of the
        // two normals: |a1*a2 + b1*b2| close to 1 means parallel/anti-parallel, close to 0 means
        // perpendicular.
        let parallelism = (cand.a * baseline.a + cand.b * baseline.b).abs();
        if parallelism < 0.94 {
            // ~20° tolerance (cos 20° ≈ 0.94)
            continue;
        }
        let mut inliers: Vec<(f32, f32)> = Vec::new();
        for p in pts {
            if cand.distance(*p).abs() <= inlier_thresh {
                inliers.push(*p);
            }
        }
        if inliers.len() > best_count {
            best_count = inliers.len();
            best_inliers = inliers;
        }
    }
    if best_count < MIN_INLIERS {
        return None;
    }
    Some(total_least_squares(&best_inliers))
}

/// Refit a line through `pts` by total least squares (PCA on the centered points). The line
/// passes through the centroid and is oriented along the principal eigenvector of the
/// covariance matrix.
fn total_least_squares(pts: &[(f32, f32)]) -> Line {
    let n = pts.len() as f32;
    let mean_x = pts.iter().map(|p| p.0).sum::<f32>() / n;
    let mean_y = pts.iter().map(|p| p.1).sum::<f32>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0f32, 0.0f32, 0.0f32);
    for p in pts {
        let dx = p.0 - mean_x;
        let dy = p.1 - mean_y;
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let trace = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let disc = (trace * trace * 0.25 - det).max(0.0).sqrt();
    let small_eigen = trace * 0.5 - disc;
    // Eigenvector for small eigenvalue is the line normal: solve [[sxx-λ, sxy],[sxy, syy-λ]] v = 0.
    let (nx, ny) = if sxy.abs() > 1e-6 {
        let vx = sxy;
        let vy = small_eigen - sxx;
        let m = (vx * vx + vy * vy).sqrt().max(1e-9);
        (vx / m, vy / m)
    } else if sxx > syy {
        (0.0, 1.0)
    } else {
        (1.0, 0.0)
    };
    let c = -(nx * mean_x + ny * mean_y);
    Line { a: nx, b: ny, c }
}

fn intersect(l1: &Line, l2: &Line) -> Option<DocumentPoint> {
    let det = l1.a * l2.b - l2.a * l1.b;
    if det.abs() < 1e-6 {
        return None;
    }
    let x = (l1.b * l2.c - l2.b * l1.c) / det;
    let y = (l2.a * l1.c - l1.a * l2.c) / det;
    Some(DocumentPoint { x, y })
}

fn next_rand(state: &mut u64) -> u32 {
    // xorshift64*
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refine_quad_snaps_to_synthetic_rectangle() {
        // 200x200 white image with a black-bordered rectangle from (40,40) to (160,160).
        // Model quad is offset by +6 pixels on each corner — refinement should snap it back.
        let w = 200u32;
        let h = 200u32;
        let mut rgba = vec![255u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let on_top = y == 40 && (40..=160).contains(&x);
                let on_bot = y == 160 && (40..=160).contains(&x);
                let on_left = x == 40 && (40..=160).contains(&y);
                let on_right = x == 160 && (40..=160).contains(&y);
                if on_top || on_bot || on_left || on_right {
                    let i = ((y * w + x) * 4) as usize;
                    rgba[i] = 0;
                    rgba[i + 1] = 0;
                    rgba[i + 2] = 0;
                }
            }
        }
        let model = DocumentQuad::from_corners([
            DocumentPoint { x: 34.0, y: 34.0 },
            DocumentPoint { x: 166.0, y: 34.0 },
            DocumentPoint { x: 166.0, y: 166.0 },
            DocumentPoint { x: 34.0, y: 166.0 },
        ]);
        let refined = refine_quad(&rgba, w, h, &model);
        let r = refined.corners();
        // Should snap within 2 pixels of the true rectangle (40, 160).
        assert!(
            (r[0].x - 40.0).abs() <= 2.0 && (r[0].y - 40.0).abs() <= 2.0,
            "TL drifted: {:?}",
            r[0]
        );
        assert!(
            (r[1].x - 160.0).abs() <= 2.0 && (r[1].y - 40.0).abs() <= 2.0,
            "TR drifted: {:?}",
            r[1]
        );
        assert!(
            (r[2].x - 160.0).abs() <= 2.0 && (r[2].y - 160.0).abs() <= 2.0,
            "BR drifted: {:?}",
            r[2]
        );
        assert!(
            (r[3].x - 40.0).abs() <= 2.0 && (r[3].y - 160.0).abs() <= 2.0,
            "BL drifted: {:?}",
            r[3]
        );
    }

    #[test]
    fn refine_quad_falls_back_when_no_edges_present() {
        // Uniform image: no gradient anywhere. Refinement must return the model quad untouched
        // and report Bad quality (no side has edge support).
        let w = 100u32;
        let h = 100u32;
        let rgba = vec![200u8; (w * h * 4) as usize];
        let model = DocumentQuad::from_corners([
            DocumentPoint { x: 10.0, y: 10.0 },
            DocumentPoint { x: 90.0, y: 10.0 },
            DocumentPoint { x: 90.0, y: 90.0 },
            DocumentPoint { x: 10.0, y: 90.0 },
        ]);
        let result = refine_quad_with_quality(&rgba, w, h, &model);
        assert_eq!(result.quad, model);
        assert_eq!(result.supported_sides, 0);
        assert_eq!(result.quality, QuadQuality::Bad);
    }

    #[test]
    fn refine_quad_reports_good_quality_when_all_sides_have_edges() {
        // Same synthetic rectangle as the snap test — all 4 sides have a strong edge running
        // their full length, so all 4 sides should count as supported.
        let w = 200u32;
        let h = 200u32;
        let mut rgba = vec![255u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let on_top = y == 40 && (40..=160).contains(&x);
                let on_bot = y == 160 && (40..=160).contains(&x);
                let on_left = x == 40 && (40..=160).contains(&y);
                let on_right = x == 160 && (40..=160).contains(&y);
                if on_top || on_bot || on_left || on_right {
                    let i = ((y * w + x) * 4) as usize;
                    rgba[i] = 0;
                    rgba[i + 1] = 0;
                    rgba[i + 2] = 0;
                }
            }
        }
        let model = DocumentQuad::from_corners([
            DocumentPoint { x: 34.0, y: 34.0 },
            DocumentPoint { x: 166.0, y: 34.0 },
            DocumentPoint { x: 166.0, y: 166.0 },
            DocumentPoint { x: 34.0, y: 166.0 },
        ]);
        let result = refine_quad_with_quality(&rgba, w, h, &model);
        assert_eq!(result.supported_sides, 4);
        assert_eq!(result.quality, QuadQuality::Good);
    }

    #[test]
    fn refine_quad_reports_bad_when_only_one_side_supported() {
        // Image with a single horizontal black line across the top — only the top side of the
        // quad has any edge to snap to; left/right/bottom run through uniform white.
        let w = 200u32;
        let h = 200u32;
        let mut rgba = vec![255u8; (w * h * 4) as usize];
        for x in 0..w {
            let i = ((40 * w + x) * 4) as usize;
            rgba[i] = 0;
            rgba[i + 1] = 0;
            rgba[i + 2] = 0;
        }
        let model = DocumentQuad::from_corners([
            DocumentPoint { x: 20.0, y: 38.0 },
            DocumentPoint { x: 180.0, y: 38.0 },
            DocumentPoint { x: 180.0, y: 180.0 },
            DocumentPoint { x: 20.0, y: 180.0 },
        ]);
        let result = refine_quad_with_quality(&rgba, w, h, &model);
        assert!(
            result.supported_sides <= 1,
            "expected ≤1 supported sides, got {}",
            result.supported_sides
        );
        assert_eq!(result.quality, QuadQuality::Bad);
    }
}
