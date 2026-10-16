#![allow(dead_code)]

//! Live sliding-window tracker/content simulator.
//!
//! This is intentionally a small debug executable, not app UI. It renders a
//! synthetic long text surface, moves a viewport across it, runs the production
//! planar tracker on each crop, and overlays the information we need to reason
//! about sliding-window OCR:
//!
//! - blue: current FAST features
//! - green dots: anchor FAST features projected into the current crop
//! - cyan quad: active anchor bitmap footprint projected into the current crop
//! - green boxes: content discovered when the current anchor was acquired
//! - yellow boxes: currently visible synthetic detections that are not in the
//!   anchor content set yet
//!
//! Usage:
//!
//!   cargo run --release --manifest-path bench-tools/sliding-window-sim/Cargo.toml
//!
//! Keys:
//!   Space pause/resume auto-pan
//!   Left/Right pan manually
//!   A acquire a new anchor at the current viewport
//!   F toggle feature points
//!   D toggle synthetic detection boxes
//!   C toggle anchored content boxes
//!   R reset to the beginning
//!   Esc quit

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use image::imageops;
use image::{GrayImage, ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing::{
    draw_filled_circle_mut, draw_filled_rect_mut, draw_hollow_rect_mut, draw_line_segment_mut,
    draw_text_mut,
};
use imageproc::rect::Rect as ImgRect;
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[path = "../../../src/homography.rs"]
mod homography;
#[path = "../../../src/planar_tracker.rs"]
mod planar_tracker;

use homography::project;
use planar_tracker::{
    SceneAnchor, TrackResult, TrackerConfig, build_anchor, detect_fast, track_against_anchor,
};

const SURFACE_W: u32 = 2600;
const SURFACE_H: u32 = 520;
const VIEW_W: u32 = 780;
const VIEW_H: u32 = 360;
const VIEW_X: u32 = 20;
const VIEW_Y: u32 = 20;
const PANEL_X: u32 = 820;
const WIN_W: usize = 1160;
const WIN_H: usize = 640;
const SURFACE_VIEW_Y: u32 = 82;
const FONT_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

#[derive(Clone, Copy)]
struct FRect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl FRect {
    fn width(self) -> f32 {
        self.right - self.left
    }

    fn height(self) -> f32 {
        self.bottom - self.top
    }

    fn intersects(self, other: FRect) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }

    fn translate(self, dx: f32, dy: f32) -> Self {
        Self {
            left: self.left + dx,
            top: self.top + dy,
            right: self.right + dx,
            bottom: self.bottom + dy,
        }
    }

    fn to_img_rect(self) -> ImgRect {
        ImgRect::at(self.left.round() as i32, self.top.round() as i32)
            .of_size(self.width().max(1.0) as u32, self.height().max(1.0) as u32)
    }
}

#[derive(Clone)]
struct Word {
    id: usize,
    text: String,
    rect: FRect,
}

struct Surface {
    rgba: RgbaImage,
    words: Vec<Word>,
}

struct AnchorState {
    id: u64,
    anchor: SceneAnchor,
    surface_x: f32,
    content_word_ids: HashSet<usize>,
}

struct WorkRequest {
    seq: u64,
    anchor_id: Option<u64>,
    anchor: Option<SceneAnchor>,
    cfg: TrackerConfig,
    offset_x: f32,
}

struct WorkResult {
    seq: u64,
    anchor_id: Option<u64>,
    features: Vec<planar_tracker::KeyPoint>,
    track: Option<TrackResult>,
}

struct SimState {
    surface: Surface,
    font: FontArc,
    cfg: TrackerConfig,
    offset_x: f32,
    auto_pan: bool,
    pan_dir: f32,
    show_features: bool,
    show_detections: bool,
    show_content: bool,
    anchor: Option<AnchorState>,
    next_anchor_id: u64,
    last_track: Option<TrackResult>,
    last_lost: bool,
    frame_no: u64,
    needs_retrack: bool,
    current_features: Vec<planar_tracker::KeyPoint>,
    worker_tx: Sender<WorkRequest>,
    worker_rx: Receiver<WorkResult>,
    next_request_seq: u64,
    latest_result_seq: u64,
    worker_in_flight: bool,
    worker_pending: bool,
}

fn main() {
    let font = load_font();
    let surface = make_surface(&font);
    let (worker_tx, worker_rx) = start_worker(surface.rgba.clone());
    let cfg = TrackerConfig {
        fast_threshold: 18,
        max_features: 450,
        lowe_ratio: 0.82,
        ransac_residual_px: 4.0,
        ransac_iters: 120,
        min_inliers: 18,
        min_inliers_keep_locked: 8,
        nms_radius: 3,
    };

    let mut sim = SimState {
        surface,
        font,
        cfg,
        offset_x: 0.0,
        auto_pan: true,
        pan_dir: 1.0,
        show_features: true,
        show_detections: true,
        show_content: true,
        anchor: None,
        next_anchor_id: 1,
        last_track: None,
        last_lost: false,
        frame_no: 0,
        needs_retrack: true,
        current_features: Vec::new(),
        worker_tx,
        worker_rx,
        next_request_seq: 1,
        latest_result_seq: 0,
        worker_in_flight: false,
        worker_pending: false,
    };
    sim.acquire_anchor();

    if let Some(path) = dump_frame_arg() {
        sim.step();
        sim.render().save(&path).expect("save dump frame");
        eprintln!("wrote {}", path);
        return;
    }

    let mut window = Window::new(
        "sliding-window-sim",
        WIN_W,
        WIN_H,
        WindowOptions {
            resize: false,
            scale: Scale::X1,
            ..WindowOptions::default()
        },
    )
    .expect("open window");
    window.set_target_fps(30);

    let mut buffer = vec![0u32; WIN_W * WIN_H];
    while window.is_open() && !window.is_key_down(Key::Escape) {
        sim.handle_input(&window);
        sim.step();
        let frame = sim.render();
        rgba_to_minifb(&frame, &mut buffer);
        window
            .update_with_buffer(&buffer, WIN_W, WIN_H)
            .expect("update window");
    }
}

fn start_worker(surface_rgba: RgbaImage) -> (Sender<WorkRequest>, Receiver<WorkResult>) {
    let (request_tx, request_rx) = mpsc::channel::<WorkRequest>();
    let (result_tx, result_rx) = mpsc::channel::<WorkResult>();
    thread::spawn(move || {
        while let Ok(mut request) = request_rx.recv() {
            while let Ok(newer) = request_rx.try_recv() {
                request = newer;
            }

            let gray = crop_gray(&surface_rgba, request.offset_x);
            let features = detect_fast(
                &gray,
                request.cfg.fast_threshold,
                180,
                request.cfg.nms_radius,
            );
            let track = request
                .anchor
                .as_ref()
                .and_then(|anchor| track_against_anchor(anchor, &gray, &request.cfg));
            if result_tx
                .send(WorkResult {
                    seq: request.seq,
                    anchor_id: request.anchor_id,
                    features,
                    track,
                })
                .is_err()
            {
                break;
            }
        }
    });
    (request_tx, result_rx)
}

impl SimState {
    fn handle_input(&mut self, window: &Window) {
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            self.auto_pan = !self.auto_pan;
        }
        if window.is_key_pressed(Key::A, KeyRepeat::No) {
            self.acquire_anchor();
        }
        if window.is_key_pressed(Key::F, KeyRepeat::No) {
            self.show_features = !self.show_features;
        }
        if window.is_key_pressed(Key::D, KeyRepeat::No) {
            self.show_detections = !self.show_detections;
        }
        if window.is_key_pressed(Key::C, KeyRepeat::No) {
            self.show_content = !self.show_content;
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            self.offset_x = 0.0;
            self.pan_dir = 1.0;
            self.anchor = None;
            self.next_anchor_id = 1;
            self.acquire_anchor();
        }

        let manual_step = if window.is_key_down(Key::Left) {
            Some(-9.0)
        } else if window.is_key_down(Key::Right) {
            Some(9.0)
        } else {
            None
        };
        if let Some(dx) = manual_step {
            self.offset_x = (self.offset_x + dx).clamp(0.0, self.max_offset_x());
            self.auto_pan = false;
            self.needs_retrack = true;
        }
    }

    fn step(&mut self) {
        self.frame_no = self.frame_no.wrapping_add(1);
        if self.auto_pan {
            self.offset_x += self.pan_dir * 2.3;
            if self.offset_x >= self.max_offset_x() {
                self.offset_x = self.max_offset_x();
                self.pan_dir = -1.0;
            } else if self.offset_x <= 0.0 {
                self.offset_x = 0.0;
                self.pan_dir = 1.0;
            }
            self.needs_retrack = true;
        }

        // Tracking and FAST debug extraction are the expensive operations. Run
        // them on a worker thread at ~10 Hz during auto-pan and immediately
        // after manual changes, so window event polling stays responsive.
        if self.needs_retrack || self.frame_no % 3 == 0 {
            self.needs_retrack = false;
            self.request_work();
        }
        self.poll_worker();
    }

    fn acquire_anchor(&mut self) {
        let gray = self.current_gray();
        let Some(anchor) = build_anchor(&gray, &self.cfg, self.next_anchor_id) else {
            return;
        };
        let ids = self
            .visible_words()
            .into_iter()
            .map(|w| w.id)
            .collect::<HashSet<_>>();
        self.anchor = Some(AnchorState {
            id: self.next_anchor_id,
            anchor,
            surface_x: self.offset_x,
            content_word_ids: ids,
        });
        self.next_anchor_id += 1;
        self.last_track = None;
        self.last_lost = false;
        self.needs_retrack = true;
        self.request_work();
    }

    fn request_work(&mut self) {
        if self.worker_in_flight {
            self.worker_pending = true;
            return;
        }
        let seq = self.next_request_seq;
        self.next_request_seq = self.next_request_seq.wrapping_add(1);
        let request = WorkRequest {
            seq,
            anchor_id: self.anchor.as_ref().map(|a| a.id),
            anchor: self.anchor.as_ref().map(|a| a.anchor.clone()),
            cfg: self.cfg,
            offset_x: self.offset_x,
        };
        if self.worker_tx.send(request).is_ok() {
            self.worker_in_flight = true;
        }
    }

    fn poll_worker(&mut self) {
        let mut received = false;
        while let Ok(result) = self.worker_rx.try_recv() {
            received = true;
            if result.seq < self.latest_result_seq {
                continue;
            }
            let current_anchor_id = self.anchor.as_ref().map(|a| a.id);
            if result.anchor_id != current_anchor_id {
                continue;
            }
            self.latest_result_seq = result.seq;
            self.current_features = result.features;
            self.last_track = result.track;
            self.last_lost = self.anchor.is_some() && self.last_track.is_none();
        }
        if received {
            self.worker_in_flight = false;
            if self.worker_pending {
                self.worker_pending = false;
                self.request_work();
            }
        }
    }

    fn render(&self) -> RgbaImage {
        let mut frame =
            ImageBuffer::from_pixel(WIN_W as u32, WIN_H as u32, Rgba([32, 34, 38, 255]));
        draw_filled_rect_mut(
            &mut frame,
            ImgRect::at(VIEW_X as i32 - 2, VIEW_Y as i32 - 2).of_size(VIEW_W + 4, VIEW_H + 4),
            Rgba([12, 14, 18, 255]),
        );

        let crop = self.current_rgba();
        imageops::overlay(&mut frame, &crop, VIEW_X as i64, VIEW_Y as i64);
        draw_hollow_rect_mut(
            &mut frame,
            ImgRect::at(VIEW_X as i32, VIEW_Y as i32).of_size(VIEW_W, VIEW_H),
            Rgba([235, 235, 220, 255]),
        );

        if self.show_detections {
            self.draw_current_detections(&mut frame);
        }
        if self.show_content {
            self.draw_anchored_content(&mut frame);
        }
        self.draw_anchor_projection(&mut frame);
        if self.show_features {
            self.draw_features(&mut frame);
        }
        self.draw_minimap(&mut frame);
        self.draw_panel(&mut frame);
        frame
    }

    fn draw_current_detections(&self, frame: &mut RgbaImage) {
        let known = self.anchor.as_ref().map(|a| &a.content_word_ids);
        for word in self.visible_words() {
            let local = word
                .rect
                .translate(-self.offset_x, -(SURFACE_VIEW_Y as f32));
            let color = match known {
                Some(ids) if ids.contains(&word.id) => Rgba([90, 170, 80, 255]),
                _ => Rgba([245, 190, 40, 255]),
            };
            draw_hollow_rect_mut(
                frame,
                local.translate(VIEW_X as f32, VIEW_Y as f32).to_img_rect(),
                color,
            );
        }
    }

    fn draw_anchored_content(&self, frame: &mut RgbaImage) {
        let (Some(anchor), Some(track)) = (&self.anchor, &self.last_track) else {
            return;
        };
        for word in &self.surface.words {
            if !anchor.content_word_ids.contains(&word.id) {
                continue;
            }
            let local = word
                .rect
                .translate(-anchor.surface_x, -(SURFACE_VIEW_Y as f32));
            let pts = [
                (local.left, local.top),
                (local.right, local.top),
                (local.right, local.bottom),
                (local.left, local.bottom),
            ];
            let mut projected = Vec::with_capacity(4);
            for (x, y) in pts {
                if let Some((px, py)) = project(&track.homography, x, y) {
                    projected.push((px + VIEW_X as f32, py + VIEW_Y as f32));
                }
            }
            if projected.len() == 4 {
                draw_polyline(frame, &projected, Rgba([35, 215, 95, 255]));
            }
        }
    }

    fn draw_anchor_projection(&self, frame: &mut RgbaImage) {
        let Some(track) = &self.last_track else {
            return;
        };
        let pts = [
            (0.0, 0.0),
            (VIEW_W as f32, 0.0),
            (VIEW_W as f32, VIEW_H as f32),
            (0.0, VIEW_H as f32),
        ];
        let mut projected = Vec::with_capacity(4);
        for (x, y) in pts {
            if let Some((px, py)) = project(&track.homography, x, y) {
                projected.push((px + VIEW_X as f32, py + VIEW_Y as f32));
            }
        }
        if projected.len() == 4 {
            draw_polyline(frame, &projected, Rgba([55, 210, 235, 255]));
        }
    }

    fn draw_features(&self, frame: &mut RgbaImage) {
        for kp in self.current_features.iter() {
            draw_filled_circle_mut(
                frame,
                (
                    VIEW_X as i32 + kp.x.round() as i32,
                    VIEW_Y as i32 + kp.y.round() as i32,
                ),
                2,
                Rgba([55, 120, 255, 255]),
            );
        }

        let (Some(anchor), Some(track)) = (&self.anchor, &self.last_track) else {
            return;
        };
        for &(x, y) in anchor.anchor.positions.iter().step_by(3) {
            if let Some((px, py)) = project(&track.homography, x, y) {
                if px >= 0.0 && py >= 0.0 && px < VIEW_W as f32 && py < VIEW_H as f32 {
                    draw_filled_circle_mut(
                        frame,
                        (
                            VIEW_X as i32 + px.round() as i32,
                            VIEW_Y as i32 + py.round() as i32,
                        ),
                        1,
                        Rgba([40, 245, 130, 255]),
                    );
                }
            }
        }
    }

    fn draw_minimap(&self, frame: &mut RgbaImage) {
        let map_x = VIEW_X;
        let map_y = 430;
        let map_w = VIEW_W;
        let map_h = 64;
        draw_filled_rect_mut(
            frame,
            ImgRect::at(map_x as i32, map_y as i32).of_size(map_w, map_h),
            Rgba([48, 51, 56, 255]),
        );
        draw_hollow_rect_mut(
            frame,
            ImgRect::at(map_x as i32, map_y as i32).of_size(map_w, map_h),
            Rgba([170, 174, 180, 255]),
        );

        let sx = map_w as f32 / SURFACE_W as f32;
        if let Some(anchor) = &self.anchor {
            let ax = map_x as f32 + anchor.surface_x * sx;
            let aw = VIEW_W as f32 * sx;
            draw_filled_rect_mut(
                frame,
                ImgRect::at(ax.round() as i32, map_y as i32 + 10).of_size(aw.round() as u32, 18),
                Rgba([20, 115, 135, 255]),
            );
        }
        let vx = map_x as f32 + self.offset_x * sx;
        let vw = VIEW_W as f32 * sx;
        draw_hollow_rect_mut(
            frame,
            ImgRect::at(vx.round() as i32, map_y as i32 + 6).of_size(vw.round() as u32, 54),
            Rgba([245, 225, 95, 255]),
        );

        let label = "minimap: cyan=anchor bitmap, yellow=current viewport";
        draw_label(
            frame,
            &self.font,
            map_x as i32,
            map_y as i32 + 76,
            label,
            15.0,
            Rgba([220, 224, 228, 255]),
        );
    }

    fn draw_panel(&self, frame: &mut RgbaImage) {
        draw_filled_rect_mut(
            frame,
            ImgRect::at(PANEL_X as i32, VIEW_Y as i32).of_size(320, 580),
            Rgba([42, 45, 50, 255]),
        );
        draw_hollow_rect_mut(
            frame,
            ImgRect::at(PANEL_X as i32, VIEW_Y as i32).of_size(320, 580),
            Rgba([92, 97, 106, 255]),
        );

        let mut y = VIEW_Y as i32 + 18;
        draw_label(
            frame,
            &self.font,
            PANEL_X as i32 + 14,
            y,
            "sliding-window sim",
            22.0,
            Rgba([245, 245, 235, 255]),
        );
        y += 38;

        let anchor_id = self.anchor.as_ref().map(|a| a.id).unwrap_or(0);
        let status = match (&self.anchor, &self.last_track) {
            (None, _) => "no anchor",
            (Some(_), Some(_)) => "locked",
            (Some(_), None) => "lost",
        };
        let lines = [
            format!("status: {status}"),
            format!("anchor id: {anchor_id}"),
            format!("viewport x: {:.0}", self.offset_x),
            format!("auto pan: {}", if self.auto_pan { "on" } else { "off" }),
        ];
        for line in lines {
            draw_label(
                frame,
                &self.font,
                PANEL_X as i32 + 14,
                y,
                &line,
                17.0,
                Rgba([225, 228, 232, 255]),
            );
            y += 24;
        }

        y += 8;
        if let Some(track) = &self.last_track {
            for line in [
                format!("matches: {}", track.matches),
                format!("inliers: {}", track.inliers),
                format!("residual: {:.2}px", track.median_residual_px),
            ] {
                draw_label(
                    frame,
                    &self.font,
                    PANEL_X as i32 + 14,
                    y,
                    &line,
                    17.0,
                    Rgba([150, 235, 170, 255]),
                );
                y += 24;
            }
        } else if self.anchor.is_some() {
            draw_label(
                frame,
                &self.font,
                PANEL_X as i32 + 14,
                y,
                "tracking unavailable",
                17.0,
                Rgba([250, 130, 120, 255]),
            );
            y += 30;
        }

        y += 10;
        let visible = self.visible_words();
        let known_visible = self
            .anchor
            .as_ref()
            .map(|a| {
                visible
                    .iter()
                    .filter(|w| a.content_word_ids.contains(&w.id))
                    .count()
            })
            .unwrap_or(0);
        let unknown_visible = visible.len().saturating_sub(known_visible);
        for line in [
            format!("visible words: {}", visible.len()),
            format!("known visible: {known_visible}"),
            format!("new visible: {unknown_visible}"),
            format!("debug features: {}", self.current_features.len()),
        ] {
            draw_label(
                frame,
                &self.font,
                PANEL_X as i32 + 14,
                y,
                &line,
                17.0,
                Rgba([235, 220, 145, 255]),
            );
            y += 24;
        }

        y += 12;
        draw_label(
            frame,
            &self.font,
            PANEL_X as i32 + 14,
            y,
            "synthetic recognized:",
            16.0,
            Rgba([245, 245, 235, 255]),
        );
        y += 24;
        let recognized = visible
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for line in wrap_words(&recognized, 28).into_iter().take(4) {
            draw_label(
                frame,
                &self.font,
                PANEL_X as i32 + 14,
                y,
                &line,
                14.0,
                Rgba([210, 218, 230, 255]),
            );
            y += 20;
        }

        y += 20;
        for line in [
            "Space pause/resume",
            "Left/Right pan",
            "A acquire current view",
            "F features",
            "D detections",
            "C anchored content",
            "R reset",
        ] {
            draw_label(
                frame,
                &self.font,
                PANEL_X as i32 + 14,
                y,
                line,
                15.0,
                Rgba([198, 203, 210, 255]),
            );
            y += 22;
        }

        y += 18;
        draw_label(
            frame,
            &self.font,
            PANEL_X as i32 + 14,
            y,
            "Current limitation:",
            16.0,
            Rgba([245, 245, 235, 255]),
        );
        y += 24;
        for line in [
            "yellow words are detected in",
            "the current viewport, but are",
            "not part of the anchored",
            "content map until reacquire",
            "or future merge logic exists.",
        ] {
            draw_label(
                frame,
                &self.font,
                PANEL_X as i32 + 14,
                y,
                line,
                14.0,
                Rgba([235, 220, 145, 255]),
            );
            y += 20;
        }
    }

    fn current_rgba(&self) -> RgbaImage {
        imageops::crop_imm(
            &self.surface.rgba,
            self.offset_x.round() as u32,
            SURFACE_VIEW_Y,
            VIEW_W,
            VIEW_H,
        )
        .to_image()
    }

    fn current_gray(&self) -> GrayImage {
        crop_gray(&self.surface.rgba, self.offset_x)
    }

    fn visible_words(&self) -> Vec<&Word> {
        let viewport = FRect {
            left: self.offset_x,
            top: SURFACE_VIEW_Y as f32,
            right: self.offset_x + VIEW_W as f32,
            bottom: SURFACE_VIEW_Y as f32 + VIEW_H as f32,
        };
        self.surface
            .words
            .iter()
            .filter(|w| w.rect.intersects(viewport))
            .collect()
    }

    fn max_offset_x(&self) -> f32 {
        (SURFACE_W - VIEW_W) as f32
    }
}

fn make_surface(font: &FontArc) -> Surface {
    let mut rgba = ImageBuffer::from_pixel(SURFACE_W, SURFACE_H, Rgba([238, 236, 226, 255]));
    draw_background_texture(&mut rgba);

    let lines = [
        "this is some sentence being slowly discovered as the viewport slides across a much larger page",
        "the tracker can keep old overlays moving while new words appear without content context",
        "overlapping views should merge observations into complete lines instead of isolated fragments",
        "panning back should reuse the same surface content instead of translating partial windows again",
    ];

    let mut words = Vec::new();
    let mut id = 0usize;
    let text_color = Rgba([28, 31, 35, 255]);
    let scale = PxScale::from(36.0);
    let line_h = 74.0;
    for (line_idx, line) in lines.iter().enumerate() {
        let mut x = 64.0 + (line_idx as f32 * 35.0);
        let y = 68.0 + line_idx as f32 * line_h;
        for word in line.split_whitespace() {
            let word_w = measure_text(font, scale, word);
            draw_text_mut(
                &mut rgba,
                text_color,
                x.round() as i32,
                y.round() as i32,
                scale,
                font,
                word,
            );
            words.push(Word {
                id,
                text: word.to_owned(),
                rect: FRect {
                    left: x,
                    top: y + 5.0,
                    right: x + word_w,
                    bottom: y + 45.0,
                },
            });
            id += 1;
            x += word_w + 24.0;
        }
    }

    Surface { rgba, words }
}

fn draw_background_texture(image: &mut RgbaImage) {
    for y in (20..SURFACE_H).step_by(36) {
        draw_line_segment_mut(
            image,
            (0.0, y as f32),
            (SURFACE_W as f32, y as f32),
            Rgba([224, 221, 210, 255]),
        );
    }
    for x in (30..SURFACE_W).step_by(80) {
        let shade = if x % 160 == 30 { 218 } else { 228 };
        draw_line_segment_mut(
            image,
            (x as f32, 0.0),
            (x as f32, SURFACE_H as f32),
            Rgba([shade, shade, shade - 8, 255]),
        );
    }
    for i in 0..520 {
        let x = ((i * 193 + 41) % SURFACE_W as usize) as i32;
        let y = ((i * 97 + 23) % SURFACE_H as usize) as i32;
        put_pixel_checked(image, x, y, Rgba([205, 202, 192, 255]));
    }
}

fn crop_gray(surface_rgba: &RgbaImage, offset_x: f32) -> GrayImage {
    imageops::grayscale(
        &imageops::crop_imm(
            surface_rgba,
            offset_x.round() as u32,
            SURFACE_VIEW_Y,
            VIEW_W,
            VIEW_H,
        )
        .to_image(),
    )
}

fn measure_text(font: &FontArc, scale: PxScale, text: &str) -> f32 {
    let scaled = font.as_scaled(scale);
    text.chars()
        .map(|ch| scaled.h_advance(scaled.glyph_id(ch)))
        .sum::<f32>()
}

fn load_font() -> FontArc {
    let bytes = fs::read(Path::new(FONT_PATH)).expect("read DejaVuSans.ttf");
    FontArc::try_from_vec(bytes).expect("parse font")
}

fn draw_label(
    image: &mut RgbaImage,
    font: &FontArc,
    x: i32,
    y: i32,
    text: &str,
    size: f32,
    color: Rgba<u8>,
) {
    draw_text_mut(image, color, x, y, PxScale::from(size), font, text);
}

fn draw_polyline(image: &mut RgbaImage, pts: &[(f32, f32)], color: Rgba<u8>) {
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        draw_line_segment_mut(image, a, b, color);
        draw_line_segment_mut(image, (a.0 + 1.0, a.1), (b.0 + 1.0, b.1), color);
    }
}

fn put_pixel_checked(image: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32 {
        image.put_pixel(x as u32, y as u32, color);
    }
}

fn rgba_to_minifb(image: &RgbaImage, out: &mut [u32]) {
    for (dst, px) in out.iter_mut().zip(image.pixels()) {
        *dst = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32;
    }
}

fn dump_frame_arg() -> Option<String> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--dump-frame" {
            return args.next();
        }
    }
    None
}

fn wrap_words(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let next_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if next_len > max_chars && !current.is_empty() {
            lines.push(current);
            current = word.to_owned();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
