use translator_core::catalog::PpocrScript;
use translator_core::coords::Quadrant;
use translator_core::ocr::{DetectedTextBox, RecognizedTextLine};
use translator_raster::live_frame::OrientedImage;

pub fn estimate_canonical_quadrant_with(
    ppocr: &crate::ppocr::PpocrEngine,
    oriented: &OrientedImage,
    boxes: &[DetectedTextBox],
) -> Option<Quadrant> {
    let rgb = oriented
        .rgb
        .as_ref()
        .expect("estimate path requires build_with_rgb");
    let gray_display = image::imageops::grayscale(&rgb.to_rgb8());
    let scaled = oriented.rec_scaled_boxes(boxes);
    estimate_canonical_quadrant(ppocr, rgb, &gray_display, &scaled)
}

/// Estimate the scene's reading-direction quadrant from a freshly-detected
/// set of text boxes. Runs the textline orientation classifier twice per
/// candidate strip (raw + CW90) so the binary 0°/180° model resolves the
/// full 90° quadrant; per-strip votes are aggregated with confidence,
/// quorum, and majority gates. Returns `None` if the classifier model
/// isn't installed, no wide strips qualified, or consensus didn't reach
/// the gate.
///
/// Pure relative to its arguments: no engine state is mutated. The caller
/// (the platform acquire path) feeds the result into
/// `LivePlanarEngine::acquire_now_with_orientation`.
/// Rec-based orientation estimator. Picks the quadrant that produces the
/// highest average recognition confidence across K sample detections, by
/// running the script-specific recognizer at each of the four candidate
/// canonicals and keeping the winner.
///
/// Why this beats the textline-ori classifier on hard / OOD content:
/// rec is a CTC decoder over a real character vocabulary, so it lights
/// up when the orientation is right and emits low-conf character salad
/// when it isn't — the conf gap is a strong, well-calibrated signal.
/// The textline-ori model is binary with a class prior; on out-of-
/// distribution inputs it falls back to the prior regardless of the
/// pixels, giving us the "Up vs Up @0.999" pathology.
///
/// Cost: 2 × K rec invocations (≈ K boxes × 2 sides of the scene's
/// dominant axis) — typically O(150-250 ms) at K=3, comparable to the
/// windowed textline-ori path while being far more accurate.
///
/// Returns `None` when:
/// - `script` is unknown at the call site (auto-source mode — caller
///   should fall back to the textline-ori path).
/// - no detections qualify as samples (too few, low conf, no contour).
/// - the winning canonical doesn't beat the loser by `MIN_REC_CONF_MARGIN`.
pub fn estimate_canonical_via_rec(
    engine: &crate::ppocr::PpocrEngine,
    oriented: &OrientedImage,
    boxes: &[DetectedTextBox],
    script: PpocrScript,
) -> Option<Quadrant> {
    use crate::ppocr::PpocrProfile;

    const MIN_DET_SCORE: f32 = 0.5;
    const MIN_LONG_PX: f32 = 40.0;
    const K_SAMPLES: usize = 3;
    /// Winner's avg rec confidence must beat the runner-up's by this
    /// margin to be trusted. Small but real — if all 4 canonicals are
    /// close, content is genuinely orientation-ambiguous (numerals,
    /// symmetric glyphs, single-character boxes).
    const MIN_REC_CONF_MARGIN: f32 = 0.05;
    /// Absolute floor on the winner. Without this, low-quality content
    /// where rec is bad in every direction could still elect a
    /// canonical purely on noise.
    const MIN_WINNER_CONF: f32 = 0.30;

    if boxes.is_empty() {
        return None;
    }
    let rgb = oriented.rgb.as_ref()?;

    // Pick top-K by det score, filtered for size + presence of contour.
    let mut sample_indices: Vec<(usize, f32)> = boxes
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            if b.score < MIN_DET_SCORE {
                return None;
            }
            if b.contour.is_empty() || b.contour.len() % 2 != 0 {
                return None;
            }
            let w = b.rect.right.saturating_sub(b.rect.left) as f32;
            let h = b.rect.bottom.saturating_sub(b.rect.top) as f32;
            let long = w.max(h);
            if long < MIN_LONG_PX {
                return None;
            }
            Some((i, b.score * long))
        })
        .collect();
    sample_indices
        .sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sample_indices.truncate(K_SAMPLES);
    if sample_indices.is_empty() {
        log::info!(
            "[orient-rec] no qualifying samples among {} boxes",
            boxes.len()
        );
        return None;
    }
    // Scale to the rec-source resolution: these samples are both PCA-analyzed
    // (angle is scale-invariant) and cropped from `oriented.rgb` (rec-res).
    let samples: Vec<DetectedTextBox> = oriented.rec_scaled_boxes(
        &sample_indices
            .iter()
            .map(|(i, _)| boxes[*i].clone())
            .collect::<Vec<_>>(),
    );
    let scripts = vec![script; samples.len()];

    // The dewarp's canonical parameter is a sign-flip on the PCA axis,
    // not an absolute-direction rotation. With (ux, uy) ≈ (cos θ_pca,
    // sin θ_pca), the four cardinal canonicals collapse to two distinct
    // outputs depending on the sign of ux*cos(Q) + uy*sin(Q). So we only
    // need ONE pair of opposite canonicals on the axis the scene's PCA
    // picks. Two rec calls per acquire, not four.
    //
    // Axis selection: average the sample boxes' PCA angles via the
    // doubled-angle trick (Mardia's circular mean for axial data),
    // giving a stable scene axis under per-box tilt. If the resulting
    // axis is closer to horizontal, test R0 vs R180; if vertical, test
    // R90 vs R270.
    let mut axis_cos2 = 0.0f32;
    let mut axis_sin2 = 0.0f32;
    let mut axis_count = 0usize;
    for b in &samples {
        let contour: Vec<(f32, f32)> = b.contour.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        if let Some(theta) = crate::ppocr::contour_principal_axis_angle(&contour) {
            axis_cos2 += (2.0 * theta).cos();
            axis_sin2 += (2.0 * theta).sin();
            axis_count += 1;
        }
    }
    if axis_count == 0 {
        return None;
    }
    let axis_theta = 0.5 * axis_sin2.atan2(axis_cos2); // ∈ [-π/2, π/2]
    let (canon_a, canon_b) = if axis_theta.cos().abs() >= axis_theta.sin().abs() {
        (Quadrant::R0, Quadrant::R180)
    } else {
        (Quadrant::R90, Quadrant::R270)
    };
    let axis_label = if matches!(canon_a, Quadrant::R0) {
        "horizontal"
    } else {
        "vertical"
    };

    let t_start = std::time::Instant::now();
    let mut sides: [(Quadrant, f32, Vec<RecognizedTextLine>); 2] =
        [(canon_a, 0.0, Vec::new()), (canon_b, 0.0, Vec::new())];
    for slot in sides.iter_mut() {
        let canon = slot.0;
        match engine.recognize_text_in_boxes_image(
            rgb,
            &samples,
            &scripts,
            PpocrProfile::Live,
            Some(canon),
        ) {
            Ok(lines) => {
                let conf_sum: f32 = lines.iter().map(|l| l.confidence).sum();
                let conf_avg = if lines.is_empty() {
                    0.0
                } else {
                    conf_sum / lines.len() as f32
                };
                // Truncate each rec text to keep logcat readable on
                // long sentences. Enough characters to tell "real text"
                // from "empty / gibberish" at a glance, no more.
                const LOG_TEXT_MAX_CHARS: usize = 30;
                let texts: Vec<String> = lines
                    .iter()
                    .map(|l| {
                        let t = l.text.trim();
                        if t.chars().count() <= LOG_TEXT_MAX_CHARS {
                            t.to_string()
                        } else {
                            let truncated: String = t.chars().take(LOG_TEXT_MAX_CHARS).collect();
                            format!("{truncated}…")
                        }
                    })
                    .collect();
                log::info!(
                    "[orient-rec canonical={:?}] avg_conf={:.3} n={} texts={:?}",
                    canon,
                    conf_avg,
                    lines.len(),
                    texts,
                );
                slot.1 = conf_avg;
                slot.2 = lines;
            }
            Err(e) => {
                log::debug!("[orient-rec canonical={:?}] recognize failed: {e:?}", canon);
            }
        }
    }
    let elapsed_ms = t_start.elapsed().as_secs_f64() * 1000.0;

    let (win_idx, loser_idx) = if sides[0].1 >= sides[1].1 {
        (0, 1)
    } else {
        (1, 0)
    };
    let win_conf = sides[win_idx].1;
    let loser_conf = sides[loser_idx].1;
    let margin = win_conf - loser_conf;
    let winner = sides[win_idx].0;

    log::info!(
        "[orient-rec] {:.1}ms K={} axis={} ({:?}@{:.3} vs {:?}@{:.3}) → winner={:?} margin={:.3}",
        elapsed_ms,
        samples.len(),
        axis_label,
        sides[0].0,
        sides[0].1,
        sides[1].0,
        sides[1].1,
        winner,
        margin,
    );

    if win_conf < MIN_WINNER_CONF {
        log::info!(
            "[orient-rec] winner conf {:.3} below floor {:.3} — falling back to default",
            win_conf,
            MIN_WINNER_CONF,
        );
        return None;
    }
    if margin < MIN_REC_CONF_MARGIN {
        log::info!(
            "[orient-rec] margin {:.3} below floor {:.3} — content orientation-symmetric, falling back",
            margin,
            MIN_REC_CONF_MARGIN,
        );
        return None;
    }

    Some(winner)
}

/// Original textline-ori-based estimator, kept for the auto-source path
/// (where we don't know the script up-front and can't run rec).
/// Geometric reading-direction quadrant from detected-box contour angles —
/// no recognizer pass. Picks the dominant principal axis (horizontal vs
/// vertical, length-weighted) and assumes world-up: horizontal → R0, vertical
/// → R90. Used by the screen pipeline, which is always world-up but may be
/// captured in landscape; the rec-based 180° disambiguation that
/// [`estimate_canonical_quadrant`] adds is skipped for speed.
pub fn dominant_axis_quadrant(boxes: &[DetectedTextBox]) -> Quadrant {
    use crate::ppocr::contour_principal_axis_angle;
    let mut horizontal = 0.0f32;
    let mut vertical = 0.0f32;
    for b in boxes {
        if b.contour.is_empty() || b.contour.len() % 2 != 0 {
            continue;
        }
        let contour: Vec<(f32, f32)> = b.contour.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        let Some(theta) = contour_principal_axis_angle(&contour) else {
            continue;
        };
        let w = b.rect.right.saturating_sub(b.rect.left) as f32;
        let h = b.rect.bottom.saturating_sub(b.rect.top) as f32;
        let weight = w.max(h);
        // θ ∈ [0, π): |sin θ| is 0 for a horizontal axis, 1 for vertical; split
        // at 45°.
        if theta.sin().abs() < std::f32::consts::FRAC_1_SQRT_2 {
            horizontal += weight;
        } else {
            vertical += weight;
        }
    }
    if vertical > horizontal {
        Quadrant::R90
    } else {
        Quadrant::R0
    }
}

pub fn estimate_canonical_quadrant(
    engine: &crate::ppocr::PpocrEngine,
    image: &image::DynamicImage,
    gray: &image::GrayImage,
    boxes: &[DetectedTextBox],
) -> Option<Quadrant> {
    use crate::ppocr::{TextlineOriLabel, contour_principal_axis_angle, dewarp_contour_to_strip};

    // Deskew-and-classify pipeline. For each detected box we PCA the
    // contour to get its principal-axis angle θ ∈ [0, π), dewarp the
    // strip so the text becomes axis-aligned in a tight rectangle,
    // and only then feed it to the (binary, horizontal-only) textline-
    // ori model. The classifier then resolves the ±x ambiguity. World
    // reading direction = θ (Up) or θ + π (Flipped180).
    //
    // This replaces the older Wide/Tall aspect-gate hack which dropped
    // any non-axis-aligned tilt on the floor and crushed the classifier
    // input. The deskew gives the model the kind of input it was
    // trained on; the validation-pair gate below filters out the
    // model's class-prior bias on hard content.
    const MIN_LONG_PX: f32 = 40.0;
    const MIN_DET_SCORE: f32 = 0.5;
    const TOP_N: usize = 10;
    // Must match the textline-ori model's native input size
    // (TEXTLINE_ORI_WIDTH/HEIGHT in ppocr.rs).
    const WINDOW_W: u32 = 160;
    const WINDOW_H: u32 = 80;
    const MAX_WINDOWS_PER_STRIP: u32 = 3;
    // The asymmetry gate: Δ = P(Up|raw) − P(Up|raw180). An unbiased
    // classifier on a 180°-rotated copy of the same input MUST give
    // the opposite probability, so |Δ| ≈ 1 is the ideal signal. A
    // biased classifier hallucinates the same label on both pair
    // members → Δ ≈ 0. Anything below this threshold is treated as
    // "model can't discriminate" and dropped. Raw180's absolute
    // confidence is intentionally ignored — what matters is that the
    // model's answer *changes* under a 180° rotation, not that the
    // rotated input also crosses some absolute confidence bar.
    const MIN_ASYMMETRY: f32 = 0.3;
    const MIN_VOTES: usize = 2;
    const MAJORITY_RATIO: f32 = 0.7;

    if !engine.has_textline_orientation() || boxes.is_empty() {
        return None;
    }
    let _ = image; // dewarp samples `gray`; the RGB copy isn't used here.

    let mut selected: Vec<(usize, f32)> = boxes
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            if b.score < MIN_DET_SCORE {
                return None;
            }
            if b.contour.is_empty() || b.contour.len() % 2 != 0 {
                return None;
            }
            let w = b.rect.right.saturating_sub(b.rect.left) as f32;
            let h = b.rect.bottom.saturating_sub(b.rect.top) as f32;
            let long = w.max(h);
            if long < MIN_LONG_PX {
                return None;
            }
            Some((i, long))
        })
        .collect();
    selected.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    selected.truncate(TOP_N);
    if selected.is_empty() {
        log::info!(
            "[estimate_canonical_quadrant] no contour-qualified detections among {} boxes",
            boxes.len()
        );
        return None;
    }

    // Per box: PCA → θ, dewarp to axis-aligned RGB strip, H-normalize
    // to WINDOW_H tall, slice up to MAX_WINDOWS_PER_STRIP windows.
    let mut windows: Vec<image::DynamicImage> = Vec::new();
    // (box_idx, θ, window_index_within_strip)
    let mut window_meta: Vec<(usize, f32, u32)> = Vec::new();
    for &(i, _) in &selected {
        let b = &boxes[i];
        let contour: Vec<(f32, f32)> = b.contour.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        let theta = match contour_principal_axis_angle(&contour) {
            Some(t) => t,
            None => continue,
        };
        let deskewed_luma = match dewarp_contour_to_strip(gray, &contour, None, 0.0) {
            Some(s) => s,
            None => continue,
        };
        let strip = image::DynamicImage::ImageLuma8(deskewed_luma);
        let ow = strip.width();
        let oh = strip.height();
        if oh == 0 || ow == 0 {
            continue;
        }
        let scale = WINDOW_H as f32 / oh as f32;
        let new_w = ((ow as f32) * scale).round().max(1.0) as u32;
        let normalized = strip.resize_exact(new_w, WINDOW_H, image::imageops::FilterType::Triangle);
        if new_w <= WINDOW_W {
            // Strip too short for a native-scale window. Fall back to
            // a single padded sample — preprocess_for_textline_ori
            // will resize_exact to WINDOW_W (only stretching the
            // narrow direction, no aspect crush of glyphs).
            windows.push(normalized);
            window_meta.push((i, theta, 0));
            continue;
        }
        let span = new_w - WINDOW_W;
        let n = (new_w / WINDOW_W).clamp(1, MAX_WINDOWS_PER_STRIP);
        for k in 0..n {
            let x_start = if n == 1 {
                span / 2
            } else {
                (span * k) / (n - 1)
            };
            let window = normalized.crop_imm(x_start, 0, WINDOW_W, WINDOW_H);
            windows.push(window);
            window_meta.push((i, theta, k));
        }
    }

    if windows.is_empty() {
        return None;
    }
    log::info!(
        "[estimate_canonical_quadrant] {} boxes → {} windows",
        selected.len(),
        windows.len(),
    );

    // Validation-pair classification: classify each window in its
    // original orientation AND rotated 180°. An unbiased classifier
    // MUST give opposite labels on the pair (if raw is Up, raw180 is
    // Flipped180, and vice versa). On hard content (sparse text,
    // big-glyph book covers, etc.) the binary textline-ori model has a
    // strong "Up" bias and will call both pair members Up — those
    // votes are unreliable and get discarded. Doubles the eval count
    // but the model is tiny (160×80) and this is the only place we
    // get pixel-orientation evidence in the live pipeline; cheaper
    // than tightening thresholds and rejecting everything.
    let windows_180: Vec<image::DynamicImage> = windows.iter().map(|w| w.rotate180()).collect();
    let labels = match engine.textline_orientation_classify(&windows) {
        Ok(v) => v,
        Err(e) => {
            log::debug!("[estimate_canonical_quadrant] classify(raw) failed: {e:?}");
            return None;
        }
    };
    let labels_180 = match engine.textline_orientation_classify(&windows_180) {
        Ok(v) => v,
        Err(e) => {
            log::debug!("[estimate_canonical_quadrant] classify(180) failed: {e:?}");
            return None;
        }
    };

    // Per-window vote via asymmetry. The strip was deskewed by -θ so
    // the classifier sees axis-aligned text; the only remaining
    // ambiguity is ±x reading direction, which the validation pair
    // resolves. Decision rule:
    //   Δ = P(Up | raw) − P(Up | raw180)
    //   Δ ≥ +MIN_ASYMMETRY  → vote "Up"-side  (world angle = θ)
    //   Δ ≤ -MIN_ASYMMETRY  → vote "Flipped"-side (world angle = θ + π)
    //   |Δ| < MIN_ASYMMETRY → bias-reject (model says same thing on
    //                         both pair members → no discrimination)
    //
    // |Δ| is also the vote weight — strong asymmetry = trustworthy
    // vote, weak asymmetry just barely clears the gate.
    fn p_up(c: &crate::ppocr::TextlineOriCandidate) -> f32 {
        match c.label {
            TextlineOriLabel::Up => c.score,
            TextlineOriLabel::Flipped180 => 1.0 - c.score,
        }
    }
    let mut votes: Vec<(Quadrant, f32)> = Vec::with_capacity(windows.len());
    for (idx, ((cand, cand180), &(box_idx, theta, win_k))) in labels
        .iter()
        .zip(labels_180.iter())
        .zip(window_meta.iter())
        .enumerate()
    {
        let fmt = |c: &Option<crate::ppocr::TextlineOriCandidate>| match c {
            Some(c) => format!("{:?}@{:.3}", c.label, c.score),
            None => "—".to_string(),
        };
        let theta_deg = theta.to_degrees();
        let (raw, raw180) = match (cand, cand180) {
            (Some(r), Some(r180)) => (r, r180),
            _ => {
                log::info!(
                    "[orient-window {}] box={} θ={:.1}° w{} raw={} raw180={} → rejected (missing)",
                    idx,
                    box_idx,
                    theta_deg,
                    win_k,
                    fmt(cand),
                    fmt(cand180)
                );
                continue;
            }
        };
        let delta = p_up(raw) - p_up(raw180);
        if delta.abs() < MIN_ASYMMETRY {
            log::info!(
                "[orient-window {}] box={} θ={:.1}° w{} raw={} raw180={} → rejected (Δ={:+.3}, model bias)",
                idx,
                box_idx,
                theta_deg,
                win_k,
                fmt(cand),
                fmt(cand180),
                delta
            );
            continue;
        }
        let world_angle = if delta > 0.0 {
            theta
        } else {
            theta + std::f32::consts::PI
        };
        let q = Quadrant::from_radians(world_angle);
        log::info!(
            "[orient-window {}] box={} θ={:.1}° w{} raw={} raw180={} Δ={:+.3} → vote {:?}",
            idx,
            box_idx,
            theta_deg,
            win_k,
            fmt(cand),
            fmt(cand180),
            delta,
            q
        );
        votes.push((q, delta.abs()));
    }

    if votes.len() < MIN_VOTES {
        log::info!(
            "[estimate_canonical_quadrant] only {} confident votes (need {}) — falling back to default",
            votes.len(),
            MIN_VOTES
        );
        return None;
    }

    let mut counts: [(Quadrant, f32, usize); 4] = [
        (Quadrant::R0, 0.0, 0),
        (Quadrant::R90, 0.0, 0),
        (Quadrant::R180, 0.0, 0),
        (Quadrant::R270, 0.0, 0),
    ];
    for (q, conf) in &votes {
        let idx = match q {
            Quadrant::R0 => 0,
            Quadrant::R90 => 1,
            Quadrant::R180 => 2,
            Quadrant::R270 => 3,
        };
        counts[idx].1 += conf;
        counts[idx].2 += 1;
    }
    // Tally diagnostic: every quadrant's count + avg conf, in a stable order.
    let tally_fmt = counts
        .iter()
        .map(|(q, sum, n)| {
            let avg = if *n > 0 { sum / *n as f32 } else { 0.0 };
            format!("{:?}={}@{:.3}", q, n, avg)
        })
        .collect::<Vec<_>>()
        .join(" ");
    log::info!("[orient-tally] votes={} {}", votes.len(), tally_fmt);

    let (winner, sum_conf, winner_count) = counts.iter().copied().max_by_key(|c| c.2)?;
    let majority = (winner_count as f32) / (votes.len() as f32);
    if majority < MAJORITY_RATIO {
        log::info!(
            "[estimate_canonical_quadrant] no majority: winner {:?} {}/{} (need ≥{:.0}%) — falling back to default",
            winner,
            winner_count,
            votes.len(),
            MAJORITY_RATIO * 100.0
        );
        return None;
    }
    // Each surviving vote already cleared the per-window asymmetry
    // gate (|Δ| ≥ MIN_ASYMMETRY); the winner's average |Δ| is logged
    // for diagnostics but is not used as a second gate.
    let avg_delta = sum_conf / winner_count.max(1) as f32;
    log::info!(
        "[estimate_canonical_quadrant] winner={:?} votes={}/{} avg_Δ={:.3}",
        winner,
        winner_count,
        votes.len(),
        avg_delta
    );
    Some(winner)
}
