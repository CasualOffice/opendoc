// SPDX-License-Identifier: Apache-2.0

//! Paste as picture (`docs/153` `text.paste-as-picture`): lay clipboard content
//! out with the engine and hand back a PNG of it, trimmed to its ink.
//!
//! Word's Paste Special ▸ Picture and ONLYOFFICE's paste-as-image both turn the
//! clipboard into an image of how it LOOKS. Rasterising in the browser (an HTML
//! fragment drawn into a `<canvas>` through an SVG `foreignObject`) would draw
//! it with the browser's fonts and CSS, not the document's, taint the canvas on
//! any cross-origin image, and still not match what pasting the same content as
//! text would have produced. So the engine does it, with the pipeline that
//! paints pages:
//!
//! 1. a scratch [`Document`] is built with the live document's styles, theme and
//!    defaults, and ONE section whose text column is as wide as the column the
//!    caret is in — so the picture wraps where the pasted text would have;
//! 2. the clipboard's rich runs (the same [`ClipboardRun`] payload `pasteRichRuns`
//!    takes) are applied to it through the ordinary closed op set
//!    (`InsertText`, `FormatText`, `SplitParagraph`) via [`casual_doc_edit::apply`];
//! 3. it is paginated with THIS document's shaper — so host-registered faces
//!    (the browser build registers its fonts at runtime) are the ones used — and
//!    each page is painted by the CPU renderer onto a transparent surface;
//! 4. each page's inked rows are stitched under the previous page's, cropped to
//!    the union of the pages' inked columns, and encoded as PNG.
//!
//! The live document is not touched: the scratch document has its own ids and
//! is dropped when the call returns. Inserting the picture is the host's next
//! call (`insertImage`), which is one undoable action.
//!
//! What a run's `href` carries is not drawn as a link: a picture has no links,
//! and the run's own colour and underline are what the reader saw.
//!
//! # Complexity
//!
//! O(clipboard) to build and lay out, plus O(pixels) to paint, scan and stitch
//! the pages — bounded by [`MAX_PAGES`] and [`MAX_PICTURE_PIXELS`], so a huge
//! clipboard is refused with a reason rather than allocating without bound.
//! Resolving the caret's section is the existing O(document) walk
//! (`section_of`). None of it runs per keystroke.

use casual_doc_edit::refusal;
use casual_doc_edit::{FormatDelta, Mint, Operation, Pos, Range as EditRange};
use casual_doc_layout::compose::{ComposeOptions, compose_page_with};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::formatting_marks::FormattingMarks;
use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, Paragraph, SectionBoundary, VerticalAlignment,
};
use casual_doc_render::{RegistryFontSource, Surface, render};
use wasm_bindgen::prelude::*;

use crate::{
    BorrowedMedia, ClipboardRun, WasmDocument, parse_hex_color, parse_highlight,
    parse_underline_style, to_js,
};

/// Pages of laid-out clipboard a picture may span. Eight Letter pages is a
/// picture taller than any screen; past it the request is a mistake, not a paste.
pub(crate) const MAX_PAGES: usize = 8;

/// The largest picture, in device pixels, this will allocate (≈ 6000 × 6000).
pub(crate) const MAX_PICTURE_PIXELS: u64 = 36_000_000;

/// The resolution range a host may ask for. 96 is one CSS pixel per device pixel;
/// 288 is a 3x display. Outside it a request is clamped, not refused.
const MIN_DPI: f32 = 72.0;
const MAX_DPI: f32 = 288.0;

/// The tallest scratch page, in twips (22in, Word's own page-height ceiling): a
/// taller page means fewer page seams in the middle of a picture.
const SCRATCH_PAGE_HEIGHT: i32 = 31_680;

/// A pasted picture: PNG bytes plus their pixel size and the resolution they were
/// painted at, so the host can size the picture at its true physical size
/// (`widthPx * 96 / dpi` CSS pixels).
#[wasm_bindgen]
#[derive(Debug)]
pub struct PictureRaster {
    png: Vec<u8>,
    width_px: u32,
    height_px: u32,
    dpi: f32,
}

#[wasm_bindgen]
impl PictureRaster {
    /// The PNG bytes.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn png(&self) -> Vec<u8> {
        self.png.clone()
    }

    /// Width in device pixels.
    #[wasm_bindgen(getter, js_name = widthPx)]
    #[must_use]
    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    /// Height in device pixels.
    #[wasm_bindgen(getter, js_name = heightPx)]
    #[must_use]
    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    /// The resolution the picture was painted at, in device pixels per inch.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn dpi(&self) -> f32 {
        self.dpi
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// Paste as picture: lays `runs_json` (the `pasteRichRuns` payload) out at the
    /// width of the column holding `node`, with this document's styles and fonts,
    /// and returns it painted at `dpi` as a PNG trimmed to its ink. Does not
    /// change the document; insert the result with `insertImage`.
    ///
    /// Throws (with a refusal code) when the clipboard has nothing visible
    /// (`paste.picture-empty`) or would make a picture too large to allocate
    /// (`paste.picture-too-large`).
    #[wasm_bindgen(js_name = rasterizeClipboard)]
    pub fn rasterize_clipboard(
        &self,
        node: &str,
        runs_json: &str,
        dpi: f32,
    ) -> Result<PictureRaster, JsValue> {
        self.rasterize_clipboard_inner(node, runs_json, dpi)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// See [`WasmDocument::rasterize_clipboard`].
    pub(crate) fn rasterize_clipboard_inner(
        &self,
        node: &str,
        runs_json: &str,
        dpi: f32,
    ) -> Result<PictureRaster, String> {
        let runs: Vec<ClipboardRun> = serde_json::from_str(runs_json)
            .map_err(|e| format!("invalid clipboard payload: {e}"))?;
        let dpi = if dpi.is_finite() {
            dpi.clamp(MIN_DPI, MAX_DPI)
        } else {
            96.0
        };
        let scratch = self.scratch_document(node, &runs)?;
        let layout = paginate_document(&scratch, &self.shaper);
        if layout.pages.len() > MAX_PAGES {
            return Err(too_large());
        }
        let registry = self.shaper.registry();
        let fonts = RegistryFontSource::new(&registry);
        let media = BorrowedMedia(self.resources.as_map());
        let options = ComposeOptions {
            marks: FormattingMarks::default(),
        };

        // Paint every page and find its ink.
        let mut painted = Vec::with_capacity(layout.pages.len());
        for page in &layout.pages {
            let width_px = page.page_size.width.to_device_px(dpi).ceil() as u32;
            let height_px = page.page_size.height.to_device_px(dpi).ceil() as u32;
            if u64::from(width_px) * u64::from(height_px) > MAX_PICTURE_PIXELS {
                return Err(too_large());
            }
            let mut surface = Surface::transparent(width_px.max(1), height_px.max(1))
                .map_err(|e| format!("allocate surface: {e:?}"))?;
            render(
                &compose_page_with(page, &options),
                &mut surface,
                dpi,
                &fonts,
                &media,
            );
            if let Some(ink) = ink_bounds(&surface) {
                painted.push((surface, ink));
            }
        }
        let Some(picture) = stitch(&painted)? else {
            return Err(refusal::marked(
                "paste.picture-empty",
                "There is nothing visible on the clipboard to paste as a picture.",
            ));
        };
        let png = picture
            .encode_png()
            .map_err(|e| format!("encode picture: {e:?}"))?;
        Ok(PictureRaster {
            png,
            width_px: picture.width(),
            height_px: picture.height(),
            dpi,
        })
    }

    /// The scratch document `runs` are laid out in: this document's styles and
    /// theme, one section as wide as the column holding `node`, and the runs
    /// applied through the closed op set.
    fn scratch_document(&self, node: &str, runs: &[ClipboardRun]) -> Result<Document, String> {
        let mut ids = IdGenerator::new(1);
        let document_id = fresh(&mut ids)?;
        let first = fresh(&mut ids)?;
        let paragraph = || {
            BlockNode::Paragraph(Paragraph {
                id: first,
                properties: Default::default(),
                inlines: Vec::new(),
            })
        };
        let section = self.scratch_section(node);
        let live = self.document.definitions();
        let mut definitions = Definitions {
            styles: live.styles.clone(),
            abstract_numbering: live.abstract_numbering.clone(),
            numbering: live.numbering.clone(),
            document_defaults: live.document_defaults.clone(),
            latent_styles: live.latent_styles.clone(),
            font_table: live.font_table.clone(),
            font_scheme: live.font_scheme.clone(),
            color_scheme: live.color_scheme.clone(),
            ..Definitions::default()
        };
        definitions.sections.extend(section.clone());
        // The live styles are what make the picture look like the paste would
        // have; if they do not validate in isolation (a style naming a part this
        // scratch does not carry), the bare defaults still produce a picture.
        let mut document = match Document::new(document_id, vec![paragraph()], definitions) {
            Ok(document) => document,
            Err(_) => {
                let mut bare = Definitions::default();
                bare.sections.extend(section);
                Document::new(document_id, vec![paragraph()], bare)
                    .map_err(|e| format!("scratch document: {e}"))?
            }
        };

        let mut cursor = Pos::new(first, 0);
        for run in runs {
            if run.paragraph_break {
                let new_id = fresh(&mut ids)?;
                apply(
                    &mut document,
                    &mut ids,
                    &Operation::SplitParagraph {
                        at: cursor,
                        new_id,
                        properties: None,
                    },
                )?;
                cursor = Pos::new(new_id, 0);
                continue;
            }
            if run.text.is_empty() {
                continue;
            }
            let end = Pos::new(cursor.node, cursor.offset + run.text.len() as u32);
            apply(
                &mut document,
                &mut ids,
                &Operation::InsertText {
                    at: cursor,
                    text: run.text.clone(),
                },
            )?;
            let delta = clipboard_run_delta(run)?;
            if delta != FormatDelta::default() {
                apply(
                    &mut document,
                    &mut ids,
                    &Operation::FormatText {
                        range: EditRange { start: cursor, end },
                        delta,
                    },
                )?;
            }
            cursor = end;
        }
        Ok(document)
    }

    /// One section whose single text column is as wide as the column holding
    /// `node` in this document, with nothing on the page but the text column: no
    /// header, footer, border, line numbers or watermark. `None` when this
    /// document has no sections (the layout's own Letter default then applies).
    fn scratch_section(&self, node: &str) -> Option<SectionBoundary> {
        let id = self.section_of(node);
        let sections = &self.document.definitions().sections;
        let live = sections
            .iter()
            .find(|section| section.id.node_id() == id)
            .or_else(|| sections.first())?;
        let mut section = live.clone();
        let margins = &section.page_margins;
        let text_width = section.page_size.width_twips - margins.start_twips - margins.end_twips;
        let columns = i32::from(section.columns.count.max(1));
        let gap = section.columns.space_twips.unwrap_or(720);
        let column = if columns > 1 {
            section
                .columns
                .columns
                .first()
                .map(|first| first.width_twips)
                .unwrap_or((text_width - gap * (columns - 1)) / columns)
        } else {
            text_width
        }
        .max(720);
        section.page_size.width_twips = margins.start_twips + column + margins.end_twips;
        section.page_size.height_twips = SCRATCH_PAGE_HEIGHT;
        section.columns.count = 1;
        section.columns.columns.clear();
        section.columns.separator = None;
        section.headers.clear();
        section.footers.clear();
        section.title_page = None;
        section.vertical_alignment = None;
        section.page_borders = Default::default();
        section.line_numbering = Default::default();
        section.watermark = None;
        section.section_change = None;
        Some(section)
    }
}

/// A fresh scratch identity.
fn fresh(ids: &mut IdGenerator) -> Result<casual_doc_model::NodeId, String> {
    ids.next_id().map_err(|_| "id space exhausted".to_owned())
}

/// Applies one operation to the scratch document in its own mint block, which
/// `Mint::reserve` advances the generator past, so a later [`fresh`] id can
/// never collide with one the operation minted.
fn apply(document: &mut Document, ids: &mut IdGenerator, op: &Operation) -> Result<(), String> {
    let mint = Mint::reserve(ids, 1).ok_or_else(|| "id space exhausted".to_owned())?;
    casual_doc_edit::apply(document, mint, op)
        .map(|_| ())
        .map_err(|e| format!("scratch edit: {e:?}"))
}

/// The formatting a clipboard run asks for, as the delta `pasteRichRuns` applies.
fn clipboard_run_delta(run: &ClipboardRun) -> Result<FormatDelta, String> {
    let underline_style = run
        .underline_style
        .as_deref()
        .map(parse_underline_style)
        .transpose()?;
    let underline_color = run
        .underline_color
        .as_deref()
        .map(|color| {
            parse_hex_color(color)
                .ok_or_else(|| format!("invalid clipboard underline color: {color}"))
        })
        .transpose()?
        .map(Some);
    Ok(FormatDelta {
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        underline_style,
        underline_color,
        strike: run.strike,
        color: run.color.as_deref().and_then(parse_hex_color),
        highlight: run.highlight.as_deref().map(parse_highlight),
        size_half_points: run.size_half_points,
        vertical_alignment: run.vert_align.as_deref().map(|v| match v {
            "super" => VerticalAlignment::Superscript,
            "sub" => VerticalAlignment::Subscript,
            _ => VerticalAlignment::Baseline,
        }),
        font: run.font.clone(),
        rtl: None,
        character: None,
    })
}

/// An inked rectangle in device pixels: `(left, top, right, bottom)`, exclusive.
type Ink = (u32, u32, u32, u32);

fn too_large() -> String {
    refusal::marked(
        "paste.picture-too-large",
        "The clipboard is too large to paste as one picture.",
    )
}

/// The inked rectangle of `surface` — every pixel with any alpha — as
/// `(left, top, right, bottom)` exclusive, or `None` for a blank surface.
/// O(pixels).
pub(crate) fn ink_bounds(surface: &Surface) -> Option<Ink> {
    let width = surface.width() as usize;
    let data = surface.data();
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for (row, pixels) in data.chunks_exact(width * 4).enumerate() {
        for (column, pixel) in pixels.chunks_exact(4).enumerate() {
            if pixel[3] != 0 {
                left = left.min(column);
                right = right.max(column + 1);
                top = top.min(row);
                bottom = bottom.max(row + 1);
            }
        }
    }
    (left != usize::MAX).then_some((left as u32, top as u32, right as u32, bottom as u32))
}

/// Stacks each page's inked rows under the previous page's, cropped to the
/// union of their inked columns. `None` when there is no ink at all.
fn stitch(painted: &[(Surface, Ink)]) -> Result<Option<Surface>, String> {
    let Some(left) = painted.iter().map(|(_, ink)| ink.0).min() else {
        return Ok(None);
    };
    let right = painted.iter().map(|(_, ink)| ink.2).max().unwrap_or(left);
    let width = right.saturating_sub(left).max(1);
    let height: u32 = painted
        .iter()
        .map(|(_, (_, top, _, bottom))| bottom - top)
        .sum::<u32>()
        .max(1);
    if u64::from(width) * u64::from(height) > MAX_PICTURE_PIXELS {
        return Err(too_large());
    }
    let mut picture =
        Surface::transparent(width, height).map_err(|e| format!("allocate picture: {e:?}"))?;
    let mut y = 0i32;
    for (surface, (_, top, _, bottom)) in painted {
        // Rows above `top` and below `bottom` are blank by construction, so
        // compositing the whole page at this offset adds only its inked band.
        picture.draw_surface(surface, -(left as i32), y - *top as i32);
        y += (bottom - top) as i32;
    }
    Ok(Some(picture))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTI: &str = r##"[{"text":"Quarterly results","bold":true,"sizeHalfPoints":36},{"paragraphBreak":true},{"text":"Revenue grew in every region.","color":"#c00000"}]"##;

    fn doc() -> WasmDocument {
        crate::open_document(include_bytes!("../../../sample.docx")).expect("sample opens")
    }

    fn first_body_paragraph(doc: &WasmDocument) -> String {
        doc.document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(p) => Some(p.id.to_string()),
                _ => None,
            })
            .expect("a body paragraph")
    }

    fn decode(png: &[u8]) -> (u32, u32) {
        // IHDR is the first chunk: 8-byte signature, 4 length, 4 type, then
        // big-endian width and height.
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "a PNG signature");
        assert_eq!(&png[12..16], b"IHDR");
        let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
        (width, height)
    }

    #[test]
    fn paste_as_picture_paints_a_png_trimmed_to_the_texts_ink() {
        let doc = doc();
        let node = first_body_paragraph(&doc);
        let before = doc.document.clone();
        let raster = doc
            .rasterize_clipboard_inner(&node, MULTI, 96.0)
            .expect("rasterises");
        let (width, height) = decode(&raster.png);
        assert_eq!((width, height), (raster.width_px, raster.height_px));
        // Trimmed: far narrower than a page (816 px at 96 dpi) and only the height
        // of two short paragraphs, not a page (1056 px).
        assert!(width > 40 && width < 700, "trimmed width, got {width}");
        assert!(height > 20 && height < 200, "two lines tall, got {height}");
        // The document itself is untouched: the scratch is separate.
        assert_eq!(doc.document, before);
    }

    #[test]
    fn the_picture_has_ink_and_scales_with_dpi() {
        let doc = doc();
        let node = first_body_paragraph(&doc);
        let one = doc.rasterize_clipboard_inner(&node, MULTI, 96.0).unwrap();
        let two = doc.rasterize_clipboard_inner(&node, MULTI, 192.0).unwrap();
        let ratio = f64::from(two.width_px) / f64::from(one.width_px);
        assert!(
            (1.8..2.2).contains(&ratio),
            "2x dpi is ~2x wide, got {ratio}"
        );
        assert!((two.dpi - 192.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_long_line_wraps_at_the_columns_width_not_beyond_it() {
        let doc = doc();
        let node = first_body_paragraph(&doc);
        let long = format!(r#"[{{"text":"{}"}}]"#, "wrap these words ".repeat(60));
        let raster = doc.rasterize_clipboard_inner(&node, &long, 96.0).unwrap();
        // The LIVE document's column, read from the model, not from the scratch
        // section this is testing (a single-column sample, so the text width).
        let section = doc.document.definitions().sections[0].clone();
        assert_eq!(section.columns.count, 1, "the premise: one column");
        let column = section.page_size.width_twips
            - section.page_margins.start_twips
            - section.page_margins.end_twips;
        // twips -> px at 96 dpi is / 15.
        let column_px = (column / 15) as u32;
        assert!(
            raster.width_px <= column_px + 2,
            "{} px wraps inside the {column_px} px column",
            raster.width_px
        );
        assert!(raster.height_px > 60, "wrapped onto several lines");
    }

    #[test]
    fn a_clipboard_with_nothing_visible_is_refused_with_a_reason() {
        let doc = doc();
        let node = first_body_paragraph(&doc);
        let err = doc
            .rasterize_clipboard_inner(&node, r#"[{"text":"   "}]"#, 96.0)
            .expect_err("blank");
        assert!(err.contains("paste.picture-empty"), "{err}");
    }

    #[test]
    fn ink_bounds_finds_the_painted_rectangle() {
        let mut surface = Surface::transparent(10, 10).unwrap();
        let dot = Surface::new(2, 3).unwrap();
        surface.draw_surface(&dot, 4, 5);
        assert_eq!(ink_bounds(&surface), Some((4, 5, 6, 8)));
        assert_eq!(ink_bounds(&Surface::transparent(3, 3).unwrap()), None);
    }
}
