//! Footnote and endnote numbering and placement options — the layout consumer for
//! `w:footnotePr`/`w:endnotePr` (`docs/105` FID-L-05).
//!
//! The model has carried `w:pos`, `w:numFmt`, `w:numStart` and `w:numRestart`
//! (per section *and* as a document default in `w:settings`) for a long time with
//! **no reader**, so every note printed a decimal number, counted continuously
//! from 1, and sat at the page bottom whatever the document said. Legal and
//! academic documents depend on all four.
//!
//! This module resolves them into two things the rest of the engine consumes:
//!
//! - [`NoteLabels`] — one *display label* per referenced note, so the reference
//!   marker in the body and the auto-number at the head of the note body print the
//!   same formatted string (`crate::flow`). Numbering is driven by the
//!   **document-order reference sequence**, not by definition-map order, because
//!   `w:numRestart` is defined over that sequence.
//! - [`ResolvedNoteProps`] — the effective options for one section and note kind,
//!   which `crate::notes` reads for footnote placement (`pageBottom` vs
//!   `beneathText`) and `crate::document_layout` reads for endnote placement
//!   (`docEnd` vs `sectEnd`).
//!
//! # Property resolution order
//!
//! Field by field: the section's own `w:footnotePr`/`w:endnotePr`, then the
//! document default in `w:settings`, then the `ST_` implicit default. Word writes
//! both levels and expects the section to win per *field*, not per element, so an
//! authored per-section `w:numRestart` must not discard a document-default
//! `w:numFmt`.
//!
//! # `eachPage` needs pages, so it is a second pass
//!
//! `continuous` and `eachSect` are functions of document order alone and resolve
//! before anything is flowed. `eachPage` is not: the number depends on which page
//! the reference landed on, and the marker's width (`1` versus `12`) feeds back
//! into line breaking. `crate::document_layout` therefore paginates once with the
//! page-independent numbering, re-resolves labels against the produced pages, and
//! repaginates — a bounded fixed point, the same shape as the footnote-band
//! reservation loop in `crate::notes`.
//!
//! # Deliberate limits, recorded rather than hidden
//!
//! - A note referenced more than once keeps **one** label (its first). Word cannot
//!   author a second reference to the same note, and one label per note is what
//!   the previous definition-order numbering also produced.
//! - A note reference inside running content (a header or footer) does not take
//!   part in the page-restart sequence, matching `crate::notes`, which already
//!   documents that header markers are visible metadata and reserve no band.
//! - An authored `w:separator`/`w:continuationSeparator` is still **not** honoured:
//!   the import drops non-`normal` note definitions and [`Note`] carries no type
//!   discriminator, so the authored separator never reaches layout. That is a
//!   model + import change, not a layout one.
//!
//! [`Note`]: casual_doc_model::v1::Note

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    BlockNode, Document, GroupChild, InlineNode, NoteId, NoteKind, NoteNumberRestart, NotePosition,
    NoteProperties, NumberFormat, ReviewProjection, SectionBoundary, SectionId,
};

use crate::numbering::format_number;
use crate::page::PaginatedLayout;

/// The effective `w:footnotePr`/`w:endnotePr` options for one section and note
/// kind, with every field resolved through section → document default → implicit
/// default.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedNoteProps {
    /// Where the notes are placed (`w:pos`).
    pub(crate) position: NotePosition,
    /// The `w:numFmt` the label is rendered with.
    pub(crate) format: NumberFormat,
    /// The `w:numStart` origin, clamped to a non-negative counter value.
    pub(crate) start: u32,
    /// The `w:numRestart` policy.
    pub(crate) restart: NoteNumberRestart,
}

/// The implicit default position for a note kind: footnotes at the page bottom,
/// endnotes at the end of the document (`ST_FtnPos`/`ST_EdnPos` defaults).
fn default_position(kind: NoteKind) -> NotePosition {
    match kind {
        NoteKind::Footnote => NotePosition::PageBottom,
        NoteKind::Endnote => NotePosition::DocumentEnd,
    }
}

/// Whether `position` is meaningful for `kind`. `w:pos` shares one attribute name
/// across two different vocabularies (`ST_FtnPos` has `pageBottom`/`beneathText`,
/// `ST_EdnPos` has `sectEnd`/`docEnd`), and a document that carries the other
/// kind's token is ignored rather than mis-placed.
fn position_applies(kind: NoteKind, position: NotePosition) -> bool {
    match kind {
        NoteKind::Footnote => matches!(
            position,
            NotePosition::PageBottom | NotePosition::BeneathText
        ),
        NoteKind::Endnote => matches!(
            position,
            NotePosition::SectionEnd | NotePosition::DocumentEnd
        ),
    }
}

/// Resolves one note container's options from the section layer over the document
/// default layer, field by field.
fn resolve_props(
    kind: NoteKind,
    section: Option<&NoteProperties>,
    defaults: &NoteProperties,
) -> ResolvedNoteProps {
    let layered = |pick: &dyn Fn(&NoteProperties) -> bool| -> Option<&NoteProperties> {
        section
            .filter(|props| pick(props))
            .or(Some(defaults).filter(|props| pick(props)))
    };
    let position = layered(&|props: &NoteProperties| {
        props
            .position
            .is_some_and(|position| position_applies(kind, position))
    })
    .and_then(|props| props.position)
    .unwrap_or_else(|| default_position(kind));
    let format = layered(&|props: &NoteProperties| props.number_format.is_some())
        .and_then(|props| props.number_format.clone())
        .unwrap_or(NumberFormat::Decimal);
    let start = layered(&|props: &NoteProperties| props.number_start.is_some())
        .and_then(|props| props.number_start)
        .unwrap_or(1)
        .max(0) as u32;
    let restart = layered(&|props: &NoteProperties| props.number_restart.is_some())
        .and_then(|props| props.number_restart)
        .unwrap_or(NoteNumberRestart::Continuous);
    ResolvedNoteProps {
        position,
        format,
        start,
        restart,
    }
}

/// The effective note options for `section` (or the document default when the
/// reference belongs to no declared section) and `kind`.
pub(crate) fn note_props_for_section(
    document: &Document,
    section: Option<&SectionBoundary>,
    kind: NoteKind,
) -> ResolvedNoteProps {
    let settings = &document.definitions().settings;
    let defaults = match kind {
        NoteKind::Footnote => &settings.footnote_props,
        NoteKind::Endnote => &settings.endnote_props,
    };
    let own = section.map(|section| match kind {
        NoteKind::Footnote => &section.footnote_props,
        NoteKind::Endnote => &section.endnote_props,
    });
    resolve_props(kind, own, defaults)
}

/// The effective note options for the section identified by `id`.
pub(crate) fn note_props_for_section_id(
    document: &Document,
    id: SectionId,
    kind: NoteKind,
) -> ResolvedNoteProps {
    let section = document
        .definitions()
        .sections
        .iter()
        .find(|section| section.id == id);
    note_props_for_section(document, section, kind)
}

/// One note's display label per referenced note definition, plus whether any
/// section asks for `eachPage` restart (which is what makes the second pagination
/// pass necessary).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NoteLabels {
    footnotes: BTreeMap<NoteId, String>,
    endnotes: BTreeMap<NoteId, String>,
    restarts_each_page: bool,
}

impl NoteLabels {
    /// The label for one note, or `None` when the note is never referenced in the
    /// body (its number would not be shown anywhere).
    #[must_use]
    pub(crate) fn label(&self, kind: NoteKind, note: NoteId) -> Option<&str> {
        self.map(kind).get(&note).map(String::as_str)
    }

    /// Whether any resolved note container restarts numbering each page, so the
    /// labels depend on pagination and must be re-resolved against it.
    #[must_use]
    pub(crate) fn restarts_each_page(&self) -> bool {
        self.restarts_each_page
    }

    fn map(&self, kind: NoteKind) -> &BTreeMap<NoteId, String> {
        match kind {
            NoteKind::Footnote => &self.footnotes,
            NoteKind::Endnote => &self.endnotes,
        }
    }

    fn map_mut(&mut self, kind: NoteKind) -> &mut BTreeMap<NoteId, String> {
        match kind {
            NoteKind::Footnote => &mut self.footnotes,
            NoteKind::Endnote => &mut self.endnotes,
        }
    }
}

/// One note reference in document order, with the section it belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NoteRef {
    kind: NoteKind,
    note: NoteId,
    section: Option<SectionId>,
}

/// Resolves every referenced note's display label.
///
/// `layout` supplies the page each reference landed on; pass `None` on the first
/// pass (nothing is paginated yet) and the produced layout on the second. With
/// `None`, an `eachPage` container behaves like `continuous` — a defined
/// intermediate state, and [`NoteLabels::restarts_each_page`] is what tells the
/// driver a second pass is owed.
#[must_use]
pub(crate) fn resolve_note_labels(
    document: &Document,
    layout: Option<&PaginatedLayout>,
) -> NoteLabels {
    let mut labels = NoteLabels::default();
    let references = document_order_note_refs(document);
    // Only a document that both asks for page restart *and* has a note to number
    // owes the driver a second pagination pass; a `w:settings` container carrying
    // the policy with no reference in the body must not cost a whole extra layout.
    labels.restarts_each_page =
        !references.is_empty() && any_container_restarts_each_page(document);
    let pages = layout.map(note_pages);
    for kind in [NoteKind::Footnote, NoteKind::Endnote] {
        let mut counter: u32 = 0;
        let mut started = false;
        let mut previous_section: Option<Option<SectionId>> = None;
        let mut previous_page: Option<usize> = None;
        for reference in references.iter().filter(|r| r.kind == kind) {
            // One label per note: a repeated reference is not a second note and
            // must not consume a number.
            if labels.map(kind).contains_key(&reference.note) {
                continue;
            }
            // A break whose id does not resolve falls back to the first section's
            // properties, the same rule `document_layout::section_break_points`
            // applies to that section's geometry, so a malformed document still
            // numbers consistently with how it is laid out.
            let section = reference
                .section
                .and_then(|id| find_section(document, id))
                .or_else(|| document.definitions().sections.first());
            let props = note_props_for_section(document, section, kind);
            let page = pages
                .as_ref()
                .and_then(|pages| pages.get(kind, reference.note));
            let restart = !started
                || match props.restart {
                    NoteNumberRestart::Continuous => false,
                    NoteNumberRestart::EachSection => previous_section != Some(reference.section),
                    // Without a layout there is no page to compare, so the
                    // counter simply continues (see the doc comment).
                    NoteNumberRestart::EachPage => page.is_some() && previous_page != page,
                };
            counter = if restart {
                props.start
            } else {
                counter.saturating_add(1)
            };
            started = true;
            previous_section = Some(reference.section);
            previous_page = page;
            labels
                .map_mut(kind)
                .insert(reference.note, format_number(counter, &props.format));
        }
    }
    labels
}

fn find_section(document: &Document, id: SectionId) -> Option<&SectionBoundary> {
    document
        .definitions()
        .sections
        .iter()
        .find(|section| section.id == id)
}

/// Whether any note container in the document — a section's or the document
/// default — asks for `eachPage` restart.
fn any_container_restarts_each_page(document: &Document) -> bool {
    let sections = &document.definitions().sections;
    [NoteKind::Footnote, NoteKind::Endnote]
        .into_iter()
        .any(|kind| {
            std::iter::once(None)
                .chain(sections.iter().map(Some))
                .any(|section| {
                    note_props_for_section(document, section, kind).restart
                        == NoteNumberRestart::EachPage
                })
        })
}

/// The page index each note was first referenced from, split by kind because
/// [`NoteKind`] is not ordered (so it cannot key a map jointly with the id).
#[derive(Clone, Debug, Default)]
struct NotePages {
    footnotes: BTreeMap<NoteId, usize>,
    endnotes: BTreeMap<NoteId, usize>,
}

impl NotePages {
    fn get(&self, kind: NoteKind, note: NoteId) -> Option<usize> {
        self.map(kind).get(&note).copied()
    }

    fn map(&self, kind: NoteKind) -> &BTreeMap<NoteId, usize> {
        match kind {
            NoteKind::Footnote => &self.footnotes,
            NoteKind::Endnote => &self.endnotes,
        }
    }

    fn map_mut(&mut self, kind: NoteKind) -> &mut BTreeMap<NoteId, usize> {
        match kind {
            NoteKind::Footnote => &mut self.footnotes,
            NoteKind::Endnote => &mut self.endnotes,
        }
    }
}

/// The page each note was first referenced from, read off a produced layout's body
/// fragments in page then reading order. Running content is excluded: a header
/// marker reserves no band (see `crate::notes`) and repeats on every page, so it
/// cannot own a place in a page-restart sequence.
fn note_pages(layout: &PaginatedLayout) -> NotePages {
    let mut pages = NotePages::default();
    for (index, page) in layout.pages.iter().enumerate() {
        for placed in &page.placed {
            crate::notes::collect_fragment_note_markers(&placed.fragment, &mut |marker| {
                pages
                    .map_mut(marker.kind)
                    .entry(marker.note)
                    .or_insert(index);
            });
        }
    }
    pages
}

/// Every note reference in the body, in document order, tagged with the section
/// whose `w:sectPr` governs it.
fn document_order_note_refs(document: &Document) -> Vec<NoteRef> {
    let sections = &document.definitions().sections;
    let body = document.body();
    // `(end_exclusive, section)` cut points: the paragraph carrying a section
    // break is the last block of that section.
    let cuts: Vec<(usize, SectionId)> = body
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            BlockNode::Paragraph(paragraph) => {
                paragraph.properties.section_break.map(|id| (index + 1, id))
            }
            _ => None,
        })
        .collect();
    let trailing = sections.last().map(|section| section.id);
    let mut refs = Vec::new();
    for (index, block) in body.iter().enumerate() {
        let section = cuts
            .iter()
            .find(|(end, _)| index < *end)
            .map(|(_, id)| *id)
            .or(trailing);
        visit_block_note_refs(block, &mut |kind, note| {
            refs.push(NoteRef {
                kind,
                note,
                section,
            });
        });
    }
    refs
}

/// Visits every note reference in `block` (recursing through tables, structured
/// document tags, inline and grouped text boxes, hyperlinks, and fields) in
/// document order.
///
/// This is the one note-reference walker in the crate: `crate::document_layout`'s
/// endnote collection uses it too, so a new container that can hold a reference is
/// taught to both at once.
pub(crate) fn visit_block_note_refs(block: &BlockNode, f: &mut impl FnMut(NoteKind, NoteId)) {
    match block {
        BlockNode::Paragraph(paragraph) => visit_inline_note_refs(&paragraph.inlines, f),
        BlockNode::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for block in &cell.blocks {
                        visit_block_note_refs(block, f);
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for block in &sdt.blocks {
                visit_block_note_refs(block, f);
            }
        }
        BlockNode::AltChunk(_) => {}
    }
}

fn visit_inline_note_refs(inlines: &[InlineNode], f: &mut impl FnMut(NoteKind, NoteId)) {
    for inline in inlines {
        match inline {
            InlineNode::NoteReference(reference) => f(reference.kind, reference.note),
            InlineNode::Hyperlink(hyperlink) => visit_inline_note_refs(&hyperlink.inlines, f),
            InlineNode::Field(field) => visit_inline_note_refs(&field.inlines, f),
            InlineNode::TextBox(text_box) => {
                for block in &text_box.blocks {
                    visit_block_note_refs(block, f);
                }
            }
            InlineNode::Group(group) => visit_group_note_refs(&group.children, f),
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup) =>
            {
                visit_inline_note_refs(&revision.inlines, f);
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => visit_inline_note_refs(&sdt.inlines, f),
            InlineNode::Run(_)
            | InlineNode::Tab(_)
            | InlineNode::Break(_)
            | InlineNode::Drawing(_)
            | InlineNode::AnchoredDrawing(_)
            | InlineNode::EmbeddedObject(_)
            | InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_)
            | InlineNode::BookmarkStart(_)
            | InlineNode::BookmarkEnd(_)
            | InlineNode::MoveRangeStart(_)
            | InlineNode::MoveRangeEnd(_)
            | InlineNode::Math(_)
            | InlineNode::Symbol(_)
            | InlineNode::HorizontalRule(_)
            | InlineNode::NoBreakHyphen(_)
            | InlineNode::SoftHyphen(_)
            | InlineNode::PositionalTab(_)
            | InlineNode::NoteNumberMark(_) => {}
        }
    }
}

fn visit_group_note_refs(children: &[GroupChild], f: &mut impl FnMut(NoteKind, NoteId)) {
    for child in children {
        match child {
            GroupChild::TextBox(text_box) => {
                for block in &text_box.blocks {
                    visit_block_note_refs(block, f);
                }
            }
            GroupChild::Group(group) => visit_group_note_refs(&group.children, f),
            GroupChild::Picture(_) | GroupChild::Shape(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_props_win_field_by_field_over_the_document_default() {
        let defaults = NoteProperties {
            number_format: Some(NumberFormat::LowerRoman),
            number_start: Some(3),
            ..NoteProperties::default()
        };
        let section = NoteProperties {
            number_restart: Some(NoteNumberRestart::EachPage),
            ..NoteProperties::default()
        };
        let resolved = resolve_props(NoteKind::Footnote, Some(&section), &defaults);
        assert_eq!(resolved.restart, NoteNumberRestart::EachPage);
        assert_eq!(
            resolved.format,
            NumberFormat::LowerRoman,
            "a section that overrides only the restart policy must not discard the \
             document-default number format"
        );
        assert_eq!(resolved.start, 3);
        assert_eq!(resolved.position, NotePosition::PageBottom);
    }

    #[test]
    fn a_note_position_from_the_other_kinds_vocabulary_is_ignored() {
        let endnote_token = NoteProperties {
            position: Some(NotePosition::SectionEnd),
            ..NoteProperties::default()
        };
        let resolved = resolve_props(
            NoteKind::Footnote,
            Some(&endnote_token),
            &NoteProperties::default(),
        );
        assert_eq!(
            resolved.position,
            NotePosition::PageBottom,
            "`sectEnd` belongs to ST_EdnPos; a footnote container keeps its own default"
        );
        let footnote_token = NoteProperties {
            position: Some(NotePosition::BeneathText),
            ..NoteProperties::default()
        };
        let resolved = resolve_props(
            NoteKind::Endnote,
            Some(&footnote_token),
            &NoteProperties::default(),
        );
        assert_eq!(resolved.position, NotePosition::DocumentEnd);
    }

    #[test]
    fn a_negative_number_start_clamps_to_zero() {
        let props = NoteProperties {
            number_start: Some(-5),
            ..NoteProperties::default()
        };
        assert_eq!(
            resolve_props(NoteKind::Footnote, Some(&props), &NoteProperties::default()).start,
            0
        );
    }
}
