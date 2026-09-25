//! Watermark recognition and the header→section lift (`109` OO-006, `105`
//! FID-L-10).
//!
//! # The problem this solves
//!
//! Word has **no watermark element**. Design ▸ Watermark writes a floating VML
//! shape into every header of the section and recognises it again on open by the
//! shape's `id`. So a watermark arrives at this importer looking exactly like an
//! ordinary `w:pict` float, and until this module existed that is what it became:
//! a grey box on the float layer whose words were gone, because a text
//! watermark's words live in `v:textpath@string` — an ATTRIBUTE — and nothing
//! read it.
//!
//! [`Watermark`] is a section property instead (see its own documentation for
//! why), so import has to do the inverse of what Word does: **recognise** the
//! shape, **extract** what the user actually asked for, and **lift** it out of
//! the header onto the section. The lift is not cosmetic — leaving the shape on
//! the float layer as well would paint the stamp twice, once as a header float
//! and once as [`crate::body`]'s section watermark through
//! `casual_doc_layout::watermark`.
//!
//! # How recognition works, and what it deliberately refuses
//!
//! Two arms, deliberately unequal in confidence:
//!
//! 1. **Word's magic shape id** — `PowerPlusWaterMarkObject…` for text,
//!    `WordPictureWatermark…` for a picture. These prefixes are not decoration:
//!    they are how Word itself re-identifies its own watermark, and no other
//!    construct uses them. Trusted anywhere in the document.
//! 2. **Plain-text WordArt geometry** — a `type="#_x0000_t136"` shape carrying a
//!    `v:textpath`. `_x0000_t136` is VML's *plain text* WordArt shapetype, which
//!    is what LibreOffice, Aspose and docx4j write for a watermark instead of
//!    Word's id. This arm is **header-only** ([`Recognised::header_only`]): a
//!    t136 shape in body flow is ordinary WordArt and stays a float, because
//!    lifting it would delete a visible object from the page and re-stamp it on
//!    every page of the section.
//!
//! What is deliberately **not** recognised, each because the false positive costs
//! more than the miss:
//!
//! - A `v:textpath` on any other shapetype (`#_x0000_t137` and up are the arched,
//!   waved and curved WordArt presets). Those are decorative titles, not stamps,
//!   and this importer does not carry warped text paths at all — they remain the
//!   open half of the "Watermarks & WordArt" family.
//! - A washed-out picture (`gain`/`blacklevel`) that does **not** carry the
//!   `WordPictureWatermark` id. Washout is ordinary picture formatting available
//!   from Format ▸ Corrections; a photo someone faded is not a watermark. There
//!   is no geometric arm for pictures for this reason: `#_x0000_t75` is *every*
//!   VML image.
//! - A watermark shape in a **footer**. Word never writes one there, and a
//!   `Watermark` lifted from a footer would silently change where the stamp is
//!   anchored. It is reported and left as a float.
//! - DrawingML watermarks (`a:prstTxWarp`). A modern Word still writes VML for
//!   Design ▸ Watermark, so this covers the producer; a DrawingML-only watermark
//!   is a known gap, recorded on the fidelity page rather than guessed at.
//!
//! # Measurements, and where they come from
//!
//! Every constant below is a value Word writes, cross-checked against the VML
//! reference (ECMA-376 Part 4 §14) — never a number chosen here:
//!
//! - `rotation:315` on the shape is Word's diagonal stamp. It is a fixed angle,
//!   not one derived from the page, which is why [`WatermarkLayout`] names the
//!   choice rather than storing degrees. Anything else — including absent — is
//!   [`WatermarkLayout::Horizontal`], the only other thing Word's dialog offers.
//! - `<v:fill opacity=".5"/>` is Word's "Semitransparent" checkbox.
//! - `font-size:1pt` in `v:textpath@style` is Word's **Auto** size: the shape's
//!   own box does the scaling, and 1pt is a sentinel no user can ask for (the
//!   dialog's smallest size is 8). It maps to `None`, which layout resolves by
//!   fitting the page — what Word draws for the same shape.
//! - `gain`/`blacklevel` on `v:imagedata` is "Washout". Word writes
//!   `gain="19661f" blacklevel="22938f"`; their presence is the flag.

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    FontName, HeaderFooterId, HeaderFooterKind, MAX_WATERMARK_TEXT_BYTES, MediaId, Rgba,
    SectionBoundary, Watermark, WatermarkContent, WatermarkLayout, WatermarkPicture, WatermarkText,
};

use crate::vml::{VmlColor, VmlDrawing};

/// Word's shape-id prefix for a TEXT watermark, matched case-insensitively.
const TEXT_WATERMARK_ID_PREFIX: &str = "powerpluswatermarkobject";

/// Word's shape-id prefix for a PICTURE watermark, matched case-insensitively.
const PICTURE_WATERMARK_ID_PREFIX: &str = "wordpicturewatermark";

/// VML's plain-text WordArt shapetype — the geometry Word's text watermark uses,
/// and the only warped-text type this module will treat as a watermark.
const PLAIN_TEXT_WORDART_TYPE: &str = "_x0000_t136";

/// The angle Word writes for a diagonal watermark, in degrees clockwise.
const DIAGONAL_ROTATION_DEGREES: f64 = 315.0;

/// Tolerance when comparing the authored rotation against 315°, in degrees. Wide
/// enough for the fixed-point spelling (`20643840f` == 315.0) and a producer's
/// decimal rounding; far too narrow to swallow any other angle a dialog offers.
const ROTATION_EPSILON: f64 = 0.5;

/// The `font-size` at or below which a textpath size is Word's **Auto** sentinel
/// rather than a size the user chose, in points.
const AUTO_FONT_SIZE_CEILING_POINTS: f64 = 1.0;

/// Why watermark markup could not be lifted onto a section. Every variant is
/// reported ([`crate::report::Reporter::report_watermark`]) and leaves the shape
/// on the float layer, so the page still shows something.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WatermarkLoss {
    /// The shape is watermark markup but carries nothing stampable: a
    /// `v:textpath` with no `string`, an empty or whitespace-only one, a string
    /// past [`MAX_WATERMARK_TEXT_BYTES`], or a picture watermark whose
    /// `v:imagedata` names no relationship.
    NothingToStamp,
    /// The relationship a picture watermark names is not in the part's media
    /// index, so there is no [`MediaId`] to point the section at.
    UnresolvedMedia,
    /// The markup is in a part a watermark cannot be lifted from — a footer, or
    /// body flow. Word only ever writes one into a header.
    WrongContainer,
    /// A second watermark shape in one header. Word's dialog writes exactly one,
    /// so the first is the watermark and this one is something else's doing; it
    /// stays a float rather than overwriting what was already lifted.
    Duplicate,
}

impl WatermarkLoss {
    /// A stable token naming which way the lift failed, carried in the report so a
    /// caller can tell a broken relationship from a footer-borne stamp without
    /// parsing prose. Stable because callers may match on it.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::NothingToStamp => "nothing-to-stamp",
            Self::UnresolvedMedia => "unresolved-media",
            Self::WrongContainer => "wrong-container",
            Self::Duplicate => "duplicate",
        }
    }
}

/// What a parsed VML shape turned out to be.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Recognised {
    /// The watermark, with a picture's media still named by its relationship id
    /// because resolving it needs the part's media index.
    pub content: RecognisedContent,
    /// The angle.
    pub layout: WatermarkLayout,
    /// Word's "Semitransparent".
    pub semi_transparent: bool,
    /// Whether this recognition is only trustworthy inside a header — true for
    /// the geometric arm (a `#_x0000_t136` WordArt shape with no watermark id),
    /// false when Word's own shape id settled it. A `header_only` recognition
    /// seen anywhere else is **not a watermark at all**: it stays a float and
    /// raises nothing, because a decorative WordArt title is not a loss.
    pub header_only: bool,
}

/// The stampable content of a recognised watermark, before media resolution.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RecognisedContent {
    /// Stamped words, fully extracted.
    Text(WatermarkText),
    /// A stamped image, still naming its relationship.
    Picture {
        /// `v:imagedata@r:id`.
        image_rid: String,
        /// Whether Word's washout pair was present.
        washout: bool,
    },
    /// Watermark markup that cannot become a [`Watermark`].
    Lost(WatermarkLoss),
}

impl Recognised {
    /// Completes the recognition into a model [`Watermark`], given a resolver
    /// from a relationship id to shared media. `Err` carries what was lost.
    pub(crate) fn into_watermark(
        self,
        resolve_media: impl FnOnce(&str) -> Option<MediaId>,
    ) -> Result<Watermark, WatermarkLoss> {
        let content = match self.content {
            RecognisedContent::Text(text) => WatermarkContent::Text(text),
            RecognisedContent::Picture { image_rid, washout } => {
                let media = resolve_media(&image_rid).ok_or(WatermarkLoss::UnresolvedMedia)?;
                WatermarkContent::Picture(WatermarkPicture {
                    media,
                    // Word encodes a picture watermark's scale ONLY in the shape's
                    // CSS box, and the model's `scale_percent` is a percentage of
                    // a natural size the media table does not carry (layout says
                    // the same thing from the other side). Deriving a percentage
                    // from a box whose reference is unknown would be a fabricated
                    // number, so this stays `None` — Word's own "Auto", and the
                    // default its dialog ships with — and layout fits the page.
                    scale_percent: None,
                    washout,
                })
            }
            RecognisedContent::Lost(loss) => return Err(loss),
        };
        Ok(Watermark {
            content,
            layout: self.layout,
            semi_transparent: self.semi_transparent,
        })
    }
}

/// Decides whether one parsed VML shape is watermark markup, and extracts what it
/// stamps. `None` means "an ordinary shape" — the overwhelming majority — and the
/// caller maps it onto the float layer exactly as before.
///
/// O(1) in the document: it reads one already-parsed shape's attributes.
pub(crate) fn recognise(drawing: &VmlDrawing) -> Option<Recognised> {
    let id = drawing
        .id
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    // `type="#_x0000_t136"`; the `#` is a shapetype reference, not part of the id.
    let is_plain_text_wordart = drawing
        .shape_type
        .as_deref()
        .map(|value| value.trim_start_matches('#').to_ascii_lowercase())
        .is_some_and(|value| value == PLAIN_TEXT_WORDART_TYPE);

    let by_text_id = id.starts_with(TEXT_WATERMARK_ID_PREFIX);
    let by_picture_id = id.starts_with(PICTURE_WATERMARK_ID_PREFIX);
    // The geometric arm fires only when Word's id did not already settle it, and
    // only for a shape that actually carries warped text.
    let by_geometry = !by_text_id && !by_picture_id && is_plain_text_wordart;

    let is_watermark_markup =
        by_text_id || by_picture_id || (by_geometry && drawing.textpath.is_some());
    if !is_watermark_markup {
        return None;
    }

    let semi_transparent = is_semi_transparent(drawing);
    let layout = layout_of(drawing);
    let header_only = by_geometry;

    // A picture watermark is identified by Word's id ALONE (see the module header:
    // `#_x0000_t75` is every VML image, and washout is ordinary formatting), so
    // this arm is only ever reached through `by_picture_id`.
    let content = if by_picture_id {
        match drawing.image_rid.as_deref() {
            Some(rid) if !rid.is_empty() => RecognisedContent::Picture {
                image_rid: rid.to_owned(),
                washout: drawing.image_effects.washed_out(),
            },
            _ => RecognisedContent::Lost(WatermarkLoss::NothingToStamp),
        }
    } else {
        match watermark_text(drawing) {
            Some(text) => RecognisedContent::Text(text),
            None => RecognisedContent::Lost(WatermarkLoss::NothingToStamp),
        }
    };

    Some(Recognised {
        content,
        layout,
        semi_transparent,
        header_only,
    })
}

/// Extracts a text watermark's words and typography from the shape's
/// `v:textpath`, or `None` when there is nothing stampable.
///
/// Every bound here is the MODEL's domain (`check_section` in
/// `casual-doc-model/src/v1/document.rs`), enforced at this boundary on purpose:
/// a document that fails validation does not open at all, so a hostile or
/// malformed watermark must degrade to "not a watermark" here rather than take
/// the whole import down.
fn watermark_text(drawing: &VmlDrawing) -> Option<WatermarkText> {
    let textpath = drawing.textpath.as_ref()?;
    // `v:textpath@on="f"` hides the text; a shape whose words are switched off has
    // nothing to stamp, and painting them would ADD ink the producer suppressed.
    if !textpath.on {
        return None;
    }
    let string = textpath.string.as_deref()?;
    // `@string` may carry real line breaks (VML renders a textpath's `\n` as a
    // second line), while the model holds one string that layout shapes as a
    // single unwrapped line. Folding every control character to a space keeps
    // every word visible rather than handing the shaper a control codepoint,
    // which most faces have no glyph for.
    let text: String = string
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    // Non-empty and bounded, both because the model requires it.
    if text.trim().is_empty() || text.len() > MAX_WATERMARK_TEXT_BYTES {
        return None;
    }
    Some(WatermarkText {
        text,
        font: textpath
            .font_family
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| FontName {
                name: name.to_owned(),
            }),
        size_half_points: size_half_points(textpath.font_size_points),
        // The ink is the shape's `fillcolor`, with the `v:fill@opacity` alpha
        // stripped back off: transparency is carried by `semi_transparent`, and
        // leaving the folded alpha here too would halve it twice (layout applies
        // the flag on top of this colour). A shape with no fill colour takes
        // Word's own default watermark grey, which is what its dialog preselects.
        color: ink(drawing.fill.color),
        bold: textpath.bold,
        italic: textpath.italic,
    })
}

/// Word's default watermark ink — `silver`, `#c0c0c0`, the colour its dialog
/// preselects and the one it writes as `fillcolor="silver"`.
const DEFAULT_WATERMARK_GREY: Rgba = Rgba {
    r: 0xc0,
    g: 0xc0,
    b: 0xc0,
    a: 255,
};

/// The watermark's ink: the shape's fill colour at FULL alpha (see
/// [`watermark_text`]), or Word's default grey when the shape declares none.
fn ink(color: Option<VmlColor>) -> Rgba {
    match color {
        Some(color) => Rgba {
            r: color.r,
            g: color.g,
            b: color.b,
            a: 255,
        },
        None => DEFAULT_WATERMARK_GREY,
    }
}

/// Maps a `v:textpath` `font-size` in points onto the model's half-points, or
/// `None` for Word's **Auto**.
///
/// `None` in, `None` out. A size at or below [`AUTO_FONT_SIZE_CEILING_POINTS`] is
/// Word's Auto sentinel, not a size (see the module header). A size outside
/// `w:sz`'s own domain (½pt … 1638pt, which the model checks) also degrades to
/// Auto rather than failing validation and refusing to open the document.
fn size_half_points(points: Option<f64>) -> Option<u32> {
    let points = points?;
    if !points.is_finite() || points <= AUTO_FONT_SIZE_CEILING_POINTS {
        return None;
    }
    let half_points = (points * 2.0).round();
    (2.0..=3_276.0)
        .contains(&half_points)
        .then_some(half_points as u32)
}

/// Whether the shape asks to be drawn faintly.
///
/// Word writes exactly `<v:fill opacity=".5"/>` for its "Semitransparent"
/// checkbox, and the model carries the checkbox rather than an alpha — so ANY
/// authored opacity below opaque sets the flag. A producer that wrote some other
/// fraction (LibreOffice writes `.5` too) is therefore rendered at Word's 50%
/// rather than at its own value: a bounded, documented difference, and the
/// alternative — inventing an alpha field on the model to hold it — would put a
/// VML detail in the middle of the system for a case no dialog can produce.
fn is_semi_transparent(drawing: &VmlDrawing) -> bool {
    drawing.fill.opacity.is_some_and(|alpha| alpha < 255)
}

/// `rotation:315` is diagonal; everything else, absent included, is horizontal.
///
/// Normalised into `[0, 360)` first so the equivalent `-45` a hand-authored file
/// may carry reads as the same stamp.
fn layout_of(drawing: &VmlDrawing) -> WatermarkLayout {
    let Some(degrees) = drawing.rotation_degrees else {
        return WatermarkLayout::Horizontal;
    };
    let normalised = degrees.rem_euclid(360.0);
    if (normalised - DIAGONAL_ROTATION_DEGREES).abs() <= ROTATION_EPSILON {
        WatermarkLayout::Diagonal
    } else {
        WatermarkLayout::Horizontal
    }
}

/// Puts each header's watermark onto the sections that reference that header —
/// the second half of the lift, run once both the header parts and the body's
/// section boundaries exist (`crate::import_with_sources`).
///
/// Word writes the SAME watermark into every header of a section (default, first
/// page, even pages), because the user asked for one watermark, not three. So the
/// first match in a fixed kind order wins and the rest are ignored rather than
/// fought over: `Default` before `First` before `Even`, which is the precedence
/// Word's own "different first page / different odd and even" fallbacks use and —
/// being independent of the order the `w:sectPr` happened to list the references
/// in — is deterministic.
///
/// A section that already carries a watermark is left alone: nothing else sets one
/// at import, but this keeps the function idempotent, which is what makes running
/// it exactly once at the end of the import safe to reason about.
///
/// O(sections × headers-per-section) — three at most per section, so linear in
/// the section count and free of any per-block work.
pub(crate) fn lift_header_watermarks(
    sections: &mut [SectionBoundary],
    by_header: &BTreeMap<HeaderFooterId, Watermark>,
) {
    if by_header.is_empty() {
        return;
    }
    for section in sections {
        if section.watermark.is_some() {
            continue;
        }
        section.watermark = [
            HeaderFooterKind::Default,
            HeaderFooterKind::First,
            HeaderFooterKind::Even,
        ]
        .into_iter()
        .find_map(|kind| {
            section
                .headers
                .iter()
                .find(|reference| reference.kind == kind)
                .and_then(|reference| by_header.get(&reference.reference))
                .cloned()
        });
    }
}
