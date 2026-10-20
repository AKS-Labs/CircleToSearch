//! Extended Kalman Filter on the 8-DoF homography state.
//!
//! See `analysis.md` § "Perspective wobble — edge-amplified" and §
//! "Source-side fixes" for the design rationale. The per-frame RANSAC
//! pipeline produces a fresh, independent H every frame; per-DoF noise
//! that an output EMA can only attenuate uniformly is the result. An
//! EKF carries explicit covariance for each homography element across
//! frames, so well-determined DoFs respond promptly to new evidence
//! while under-determined ones (typically h6/h7 on a clustered inlier
//! set) stay anchored to their prior.
//!
//! State convention: the homography is stored in row-major
//! `[f32; 9]` with the projective gauge fixed by `h[8] = 1`. The
//! tracked state vector is the 8 free elements `[h0..h7]`, and the
//! covariance is an 8×8 `f64` matrix in the same ordering. Fixing
//! `h[8]` eliminates the scale-gauge nullspace that would otherwise
//! make `S = J P J^T + R` rank-deficient.
//!
//! Measurement model: each inlier correspondence `(anchor, view)` is a
//! noisy observation `view = project(H, anchor) + n`, with
//! `n ∼ N(0, R)` for an isotropic pixel-variance `R`. The 2×8
//! Jacobian is derived analytically from the projective division. Per
//! frame, inliers are folded in sequentially (standard sequential EKF
//! update); the linearisation is refreshed on each measurement, which
//! is equivalent to one inner iteration of an iterated EKF.

use crate::homography::project;

/// Process-noise diagonal `Q` for one EKF step.
///
/// Encodes "perspective DoFs change slowly, translation can change
/// fast". The values are tuned for image coordinates in the hundreds
/// of pixels; pre-normalising the inputs is not required because the
/// internal math runs in `f64` and the per-DoF tuning compensates for
/// the natural scale difference between affine and perspective rows.
pub const EKF_Q_DEFAULT: [f64; 8] = [
    1.0e-4, 1.0e-4, 4.0, // h0, h1, h2 (h2 = tx, fast)
    1.0e-4, 1.0e-4, 4.0, // h3, h4, h5 (h5 = ty, fast)
    1.0e-9, 1.0e-9, // h6, h7 (perspective, slow)
];

/// Initial covariance diagonal `P0` for a fresh filter.
///
/// Sized so the first one or two RANSAC frames dominate the state;
/// after that the steady-state covariance is set by the ratio of `Q`
/// to the per-inlier information contribution.
pub const EKF_P0_DEFAULT: [f64; 8] = [1.0e-2, 1.0e-2, 1.0e2, 1.0e-2, 1.0e-2, 1.0e2, 1.0e-7, 1.0e-7];

/// Default per-inlier measurement variance in pixel² units.
///
/// Matches the RANSAC residual gate (`4 px`) interpreted as
/// `≈ 2σ`, giving `σ² ≈ 4`.
pub const EKF_R_DEFAULT: f64 = 4.0;

#[derive(Clone, Debug)]
pub struct HomographyEkf {
    pub h: [f32; 9],
    pub p: [[f64; 8]; 8],
}

impl HomographyEkf {
    /// Initialise from a measured homography and the default initial
    /// covariance. `h` is rescaled so `h[8] = 1`.
    pub fn new(h: [f32; 9]) -> Option<Self> {
        Self::with_covariance(h, &EKF_P0_DEFAULT)
    }

    pub fn with_covariance(h: [f32; 9], p_diag: &[f64; 8]) -> Option<Self> {
        let h = canonicalise(&h)?;
        let mut p = [[0.0_f64; 8]; 8];
        for i in 0..8 {
            p[i][i] = p_diag[i];
        }
        Some(Self { h, p })
    }

    /// Process step: add `Q` to the diagonal of `P`.
    ///
    /// The trivial process model `H_t = H_{t-1} + w` makes the state
    /// transition Jacobian the identity, so the covariance update
    /// collapses to `P ← P + Q`.
    pub fn predict(&mut self, q_diag: &[f64; 8]) {
        for i in 0..8 {
            self.p[i][i] += q_diag[i];
        }
    }

    /// Fold all `(anchor_x, anchor_y, view_x, view_y)` correspondences
    /// into the state. Returns the number of measurements actually
    /// applied (some can be skipped if the linearisation degenerates).
    pub fn update_pairs(&mut self, pairs: &[(f32, f32, f32, f32)], r_var: f64) -> usize {
        let mut applied = 0;
        for &(ax, ay, vx, vy) in pairs {
            if self.update(ax, ay, vx, vy, r_var) {
                applied += 1;
            }
        }
        applied
    }

    /// Fold a single correspondence into the state. Returns `false`
    /// if the projection or the 2×2 innovation covariance is too
    /// degenerate to invert; the state is unchanged in that case.
    pub fn update(&mut self, ax: f32, ay: f32, vx: f32, vy: f32, r_var: f64) -> bool {
        let x = ax as f64;
        let y = ay as f64;
        let h0 = self.h[0] as f64;
        let h1 = self.h[1] as f64;
        let h2 = self.h[2] as f64;
        let h3 = self.h[3] as f64;
        let h4 = self.h[4] as f64;
        let h5 = self.h[5] as f64;
        let h6 = self.h[6] as f64;
        let h7 = self.h[7] as f64;
        let w = h6 * x + h7 * y + 1.0;
        if !w.is_finite() || w.abs() < 1e-9 {
            return false;
        }
        let inv_w = 1.0 / w;
        let u_pred = (h0 * x + h1 * y + h2) * inv_w;
        let v_pred = (h3 * x + h4 * y + h5) * inv_w;

        // ∂[u; v] / ∂[h0..h7].
        let j_u = [
            x * inv_w,
            y * inv_w,
            inv_w,
            0.0,
            0.0,
            0.0,
            -x * u_pred * inv_w,
            -y * u_pred * inv_w,
        ];
        let j_v = [
            0.0,
            0.0,
            0.0,
            x * inv_w,
            y * inv_w,
            inv_w,
            -x * v_pred * inv_w,
            -y * v_pred * inv_w,
        ];

        // PJ^T (8×2).
        let mut pjt = [[0.0_f64; 2]; 8];
        for i in 0..8 {
            let mut s0 = 0.0;
            let mut s1 = 0.0;
            for k in 0..8 {
                s0 += self.p[i][k] * j_u[k];
                s1 += self.p[i][k] * j_v[k];
            }
            pjt[i][0] = s0;
            pjt[i][1] = s1;
        }

        // S = J PJ^T + R (2×2).
        let mut s00 = r_var;
        let mut s01 = 0.0;
        let mut s11 = r_var;
        for k in 0..8 {
            s00 += j_u[k] * pjt[k][0];
            s01 += j_u[k] * pjt[k][1];
            s11 += j_v[k] * pjt[k][1];
        }
        let det = s00 * s11 - s01 * s01;
        if !det.is_finite() || det <= 0.0 {
            return false;
        }
        let inv_det = 1.0 / det;
        let s_inv00 = s11 * inv_det;
        let s_inv11 = s00 * inv_det;
        let s_inv01 = -s01 * inv_det;

        // K = PJ^T S^-1 (8×2).
        let mut k_mat = [[0.0_f64; 2]; 8];
        for i in 0..8 {
            k_mat[i][0] = pjt[i][0] * s_inv00 + pjt[i][1] * s_inv01;
            k_mat[i][1] = pjt[i][0] * s_inv01 + pjt[i][1] * s_inv11;
        }

        let y_u = vx as f64 - u_pred;
        let y_v = vy as f64 - v_pred;

        for i in 0..8 {
            let dh = k_mat[i][0] * y_u + k_mat[i][1] * y_v;
            let new_val = self.h[i] as f64 + dh;
            if !new_val.is_finite() {
                return false;
            }
            self.h[i] = new_val as f32;
        }
        // h[8] is the gauge anchor: keep it exactly 1.

        // P ← (I − K J) P. Symmetrise to absorb f64 round-off so the
        // covariance does not drift away from being positive-definite
        // over thousands of updates.
        let mut kj = [[0.0_f64; 8]; 8];
        for i in 0..8 {
            for j in 0..8 {
                kj[i][j] = k_mat[i][0] * j_u[j] + k_mat[i][1] * j_v[j];
            }
        }
        let mut new_p = [[0.0_f64; 8]; 8];
        for i in 0..8 {
            for j in 0..8 {
                let mut s = self.p[i][j];
                for k in 0..8 {
                    s -= kj[i][k] * self.p[k][j];
                }
                new_p[i][j] = s;
            }
        }
        for i in 0..8 {
            for j in (i + 1)..8 {
                let avg = 0.5 * (new_p[i][j] + new_p[j][i]);
                new_p[i][j] = avg;
                new_p[j][i] = avg;
            }
        }
        self.p = new_p;
        true
    }

    /// Reset the state to a freshly measured homography and the
    /// default initial covariance. Used after a tracking loss or
    /// anchor change when the historical covariance is no longer
    /// meaningful for the new anchor's coordinate frame.
    pub fn reset(&mut self, h: [f32; 9]) {
        if let Some(canon) = canonicalise(&h) {
            self.h = canon;
        } else {
            self.h = h;
        }
        for i in 0..8 {
            for j in 0..8 {
                self.p[i][j] = 0.0;
            }
            self.p[i][i] = EKF_P0_DEFAULT[i];
        }
    }

    /// Current best-estimate homography (with `h[8] = 1` by
    /// construction).
    pub fn homography(&self) -> [f32; 9] {
        self.h
    }

    /// Per-DoF marginal standard deviation. Useful for diagnostics
    /// and for downstream code that wants to gate behaviour on
    /// confidence (e.g. skip a perspective-sensitive operation when
    /// `sigma(h6)` or `sigma(h7)` is large).
    pub fn sigmas(&self) -> [f64; 8] {
        let mut out = [0.0_f64; 8];
        for i in 0..8 {
            out[i] = self.p[i][i].max(0.0).sqrt();
        }
        out
    }

    /// Root-mean-square reprojection residual of `pairs` under the
    /// current state. Diagnostic helper for tests and traces.
    pub fn residual_rms(&self, pairs: &[(f32, f32, f32, f32)]) -> f32 {
        if pairs.is_empty() {
            return 0.0;
        }
        let mut sum = 0.0_f64;
        let mut n = 0;
        for &(ax, ay, vx, vy) in pairs {
            if let Some((px, py)) = project(&self.h, ax, ay) {
                let dx = (px - vx) as f64;
                let dy = (py - vy) as f64;
                sum += dx * dx + dy * dy;
                n += 1;
            }
        }
        if n == 0 {
            return 0.0;
        }
        ((sum / n as f64).sqrt()) as f32
    }
}

fn canonicalise(h: &[f32; 9]) -> Option<[f32; 9]> {
    let s = h[8];
    if !s.is_finite() || s.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / s;
    let mut out = [0.0_f32; 9];
    for i in 0..8 {
        out[i] = h[i] * inv;
    }
    out[8] = 1.0;
    if !out.iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_pt(h: &[f32; 9], x: f32, y: f32) -> (f32, f32) {
        project(h, x, y).expect("non-degenerate")
    }

    fn corner_error(a: &[f32; 9], b: &[f32; 9]) -> f32 {
        let pts = [
            (0.0_f32, 0.0_f32),
            (640.0, 0.0),
            (640.0, 480.0),
            (0.0, 480.0),
        ];
        let mut worst = 0.0_f32;
        for &(x, y) in &pts {
            let (ax, ay) = project_pt(a, x, y);
            let (bx, by) = project_pt(b, x, y);
            let d = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
            if d > worst {
                worst = d;
            }
        }
        worst
    }

    fn make_pairs(h: &[f32; 9]) -> Vec<(f32, f32, f32, f32)> {
        let mut pairs = Vec::new();
        let step = 80.0;
        let mut x = 40.0_f32;
        while x < 640.0 {
            let mut y = 40.0_f32;
            while y < 480.0 {
                let (vx, vy) = project_pt(h, x, y);
                pairs.push((x, y, vx, vy));
                y += step;
            }
            x += step;
        }
        pairs
    }

    fn add_noise(pairs: &mut [(f32, f32, f32, f32)], sigma: f32, seed: u32) {
        // Deterministic xorshift32 → Box–Muller; tests must be
        // reproducible without pulling in a PRNG crate.
        let mut s = seed.max(1);
        let mut rand = move || -> f32 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s as f32 / u32::MAX as f32).clamp(1e-6, 1.0 - 1e-6)
        };
        for p in pairs.iter_mut() {
            let u1 = rand();
            let u2 = rand();
            let r = (-2.0 * u1.ln()).sqrt();
            let theta = 2.0 * std::f32::consts::PI * u2;
            let n1 = r * theta.cos() * sigma;
            let n2 = r * theta.sin() * sigma;
            p.2 += n1;
            p.3 += n2;
        }
    }

    #[test]
    fn canonicalises_h22() {
        let raw = [2.0, 0.0, 10.0, 0.0, 2.0, 20.0, 0.0, 0.0, 2.0];
        let ekf = HomographyEkf::new(raw).expect("non-degenerate");
        assert!((ekf.h[8] - 1.0).abs() < 1e-6);
        assert!((ekf.h[0] - 1.0).abs() < 1e-6);
        assert!((ekf.h[2] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_degenerate_h() {
        let bad = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        assert!(HomographyEkf::new(bad).is_none());
    }

    #[test]
    fn single_frame_clean_data_recovers_h() {
        // Mild perspective.
        let truth = [1.05, -0.04, 12.0, 0.03, 0.98, -7.0, 1.2e-4, -5.0e-5, 1.0];
        let pairs = make_pairs(&truth);
        // Seed the filter with identity, large initial covariance so
        // the first frame's data dominates.
        let mut ekf = HomographyEkf::with_covariance(
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            &[1.0, 1.0, 1.0e4, 1.0, 1.0, 1.0e4, 1.0e-3, 1.0e-3],
        )
        .unwrap();
        let applied = ekf.update_pairs(&pairs, 0.25);
        assert_eq!(applied, pairs.len());
        let err = corner_error(&ekf.h, &truth);
        assert!(err < 1.0, "corner error {err} px should be < 1");
        let rms = ekf.residual_rms(&pairs);
        assert!(rms < 0.5, "rms {rms}");
    }

    #[test]
    fn noisy_observations_average_out_across_frames() {
        let truth = [1.02, 0.01, 8.0, -0.015, 1.01, 4.0, 5.0e-5, 2.0e-5, 1.0];
        let mut ekf = HomographyEkf::new(truth).unwrap();
        let mut best_err = f32::INFINITY;
        for frame in 0..30 {
            let mut pairs = make_pairs(&truth);
            add_noise(&mut pairs, 0.6, 1234 + frame * 17);
            ekf.predict(&EKF_Q_DEFAULT);
            ekf.update_pairs(&pairs, EKF_R_DEFAULT);
            let err = corner_error(&ekf.h, &truth);
            if err < best_err {
                best_err = err;
            }
        }
        assert!(best_err < 0.4, "best corner error {best_err} px");
    }

    #[test]
    fn perspective_dofs_resist_under_constrained_updates() {
        // Sparse correspondences in one row — y-spread is tiny, so
        // the H7 column of J is nearly degenerate. Without the prior,
        // a least-squares fit can drift h7 wildly; with the EKF the
        // h7 covariance is small, so the state stays close to the
        // truth on that DoF.
        let truth = [1.01, 0.0, 5.0, 0.0, 1.0, 0.0, 6.0e-5, -3.0e-5, 1.0];
        let mut ekf = HomographyEkf::new(truth).unwrap();
        let mut pairs = Vec::new();
        for x in (60..600).step_by(60) {
            let xf = x as f32;
            let yf = 240.0;
            let (vx, vy) = project_pt(&truth, xf, yf);
            pairs.push((xf, yf, vx, vy));
        }
        add_noise(&mut pairs, 0.4, 99);
        ekf.predict(&EKF_Q_DEFAULT);
        let applied = ekf.update_pairs(&pairs, EKF_R_DEFAULT);
        assert_eq!(applied, pairs.len());
        let dh6 = (ekf.h[6] - truth[6]).abs();
        let dh7 = (ekf.h[7] - truth[7]).abs();
        assert!(
            dh7 < 5.0e-5,
            "h7 drift {dh7} should stay small (no y-spread)"
        );
        assert!(dh6 < 5.0e-5, "h6 drift {dh6}");
    }

    #[test]
    fn predict_only_grows_covariance() {
        let h = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let mut ekf = HomographyEkf::new(h).unwrap();
        let before = ekf.sigmas();
        for _ in 0..10 {
            ekf.predict(&EKF_Q_DEFAULT);
        }
        let after = ekf.sigmas();
        for i in 0..8 {
            assert!(after[i] >= before[i], "sigma[{i}] {before:?} -> {after:?}");
        }
        // Translation rows had Q = 4, so 10 predicts add 40 to the
        // variance — sigma grows from sqrt(100) ≈ 10 to sqrt(140) ≈ 11.83.
        assert!(after[2] > before[2]);
        assert!(after[5] > before[5]);
    }

    #[test]
    fn update_shrinks_covariance() {
        let truth = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let mut ekf = HomographyEkf::with_covariance(
            truth,
            &[1.0, 1.0, 1.0e3, 1.0, 1.0, 1.0e3, 1.0e-4, 1.0e-4],
        )
        .unwrap();
        let before = ekf.sigmas();
        let pairs = make_pairs(&truth);
        ekf.update_pairs(&pairs, 1.0);
        let after = ekf.sigmas();
        for i in 0..8 {
            assert!(
                after[i] < before[i],
                "sigma[{i}] should shrink: before {} after {}",
                before[i],
                after[i]
            );
        }
    }

    #[test]
    fn tracks_slowly_moving_h() {
        // Drift the truth H over 60 frames; the EKF should keep up
        // because the drift is well within Q's "translation can change
        // fast" budget.
        let mut truth = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let mut ekf = HomographyEkf::new(truth).unwrap();
        let mut worst = 0.0_f32;
        for frame in 0..60 {
            truth[2] += 0.6; // pan tx by 0.6 px/frame
            truth[5] -= 0.3;
            truth[6] += 1.0e-7;
            let mut pairs = make_pairs(&truth);
            add_noise(&mut pairs, 0.5, 7 + frame * 31);
            ekf.predict(&EKF_Q_DEFAULT);
            ekf.update_pairs(&pairs, EKF_R_DEFAULT);
            let err = corner_error(&ekf.h, &truth);
            if frame > 5 && err > worst {
                // Skip the first few frames where the filter is still
                // converging from its identity-ish prior.
                worst = err;
            }
        }
        assert!(worst < 1.0, "steady-state worst corner error {worst} px");
    }

    #[test]
    fn reset_clears_history() {
        let mut ekf = HomographyEkf::new([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]).unwrap();
        for _ in 0..5 {
            let pairs = make_pairs(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
            ekf.update_pairs(&pairs, EKF_R_DEFAULT);
        }
        let shrunk = ekf.sigmas();
        ekf.reset([1.1, 0.0, 5.0, 0.0, 1.05, -3.0, 0.0, 0.0, 1.0]);
        assert!((ekf.h[2] - 5.0).abs() < 1e-6);
        let reset_sigmas = ekf.sigmas();
        for i in 0..8 {
            assert!(
                reset_sigmas[i] >= shrunk[i],
                "reset should re-inflate sigma[{i}]"
            );
        }
    }
}
