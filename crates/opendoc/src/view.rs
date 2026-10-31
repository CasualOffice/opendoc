// SPDX-License-Identifier: Apache-2.0

//! The read view: an immutable projection of the normalized model.
//!
//! No model struct is exposed (`docs/176` R8, `docs/05`: "internal model and
//! transaction types are not re-exported"). Every type here is the SDK's own,
//! serialises to the same camelCase JSON every binding reads, and names nodes
//! by their stable `NodeId` string, which is what [`crate::Editor`] takes back.
//!
//! **Complexity.** Each accessor is one walk of what it projects: `body()` is
//! O(body), `comments()` O(comments + body) for the anchors, and so on. A host
//! that reads repeatedly should keep the result rather than re-project.

use std::collections::BTreeMap;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, BreakKind, Color, Definitions, Document as Model, FontRef, HeaderFooterKind,
    InlineNode, Paragraph, RevisionKind, RunProperties, StyleId, StyleKind, Table, VerticalMerge,
    node_plain_text,
};
use serde::Serialize;

/// Twips per point.
const TWIPS_PER_PT: f64 = 20.0;
/// EMU per point.
const EMU_PER_PT: f64 = 12_700.0;

/// The serde name of a unit-like enum value (`"center"`, `"yellow"`).
fn serde_name<T: Serialize>(value: &T) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
}

/// One block of a story (the body, a cell, a header, a note, a comment).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Block {
    /// A paragraph.
    Paragraph(ParagraphView),
    /// A table.
    Table(TableView),
    /// A block content control (`w:sdt`).
    ContentControl(ContentControlView),
    /// A block the view does not project (an `altChunk`). It is preserved and
    /// reported by the import report, never silently dropped.
    Other(OtherView),
}

impl Block {
    /// The block's node id.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Paragraph(p) => &p.id,
            Self::Table(t) => &t.id,
            Self::ContentControl(c) => &c.id,
            Self::Other(o) => &o.id,
        }
    }

    /// The block's plain text: a paragraph's text, a table's cells row by row.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Paragraph(p) => p.text.clone(),
            Self::Table(t) => t.text(),
            Self::ContentControl(c) => blocks_text(&c.blocks),
            Self::Other(_) => String::new(),
        }
    }
}

/// The text of a sequence of blocks, one line per paragraph.
#[must_use]
pub fn blocks_text(blocks: &[Block]) -> String {
    let mut out = Vec::new();
    for block in blocks {
        out.push(block.text());
    }
    out.join("\n")
}

/// A paragraph.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphView {
    /// Stable node id; pass it to [`crate::Editor`].
    pub id: String,
    /// The paragraph style's display name (`"Heading 1"`), when one is applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// The paragraph's text, in the byte space edit positions index.
    pub text: String,
    /// The heading level (1-9) from the style or the outline level; 0 for the
    /// Title style; absent for body text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading_level: Option<u8>,
    /// `"start"`, `"center"`, `"end"` or `"justify"`, when set directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<String>,
    /// The list it belongs to, when numbered or bulleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<ListView>,
    /// Whether a page break precedes it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub page_break_before: bool,
    /// Its inline content, flattened: wrappers (hyperlinks, tracked changes,
    /// fields, inline content controls) annotate the runs inside them.
    pub runs: Vec<RunView>,
}

/// A paragraph's list membership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListView {
    /// The numbering instance id.
    pub num_id: String,
    /// The level, 0-based.
    pub level: u8,
}

/// One inline piece of a paragraph.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    /// Stable node id.
    pub id: String,
    /// `"text"`, `"tab"`, `"break"`, `"pageBreak"`, `"columnBreak"`, `"image"`,
    /// `"noteReference"`, `"commentReference"`, `"symbol"`, `"math"`, `"textBox"`,
    /// `"object"` or `"other"`.
    pub kind: &'static str,
    /// The text it shows.
    pub text: String,
    /// Direct bold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Direct italic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Direct underline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Direct strike-through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    /// Direct font family.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// Direct size in points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_pt: Option<f64>,
    /// Direct colour as `RRGGBB`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Highlight colour name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    /// `"superscript"` or `"subscript"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<String>,
    /// The character style's display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// The hyperlink target (a URL, or `#bookmark`) when inside a link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<String>,
    /// The tracked change it sits in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<RevisionMark>,
    /// The field instruction when it is a field's result (`"PAGE"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// A picture's media part and size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageView>,
    /// The note or comment it references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// The tracked change a run sits in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionMark {
    /// `"insertion"`, `"deletion"`, `"moveFrom"` or `"moveTo"`.
    pub kind: &'static str,
    /// Its author.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Its date (ISO 8601 as written).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// A picture.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageView {
    /// The media part (`word/media/image1.png`).
    pub part: String,
    /// Width in points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_pt: Option<f64>,
    /// Height in points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height_pt: Option<f64>,
    /// Alternative text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether it floats (anchored) rather than sitting in the line.
    pub floating: bool,
}

/// A table.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableView {
    /// Stable node id.
    pub id: String,
    /// The table style's display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Grid column widths in points.
    pub column_widths_pt: Vec<f64>,
    /// The rows.
    pub rows: Vec<RowView>,
}

impl TableView {
    /// The cell at `(row, column)` by cell index within the row.
    #[must_use]
    pub fn cell(&self, row: usize, column: usize) -> Option<&CellView> {
        self.rows.get(row)?.cells.get(column)
    }

    /// The cells' text, tab-separated, one line per row.
    #[must_use]
    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(CellView::text)
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A table row.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowView {
    /// Stable node id.
    pub id: String,
    /// Whether it repeats as a header row on each page.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub header: bool,
    /// The cells.
    pub cells: Vec<CellView>,
}

/// A table cell.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellView {
    /// Stable node id.
    pub id: String,
    /// Grid columns it spans.
    pub column_span: u32,
    /// `"restart"` or `"continue"` when vertically merged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_merge: Option<&'static str>,
    /// Its blocks.
    pub blocks: Vec<Block>,
}

impl CellView {
    /// The cell's text.
    #[must_use]
    pub fn text(&self) -> String {
        blocks_text(&self.blocks)
    }
}

/// A block content control.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentControlView {
    /// Stable node id.
    pub id: String,
    /// `w:tag`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// `w:alias` (the title).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Its blocks.
    pub blocks: Vec<Block>,
}

/// A block the view does not project.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherView {
    /// Stable node id.
    pub id: String,
    /// What it is (`"altChunk"`).
    pub kind: &'static str,
}

/// A section's page setup.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionView {
    /// Stable id.
    pub id: String,
    /// Page width in points.
    pub page_width_pt: f64,
    /// Page height in points.
    pub page_height_pt: f64,
    /// `"portrait"` or `"landscape"`.
    pub orientation: &'static str,
    /// Margins in points.
    pub margins_pt: MarginsView,
    /// Text columns.
    pub columns: u16,
    /// Whether the first page has its own header and footer.
    pub title_page: bool,
    /// How the section starts: `"nextPage"`, `"continuous"`, `"evenPage"`,
    /// `"oddPage"` or `"nextColumn"` (`nextPage` when the file says nothing).
    pub start: &'static str,
    /// Whether even pages use their own header and footer variant. One
    /// document-wide setting in OOXML (`w:evenAndOddHeaders`), repeated on
    /// every section so a reader of one section sees it.
    pub even_and_odd_headers: bool,
    /// The headers it references.
    pub headers: Vec<StoryRef>,
    /// The footers it references.
    pub footers: Vec<StoryRef>,
}

/// Page margins in points.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginsView {
    /// Top.
    pub top: f64,
    /// Bottom.
    pub bottom: f64,
    /// Left (start).
    pub left: f64,
    /// Right (end).
    pub right: f64,
    /// Header distance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<f64>,
    /// Footer distance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<f64>,
}

/// A section's reference to a header or footer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryRef {
    /// `"default"`, `"first"` or `"even"`.
    pub kind: &'static str,
    /// The header or footer id.
    pub id: String,
}

/// A header or a footer.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderFooterView {
    /// Stable id.
    pub id: String,
    /// `"header"` or `"footer"`.
    pub part: &'static str,
    /// Its blocks.
    pub blocks: Vec<Block>,
    /// Its text.
    pub text: String,
}

/// A footnote or an endnote.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteView {
    /// Stable id.
    pub id: String,
    /// `"footnote"` or `"endnote"`.
    pub kind: &'static str,
    /// Its blocks.
    pub blocks: Vec<Block>,
    /// Its text.
    pub text: String,
}

/// A style definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleView {
    /// Stable id.
    pub id: String,
    /// Display name (`"Heading 1"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `"paragraph"`, `"character"`, `"table"` or `"numbering"`.
    pub kind: &'static str,
    /// The style it inherits from, by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    /// Whether it is the default style of its kind.
    pub is_default: bool,
    /// Whether the document defines it (not a built-in).
    pub custom: bool,
}

/// A comment, with its anchor.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentView {
    /// Stable id.
    pub id: String,
    /// Author.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Initials.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    /// Date as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Whether it is resolved.
    pub resolved: bool,
    /// Its text.
    pub text: String,
    /// The paragraph and byte range it comments on, when resolvable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<AnchorView>,
    /// The thread root's `paraId`, on a reply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

/// A range in one paragraph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorView {
    /// The paragraph id.
    pub paragraph: String,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset.
    pub end: u32,
}

/// One tracked change.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionView {
    /// Stable id.
    pub id: String,
    /// `"insertion"`, `"deletion"`, `"move_from"`, `"move_to"` or `"formatting"`.
    pub kind: String,
    /// Author.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Date as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The text it inserts or deletes.
    pub text: String,
    /// Where it is.
    pub anchor: AnchorView,
}

/// A media part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaView {
    /// Stable id.
    pub id: String,
    /// Package part name.
    pub part: String,
    /// Content type.
    pub content_type: String,
    /// Size in bytes, when the bytes are held.
    pub bytes: usize,
}

/// Document properties (core, plus custom).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertiesView {
    /// `dc:title`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `dc:subject`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// `dc:creator`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    /// `cp:keywords`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    /// `dc:description`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `cp:lastModifiedBy`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
    /// `cp:revision`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// `dcterms:created`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// `dcterms:modified`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// `cp:category`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Custom properties, as text.
    pub custom: BTreeMap<String, String>,
}

// --- projection -----------------------------------------------------------

/// Projects the model. One per read call; holds only borrowed lookups.
pub(crate) struct Projector<'a> {
    defs: &'a Definitions,
}

impl<'a> Projector<'a> {
    pub(crate) fn new(model: &'a Model) -> Self {
        Self {
            defs: model.definitions(),
        }
    }

    fn style_name(&self, id: Option<StyleId>) -> Option<String> {
        let id = id?;
        self.defs
            .styles
            .get(&id)
            .and_then(|style| style.name.clone())
            .or_else(|| Some(id.node_id().to_string()))
    }

    pub(crate) fn blocks(&self, blocks: &[BlockNode]) -> Vec<Block> {
        blocks.iter().map(|block| self.block(block)).collect()
    }

    fn block(&self, block: &BlockNode) -> Block {
        match block {
            BlockNode::Paragraph(p) => Block::Paragraph(self.paragraph(p)),
            BlockNode::Table(t) => Block::Table(self.table(t)),
            BlockNode::Sdt(sdt) => Block::ContentControl(ContentControlView {
                id: sdt.id.to_string(),
                tag: sdt.properties.tag.clone(),
                alias: sdt.properties.alias.clone(),
                blocks: self.blocks(&sdt.blocks),
            }),
            BlockNode::AltChunk(chunk) => Block::Other(OtherView {
                id: chunk.id.to_string(),
                kind: "altChunk",
            }),
        }
    }

    pub(crate) fn paragraph(&self, p: &Paragraph) -> ParagraphView {
        let props = &*p.properties;
        let style = self.style_name(props.style_ref);
        let heading_level = style.as_deref().and_then(heading_level_of).or(props
            .outline_level
            .filter(|level| *level < 9)
            .map(|l| l + 1));
        let mut runs = Vec::new();
        self.inlines(&p.inlines, &Context::default(), &mut runs);
        ParagraphView {
            id: p.id.to_string(),
            style,
            text: node_plain_text(&p.inlines),
            heading_level,
            alignment: props.alignment.as_ref().and_then(serde_name),
            list: props.numbering.as_ref().map(|n| ListView {
                num_id: n.instance.node_id().to_string(),
                level: n.level,
            }),
            page_break_before: props.page_break_before.unwrap_or(false),
            runs,
        }
    }

    fn table(&self, t: &Table) -> TableView {
        TableView {
            id: t.id.to_string(),
            style: self.style_name(t.properties.style_ref),
            column_widths_pt: t
                .grid
                .iter()
                .map(|column| f64::from(column.width_twips.unwrap_or(0)) / TWIPS_PER_PT)
                .collect(),
            rows: t
                .rows
                .iter()
                .map(|row| RowView {
                    id: row.id.to_string(),
                    header: row.properties.header,
                    cells: row
                        .cells
                        .iter()
                        .map(|cell| CellView {
                            id: cell.id.to_string(),
                            column_span: cell.properties.grid_span.unwrap_or(1).max(1),
                            vertical_merge: cell.properties.vertical_merge.map(|m| match m {
                                VerticalMerge::Restart => "restart",
                                VerticalMerge::Continue => "continue",
                            }),
                            blocks: self.blocks(&cell.blocks),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn run_format(&self, props: &RunProperties, view: &mut RunView) {
        view.bold = props.bold;
        view.italic = props.italic;
        view.underline = props.underline;
        view.strike = props.strike;
        view.font = props.font_ref.as_ref().and_then(|font| match font {
            FontRef::Named(name) => Some(name.name.clone()),
            FontRef::Theme(_) => None,
        });
        view.size_pt = props.size_half_points.map(|half| f64::from(half) / 2.0);
        view.color = props.color.as_ref().and_then(|color| match color {
            Color::Rgb(rgb) => Some(format!("{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b)),
            Color::Auto | Color::Theme(_) => None,
        });
        view.highlight = props.highlight.as_ref().and_then(serde_name);
        view.vertical_align = props
            .vertical_alignment
            .as_ref()
            .and_then(serde_name)
            .filter(|v| v != "baseline");
        view.style = props
            .style_ref
            .and_then(|id| self.style_name(Some(id)))
            .filter(|_| props.style_ref.is_some());
    }

    fn inlines(&self, inlines: &[InlineNode], ctx: &Context, out: &mut Vec<RunView>) {
        for inline in inlines {
            let mut view = RunView {
                id: inline.id().to_string(),
                hyperlink: ctx.hyperlink.clone(),
                revision: ctx.revision.clone(),
                field: ctx.field.clone(),
                ..RunView::default()
            };
            match inline {
                InlineNode::Run(run) => {
                    view.kind = "text";
                    view.text.clone_from(&run.text);
                    self.run_format(&run.properties, &mut view);
                }
                InlineNode::Tab(_) | InlineNode::PositionalTab(_) => {
                    view.kind = "tab";
                    view.text = "\t".to_owned();
                }
                InlineNode::Break(b) => {
                    view.kind = match b.kind {
                        BreakKind::Line => "break",
                        BreakKind::Page => "pageBreak",
                        BreakKind::Column => "columnBreak",
                    };
                    view.text = "\n".to_owned();
                }
                InlineNode::Drawing(drawing) => {
                    view.kind = "image";
                    view.image = Some(ImageView {
                        part: self.media_part(drawing.media),
                        width_pt: drawing.extent.map(|e| e.width_emu as f64 / EMU_PER_PT),
                        height_pt: drawing.extent.map(|e| e.height_emu as f64 / EMU_PER_PT),
                        description: drawing.descr.clone(),
                        floating: false,
                    });
                }
                InlineNode::AnchoredDrawing(drawing) => {
                    view.kind = "image";
                    view.image = Some(ImageView {
                        part: self.media_part(drawing.media),
                        width_pt: Some(drawing.extent.width_emu as f64 / EMU_PER_PT),
                        height_pt: Some(drawing.extent.height_emu as f64 / EMU_PER_PT),
                        description: drawing.descr.clone(),
                        floating: true,
                    });
                }
                InlineNode::Hyperlink(link) => {
                    let target = match &link.target {
                        casual_doc_model::v1::HyperlinkTarget::External(external) => {
                            match &external.anchor {
                                Some(anchor) => format!("{}#{anchor}", external.url),
                                None => external.url.clone(),
                            }
                        }
                        casual_doc_model::v1::HyperlinkTarget::Internal(internal) => {
                            format!("#{}", internal.anchor)
                        }
                    };
                    let inner = Context {
                        hyperlink: Some(target),
                        ..ctx.clone()
                    };
                    self.inlines(&link.inlines, &inner, out);
                    continue;
                }
                InlineNode::Revision(revision) => {
                    let inner = Context {
                        revision: Some(RevisionMark {
                            kind: match revision.kind {
                                RevisionKind::Insertion => "insertion",
                                RevisionKind::Deletion => "deletion",
                                RevisionKind::MoveFrom => "moveFrom",
                                RevisionKind::MoveTo => "moveTo",
                            },
                            author: revision.author.clone(),
                            date: revision.date.clone(),
                        }),
                        ..ctx.clone()
                    };
                    self.inlines(&revision.inlines, &inner, out);
                    continue;
                }
                InlineNode::Field(field) => {
                    let inner = Context {
                        field: Some(field.instruction.trim().to_owned()),
                        ..ctx.clone()
                    };
                    self.inlines(&field.inlines, &inner, out);
                    continue;
                }
                InlineNode::Sdt(sdt) => {
                    self.inlines(&sdt.inlines, ctx, out);
                    continue;
                }
                InlineNode::NoteReference(note) => {
                    view.kind = "noteReference";
                    view.reference = Some(note.note.node_id().to_string());
                }
                InlineNode::CommentReference(comment) => {
                    view.kind = "commentReference";
                    view.reference = Some(comment.comment.node_id().to_string());
                }
                InlineNode::Symbol(symbol) => {
                    view.kind = "symbol";
                    view.text = char::from_u32(symbol.char)
                        .map(String::from)
                        .unwrap_or_default();
                }
                InlineNode::Math(_) => view.kind = "math",
                InlineNode::TextBox(_) | InlineNode::Group(_) => view.kind = "textBox",
                InlineNode::EmbeddedObject(_) => view.kind = "object",
                InlineNode::NoBreakHyphen(_) => {
                    view.kind = "text";
                    view.text = "\u{2011}".to_owned();
                }
                // Zero-width markers: ranges, bookmarks, field ranges, soft
                // hyphens, rules. They carry no text and are not listed.
                _ => continue,
            }
            out.push(view);
        }
    }

    fn media_part(&self, media: casual_doc_model::v1::MediaId) -> String {
        self.defs
            .media
            .get(&media)
            .map(|reference| reference.part_name.clone())
            .unwrap_or_default()
    }

    pub(crate) fn sections(&self) -> Vec<SectionView> {
        self.defs
            .sections
            .iter()
            .map(|section| {
                let refs = |list: &[casual_doc_model::v1::HeaderFooterRef]| {
                    list.iter()
                        .map(|r| StoryRef {
                            kind: kind_name(r.kind),
                            id: r.reference.node_id().to_string(),
                        })
                        .collect()
                };
                let m = &section.page_margins;
                let pt = |twips: i32| f64::from(twips) / TWIPS_PER_PT;
                let landscape = matches!(
                    section.orientation,
                    Some(casual_doc_model::v1::PageOrientation::Landscape)
                ) || (section.orientation.is_none()
                    && section.page_size.width_twips > section.page_size.height_twips);
                SectionView {
                    id: section.id.node_id().to_string(),
                    page_width_pt: pt(section.page_size.width_twips),
                    page_height_pt: pt(section.page_size.height_twips),
                    orientation: if landscape { "landscape" } else { "portrait" },
                    margins_pt: MarginsView {
                        top: pt(m.top_twips),
                        bottom: pt(m.bottom_twips),
                        left: pt(m.start_twips),
                        right: pt(m.end_twips),
                        header: m.header_twips.map(pt),
                        footer: m.footer_twips.map(pt),
                    },
                    columns: section.columns.count.max(1),
                    title_page: section.title_page.unwrap_or(false),
                    even_and_odd_headers: self.defs.settings.even_and_odd_headers,
                    start: match section.section_type {
                        Some(casual_doc_model::v1::SectionType::Continuous) => "continuous",
                        Some(casual_doc_model::v1::SectionType::EvenPage) => "evenPage",
                        Some(casual_doc_model::v1::SectionType::OddPage) => "oddPage",
                        Some(casual_doc_model::v1::SectionType::NextColumn) => "nextColumn",
                        Some(casual_doc_model::v1::SectionType::NextPage) | None => "nextPage",
                    },
                    headers: refs(&section.headers),
                    footers: refs(&section.footers),
                }
            })
            .collect()
    }

    pub(crate) fn styles(&self) -> Vec<StyleView> {
        self.defs
            .styles
            .iter()
            .map(|(id, style)| StyleView {
                id: id.node_id().to_string(),
                name: style.name.clone(),
                kind: match style.kind {
                    StyleKind::Paragraph => "paragraph",
                    StyleKind::Character => "character",
                    StyleKind::Table => "table",
                    StyleKind::Numbering => "numbering",
                },
                based_on: style.based_on.and_then(|b| self.style_name(Some(b))),
                is_default: style.is_default,
                custom: style.custom_style,
            })
            .collect()
    }

    pub(crate) fn headers_footers(&self) -> Vec<HeaderFooterView> {
        let mut out = Vec::new();
        for (part, map) in [
            ("header", &self.defs.headers),
            ("footer", &self.defs.footers),
        ] {
            for (id, story) in map.iter() {
                let blocks = self.blocks(&story.blocks);
                out.push(HeaderFooterView {
                    id: id.node_id().to_string(),
                    part,
                    text: blocks_text(&blocks),
                    blocks,
                });
            }
        }
        out
    }

    pub(crate) fn notes(&self) -> Vec<NoteView> {
        let mut out = Vec::new();
        for (kind, map) in [
            ("footnote", &self.defs.footnotes),
            ("endnote", &self.defs.endnotes),
        ] {
            for (id, note) in map.iter() {
                let blocks = self.blocks(&note.blocks);
                out.push(NoteView {
                    id: id.node_id().to_string(),
                    kind,
                    text: blocks_text(&blocks),
                    blocks,
                });
            }
        }
        out
    }
}

/// The wrappers a run sits inside.
#[derive(Clone, Debug, Default)]
struct Context {
    hyperlink: Option<String>,
    revision: Option<RevisionMark>,
    field: Option<String>,
}

const fn kind_name(kind: HeaderFooterKind) -> &'static str {
    match kind {
        HeaderFooterKind::Default => "default",
        HeaderFooterKind::First => "first",
        HeaderFooterKind::Even => "even",
    }
}

/// The heading level a style name implies: `Heading 1`..`Heading 9` (any case,
/// with or without the space, which is how Word's built-in names and ids read)
/// and 0 for `Title`.
pub(crate) fn heading_level_of(name: &str) -> Option<u8> {
    let lower = name.trim().to_ascii_lowercase();
    if lower == "title" {
        return Some(0);
    }
    let rest = lower.strip_prefix("heading")?.trim_start();
    let level: u8 = rest.parse().ok()?;
    (1..=9).contains(&level).then_some(level)
}

/// A node id from its string form.
pub(crate) fn parse_id(id: &str) -> crate::Result<NodeId> {
    id.parse()
        .map_err(|_| crate::Error::invalid(format!("\"{id}\" is not a node id")))
}
