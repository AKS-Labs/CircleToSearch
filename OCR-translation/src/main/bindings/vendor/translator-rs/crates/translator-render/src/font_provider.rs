//! Font lookup interface for PDF and image output.
//!
//! The translator crate is platform-agnostic: it does not know about
//! `/system/fonts`, `fonts.xml`, `AFontMatcher`, fontconfig, CoreText, etc.
//! Consumer applications (Android, native Linux, …) own that knowledge and
//! expose it through this trait.
//!
//! Both the PDF writer and the image renderer ask for a font when they have
//! decided what to render and what style is needed. The provider returns a
//! preference-ordered chain of TrueType / OpenType files that should cover
//! the requested script + style; the writer picks the first one whose cmap
//! covers the codepoint(s) at hand.

use std::path::PathBuf;

use translator_core::script::Script;

/// What the writer is asking for.
///
/// `script` is the primary key — providers walk their per-script font tables
/// (matches Android's `fonts.xml`, fontconfig's `:lang`/`:charset`, etc.).
/// `language` is a BCP-47 hint, useful mainly for Han disambiguation
/// (`zh-Hans` vs `ja` vs `ko` produce visibly different glyphs from the
/// same Unicode codepoints).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontRequest {
    pub script: Script,
    pub language: String,
    pub bold: bool,
    pub italic: bool,
    pub monospace: bool,
}

/// Path + sub-font index. For `.ttf` / `.otf` set `ttc_index = 0`; for
/// `.ttc` collections (e.g. `NotoSansCJK-Regular.ttc`) the platform's font
/// API tells you which index inside the collection covers the requested
/// script (Android's `AFont_getCollectionIndex()`, fontconfig's `index`
/// property, the `index="N"` attribute in `fonts.xml`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontHandle {
    pub path: PathBuf,
    pub ttc_index: u32,
    /// Resolved font weight (100–900) of the face this handle points at. For a
    /// variable font the file is the same across weights and the renderer drives
    /// the `wght` axis to this value; for a static family it's the weight of the
    /// picked file. `400` is regular. Part of the identity so bold and regular
    /// cache as distinct faces/glyphs even when they share a file.
    pub weight: u16,
}

impl FontHandle {
    pub fn new(path: impl Into<PathBuf>, ttc_index: u32) -> Self {
        Self {
            path: path.into(),
            ttc_index,
            weight: 400,
        }
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }
}

impl<P: Into<PathBuf>> From<P> for FontHandle {
    /// Convenience for the common single-font case where `ttc_index` is 0.
    fn from(path: P) -> Self {
        Self::new(path, 0)
    }
}

/// Resolves a [`FontRequest`] to a preference-ordered chain of fonts on disk.
///
/// The first entry is the primary choice (covers the requested script);
/// subsequent entries are fallbacks the renderer walks when it encounters
/// codepoints the primary doesn't cover (e.g. Latin text embedded inside a
/// Bengali run). An empty `Vec` means "no preference / unsupported"; the PDF
/// writer falls back to the Standard-14 path (Helvetica / Courier), and the
/// image renderer falls back to a tofu glyph.
pub trait FontProvider {
    fn locate(&self, request: &FontRequest) -> Vec<FontHandle>;
}

/// Blanket impl so callers can pass a closure when a one-liner suffices,
/// e.g. integration tests:
/// `&|_req| vec![FontHandle::from("/usr/share/.../X.ttf")]`.
impl<F> FontProvider for F
where
    F: Fn(&FontRequest) -> Vec<FontHandle>,
{
    fn locate(&self, request: &FontRequest) -> Vec<FontHandle> {
        self(request)
    }
}

/// Always returns an empty chain. Use when font discovery isn't wired up
/// yet — the writer keeps its current Standard-14 behavior.
pub struct NoFontProvider;

impl FontProvider for NoFontProvider {
    fn locate(&self, _request: &FontRequest) -> Vec<FontHandle> {
        Vec::new()
    }
}
