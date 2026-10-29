//! The faces and CMaps hayro asks for that a PDF does not carry, supplied by
//! the host (`docs/182` §17a, `docs/40`'s one host-populatable registry).
//!
//! hayro-interpret's default features bundle Foxit's standard-14 `.pfb` faces
//! (320 KB) and Adobe's 61 predefined CMaps (256 KB, plus a Brotli decoder).
//! This crate turns both off and installs its own resolvers, so nothing reaches
//! a page that the host did not provide:
//!
//! - **Standard 14.** Each is mapped to a face opendoc already ships
//!   ([`StandardFace::substitute`]): Liberation Sans, Serif and Mono, which are
//!   metric-compatible with Helvetica, Times and Courier. Symbol and
//!   ZapfDingbats have no metric-compatible face in the tree; they resolve to
//!   Liberation Sans and say so ([`FaceRequest::metric_compatible`]). Either
//!   way the evidence does not move: hayro takes a standard font's advances
//!   from its own built-in AFM widths and its Unicode from the glyph-name
//!   tables, and stretches the face's outline to the AFM width; the face only
//!   draws the outline, which the reader never uses. It must still exist, or
//!   hayro does not create the font and the text is not seen at all.
//! - **Predefined CMaps** (`UniJIS-UCS2-H`, `Adobe-Japan1-UCS2`, …), named in
//!   a Type0 font's `/Encoding` or implied by its `/CIDSystemInfo`.
//!   [`crate::PdfSource::required_cmaps`] lists them at admission so a host
//!   can fetch them before the first page; [`provide_cmap`] registers one.
//!   A page that needs one that is absent is refused with
//!   [`crate::PdfReadError::CMapUnavailable`], naming it: text drawn through
//!   an unreadable encoding would otherwise vanish without a finding.
//!
//! Sources, in order: what the host registered ([`provide_face`],
//! [`provide_cmap`]); the host's lazy source ([`set_face_source`]); then, only
//! when compiled in, the native fallbacks: feature `liberation-faces` (the
//! layout crate's own Liberation files) and feature `embedded-cmaps`
//! (hayro-cmap's bundle). Native hosts (`casual-doc-io`'s `pdf-import`, the
//! viewer, the MCP server) turn both on; the browser's PDF module turns
//! neither on and is given faces by the editor's main module.
//!
//! The registry is process-wide because hayro's resolvers are `Send + Sync`
//! closures it calls mid-page and a host provisions once per process. It only
//! grows: at most 14 faces and 61 CMaps (a CMap's bytes are kept for the
//! process, which hayro's `&'static` resolver signature requires).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use hayro_interpret::font::{FontData, FontQuery, StandardFont};
use hayro_interpret::hayro_cmap::CMapName;
use hayro_interpret::{CMapResolverFn, FontResolverFn, InterpreterSettings};

/// One of the PDF standard 14 fonts (ISO 32000-1 §9.6.2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum StandardFace {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    Times,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
    Symbol,
    ZapfDingbats,
}

/// The shipped face a standard font is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FaceRequest {
    /// The family, as the layout crate's bundle names it.
    pub family: &'static str,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Whether the face has the standard font's advance widths.
    pub metric_compatible: bool,
}

impl StandardFace {
    /// Every standard font, in ISO order.
    pub const ALL: [Self; 14] = [
        Self::Helvetica,
        Self::HelveticaBold,
        Self::HelveticaOblique,
        Self::HelveticaBoldOblique,
        Self::Times,
        Self::TimesBold,
        Self::TimesItalic,
        Self::TimesBoldItalic,
        Self::Courier,
        Self::CourierBold,
        Self::CourierOblique,
        Self::CourierBoldOblique,
        Self::Symbol,
        Self::ZapfDingbats,
    ];

    /// Its PostScript name.
    #[must_use]
    pub fn postscript_name(self) -> &'static str {
        match self {
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Self::Times => "Times-Roman",
            Self::TimesBold => "Times-Bold",
            Self::TimesItalic => "Times-Italic",
            Self::TimesBoldItalic => "Times-BoldItalic",
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
            Self::CourierOblique => "Courier-Oblique",
            Self::CourierBoldOblique => "Courier-BoldOblique",
            Self::Symbol => "Symbol",
            Self::ZapfDingbats => "ZapfDingbats",
        }
    }

    /// The shipped face it is drawn with. Liberation Sans, Serif and Mono are
    /// metric-compatible with Helvetica, Times and Courier; nothing shipped is
    /// for Symbol or ZapfDingbats.
    #[must_use]
    pub fn substitute(self) -> FaceRequest {
        let (family, bold, italic, metric_compatible) = match self {
            Self::Helvetica => ("Liberation Sans", false, false, true),
            Self::HelveticaBold => ("Liberation Sans", true, false, true),
            Self::HelveticaOblique => ("Liberation Sans", false, true, true),
            Self::HelveticaBoldOblique => ("Liberation Sans", true, true, true),
            Self::Times => ("Liberation Serif", false, false, true),
            Self::TimesBold => ("Liberation Serif", true, false, true),
            Self::TimesItalic => ("Liberation Serif", false, true, true),
            Self::TimesBoldItalic => ("Liberation Serif", true, true, true),
            Self::Courier => ("Liberation Mono", false, false, true),
            Self::CourierBold => ("Liberation Mono", true, false, true),
            Self::CourierOblique => ("Liberation Mono", false, true, true),
            Self::CourierBoldOblique => ("Liberation Mono", true, true, true),
            Self::Symbol | Self::ZapfDingbats => ("Liberation Sans", false, false, false),
        };
        FaceRequest {
            family,
            bold,
            italic,
            metric_compatible,
        }
    }

    fn from_hayro(font: StandardFont) -> Self {
        match font {
            StandardFont::Helvetica => Self::Helvetica,
            StandardFont::HelveticaBold => Self::HelveticaBold,
            StandardFont::HelveticaOblique => Self::HelveticaOblique,
            StandardFont::HelveticaBoldOblique => Self::HelveticaBoldOblique,
            StandardFont::TimesRoman => Self::Times,
            StandardFont::TimesBold => Self::TimesBold,
            StandardFont::TimesItalic => Self::TimesItalic,
            StandardFont::TimesBoldItalic => Self::TimesBoldItalic,
            StandardFont::Courier => Self::Courier,
            StandardFont::CourierBold => Self::CourierBold,
            StandardFont::CourierOblique => Self::CourierOblique,
            StandardFont::CourierBoldOblique => Self::CourierBoldOblique,
            StandardFont::Symbol => Self::Symbol,
            StandardFont::ZapfDingBats => Self::ZapfDingbats,
        }
    }
}

/// Face bytes, shared rather than copied (hayro's own `FontData`).
pub type FaceBytes = FontData;

/// A host's lazy face source: called the first time a face is needed, from
/// inside page interpretation.
pub type FaceSource = Arc<dyn Fn(&FaceRequest) -> Option<FaceBytes> + Send + Sync>;

#[derive(Default)]
struct Registry {
    faces: BTreeMap<FaceRequest, FaceBytes>,
    source: Option<FaceSource>,
    cmaps: BTreeMap<String, &'static [u8]>,
}

fn registry() -> &'static RwLock<Registry> {
    static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(RwLock::default)
}

/// Registers the bytes `request` resolves to. Later registrations win.
pub fn provide_face(request: FaceRequest, bytes: FaceBytes) {
    if let Ok(mut registry) = registry().write() {
        registry.faces.insert(request, bytes);
    }
}

/// Installs the host's lazy face source (replacing any earlier one).
pub fn set_face_source(source: FaceSource) {
    if let Ok(mut registry) = registry().write() {
        registry.source = Some(source);
    }
}

/// Registers a predefined CMap's file (text or binary CMap). Its bytes are
/// kept for the process. Bounded: a name hayro does not predefine is ignored,
/// so at most 61 are ever kept.
pub fn provide_cmap(name: &str, bytes: Vec<u8>) {
    if matches!(
        CMapName::from_bytes(name.as_bytes()),
        CMapName::Custom(_) | CMapName::IdentityH | CMapName::IdentityV
    ) {
        return;
    }
    if let Ok(mut registry) = registry().write()
        && !registry.cmaps.contains_key(name)
    {
        let kept: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        registry.cmaps.insert(name.to_owned(), kept);
    }
}

/// Whether a predefined CMap is available (registered or compiled in).
#[must_use]
pub fn has_cmap(name: &str) -> bool {
    cmap(name).is_some()
}

fn cmap(name: &str) -> Option<&'static [u8]> {
    if let Some(bytes) = registry()
        .read()
        .ok()
        .and_then(|r| r.cmaps.get(name).copied())
    {
        return Some(bytes);
    }
    let embedded =
        hayro_interpret::hayro_cmap::load_embedded(CMapName::from_bytes(name.as_bytes()));
    // Without `embedded-cmaps`, hayro-cmap answers `None` for every name.
    embedded
}

/// The face `request` resolves to, if any source has it.
#[must_use]
pub fn face(request: &FaceRequest) -> Option<FaceBytes> {
    let source = {
        let registry = registry().read().ok()?;
        if let Some(bytes) = registry.faces.get(request) {
            return Some(bytes.clone());
        }
        registry.source.clone()
    };
    if let Some(bytes) = source.and_then(|source| source(request)) {
        provide_face(*request, bytes.clone());
        return Some(bytes);
    }
    builtin_face(request)
}

#[cfg(any(test, feature = "liberation-faces"))]
fn builtin_face(request: &FaceRequest) -> Option<FaceBytes> {
    macro_rules! face {
        ($file:literal) => {
            &include_bytes!(concat!("../../casual-doc-layout/fonts/liberation/", $file))[..]
        };
    }
    let bytes: &'static [u8] = match (request.family, request.bold, request.italic) {
        ("Liberation Sans", false, false) => face!("LiberationSans-Regular.ttf"),
        ("Liberation Sans", true, false) => face!("LiberationSans-Bold.ttf"),
        ("Liberation Sans", false, true) => face!("LiberationSans-Italic.ttf"),
        ("Liberation Sans", true, true) => face!("LiberationSans-BoldItalic.ttf"),
        ("Liberation Serif", false, false) => face!("LiberationSerif-Regular.ttf"),
        ("Liberation Serif", true, false) => face!("LiberationSerif-Bold.ttf"),
        ("Liberation Serif", false, true) => face!("LiberationSerif-Italic.ttf"),
        ("Liberation Serif", true, true) => face!("LiberationSerif-BoldItalic.ttf"),
        ("Liberation Mono", false, false) => face!("LiberationMono-Regular.ttf"),
        ("Liberation Mono", true, false) => face!("LiberationMono-Bold.ttf"),
        ("Liberation Mono", false, true) => face!("LiberationMono-Italic.ttf"),
        ("Liberation Mono", true, true) => face!("LiberationMono-BoldItalic.ttf"),
        _ => return None,
    };
    Some(Arc::new(bytes))
}

#[cfg(not(any(test, feature = "liberation-faces")))]
fn builtin_face(_request: &FaceRequest) -> Option<FaceBytes> {
    None
}

/// The names of predefined CMaps a page asked for and no source had.
pub(crate) type Misses = Arc<Mutex<BTreeSet<String>>>;

fn font_resolver() -> FontResolverFn {
    Arc::new(|query: &FontQuery| {
        let standard = match query {
            FontQuery::Standard(font) => *font,
            FontQuery::Fallback(fallback) => fallback.pick_standard_font(),
        };
        let request = StandardFace::from_hayro(standard).substitute();
        face(&request).map(|bytes| (bytes, 0))
    })
}

fn cmap_resolver(misses: Misses) -> CMapResolverFn {
    Arc::new(move |name: CMapName<'_>| {
        let key = String::from_utf8_lossy(name.to_bytes()).into_owned();
        let found = cmap(&key);
        // Only a predefined name can be supplied; a custom `usecmap` target
        // hayro cannot fetch from the file either (its own TODO), as before.
        if found.is_none()
            && !matches!(name, CMapName::Custom(_))
            && let Ok(mut misses) = misses.lock()
        {
            misses.insert(key);
        }
        found
    })
}

/// Interpreter settings with this crate's resolvers and `misses` collecting
/// the CMaps nobody had.
pub(crate) fn settings_recording(misses: Misses) -> InterpreterSettings {
    InterpreterSettings {
        font_resolver: font_resolver(),
        cmap_resolver: cmap_resolver(misses),
        ..InterpreterSettings::default()
    }
}

/// Interpreter settings with this crate's face and CMap resolvers: what any
/// other hayro pass over an admitted file (OCR's rasterisation) uses, so it
/// draws with the same faces the reader saw.
#[must_use]
pub fn interpreter_settings() -> InterpreterSettings {
    settings_recording(Misses::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_standard_font_maps_to_a_shipped_face_and_only_symbol_and_dingbats_are_not_metric_compatible()
     {
        let not_compatible: Vec<_> = StandardFace::ALL
            .into_iter()
            .filter(|face| !face.substitute().metric_compatible)
            .map(StandardFace::postscript_name)
            .collect();
        assert_eq!(not_compatible, ["Symbol", "ZapfDingbats"]);
        for face in StandardFace::ALL {
            assert!(
                super::face(&face.substitute()).is_some(),
                "{} resolves to a face",
                face.postscript_name()
            );
        }
    }

    #[test]
    fn a_name_hayro_does_not_predefine_is_never_kept() {
        provide_cmap("Not-A-Predefined-CMap", vec![1, 2, 3]);
        assert!(!has_cmap("Not-A-Predefined-CMap"));
    }
}
