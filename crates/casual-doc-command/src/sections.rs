// SPDX-License-Identifier: Apache-2.0

//! Section, page-setup and running-content lowerings the editor and agents
//! share (`docs/181` §4.2, step 6).
//!
//! The editor's dialogs read a section's values, let a person change some, and
//! write the whole record back; an agent names a section by its 0-based
//! ordinal (`get_outline`'s `section`) and the fields it changes. Both arrive
//! here with a resolved section and the complete new value, so the operations
//! are built once.

use casual_doc_edit::CommonField;
use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos, RunningRegion};
use casual_doc_model::NodeId;
use casual_doc_model::v1::PageBorders;
use casual_doc_model::v1::{Alignment, InlineNode};
use casual_doc_model::v1::{
    BlockNode, Document, FontName, HeaderFooterId, HeaderFooterKind, HeaderFooterRef,
    LineNumbering, NumberFormat, PageMargins, PageNumbering, PageOrientation, PageSize,
    PageVerticalAlignment, Paragraph, ParagraphProperties, Rgba, SectionBoundary, SectionColumns,
    SectionId, Watermark, WatermarkContent, WatermarkLayout, WatermarkText,
};

fn next(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

/// One column, no spacing or separator: a section's layout when it names none.
#[must_use]
pub fn single_column() -> SectionColumns {
    SectionColumns {
        count: 1,
        space_twips: None,
        separator: None,
        equal_width: None,
        columns: Vec::new(),
    }
}

/// The section boundary `ordinal` (0-based, body order), if the document has
/// that many. O(1).
#[must_use]
pub fn section_at(document: &Document, ordinal: usize) -> Option<&SectionBoundary> {
    document.definitions().sections.get(ordinal)
}

/// The boundary `section` names, if it exists. O(sections).
#[must_use]
pub fn boundary(document: &Document, section: SectionId) -> Option<&SectionBoundary> {
    document
        .definitions()
        .sections
        .iter()
        .find(|candidate| candidate.id == section)
}

/// Installs a section's page size, margins, orientation and columns. `columns`
/// `None` keeps the section's own (one column when the section is unknown, and
/// the engine then refuses the section). One `SetSectionGeometry`.
/// O(sections).
#[must_use]
pub fn set_geometry(
    document: &Document,
    section: SectionId,
    page_size: PageSize,
    page_margins: PageMargins,
    orientation: Option<PageOrientation>,
    columns: Option<SectionColumns>,
) -> Vec<Operation> {
    let columns = columns.unwrap_or_else(|| {
        boundary(document, section).map_or_else(single_column, |found| found.columns.clone())
    });
    vec![Operation::SetSectionGeometry {
        section,
        page_size,
        page_margins,
        orientation,
        columns,
    }]
}

/// Installs a section's line numbering; the empty rule turns it off. O(1).
#[must_use]
pub fn set_line_numbering(section: SectionId, line_numbering: LineNumbering) -> Vec<Operation> {
    vec![Operation::SetSectionLineNumbering {
        section,
        line_numbering,
    }]
}

/// Installs page borders on each of `sections` in ONE action, as the editor's
/// Page Borders dialog applies them to "This section" or "Whole document" — one
/// operation per section, one undo step. The empty value removes them. O(sections).
#[must_use]
pub fn set_page_borders(sections: &[SectionId], page_borders: &PageBorders) -> Vec<Operation> {
    sections
        .iter()
        .map(|&section| Operation::SetSectionPageBorders {
            section,
            page_borders: Box::new(page_borders.clone()),
        })
        .collect()
}

/// Installs a section's page numbering and vertical alignment together, as the
/// editor's Section Layout dialog applies them (one undo step). Each `None`
/// clears its property. O(1).
#[must_use]
pub fn set_section_layout(
    section: SectionId,
    page_number_format: Option<NumberFormat>,
    page_number_start: Option<i32>,
    vertical_alignment: Option<PageVerticalAlignment>,
) -> Vec<Operation> {
    vec![
        Operation::SetSectionPageNumbering {
            section,
            page_numbering: PageNumbering {
                format: page_number_format,
                start: page_number_start,
            },
        },
        Operation::SetSectionVerticalAlignment {
            section,
            vertical_alignment,
        },
    ]
}

/// A text watermark as Word's dialog builds one. O(text).
#[must_use]
#[allow(clippy::too_many_arguments)] // the dialog's fields, one each
pub fn text_watermark(
    text: String,
    font: Option<&str>,
    size_half_points: Option<u32>,
    color: Rgba,
    bold: bool,
    italic: bool,
    layout: WatermarkLayout,
    semi_transparent: bool,
) -> Watermark {
    Watermark {
        content: WatermarkContent::Text(WatermarkText {
            text,
            font: font.filter(|name| !name.is_empty()).map(|name| FontName {
                name: name.to_owned(),
            }),
            size_half_points,
            color,
            bold,
            italic,
        }),
        layout,
        semi_transparent,
    }
}

/// Installs `watermark` (or removes it, `None`) on **every** section: Word's
/// Design ▸ Watermark writes the same stamp into every section, so a section
/// break never stops it. One operation per section, one undo step.
/// O(sections).
#[must_use]
pub fn set_watermark(document: &Document, watermark: Option<Box<Watermark>>) -> Vec<Operation> {
    document
        .definitions()
        .sections
        .iter()
        .map(|candidate| Operation::SetSectionWatermark {
            section: candidate.id,
            watermark: watermark.clone(),
        })
        .collect()
}

/// Turns a section's distinct first-page header and footer on or off. O(1).
#[must_use]
pub fn set_title_page(section: SectionId, enabled: bool) -> Vec<Operation> {
    vec![Operation::SetSectionTitlePage {
        section,
        title_page: Some(enabled),
    }]
}

/// Turns distinct even and odd page headers and footers on or off
/// (document-wide, as OOXML carries it). O(1).
#[must_use]
pub fn set_even_and_odd(enabled: bool) -> Vec<Operation> {
    vec![Operation::SetEvenAndOddHeaders { enabled }]
}

/// The references of `region` a section carries.
fn references(boundary: &SectionBoundary, region: RunningRegion) -> &[HeaderFooterRef] {
    match region {
        RunningRegion::Header => &boundary.headers,
        RunningRegion::Footer => &boundary.footers,
    }
}

/// Whether section `ordinal` shows a `kind` header or footer already, its own
/// or one it inherits from an earlier section ("Link to Previous"), so that
/// creating one would break the chain or duplicate it. O(sections).
#[must_use]
pub fn has_running_content(
    document: &Document,
    ordinal: usize,
    region: RunningRegion,
    kind: HeaderFooterKind,
) -> bool {
    document
        .definitions()
        .sections
        .iter()
        .take(ordinal.saturating_add(1))
        .any(|boundary| {
            references(boundary, region)
                .iter()
                .any(|reference| reference.kind == kind)
        })
}

/// A lowered header or footer creation.
#[derive(Clone, Debug)]
pub struct RunningEdit {
    /// The operations, one step.
    pub operations: Vec<Operation>,
    /// The new body's paragraph, where the caret goes.
    pub caret: Pos,
}

/// Creates an empty header or footer body (one empty paragraph, so a caret has
/// somewhere to be) and points `section`'s `kind` reference at it. The body id
/// and the paragraph id are minted in that order. O(1).
///
/// # Errors
/// When ids run out.
pub fn create_running_content(
    section: SectionId,
    region: RunningRegion,
    kind: HeaderFooterKind,
    ids: MintId<'_>,
) -> Result<RunningEdit, String> {
    let body = HeaderFooterId::new(next(ids)?);
    let paragraph = next(ids)?;
    Ok(RunningEdit {
        operations: vec![
            Operation::CreateHeaderFooterBody {
                region,
                id: body,
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: paragraph,
                    properties: ParagraphProperties::default().into(),
                    inlines: Vec::new(),
                })],
            },
            Operation::SetSectionRunningRef {
                section,
                region,
                kind,
                reference: Some(body),
            },
        ],
        caret: Pos::new(paragraph, 0),
    })
}

// ---- Link to Previous, Remove, and page-number positions (RM-23, HF-192) ----
//
// OOXML expresses "Link to Previous" as ABSENCE: a section that omits a
// `w:headerReference` of some type shows the nearest earlier section's
// (`docs/85` §8.4). So re-linking is one `SetSectionRunningRef { None }`, and
// unlinking has to give the section a body of its own — a faithful deep copy of
// the one it was showing, with fresh ids, which is what `docs/129` §2 found
// missing and #645 (HF-191) built as `casual_doc_edit::clone`. Every lowering
// here names a section by ordinal and a variant by kind; the editor resolves both
// from the page in view, an agent from `get_outline`.

/// The body a section's `kind` variant shows after inheritance: the nearest
/// declaration at or before `ordinal` (`docs/85` §8.4). The same rule the layout
/// (`merge_running_refs`) and the editor's `effective_running_ref` apply.
/// O(sections).
#[must_use]
pub fn effective_reference(
    document: &Document,
    ordinal: usize,
    region: RunningRegion,
    kind: HeaderFooterKind,
) -> Option<HeaderFooterId> {
    document
        .definitions()
        .sections
        .iter()
        .take(ordinal.saturating_add(1))
        .filter_map(|boundary| {
            references(boundary, region)
                .iter()
                .find(|reference| reference.kind == kind)
                .map(|reference| reference.reference)
        })
        .next_back()
}

/// Whether section `ordinal`'s `kind` variant of `region` is linked to the
/// previous section: it declares no reference of its own. The first section has
/// no previous section, so it is never linked, and `None` says so — the state in
/// which ONLYOFFICE disables the control (`controller/HeaderFooterTab.js:247`,
/// keyed on its core returning `null` rather than `false`). O(sections).
#[must_use]
pub fn is_linked_to_previous(
    document: &Document,
    ordinal: usize,
    region: RunningRegion,
    kind: HeaderFooterKind,
) -> Option<bool> {
    if ordinal == 0 {
        return None;
    }
    let boundary = section_at(document, ordinal)?;
    Some(
        !references(boundary, region)
            .iter()
            .any(|reference| reference.kind == kind),
    )
}

/// The blocks of a header or footer body, if it exists. O(log bodies).
fn body_blocks(
    document: &Document,
    region: RunningRegion,
    id: HeaderFooterId,
) -> Option<&[BlockNode]> {
    let definitions = document.definitions();
    match region {
        RunningRegion::Header => definitions.headers.get(&id),
        RunningRegion::Footer => definitions.footers.get(&id),
    }
    .map(|body| body.blocks.as_slice())
}

/// The blocks section `ordinal`'s `kind` variant shows, own or inherited.
/// O(sections).
#[must_use]
pub fn shown_blocks(
    document: &Document,
    ordinal: usize,
    region: RunningRegion,
    kind: HeaderFooterKind,
) -> Option<&[BlockNode]> {
    body_blocks(
        document,
        region,
        effective_reference(document, ordinal, region, kind)?,
    )
}

/// One empty paragraph: the least a body can hold and still take a caret.
fn empty_paragraph(id: NodeId) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id,
        properties: ParagraphProperties::default().into(),
        inlines: Vec::new(),
    })
}

/// Adapts a [`MintId`] closure to the `RunIds` the deep copy takes, so the copy
/// mints from the same id space as every other lowering.
struct Minter<'a, 'b>(&'a mut (dyn FnMut() -> Option<NodeId> + 'b));

impl casual_doc_edit::RunIds for Minter<'_, '_> {
    fn next(&mut self) -> Option<NodeId> {
        (self.0)()
    }
}

/// Every paragraph id in `blocks`, in document order, descending into tables and
/// block content controls. The deep copy is structural, so the n-th paragraph of
/// a copy corresponds to the n-th of its source — which is how a caret that was
/// in the inherited body follows the user into the section's own copy.
/// O(size of blocks).
#[must_use]
pub fn paragraph_order(blocks: &[BlockNode]) -> Vec<NodeId> {
    fn walk(blocks: &[BlockNode], out: &mut Vec<NodeId>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => out.push(paragraph.id),
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => walk(&sdt.blocks, out),
                // An imported alternate-format chunk is opaque: no paragraph to
                // land a caret on.
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(blocks, &mut out);
    out
}

/// A lowered Link to Previous change.
#[derive(Clone, Debug, Default)]
pub struct RunningLink {
    /// The operations, one step. Empty when the variant is already in the
    /// requested state, which the caller reports as unchanged rather than as an
    /// empty edit.
    pub operations: Vec<Operation>,
    /// What the copy could not carry (bookmarks, comment anchors, note
    /// references and field-range markers cannot be duplicated in one document;
    /// `casual_doc_edit::clone`). Empty for a faithful copy and for a re-link.
    pub loss: casual_doc_edit::clone::CloneReport,
    /// `(before, after)` pairs mapping each paragraph of the body the band showed
    /// before the change to the paragraph it shows after, so a caret in the band
    /// can follow the change instead of staying in a body the page no longer
    /// shows.
    pub paragraphs: Vec<(NodeId, NodeId)>,
}

/// Links section `ordinal`'s `kind` variant of `region` to the previous
/// section's, or unlinks it, as one step (Word's and ONLYOFFICE's "Link to
/// previous", per band and per variant — `docs/85` §8d Q7).
///
/// * **Unlink** (`linked: false`): the section gets its own body, a deep copy
///   of the one it was showing with every id re-minted (`docs/129` §4), so the
///   band looks exactly as it did and editing it no longer edits the previous
///   section's. With nothing to inherit the copy is one empty paragraph.
/// * **Re-link** (`linked: true`): the section's own reference is removed and it
///   shows the previous section's again. The body it owned stays in the
///   definitions for undo; the exporter's reachability pass drops it from the
///   package (`docs/129` §3a), so an unlink/re-link cycle never grows the file.
///
/// Where this differs from Word: Word asks "Do you want to delete this header
/// and link to the previous section?" before a re-link. Here the change is one
/// undoable step and the host says so, which is the rule every other
/// destructive-but-undoable command in this editor follows.
///
/// O(sections + size of the copied body).
///
/// # Errors
/// The first section (there is no previous section), an unknown section, or
/// ids running out.
pub fn set_linked_to_previous(
    document: &Document,
    ordinal: usize,
    region: RunningRegion,
    kind: HeaderFooterKind,
    linked: bool,
    ids: MintId<'_>,
) -> Result<RunningLink, String> {
    let boundary =
        section_at(document, ordinal).ok_or_else(|| format!("there is no section {ordinal}"))?;
    let Some(now_linked) = is_linked_to_previous(document, ordinal, region, kind) else {
        return Err("the first section has no previous section to link to".to_owned());
    };
    if now_linked == linked {
        return Ok(RunningLink::default());
    }
    let section = boundary.id;
    let before = shown_blocks(document, ordinal, region, kind).unwrap_or(&[]);
    if linked {
        let after = shown_blocks(document, ordinal - 1, region, kind).unwrap_or(&[]);
        // A re-link replaces one body with an unrelated one, so the first
        // paragraph is the only meaningful landing place for a caret.
        let paragraphs = match paragraph_order(after).first() {
            Some(first) => paragraph_order(before)
                .into_iter()
                .map(|id| (id, *first))
                .collect(),
            None => Vec::new(),
        };
        return Ok(RunningLink {
            operations: vec![Operation::SetSectionRunningRef {
                section,
                region,
                kind,
                reference: None,
            }],
            loss: casual_doc_edit::clone::CloneReport::default(),
            paragraphs,
        });
    }
    let body = HeaderFooterId::new(next(ids)?);
    let mut loss = casual_doc_edit::clone::CloneReport::default();
    let blocks = if before.is_empty() {
        vec![empty_paragraph(next(ids)?)]
    } else {
        casual_doc_edit::clone::clone_blocks_with_fresh_ids(
            before,
            &mut Minter(&mut *ids),
            &mut loss,
        )
        .ok_or_else(|| "id space exhausted".to_owned())?
    };
    let paragraphs = paragraph_order(before)
        .into_iter()
        .zip(paragraph_order(&blocks))
        .collect();
    Ok(RunningLink {
        operations: vec![
            Operation::CreateHeaderFooterBody {
                region,
                id: body,
                blocks,
            },
            Operation::SetSectionRunningRef {
                section,
                region,
                kind,
                reference: Some(body),
            },
        ],
        loss,
        paragraphs,
    })
}

/// Whether a body holds nothing a reader could see: no blocks, or only
/// paragraphs with no inline content. O(top-level blocks).
#[must_use]
pub fn blocks_are_blank(blocks: &[BlockNode]) -> bool {
    blocks.iter().all(|block| match block {
        BlockNode::Paragraph(paragraph) => paragraph.inlines.is_empty(),
        BlockNode::Table(_) | BlockNode::Sdt(_) | BlockNode::AltChunk(_) => false,
    })
}

/// Removes the header or footer section `ordinal` shows for `kind` — Word's
/// Header ▸ Remove Header, ONLYOFFICE's `RemoveHdrFtr`
/// (`sdkjs/word/Editor/Document.js:7352`).
///
/// The band disappears from THIS section's pages and from every later section
/// that links to it, and from no other page:
///
/// * when no earlier section shows a `kind` band, removing the reference is
///   enough — nothing inherits through — so it is one `SetSectionRunningRef
///   { None }` (ONLYOFFICE's first-section case, `SectPr::RemoveHeader`);
/// * otherwise removing the reference would make the section inherit the
///   earlier band, which is not "removed", so the section is pointed at a new
///   EMPTY body instead — the only way OOXML can say "this section has no
///   header" while an earlier one has.
///
/// Where this differs from ONLYOFFICE: for a later section it empties the body
/// in place (`UpdateContentToDefaults`), which also empties every EARLIER
/// section linked to the same body — the `docs/104` T-01 cross-section mutation.
/// Here an inherited band is never written through.
///
/// O(sections).
///
/// # Errors
/// An unknown section, nothing to remove, or ids running out.
pub fn remove_running_content(
    document: &Document,
    ordinal: usize,
    region: RunningRegion,
    kind: HeaderFooterKind,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    let boundary =
        section_at(document, ordinal).ok_or_else(|| format!("there is no section {ordinal}"))?;
    if shown_blocks(document, ordinal, region, kind).is_none_or(blocks_are_blank) {
        return Err(match region {
            RunningRegion::Header => "there is no header to remove here",
            RunningRegion::Footer => "there is no footer to remove here",
        }
        .to_owned());
    }
    let section = boundary.id;
    let earlier_shows = ordinal > 0
        && shown_blocks(document, ordinal - 1, region, kind)
            .is_some_and(|blocks| !blocks_are_blank(blocks));
    if !earlier_shows {
        return Ok(vec![Operation::SetSectionRunningRef {
            section,
            region,
            kind,
            reference: None,
        }]);
    }
    let body = HeaderFooterId::new(next(ids)?);
    let paragraph = next(ids)?;
    Ok(vec![
        Operation::CreateHeaderFooterBody {
            region,
            id: body,
            blocks: vec![empty_paragraph(paragraph)],
        },
        Operation::SetSectionRunningRef {
            section,
            region,
            kind,
            reference: Some(body),
        },
    ])
}

/// Where a page number goes across the band: Word's and ONLYOFFICE's left,
/// centre and right presets. Left and right are the paragraph's START and END,
/// so a right-to-left section mirrors them, as Word does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageNumberAlign {
    /// The start edge.
    Left,
    /// Centred.
    Center,
    /// The end edge.
    Right,
}

impl PageNumberAlign {
    /// `left`, `center` (or `centre`) and `right`. O(1).
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "left" => Some(Self::Left),
            "center" | "centre" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    const fn alignment(self) -> Alignment {
        match self {
            Self::Left => Alignment::Start,
            Self::Center => Alignment::Center,
            Self::Right => Alignment::End,
        }
    }
}

/// Puts a `PAGE` field in section `ordinal`'s `kind` header or footer, aligned
/// left, centre or right, as one step — the six position presets of Word's
/// Page Number gallery and ONLYOFFICE's Insert ▸ Page Number
/// (`Document_AddPageNum`, `sdkjs/word/Editor/Document.js:11440`).
///
/// * No band yet: one is created holding the aligned number, and the section's
///   variant is pointed at it (ONLYOFFICE's `Create_SectionHdrFtr`).
/// * A blank band (the empty paragraph "Edit header" leaves): the number goes in
///   that paragraph.
/// * A band with content: the number gets a paragraph of its own — above the
///   content in a header, below it in a footer — carrying the neighbouring
///   paragraph's style, so the content stays.
///
/// Where this differs from both references, deliberately: Word's building block
/// and ONLYOFFICE's `SetHdrFtrPageNum` (`DocumentContent.js:1961`) CLEAR the
/// band first, keeping only a watermark, so a logo or a title in the header is
/// thrown away by a click that asked for a page number. That is silent data loss
/// (`AGENTS.md`), so the content is kept and the number is added beside it.
///
/// An inherited band is edited where it lives, as typing into it is: the number
/// appears on every section linked to it, which is what "linked" means.
///
/// The caret lands after the new field. O(sections + top-level blocks).
///
/// # Errors
/// An unknown section, a band whose edge block is not a paragraph (a table at
/// the top of a header), or ids running out.
pub fn insert_page_number(
    document: &Document,
    ordinal: usize,
    region: RunningRegion,
    kind: HeaderFooterKind,
    align: PageNumberAlign,
    ids: MintId<'_>,
) -> Result<RunningEdit, String> {
    let boundary =
        section_at(document, ordinal).ok_or_else(|| format!("there is no section {ordinal}"))?;
    let field_id = next(ids)?;
    let result_id = next(ids)?;
    let field = CommonField::Page.build(field_id, result_id);
    let field_len = casual_doc_edit::field_text_len(&field);
    let Some(blocks) = shown_blocks(document, ordinal, region, kind) else {
        let body = HeaderFooterId::new(next(ids)?);
        let paragraph = next(ids)?;
        let properties = ParagraphProperties {
            alignment: Some(align.alignment()),
            ..ParagraphProperties::default()
        };
        return Ok(RunningEdit {
            operations: vec![
                Operation::CreateHeaderFooterBody {
                    region,
                    id: body,
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: paragraph,
                        properties: properties.into(),
                        inlines: vec![InlineNode::Field(Box::new(field))],
                    })],
                },
                Operation::SetSectionRunningRef {
                    section: boundary.id,
                    region,
                    kind,
                    reference: Some(body),
                },
            ],
            caret: Pos::new(paragraph, field_len),
        });
    };
    let edge = match region {
        RunningRegion::Header => blocks.first(),
        RunningRegion::Footer => blocks.last(),
    };
    let Some(BlockNode::Paragraph(edge)) = edge else {
        return Err(match region {
            RunningRegion::Header => {
                "the header starts with a table; put the caret where the number goes and insert it there"
            }
            RunningRegion::Footer => {
                "the footer ends with a table; put the caret where the number goes and insert it there"
            }
        }
        .to_owned());
    };
    let mut properties = (*edge.properties).clone();
    properties.alignment = Some(align.alignment());
    let mut operations = Vec::with_capacity(3);
    // An empty edge paragraph takes the number itself: a blank band, or the empty
    // line a person left at the bottom of a footer.
    let target = if edge.inlines.is_empty() {
        edge.id
    } else {
        let new_id = next(ids)?;
        match region {
            // Splitting at the start leaves the ORIGINAL paragraph empty and on
            // top, with the content moved into `new_id` below it.
            RunningRegion::Header => {
                operations.push(Operation::SplitParagraph {
                    at: Pos::new(edge.id, 0),
                    new_id,
                    properties: None,
                });
                edge.id
            }
            // Splitting at the end leaves the content where it is and opens an
            // empty `new_id` below it.
            RunningRegion::Footer => {
                operations.push(Operation::SplitParagraph {
                    at: Pos::new(edge.id, casual_doc_edit::inlines_text_len(&edge.inlines)),
                    new_id,
                    properties: None,
                });
                new_id
            }
        }
    };
    operations.push(Operation::SetParagraphProperties {
        node: target,
        properties: Box::new(properties),
    });
    operations.push(Operation::InsertField {
        at: Pos::new(target, 0),
        field: Box::new(field),
    });
    Ok(RunningEdit {
        operations,
        caret: Pos::new(target, field_len),
    })
}
