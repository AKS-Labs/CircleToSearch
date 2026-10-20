//! Sparse pyramidal Lucas-Kanade tracker.
//!
//! Given the previous frame's grayscale image, the current frame's
//! grayscale image, and a set of points in previous-frame coordinates,
//! returns the displaced positions in current-frame coordinates with a
//! per-point success flag and final residual.
//!
//! Used by the planar engine to propagate the previous frame's RANSAC
//! inliers across frames as sub-pixel, same-physical-point
//! correspondences. The output is appended to the descriptor-matched
//! correspondences before RANSAC, raising the inlier floor when the
//! descriptor matcher is starved by blur or low texture.
//!
//! Algorithm: classic Bouguet pyramidal LK with the inverse-compositional
//! parameterisation:
//!   - Build a 3-level pyramid for both frames (downsample by 2 each level).
//!   - For each point, iterate coarse → fine. At each level, the
//!     Hessian and gradient image are computed once over the prev-frame
//!     window (per-level constants); each iteration only re-samples the
//!     warped cur-frame window and solves a 2×2 system.
//!   - Sub-pixel sampling via bilinear interpolation.
//!   - Reject tracks whose final residual exceeds a threshold or whose
//!     2×2 Hessian is degenerate.

use image::GrayImage;
use rayon::prelude::*;

/// Default pyramid depth used by the engine. Each level halves the
/// resolution; the coarse-to-fine search extends KLT's tractable
/// frame-to-frame displacement to roughly `window_radius × 2^(N-1)` px
/// in L0 coordinates. 4 levels with r=5 → ~40 px ceiling, matched to
/// the per-frame motion seen on fast-pan clips (park, gintonic).
pub const DEFAULT_PYRAMID_LEVELS: usize = 4;

/// One pyramid level + the level's downsample factor relative to L0.
#[derive(Clone)]
pub struct Pyramid {
    pub levels: Vec<GrayImage>,
}

impl Pyramid {
    pub fn build(base: &GrayImage, num_levels: usize) -> Self {
        let mut levels = Vec::with_capacity(num_levels);
        levels.push(base.clone());
        for _ in 1..num_levels {
            let prev = levels.last().expect("pyramid base");
            levels.push(downsample_2x(prev));
        }
        Self { levels }
    }
}

/// Half-resolution downsample using a 2×2 box filter. Cheap, decent
/// anti-aliasing for LK's purposes.
fn downsample_2x(src: &GrayImage) -> GrayImage {
    let w = src.width() / 2;
    let h = src.height() / 2;
    let mut out = GrayImage::new(w.max(1), h.max(1));
    if w == 0 || h == 0 {
        return out;
    }
    let src_raw = src.as_raw();
    let src_w = src.width() as usize;
    let dst_raw = out.as_mut();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let sx = x * 2;
            let sy = y * 2;
            let i0 = sy * src_w + sx;
            let i1 = i0 + 1;
            let i2 = i0 + src_w;
            let i3 = i2 + 1;
            let v = (src_raw[i0] as u32
                + src_raw[i1] as u32
                + src_raw[i2] as u32
                + src_raw[i3] as u32
                + 2)
                / 4;
            dst_raw[y * w as usize + x] = v as u8;
        }
    }
    out
}

/// `true` when a `(2·margin)`-wide window centered at `(cx, cy)` fits inside
/// `img` with room for the bilinear-interpolation reads inside `sample()`.
/// Used by `track_one` to skip points whose LK window would silently include
/// out-of-bounds zero samples.
#[inline]
fn level_window_in_bounds(img: &GrayImage, cx: f32, cy: f32, margin: f32) -> bool {
    let w = img.width() as f32;
    let h = img.height() as f32;
    // `sample()` requires `floor(x) + 1 < w`, i.e. `x < w - 1`. We want the
    // leftmost touched pixel (`cx - margin`) to be ≥ 0 and the rightmost
    // (`cx + margin`) to satisfy that bound.
    cx >= margin && cy >= margin && cx + margin < w - 1.0 && cy + margin < h - 1.0
}

/// Sub-pixel bilinear sample. Returns 0 for out-of-bounds; callers
/// gate on the half-window border before invoking, so OOB sampling
/// only happens at literal image edges.
#[inline]
fn sample(img: &GrayImage, x: f32, y: f32) -> f32 {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    if x0 < 0 || y0 < 0 || x1 >= w || y1 >= h {
        return 0.0;
    }
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let raw = img.as_raw();
    let stride = w as usize;
    let i00 = (y0 as usize) * stride + (x0 as usize);
    let v00 = raw[i00] as f32;
    let v10 = raw[i00 + 1] as f32;
    let v01 = raw[i00 + stride] as f32;
    let v11 = raw[i00 + stride + 1] as f32;
    let a = v00 * (1.0 - fx) + v10 * fx;
    let b = v01 * (1.0 - fx) + v11 * fx;
    a * (1.0 - fy) + b * fy
}

/// LK tuning. Defaults are conservative.
#[derive(Clone, Copy, Debug)]
pub struct KltConfig {
    pub num_levels: usize,
    pub window_radius: i32,
    pub max_iterations: usize,
    pub convergence_epsilon: f32,
    pub max_residual: f32,
    pub min_eigenvalue: f32,
    /// Forward-backward consistency tolerance in L0 pixels. After tracking
    /// each point prev→cur, the track is re-run cur→prev from the forward
    /// destination; if the round-trip endpoint differs from the original
    /// start by more than this distance, the track is rejected. Catches
    /// wrong-basin matches on repetitive content (text columns, brick
    /// patterns, etc.) that the structure-tensor min-eigenvalue and
    /// max-residual gates pass. Set to `0.0` to disable.
    pub fb_check_eps: f32,
}

impl Default for KltConfig {
    fn default() -> Self {
        Self {
            num_levels: DEFAULT_PYRAMID_LEVELS,
            window_radius: 5,
            max_iterations: 8,
            convergence_epsilon: 0.03,
            max_residual: 12.0,
            min_eigenvalue: 1.0,
            // 0.5 px round-trip error — bad tracks (wrong-basin) typically
            // drift more than 1 px on the reverse pass, healthy ones stay
            // well under a tenth. The cost is doubling per-point LK work
            // on forward-successes only, which on ~80 seeds is sub-ms.
            fb_check_eps: 0.5,
        }
    }
}

/// Per-point output. `success` false means the track was rejected and
/// `(x, y)` should be ignored.
#[derive(Clone, Copy, Debug)]
pub struct TrackOut {
    pub x: f32,
    pub y: f32,
    pub residual: f32,
    pub success: bool,
}

/// Track points from `prev` to `cur`. `points` are in L0 (full-res)
/// coordinates of `prev`. Returns one entry per input point.
pub fn track_points(
    prev: &Pyramid,
    cur: &Pyramid,
    points: &[(f32, f32)],
    cfg: &KltConfig,
) -> Vec<TrackOut> {
    debug_assert_eq!(prev.levels.len(), cur.levels.len());
    let num_levels = prev.levels.len();
    // Per-point pyramidal LK solves are independent and read-only over the
    // pyramids, so fan them out over the active (tracker) pool. `map`+`collect`
    // preserves input order, which the caller relies on to zip back to its
    // inlier pairs. The whole engine step runs inside `tracker_pool.install`,
    // so this uses those threads rather than the global pool.
    let forward: Vec<TrackOut> = points
        .par_iter()
        .map(|&(px, py)| track_one(prev, cur, px, py, num_levels, cfg))
        .collect();

    if cfg.fb_check_eps <= 0.0 {
        return forward;
    }

    // Forward-backward consistency. For each forward-successful track,
    // re-track its destination cur→prev; drop if the round-trip endpoint
    // misses the original start by more than `fb_check_eps`. Catches
    // wrong-basin matches that the per-track min-eigenvalue and max-
    // residual gates accept (common on repetitive text patterns where a
    // window can find a self-consistent match a few characters over).
    let fb_eps_sq = cfg.fb_check_eps * cfg.fb_check_eps;
    forward
        .into_par_iter()
        .zip(points.par_iter())
        .map(|(fwd, &(orig_x, orig_y))| {
            if !fwd.success {
                return fwd;
            }
            let bwd = track_one(cur, prev, fwd.x, fwd.y, num_levels, cfg);
            if !bwd.success {
                return TrackOut {
                    success: false,
                    ..fwd
                };
            }
            let dx = bwd.x - orig_x;
            let dy = bwd.y - orig_y;
            if dx * dx + dy * dy > fb_eps_sq {
                TrackOut {
                    success: false,
                    ..fwd
                }
            } else {
                fwd
            }
        })
        .collect()
}

fn track_one(
    prev: &Pyramid,
    cur: &Pyramid,
    px: f32,
    py: f32,
    num_levels: usize,
    cfg: &KltConfig,
) -> TrackOut {
    let mut d_x = 0.0_f32;
    let mut d_y = 0.0_f32;
    let mut final_residual = f32::INFINITY;
    let mut success = false;

    for level in (0..num_levels).rev() {
        let scale = (1 << level) as f32;
        let prev_l = &prev.levels[level];
        let cur_l = &cur.levels[level];
        let px_l = px / scale;
        let py_l = py / scale;
        let r = cfg.window_radius;

        // Reject points whose prev window leaves the image. The structure-
        // tensor loop below samples `sample(prev, sx ± 1, sy ± 1)` for the
        // central differences, so the tightest pixel touched is at
        // `±(r + 1)` from the center, and `sample()` itself needs
        // `floor(x) + 1 < w`. Without this gate, OOB reads silently return
        // 0 from `sample()` and bias the structure tensor / residual toward
        // a low-eigenvalue direction.
        if !level_window_in_bounds(prev_l, px_l, py_l, r as f32 + 1.0) {
            return TrackOut {
                x: px,
                y: py,
                residual: f32::INFINITY,
                success: false,
            };
        }

        let mut gxx = 0.0_f32;
        let mut gxy = 0.0_f32;
        let mut gyy = 0.0_f32;
        let mut grads = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
        let mut prev_vals = Vec::with_capacity(grads.capacity());
        for dy in -r..=r {
            for dx in -r..=r {
                let sx = px_l + dx as f32;
                let sy = py_l + dy as f32;
                let ix = (sample(prev_l, sx + 1.0, sy) - sample(prev_l, sx - 1.0, sy)) * 0.5;
                let iy = (sample(prev_l, sx, sy + 1.0) - sample(prev_l, sx, sy - 1.0)) * 0.5;
                let v = sample(prev_l, sx, sy);
                gxx += ix * ix;
                gxy += ix * iy;
                gyy += iy * iy;
                grads.push((ix, iy));
                prev_vals.push(v);
            }
        }
        let det = gxx * gyy - gxy * gxy;
        let trace = gxx + gyy;
        let disc_sq = (trace * trace - 4.0 * det).max(0.0);
        let min_eig = 0.5 * (trace - disc_sq.sqrt());
        if min_eig < cfg.min_eigenvalue || !det.is_finite() {
            return TrackOut {
                x: px,
                y: py,
                residual: f32::INFINITY,
                success: false,
            };
        }

        let inv_det = 1.0 / det;
        for _ in 0..cfg.max_iterations {
            // Reject if the current-frame window (centered at
            // `(px_l + d_x, py_l + d_y)`) has drifted outside the image
            // during the LK iteration. No central-difference margin here —
            // cur is only sampled, not differentiated — but `sample()` still
            // needs `floor(x) + 1 < w`, hence the bare `r` margin.
            if !level_window_in_bounds(cur_l, px_l + d_x, py_l + d_y, r as f32) {
                return TrackOut {
                    x: px,
                    y: py,
                    residual: f32::INFINITY,
                    success: false,
                };
            }
            let mut bx = 0.0_f32;
            let mut by = 0.0_f32;
            let mut resid_acc = 0.0_f32;
            let mut idx = 0usize;
            for dy in -r..=r {
                for dx in -r..=r {
                    let sx_cur = px_l + d_x + dx as f32;
                    let sy_cur = py_l + d_y + dy as f32;
                    let cur_v = sample(cur_l, sx_cur, sy_cur);
                    let it = cur_v - prev_vals[idx];
                    let (ix, iy) = grads[idx];
                    bx += ix * it;
                    by += iy * it;
                    resid_acc += it * it;
                    idx += 1;
                }
            }
            let dx_step = (gyy * bx - gxy * by) * inv_det;
            let dy_step = (-gxy * bx + gxx * by) * inv_det;
            d_x -= dx_step;
            d_y -= dy_step;
            let n = grads.len() as f32;
            final_residual = (resid_acc / n).sqrt();
            if dx_step.abs() < cfg.convergence_epsilon && dy_step.abs() < cfg.convergence_epsilon {
                break;
            }
        }

        if level > 0 {
            d_x *= 2.0;
            d_y *= 2.0;
        } else {
            success = final_residual <= cfg.max_residual;
        }
    }

    TrackOut {
        x: px + d_x,
        y: py + d_y,
        residual: final_residual,
        success,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GrayImage;

    fn checker(w: u32, h: u32, sx: u32, sy: u32) -> GrayImage {
        let mut img = GrayImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = if ((x.wrapping_add(sx) / 8) + (y.wrapping_add(sy) / 8)) % 2 == 0 {
                    220
                } else {
                    40
                };
                img.put_pixel(x, y, image::Luma([v]));
            }
        }
        img
    }

    #[test]
    fn klt_tracks_pure_translation() {
        // Wider patches so each point falls inside a clear local
        // intensity gradient (no aperture-collapse / fence ambiguity).
        let prev = checker(128, 128, 0, 0);
        let cur = checker(128, 128, 3, 2);
        let p_prev = Pyramid::build(&prev, 3);
        let p_cur = Pyramid::build(&cur, 3);
        // Sample multiple points on grid junctions where both Ix and Iy
        // have signal. Average displacement should be ~(-3, -2).
        let pts: Vec<(f32, f32)> = (1..=4)
            .flat_map(|i| (1..=4).map(move |j| (i as f32 * 16.0 + 4.0, j as f32 * 16.0 + 4.0)))
            .collect();
        let cfg = KltConfig::default();
        let out = track_points(&p_prev, &p_cur, &pts, &cfg);
        let ok: Vec<&TrackOut> = out.iter().filter(|o| o.success).collect();
        assert!(
            ok.len() >= pts.len() / 2,
            "too few successful tracks: {}/{}",
            ok.len(),
            pts.len()
        );
        let mean_dx = ok
            .iter()
            .map(|o| o.x - pts[out.iter().position(|x| std::ptr::eq(x, *o)).unwrap()].0)
            .sum::<f32>()
            / ok.len() as f32;
        let mean_dy = ok
            .iter()
            .map(|o| o.y - pts[out.iter().position(|x| std::ptr::eq(x, *o)).unwrap()].1)
            .sum::<f32>()
            / ok.len() as f32;
        assert!(
            (mean_dx - (-3.0)).abs() < 1.0,
            "dx mean {} not ~ -3",
            mean_dx
        );
        assert!(
            (mean_dy - (-2.0)).abs() < 1.0,
            "dy mean {} not ~ -2",
            mean_dy
        );
    }

    #[test]
    fn klt_rejects_uniform_region() {
        let prev = GrayImage::from_pixel(64, 64, image::Luma([120]));
        let cur = GrayImage::from_pixel(64, 64, image::Luma([120]));
        let p_prev = Pyramid::build(&prev, 3);
        let p_cur = Pyramid::build(&cur, 3);
        let pts = vec![(32.0, 32.0)];
        let cfg = KltConfig::default();
        let out = track_points(&p_prev, &p_cur, &pts, &cfg);
        assert!(
            !out[0].success,
            "uniform region must be rejected by min-eigenvalue gate"
        );
    }
}
