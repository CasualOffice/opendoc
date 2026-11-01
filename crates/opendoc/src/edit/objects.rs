// SPDX-License-Identifier: Apache-2.0

//! Pictures, headers and footers, notes and sections, by edit (`docs/176`
//! §12.12).
//!
//! Every call lowers through `casual_doc_command`, the one capability registry
//! the editor and the agent front doors share (`docs/181`), and applies through
//! the batch's choke point ([`Editor::apply`], ADR-030 I1), so a picture, a
//! header, a note or a section break inserted here is the same operation the
//! editor's own gesture produces, inside the same all-or-nothing batch with the
//! same exact undo.
//!
//! Sections are named by their 0-based index in body order (python-docx's
//! `document.sections[i]`, POI's section order); headers and footers by
//! `(section, region, kind)`; pictures and notes by the node id the read view
//! shows (a run of kind `"image"`, a note's `id`).

use casual_doc_command::{breaks, fields, notes, objects, sections};
use casual_doc_edit::{Operation, Pos, RunningRegion, SplitProperties};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document as Model, Extent, HeaderFooterKind, InlineNode, NoteId, NoteKind,
    PageOrientation, PageSize, Paragraph, SectionBoundary, SectionId, SectionType,
};
use serde::Deserialize;

use super::{Editor, find_paragraph};
use crate::error::{Error, Result};
use crate::view::parse_id;

/// Twips per point.
const TWIPS_PER_PT: f64 = 20.0;
/// The largest picture any image format here describes, in bytes.
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// The formats a picture may be, in the order their signatures are tried.
const IMAGE_TYPES: [&str; 6] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/bmp",
    "image/tiff",
    "image/webp",
];

/// Which band: the header or the footer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Region {
    /// The header.
    Header,
    /// The footer.
    Footer,
}

/// Which page a header or footer applies to (Word's three variants).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PageKind {
    /// Every page that has no other variant.
    #[default]
    Default,
    /// The section's first page (needs a different first page).
    First,
    /// Even pages (needs different odd and even pages).
    Even,
}

impl Region {
    const fn engine(self) -> RunningRegion {
        match self {
            Self::Header => RunningRegion::Header,
            Self::Footer => RunningRegion::Footer,
        }
    }
}

impl PageKind {
    const fn engine(self) -> HeaderFooterKind {
        match self {
            Self::Default => HeaderFooterKind::Default,
            Self::First => HeaderFooterKind::First,
            Self::Even => HeaderFooterKind::Even,
        }
    }
}

/// A section's page setup to change; `None` leaves a value as it is.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSetup {
    /// Page width in points.
    #[serde(default)]
    pub width_pt: Option<f64>,
    /// Page height in points.
    #[serde(default)]
    pub height_pt: Option<f64>,
    /// `"portrait"` or `"landscape"`. Turning a page sets the flag and, as Word
    /// does, swaps the width and height when they disagree with it (python-docx
    /// sets the flag alone; there a caller swaps them).
    #[serde(default)]
    pub orientation: Option<String>,
    /// Top margin in points.
    #[serde(default)]
    pub top_pt: Option<f64>,
    /// Bottom margin in points.
    #[serde(default)]
    pub bottom_pt: Option<f64>,
    /// Left (start) margin in points.
    #[serde(default)]
    pub left_pt: Option<f64>,
    /// Right (end) margin in points.
    #[serde(default)]
    pub right_pt: Option<f64>,
    /// Header distance from the page's top edge, in points.
    #[serde(default)]
    pub header_pt: Option<f64>,
    /// Footer distance from the page's bottom edge, in points.
    #[serde(default)]
    pub footer_pt: Option<f64>,
    /// Text columns, 1-45.
    #[serde(default)]
    pub columns: Option<u16>,
    /// Space between columns in points.
    #[serde(default)]
    pub column_spacing_pt: Option<f64>,
}

fn twips(points: f64, what: &str) -> Result<i32> {
    if !points.is_finite() || !(0.0..=31_680.0 / TWIPS_PER_PT * 2.0).contains(&points) {
        return Err(Error::invalid(format!(
            "{what} {points} pt is out of range"
        )));
    }
    #[allow(clippy::cast_possible_truncation)] // bounded just above
    Ok((points * TWIPS_PER_PT).round() as i32)
}

fn emu(points: f64, what: &str) -> Result<i64> {
    let value = objects::points_to_emu(points)
        .ok_or_else(|| Error::invalid(format!("{what} {points} pt is out of range")))?;
    if value < 1 {
        return Err(Error::invalid(format!(
            "{what} {points} pt is out of range"
        )));
    }
    Ok(value)
}

/// The start a name gives (`"nextPage"`, `"continuous"`, `"evenPage"`,
/// `"oddPage"`; python-docx's `NEW_PAGE`/`CONTINUOUS`/`EVEN_PAGE`/`ODD_PAGE`
/// and POI-style `new_page` are accepted too).
fn start_type(name: &str) -> Result<SectionType> {
    let squashed = name.replace(['_', '-', ' '], "").to_ascii_lowercase();
    match squashed.as_str() {
        "nextpage" | "newpage" => Ok(SectionType::NextPage),
        "continuous" => Ok(SectionType::Continuous),
        "evenpage" => Ok(SectionType::EvenPage),
        "oddpage" => Ok(SectionType::OddPage),
        "nextcolumn" | "newcolumn" => Ok(SectionType::NextColumn),
        _ => breaks::section_start(name).ok_or_else(|| {
            Error::invalid(format!(
                "\"{name}\" is not a section start (nextPage, continuous, evenPage, oddPage)"
            ))
        }),
    }
}

fn note_kind(name: &str) -> Result<NoteKind> {
    match name.to_ascii_lowercase().as_str() {
        "footnote" | "foot" => Ok(NoteKind::Footnote),
        "endnote" | "end" => Ok(NoteKind::Endnote),
        other => Err(Error::invalid(format!(
            "\"{other}\" is not footnote or endnote"
        ))),
    }
}

/// The media type of an image, from its signature.
fn sniff(image: &[u8]) -> Result<&'static str> {
    if image.is_empty() {
        return Err(Error::invalid("the picture is empty"));
    }
    if image.len() > MAX_IMAGE_BYTES {
        return Err(Error::invalid("the picture is larger than 64 MB"));
    }
    IMAGE_TYPES
        .into_iter()
        .find(|media_type| objects::bytes_are(media_type, image))
        .ok_or_else(|| {
            Error::invalid("the picture is not a PNG, JPEG, GIF, BMP, TIFF or WebP image")
        })
}

/// A picture's size in EMU: the given points, the other side scaled to keep the
/// image's proportions when only one is given, and the image's own size at
/// 96 dpi when neither is (a format with no stated size is one inch square).
fn picture_extent(
    media_type: &str,
    image: &[u8],
    width_pt: Option<f64>,
    height_pt: Option<f64>,
) -> Result<(i64, i64)> {
    let natural = objects::pixel_size(media_type, image)
        .map(|(w, h)| (objects::pixels_to_emu(w), objects::pixels_to_emu(h)))
        .filter(|&(w, h)| w > 0 && h > 0)
        .unwrap_or((914_400, 914_400));
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let scale = |value: i64, by: f64| ((value as f64) * by).round().max(1.0) as i64;
    #[allow(clippy::cast_precision_loss)]
    Ok(match (width_pt, height_pt) {
        (Some(w), Some(h)) => (emu(w, "width")?, emu(h, "height")?),
        (Some(w), None) => {
            let width = emu(w, "width")?;
            (width, scale(natural.1, width as f64 / natural.0 as f64))
        }
        (None, Some(h)) => {
            let height = emu(h, "height")?;
            (scale(natural.0, height as f64 / natural.1 as f64), height)
        }
        (None, None) => natural,
    })
}

/// The paragraph holding the inline object `object`, and the object's inline
/// node, on any surface. **O(document)**: an object edit is one call per
/// object, not per keystroke.
fn object_site(model: &Model, object: NodeId) -> Option<(NodeId, InlineNode)> {
    fn search(inlines: &[InlineNode], object: NodeId) -> Option<InlineNode> {
        for inline in inlines {
            match inline {
                InlineNode::Drawing(d) if d.id == object => return Some(inline.clone()),
                InlineNode::AnchoredDrawing(d) if d.id == object => return Some(inline.clone()),
                InlineNode::Hyperlink(link) => {
                    if let Some(found) = search(&link.inlines, object) {
                        return Some(found);
                    }
                }
                InlineNode::Revision(revision) => {
                    if let Some(found) = search(&revision.inlines, object) {
                        return Some(found);
                    }
                }
                InlineNode::Sdt(sdt) => {
                    if let Some(found) = search(&sdt.inlines, object) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    let mut found = None;
    casual_doc_edit::visit_paragraphs_all_surfaces(model, &mut |p: &Paragraph| {
        if found.is_none()
            && let Some(node) = search(&p.inlines, object)
        {
            found = Some((p.id, node));
        }
    });
    found
}

/// Rewrites the picture `object` in `inlines` to show `media` at `extent`, its
/// crop cleared; every other field is kept. Reports whether it was found.
fn swap_picture(
    inlines: &mut [InlineNode],
    object: NodeId,
    media: casual_doc_model::v1::MediaId,
    extent: Extent,
) -> bool {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Drawing(d) if d.id == object => {
                d.media = media;
                d.extent = Some(extent);
                d.crop = None;
                return true;
            }
            InlineNode::AnchoredDrawing(d) if d.id == object => {
                d.media = media;
                d.extent = extent;
                d.crop = None;
                return true;
            }
            InlineNode::Hyperlink(link) => {
                if swap_picture(&mut link.inlines, object, media, extent) {
                    return true;
                }
            }
            InlineNode::Revision(revision) => {
                if swap_picture(&mut revision.inlines, object, media, extent) {
                    return true;
                }
            }
            InlineNode::Sdt(sdt) => {
                if swap_picture(&mut sdt.inlines, object, media, extent) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// The largest box with `natural`'s proportions inside `frame` (Word's Change
/// Picture keeps the frame and fits the new image in it).
fn fit_within(natural: (i64, i64), frame: Extent) -> Extent {
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    {
        let scale = (frame.width_emu as f64 / natural.0 as f64)
            .min(frame.height_emu as f64 / natural.1 as f64);
        let scaled = |value: i64| ((value as f64 * scale).round() as i64).max(1);
        Extent {
            width_emu: scaled(natural.0),
            height_emu: scaled(natural.1),
        }
    }
}

/// The note reference that marks `note` in the text, as `(kind, reference id)`.
/// **O(document)**.
fn note_reference(model: &Model, note: NoteId) -> Option<(NoteKind, NodeId)> {
    fn search(inlines: &[InlineNode], note: NoteId) -> Option<(NoteKind, NodeId)> {
        for inline in inlines {
            match inline {
                InlineNode::NoteReference(r) if r.note == note => return Some((r.kind, r.id)),
                InlineNode::Hyperlink(link) => {
                    if let Some(found) = search(&link.inlines, note) {
                        return Some(found);
                    }
                }
                InlineNode::Revision(revision) => {
                    if let Some(found) = search(&revision.inlines, note) {
                        return Some(found);
                    }
                }
                InlineNode::Sdt(sdt) => {
                    if let Some(found) = search(&sdt.inlines, note) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    let mut found = None;
    casual_doc_edit::visit_paragraphs_all_surfaces(model, &mut |p: &Paragraph| {
        if found.is_none() {
            found = search(&p.inlines, note);
        }
    });
    found
}

/// The first paragraph among `blocks`, in document order.
fn first_paragraph(blocks: &[BlockNode]) -> Option<NodeId> {
    sections::paragraph_order(blocks).first().copied()
}

impl Editor<'_> {
    /// Applies a lowering's operations, turning its error sentence into an
    /// edit error.
    fn apply_lowered(
        &mut self,
        lowered: std::result::Result<Vec<Operation>, String>,
    ) -> Result<()> {
        self.apply(lowered.map_err(Error::edit)?)
    }

    /// Runs a lowering with the batch's id minter.
    fn lower<T>(
        &mut self,
        f: impl FnOnce(&Model, &mut dyn FnMut() -> Option<NodeId>) -> std::result::Result<T, String>,
    ) -> Result<T> {
        let state = &mut *self.state;
        let model = &state.model;
        let ids = &mut state.ids;
        let mut mint = || ids.next_id().ok();
        f(model, &mut mint).map_err(Error::edit)
    }

    /// A character offset in a paragraph as the engine's position.
    fn pos(&self, paragraph: &str, offset: usize) -> Result<Pos> {
        let node = parse_id(paragraph)?;
        let text = self.paragraph_text(node)?;
        Ok(Pos::new(node, Self::byte_offset(&text, offset)?))
    }

    fn section_boundary(&self, section: usize) -> Result<&SectionBoundary> {
        sections::section_at(self.model(), section).ok_or_else(|| {
            Error::invalid(format!(
                "no section {section}: the document has {}",
                self.model().definitions().sections.len()
            ))
        })
    }

    // -- pictures --

    /// Inserts a picture at character `offset` of a paragraph and returns its
    /// id. The format is read from the bytes (PNG, JPEG, GIF, BMP, TIFF, WebP).
    /// With neither size given it takes the image's own size at 96 dpi; with
    /// one, the other keeps the image's proportions. python-docx's
    /// `run.add_picture`, docx-js's `ImageRun`, POI's `addPicture`.
    ///
    /// # Errors
    ///
    /// An unknown paragraph, an offset past its end, bytes that are not an
    /// image, or a size out of range.
    pub fn insert_picture(
        &mut self,
        paragraph: &str,
        offset: usize,
        image: &[u8],
        width_pt: Option<f64>,
        height_pt: Option<f64>,
    ) -> Result<String> {
        let media_type = sniff(image)?;
        let (width, height) = picture_extent(media_type, image, width_pt, height_pt)?;
        let at = self.pos(paragraph, offset)?;
        let picture =
            self.lower(|_, ids| objects::insert_picture(at, media_type, width, height, ids))?;
        self.apply(picture.operations)?;
        self.media.push((picture.part_name, image.to_vec()));
        Ok(picture.drawing.to_string())
    }

    /// Replaces a picture's image, keeping the picture (its id, position, wrap,
    /// alt text) and its frame: the new image is fitted inside the old frame,
    /// as Word's Change Picture does, and its crop is cleared.
    ///
    /// # Errors
    ///
    /// No picture with that id, or bytes that are not an image.
    pub fn replace_picture(&mut self, picture: &str, image: &[u8]) -> Result<()> {
        let object = parse_id(picture)?;
        let media_type = sniff(image)?;
        let (paragraph, node) = object_site(self.model(), object)
            .ok_or_else(|| Error::invalid(format!("no picture with id {picture}")))?;
        let frame = match &node {
            InlineNode::Drawing(d) => d.extent,
            InlineNode::AnchoredDrawing(d) => Some(d.extent),
            _ => None,
        };
        let natural = picture_extent(media_type, image, None, None)?;
        let extent = frame
            .filter(|f| f.width_emu > 0 && f.height_emu > 0)
            .map_or(
                Extent {
                    width_emu: natural.0,
                    height_emu: natural.1,
                },
                |frame| fit_within(natural, frame),
            );
        // The registration is the same `SetMediaReference` an insertion makes;
        // a zero-size placeholder picture borrows its minted ids and part name.
        let staged = self
            .lower(|_, ids| objects::insert_picture(Pos::new(object, 0), media_type, 1, 1, ids))?;
        let Some(Operation::SetMediaReference { id: media, .. }) =
            staged.operations.first().cloned()
        else {
            return Err(Error::edit("the picture could not be registered"));
        };
        let mut inlines = find_paragraph(self.model(), paragraph)
            .map(|p| p.inlines.clone())
            .ok_or_else(|| Error::invalid(format!("no picture with id {picture}")))?;
        if !swap_picture(&mut inlines, object, media, extent) {
            return Err(Error::invalid(format!("no picture with id {picture}")));
        }
        let registration = staged
            .operations
            .into_iter()
            .next()
            .ok_or_else(|| Error::edit("the picture could not be registered"))?;
        self.apply(vec![
            registration,
            Operation::SetInlines {
                node: paragraph,
                inlines,
            },
        ])?;
        self.media.push((staged.part_name, image.to_vec()));
        Ok(())
    }

    /// Resizes a picture to `width_pt` × `height_pt`.
    ///
    /// # Errors
    ///
    /// No such picture, or a size out of range.
    pub fn resize_picture(&mut self, picture: &str, width_pt: f64, height_pt: f64) -> Result<()> {
        let object = parse_id(picture)?;
        let lowered =
            objects::set_extent(object, emu(width_pt, "width")?, emu(height_pt, "height")?);
        self.apply_lowered(lowered)
    }

    /// Sets a picture's alternative text (`wp:docPr/@descr`); `None` or `""`
    /// clears it.
    ///
    /// # Errors
    ///
    /// No such picture, or an over-long text.
    pub fn set_alt_text(&mut self, picture: &str, text: Option<&str>) -> Result<()> {
        let object = parse_id(picture)?;
        let text = text.filter(|t| !t.is_empty()).map(str::to_owned);
        self.apply(objects::set_alt_text(object, text))
    }

    /// Deletes a picture.
    ///
    /// # Errors
    ///
    /// No such picture.
    pub fn delete_picture(&mut self, picture: &str) -> Result<()> {
        let object = parse_id(picture)?;
        self.apply(objects::delete_object(object))
    }

    // -- headers and footers --

    /// The first paragraph of section `section`'s own `kind` header or footer,
    /// creating it when the section has none of its own. A section that showed
    /// an earlier section's band (Word's "Link to Previous") gets its own copy
    /// of it first, so editing it does not edit the earlier section's, as
    /// python-docx's `is_linked_to_previous = False` and Word's unlink do. Then
    /// the ordinary text, run and paragraph edits address it by that id.
    ///
    /// A `First` band shows only where the section has a different first page,
    /// and an `Even` band only with different odd and even pages; this does
    /// not turn either on ([`Editor::set_different_first_page`],
    /// [`Editor::set_different_odd_even`]).
    ///
    /// # Errors
    ///
    /// No such section.
    pub fn header_footer(
        &mut self,
        section: usize,
        region: Region,
        kind: PageKind,
    ) -> Result<String> {
        let boundary = self.section_boundary(section)?.clone();
        let refs = match region {
            Region::Header => &boundary.headers,
            Region::Footer => &boundary.footers,
        };
        let own = refs
            .iter()
            .find(|r| r.kind == kind.engine())
            .map(|r| r.reference);
        if let Some(body) = own {
            let defs = self.model().definitions();
            let blocks = match region {
                Region::Header => defs.headers.get(&body).map(|h| h.blocks.as_slice()),
                Region::Footer => defs.footers.get(&body).map(|f| f.blocks.as_slice()),
            };
            if let Some(first) = blocks.and_then(first_paragraph) {
                return Ok(first.to_string());
            }
        }
        let inherits = section > 0
            && sections::shown_blocks(self.model(), section, region.engine(), kind.engine())
                .is_some();
        if own.is_none() && inherits {
            let link = self.lower(|model, ids| {
                sections::set_linked_to_previous(
                    model,
                    section,
                    region.engine(),
                    kind.engine(),
                    false,
                    ids,
                )
            })?;
            self.apply(link.operations)?;
        } else {
            let edit = self.lower(|_, ids| {
                sections::create_running_content(boundary.id, region.engine(), kind.engine(), ids)
            })?;
            self.apply(edit.operations)?;
        }
        let blocks = sections::shown_blocks(self.model(), section, region.engine(), kind.engine())
            .ok_or_else(|| Error::edit("the header or footer was not created"))?;
        first_paragraph(blocks)
            .map(|id| id.to_string())
            .ok_or_else(|| Error::edit("the header or footer has no paragraph"))
    }

    /// Removes section `section`'s `kind` header or footer: it shows nothing.
    /// When an earlier section's band would show through, the section gets an
    /// empty one instead, so "removed" means removed (the earlier section's is
    /// never edited).
    ///
    /// # Errors
    ///
    /// No such section, or nothing to remove.
    pub fn remove_header_footer(
        &mut self,
        section: usize,
        region: Region,
        kind: PageKind,
    ) -> Result<()> {
        let ops = self.lower(|model, ids| {
            sections::remove_running_content(model, section, region.engine(), kind.engine(), ids)
        })?;
        self.apply(ops)
    }

    /// Links section `section`'s `kind` band to the previous section's, or
    /// unlinks it with a copy. python-docx's `is_linked_to_previous`.
    ///
    /// # Errors
    ///
    /// The first section, or no such section.
    pub fn set_linked_to_previous(
        &mut self,
        section: usize,
        region: Region,
        kind: PageKind,
        linked: bool,
    ) -> Result<()> {
        let link = self.lower(|model, ids| {
            sections::set_linked_to_previous(
                model,
                section,
                region.engine(),
                kind.engine(),
                linked,
                ids,
            )
        })?;
        if link.operations.is_empty() {
            return Ok(());
        }
        self.apply(link.operations)
    }

    /// Gives section `section` a different first-page header and footer, or not.
    /// python-docx's `different_first_page_header_footer`.
    ///
    /// # Errors
    ///
    /// No such section.
    pub fn set_different_first_page(&mut self, section: usize, on: bool) -> Result<()> {
        let id = self.section_boundary(section)?.id;
        self.apply(sections::set_title_page(id, on))
    }

    /// Different odd and even page headers and footers, document-wide as OOXML
    /// carries it. python-docx's `settings.odd_and_even_pages_header_footer`.
    ///
    /// # Errors
    ///
    /// An engine refusal.
    pub fn set_different_odd_even(&mut self, on: bool) -> Result<()> {
        self.apply(sections::set_even_and_odd(on))
    }

    /// Inserts a field at character `offset` of a paragraph and returns its id:
    /// `"page"`, `"numpages"`, `"date"`, `"time"`, `"filename"` or `"author"`.
    /// `page` and `numpages` are computed at layout; the others show `result`.
    /// docx-js's `PageNumber.CURRENT` / `TOTAL_PAGES`.
    ///
    /// # Errors
    ///
    /// An unknown paragraph or kind.
    pub fn insert_field(
        &mut self,
        paragraph: &str,
        offset: usize,
        kind: &str,
        result: &str,
    ) -> Result<String> {
        let at = self.pos(paragraph, offset)?;
        let ops = self.lower(|_, ids| fields::insert_field(at, kind, result.to_owned(), ids))?;
        let id = ops.iter().find_map(|op| match op {
            Operation::InsertField { field, .. } => Some(field.id.to_string()),
            _ => None,
        });
        self.apply(ops)?;
        id.ok_or_else(|| Error::edit("the field was not created"))
    }

    // -- notes --

    /// Inserts a footnote or endnote (`kind` `"footnote"` or `"endnote"`)
    /// whose reference mark sits at character `offset` of a paragraph, its body
    /// one paragraph holding `text`, and returns the note's id. The number is
    /// derived at layout.
    ///
    /// # Errors
    ///
    /// An unknown paragraph or kind, or an offset past the end.
    pub fn insert_note(
        &mut self,
        kind: &str,
        paragraph: &str,
        offset: usize,
        text: &str,
    ) -> Result<String> {
        let kind = note_kind(kind)?;
        let at = self.pos(paragraph, offset)?;
        let edit = self.lower(|_, ids| notes::insert_note(kind, at, Some(text), ids))?;
        let id = edit.operations.iter().find_map(|op| match op {
            Operation::InsertNote { note, .. } => Some(note.node_id().to_string()),
            _ => None,
        });
        self.apply(edit.operations)?;
        id.ok_or_else(|| Error::edit("the note was not created"))
    }

    /// Replaces a note's text: its first paragraph holds `text` and any further
    /// paragraphs are joined into it first, so the note is one paragraph.
    ///
    /// # Errors
    ///
    /// No note with that id.
    pub fn set_note_text(&mut self, note: &str, text: &str) -> Result<()> {
        let id = NoteId::new(parse_id(note)?);
        let defs = self.model().definitions();
        let blocks = defs
            .footnotes
            .get(&id)
            .or_else(|| defs.endnotes.get(&id))
            .map(|n| n.blocks.clone())
            .ok_or_else(|| Error::invalid(format!("no note with id {note}")))?;
        let order = sections::paragraph_order(&blocks);
        let (&first, rest) = order
            .split_first()
            .ok_or_else(|| Error::invalid(format!("the note {note} has no paragraph")))?;
        let top: Vec<NodeId> = blocks
            .iter()
            .filter_map(|b| match b {
                BlockNode::Paragraph(p) => Some(p.id),
                _ => None,
            })
            .collect();
        if top.len() != blocks.len() || rest.len() + 1 != top.len() {
            return Err(Error::invalid(format!(
                "the note {note} holds a table; edit its paragraphs one by one"
            )));
        }
        for &second in rest {
            self.apply(vec![Operation::JoinParagraphs {
                first,
                second,
                properties: None,
            }])?;
        }
        let first = first.to_string();
        let length = self.paragraph_text(parse_id(&first)?)?.chars().count();
        if length > 0 {
            self.delete_text(&first, 0, length)?;
        }
        self.insert_text(&first, 0, text)
    }

    /// Deletes a note and its reference mark.
    ///
    /// # Errors
    ///
    /// No note with that id.
    pub fn delete_note(&mut self, note: &str) -> Result<()> {
        let id = NoteId::new(parse_id(note)?);
        let (kind, reference_id) = note_reference(self.model(), id)
            .ok_or_else(|| Error::invalid(format!("no note with id {note}")))?;
        self.apply(vec![Operation::RemoveNote {
            kind,
            note: id,
            reference_id,
        }])
    }

    // -- sections --

    /// Adds a section at the end of the document, starting as `start`
    /// (`"nextPage"`, `"continuous"`, `"evenPage"`, `"oddPage"`), and returns
    /// its index. The new section inherits the last one's page setup, headers
    /// and footers, as Word and python-docx's `add_section` do; what follows
    /// is in it.
    ///
    /// # Errors
    ///
    /// An unknown start.
    pub fn add_section(&mut self, start: &str) -> Result<usize> {
        let start = start_type(start)?;
        // As python-docx's `add_section` does, the break goes in a new empty
        // paragraph at the end, which ends the current section. Splitting the
        // last paragraph at its end instead would carry a picture or note mark
        // sitting there (zero characters wide) into the new section.
        let last = self.append_paragraph("", None)?;
        self.break_section(&last, 0, start)
    }

    /// Inserts a section break at character `offset` of a body paragraph: the
    /// text before it ends the current section and the rest starts a new one
    /// as `start`. Returns the new section's index.
    ///
    /// # Errors
    ///
    /// A paragraph that is not top-level in the body, or an unknown start.
    pub fn insert_section_break(
        &mut self,
        paragraph: &str,
        offset: usize,
        start: &str,
    ) -> Result<usize> {
        let start = start_type(start)?;
        self.break_section(paragraph, offset, start)
    }

    fn break_section(
        &mut self,
        paragraph: &str,
        offset: usize,
        start: SectionType,
    ) -> Result<usize> {
        let at = self.pos(paragraph, offset)?;
        let edit = self.lower(|model, ids| breaks::section_break(model, at, start, ids))?;
        let new = edit.operations.iter().find_map(|op| match op {
            Operation::SpliceSectionBoundary {
                boundary: Some(b), ..
            } => Some(b.id),
            _ => None,
        });
        self.apply(edit.operations)?;
        let new = new.ok_or_else(|| Error::edit("the section was not created"))?;
        let index = self
            .model()
            .definitions()
            .sections
            .iter()
            .position(|b| b.id == new)
            .ok_or_else(|| Error::edit("the section was not created"))?;
        // The break copies the section's header and footer references, as Word
        // copies the whole `w:sectPr`, so both sections would name the same
        // bodies. They are cleared instead: the new section then shows the
        // previous one's by inheritance (Link to Previous), which renders the
        // same and is what python-docx's `add_section` writes, so
        // `is_linked_to_previous` answers true until someone unlinks it.
        let boundary = self.section_boundary(index)?.clone();
        let mut unlink = Vec::new();
        for (region, refs) in [
            (RunningRegion::Header, &boundary.headers),
            (RunningRegion::Footer, &boundary.footers),
        ] {
            for reference in refs {
                unlink.push(Operation::SetSectionRunningRef {
                    section: new,
                    region,
                    kind: reference.kind,
                    reference: None,
                });
            }
        }
        if !unlink.is_empty() {
            self.apply(unlink)?;
        }
        Ok(index)
    }

    /// Changes how section `section` starts (`"nextPage"`, `"continuous"`,
    /// `"evenPage"`, `"oddPage"`). python-docx's `section.start_type`.
    ///
    /// The engine has no single "set start" operation, so this is the existing
    /// set composed in one step: a boundary carrying the new start is spliced in
    /// beside the old one, the paragraph that ends the section is pointed at it,
    /// its form protection moves with it, and the old boundary is removed. The
    /// section's id changes; its index, content, headers and footers do not.
    ///
    /// # Errors
    ///
    /// No such section, or an unknown start.
    pub fn set_section_start(&mut self, section: usize, start: &str) -> Result<()> {
        let start = start_type(start)?;
        let old = self.section_boundary(section)?.clone();
        if old.section_type.unwrap_or(SectionType::NextPage) == start && old.section_type.is_some()
        {
            return Ok(());
        }
        let fresh = SectionId::new(self.fresh()?);
        let mut boundary = old.clone();
        boundary.id = fresh;
        boundary.section_type = Some(start);
        let terminator = self.model().body().iter().find_map(|b| match b {
            BlockNode::Paragraph(p) if p.properties.section_break == Some(old.id) => {
                Some((p.id, (*p.properties).clone()))
            }
            _ => None,
        });
        let mut ops = vec![Operation::SpliceSectionBoundary {
            at: Some(old.id),
            boundary: Some(Box::new(boundary)),
        }];
        if let Some((node, mut properties)) = terminator {
            properties.section_break = Some(fresh);
            ops.push(Operation::SetParagraphProperties {
                node,
                properties: Box::new(properties),
            });
        }
        if let Some(protected) = self.model().definitions().section_form_protection(old.id) {
            ops.push(Operation::SetSectionFormProtection {
                section: fresh,
                protected: Some(protected),
            });
            ops.push(Operation::SetSectionFormProtection {
                section: old.id,
                protected: None,
            });
        }
        ops.push(Operation::SpliceSectionBoundary {
            at: Some(old.id),
            boundary: None,
        });
        self.apply(ops)
    }

    /// Changes section `section`'s page size, orientation, margins or columns;
    /// what `setup` leaves `None` stays. python-docx's `section.page_width`,
    /// `orientation`, `left_margin`, …; docx-js's `properties.page`.
    ///
    /// # Errors
    ///
    /// No such section, or a value out of range (the engine refuses a page
    /// over 22 inches).
    pub fn set_page_setup(&mut self, section: usize, setup: &PageSetup) -> Result<()> {
        let boundary = self.section_boundary(section)?.clone();
        let mut size = boundary.page_size;
        let mut margins = boundary.page_margins;
        let mut columns = boundary.columns.clone();
        let mut orientation = boundary.orientation;
        if let Some(w) = setup.width_pt {
            size.width_twips = twips(w, "page width")?;
        }
        if let Some(h) = setup.height_pt {
            size.height_twips = twips(h, "page height")?;
        }
        if let Some(name) = &setup.orientation {
            let wanted = match name.to_ascii_lowercase().as_str() {
                "portrait" => PageOrientation::Portrait,
                "landscape" => PageOrientation::Landscape,
                other => {
                    return Err(Error::invalid(format!(
                        "\"{other}\" is not portrait or landscape"
                    )));
                }
            };
            let wide = size.width_twips > size.height_twips;
            if (wanted == PageOrientation::Landscape) != wide
                && size.width_twips != size.height_twips
            {
                size = PageSize {
                    width_twips: size.height_twips,
                    height_twips: size.width_twips,
                };
            }
            orientation = Some(wanted);
        }
        for (value, slot, what) in [
            (setup.top_pt, &mut margins.top_twips, "top margin"),
            (setup.bottom_pt, &mut margins.bottom_twips, "bottom margin"),
            (setup.left_pt, &mut margins.start_twips, "left margin"),
            (setup.right_pt, &mut margins.end_twips, "right margin"),
        ] {
            if let Some(points) = value {
                *slot = twips(points, what)?;
            }
        }
        if let Some(points) = setup.header_pt {
            margins.header_twips = Some(twips(points, "header distance")?);
        }
        if let Some(points) = setup.footer_pt {
            margins.footer_twips = Some(twips(points, "footer distance")?);
        }
        if let Some(count) = setup.columns {
            if !(1..=45).contains(&count) {
                return Err(Error::invalid(format!("{count} columns is not 1-45")));
            }
            columns.count = count;
            if count == 1 {
                columns.columns.clear();
            }
        }
        if let Some(points) = setup.column_spacing_pt {
            columns.space_twips = Some(twips(points, "column spacing")?);
        }
        self.apply(sections::set_geometry(
            self.model(),
            boundary.id,
            size,
            margins,
            orientation,
            Some(columns),
        ))
    }

    /// Inserts a paragraph after `anchor` when the anchor lives outside the
    /// body (a header, footer, note or comment), where blocks cannot be
    /// inserted by position: the anchor is split at its end, as Enter does, and
    /// the new paragraph takes `text` and `style`. Returns its id.
    pub(super) fn insert_paragraph_after_elsewhere(
        &mut self,
        anchor: NodeId,
        text: &str,
        style: Option<casual_doc_model::v1::StyleId>,
    ) -> Result<String> {
        let source = find_paragraph(self.model(), anchor)
            .ok_or_else(|| Error::invalid(format!("no block with id {anchor}")))?;
        let length = casual_doc_edit::inlines_text_len(&source.inlines);
        let leading = (*source.properties).clone();
        let mut trailing = leading.clone();
        trailing.section_break = None;
        trailing.style_ref = style.or(leading.style_ref);
        let new_id = self.fresh()?;
        self.apply(vec![Operation::SplitParagraph {
            at: Pos::new(anchor, length),
            new_id,
            properties: Some(Box::new(SplitProperties { leading, trailing })),
        }])?;
        let id = new_id.to_string();
        // A split at the end carries no content, but it may carry the end's
        // formatting state; start the new paragraph clean.
        let carried = self.paragraph_text(new_id)?.chars().count();
        if carried > 0 {
            self.delete_text(&id, 0, carried)?;
        }
        self.insert_text(&id, 0, text)?;
        Ok(id)
    }
}
