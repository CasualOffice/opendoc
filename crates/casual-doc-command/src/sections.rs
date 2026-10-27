// SPDX-License-Identifier: Apache-2.0

//! Section, page-setup and running-content lowerings the editor and agents
//! share (`docs/181` §4.2, step 6).
//!
//! The editor's dialogs read a section's values, let a person change some, and
//! write the whole record back; an agent names a section by its 0-based
//! ordinal (`get_outline`'s `section`) and the fields it changes. Both arrive
//! here with a resolved section and the complete new value, so the operations
//! are built once.

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos, RunningRegion};
use casual_doc_model::NodeId;
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
