// SPDX-License-Identifier: Apache-2.0

//! The browser facade over the presentation engine: a `.pptx` in, pixels out.
//!
//! # Why this crate is the difference between an engine and a product
//!
//! Everything below it works. A deck imports, its three inheritance tiers
//! resolve, its text shapes into glyph runs, its shapes paint, and it writes back
//! out with its unmodelled parts retained. **None of it was reachable**: there was
//! no crate a browser could call, so `docs/109` PRES-01 read "landed; unreachable
//! from the product". This is the boundary that makes it reachable, and it is
//! deliberately thin — it owns no geometry, no shaping and no colour resolution,
//! because all three are already decided one layer down and a facade with
//! opinions is a second engine.
//!
//! # The shape it takes, and why it mirrors the document facade
//!
//! `casual-doc-wasm` is the template: `cdylib` for the browser plus `rlib` so the
//! parity tests exercise the same code natively, one owned handle the host keeps,
//! and errors thrown as JS `Error`s. Following it is not deference — a host that
//! has to learn two boundary idioms for one product is the cost of divergence, and
//! `SKILL` §8 is explicit that a parallel path is evidence the abstraction is
//! wrong.
//!
//! # The one decision this crate makes on its own
//!
//! **It holds the original package bytes.** The importer does not return media
//! content — `Definitions::media` carries part names and relationship ids, not
//! bytes — so a facade that discarded the input could not render a picture. And
//! `casual-pres-export::RetainedParts` needs exactly the same bytes to carry a
//! deck's theme, transitions and animation through a save. One copy serves both:
//! the renderer reads media out of it, and the writer carries the rest of it
//! through. Discarding it would mean either no pictures or no retention, and
//! re-reading the package twice to get both would be the same bytes parsed twice.
//!
//! # What a host can and cannot do with this
//!
//! Open, inspect, render and save. **Not edit** — there is no operation set for a
//! slide, no transaction envelope and no undo. The document side routes every
//! mutation through `casual-doc-transaction` (ADR-005, ADR-043), and a presentation
//! editing surface that bypassed it would repeat the defect `105` CQ-002 records
//! on the document path. So this facade is read-and-render, and that is a stated
//! boundary rather than an unfinished one.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use casual_doc_layout::shape::ParleyShaper;
use casual_doc_render::{RegistryFontSource, Surface, render};
use casual_pres_export::{RetainedParts, export_pptx_retaining};
use casual_pres_import::{ImportLimits, import_pptx};
use casual_pres_layout::{compose_slide, lay_out_slide};
use casual_pres_model::Presentation;
use wasm_bindgen::prelude::*;

#[cfg(test)]
mod tests;

/// Turns anything displayable into a JS `Error`.
fn to_js<E: core::fmt::Display>(error: E) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// The media a deck's pictures resolve through: the original package's own
/// `ppt/media/*` parts.
///
/// A borrowing view over the retained parts rather than a second copy of the
/// bytes. A deck with a 4 MB photograph on every slide is an ordinary deck, and
/// duplicating that into a render-only table would double the facade's footprint
/// in a browser tab for nothing.
struct PackageMedia<'a>(&'a BTreeMap<String, Vec<u8>>);

impl casual_doc_render::MediaSource for PackageMedia<'_> {
    fn media_bytes(&self, media: &str) -> Option<&[u8]> {
        // `Definitions::media` records the part name WITHOUT a leading slash, which
        // is the normalized form the package reader produces, so the lookup is
        // direct. A leading-slash form is accepted too rather than silently missing:
        // an OPC override names a part absolutely, and a caller that hands one
        // through should get its picture rather than a blank gap.
        self.0
            .get(media)
            .or_else(|| self.0.get(media.trim_start_matches('/')))
            .map(Vec::as_slice)
    }
}

/// One opened deck.
///
/// Owned by the host, which keeps it for the life of the document. The shaper is
/// built once and kept: constructing one loads and parses the font bundle, and
/// doing that per rendered slide would put a measurable cost behind every scroll.
#[wasm_bindgen]
pub struct WasmPresentation {
    presentation: Presentation,
    /// The original package, for media on the render path and for retention on
    /// the save path. See the crate documentation for why one copy serves both.
    retained: RetainedParts,
    report: String,
    shaper: ParleyShaper,
}

impl core::fmt::Debug for WasmPresentation {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `ParleyShaper` is opaque and the deck itself is large, so this reports
        // the shape a caller can act on rather than dumping the model. The same
        // choice `WasmDocument` makes, and for the same reason: a debug line that
        // prints a whole document is a line nobody reads.
        formatter
            .debug_struct("WasmPresentation")
            .field("slides", &self.presentation.slides().len())
            .field("masters", &self.presentation.masters().len())
            .field("layouts", &self.presentation.layouts().len())
            .field("retained_parts", &self.retained.parts.len())
            .finish_non_exhaustive()
    }
}

impl core::fmt::Debug for SlideBitmap {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The pixel count rather than the pixels: a full-slide bitmap is megabytes
        // and a debug print of it is unreadable by construction.
        formatter
            .debug_struct("SlideBitmap")
            .field("width_px", &self.width_px)
            .field("height_px", &self.height_px)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

/// Opens a `.pptx` and returns the handle.
///
/// # Errors
///
/// Throws a JS `Error` when the container is refused, the package is not a
/// presentation, or the deck fails the model's own validation. A refusal is
/// deliberately loud: a deck that opens wrong is worse than one that does not
/// open, which is the judgement `MissingSlideSize` already encodes one layer down.
#[wasm_bindgen]
pub fn open(bytes: &[u8]) -> Result<WasmPresentation, JsValue> {
    open_deck(bytes).map_err(to_js)
}

/// The engine's version, from the crate manifest at compile time.
///
/// `CARGO_PKG_VERSION` cannot drift from the manifest because it IS the manifest —
/// the same reason the document facade takes it from there rather than from a
/// string typed into a page (`105` EV-002).
#[wasm_bindgen(js_name = engineVersion)]
#[must_use]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The fallible half of [`open`], free of `JsValue` so the parity tests exercise
/// the same import, the same validation and the same report serialization the
/// browser gets.
fn open_deck(bytes: &[u8]) -> Result<WasmPresentation, String> {
    let imported = import_pptx(
        bytes,
        casual_doc_package::PackageLimits::default(),
        ImportLimits::default(),
    )
    .map_err(|error| format!("open: {error}"))?;
    let retained = RetainedParts::from_package(bytes)
        .map_err(|error| format!("retain the source package: {error}"))?;
    let report = report_json(&imported.report)
        .map_err(|error| format!("serialize the fidelity report: {error}"))?;
    Ok(WasmPresentation {
        presentation: imported.presentation,
        retained,
        report,
        // Without the system faces: a facade that silently used the host's fonts
        // would render one way on the developer's machine and another in a browser
        // with a different bundle, and no guard could tell the difference. The
        // document facade makes the same choice.
        shaper: ParleyShaper::without_system_fonts(),
    })
}

/// One paragraph of a slide's text, as the host sees it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OutlineParagraphJson<'a> {
    /// The zero-based outline level, so a mirror can nest a list rather than
    /// flattening every bullet to one depth.
    level: u8,
    text: &'a str,
}

/// One text-bearing shape of a slide, as the host sees it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OutlineShapeJson<'a> {
    id: String,
    tier: &'static str,
    role: &'static str,
    name: Option<&'a str>,
    paragraphs: Vec<OutlineParagraphJson<'a>>,
}

/// A slide's text, as the host sees it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OutlineJson<'a> {
    shapes: Vec<OutlineShapeJson<'a>>,
}

/// One finding, as the host sees it.
///
/// A bridge struct rather than `serde` on the loss types themselves, for the
/// reason the document facade gives for its own: the taxonomy is engine
/// vocabulary and the JSON is a boundary contract, so letting a derive couple
/// them means a model refactor silently changes a published shape.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FindingJson<'a> {
    feature: &'a str,
    occurrences: u32,
    /// The part the finding is charged to, which is what makes a deck's report
    /// actionable: a gradient lost on slide 7 is a different fact from one lost in
    /// the master.
    part: Option<&'a str>,
    element: Option<&'a str>,
    attribute: Option<&'a str>,
    disposition: &'static str,
    model_outcome: &'static str,
    retention_outcome: &'static str,
}

/// The report, as the host sees it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportJson<'a> {
    findings: Vec<FindingJson<'a>>,
}

/// Serializes a compatibility report for the boundary.
fn report_json(report: &casual_doc_loss::CompatibilityReport) -> Result<String, String> {
    let findings = report
        .entries
        .iter()
        .map(|entry| FindingJson {
            feature: entry.feature.as_str(),
            occurrences: entry.occurrences,
            part: entry.location.part_name.as_deref(),
            element: entry.location.element.as_deref(),
            attribute: entry.location.attribute.as_deref(),
            disposition: disposition_token(entry.disposition),
            model_outcome: model_outcome_token(entry.disposition.model_outcome()),
            retention_outcome: retention_outcome_token(entry.disposition.retention_outcome()),
        })
        .collect();
    serde_json::to_string(&ReportJson { findings }).map_err(|error| error.to_string())
}

/// The boundary token for a disposition.
///
/// Spelled out rather than derived from the variant name: these strings are a
/// published contract a host branches on, and a rename one layer down must be a
/// compile error here rather than a silent change to the JSON.
const fn disposition_token(disposition: casual_doc_loss::Disposition) -> &'static str {
    use casual_doc_loss::Disposition as D;
    match disposition {
        D::MappedComplete => "mapped-complete",
        D::MappedPreserved => "mapped-preserved",
        D::DegradedPreserved => "degraded-preserved",
        D::DegradedNotRetained => "degraded-not-retained",
        D::DegradedBlocked => "degraded-blocked",
        D::OmittedPreserved => "omitted-preserved",
        D::OmittedNotRetained => "omitted-not-retained",
        D::OmittedBlocked => "omitted-blocked",
        D::OmittedRejected => "omitted-rejected",
    }
}

/// The boundary token for a model outcome.
const fn model_outcome_token(outcome: casual_doc_loss::ModelOutcome) -> &'static str {
    use casual_doc_loss::ModelOutcome as M;
    match outcome {
        M::Mapped => "mapped",
        M::Degraded => "degraded",
        M::Omitted => "omitted",
    }
}

/// The boundary token for a retention outcome.
const fn retention_outcome_token(outcome: casual_doc_loss::RetentionOutcome) -> &'static str {
    use casual_doc_loss::RetentionOutcome as R;
    match outcome {
        R::Preserved => "preserved",
        R::NotRetained => "not-retained",
        R::Blocked => "blocked",
        R::Rejected => "rejected",
        R::NotApplicable => "not-applicable",
    }
}

#[wasm_bindgen]
impl WasmPresentation {
    /// How many slides the deck has, in presentation order.
    #[wasm_bindgen(getter, js_name = slideCount)]
    #[must_use]
    pub fn slide_count(&self) -> usize {
        self.presentation.slides().len()
    }

    /// The surface width in EMU (`p:sldSz@cx`).
    ///
    /// EMU rather than pixels, because the surface is a property of the file and
    /// the device scale is the host's. Returning pixels here would bake one zoom
    /// into the document's own dimensions.
    #[wasm_bindgen(getter, js_name = slideWidthEmu)]
    #[must_use]
    pub fn slide_width_emu(&self) -> f64 {
        self.presentation.slide_size().width_emu as f64
    }

    /// The surface height in EMU (`p:sldSz@cy`).
    #[wasm_bindgen(getter, js_name = slideHeightEmu)]
    #[must_use]
    pub fn slide_height_emu(&self) -> f64 {
        self.presentation.slide_size().height_emu as f64
    }

    /// The import fidelity report, as JSON.
    ///
    /// Not a side channel: it is what makes "direct OOXML" an advantage over a
    /// converter rather than a claim, so the host can say "opened with N
    /// unsupported constructs" instead of opening silently. Every entry reads
    /// `Regenerated` — there is no verbatim retention LEDGER on this path yet, so
    /// nothing may claim `preserved`, and `save` carrying bytes through is not the
    /// same as being able to claim them.
    #[wasm_bindgen(getter, js_name = fidelityReport)]
    #[must_use]
    pub fn fidelity_report(&self) -> String {
        self.report.clone()
    }

    /// One slide's author-visible name (`p:cSld@name`), or an empty string.
    ///
    /// An empty string rather than `null`: the slide sorter needs a label for
    /// every slide and a host that had to branch on null for the common case would
    /// write that branch once per surface.
    #[wasm_bindgen(js_name = slideName)]
    #[must_use]
    pub fn slide_name(&self, index: usize) -> String {
        self.presentation
            .slides()
            .get(index)
            .and_then(|slide| slide.name.clone())
            .unwrap_or_default()
    }

    /// Whether a slide is hidden (`p:sld@show="0"`).
    ///
    /// A hidden slide is still IN the deck — it is retained, edited and exported —
    /// so it is reported rather than omitted from the count. A sorter shows it
    /// dimmed; a slide show skips it. That decision belongs to the host.
    #[wasm_bindgen(js_name = slideHidden)]
    #[must_use]
    pub fn slide_hidden(&self, index: usize) -> bool {
        self.presentation
            .slides()
            .get(index)
            .is_some_and(|slide| slide.hidden)
    }

    /// One slide's text as STRUCTURE, for an accessibility mirror.
    ///
    /// # Why a facade needs this at all
    ///
    /// Because [`WasmPresentation::render_slide`] returns PIXELS, and a `<canvas>`
    /// exposes no text, no headings and no list structure. Without this a screen
    /// reader presented with a painted deck gets nothing — while the text has been
    /// in the model and shaped into glyph runs the whole time, which is exactly the
    /// "modelled but unreachable" failure `SKILL` §9.4 names. The document facade
    /// answers the same problem the same way, with a projection its page mirrors
    /// into real elements off-screen.
    ///
    /// # What the JSON says
    ///
    /// `{"shapes":[{"id","tier","role","name","paragraphs":[{"level","text"}]}]}`,
    /// in READING order: the slide's own words first, then the painted furniture it
    /// inherits from its layout and master. The decisions behind that — which text
    /// is a prompt and must never be read, which is a running footer and must be,
    /// and why reading order is not paint order — all live in
    /// [`casual_pres_layout::slide_text_outline`], beside the painter's own copy of
    /// the same rule, so the mirror and the canvas cannot disagree about what the
    /// slide says.
    ///
    /// # Errors
    ///
    /// Throws when the index is out of range, or when the structure cannot be
    /// serialized — which for an in-memory buffer means a bug here.
    #[wasm_bindgen(js_name = slideText)]
    pub fn slide_text(&self, index: usize) -> Result<String, JsValue> {
        self.slide_text_inner(index).map_err(to_js)
    }

    /// The fallible half of [`WasmPresentation::slide_text`], free of `JsValue`.
    fn slide_text_inner(&self, index: usize) -> Result<String, String> {
        let outline = casual_pres_layout::slide_text_outline(&self.presentation, index)
            .ok_or_else(|| format!("no slide at index {index}"))?;
        let shapes = outline
            .shapes
            .iter()
            .map(|shape| OutlineShapeJson {
                // The drawing's own id, stringified: a `NodeId` is 64-bit and
                // JSON's number is a double, so a host that read it as a number
                // would silently round a real file's ids. The document facade
                // stringifies its node ids for the same reason.
                id: shape.id.to_string(),
                tier: shape.tier.token(),
                role: shape.role.token(),
                name: shape.name.as_deref(),
                paragraphs: shape
                    .paragraphs
                    .iter()
                    .map(|paragraph| OutlineParagraphJson {
                        level: paragraph.level,
                        text: paragraph.text.as_str(),
                    })
                    .collect(),
            })
            .collect();
        serde_json::to_string(&OutlineJson { shapes }).map_err(|error| error.to_string())
    }

    /// Renders one slide to an RGBA bitmap at `dpi` device pixels per inch.
    ///
    /// # Errors
    ///
    /// Throws when the index is out of range or the surface cannot be allocated.
    /// A zero or absurd `dpi` reaches the allocator as an absurd size and is
    /// refused there rather than clamped here, so the host gets an error naming
    /// the real cause instead of a silently resized bitmap.
    #[wasm_bindgen(js_name = renderSlide)]
    pub fn render_slide(&self, index: usize, dpi: f32) -> Result<SlideBitmap, JsValue> {
        self.render_slide_inner(index, dpi).map_err(to_js)
    }

    /// Writes the deck back out as a `.pptx`, carrying every part this engine does
    /// not regenerate through unchanged.
    ///
    /// # Errors
    ///
    /// Throws when the package cannot be assembled.
    #[wasm_bindgen(js_name = save)]
    pub fn save(&self) -> Result<Vec<u8>, JsValue> {
        // The media table is the retained parts, filtered — the writer refuses to
        // emit an `a:blip` for media it was not handed, so passing the retained
        // parts whole would be the same thing with a wider argument.
        let media: BTreeMap<String, Vec<u8>> = self
            .retained
            .parts
            .iter()
            .filter(|(name, _)| name.starts_with("ppt/media/"))
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .collect();
        export_pptx_retaining(&self.presentation, &media, &self.retained).map_err(to_js)
    }

    /// The fallible half of [`WasmPresentation::render_slide`], free of `JsValue`.
    fn render_slide_inner(&self, index: usize, dpi: f32) -> Result<SlideBitmap, String> {
        let canvas = lay_out_slide(&self.presentation, index, &self.shaper)
            .ok_or_else(|| format!("no slide at index {index}"))?;
        let width_px = canvas.size.width.to_device_px(dpi).ceil() as u32;
        let height_px = canvas.size.height.to_device_px(dpi).ceil() as u32;

        // The slide's own background, where it states one. White is the fallback
        // rather than transparency: a deck is a printed surface and a transparent
        // slide composited onto a dark host chrome would show black text on black.
        let background = self
            .presentation
            .slides()
            .get(index)
            .and_then(|slide| slide.background.as_ref())
            .map(casual_doc_model::v1::Fill::flat_color);
        let mut surface = match background {
            Some(color) => {
                Surface::with_background(width_px, height_px, [color.r, color.g, color.b])
            }
            None => Surface::with_background(width_px, height_px, [255, 255, 255]),
        }
        .map_err(|error| format!("allocate a {width_px}x{height_px} surface: {error:?}"))?;

        let registry = self.shaper.registry();
        let fonts = RegistryFontSource::new(&registry);
        render(
            &compose_slide(&canvas),
            &mut surface,
            dpi,
            &fonts,
            &PackageMedia(&self.retained.parts),
        );
        Ok(SlideBitmap {
            width_px,
            height_px,
            rgba: surface.data().to_vec(),
        })
    }
}

/// One rendered slide: straight-alpha RGBA, row-major, no padding.
///
/// Shaped for `ImageData`, which is what a canvas host puts it into — so the
/// boundary hands over exactly what `putImageData` wants rather than a layout the
/// host has to repack per frame.
#[wasm_bindgen]
pub struct SlideBitmap {
    width_px: u32,
    height_px: u32,
    rgba: Vec<u8>,
}

#[wasm_bindgen]
impl SlideBitmap {
    /// Bitmap width in device pixels.
    #[wasm_bindgen(getter, js_name = widthPx)]
    #[must_use]
    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    /// Bitmap height in device pixels.
    #[wasm_bindgen(getter, js_name = heightPx)]
    #[must_use]
    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    /// The pixels, moved out rather than copied.
    ///
    /// Taking by value is what keeps a scroll cheap: a copy here would duplicate
    /// a full-page bitmap on every frame, which is the cost the document facade
    /// avoids the same way.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn rgba(self) -> Vec<u8> {
        self.rgba
    }
}
