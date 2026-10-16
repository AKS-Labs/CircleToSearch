//! GLES2 overlay compositor: bakes the translated overlay (pills + glyph quads)
//! into an FBO texture on the GPU, then presents it — warped by the tracker
//! homography over the live camera passthrough ([`GlesRenderer::present_camera`]),
//! or blitted as a translucent window layer for screen capture
//! ([`GlesRenderer::present_screen_overlay_fbo`]). Both share one bake
//! ([`GlesRenderer::render_overlay_to_texture`]) and one warp core; the only
//! difference is the surface→clip transform and whether the camera is drawn under it.
//!
//! The crate never creates a GL context: the app owns EGL and the render
//! thread, and hands in a loader (`eglGetProcAddress`-style) at
//! construction. The shader is GLSL ES 1.00 / GLES2 — the universal floor
//! across Android 5+ and Ubuntu Touch — and runs unchanged on GLES3
//! contexts. `GlesRenderer` holds a `glow::Context`, which is `!Send`, so
//! it is bound to its owning thread at compile time.

use std::num::NonZeroU32;

use glow::HasContext;

use translator_core::homography::mat3_mul;
use translator_live::live_session::OverlayDrawList;

#[derive(Debug)]
pub enum GlError {
    ShaderCompile(String),
    ProgramLink(String),
    MissingLocation(&'static str),
}

/// What [`GlesRenderer::present`] draws into the bound framebuffer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PresentContent {
    /// Opaque camera passthrough with the overlays composited on top — the
    /// live-camera path. Clears to opaque black under the camera quad.
    #[default]
    CameraAndOverlay,
    /// Only the overlays, over a transparent (alpha 0) clear, with
    /// premultiplied-alpha output. For a translucent window that floats over
    /// live content the renderer does not own (the MediaProjection
    /// screen-translate overlay): the camera passthrough quad is skipped so
    /// the real screen behind the window shows through. The tracker gray/RGBA
    /// readbacks still sample the external source, so OCR is unaffected.
    OverlayOnly,
}

const VERT_SRC: &str = r#"#version 100
attribute vec2 a_pos;
uniform mat3 u_transform;
uniform mat3 u_uv_xform;
// Quad → canonical (overlay) coords so the present fragment can punch the pinhole
// lattice at display resolution (crisp), instead of baking soft holes into the
// overlay texture. Identity for non-overlay draws (camera/luma ignore v_canon).
uniform mat3 u_quad_to_canon;
varying vec2 v_uv;
varying vec2 v_canon;
void main() {
    vec3 clip = u_transform * vec3(a_pos, 1.0);
    vec3 uv = u_uv_xform * vec3(a_pos, 1.0);
    v_uv = uv.xy;
    v_canon = (u_quad_to_canon * vec3(a_pos, 1.0)).xy;
    gl_Position = vec4(clip.xy, 0.0, clip.z);
}
"#;

const FRAG_SRC: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform sampler2D u_tex;
uniform float u_overlay_alpha;
// Pinhole lattice, punched here at display resolution so each hole is one crisp
// pixel rather than a baked sub-pixel disc smeared by the present's LINEAR upscale.
// u_hole_spacing <= 0 disables it (camera passthrough). Centre on the mirror texel
// the recovery reads (lattice + 0.25 — see REC_FRAG) so punch and sample coincide.
uniform float u_hole_spacing;
uniform float u_hole_radius;
uniform float u_hole_alpha;
varying vec2 v_uv;
varying vec2 v_canon;
void main() {
    vec4 c = texture2D(u_tex, v_uv);
    if (u_hole_spacing > 0.0 && c.a > 0.1) {
        float origin = u_hole_spacing * 0.5;
        // Per-row stagger (see Lattice::build): each row's holes shift right by
        // row mod pitch (1 canonical px/row = 2 display px after the 2× present
        // upscale), turning solid hole-columns into a sheared lattice so the grid's
        // moiré against the display pixels rotates off the vertical axis.
        float row = floor((v_canon.y - origin) / u_hole_spacing + 0.5);
        float phase = mod(row, u_hole_spacing);
        vec2 base = vec2(
            floor((v_canon.x - origin - phase) / u_hole_spacing + 0.5) * u_hole_spacing + origin + phase,
            row * u_hole_spacing + origin);
        // Snap to the nearest mirror-texel centre so punch and recovery land on the
        // same hole pixel even at a fractional pitch (see REC_FRAG for the why).
        vec2 nearest = (floor(base * 2.0) + 0.5) / 2.0;
        if (distance(v_canon, nearest) <= u_hole_radius) {
            c.a = u_hole_alpha;
        }
    }
    // Parametric overlay opacity (1.0 for camera passthrough). Scaling alpha
    // pre-blend is premultiply-correct: the source-over blend then yields an
    // effective coverage of (texel alpha × u_overlay_alpha).
    c.a *= u_overlay_alpha;
    gl_FragColor = c;
}
"#;

/// Camera fragment shader for a borrowed `GL_TEXTURE_EXTERNAL_OES` source
/// (e.g. an Android/UT SurfaceTexture). Shares [`VERT_SRC`]; the only
/// difference from [`FRAG_SRC`] is the external sampler. `#extension` must
/// precede the `precision` block.
const EXT_FRAG_SRC: &str = r#"#version 100
#extension GL_OES_EGL_image_external : require
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform samplerExternalOES u_tex;
varying vec2 v_uv;
void main() {
    gl_FragColor = texture2D(u_tex, v_uv);
}
"#;

/// Like [`EXT_FRAG_SRC`] but outputs luminance (Rec. 601) instead of colour,
/// for the per-frame tracker readback into an R8 framebuffer. Writing luma to
/// all channels keeps it correct whether the attachment is R8 or RGBA.
const EXT_LUMA_FRAG_SRC: &str = r#"#version 100
#extension GL_OES_EGL_image_external : require
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform samplerExternalOES u_tex;
varying vec2 v_uv;
void main() {
    vec3 c = texture2D(u_tex, v_uv).rgb;
    float y = dot(c, vec3(0.299, 0.587, 0.114));
    gl_FragColor = vec4(y, y, y, 1.0);
}
"#;

/// Pill vertex shader: the unit quad spans one oriented pill. `a_pos` (0..1) is
/// passed through as the local uv so the fragment shader can evaluate the rounded
/// rect SDF in pill-local pixel space; `u_transform` maps the unit quad to clip.
const PILL_VERT_SRC: &str = r#"#version 100
attribute vec2 a_pos;
uniform mat3 u_transform;
varying vec2 v_local;
void main() {
    v_local = a_pos;
    vec3 clip = u_transform * vec3(a_pos, 1.0);
    gl_Position = vec4(clip.xy, 0.0, clip.z);
}
"#;

/// Pill fragment shader: rounded-rect SDF coverage, the GPU port of
/// `planar_engine::fill_oriented_rect_blended`. `u_half` is the pill half-extent
/// in texels, `u_radius` the corner radius (texels); coverage `= clamp(0.5 - sdf,
/// 0, 1)` with a 1px feather. Output is opaque-coverage (rgb = color, a =
/// coverage) so overlapping pills union flat in the pill-layer FBO.
const PILL_FRAG_SRC: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform vec4 u_color;
uniform vec2 u_half;
uniform float u_radius;
varying vec2 v_local;
void main() {
    // Local pixel coords centred on the pill (v_local 0..1 → [-half, half]).
    vec2 p = (v_local - 0.5) * 2.0 * u_half;
    vec2 q = abs(p) - (u_half - vec2(u_radius));
    float outside = length(max(q, 0.0));
    float inside = min(max(q.x, q.y), 0.0);
    float sdf = outside + inside - u_radius;
    float coverage = clamp(0.5 - sdf, 0.0, 1.0);
    if (coverage <= 0.0) {
        discard;
    }
    gl_FragColor = vec4(u_color.rgb, u_color.a * coverage);
}
"#;

/// Glyph vertex shader: maps the unit quad to clip and passes the atlas uv.
const GLYPH_VERT_SRC: &str = r#"#version 100
attribute vec2 a_pos;
uniform mat3 u_transform;
uniform mat3 u_uv_xform;
varying vec2 v_uv;
void main() {
    vec3 clip = u_transform * vec3(a_pos, 1.0);
    vec3 uv = u_uv_xform * vec3(a_pos, 1.0);
    v_uv = uv.xy;
    gl_Position = vec4(clip.xy, 0.0, clip.z);
}
"#;

/// Glyph fragment shader: samples an R8 coverage atlas and outputs straight alpha
/// (fg color × coverage × text_alpha). Holes are punched at present time over the
/// whole overlay (pills + glyphs alike), so the glyph itself bakes solid.
const GLYPH_FRAG_SRC: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform sampler2D u_atlas;
uniform vec4 u_color;
uniform float u_text_alpha;
varying vec2 v_uv;
void main() {
    float a = texture2D(u_atlas, v_uv).r;
    gl_FragColor = vec4(u_color.rgb, a * u_text_alpha);
}
"#;

/// `GL_TEXTURE_EXTERNAL_OES` (not in glow's constant set).
const TEXTURE_EXTERNAL_OES: u32 = 0x8D65;

/// Max pills the lattice recovery shader handles in one pass (uniform array).
const REC_MAX_PILLS: usize = 64;

/// Vertex shader for the lattice recovery pass: a full-clip quad from the unit
/// `a_pos`, no transforms — the fragment derives the lattice point from
/// `gl_FragCoord`.
const REC_VERT_SRC: &str = r#"#version 100
attribute vec2 a_pos;
void main() {
    gl_Position = vec4(a_pos * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// Recovery fragment shader: each output texel is one lattice point. Map the
/// point's canonical position to the camera uv (same `u_uv_xform` the present
/// uses), sample the external mirror, and where a pill covers the point invert
/// the known blend `screen = (raw − (1−f)·pill) / f`. Output luma in R.
const REC_FRAG_SRC: &str = r#"#version 100
#extension GL_OES_EGL_image_external : require
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform samplerExternalOES u_tex;
uniform mat3 u_uv_xform;
uniform vec2 u_canon_size;
uniform float u_spacing;
uniform float u_origin;
uniform float u_pill_luma;   // 0..1
uniform float u_screen_frac; // f in (0,1]
uniform int u_pill_count;
uniform vec4 u_pills[64];     // cx, cy, half_w, half_h (canonical)
void main() {
    vec2 cell = floor(gl_FragCoord.xy);
    vec2 base = vec2(u_origin) + cell * u_spacing;
    base.x += mod(cell.y, u_spacing); // per-row stagger, see Lattice::build
    // Snap each hole to the nearest mirror-texel CENTRE, then sample there. External
    // textures sample by normalized texture2D (no texelFetch): an integer-canonical
    // position lands on a mirror texel boundary, where NEAREST grabs a neighbour and,
    // for a 1px hole, misses it. The mirror is 2× canonical, so floor(base*2)+0.5 is
    // the containing texel's centre; /2 returns to canonical. For an integer pitch
    // this equals the old fixed +0.25 nudge; for a fractional pitch (2.5) it still
    // lands dead-centre instead of on a boundary. The punch snaps identically.
    vec2 canon = (floor(base * 2.0) + 0.5) / 2.0;
    // Flip v: the acquire's readback applies Y_FLIP_CLIP to land a top-down
    // canonical frame (the space the boxes live in). We sample the camera uv
    // directly, so without this flip the recovered grid is vertically mirrored
    // relative to the boxes — every box would monitor the wrong screen row.
    vec2 a = vec2(canon.x / u_canon_size.x, 1.0 - canon.y / u_canon_size.y);
    vec3 uv = u_uv_xform * vec3(a, 1.0);
    vec3 c = texture2D(u_tex, uv.xy).rgb;
    // Per-channel recovery (RGB, not luma): under a pill, invert the hole blend per
    // channel; elsewhere pass the raw colour. RGB so an isoluminant change registers.
    vec3 rec = c;
    for (int i = 0; i < 64; i++) {
        if (i >= u_pill_count) { break; }
        vec4 pr = u_pills[i];
        if (abs(canon.x - pr.x) <= pr.z && abs(canon.y - pr.y) <= pr.w) {
            rec = clamp((c - vec3((1.0 - u_screen_frac) * u_pill_luma)) / u_screen_frac, 0.0, 1.0);
            break;
        }
    }
    gl_FragColor = vec4(rec, 1.0);
}
"#;

/// Pinhole grid for the screen-monitor overlay: where to drop pill alpha so the
/// captured mirror exposes a known blend of pill + screen, recoverable by the
/// lattice probe. `spacing`/`radius` are canonical-coord units; `alpha` is the
/// baked pill alpha at a hole, chosen so the *effective* on-screen blend (after
/// the present pill_alpha and the overlay window alpha) lands at the recovery
/// shader's fixed 50%.
#[derive(Debug, Clone, Copy)]
pub struct HoleGrid {
    pub spacing: f32,
    pub radius: f32,
    pub alpha: f32,
}

struct RecProgram {
    program: glow::Program,
    a_pos: u32,
    u_tex: glow::UniformLocation,
    u_uv_xform: glow::UniformLocation,
    u_canon_size: glow::UniformLocation,
    u_spacing: glow::UniformLocation,
    u_origin: glow::UniformLocation,
    u_pill_luma: glow::UniformLocation,
    u_screen_frac: glow::UniformLocation,
    u_pill_count: glow::UniformLocation,
    u_pills: glow::UniformLocation,
}
/// `GL_R8` sized internal format (GLES3) for the single-channel gray FBO.
const R8: u32 = 0x8229;

/// Row-major 3×3 that negates the clip-space y. Pre-multiplied into a readback's
/// `dst_to_clip` so the FBO renders vertically mirrored; the bottom-up `glReadPixels`
/// then yields a top-down buffer with no CPU row-flip.
const Y_FLIP_CLIP: [f32; 9] = [1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0];

/// Row-major identity-affine helpers, matching the convention of
/// [`translator_core::homography`] (`mat3_mul` is row-major, `a * b`).
fn translate(tx: f32, ty: f32) -> [f32; 9] {
    [1.0, 0.0, tx, 0.0, 1.0, ty, 0.0, 0.0, 1.0]
}

fn scale(sx: f32, sy: f32) -> [f32; 9] {
    [sx, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 1.0]
}

/// Maps top-left-origin viewport pixels to clip space, flipping y so the
/// image's top row lands at the top of the framebuffer. Affine (`w` row
/// is `[0,0,1]`), so it leaves the homography's perspective divide intact.
fn ndc_from_viewport(w: f32, h: f32) -> [f32; 9] {
    [2.0 / w, 0.0, -1.0, 0.0, -2.0 / h, 1.0, 0.0, 0.0, 1.0]
}

/// Like [`ndc_from_viewport`] but **without** the y flip: top-left-origin pixel
/// coords map to clip with `y` increasing downward landing at texture `v` = 0 for
/// the top row. Used when rendering *into* the overlay FBO so the baked texture
/// is stored top-row-at-`v=0`, matching the CPU-uploaded overlay convention the
/// present path already samples.
fn ndc_from_viewport_no_flip(w: f32, h: f32) -> [f32; 9] {
    [2.0 / w, 0.0, -1.0, 0.0, 2.0 / h, -1.0, 0.0, 0.0, 1.0]
}

/// GLES2 `glUniformMatrix3fv` forbids the transpose flag, so we transpose
/// our row-major matrices into the column-major order GL expects.
fn to_column_major(m: &[f32; 9]) -> [f32; 9] {
    [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]]
}

/// Geometry of the overlay last baked into [`GlesRenderer::overlay_fbo`]. Lets the
/// per-frame present warp/blit the baked texture without rebaking (the camera
/// presents every frame but rebakes only when content changes).
#[derive(Clone, Copy)]
struct BakedOverlay {
    /// Overlay-texture origin in surface (canonical) coords.
    origin_x: f32,
    origin_y: f32,
    /// Texels per surface unit baked into the texture.
    oversample: f32,
    /// Used sub-rect of the (grow-only) overlay FBO, in texels.
    used_w: u32,
    used_h: u32,
    /// Pinhole params to punch at present time (canonical units; spacing 0 = none).
    hole_spacing: f32,
    hole_radius: f32,
    hole_alpha: f32,
}

pub struct GlesRenderer {
    gl: glow::Context,
    program: glow::Program,
    u_transform: glow::UniformLocation,
    u_uv_xform: glow::UniformLocation,
    u_tex: glow::UniformLocation,
    u_overlay_alpha: glow::UniformLocation,
    // Present-time pinhole punch; `None` if the driver optimized them out (a build
    // without the holes path). Set per-overlay-draw from the baked hole params.
    u_quad_to_canon: Option<glow::UniformLocation>,
    u_hole_spacing: Option<glow::UniformLocation>,
    u_hole_radius: Option<glow::UniformLocation>,
    u_hole_alpha: Option<glow::UniformLocation>,
    a_pos: u32,
    /// Parametric overlay opacity for the 2D program, applied to overlay draws
    /// (forced 1.0 for the camera passthrough). Set once per renderer via
    /// [`set_overlay_alpha`](Self::set_overlay_alpha); the screen path uses it
    /// to control overlay opacity independently of the camera path.
    overlay_alpha: f32,
    quad_vbo: glow::Buffer,
    fbo: Option<glow::Framebuffer>,
    fbo_tex: Option<glow::Texture>,
    fbo_size: Option<(u32, u32)>,
    /// Lazily-built program for a borrowed external-OES camera source.
    ext: Option<ExtProgram>,
    /// Lazily-built external-OES program that outputs luminance, for the
    /// single-channel tracker-gray readback.
    ext_luma: Option<ExtProgram>,
    /// Lazily-built external-OES program that recovers screen_est at the
    /// pinhole lattice for the screen monitor.
    rec_lattice: Option<RecProgram>,
    /// RGBA FBO for the lattice recovery readback (`read_lattice_screen_est`), grow-only.
    rec_rgba_fbo: Option<(glow::Framebuffer, glow::Texture, u32, u32)>,
    /// R8 FBO for the gray readback (separate from the RGBA `fbo`).
    gray_fbo: Option<glow::Framebuffer>,
    gray_fbo_tex: Option<glow::Texture>,
    gray_fbo_size: Option<(u32, u32)>,
    /// `(external texture id, canonical uv transform row-major)` when the
    /// camera source is a borrowed `GL_TEXTURE_EXTERNAL_OES` (zero-copy) rather
    /// than the uploaded 2D `camera_tex`. The uv transform carries the
    /// upright/crop/flip from sensor space into the canonical frame.
    camera_external: Option<(u32, [f32; 9])>,
    /// Whether [`present`](Self::present) composites the camera + overlays or
    /// only the overlays over transparent. Set once for the renderer's
    /// lifetime via [`set_present_content`](Self::set_present_content).
    present_content: PresentContent,
    /// Framebuffer the present functions draw into. 0 (the window surface) for
    /// shells that own the EGL surface (Android); shells that composite under a
    /// retained UI (QtQuick) point this at their own offscreen FBO via
    /// [`set_present_fbo`](Self::set_present_fbo).
    present_fbo: u32,
    /// Lazily-built rounded-rect pill program (the GPU overlay compositor).
    pill: Option<PillProgram>,
    /// Lazily-built glyph atlas program (R8 coverage × fg color → straight alpha).
    /// Shares [`VERT_SRC`]; only the fragment shader differs from the 2D program.
    glyph_prog: Option<GlyphProgram>,
    /// Overlay-compositor FBO: the baked pills+glyphs overlay texture that the
    /// present then warps (camera) / blits (screen). `(fbo, tex, w, h)`.
    overlay_fbo: Option<(glow::Framebuffer, glow::Texture, u32, u32)>,
    /// Pill-layer FBO: opaque rounded pills are unioned here, then composited
    /// into `overlay_fbo` at the pill opacity. `(fbo, tex, w, h)`.
    pill_fbo: Option<(glow::Framebuffer, glow::Texture, u32, u32)>,
    /// Scratch RGBA texture the matted-strip pass re-uploads per strip (sizes
    /// vary); reused across strips and overlay bakes. Lazily created.
    strip_tex: Option<glow::Texture>,
    /// R8 glyph coverage atlas with shelf packing. Grows until overflow, then
    /// clears and re-packs the current frame's masks.
    glyph_atlas: Option<GlyphAtlas>,
    /// Geometry of the overlay last baked into `overlay_fbo`, so the present can
    /// warp/blit it without rebaking. `None` until the first bake.
    baked_overlay: Option<BakedOverlay>,
    /// Session content version the baked overlay reflects. The camera present
    /// rebakes only when this moves; the screen present rebakes every present.
    overlay_baked_version: Option<u64>,
}

/// The rounded-rect pill program (shares nothing with the 2D program — its own
/// SDF fragment shader). Built lazily on first overlay composite.
struct PillProgram {
    program: glow::Program,
    u_transform: glow::UniformLocation,
    u_color: glow::UniformLocation,
    u_half: glow::UniformLocation,
    u_radius: glow::UniformLocation,
    a_pos: u32,
}

/// The glyph program (shares [`VERT_SRC`]; [`GLYPH_FRAG_SRC`] fragment). Built
/// lazily on first overlay composite.
struct GlyphProgram {
    program: glow::Program,
    u_transform: glow::UniformLocation,
    u_uv_xform: glow::UniformLocation,
    u_atlas: glow::UniformLocation,
    u_color: glow::UniformLocation,
    u_text_alpha: glow::UniformLocation,
    a_pos: u32,
}

/// R8 glyph coverage atlas with a shelf packer. Grows to `max_side`×`max_side`;
/// on overflow clears all slots and re-packs the current frame's glyphs.
struct GlyphAtlas {
    texture: glow::Texture,
    width: u32,
    height: u32,
    /// `(atlas_x, atlas_y)` for glyphs already uploaded.
    slots: std::collections::HashMap<translator_render::image_render::GlyphKey, (u32, u32)>,
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
}

/// The external-OES camera program (shares [`VERT_SRC`] with the 2D program;
/// only the fragment sampler differs). Built lazily on first external use so
/// contexts that never use it (Android's uploaded path) don't require the
/// `GL_OES_EGL_image_external` extension.
struct ExtProgram {
    program: glow::Program,
    u_transform: glow::UniformLocation,
    u_uv_xform: glow::UniformLocation,
    u_tex: glow::UniformLocation,
    a_pos: u32,
}

impl GlesRenderer {
    /// `loader` resolves GL function pointers (e.g. `eglGetProcAddress`).
    /// The caller must have created and made-current a GLES2 (or newer)
    /// context on the calling thread before this runs.
    pub fn new(loader: impl FnMut(&str) -> *const std::ffi::c_void) -> Result<Self, GlError> {
        let gl = unsafe { glow::Context::from_loader_function(loader) };
        unsafe {
            let max_tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE);
            log::debug!("GlesRenderer: GL_MAX_TEXTURE_SIZE={max_tex}");

            let program = link_program(&gl)?;
            let u_transform = gl
                .get_uniform_location(program, "u_transform")
                .ok_or(GlError::MissingLocation("u_transform"))?;
            let u_uv_xform = gl
                .get_uniform_location(program, "u_uv_xform")
                .ok_or(GlError::MissingLocation("u_uv_xform"))?;
            let u_tex = gl
                .get_uniform_location(program, "u_tex")
                .ok_or(GlError::MissingLocation("u_tex"))?;
            let u_overlay_alpha = gl
                .get_uniform_location(program, "u_overlay_alpha")
                .ok_or(GlError::MissingLocation("u_overlay_alpha"))?;
            let u_quad_to_canon = gl.get_uniform_location(program, "u_quad_to_canon");
            let u_hole_spacing = gl.get_uniform_location(program, "u_hole_spacing");
            let u_hole_radius = gl.get_uniform_location(program, "u_hole_radius");
            let u_hole_alpha = gl.get_uniform_location(program, "u_hole_alpha");
            let a_pos = gl
                .get_attrib_location(program, "a_pos")
                .ok_or(GlError::MissingLocation("a_pos"))?;

            let quad_vbo = gl.create_buffer().map_err(GlError::ProgramLink)?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad_vbo));
            let verts: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
            let verts_bytes = std::slice::from_raw_parts(
                verts.as_ptr() as *const u8,
                std::mem::size_of_val(&verts),
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, verts_bytes, glow::STATIC_DRAW);

            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);

            Ok(Self {
                gl,
                program,
                u_transform,
                u_uv_xform,
                u_tex,
                u_overlay_alpha,
                u_quad_to_canon,
                u_hole_spacing,
                u_hole_radius,
                u_hole_alpha,
                a_pos,
                overlay_alpha: 1.0,
                quad_vbo,
                fbo: None,
                fbo_tex: None,
                fbo_size: None,
                ext_luma: None,
                rec_lattice: None,
                rec_rgba_fbo: None,
                gray_fbo: None,
                gray_fbo_tex: None,
                gray_fbo_size: None,
                ext: None,
                camera_external: None,
                present_content: PresentContent::default(),
                present_fbo: 0,
                pill: None,
                glyph_prog: None,
                overlay_fbo: None,
                pill_fbo: None,
                strip_tex: None,
                glyph_atlas: None,
                baked_overlay: None,
                overlay_baked_version: None,
            })
        }
    }

    /// Use a borrowed `GL_TEXTURE_EXTERNAL_OES` as the camera source (zero-copy)
    /// instead of the uploaded 2D texture. `uv` is the row-major canonical uv
    /// transform (unit-quad → sensor uv, encoding upright/crop/flip). Lazily
    /// builds the external-OES program; if that fails (no extension) the source
    /// is left unchanged. Cleared with [`clear_camera_external`](Self::clear_camera_external).
    pub fn set_camera_external(&mut self, id: u32, uv: [f32; 9]) {
        if self.ext.is_none() {
            self.ext = self.build_ext_program();
        }
        if self.ext.is_some() {
            self.camera_external = Some((id, uv));
        }
    }

    pub fn clear_camera_external(&mut self) {
        self.camera_external = None;
    }

    /// Choose what [`present`](Self::present) draws — camera + overlays, or
    /// overlays only over a transparent clear (see [`PresentContent`]). The
    /// camera/screen source is independent of this; it governs only the
    /// final present composite, not the tracker readbacks.
    pub fn set_present_content(&mut self, content: PresentContent) {
        self.present_content = content;
    }

    /// Target framebuffer for the present functions. Defaults to 0 (the window
    /// surface); shells whose UI toolkit owns the window (QtQuick) pass their
    /// offscreen FBO so the present composes under the retained UI instead of
    /// over it.
    pub fn set_present_fbo(&mut self, fbo: u32) {
        self.present_fbo = fbo;
    }

    /// Set the overlay opacity multiplier (0..1) applied to overlay draws.
    /// `1.0` (default) = the overlay's own alpha unchanged. The camera path
    /// leaves it at 1.0; the screen path can lower it independently of the
    /// (touch-capped) window alpha.
    pub fn set_overlay_alpha(&mut self, alpha: f32) {
        self.overlay_alpha = alpha.clamp(0.0, 1.0);
    }

    fn build_ext_program(&self) -> Option<ExtProgram> {
        self.build_ext_program_frag(EXT_FRAG_SRC)
    }

    fn build_ext_program_frag(&self, frag_src: &str) -> Option<ExtProgram> {
        let gl = &self.gl;
        let program = match link_program_frag(gl, frag_src) {
            Ok(p) => p,
            Err(e) => {
                log::error!("GlesRenderer: external-OES program link failed: {e:?}");
                return None;
            }
        };
        unsafe {
            Some(ExtProgram {
                u_transform: gl.get_uniform_location(program, "u_transform")?,
                u_uv_xform: gl.get_uniform_location(program, "u_uv_xform")?,
                u_tex: gl.get_uniform_location(program, "u_tex")?,
                a_pos: gl.get_attrib_location(program, "a_pos")?,
                program,
            })
        }
    }

    /// Build the rounded-rect pill program if not already built.
    fn ensure_pill_program(&mut self) -> bool {
        if self.pill.is_some() {
            return true;
        }
        let gl = &self.gl;
        let program = match link_program_vert_frag(gl, PILL_VERT_SRC, PILL_FRAG_SRC) {
            Ok(p) => p,
            Err(e) => {
                log::error!("GlesRenderer: pill program link failed: {e:?}");
                return false;
            }
        };
        unsafe {
            let pill = (|| {
                Some(PillProgram {
                    u_transform: gl.get_uniform_location(program, "u_transform")?,
                    u_color: gl.get_uniform_location(program, "u_color")?,
                    u_half: gl.get_uniform_location(program, "u_half")?,
                    u_radius: gl.get_uniform_location(program, "u_radius")?,
                    a_pos: gl.get_attrib_location(program, "a_pos")?,
                    program,
                })
            })();
            self.pill = pill;
        }
        self.pill.is_some()
    }

    /// Build the glyph atlas program if not already built.
    fn ensure_glyph_program(&mut self) -> bool {
        if self.glyph_prog.is_some() {
            return true;
        }
        let gl = &self.gl;
        let program = match link_program_vert_frag(gl, GLYPH_VERT_SRC, GLYPH_FRAG_SRC) {
            Ok(p) => p,
            Err(e) => {
                log::error!("GlesRenderer: glyph program link failed: {e:?}");
                return false;
            }
        };
        unsafe {
            let prog = (|| {
                Some(GlyphProgram {
                    u_transform: gl.get_uniform_location(program, "u_transform")?,
                    u_uv_xform: gl.get_uniform_location(program, "u_uv_xform")?,
                    u_atlas: gl.get_uniform_location(program, "u_atlas")?,
                    u_color: gl.get_uniform_location(program, "u_color")?,
                    u_text_alpha: gl.get_uniform_location(program, "u_text_alpha")?,
                    a_pos: gl.get_attrib_location(program, "a_pos")?,
                    program,
                })
            })();
            self.glyph_prog = prog;
        }
        self.glyph_prog.is_some()
    }

    /// Ensure the R8 glyph atlas texture is allocated (1024×1024 fixed). Built once;
    /// when it overflows (repack needed) `upload_new_glyphs` clears and re-packs.
    fn ensure_glyph_atlas(&mut self) -> bool {
        if self.glyph_atlas.is_some() {
            return true;
        }
        const ATLAS_W: u32 = 1024;
        const ATLAS_H: u32 = 1024;
        let gl = &self.gl;
        let texture = match new_texture(gl) {
            Ok(t) => t,
            Err(_) => return false,
        };
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            // Use NEAREST for coverage masks — subpixel sampling of a 1-byte mask
            // through a linear filter can soften coverage in GLES2 mediump float.
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                R8 as i32,
                ATLAS_W as i32,
                ATLAS_H as i32,
                0,
                glow::RED,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
        }
        self.glyph_atlas = Some(GlyphAtlas {
            texture,
            width: ATLAS_W,
            height: ATLAS_H,
            slots: std::collections::HashMap::new(),
            shelf_x: 0,
            shelf_y: 0,
            shelf_h: 0,
        });
        true
    }

    /// Ensure the slot holds an RGBA8 FBO + texture at least `w×h`, **grow-only**:
    /// reused as-is when already big enough, reallocated only when the request
    /// exceeds the current allocation, never shrunk. The overlay content size
    /// changes every acquire; growing to the running max (≈ display size after the
    /// first near-full overlay) means a reallocation happens a couple of times
    /// early and then never again — vs. a per-acquire realloc of two ~18 MB
    /// textures, which was the dominant first-present cost. Returns
    /// `(fbo, tex, alloc_w, alloc_h)`; callers render into the `[0..w]×[0..h]`
    /// sub-rect and sample it back via the alloc dims.
    fn ensure_rgba_fbo(
        slot: &mut Option<(glow::Framebuffer, glow::Texture, u32, u32)>,
        gl: &glow::Context,
        w: u32,
        h: u32,
    ) -> (glow::Framebuffer, glow::Texture, u32, u32) {
        if let Some((fbo, tex, sw, sh)) = *slot {
            if sw >= w && sh >= h {
                return (fbo, tex, sw, sh);
            }
            unsafe {
                gl.delete_framebuffer(fbo);
                gl.delete_texture(tex);
            }
        }
        // Grow to the max of the request and any prior allocation so the buffer
        // only ever grows.
        let (w, h) = match *slot {
            Some((_, _, sw, sh)) => (w.max(sw), h.max(sh)),
            None => (w, h),
        };
        unsafe {
            let tex = new_texture(gl).expect("create overlay fbo texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            let fbo = gl.create_framebuffer().expect("create overlay framebuffer");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            assert_eq!(
                status,
                glow::FRAMEBUFFER_COMPLETE,
                "overlay FBO incomplete: status=0x{status:x}"
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            *slot = Some((fbo, tex, w, h));
            (fbo, tex, w, h)
        }
    }

    /// (Re)create the R8 gray readback FBO + texture to match `w*h`.
    fn ensure_gray_fbo(&mut self, w: u32, h: u32) {
        if self.gray_fbo_size == Some((w, h)) {
            return;
        }
        let gl = &self.gl;
        unsafe {
            if let Some(t) = self.gray_fbo_tex.take() {
                gl.delete_texture(t);
            }
            let tex = new_texture(gl).expect("create gray fbo texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                R8 as i32,
                w as i32,
                h as i32,
                0,
                glow::RED,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            let fbo = match self.gray_fbo {
                Some(f) => f,
                None => {
                    let f = gl.create_framebuffer().expect("create gray framebuffer");
                    self.gray_fbo = Some(f);
                    f
                }
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            assert_eq!(
                status,
                glow::FRAMEBUFFER_COMPLETE,
                "gray readback FBO incomplete: status=0x{status:x}"
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gray_fbo_tex = Some(tex);
            self.gray_fbo_size = Some((w, h));
        }
    }

    /// Render the borrowed external camera as **luminance** into the R8 FBO at
    /// `w×h` and read it back top-down — the per-frame tracker gray, produced on
    /// the GPU so the readback transfers one channel and the CPU skips the
    /// RGBA→luma pass. `dst_to_clip` must match [`read_camera_rgba`] / the
    /// present so the gray is oriented exactly like the displayed frame.
    pub fn read_camera_gray(&mut self, w: u32, h: u32, dst_to_clip: &[f32; 9]) -> Option<Vec<u8>> {
        let (id, uv) = self.camera_external?;
        let id = NonZeroU32::new(id)?;
        if self.ext_luma.is_none() {
            self.ext_luma = self.build_ext_program_frag(EXT_LUMA_FRAG_SRC);
        }
        self.ext_luma.as_ref()?;
        self.ensure_gray_fbo(w, h);
        // Render y-flipped so the bottom-up `glReadPixels` yields a top-down buffer
        // directly — no second buffer + CPU row-flip. `Y_FLIP_CLIP` negates clip-y.
        let cam_transform = mat3_mul(
            &Y_FLIP_CLIP,
            &mat3_mul(dst_to_clip, &scale(w as f32, h as f32)),
        );
        let mut gray = vec![0u8; (w as usize) * (h as usize)];
        unsafe {
            let gl = &self.gl;
            let e = self.ext_luma.as_ref().expect("ext_luma program present");
            gl.bind_framebuffer(glow::FRAMEBUFFER, self.gray_fbo);
            gl.viewport(0, 0, w as i32, h as i32);
            gl.disable(glow::BLEND);
            // The clip-y flip reverses triangle winding; cull would drop the quad.
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(e.program));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad_vbo));
            gl.enable_vertex_attrib_array(e.a_pos);
            gl.vertex_attrib_pointer_f32(e.a_pos, 2, glow::FLOAT, false, 0, 0);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(glow::NativeTexture(id)));
            gl.uniform_1_i32(Some(&e.u_tex), 0);
            gl.uniform_matrix_3_f32_slice(
                Some(&e.u_transform),
                false,
                &to_column_major(&cam_transform),
            );
            gl.uniform_matrix_3_f32_slice(Some(&e.u_uv_xform), false, &to_column_major(&uv));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            // GL_RED is 1 byte/px; the default GL_PACK_ALIGNMENT of 4 pads each
            // row up to a 4-byte boundary, overrunning a tightly-packed w*h
            // buffer when w isn't a multiple of 4. Pack tight.
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RED,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut gray)),
            );
            gl.bind_texture(TEXTURE_EXTERNAL_OES, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Some(gray)
    }

    fn ensure_rec_program(&mut self) -> bool {
        if self.rec_lattice.is_some() {
            return true;
        }
        let gl = &self.gl;
        let program = match link_program_vert_frag(gl, REC_VERT_SRC, REC_FRAG_SRC) {
            Ok(p) => p,
            Err(e) => {
                log::error!("GlesRenderer: lattice recovery program link failed: {e:?}");
                return false;
            }
        };
        unsafe {
            self.rec_lattice = (|| {
                Some(RecProgram {
                    a_pos: gl.get_attrib_location(program, "a_pos")?,
                    u_tex: gl.get_uniform_location(program, "u_tex")?,
                    u_uv_xform: gl.get_uniform_location(program, "u_uv_xform")?,
                    u_canon_size: gl.get_uniform_location(program, "u_canon_size")?,
                    u_spacing: gl.get_uniform_location(program, "u_spacing")?,
                    u_origin: gl.get_uniform_location(program, "u_origin")?,
                    u_pill_luma: gl.get_uniform_location(program, "u_pill_luma")?,
                    u_screen_frac: gl.get_uniform_location(program, "u_screen_frac")?,
                    u_pill_count: gl.get_uniform_location(program, "u_pill_count")?,
                    u_pills: gl.get_uniform_location(program, "u_pills[0]")?,
                    program,
                })
            })();
        }
        self.rec_lattice.is_some()
    }

    /// Recover screen_est at every point of a `cols×rows` lattice over a
    /// `canon_w×canon_h` canonical frame, sampling the borrowed external camera
    /// (the captured mirror, which includes our overlay). At points a pill covers
    /// the recovery inverts the 50%-hole blend; elsewhere it passes the raw luma.
    /// Returns one byte per lattice point in row-major (`row*cols + col`) order,
    /// matching `screen_monitor::Lattice`. `pills` are AABBs `(cx, cy, hw, hh)` in
    /// canonical coords; `pill_luma`/`screen_frac` are 0..1.
    #[allow(clippy::too_many_arguments)]
    pub fn read_lattice_screen_est(
        &mut self,
        cols: u32,
        rows: u32,
        canon_w: u32,
        canon_h: u32,
        spacing: f32,
        pills: &[(f32, f32, f32, f32)],
        pill_luma: f32,
        screen_frac: f32,
    ) -> Option<Vec<[u8; 3]>> {
        let (id, uv) = self.camera_external?;
        let id = NonZeroU32::new(id)?;
        if cols == 0 || rows == 0 || !self.ensure_rec_program() {
            return None;
        }
        let (rec_fbo, _, _, _) =
            Self::ensure_rgba_fbo(&mut self.rec_rgba_fbo, &self.gl, cols, rows);
        let n = pills.len().min(REC_MAX_PILLS);
        let mut flat = Vec::with_capacity(n * 4);
        for &(cx, cy, hw, hh) in &pills[..n] {
            flat.extend_from_slice(&[cx, cy, hw, hh]);
        }
        let mut rgba = vec![0u8; (cols * rows * 4) as usize];
        unsafe {
            let gl = &self.gl;
            let r = self.rec_lattice.as_ref().expect("rec program present");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(rec_fbo));
            gl.viewport(0, 0, cols as i32, rows as i32);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(r.program));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad_vbo));
            gl.enable_vertex_attrib_array(r.a_pos);
            gl.vertex_attrib_pointer_f32(r.a_pos, 2, glow::FLOAT, false, 0, 0);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(glow::NativeTexture(id)));
            // NEAREST so each lattice sample reads the exact pinhole pixel — LINEAR
            // would blend in the surrounding opaque-pill pixels (measuring
            // non-holes), dimming/compressing the recovered screen_est.
            gl.tex_parameter_i32(
                TEXTURE_EXTERNAL_OES,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                TEXTURE_EXTERNAL_OES,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            gl.uniform_1_i32(Some(&r.u_tex), 0);
            gl.uniform_matrix_3_f32_slice(Some(&r.u_uv_xform), false, &to_column_major(&uv));
            gl.uniform_2_f32(Some(&r.u_canon_size), canon_w as f32, canon_h as f32);
            gl.uniform_1_f32(Some(&r.u_spacing), spacing);
            gl.uniform_1_f32(Some(&r.u_origin), spacing * 0.5);
            gl.uniform_1_f32(Some(&r.u_pill_luma), pill_luma);
            gl.uniform_1_f32(Some(&r.u_screen_frac), screen_frac.max(1e-3));
            gl.uniform_1_i32(Some(&r.u_pill_count), n as i32);
            if !flat.is_empty() {
                gl.uniform_4_f32_slice(Some(&r.u_pills), &flat);
            }
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(
                0,
                0,
                cols as i32,
                rows as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut rgba)),
            );
            gl.bind_texture(TEXTURE_EXTERNAL_OES, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Some(rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect())
    }

    /// Render the borrowed external camera (canonical transform, no overlays)
    /// into the owned FBO at `w×h` and read it back as **top-down RGBA** — the
    /// canonical frame the pipeline's `LiveFrame` carries (replacing the CPU
    /// `map` + `transform_frame`). `None` if no external camera source is set.
    ///
    /// `dst_to_clip` must be the **same** transform used to present, so the
    /// frame the OCR sees is exactly what's displayed (no divergent orientation).
    pub fn read_camera_rgba(&mut self, w: u32, h: u32, dst_to_clip: &[f32; 9]) -> Option<Vec<u8>> {
        let (id, uv) = self.camera_external?;
        let id = NonZeroU32::new(id)?;
        self.ext.as_ref()?;
        self.ensure_fbo(w, h);
        // Render y-flipped so the bottom-up `glReadPixels` yields a top-down buffer
        // directly — no second buffer + CPU row-flip. `Y_FLIP_CLIP` negates clip-y.
        let cam_transform = mat3_mul(
            &Y_FLIP_CLIP,
            &mat3_mul(dst_to_clip, &scale(w as f32, h as f32)),
        );
        let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
        unsafe {
            let gl = &self.gl;
            let e = self.ext.as_ref().expect("ext program present");
            gl.bind_framebuffer(glow::FRAMEBUFFER, self.fbo);
            gl.viewport(0, 0, w as i32, h as i32);
            gl.disable(glow::BLEND);
            // The clip-y flip reverses triangle winding; cull would drop the quad.
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(e.program));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad_vbo));
            gl.enable_vertex_attrib_array(e.a_pos);
            gl.vertex_attrib_pointer_f32(e.a_pos, 2, glow::FLOAT, false, 0, 0);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(glow::NativeTexture(id)));
            gl.uniform_1_i32(Some(&e.u_tex), 0);
            gl.uniform_matrix_3_f32_slice(
                Some(&e.u_transform),
                false,
                &to_column_major(&cam_transform),
            );
            gl.uniform_matrix_3_f32_slice(Some(&e.u_uv_xform), false, &to_column_major(&uv));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut rgba)),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Some(rgba)
    }

    /// Bind a framebuffer + viewport on the renderer's own GL context. Pass
    /// `fbo_id == 0` for the default framebuffer (EGL window surface). Lets
    /// callers that don't own a separate `glow::Context` (the Android JNI
    /// shim) re-target the present output after [`read_camera_gray`] /
    /// [`read_camera_rgba`] leave their own readback FBO bound.
    pub fn bind_present_framebuffer(&self, fbo_id: u32, width: i32, height: i32) {
        unsafe {
            let fbo = NonZeroU32::new(fbo_id).map(glow::NativeFramebuffer);
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, fbo);
            self.gl.viewport(0, 0, width, height);
        }
    }

    /// Bake the overlay draw list into [`Self::overlay_fbo`]'s straight-alpha
    /// texture on the GPU. Two passes: (A) rounded SDF pills unioned opaque into
    /// the pill-layer FBO, then (B) that layer composited at `pill_alpha` + glyph
    /// quads sampled from the R8 atlas src-over at `text_alpha` into the overlay FBO.
    /// New glyph masks are uploaded to the atlas before the draw; the atlas clears
    /// and re-packs from scratch when it overflows. Returns false on failure.
    /// Must run on the GL thread; leaves no framebuffer bound.
    pub fn render_overlay_to_texture(
        &mut self,
        dl: &OverlayDrawList,
        pill_alpha: f32,
        text_alpha: f32,
        smooth_glyphs: bool,
        holes: Option<HoleGrid>,
    ) -> bool {
        if dl.bitmap_w == 0 || dl.bitmap_h == 0 {
            return false;
        }
        if !self.ensure_pill_program() || !self.ensure_glyph_program() || !self.ensure_glyph_atlas()
        {
            return false;
        }
        let (bw, bh) = (dl.bitmap_w, dl.bitmap_h);
        let os = dl.oversample.max(1e-3);
        // Render into the FBO without a y flip so the baked texture stores the top
        // row at v=0 (the convention the present path samples for the CPU canvas).
        let ndc = ndc_from_viewport_no_flip(bw as f32, bh as f32);

        // Upload any new glyph masks to the R8 atlas before the draw pass.
        let t_upload = std::time::Instant::now();
        let new_glyphs = self.upload_new_glyphs(&dl.glyphs.masks);
        let upload_ms = t_upload.elapsed().as_secs_f64() * 1000.0;

        // FBO (re)alloc: grow-only so the per-acquire size change only pays once.
        let t_alloc = std::time::Instant::now();
        let (pill_fbo, pill_tex, pa_w, pa_h) =
            Self::ensure_rgba_fbo(&mut self.pill_fbo, &self.gl, bw, bh);
        let (overlay_fbo, _overlay_tex, oa_w, oa_h) =
            Self::ensure_rgba_fbo(&mut self.overlay_fbo, &self.gl, bw, bh);
        let alloc_ms = t_alloc.elapsed().as_secs_f64() * 1000.0;
        let pill_uv_sx = bw as f32 / pa_w as f32;
        let pill_uv_sy = bh as f32 / pa_h as f32;
        let _ = (oa_w, oa_h);
        let t_passes = std::time::Instant::now();

        unsafe {
            let gl = &self.gl;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad_vbo));
            gl.active_texture(glow::TEXTURE0);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );

            // --- Pass A: opaque rounded pills unioned into the pill layer. ---
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(pill_fbo));
            gl.viewport(0, 0, bw as i32, bh as i32);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            let pill = self.pill.as_ref().expect("pill program present");
            gl.use_program(Some(pill.program));
            gl.enable_vertex_attrib_array(pill.a_pos);
            gl.vertex_attrib_pointer_f32(pill.a_pos, 2, glow::FLOAT, false, 0, 0);
            // Pills/glyphs bake SOLID; holes are punched at present time (display res,
            // crisp). The real params ride on `baked_overlay` below.
            for p in &dl.pills {
                let r = &p.rect;
                let cos = r.angle_radians.cos();
                let sin = r.angle_radians.sin();
                let cx = (r.cx - dl.origin_x) * os;
                let cy = (r.cy - dl.origin_y) * os;
                let hw = r.width * os * 0.5;
                let hh = r.height * os * 0.5;
                if hw <= 0.0 || hh <= 0.0 {
                    continue;
                }
                let radius = (hw.min(hh) * 0.5).min(12.0).max(0.0);
                let (tx, ty) = (cos, sin);
                let (px, py) = (-sin, cos);
                let c0x = cx - hw * tx - hh * px;
                let c0y = cy - hw * ty - hh * py;
                let quad_to_texel = [
                    2.0 * hw * tx,
                    2.0 * hh * px,
                    c0x,
                    2.0 * hw * ty,
                    2.0 * hh * py,
                    c0y,
                    0.0,
                    0.0,
                    1.0,
                ];
                let transform = mat3_mul(&ndc, &quad_to_texel);
                gl.uniform_matrix_3_f32_slice(
                    Some(&pill.u_transform),
                    false,
                    &to_column_major(&transform),
                );
                gl.uniform_4_f32_slice(
                    Some(&pill.u_color),
                    &[
                        p.color[0] as f32 / 255.0,
                        p.color[1] as f32 / 255.0,
                        p.color[2] as f32 / 255.0,
                        1.0,
                    ],
                );
                gl.uniform_2_f32_slice(Some(&pill.u_half), &[hw, hh]);
                gl.uniform_1_f32(Some(&pill.u_radius), radius);
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }

            // --- Pass B: pill layer at pill_alpha, then glyph quads at text_alpha. ---
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(overlay_fbo));
            gl.viewport(0, 0, bw as i32, bh as i32);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);

            // B1: pill layer composited at pill_alpha.
            let pill_uv = to_column_major(&scale(pill_uv_sx, pill_uv_sy));
            let full = mat3_mul(&ndc, &scale(bw as f32, bh as f32));
            gl.use_program(Some(self.program));
            gl.enable_vertex_attrib_array(self.a_pos);
            gl.vertex_attrib_pointer_f32(self.a_pos, 2, glow::FLOAT, false, 0, 0);
            gl.uniform_1_i32(Some(&self.u_tex), 0);
            gl.uniform_1_f32(Some(&self.u_overlay_alpha), pill_alpha.clamp(0.0, 1.0));
            gl.uniform_matrix_3_f32_slice(Some(&self.u_transform), false, &to_column_major(&full));
            gl.uniform_matrix_3_f32_slice(Some(&self.u_uv_xform), false, &pill_uv);
            // Bake compositing must NOT punch — holes happen at present. The 2D
            // program persists `u_hole_spacing` across draws, so a prior present's
            // value would leak in here; force it off for this baked-layer blit.
            if let Some(l) = &self.u_hole_spacing {
                gl.uniform_1_f32(Some(l), 0.0);
            }
            gl.bind_texture(glow::TEXTURE_2D, Some(pill_tex));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);

            // B1.5: matted strips — the ink model's reconstructed background, each
            // an oriented textured quad composited at full opacity through its
            // per-pixel coverage alpha (only the erased ink pixels paint; the live
            // camera shows through elsewhere). Reuses the 2D program with an
            // identity uv so the quad samples the whole strip; the strip's own
            // canonical geometry gives the quad, so texture and footprint register
            // 1:1. Drawn after the pill layer (matted boxes emit no pill) and under
            // the glyphs.
            if !dl.strips.is_empty() {
                if self.strip_tex.is_none() {
                    self.strip_tex = new_texture(gl).ok();
                }
                if let Some(stex) = self.strip_tex {
                    let ident = to_column_major(&scale(1.0, 1.0));
                    gl.use_program(Some(self.program));
                    gl.enable_vertex_attrib_array(self.a_pos);
                    gl.vertex_attrib_pointer_f32(self.a_pos, 2, glow::FLOAT, false, 0, 0);
                    gl.uniform_1_i32(Some(&self.u_tex), 0);
                    gl.uniform_1_f32(Some(&self.u_overlay_alpha), 1.0);
                    gl.uniform_matrix_3_f32_slice(Some(&self.u_uv_xform), false, &ident);
                    if let Some(l) = &self.u_hole_spacing {
                        gl.uniform_1_f32(Some(l), 0.0);
                    }
                    gl.bind_texture(glow::TEXTURE_2D, Some(stex));
                    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                    for s in &dl.strips {
                        let r = &s.rect;
                        let cos = r.angle_radians.cos();
                        let sin = r.angle_radians.sin();
                        let cx = (r.cx - dl.origin_x) * os;
                        let cy = (r.cy - dl.origin_y) * os;
                        let hw = r.width * os * 0.5;
                        let hh = r.height * os * 0.5;
                        if hw <= 0.0 || hh <= 0.0 || s.px_w == 0 || s.px_h == 0 {
                            continue;
                        }
                        let (tx, ty) = (cos, sin);
                        let (px, py) = (-sin, cos);
                        let c0x = cx - hw * tx - hh * px;
                        let c0y = cy - hw * ty - hh * py;
                        let quad_to_texel = [
                            2.0 * hw * tx,
                            2.0 * hh * px,
                            c0x,
                            2.0 * hw * ty,
                            2.0 * hh * py,
                            c0y,
                            0.0,
                            0.0,
                            1.0,
                        ];
                        let transform = mat3_mul(&ndc, &quad_to_texel);
                        gl.tex_image_2d(
                            glow::TEXTURE_2D,
                            0,
                            glow::RGBA as i32,
                            s.px_w as i32,
                            s.px_h as i32,
                            0,
                            glow::RGBA,
                            glow::UNSIGNED_BYTE,
                            glow::PixelUnpackData::Slice(Some(&s.rgba)),
                        );
                        gl.uniform_matrix_3_f32_slice(
                            Some(&self.u_transform),
                            false,
                            &to_column_major(&transform),
                        );
                        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                    }
                }
            }

            // B2: glyph quads from the R8 atlas, one draw call per instance.
            // Each quad is rotated in overlay-texel space by the glyph's line angle,
            // placed at (pen + rotate(left, top)), and UV-mapped to its atlas slot.
            let gp = self.glyph_prog.as_ref().expect("glyph program present");
            let atlas = self.glyph_atlas.as_ref().expect("atlas present");
            gl.use_program(Some(gp.program));
            gl.enable_vertex_attrib_array(gp.a_pos);
            gl.vertex_attrib_pointer_f32(gp.a_pos, 2, glow::FLOAT, false, 0, 0);
            gl.uniform_1_i32(Some(&gp.u_atlas), 0);
            gl.uniform_1_f32(Some(&gp.u_text_alpha), text_alpha.clamp(0.0, 1.0));
            gl.bind_texture(glow::TEXTURE_2D, Some(atlas.texture));
            // Axis-aligned (screen) glyphs map ~1:1 to atlas texels, so NEAREST stays
            // crisp; the camera rotates each quad by the line angle and then H-warps
            // the whole overlay, where NEAREST sampling of the upright mask aliases
            // (1px jaggies). LINEAR anti-aliases the rotated/warped sampling.
            let filter = if smooth_glyphs {
                glow::LINEAR
            } else {
                glow::NEAREST
            } as i32;
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter);
            let (aw, ah) = (atlas.width as f32, atlas.height as f32);
            for inst in &dl.glyphs.instances {
                let Some(&(ax, ay)) = atlas.slots.get(&inst.key) else {
                    continue;
                };
                let Some(mask) = dl.glyphs.masks.get(&inst.key) else {
                    continue;
                };
                let (w, h) = (mask.w as f32, mask.h as f32);
                let (left, top) = (mask.left as f32, mask.top as f32);
                // Unit quad → overlay-texel: rotate (left,top)+(u*w,v*h) by (cos,sin).
                // x(u,v) = pen_x + (left+u*w)*cos - (top+v*h)*sin
                // y(u,v) = pen_y + (left+u*w)*sin + (top+v*h)*cos
                let local_to_texel = [
                    w * inst.cos,
                    -h * inst.sin,
                    inst.pen_x + left * inst.cos - top * inst.sin,
                    w * inst.sin,
                    h * inst.cos,
                    inst.pen_y + left * inst.sin + top * inst.cos,
                    0.0,
                    0.0,
                    1.0,
                ];
                let transform = mat3_mul(&ndc, &local_to_texel);
                let u0 = ax as f32 / aw;
                let v0 = ay as f32 / ah;
                let uv_xform = [w / aw, 0.0, u0, 0.0, h / ah, v0, 0.0, 0.0, 1.0];
                gl.uniform_matrix_3_f32_slice(
                    Some(&gp.u_transform),
                    false,
                    &to_column_major(&transform),
                );
                gl.uniform_matrix_3_f32_slice(
                    Some(&gp.u_uv_xform),
                    false,
                    &to_column_major(&uv_xform),
                );
                gl.uniform_4_f32_slice(
                    Some(&gp.u_color),
                    &[
                        inst.color[0] as f32 / 255.0,
                        inst.color[1] as f32 / 255.0,
                        inst.color[2] as f32 / 255.0,
                        inst.color[3] as f32 / 255.0,
                    ],
                );
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }

            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        let passes_ms = t_passes.elapsed().as_secs_f64() * 1000.0;
        log::info!(
            "[overlay-bake] {bw}x{bh} (alloc {pa_w}x{pa_h}) \
             upload={upload_ms:.1}ms({new_glyphs} new glyphs/{} inst) \
             alloc={alloc_ms:.1}ms passes={passes_ms:.1}ms",
            dl.glyphs.instances.len()
        );
        let hole = holes.unwrap_or(HoleGrid {
            spacing: 0.0,
            radius: 0.0,
            alpha: 1.0,
        });
        self.baked_overlay = Some(BakedOverlay {
            origin_x: dl.origin_x,
            origin_y: dl.origin_y,
            oversample: os,
            used_w: bw,
            used_h: bh,
            hole_spacing: hole.spacing,
            hole_radius: hole.radius,
            // The old bake punched the hole into the pill layer (`hole.alpha`) which
            // was then composited at `pill_alpha`; the present overrides the baked
            // (solid) alpha, so fold that `pill_alpha` in here to reproduce the same
            // captured blend (SCREEN_HOLE_FRAC) the recovery expects.
            hole_alpha: hole.alpha * pill_alpha,
        });
        true
    }

    /// Content version the currently-baked overlay reflects (set via
    /// [`Self::set_overlay_baked_version`]); the camera present rebakes only when
    /// the session's content version differs from this.
    pub fn overlay_baked_version(&self) -> Option<u64> {
        self.overlay_baked_version
    }

    pub fn set_overlay_baked_version(&mut self, version: u64) {
        self.overlay_baked_version = Some(version);
    }

    /// Drop the baked overlay so the next present draws none (the active anchor's
    /// content was cleared). Cheap: just forgets the geometry; the FBO storage stays.
    pub fn clear_baked_overlay(&mut self) {
        self.baked_overlay = None;
    }

    /// Upload glyph masks not yet in the atlas. If a mask doesn't fit on the current
    /// shelf, a new shelf is started; if the atlas overflows entirely, all slots are
    /// cleared and the current frame's masks are re-uploaded from scratch. Returns
    /// the count of newly uploaded masks.
    fn upload_new_glyphs(
        &mut self,
        masks: &std::collections::HashMap<
            translator_render::image_render::GlyphKey,
            translator_render::image_render::GlyphMaskData,
        >,
    ) -> usize {
        let atlas = match self.glyph_atlas.as_mut() {
            Some(a) => a,
            None => return 0,
        };
        // Collect masks that are not yet in the atlas.
        let missing: Vec<&translator_render::image_render::GlyphMaskData> = masks
            .values()
            .filter(|m| !atlas.slots.contains_key(&m.key))
            .collect();
        if missing.is_empty() {
            return 0;
        }
        // Try to place each missing mask. If any fails, clear the atlas and retry the
        // full current-frame mask set (which always fits if no single mask > atlas size).
        let mut placed: Vec<(&translator_render::image_render::GlyphMaskData, u32, u32)> =
            Vec::new();
        let mut overflow = false;
        for m in &missing {
            if m.w == 0 || m.h == 0 {
                continue;
            }
            match Self::atlas_place(atlas, m.w, m.h) {
                Some((x, y)) => placed.push((m, x, y)),
                None => {
                    overflow = true;
                    break;
                }
            }
        }
        if overflow {
            // Clear and re-pack all current-frame masks from scratch.
            atlas.slots.clear();
            atlas.shelf_x = 0;
            atlas.shelf_y = 0;
            atlas.shelf_h = 0;
            placed.clear();
            for m in masks.values() {
                if m.w == 0 || m.h == 0 {
                    continue;
                }
                if let Some((x, y)) = Self::atlas_place(atlas, m.w, m.h) {
                    placed.push((m, x, y));
                }
            }
        }
        let gl = &self.gl;
        let atlas = self.glyph_atlas.as_mut().expect("atlas present");
        let count = placed.len();
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(atlas.texture));
            for (m, ax, ay) in &placed {
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    *ax as i32,
                    *ay as i32,
                    m.w as i32,
                    m.h as i32,
                    glow::RED,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&m.cov)),
                );
                atlas.slots.insert(m.key, (*ax, *ay));
            }
        }
        count
    }

    /// Shelf-packer: try to place a `w×h` rect in the atlas. Advances `shelf_x`
    /// on success; starts a new shelf if the current one is full. Returns `None` when
    /// the entire atlas is exhausted (caller should clear and retry).
    fn atlas_place(atlas: &mut GlyphAtlas, w: u32, h: u32) -> Option<(u32, u32)> {
        if atlas.shelf_x + w > atlas.width {
            // New shelf.
            let new_y = atlas.shelf_y + atlas.shelf_h;
            if new_y + h > atlas.height {
                return None;
            }
            atlas.shelf_y = new_y;
            atlas.shelf_x = 0;
            atlas.shelf_h = 0;
        }
        let x = atlas.shelf_x;
        let y = atlas.shelf_y;
        atlas.shelf_x += w;
        atlas.shelf_h = atlas.shelf_h.max(h);
        Some((x, y))
    }

    /// Present the GPU-baked overlay ([`Self::render_overlay_to_texture`]) into the
    /// window surface as a translucent layer (the screen `TextureView`): one quad
    /// over a transparent clear, no camera, no homography. Straight-alpha texture →
    /// premultiplied window output (SurfaceFlinger) via the global `overlay_alpha`.
    /// Uses the geometry recorded at bake time. The caller swaps after.
    pub fn present_screen_overlay_fbo(
        &mut self,
        canonical_w: u32,
        canonical_h: u32,
        surface_w: u32,
        surface_h: u32,
    ) -> bool {
        // Canonical surface coords → clip: scale canonical→surface px, then NDC.
        let sx = surface_w as f32 / canonical_w.max(1) as f32;
        let sy = surface_h as f32 / canonical_h.max(1) as f32;
        let ndc = ndc_from_viewport(surface_w as f32, surface_h as f32);
        let base = mat3_mul(&ndc, &scale(sx, sy));
        self.bind_present_framebuffer(self.present_fbo, surface_w as i32, surface_h as i32);
        unsafe {
            let gl = &self.gl;
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad_vbo));
            gl.active_texture(glow::TEXTURE0);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::SCISSOR_TEST);
        }
        self.draw_baked_overlay_quad(&base, true)
    }

    /// Present the live camera for the tracked path: draw the external camera
    /// passthrough opaque, then (when locked) warp the baked overlay over it by the
    /// tracker homography. `h_surface_to_viewport` maps the anchor's surface coords
    /// to canonical view px; `display_xform` maps canonical px to clip. Returns false
    /// only when there's no external camera source (nothing to show). The caller swaps.
    pub fn present_camera(
        &mut self,
        display_xform: &[f32; 9],
        h_surface_to_viewport: Option<[f32; 9]>,
        canonical_w: u32,
        canonical_h: u32,
        surface_w: u32,
        surface_h: u32,
    ) -> bool {
        if self.camera_external.is_none() {
            return false;
        }
        self.bind_present_framebuffer(self.present_fbo, surface_w as i32, surface_h as i32);
        unsafe {
            let gl = &self.gl;
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad_vbo));
            gl.active_texture(glow::TEXTURE0);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
        }
        self.draw_camera_passthrough_ext(canonical_w as f32, canonical_h as f32, display_xform);
        if let Some(h) = h_surface_to_viewport {
            // `read_camera_*` row-flips the readback to a top-down canonical frame, so
            // overlays are authored vertically mirrored vs the (non-flipped) on-screen
            // external camera. Undo that flip in canonical space before the warp.
            let flip_y = [1.0, 0.0, 0.0, 0.0, -1.0, canonical_h as f32, 0.0, 0.0, 1.0];
            let base = mat3_mul(display_xform, &mat3_mul(&flip_y, &h));
            self.draw_baked_overlay_quad(&base, false);
        }
        true
    }

    /// Warp the baked overlay texture by `base_surface_to_clip` and draw it as one
    /// quad with the 2D program. `premultiplied` selects the window-layer blend (true,
    /// screen) vs plain source-over onto the opaque camera (false). Assumes the caller
    /// bound the present framebuffer, the quad VBO, and cleared. The grow-only FBO's
    /// used sub-rect is selected via the uv transform. Returns false if nothing baked.
    fn draw_baked_overlay_quad(
        &self,
        base_surface_to_clip: &[f32; 9],
        premultiplied: bool,
    ) -> bool {
        let Some(baked) = self.baked_overlay else {
            return false;
        };
        let Some((_, overlay_tex, alloc_w, alloc_h)) = self.overlay_fbo else {
            return false;
        };
        if alloc_w == 0 || alloc_h == 0 || baked.used_w == 0 || baked.used_h == 0 {
            return false;
        }
        let inv_os = 1.0 / baked.oversample.max(1e-3);
        let footprint = scale(baked.used_w as f32 * inv_os, baked.used_h as f32 * inv_os);
        let to_surface = translate(baked.origin_x, baked.origin_y);
        // a_pos[0,1] → canonical (overlay) coords, so the present fragment punches the
        // pinhole lattice at display res. The full transform appends base→clip.
        let quad_to_canon = mat3_mul(&to_surface, &footprint);
        let transform = mat3_mul(base_surface_to_clip, &quad_to_canon);
        let uv_sub = to_column_major(&scale(
            baked.used_w as f32 / alloc_w as f32,
            baked.used_h as f32 / alloc_h as f32,
        ));
        unsafe {
            let gl = &self.gl;
            gl.use_program(Some(self.program));
            gl.enable_vertex_attrib_array(self.a_pos);
            gl.vertex_attrib_pointer_f32(self.a_pos, 2, glow::FLOAT, false, 0, 0);
            gl.uniform_1_i32(Some(&self.u_tex), 0);
            gl.uniform_1_f32(Some(&self.u_overlay_alpha), self.overlay_alpha);
            // The passthrough/monitor pass leaves an external texture on unit 0;
            // Adreno/Mali mis-sample a sampler2D while one lingers there, so clear it.
            if self.camera_external.is_some() {
                gl.bind_texture(TEXTURE_EXTERNAL_OES, None);
            }
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            if premultiplied {
                // Translucent window layer: leave premultiplied output.
                gl.blend_func_separate(
                    glow::SRC_ALPHA,
                    glow::ONE_MINUS_SRC_ALPHA,
                    glow::ONE,
                    glow::ONE_MINUS_SRC_ALPHA,
                );
            } else {
                // Overlay composites onto the opaque camera. The baked overlay
                // texture is PREMULTIPLIED (the bake source-over'd into a cleared
                // FBO, so rgb already carries × coverage), so source-over with
                // `ONE` for the source factor — not `SRC_ALPHA`, which would apply
                // coverage a second time and darken every soft edge toward black
                // (a dark rim around strips, a dark halo around glyph anti-aliasing
                // that reads as outlined text).
                gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            }
            gl.uniform_matrix_3_f32_slice(
                Some(&self.u_transform),
                false,
                &to_column_major(&transform),
            );
            gl.uniform_matrix_3_f32_slice(Some(&self.u_uv_xform), false, &uv_sub);
            if let Some(l) = &self.u_quad_to_canon {
                gl.uniform_matrix_3_f32_slice(Some(l), false, &to_column_major(&quad_to_canon));
            }
            if let Some(l) = &self.u_hole_spacing {
                gl.uniform_1_f32(Some(l), baked.hole_spacing);
            }
            if let Some(l) = &self.u_hole_radius {
                gl.uniform_1_f32(Some(l), baked.hole_radius);
            }
            if let Some(l) = &self.u_hole_alpha {
                gl.uniform_1_f32(Some(l), baked.hole_alpha);
            }
            gl.bind_texture(glow::TEXTURE_2D, Some(overlay_tex));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
        true
    }

    /// Draw the borrowed external-OES camera as an opaque passthrough quad covering
    /// the `dst_w×dst_h` canonical frame, mapped to clip by `dst_to_clip` and sampled
    /// through the stored canonical uv transform (upright/crop/flip). No-op if no
    /// external source is set. Caller sets blend/clear state.
    fn draw_camera_passthrough_ext(&self, dst_w: f32, dst_h: f32, dst_to_clip: &[f32; 9]) {
        let (Some((id, uv)), Some(e)) = (self.camera_external, self.ext.as_ref()) else {
            return;
        };
        let cam_transform = mat3_mul(dst_to_clip, &scale(dst_w, dst_h));
        unsafe {
            let gl = &self.gl;
            gl.use_program(Some(e.program));
            gl.enable_vertex_attrib_array(e.a_pos);
            gl.vertex_attrib_pointer_f32(e.a_pos, 2, glow::FLOAT, false, 0, 0);
            gl.uniform_1_i32(Some(&e.u_tex), 0);
            if let Some(id) = NonZeroU32::new(id) {
                gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(glow::NativeTexture(id)));
            }
            gl.uniform_matrix_3_f32_slice(
                Some(&e.u_transform),
                false,
                &to_column_major(&cam_transform),
            );
            gl.uniform_matrix_3_f32_slice(Some(&e.u_uv_xform), false, &to_column_major(&uv));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }

    fn ensure_fbo(&mut self, w: u32, h: u32) {
        if self.fbo_size == Some((w, h)) {
            return;
        }
        let gl = &self.gl;
        unsafe {
            if let Some(t) = self.fbo_tex.take() {
                gl.delete_texture(t);
            }
            let tex = new_texture(gl).expect("create fbo texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            let fbo = match self.fbo {
                Some(f) => f,
                None => {
                    let f = gl.create_framebuffer().expect("create framebuffer");
                    self.fbo = Some(f);
                    f
                }
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            assert_eq!(
                status,
                glow::FRAMEBUFFER_COMPLETE,
                "readback FBO incomplete: status=0x{status:x}"
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.fbo_tex = Some(tex);
            self.fbo_size = Some((w, h));
        }
    }
}

impl Drop for GlesRenderer {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_program(self.program);
            self.gl.delete_buffer(self.quad_vbo);
            if let Some(fbo) = self.fbo {
                self.gl.delete_framebuffer(fbo);
            }
            if let Some(t) = self.fbo_tex {
                self.gl.delete_texture(t);
            }
            if let Some(p) = self.pill.take() {
                self.gl.delete_program(p.program);
            }
            if let Some(p) = self.glyph_prog.take() {
                self.gl.delete_program(p.program);
            }
            if let Some(a) = self.glyph_atlas.take() {
                self.gl.delete_texture(a.texture);
            }
            for (fbo, tex, _, _) in [self.overlay_fbo.take(), self.pill_fbo.take()]
                .into_iter()
                .flatten()
            {
                self.gl.delete_framebuffer(fbo);
                self.gl.delete_texture(tex);
            }
        }
    }
}

fn new_texture(gl: &glow::Context) -> Result<glow::Texture, GlError> {
    unsafe {
        let tex = gl.create_texture().map_err(GlError::ProgramLink)?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        Ok(tex)
    }
}

fn link_program(gl: &glow::Context) -> Result<glow::Program, GlError> {
    link_program_frag(gl, FRAG_SRC)
}

fn link_program_frag(gl: &glow::Context, frag_src: &str) -> Result<glow::Program, GlError> {
    link_program_vert_frag(gl, VERT_SRC, frag_src)
}

fn link_program_vert_frag(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<glow::Program, GlError> {
    unsafe {
        let program = gl.create_program().map_err(GlError::ProgramLink)?;
        let shaders = [
            compile_shader(gl, glow::VERTEX_SHADER, vert_src)?,
            compile_shader(gl, glow::FRAGMENT_SHADER, frag_src)?,
        ];
        for s in shaders {
            gl.attach_shader(program, s);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(GlError::ProgramLink(gl.get_program_info_log(program)));
        }
        for s in shaders {
            gl.detach_shader(program, s);
            gl.delete_shader(s);
        }
        Ok(program)
    }
}

fn compile_shader(gl: &glow::Context, kind: u32, src: &str) -> Result<glow::Shader, GlError> {
    unsafe {
        let shader = gl.create_shader(kind).map_err(GlError::ShaderCompile)?;
        gl.shader_source(shader, src);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            return Err(GlError::ShaderCompile(gl.get_shader_info_log(shader)));
        }
        Ok(shader)
    }
}
