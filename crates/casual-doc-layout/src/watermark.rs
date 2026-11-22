//! Watermarks (`109` OO-006) — the paint half of Word's Design ▸ Watermark.
//!
//! A section's [`Watermark`] is resolved to a per-page stamp by a
//! **post-pagination pass**, the same seam as [`crate::line_number`] and
//! [`crate::page_border`]: it runs off the pagination hot path (so page reuse —
//! the stabilization halt — stays position-free), reads only the final page list,
//! and writes [`Page::watermark`](crate::page::Page::watermark), which
//! [`compose_page`](crate::compose::compose_page) paints *first*, behind
//! everything else on the page.
//!
//! A watermark is furniture, not content: it is not in the flow, carries no caret
//! position, is never selected, and copying the page does not copy the word
//! "DRAFT". That is also why it is not a header shape here even though that is
//! how Word stores it — see [`Watermark`]'s own documentation.
//!
//! # What this decides, and on what authority
//!
//! - **Diagonal is 315°.** Word writes a fixed `rotation:315` on the shape, so
//!   that angle is copied rather than derived from the page.
//! - **Auto size fits the stamp's ROTATED BOX inside 85% of the page** — not 85%
//!   of the line it lies along. Word's "Auto" scales the shape to the page
//!   without publishing a formula; 85% leaves it clear of the edge.
//!
//!   Fitting the text LENGTH to 85% of the diagonal is the obvious reading and it
//!   is wrong: a length `L` at 45° projects a box `L·cos45` wide, so on A4 a stamp
//!   fitted to 0.85 of the 20,622-twip diagonal is 12,395 twips across a
//!   11,906-twip page and runs off both side edges by 0.17in each. It fit on US
//!   Letter by 208 twips, which is why the first test of it passed. So the bound
//!   is on the box, and the box has a height as well as a length: a stamp `L`
//!   long and `k·L` tall needs `L ≤ f · min(width/(cosθ + k·sinθ),
//!   height/(sinθ + k·cosθ))`. Counting the length alone still let the glyphs
//!   of a tall stamp run off the page.
//! - **An authored box wins over the page.** Word's Auto is not a formula: it is
//!   the VML shape box Word measured from the text (`width:527.85pt;
//!   height:131.95pt` for DRAFT), the text stretched along it by
//!   `v:textpath@fitshape`. An imported stamp keeps that box
//!   (`WatermarkText::shape_box`) and its text is fitted to the box's length,
//!   capped by the page fit. A stamp made here has no box and fits the page.
//!
//!   An explicit size is used as given and never scaled — the author's
//!   instruction outranks the fit.
//! - **Semitransparent is 50% alpha**, applied on top of the content's own. Word
//!   writes `<v:fill opacity=".5"/>` for a semitransparent text watermark.
//! - **Washout reuses the picture-opacity seam** rather than a second mechanism:
//!   Word's `gain`/`blacklevel` pair washes an image toward white, and the
//!   existing `PaintItem::Image.opacity` over a white page is the same result for
//!   the case a watermark is used in. A picture watermark over a coloured page
//!   background would differ, and that is recorded rather than hidden.
//! - **The stamp is centred on the page**, not on the content area: Word anchors
//!   it to the margin box centre, which for symmetric margins is the page centre,
//!   and a watermark whose position shifted with an asymmetric binding gutter
//!   would not read as a stamp.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Document, SectionId, Watermark, WatermarkContent, WatermarkLayout, WatermarkText,
};

use crate::model::{ModelPos, ModelRange};

use crate::display::ShapeTransform;
use crate::page::{PaginatedLayout, PlacedWatermark, PlacedWatermarkContent};
use crate::text::{Decoration, GlyphRun, LineShaper, StyledRun};
use crate::units::{Point, Rect, Size, Twip};

/// The reference size the auto-fit measurement is taken at. Advance scales
/// linearly with size, so one shaping at a known size gives the size that fits.
const AUTO_FIT_REFERENCE: Twip = Twip(2_000); // 100pt

/// The fraction of the available span an auto-sized watermark fills.
const AUTO_FIT_FRACTION: f32 = 0.85;

/// Word's `rotation:315` for a diagonal watermark, in 60,000ths of a degree.
const DIAGONAL_ROTATION: i32 = 315 * 60_000;

/// Alpha applied to a semitransparent watermark (`<v:fill opacity=".5"/>`).
const SEMI_TRANSPARENT_ALPHA: f32 = 0.5;

/// Stamps every page's watermark, in place.
///
/// Returns immediately when no section declares one, which is the overwhelming
/// majority of documents. Idempotent: a pure function of the final page list plus
/// the document, so running it twice is the same as running it once — the
/// property that keeps `repaginate == paginate`.
pub(crate) fn place_watermarks(
    layout: &mut PaginatedLayout,
    document: &Document,
    shaper: &dyn LineShaper,
) {
    let sections = &document.definitions().sections;
    if sections.iter().all(|section| section.watermark.is_none()) {
        return;
    }
    // Every page of a section shares its page size, so the stamp is resolved once
    // per (section, size) and cloned onto its pages. A document with a watermark
    // and two hundred pages shapes the text twice, not two hundred times.
    let mut resolved: Vec<(SectionId, Size, Option<PlacedWatermark>)> = Vec::new();
    for page in &mut layout.pages {
        let Some(watermark) = sections
            .iter()
            .find(|section| section.id == page.section)
            .and_then(|section| section.watermark.as_ref())
        else {
            continue;
        };
        let cached = resolved
            .iter()
            .find(|(id, size, _)| *id == page.section && *size == page.page_size);
        let stamp = match cached {
            Some((_, _, stamp)) => stamp.clone(),
            None => {
                let stamp = resolve_watermark(watermark, page.page_size, shaper);
                resolved.push((page.section, page.page_size, stamp.clone()));
                stamp
            }
        };
        page.watermark = stamp;
    }
}

/// Resolves one watermark against one page size, or `None` when it cannot be
/// drawn (empty text, or text that shapes to no glyphs).
fn resolve_watermark(
    watermark: &Watermark,
    page_size: Size,
    shaper: &dyn LineShaper,
) -> Option<PlacedWatermark> {
    let transform = match watermark.layout {
        WatermarkLayout::Diagonal => Some(ShapeTransform {
            rotation: DIAGONAL_ROTATION,
            flip_h: false,
            flip_v: false,
            center: page_centre(page_size),
        }),
        WatermarkLayout::Horizontal => None,
    };
    let content = match &watermark.content {
        WatermarkContent::Text(text) => resolve_text(text, watermark, page_size, shaper)
            .map(|runs| PlacedWatermarkContent::Text { runs })?,
        WatermarkContent::Picture(picture) => PlacedWatermarkContent::Picture {
            media: picture.media.node_id().to_string(),
            rect: picture_rect(picture.scale_percent, page_size),
            opacity: picture_opacity(picture.washout, watermark.semi_transparent),
        },
    };
    Some(PlacedWatermark { content, transform })
}

/// The centre of the page box.
fn page_centre(page_size: Size) -> Point {
    Point::new(
        Twip(page_size.width.raw() / 2),
        Twip(page_size.height.raw() / 2),
    )
}

/// The longest stamp whose rotated box, glyph height included, fits the page,
/// for a stamp `aspect` times as tall as it is long.
///
/// A box `L` long and `k·L` tall at angle θ projects `L·(cosθ + k·sinθ)` across
/// and `L·(sinθ + k·cosθ)` down. Fitting only the length (`k = 0`) is what let
/// a 130pt-tall DRAFT run off both side edges of the page (`109` FID-L-10).
fn fitted_length(layout: WatermarkLayout, page_size: Size, aspect: f32) -> f32 {
    let w = page_size.width.raw() as f32;
    let h = page_size.height.raw() as f32;
    let (sin, cos) = match layout {
        WatermarkLayout::Diagonal => {
            let theta = (f64::from(DIAGONAL_ROTATION) / 60_000.0).to_radians();
            (theta.sin().abs() as f32, theta.cos().abs() as f32)
        }
        WatermarkLayout::Horizontal => (0.0, 1.0),
    };
    let bound = |extent: f32, along: f32, across: f32| {
        let per_unit = along + aspect * across;
        if per_unit > f32::EPSILON {
            extent / per_unit
        } else {
            f32::MAX
        }
    };
    bound(w, cos, sin).min(bound(h, sin, cos))
}

/// Shapes the watermark's words and positions them centred on the page,
/// unrotated — the rotation is the stamp's own transform, applied to the whole
/// group, so the runs here are in ordinary page space.
fn resolve_text(
    text: &WatermarkText,
    watermark: &Watermark,
    page_size: Size,
    shaper: &dyn LineShaper,
) -> Option<Vec<GlyphRun>> {
    if text.text.trim().is_empty() {
        return None;
    }
    let mut color = [text.color.r, text.color.g, text.color.b, text.color.a];
    if watermark.semi_transparent {
        color[3] = (f32::from(color[3]) * SEMI_TRANSPARENT_ALPHA).round() as u8;
    }

    // Auto size: measure once at a reference size, then scale. `size` is a font
    // size, and a run's advance is linear in it, so one measurement is exact
    // rather than an iteration toward a fit.
    let size = match text.size_half_points {
        Some(half_points) => Twip(i32::try_from(half_points).unwrap_or(i32::MAX) * 10),
        None => {
            let reference = shape_once(text, color, AUTO_FIT_REFERENCE, shaper)?;
            let advance = reference.1.raw() as f32;
            if advance <= 0.0 {
                return None;
            }
            // Glyph height per unit of advance: a stamp is a box, and its height
            // turns into width and height on the page as much as its length does.
            let aspect = reference.3.raw().max(0) as f32 / advance;
            let page_fit = fitted_length(watermark.layout, page_size, aspect) * AUTO_FIT_FRACTION;
            // Word's Auto is the shape box it measured from the text; the text
            // is stretched along it (`fitshape`). The page bound still applies,
            // so a box authored for a larger page cannot run off this one.
            //
            // The box's own height is Word's glyph height (`fitshape` stretches
            // the ink, not the line box, into it), so the box is bounded by the
            // page with ITS aspect and no margin: a box that lies inside the
            // page is used as authored. The line box would overstate it.
            let target = text.shape_box.map_or(page_fit, |shape_box| {
                let width = shape_box.width_twips as f32;
                let box_aspect = shape_box.height_twips as f32 / width;
                width.min(fitted_length(watermark.layout, page_size, box_aspect))
            });
            Twip(((AUTO_FIT_REFERENCE.raw() as f32) * (target / advance)).round() as i32)
        }
    };
    let (mut runs, advance, ascent) = {
        let (runs, advance, ascent, _) = shape_once(text, color, size, shaper)?;
        (runs, advance, ascent)
    };
    if runs.is_empty() {
        return None;
    }

    // Centre the shaped line on the page. The shaper placed the runs from the
    // origin with their baselines at the line's ascent, so shifting by the
    // difference puts the line's own box centre at the page's centre.
    let centre = page_centre(page_size);
    let dx = Twip(centre.x.raw() - advance.raw() / 2);
    let dy = Twip(centre.y.raw() - ascent.raw() / 2);
    for run in &mut runs {
        run.origin = Point::new(
            Twip(run.origin.x.raw() + dx.raw()),
            Twip(run.origin.y.raw() + dy.raw()),
        );
    }
    Some(runs)
}

/// Shapes the watermark text at `size`, returning its runs, total advance, the
/// line's ascent, and its glyph height (ascent plus descent).
fn shape_once(
    text: &WatermarkText,
    color: [u8; 4],
    size: Twip,
    shaper: &dyn LineShaper,
) -> Option<(Vec<GlyphRun>, Twip, Twip, Twip)> {
    let styled = StyledRun {
        text: text.text.as_str().into(),
        // The authored family, handed to the shaper so it resolves the face — and
        // its fallbacks, which is what lets a CJK watermark shape at all.
        requested_family: text.font.as_ref().map(|font| font.name.as_str().into()),
        requested_family_kind: None,
        font: crate::fonts::face_id(text.bold, text.italic),
        size,
        character_scale_percent: 100,
        bold: text.bold,
        italic: text.italic,
        letter_spacing: Twip::ZERO,
        color,
        decoration: Decoration::default(),
        highlight: None,
        shading: None,
        baseline_shift: Twip::ZERO,
    };
    let node = NodeId::from_parts(1, 1).expect("1/1 is a valid node id");
    let range = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0));
    // Unwrapped: a watermark is one line however long, and a wrapped stamp is not
    // a thing Word can produce either.
    let layout = shaper.shape_paragraph(&[styled], crate::tabs::unwrapped_constraints(), range);
    let line = layout.lines.first()?;
    let runs: Vec<GlyphRun> = line.runs.clone();
    let advance = runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .fold(Twip::ZERO, |sum, glyph| sum + glyph.advance);
    Some((runs, advance, line.ascent, line.ascent + line.descent))
}

/// The destination box for a picture watermark, centred on the page.
///
/// `None` scale is Word's "Auto": fit the page, keeping the box square because
/// the media's natural aspect ratio is not known here — the renderer scales the
/// decoded image into this box, and the media table carries no dimensions for
/// layout to read. An explicit scale is a percentage of the PAGE's shorter edge
/// for the same reason.
fn picture_rect(scale_percent: Option<u32>, page_size: Size) -> Rect {
    let shorter = page_size.width.raw().min(page_size.height.raw());
    let side = match scale_percent {
        Some(percent) => (i64::from(shorter) * i64::from(percent) / 100) as i32,
        None => (shorter as f32 * AUTO_FIT_FRACTION).round() as i32,
    }
    .max(1);
    Rect::new(
        Point::new(
            Twip((page_size.width.raw() - side) / 2),
            Twip((page_size.height.raw() - side) / 2),
        ),
        Size::new(Twip(side), Twip(side)),
    )
}

/// A picture watermark's opacity in 1000ths of a percent, matching
/// `PaintItem::Image.opacity`. `None` is fully opaque.
fn picture_opacity(washout: bool, semi_transparent: bool) -> Option<u32> {
    // Word's washout is a gain/blacklevel pair that lifts the image toward white.
    // Over the white page a watermark sits on, scaling alpha to the same degree
    // is the same picture; the difference only shows on a coloured page
    // background, which is recorded in the module header rather than papered over.
    match (washout, semi_transparent) {
        (true, _) => Some(20_000),
        (false, true) => Some(50_000),
        (false, false) => None,
    }
}
