//! Homography fitting and 3x3 matrix utilities shared between the legacy
//! per-region tracker and the planar surface tracker.
//!
//! Convention: a homography is a row-major `[f32; 9]` such that
//! `(qx, qy, 1) ~ H * (px, py, 1)` in homogeneous coordinates — divide
//! the projected vector by its `w` component to recover the destination
//! pixel.

/// Direct Linear Transform homography fit with Hartley point normalization.
/// Given correspondences `(px, py) -> (qx, qy)`, returns a row-major 3x3
/// matrix `H` such that `(qx, qy, 1) ~ H * (px, py, 1)` in homogeneous
/// coordinates (i.e. the projected point divided by w gives `(qx, qy)`).
///
/// Uses normal equations on the 2N x 8 system that fixes `h33 = 1`. Each
/// correspondence contributes two rows:
///
///   [px, py, 1, 0, 0, 0, -px*qx, -py*qx]   = qx
///   [0, 0, 0, px, py, 1, -px*qy, -py*qy]   = qy
///
/// Returns `None` if the system is too ill-conditioned (e.g. correspondences
/// are colinear or all clustered at one point).
pub fn fit_homography(pairs: &[(f32, f32, f32, f32)]) -> Option<[f32; 9]> {
    if pairs.len() < 4 {
        return None;
    }
    let n = pairs.len() as f32;
    let (mut mpx, mut mpy, mut mqx, mut mqy) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    for &(px, py, qx, qy) in pairs {
        mpx += px;
        mpy += py;
        mqx += qx;
        mqy += qy;
    }
    mpx /= n;
    mpy /= n;
    mqx /= n;
    mqy /= n;
    let (mut sp, mut sq) = (0.0_f32, 0.0_f32);
    for &(px, py, qx, qy) in pairs {
        let dpx = px - mpx;
        let dpy = py - mpy;
        let dqx = qx - mqx;
        let dqy = qy - mqy;
        sp += (dpx * dpx + dpy * dpy).sqrt();
        sq += (dqx * dqx + dqy * dqy).sqrt();
    }
    sp /= n;
    sq /= n;
    if sp <= 1e-3 || sq <= 1e-3 {
        return None;
    }
    let kp = (2.0_f32).sqrt() / sp;
    let kq = (2.0_f32).sqrt() / sq;

    let mut ata = [[0.0_f64; 8]; 8];
    let mut atb = [0.0_f64; 8];
    for &(px, py, qx, qy) in pairs {
        let px_n = (px - mpx) * kp;
        let py_n = (py - mpy) * kp;
        let qx_n = (qx - mqx) * kq;
        let qy_n = (qy - mqy) * kq;
        let r1 = [
            px_n as f64,
            py_n as f64,
            1.0,
            0.0,
            0.0,
            0.0,
            (-px_n * qx_n) as f64,
            (-py_n * qx_n) as f64,
        ];
        let r2 = [
            0.0,
            0.0,
            0.0,
            px_n as f64,
            py_n as f64,
            1.0,
            (-px_n * qy_n) as f64,
            (-py_n * qy_n) as f64,
        ];
        for i in 0..8 {
            for j in 0..8 {
                ata[i][j] += r1[i] * r1[j] + r2[i] * r2[j];
            }
            atb[i] += r1[i] * qx_n as f64 + r2[i] * qy_n as f64;
        }
    }
    let h_norm = solve_8x8(ata, atb)?;
    let h_normalised = [
        h_norm[0] as f32,
        h_norm[1] as f32,
        h_norm[2] as f32,
        h_norm[3] as f32,
        h_norm[4] as f32,
        h_norm[5] as f32,
        h_norm[6] as f32,
        h_norm[7] as f32,
        1.0_f32,
    ];
    let tp = [kp, 0.0, -kp * mpx, 0.0, kp, -kp * mpy, 0.0, 0.0, 1.0];
    let tq_inv = [1.0 / kq, 0.0, mqx, 0.0, 1.0 / kq, mqy, 0.0, 0.0, 1.0];
    let h1 = mat3_mul(&h_normalised, &tp);
    let h2 = mat3_mul(&tq_inv, &h1);
    if !h2.iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(h2)
}

/// Like [`fit_homography`] but adds a damped Tikhonov-style prior
/// term that pulls the solution toward `h_prior`. Used as the
/// post-RANSAC refit step when a previous frame's H is available:
/// the data term `||A·h − b||²` (algebraic DLT residuals on
/// normalised coords) is augmented with `λ · ||h − h_prior_norm||²`,
/// so the solver picks the basin closest to the prior when the
/// data is consistent with multiple basins (a recurring source of
/// frame-96-style wrong-basin events on sparse-inlier frames).
///
/// `lambda` is in normalised-coord units (Hartley-normalised
/// coordinates have RMS ≈ √2). A reasonable scale is the inlier
/// count `N`: setting `lambda = N` puts the prior on the same
/// footing as roughly one inlier pair's worth of data. For well-
/// determined DoFs (data Jacobian dominates) the prior contributes
/// ≲ 25% of the diagonal. For under-determined DoFs (e.g.
/// perspective elements at large rotation, where the data
/// Jacobian column collapses), the prior takes over.
///
/// The prior H is transformed into the normalised coordinate
/// frame before being baked into the normal equations and the
/// result is de-normalised back to image coords.
pub fn fit_homography_with_temporal_prior(
    pairs: &[(f32, f32, f32, f32)],
    h_prior: &[f32; 9],
    lambda: f32,
) -> Option<[f32; 9]> {
    if pairs.len() < 4 {
        return None;
    }
    let n = pairs.len() as f32;
    let (mut mpx, mut mpy, mut mqx, mut mqy) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    for &(px, py, qx, qy) in pairs {
        mpx += px;
        mpy += py;
        mqx += qx;
        mqy += qy;
    }
    mpx /= n;
    mpy /= n;
    mqx /= n;
    mqy /= n;
    let (mut sp, mut sq) = (0.0_f32, 0.0_f32);
    for &(px, py, qx, qy) in pairs {
        let dpx = px - mpx;
        let dpy = py - mpy;
        let dqx = qx - mqx;
        let dqy = qy - mqy;
        sp += (dpx * dpx + dpy * dpy).sqrt();
        sq += (dqx * dqx + dqy * dqy).sqrt();
    }
    sp /= n;
    sq /= n;
    if sp <= 1e-3 || sq <= 1e-3 {
        return None;
    }
    let kp = (2.0_f32).sqrt() / sp;
    let kq = (2.0_f32).sqrt() / sq;
    // Forward normalisation matrices: maps image coords → normalised.
    let tp = [kp, 0.0, -kp * mpx, 0.0, kp, -kp * mpy, 0.0, 0.0, 1.0];
    let tq = [kq, 0.0, -kq * mqx, 0.0, kq, -kq * mqy, 0.0, 0.0, 1.0];
    // Transform the prior into the normalised coordinate frame:
    //   H_norm = T_q · H_prior · T_p⁻¹
    let tp_inv = invert(&tp)?;
    let h_prior_norm = mat3_mul(&tq, &mat3_mul(h_prior, &tp_inv));
    // Rescale so h_prior_norm[8] = 1 (consistent with the DLT solver's
    // h22 = 1 convention).
    let scale = h_prior_norm[8];
    if scale.abs() < 1e-9 {
        return None;
    }
    let inv_scale = 1.0 / scale;
    let h_prior_8 = [
        (h_prior_norm[0] * inv_scale) as f64,
        (h_prior_norm[1] * inv_scale) as f64,
        (h_prior_norm[2] * inv_scale) as f64,
        (h_prior_norm[3] * inv_scale) as f64,
        (h_prior_norm[4] * inv_scale) as f64,
        (h_prior_norm[5] * inv_scale) as f64,
        (h_prior_norm[6] * inv_scale) as f64,
        (h_prior_norm[7] * inv_scale) as f64,
    ];
    // Same DLT system as `fit_homography`, accumulating A^T A and A^T b.
    let mut ata = [[0.0_f64; 8]; 8];
    let mut atb = [0.0_f64; 8];
    for &(px, py, qx, qy) in pairs {
        let px_n = (px - mpx) * kp;
        let py_n = (py - mpy) * kp;
        let qx_n = (qx - mqx) * kq;
        let qy_n = (qy - mqy) * kq;
        let r1 = [
            px_n as f64,
            py_n as f64,
            1.0,
            0.0,
            0.0,
            0.0,
            (-px_n * qx_n) as f64,
            (-py_n * qx_n) as f64,
        ];
        let r2 = [
            0.0,
            0.0,
            0.0,
            px_n as f64,
            py_n as f64,
            1.0,
            (-px_n * qy_n) as f64,
            (-py_n * qy_n) as f64,
        ];
        for i in 0..8 {
            for j in 0..8 {
                ata[i][j] += r1[i] * r1[j] + r2[i] * r2[j];
            }
            atb[i] += r1[i] * qx_n as f64 + r2[i] * qy_n as f64;
        }
    }
    // Prior damping: (A^T A + λI) h = A^T b + λ h_prior_8.
    // In normalised coords, A^T A diagonals scale ~ N; choosing λ = N
    // puts the prior at "one pair's worth" of data weight on well-
    // determined DoFs (contributes ≲ 25%) and dominates on DoFs whose
    // diagonal is much smaller than N (the under-determined ones).
    let lambda_f64 = lambda as f64;
    for i in 0..8 {
        ata[i][i] += lambda_f64;
        atb[i] += lambda_f64 * h_prior_8[i];
    }
    let h_norm = solve_8x8(ata, atb)?;
    let h_normalised = [
        h_norm[0] as f32,
        h_norm[1] as f32,
        h_norm[2] as f32,
        h_norm[3] as f32,
        h_norm[4] as f32,
        h_norm[5] as f32,
        h_norm[6] as f32,
        h_norm[7] as f32,
        1.0_f32,
    ];
    // De-normalise: H = T_q⁻¹ · H_norm · T_p.
    let tq_inv = [1.0 / kq, 0.0, mqx, 0.0, 1.0 / kq, mqy, 0.0, 0.0, 1.0];
    let h1 = mat3_mul(&h_normalised, &tp);
    let h2 = mat3_mul(&tq_inv, &h1);
    if !h2.iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(h2)
}

/// 4-DoF similarity transform fit (translation + uniform scale +
/// rotation). Returns a 3×3 matrix lifted from the 2×3 affine form so
/// callers can use it interchangeably with a homography:
///
///   [ s·cosθ   -s·sinθ   tx ]
///   [ s·sinθ    s·cosθ   ty ]
///   [   0          0      1 ]
///
/// Why a separate fit instead of just using `fit_homography`: when
/// inliers are sparse, the 8-DoF homography fit is under-constrained
/// and the implicit rotation that emerges from the linear solve can
/// swing by tens of degrees frame-to-frame. A similarity fit has
/// rotation as a single explicit parameter computed by closed-form
/// least squares over *all* inliers, so it stays physically stable
/// even with ~10 correspondences.
///
/// Closed form follows Horn (1987): centroid-subtracted complex inner
/// product gives the rotation+scale; the centroid pair determines the
/// translation. Returns `None` if all correspondences collapse to a
/// single point (zero scatter).
pub fn fit_similarity(pairs: &[(f32, f32, f32, f32)]) -> Option<[f32; 9]> {
    if pairs.len() < 2 {
        return None;
    }
    let n = pairs.len() as f32;
    let (mut mpx, mut mpy, mut mqx, mut mqy) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    for &(px, py, qx, qy) in pairs {
        mpx += px;
        mpy += py;
        mqx += qx;
        mqy += qy;
    }
    mpx /= n;
    mpy /= n;
    mqx /= n;
    mqy /= n;
    let mut num_real = 0.0_f32;
    let mut num_imag = 0.0_f32;
    let mut den = 0.0_f32;
    for &(px, py, qx, qy) in pairs {
        let dpx = px - mpx;
        let dpy = py - mpy;
        let dqx = qx - mqx;
        let dqy = qy - mqy;
        // Complex conjugate inner product Σ dq · conj(dp):
        //   real = dqx·dpx + dqy·dpy   (= s·cosθ · |dp|²)
        //   imag = dqy·dpx − dqx·dpy   (= s·sinθ · |dp|²)
        num_real += dqx * dpx + dqy * dpy;
        num_imag += dqy * dpx - dqx * dpy;
        den += dpx * dpx + dpy * dpy;
    }
    if den < 1e-6 {
        return None;
    }
    let s_cos = num_real / den;
    let s_sin = num_imag / den;
    let tx = mqx - (s_cos * mpx - s_sin * mpy);
    let ty = mqy - (s_sin * mpx + s_cos * mpy);
    let m = [s_cos, -s_sin, tx, s_sin, s_cos, ty, 0.0, 0.0, 1.0];
    if !m.iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(m)
}

/// 6-DoF affine fit. Same shape as `fit_homography` but without the
/// projective division — the bottom row of the resulting 3×3 is fixed
/// to `[0, 0, 1]`. Used in the 15–30-inlier band where homography is
/// under-constrained but similarity loses too much expressiveness
/// (e.g. shearing under mild perspective). Solves the 2N×6 normal
/// equations directly.
pub fn fit_affine(pairs: &[(f32, f32, f32, f32)]) -> Option<[f32; 9]> {
    if pairs.len() < 3 {
        return None;
    }
    // Centre + isotropic scale: same Hartley-style normalization as
    // `fit_homography`. The 6×6 normal equations are well-conditioned
    // for any reasonable point spread, but normalization keeps the
    // numerics consistent with the homography path.
    let n = pairs.len() as f32;
    let (mut mpx, mut mpy, mut mqx, mut mqy) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    for &(px, py, qx, qy) in pairs {
        mpx += px;
        mpy += py;
        mqx += qx;
        mqy += qy;
    }
    mpx /= n;
    mpy /= n;
    mqx /= n;
    mqy /= n;
    let (mut sp, mut sq) = (0.0_f32, 0.0_f32);
    for &(px, py, qx, qy) in pairs {
        let dpx = px - mpx;
        let dpy = py - mpy;
        let dqx = qx - mqx;
        let dqy = qy - mqy;
        sp += (dpx * dpx + dpy * dpy).sqrt();
        sq += (dqx * dqx + dqy * dqy).sqrt();
    }
    sp /= n;
    sq /= n;
    if sp <= 1e-3 || sq <= 1e-3 {
        return None;
    }
    let kp = (2.0_f32).sqrt() / sp;
    let kq = (2.0_f32).sqrt() / sq;
    let mut ata = [[0.0_f64; 6]; 6];
    let mut atb = [0.0_f64; 6];
    for &(px, py, qx, qy) in pairs {
        let px_n = ((px - mpx) * kp) as f64;
        let py_n = ((py - mpy) * kp) as f64;
        let qx_n = ((qx - mqx) * kq) as f64;
        let qy_n = ((qy - mqy) * kq) as f64;
        // Affine: qx = a*px + b*py + c, qy = d*px + e*py + f.
        // Unknown vector is [a, b, c, d, e, f]. Two rows per point.
        let r1 = [px_n, py_n, 1.0, 0.0, 0.0, 0.0];
        let r2 = [0.0, 0.0, 0.0, px_n, py_n, 1.0];
        for i in 0..6 {
            for j in 0..6 {
                ata[i][j] += r1[i] * r1[j] + r2[i] * r2[j];
            }
            atb[i] += r1[i] * qx_n + r2[i] * qy_n;
        }
    }
    let sol = solve_6x6(ata, atb)?;
    let a = sol[0] as f32;
    let b = sol[1] as f32;
    let c = sol[2] as f32;
    let d = sol[3] as f32;
    let e = sol[4] as f32;
    let f = sol[5] as f32;
    let a_norm = [a, b, c, d, e, f, 0.0, 0.0, 1.0];
    // Unnormalize: H = T_q^-1 · A_norm · T_p, where T_p and T_q^-1 are
    // the same translation+scale matrices used in the homography path.
    let tp = [kp, 0.0, -kp * mpx, 0.0, kp, -kp * mpy, 0.0, 0.0, 1.0];
    let tq_inv = [1.0 / kq, 0.0, mqx, 0.0, 1.0 / kq, mqy, 0.0, 0.0, 1.0];
    let h1 = mat3_mul(&a_norm, &tp);
    let h2 = mat3_mul(&tq_inv, &h1);
    if !h2.iter().all(|v| v.is_finite()) {
        return None;
    }
    // Force the bottom row exactly to (0, 0, 1). The composition can
    // leave it floating-point noisy; we want it bit-exact so downstream
    // projections never accidentally divide by anything other than 1.
    Some([h2[0], h2[1], h2[2], h2[3], h2[4], h2[5], 0.0, 0.0, 1.0])
}

/// Gauss-Jordan elimination with partial pivoting on the 6×6 system
/// `A · x = b`. Returns `None` if the matrix is near-singular.
fn solve_6x6(mut a: [[f64; 6]; 6], mut b: [f64; 6]) -> Option<[f64; 6]> {
    for col in 0..6 {
        let mut piv_row = col;
        let mut piv_abs = a[col][col].abs();
        for r in (col + 1)..6 {
            let v = a[r][col].abs();
            if v > piv_abs {
                piv_abs = v;
                piv_row = r;
            }
        }
        if piv_abs < 1e-9 {
            return None;
        }
        if piv_row != col {
            a.swap(col, piv_row);
            b.swap(col, piv_row);
        }
        let inv = 1.0 / a[col][col];
        for j in 0..6 {
            a[col][j] *= inv;
        }
        b[col] *= inv;
        for r in 0..6 {
            if r == col {
                continue;
            }
            let factor = a[r][col];
            if factor == 0.0 {
                continue;
            }
            for j in 0..6 {
                a[r][j] -= factor * a[col][j];
            }
            b[r] -= factor * b[col];
        }
    }
    Some(b)
}

pub fn mat3_mul(a: &[f32; 9], b: &[f32; 9]) -> [f32; 9] {
    let mut out = [0.0_f32; 9];
    for i in 0..3 {
        for j in 0..3 {
            let mut s = 0.0_f32;
            for k in 0..3 {
                s += a[i * 3 + k] * b[k * 3 + j];
            }
            out[i * 3 + j] = s;
        }
    }
    out
}

/// Project a 2D point through a row-major 3x3 homography.
/// Returns `None` if the projected homogeneous `w` is degenerate.
pub fn project(h: &[f32; 9], x: f32, y: f32) -> Option<(f32, f32)> {
    let qx = h[0] * x + h[1] * y + h[2];
    let qy = h[3] * x + h[4] * y + h[5];
    let qw = h[6] * x + h[7] * y + h[8];
    if qw.abs() < 1e-9 || !qw.is_finite() {
        return None;
    }
    Some((qx / qw, qy / qw))
}

/// Inverse of a row-major 3x3. Returns `None` if singular.
pub fn invert(h: &[f32; 9]) -> Option<[f32; 9]> {
    let a = h[0] as f64;
    let b = h[1] as f64;
    let c = h[2] as f64;
    let d = h[3] as f64;
    let e = h[4] as f64;
    let f = h[5] as f64;
    let g = h[6] as f64;
    let hh = h[7] as f64;
    let i = h[8] as f64;
    let det = a * (e * i - f * hh) - b * (d * i - f * g) + c * (d * hh - e * g);
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    let inv_det = 1.0 / det;
    let m = [
        ((e * i - f * hh) * inv_det) as f32,
        ((c * hh - b * i) * inv_det) as f32,
        ((b * f - c * e) * inv_det) as f32,
        ((f * g - d * i) * inv_det) as f32,
        ((a * i - c * g) * inv_det) as f32,
        ((c * d - a * f) * inv_det) as f32,
        ((d * hh - e * g) * inv_det) as f32,
        ((b * g - a * hh) * inv_det) as f32,
        ((a * e - b * d) * inv_det) as f32,
    ];
    if !m.iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(m)
}

/// Decompose `h` into a 4-DoF similarity `S` (rotation + uniform
/// scale + translation) and a perspective/shear residual `P` such
/// that `H = S · P`. Returns `(S, P)`.
///
/// Used by the per-frame H smoother to apply different time-
/// constants to similarity (passed through — real camera motion
/// lives here) and perspective (heavily EMA'd — the corner-amplified
/// wobble lives here). See `analysis.md` § "Perspective wobble".
///
/// Method: project four corners of a reference square through H,
/// fit a similarity to (square, projected_quad), then compute
/// `P = S⁻¹ · H`. Falls back to `(identity, h)` if the H is
/// degenerate enough that the projection or fit fails.
pub fn decompose_similarity_perspective(h: &[f32; 9]) -> ([f32; 9], [f32; 9]) {
    const W: f32 = 100.0;
    let src = [(0.0_f32, 0.0_f32), (W, 0.0), (W, W), (0.0, W)];
    let mut dst = [(0.0_f32, 0.0_f32); 4];
    for i in 0..4 {
        match project(h, src[i].0, src[i].1) {
            Some(p) => dst[i] = p,
            None => return (IDENTITY3, *h),
        }
    }
    let pairs: [(f32, f32, f32, f32); 4] = [
        (src[0].0, src[0].1, dst[0].0, dst[0].1),
        (src[1].0, src[1].1, dst[1].0, dst[1].1),
        (src[2].0, src[2].1, dst[2].0, dst[2].1),
        (src[3].0, src[3].1, dst[3].0, dst[3].1),
    ];
    let s = match fit_similarity(&pairs) {
        Some(s) => s,
        None => return (IDENTITY3, *h),
    };
    let s_inv = match invert(&s) {
        Some(i) => i,
        None => return (IDENTITY3, *h),
    };
    let p = mat3_mul(&s_inv, h);
    (s, p)
}

const IDENTITY3: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(h: &[f32; 9], x: f32, y: f32) -> (f32, f32) {
        project(h, x, y).unwrap()
    }

    #[test]
    fn similarity_recovers_rotation_and_scale() {
        // 12° rotation + 1.4× scale + translation (5, -3).
        let theta = 12.0_f32.to_radians();
        let (s_sin, s_cos) = (theta.sin() * 1.4, theta.cos() * 1.4);
        let known = [s_cos, -s_sin, 5.0, s_sin, s_cos, -3.0, 0.0, 0.0, 1.0];
        let pts = [
            (0.0, 0.0),
            (50.0, 0.0),
            (50.0, 30.0),
            (0.0, 30.0),
            (25.0, 15.0),
            (10.0, 8.0),
        ];
        let pairs: Vec<(f32, f32, f32, f32)> = pts
            .iter()
            .map(|&(x, y)| {
                let (qx, qy) = apply(&known, x, y);
                (x, y, qx, qy)
            })
            .collect();
        let fit = fit_similarity(&pairs).expect("similarity fit");
        for &(px, py, qx, qy) in &pairs {
            let (rx, ry) = apply(&fit, px, py);
            let err = ((rx - qx).powi(2) + (ry - qy).powi(2)).sqrt();
            assert!(err < 1e-3, "similarity recovers within 1e-3, got {}", err);
        }
        // Bottom row must be exact identity so chain composition stays
        // numerically clean.
        assert_eq!(fit[6], 0.0);
        assert_eq!(fit[7], 0.0);
        assert_eq!(fit[8], 1.0);
    }

    #[test]
    fn similarity_robust_to_sparse_inliers() {
        // Same transform, only 4 correspondences — the sparse regime
        // where homography fits get noisy. Similarity should still
        // recover cleanly.
        let theta = 20.0_f32.to_radians();
        let (s_sin, s_cos) = (theta.sin() * 0.9, theta.cos() * 0.9);
        let known = [s_cos, -s_sin, 8.0, s_sin, s_cos, 4.0, 0.0, 0.0, 1.0];
        let pts = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let pairs: Vec<(f32, f32, f32, f32)> = pts
            .iter()
            .map(|&(x, y)| {
                let (qx, qy) = apply(&known, x, y);
                (x, y, qx, qy)
            })
            .collect();
        let fit = fit_similarity(&pairs).expect("similarity fit");
        for &(px, py, qx, qy) in &pairs {
            let (rx, ry) = apply(&fit, px, py);
            let err = ((rx - qx).powi(2) + (ry - qy).powi(2)).sqrt();
            assert!(err < 1e-3, "sparse similarity err {}", err);
        }
    }

    #[test]
    fn affine_recovers_shear() {
        // Affine transform with shear that similarity can't represent.
        let known = [1.1, 0.3, 7.0, -0.2, 0.95, -4.0, 0.0, 0.0, 1.0];
        let pts = [
            (0.0, 0.0),
            (60.0, 0.0),
            (60.0, 40.0),
            (0.0, 40.0),
            (20.0, 10.0),
            (40.0, 30.0),
            (15.0, 35.0),
        ];
        let pairs: Vec<(f32, f32, f32, f32)> = pts
            .iter()
            .map(|&(x, y)| {
                let (qx, qy) = apply(&known, x, y);
                (x, y, qx, qy)
            })
            .collect();
        let fit = fit_affine(&pairs).expect("affine fit");
        for &(px, py, qx, qy) in &pairs {
            let (rx, ry) = apply(&fit, px, py);
            let err = ((rx - qx).powi(2) + (ry - qy).powi(2)).sqrt();
            assert!(err < 1e-2, "affine recovery err {}", err);
        }
        assert_eq!(fit[6], 0.0);
        assert_eq!(fit[7], 0.0);
        assert_eq!(fit[8], 1.0);
    }

    #[test]
    fn similarity_returns_none_on_collapsed_points() {
        let pairs = vec![(5.0, 5.0, 10.0, 10.0); 6];
        assert!(fit_similarity(&pairs).is_none());
    }
}

/// Gauss-Jordan elimination with partial pivoting on the 8x8 system
/// `A * x = b`. Returns `None` if the matrix is near-singular.
fn solve_8x8(mut a: [[f64; 8]; 8], mut b: [f64; 8]) -> Option<[f64; 8]> {
    for col in 0..8 {
        let mut piv_row = col;
        let mut piv_abs = a[col][col].abs();
        for r in (col + 1)..8 {
            let v = a[r][col].abs();
            if v > piv_abs {
                piv_abs = v;
                piv_row = r;
            }
        }
        if piv_abs < 1e-9 {
            return None;
        }
        if piv_row != col {
            a.swap(col, piv_row);
            b.swap(col, piv_row);
        }
        let inv = 1.0 / a[col][col];
        for j in 0..8 {
            a[col][j] *= inv;
        }
        b[col] *= inv;
        for r in 0..8 {
            if r == col {
                continue;
            }
            let factor = a[r][col];
            if factor == 0.0 {
                continue;
            }
            for j in 0..8 {
                a[r][j] -= factor * a[col][j];
            }
            b[r] -= factor * b[col];
        }
    }
    Some(b)
}
