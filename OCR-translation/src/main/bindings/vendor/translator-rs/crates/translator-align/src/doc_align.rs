use std::path::Path;
use std::sync::Mutex;

use mnn_sys::{ModuleEngine, NamedInput, NamedOutput, TensorData};

use crate::inference::{
    HAS_OBJ_OUTPUT_NAME, MODEL_INPUT_NAME, POINTS_OUTPUT_NAME, load_doc_align_engine,
};
use translator_core::api::{TranslatorError, TranslatorErrorKind};

const MODEL_INPUT_SIZE: usize = 256;

const CONFIDENCE_THRESHOLD: f32 = 0.75;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DocumentPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DocumentQuad {
    pub top_left: DocumentPoint,
    pub top_right: DocumentPoint,
    pub bottom_right: DocumentPoint,
    pub bottom_left: DocumentPoint,
}

impl DocumentQuad {
    pub fn corners(&self) -> [DocumentPoint; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }

    pub fn from_corners(corners: [DocumentPoint; 4]) -> Self {
        Self {
            top_left: corners[0],
            top_right: corners[1],
            bottom_right: corners[2],
            bottom_left: corners[3],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DocumentDetection {
    pub quad: DocumentQuad,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WarpedImageRgba {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct DocAligner {
    engine: Mutex<ModuleEngine>,
}

impl DocAligner {
    pub fn load(model_path: &Path, intra_threads: usize) -> Result<Self, TranslatorError> {
        let engine = load_doc_align_engine(model_path, intra_threads)?;
        Ok(Self {
            engine: Mutex::new(engine),
        })
    }

    /// Run DocAligner on `rgba` (tightly packed RGBA8, row-major, `width * height * 4` bytes).
    /// Returns `Ok(None)` when `has_obj` is below threshold (no document found). The returned
    /// corners are in original-image pixel coordinates.
    pub fn detect(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Option<DocumentDetection>, TranslatorError> {
        if width == 0 || height == 0 {
            return Err(TranslatorError::new(
                TranslatorErrorKind::InvalidInput,
                "zero-sized image",
            ));
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| {
                TranslatorError::new(
                    TranslatorErrorKind::InvalidInput,
                    "image dimensions overflow",
                )
            })?;
        if rgba.len() != expected {
            return Err(TranslatorError::new(
                TranslatorErrorKind::InvalidInput,
                format!(
                    "rgba length {} does not match {}x{}x4 = {}",
                    rgba.len(),
                    width,
                    height,
                    expected
                ),
            ));
        }

        let tensor_buf = preprocess_to_planar_rgb(rgba, width, height);
        let input_shape = [1usize, 3, MODEL_INPUT_SIZE, MODEL_INPUT_SIZE];

        let (points, confidence) = {
            let engine = self
                .engine
                .lock()
                .expect("doc-align session mutex poisoned");
            let outputs = engine
                .run_named_dynamic(
                    &[NamedInput {
                        name: MODEL_INPUT_NAME,
                        data: TensorData::F32(&tensor_buf),
                        shape: &input_shape,
                    }],
                    &[POINTS_OUTPUT_NAME, HAS_OBJ_OUTPUT_NAME],
                )
                .map_err(|error| {
                    TranslatorError::new(
                        TranslatorErrorKind::Internal,
                        format!("doc-align inference failed: {error}"),
                    )
                })?;

            let points = output_data(&outputs, POINTS_OUTPUT_NAME)?;
            let has_obj = output_data(&outputs, HAS_OBJ_OUTPUT_NAME)?;
            if points.len() < 8 || has_obj.is_empty() {
                return Err(TranslatorError::new(
                    TranslatorErrorKind::Internal,
                    "doc-align outputs have unexpected shape",
                ));
            }
            (
                [
                    points[0], points[1], points[2], points[3], points[4], points[5], points[6],
                    points[7],
                ],
                has_obj[0],
            )
        };
        if !(confidence > CONFIDENCE_THRESHOLD) {
            return Ok(None);
        }

        let w = width as f32;
        let h = height as f32;
        let corners = [
            DocumentPoint {
                x: points[0] * w,
                y: points[1] * h,
            },
            DocumentPoint {
                x: points[2] * w,
                y: points[3] * h,
            },
            DocumentPoint {
                x: points[4] * w,
                y: points[5] * h,
            },
            DocumentPoint {
                x: points[6] * w,
                y: points[7] * h,
            },
        ];
        Ok(Some(DocumentDetection {
            quad: DocumentQuad::from_corners(corners),
            confidence,
        }))
    }
}

fn output_data<'a>(outputs: &'a [NamedOutput], name: &str) -> Result<&'a [f32], TranslatorError> {
    outputs
        .iter()
        .find(|output| output.name == name)
        .map(|output| output.data.as_slice())
        .ok_or_else(|| {
            TranslatorError::new(
                TranslatorErrorKind::Internal,
                format!("doc-align output `{name}` missing"),
            )
        })
}

/// Map `rgba` (width×height) into the model's planar RGB f32 tensor (3×256×256, /255). The
/// aspect ratio is intentionally squeezed — DocAligner was trained that way and returns
/// corners in the same squeezed normalized space.
fn preprocess_to_planar_rgb(rgba: &[u8], width: u32, height: u32) -> Vec<f32> {
    let plane = MODEL_INPUT_SIZE * MODEL_INPUT_SIZE;
    let mut buf = vec![0.0f32; 3 * plane];
    let stride = (width as usize) * 4;
    let scale_x = width as f32 / MODEL_INPUT_SIZE as f32;
    let scale_y = height as f32 / MODEL_INPUT_SIZE as f32;
    for dst_y in 0..MODEL_INPUT_SIZE {
        // Nearest-neighbour sampling; we're collapsing aspect ratio anyway and the model
        // is tolerant. Skipping bilinear keeps preprocessing under a millisecond.
        let src_y = ((dst_y as f32 + 0.5) * scale_y) as usize;
        let src_y = src_y.min(height as usize - 1);
        for dst_x in 0..MODEL_INPUT_SIZE {
            let src_x = ((dst_x as f32 + 0.5) * scale_x) as usize;
            let src_x = src_x.min(width as usize - 1);
            let i = src_y * stride + src_x * 4;
            let r = rgba[i] as f32 / 255.0;
            let g = rgba[i + 1] as f32 / 255.0;
            let b = rgba[i + 2] as f32 / 255.0;
            let idx = dst_y * MODEL_INPUT_SIZE + dst_x;
            buf[idx] = r;
            buf[plane + idx] = g;
            buf[2 * plane + idx] = b;
        }
    }
    buf
}

/// Suggested output dimensions for a warp of `quad`: use the longer of opposing edge lengths.
pub fn suggested_output_dims(quad: &DocumentQuad) -> (u32, u32) {
    let c = quad.corners();
    let top = edge_len(c[0], c[1]);
    let bottom = edge_len(c[3], c[2]);
    let left = edge_len(c[0], c[3]);
    let right = edge_len(c[1], c[2]);
    let w = top.max(bottom).round().max(1.0) as u32;
    let h = left.max(right).round().max(1.0) as u32;
    (w, h)
}

fn edge_len(a: DocumentPoint, b: DocumentPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

/// Perspective-warp `rgba` so that `quad` maps to the full output rectangle (0,0)-(out_w,out_h).
/// When `postprocess` is true, apply CLAHE on the luma channel afterwards. Sampling: bilinear;
/// out-of-bounds source coords sample as transparent black. The output is RGBA8, opaque
/// (alpha=255) for in-bounds samples.
pub fn warp(
    rgba: &[u8],
    width: u32,
    height: u32,
    quad: &DocumentQuad,
    out_w: u32,
    out_h: u32,
    postprocess: bool,
) -> Result<WarpedImageRgba, TranslatorError> {
    let mut warped = warp_geometric(rgba, width, height, quad, out_w, out_h)?;
    if !postprocess {
        return Ok(warped);
    }
    apply_clahe(
        &mut warped.rgba,
        warped.width,
        warped.height,
        CLAHE_CLIP_LIMIT,
        CLAHE_TILES,
        CLAHE_TILES,
    );
    Ok(warped)
}

/// Pure geometric perspective warp; same as `warp` but without the CLAHE post-step. Exposed
/// only for tests of the geometric transform — production calls should use `warp` so the OCR
/// downstream sees a contrast-normalized image.
fn warp_geometric(
    rgba: &[u8],
    width: u32,
    height: u32,
    quad: &DocumentQuad,
    out_w: u32,
    out_h: u32,
) -> Result<WarpedImageRgba, TranslatorError> {
    if width == 0 || height == 0 || out_w == 0 || out_h == 0 {
        return Err(TranslatorError::new(
            TranslatorErrorKind::InvalidInput,
            "zero-sized image",
        ));
    }
    let expected = (width as usize) * (height as usize) * 4;
    if rgba.len() != expected {
        return Err(TranslatorError::new(
            TranslatorErrorKind::InvalidInput,
            format!(
                "rgba length {} does not match {}x{}x4 = {}",
                rgba.len(),
                width,
                height,
                expected
            ),
        ));
    }

    let c = quad.corners();
    let src = [
        (c[0].x, c[0].y),
        (c[1].x, c[1].y),
        (c[2].x, c[2].y),
        (c[3].x, c[3].y),
    ];
    let dst = [
        (0.0_f32, 0.0_f32),
        (out_w as f32, 0.0),
        (out_w as f32, out_h as f32),
        (0.0, out_h as f32),
    ];
    // We solve homography H s.t. H * dst = src — i.e. the inverse map, so for each output
    // pixel (u,v) we get the source coord (x,y) = H * (u,v,1). This avoids a separate
    // matrix inversion and lets us sample once per output pixel.
    let h = solve_homography(&dst, &src).ok_or_else(|| {
        TranslatorError::new(
            TranslatorErrorKind::InvalidInput,
            "degenerate quadrilateral — cannot warp",
        )
    })?;

    let stride = (width as usize) * 4;
    let max_x = width as f32 - 1.0;
    let max_y = height as f32 - 1.0;
    let mut out = vec![0u8; (out_w as usize) * (out_h as usize) * 4];
    for v in 0..out_h {
        for u in 0..out_w {
            let uf = u as f32 + 0.5;
            let vf = v as f32 + 0.5;
            let denom = h[6] * uf + h[7] * vf + h[8];
            if denom.abs() < 1e-9 {
                continue;
            }
            let sx = (h[0] * uf + h[1] * vf + h[2]) / denom;
            let sy = (h[3] * uf + h[4] * vf + h[5]) / denom;
            if sx < 0.0 || sy < 0.0 || sx > max_x || sy > max_y {
                continue;
            }
            let x0 = sx.floor();
            let y0 = sy.floor();
            let dx = sx - x0;
            let dy = sy - y0;
            let x0i = x0 as usize;
            let y0i = y0 as usize;
            let x1i = (x0i + 1).min(width as usize - 1);
            let y1i = (y0i + 1).min(height as usize - 1);

            let p00 = &rgba[y0i * stride + x0i * 4..][..3];
            let p10 = &rgba[y0i * stride + x1i * 4..][..3];
            let p01 = &rgba[y1i * stride + x0i * 4..][..3];
            let p11 = &rgba[y1i * stride + x1i * 4..][..3];

            let w00 = (1.0 - dx) * (1.0 - dy);
            let w10 = dx * (1.0 - dy);
            let w01 = (1.0 - dx) * dy;
            let w11 = dx * dy;

            let r = p00[0] as f32 * w00
                + p10[0] as f32 * w10
                + p01[0] as f32 * w01
                + p11[0] as f32 * w11;
            let g = p00[1] as f32 * w00
                + p10[1] as f32 * w10
                + p01[1] as f32 * w01
                + p11[1] as f32 * w11;
            let b = p00[2] as f32 * w00
                + p10[2] as f32 * w10
                + p01[2] as f32 * w01
                + p11[2] as f32 * w11;

            let oi = ((v as usize) * (out_w as usize) + (u as usize)) * 4;
            out[oi] = r as u8;
            out[oi + 1] = g as u8;
            out[oi + 2] = b as u8;
            out[oi + 3] = 255;
        }
    }
    Ok(WarpedImageRgba {
        rgba: out,
        width: out_w,
        height: out_h,
    })
}

pub(crate) const CLAHE_CLIP_LIMIT: f32 = 2.0;
pub(crate) const CLAHE_TILES: u32 = 8;

/// Contrast Limited Adaptive Histogram Equalization on the luma channel. Operates in place on an
/// RGBA8 buffer (`width * height * 4` bytes). Converts each pixel to YCbCr (BT.601), equalizes Y
/// per [tiles_x × tiles_y] tile with bilinear interpolation between adjacent tile LUTs, and
/// recombines with the original chroma so colour is preserved. Doc-align outputs are real photos
/// with uneven lighting; CLAHE flattens that so OCR sees consistent local contrast.
pub(crate) fn apply_clahe(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    clip_limit: f32,
    tiles_x: u32,
    tiles_y: u32,
) {
    if width < tiles_x || height < tiles_y {
        log::info!(
            "apply_clahe: skipping (image {}x{} smaller than {}x{} tile grid)",
            width,
            height,
            tiles_x,
            tiles_y
        );
        return;
    }
    let w = width as usize;
    let h = height as usize;
    let tx = tiles_x as usize;
    let ty = tiles_y as usize;

    // Per-pixel luma + chroma planes. Chroma stays as-is; luma is what CLAHE rewrites.
    let pixels = w * h;
    let mut luma = vec![0u8; pixels];
    let mut cb = vec![0u8; pixels];
    let mut cr = vec![0u8; pixels];
    let mut luma_sum_before: u64 = 0;
    for i in 0..pixels {
        let r = rgba[i * 4] as f32;
        let g = rgba[i * 4 + 1] as f32;
        let b = rgba[i * 4 + 2] as f32;
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let cb_v = -0.168736 * r - 0.331264 * g + 0.5 * b + 128.0;
        let cr_v = 0.5 * r - 0.418688 * g - 0.081312 * b + 128.0;
        let yi = y.round().clamp(0.0, 255.0) as u8;
        luma[i] = yi;
        cb[i] = cb_v.round().clamp(0.0, 255.0) as u8;
        cr[i] = cr_v.round().clamp(0.0, 255.0) as u8;
        luma_sum_before += yi as u64;
    }
    let mean_before = luma_sum_before as f64 / pixels as f64;

    // Per-tile LUT: 256 entries mapping old luma → new luma.
    let mut tile_luts = vec![[0u8; 256]; tx * ty];
    for ty_i in 0..ty {
        let row_start = ty_i * h / ty;
        let row_end = (ty_i + 1) * h / ty;
        for tx_i in 0..tx {
            let col_start = tx_i * w / tx;
            let col_end = (tx_i + 1) * w / tx;
            let tile_pixels = (row_end - row_start) * (col_end - col_start);
            if tile_pixels == 0 {
                continue;
            }

            let mut hist = [0u32; 256];
            for y in row_start..row_end {
                let off = y * w;
                for x in col_start..col_end {
                    hist[luma[off + x] as usize] += 1;
                }
            }

            // Clip histogram bins at clip_limit × (tile_pixels / 256) and redistribute the
            // clipped excess uniformly across all bins. This is the "contrast limited" step —
            // without it, AHE blows up noise in uniform regions.
            let clip_value = ((clip_limit * tile_pixels as f32) / 256.0).max(1.0) as u32;
            let mut excess: u32 = 0;
            for v in hist.iter_mut() {
                if *v > clip_value {
                    excess += *v - clip_value;
                    *v = clip_value;
                }
            }
            let bonus = excess / 256;
            let remainder = (excess % 256) as usize;
            for (i, v) in hist.iter_mut().enumerate() {
                *v += bonus + if i < remainder { 1 } else { 0 };
            }

            let mut cdf: u32 = 0;
            let lut = &mut tile_luts[ty_i * tx + tx_i];
            for (i, v) in hist.iter().enumerate() {
                cdf += *v;
                lut[i] = ((cdf as u64 * 255 + (tile_pixels as u64 / 2)) / tile_pixels as u64) as u8;
            }
        }
    }

    // Each pixel: find the 4 surrounding tile centres and bilinearly interpolate between their
    // LUT outputs. Tile centres sit at ((col+0.5) * w/tx, (row+0.5) * h/ty).
    let tile_w = w as f32 / tx as f32;
    let tile_h = h as f32 / ty as f32;
    let mut luma_sum_after: u64 = 0;
    for y in 0..h {
        let fy = (y as f32 + 0.5) / tile_h - 0.5;
        let ty0 = fy.floor() as isize;
        let ty1 = ty0 + 1;
        let wy = fy - ty0 as f32;
        let ty0c = ty0.clamp(0, ty as isize - 1) as usize;
        let ty1c = ty1.clamp(0, ty as isize - 1) as usize;
        for x in 0..w {
            let fx = (x as f32 + 0.5) / tile_w - 0.5;
            let tx0 = fx.floor() as isize;
            let tx1 = tx0 + 1;
            let wx = fx - tx0 as f32;
            let tx0c = tx0.clamp(0, tx as isize - 1) as usize;
            let tx1c = tx1.clamp(0, tx as isize - 1) as usize;

            let i = y * w + x;
            let v = luma[i] as usize;
            let v00 = tile_luts[ty0c * tx + tx0c][v] as f32;
            let v10 = tile_luts[ty0c * tx + tx1c][v] as f32;
            let v01 = tile_luts[ty1c * tx + tx0c][v] as f32;
            let v11 = tile_luts[ty1c * tx + tx1c][v] as f32;
            let new_y = (1.0 - wx) * (1.0 - wy) * v00
                + wx * (1.0 - wy) * v10
                + (1.0 - wx) * wy * v01
                + wx * wy * v11;
            luma_sum_after += new_y.round().clamp(0.0, 255.0) as u64;

            let cb_v = cb[i] as f32 - 128.0;
            let cr_v = cr[i] as f32 - 128.0;
            let r = new_y + 1.402 * cr_v;
            let g = new_y - 0.344136 * cb_v - 0.714136 * cr_v;
            let b = new_y + 1.772 * cb_v;
            rgba[i * 4] = r.round().clamp(0.0, 255.0) as u8;
            rgba[i * 4 + 1] = g.round().clamp(0.0, 255.0) as u8;
            rgba[i * 4 + 2] = b.round().clamp(0.0, 255.0) as u8;
            // alpha untouched
        }
    }
    let mean_after = luma_sum_after as f64 / pixels as f64;
    log::info!(
        "apply_clahe: {}x{} tiles={}x{} clip={:.2} mean_luma {:.1} -> {:.1}",
        width,
        height,
        tiles_x,
        tiles_y,
        clip_limit,
        mean_before,
        mean_after,
    );
}

/// Solve the 3×3 homography H (returned as 9 floats, row-major, h[8] forced to 1) that maps
/// each source point `src[i]` to `dst[i]`. Returns `None` if the linear system is singular.
fn solve_homography(src: &[(f32, f32); 4], dst: &[(f32, f32); 4]) -> Option<[f32; 9]> {
    // Each correspondence yields 2 equations:
    //   x' = (h0 x + h1 y + h2) / (h6 x + h7 y + 1)
    //   y' = (h3 x + h4 y + h5) / (h6 x + h7 y + 1)
    // Linearized:
    //   h0 x + h1 y + h2                     - h6 x x' - h7 y x' = x'
    //                     h3 x + h4 y + h5   - h6 x y' - h7 y y' = y'
    let mut a = [[0.0f64; 8]; 8];
    let mut b = [0.0f64; 8];
    for i in 0..4 {
        let (x, y) = (src[i].0 as f64, src[i].1 as f64);
        let (xp, yp) = (dst[i].0 as f64, dst[i].1 as f64);
        let r0 = i * 2;
        let r1 = r0 + 1;
        a[r0] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * xp, -y * xp];
        b[r0] = xp;
        a[r1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * yp, -y * yp];
        b[r1] = yp;
    }
    let sol = solve_linear_8(a, b)?;
    Some([
        sol[0] as f32,
        sol[1] as f32,
        sol[2] as f32,
        sol[3] as f32,
        sol[4] as f32,
        sol[5] as f32,
        sol[6] as f32,
        sol[7] as f32,
        1.0,
    ])
}

fn solve_linear_8(mut a: [[f64; 8]; 8], mut b: [f64; 8]) -> Option<[f64; 8]> {
    // Gaussian elimination with partial pivoting. Small fixed-size; allocating routines from
    // a linalg crate would be heavier than the problem.
    for col in 0..8 {
        let mut pivot = col;
        let mut pivot_abs = a[col][col].abs();
        for row in (col + 1)..8 {
            let v = a[row][col].abs();
            if v > pivot_abs {
                pivot = row;
                pivot_abs = v;
            }
        }
        if pivot_abs < 1e-12 {
            return None;
        }
        if pivot != col {
            a.swap(col, pivot);
            b.swap(col, pivot);
        }
        let inv = 1.0 / a[col][col];
        for row in (col + 1)..8 {
            let factor = a[row][col] * inv;
            if factor == 0.0 {
                continue;
            }
            for k in col..8 {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = [0.0f64; 8];
    for row in (0..8).rev() {
        let mut sum = b[row];
        for col in (row + 1)..8 {
            sum -= a[row][col] * x[col];
        }
        x[row] = sum / a[row][row];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warp_geometric_constant_image_preserves_color() {
        let rgba: Vec<u8> = vec![[120u8, 60, 200, 255]; 8 * 8]
            .into_iter()
            .flatten()
            .collect();
        let quad = DocumentQuad {
            top_left: DocumentPoint { x: 1.0, y: 1.0 },
            top_right: DocumentPoint { x: 7.0, y: 0.5 },
            bottom_right: DocumentPoint { x: 6.5, y: 7.0 },
            bottom_left: DocumentPoint { x: 0.5, y: 6.5 },
        };
        let warped = warp_geometric(&rgba, 8, 8, &quad, 12, 12).unwrap();
        assert_eq!(warped.width, 12);
        assert_eq!(warped.height, 12);
        let center = (6 * 12 + 6) * 4;
        assert_eq!(&warped.rgba[center..center + 4], &[120, 60, 200, 255]);
    }

    #[test]
    fn warp_geometric_axis_aligned_quad_samples_correct_pixels() {
        let mut rgba = vec![0u8; 4 * 4 * 4];
        for y in 0..4 {
            for x in 0..4 {
                let i = (y * 4 + x) * 4;
                rgba[i] = (x * 60) as u8;
                rgba[i + 1] = (y * 60) as u8;
                rgba[i + 2] = 0;
                rgba[i + 3] = 255;
            }
        }
        let quad = DocumentQuad {
            top_left: DocumentPoint { x: 1.0, y: 1.0 },
            top_right: DocumentPoint { x: 3.0, y: 1.0 },
            bottom_right: DocumentPoint { x: 3.0, y: 3.0 },
            bottom_left: DocumentPoint { x: 1.0, y: 3.0 },
        };
        let warped = warp_geometric(&rgba, 4, 4, &quad, 2, 2).unwrap();
        assert_eq!(warped.width, 2);
        assert_eq!(warped.height, 2);
        let center = warped.rgba[(0 * 2 + 0) * 4];
        assert!(
            center >= 60,
            "top-left output should sample around x=1.5, got R={center}"
        );
    }

    #[test]
    fn clahe_clip_caps_extreme_dark_image_brightening() {
        // Property test: CLAHE never blows up a uniform image because the clip-limit caps the
        // gain. A near-uniform dark image should *not* be remapped to bright values.
        let w = 200u32;
        let h = 200u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for i in 0..(w * h) as usize {
            rgba[i * 4] = 30;
            rgba[i * 4 + 1] = 30;
            rgba[i * 4 + 2] = 30;
            rgba[i * 4 + 3] = 255;
        }
        apply_clahe(&mut rgba, w, h, 2.0, 8, 8);
        let center = rgba[((h / 2) * w * 4) as usize] as i32;
        assert!(
            center < 80,
            "Uniform-dark image should stay dark after CLAHE; got {center}",
        );
    }

    #[test]
    fn clahe_preserves_chroma_for_grayscale_input() {
        // An originally-gray image should stay gray (R == G == B) after CLAHE; the chroma
        // round-trip must not introduce colour casts.
        let w = 64u32;
        let h = 64u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let v = 30 + ((y * w + x) % 200) as u8;
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = v;
                rgba[i + 1] = v;
                rgba[i + 2] = v;
                rgba[i + 3] = 255;
            }
        }
        apply_clahe(&mut rgba, w, h, 2.0, 8, 8);
        for i in 0..(w * h) as usize {
            let r = rgba[i * 4] as i32;
            let g = rgba[i * 4 + 1] as i32;
            let b = rgba[i * 4 + 2] as i32;
            assert!(
                (r - g).abs() <= 2 && (g - b).abs() <= 2,
                "channels diverged at {i}: r={r} g={g} b={b}"
            );
        }
    }

    #[test]
    fn suggested_output_dims_uses_longest_edges() {
        let quad = DocumentQuad {
            top_left: DocumentPoint { x: 0.0, y: 0.0 },
            top_right: DocumentPoint { x: 100.0, y: 0.0 },
            bottom_right: DocumentPoint { x: 100.0, y: 50.0 },
            bottom_left: DocumentPoint { x: 0.0, y: 50.0 },
        };
        assert_eq!(suggested_output_dims(&quad), (100, 50));
    }
}
