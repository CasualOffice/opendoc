//! The closed editing op set on `v1::Document` (doc 59).
//!
//! Editing mutates the **same model that is rendered** (`v1::Document`), not the
//! minimal v0 model the Phase-0 transaction layer edits. This crate is the choke
//! point (doc 45 I1): [`apply`] takes an [`Operation`] (the closed set, I2),
//! mutates the document in place, and returns the **inverse** operation so undo/
//! redo are just re-applying inverses.
//!
//! Positions are the **layout anchor space** shared with hit-testing (doc 58 §3):
//! `(NodeId paragraph, u32 byte_offset)`, a node-relative UTF-8 byte offset into
//! the paragraph's shaped plain text (`node_plain_text`). No grapheme/affinity
//! model, no byte↔grapheme bridge — hit-testing, selection, and editing all speak
//! byte offsets.
//!
//! The operation set is intentionally additive and bounded: text/paragraph/run,
//! table structure/properties, exact-range hyperlink edits, and bookmark
//! create/rename/delete are supported. Partial edits inside nested wrappers and
//! broader object editing remain explicit follow-ups (doc 59 staging).

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Color, Comment, CommentId, CoreProperties, DefinitionMap, Document, DrawingAnchor,
    Extent, FontName, FontRef, GridColumn, HighlightColor, Hyperlink, HyperlinkTarget, InlineNode,
    PageMargins, PageOrientation, PageSize, Paragraph, ParagraphProperties, ReviewProjection,
    RgbColor, Run, RunProperties, SectionColumns, SectionId, Style, StyleId, Table, TableCell,
    TableCellProperties, TableProperties, TableRow, VerticalAlignment,
};
// A separate `use` line for the field-editing types (doc 59 InsertField slice).
use casual_doc_model::v1::GroupChild;
use casual_doc_model::v1::{Bookmark, BookmarkEnd, BookmarkId, BookmarkStart};
use casual_doc_model::v1::{CropRect, MAX_DESCR_BYTES};
use casual_doc_model::v1::{Field, FieldKind};
use casual_doc_model::v1::{HeaderFooter, HeaderFooterId, HeaderFooterKind, HeaderFooterRef};
use casual_doc_model::v1::{Note, NoteId, NoteKind, NoteReference};

/// A run-property change to apply over a range: each `Some(_)` field sets that
/// property, `None` leaves it untouched. Character formatting (`w:b`/`w:i`/`w:u`/
/// `w:strike`/`w:color`/`w:highlight`/`w:sz`/`w:vertAlign`/`w:rFonts`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FormatDelta {
    /// Set bold on/off.
    pub bold: Option<bool>,
    /// Set italic on/off.
    pub italic: Option<bool>,
    /// Set underline on/off.
    pub underline: Option<bool>,
    /// Set strike-through on/off.
    pub strike: Option<bool>,
    /// Set the text color (`w:color`) to an explicit RGB.
    pub color: Option<RgbColor>,
    /// Set the highlight (`w:highlight`) to a named color.
    pub highlight: Option<HighlightColor>,
    /// Set the font size in half-points (`w:sz`).
    pub size_half_points: Option<u32>,
    /// Set the baseline alignment (`w:vertAlign`): super/sub/baseline.
    pub vertical_alignment: Option<VerticalAlignment>,
    /// Set the font family (`w:rFonts`, ascii + hAnsi slots).
    pub font: Option<String>,
}

impl FormatDelta {
    /// Applies this delta onto `props`: each `Some(_)` field sets the matching run
    /// property, `None` leaves it untouched — the same mapping `FormatText` applies,
    /// exposed so a freshly built run (e.g. an external structured paste) can carry
    /// clipboard formatting without duplicating the property mapping.
    pub fn apply_to(&self, props: &mut RunProperties) {
        if let Some(b) = self.bold {
            props.bold = Some(b);
        }
        if let Some(i) = self.italic {
            props.italic = Some(i);
        }
        if let Some(u) = self.underline {
            props.underline = Some(u);
        }
        if let Some(s) = self.strike {
            props.strike = Some(s);
        }
        if let Some(c) = self.color {
            props.color = Some(Color::Rgb(c));
        }
        if let Some(h) = self.highlight {
            props.highlight = Some(h);
        }
        if let Some(sz) = self.size_half_points {
            props.size_half_points = Some(sz);
        }
        if let Some(v) = self.vertical_alignment {
            props.vertical_alignment = Some(v);
        }
        if let Some(family) = &self.font {
            let font = FontRef::Named(FontName {
                name: family.clone(),
            });
            props.font_ref = Some(font.clone());
            props.font_ref_h_ansi = Some(font);
        }
    }
}

/// A caret position: a paragraph node and a node-relative UTF-8 byte offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pos {
    /// The paragraph node.
    pub node: NodeId,
    /// UTF-8 byte offset into the paragraph's shaped plain text.
    pub offset: u32,
}

impl Pos {
    /// A position at `offset` within `node`.
    #[must_use]
    pub const fn new(node: NodeId, offset: u32) -> Self {
        Self { node, offset }
    }
}

/// A half-open range `[start, end)` within one paragraph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    /// Inclusive start.
    pub start: Pos,
    /// Exclusive end.
    pub end: Pos,
}

/// One paragraph-local replacement carried by an atomic review command.
///
/// Review authoring/decisions can rewrite wrapper and marker structure while
/// leaving every other paragraph untouched. Carrying complete inlines for only
/// the affected paragraph gives Undo an exact, bounded inverse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewParagraphState {
    /// Paragraph whose inline tree is replaced.
    pub node: NodeId,
    /// Complete replacement inline tree.
    pub inlines: Vec<InlineNode>,
}

/// A common Word field a caller can insert without a field evaluator. This crate
/// has no wall-clock or filesystem access, so a clock/context-based kind carries
/// its already-formatted display text as `result`; `Page`/`NumPages` recompute at
/// pagination and seed only a `"1"` placeholder.
///
/// [`CommonField::build`] turns a value of this enum into the [`Field`] node an
/// [`Operation::InsertField`] inserts — it constructs the field instruction
/// string, the [`FieldKind`] projection, and the cached-result leaf run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommonField {
    /// `PAGE` — the current page number (recomputed at pagination).
    Page,
    /// `NUMPAGES` — the total page count (recomputed at pagination).
    NumPages,
    /// `DATE` — the current date. `result` is the caller-formatted display value.
    Date {
        /// The `\@` picture switch to embed in the instruction, if any.
        format: Option<String>,
        /// The already-formatted date to cache as the display value.
        result: String,
    },
    /// `TIME` — the current time. `result` is the caller-formatted display value.
    Time {
        /// The `\@` picture switch to embed in the instruction, if any.
        format: Option<String>,
        /// The already-formatted time to cache as the display value.
        result: String,
    },
    /// `FILENAME` — the document file name. `result` is the caller-supplied value.
    FileName {
        /// The file name to cache as the display value.
        result: String,
    },
    /// `AUTHOR` — the document author. `result` is the caller-supplied value.
    Author {
        /// The author to cache as the display value.
        result: String,
    },
}

impl CommonField {
    /// The field instruction string (`w:instrText`) for this kind, e.g.
    /// `PAGE \* MERGEFORMAT` or `DATE \@ "M/d/yyyy"`.
    #[must_use]
    pub fn instruction(&self) -> String {
        // A keyword-with-switch pattern shared by the picture-carrying kinds.
        let with_picture = |keyword: &str, format: &Option<String>| match format {
            Some(picture) => format!("{keyword} \\@ \"{picture}\""),
            None => keyword.to_owned(),
        };
        match self {
            CommonField::Page => "PAGE \\* MERGEFORMAT".to_owned(),
            CommonField::NumPages => "NUMPAGES \\* MERGEFORMAT".to_owned(),
            CommonField::Date { format, .. } => with_picture("DATE", format),
            CommonField::Time { format, .. } => with_picture("TIME", format),
            CommonField::FileName { .. } => "FILENAME \\* MERGEFORMAT".to_owned(),
            CommonField::Author { .. } => "AUTHOR \\* MERGEFORMAT".to_owned(),
        }
    }

    /// The cached display text to seed the field's leaf run with. `Page`/
    /// `NumPages` seed a `"1"` placeholder the pagination field pass overwrites;
    /// the other kinds seed the caller-supplied `result`.
    fn result_text(&self) -> &str {
        match self {
            CommonField::Page | CommonField::NumPages => "1",
            CommonField::Date { result, .. }
            | CommonField::Time { result, .. }
            | CommonField::FileName { result }
            | CommonField::Author { result } => result,
        }
    }

    /// Builds the [`Field`] node to insert: `id` is the field's identity and
    /// `result_id` the identity of its cached-result run. An empty display value
    /// yields an empty cached result (`result_id` unused); otherwise a single
    /// default-styled leaf run carries the text. The [`FieldKind`] projection is
    /// derived from the built instruction, so it always agrees with it.
    #[must_use]
    pub fn build(&self, id: NodeId, result_id: NodeId) -> Field {
        let instruction = self.instruction();
        let kind = FieldKind::parse(&instruction);
        let text = self.result_text();
        let inlines = if text.is_empty() {
            Vec::new()
        } else {
            vec![InlineNode::Run(Run {
                id: result_id,
                properties: RunProperties::default(),
                text: text.to_owned(),
            })]
        };
        Field {
            id,
            instruction,
            kind,
            inlines,
            form: None,
        }
    }
}

/// The closed editing op set (I2). Slice 1 carries the two text ops; structural
/// and object ops are additive variants (doc 59).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Insert `text` at a caret position.
    InsertText {
        /// Where to insert.
        at: Pos,
        /// The text inserted.
        text: String,
    },
    /// Delete a non-empty range within one paragraph.
    DeleteText {
        /// The range removed.
        range: Range,
    },
    /// Split a paragraph at `at`, moving the trailing content into a new
    /// paragraph with id `new_id` inserted immediately after (Enter).
    SplitParagraph {
        /// The split boundary in the original paragraph.
        at: Pos,
        /// The id of the new trailing paragraph.
        new_id: NodeId,
    },
    /// Join `second` (which must immediately follow `first` in the same
    /// container) into the end of `first`, removing `second` (Backspace at a
    /// paragraph start). `first` keeps its own paragraph properties.
    JoinParagraphs {
        /// The paragraph that receives the content.
        first: NodeId,
        /// The paragraph appended and removed.
        second: NodeId,
    },
    /// Apply a run-property change over a range within one paragraph (bold,
    /// italic, …). Runs straddling the range are split so the change lands
    /// exactly on the selection.
    FormatText {
        /// The range to format (same node for `start`/`end`).
        range: Range,
        /// The property change.
        delta: FormatDelta,
    },
    /// Remove direct character formatting over a range, restoring the effective
    /// document/style defaults. The paragraph style itself is preserved.
    ClearFormatting {
        /// The range to clear (same node for `start`/`end`).
        range: Range,
    },
    /// Creates, updates, or removes a hyperlink over an exact same-paragraph text
    /// range. `Some(target)` creates a wrapper (or updates the wrapper already
    /// occupying exactly `range`); `None` removes that exact wrapper while
    /// preserving its inline children. The inverse restores the paragraph's
    /// original inline tree verbatim.
    SetHyperlink {
        /// The linked text range.
        range: Range,
        /// Fresh identity used when a new hyperlink wrapper is created.
        id: NodeId,
        /// New target, or `None` to remove the exact hyperlink wrapper.
        target: Option<HyperlinkTarget>,
        /// Optional screen tip. Must be non-empty and at most 255 bytes.
        tooltip: Option<String>,
    },
    /// Replace a paragraph's entire inline content. This is the inverse vehicle
    /// for structural edits (formatting run-splits) whose forward effect is not a
    /// simple reverse op — undo restores the paragraph's inlines verbatim.
    SetInlines {
        /// The paragraph whose content is replaced.
        node: NodeId,
        /// The inlines to install.
        inlines: Vec<InlineNode>,
    },
    /// Replace a paragraph's properties (alignment, spacing, indentation,
    /// shading, style, …). Its own inverse (carrying the previous properties), so
    /// undo is exact. Boxed to keep the enum small.
    SetParagraphProperties {
        /// The paragraph whose properties are replaced.
        node: NodeId,
        /// The properties to install.
        properties: Box<ParagraphProperties>,
    },
    /// Insert `row` into table `table` at the 0-based `index` (≤ the current row
    /// count). Inverse: [`Operation::DeleteRow`] of the same position.
    InsertRow {
        /// The table to insert into.
        table: NodeId,
        /// The 0-based row position.
        index: u32,
        /// The row to insert (its ids must be fresh).
        row: Box<TableRow>,
    },
    /// Remove the row at 0-based `index` from table `table`. Refuses to remove the
    /// last row (a table's rows are non-empty). Inverse: [`Operation::InsertRow`]
    /// carrying the removed row, so undo restores it verbatim.
    DeleteRow {
        /// The table to remove from.
        table: NodeId,
        /// The 0-based row position.
        index: u32,
    },
    /// Insert a column into a **regular** table (no `gridSpan`/`vMerge`; grid width
    /// matches every row's cell count) at 0-based grid `index`: a grid column of
    /// `width` and one `cells` entry per row (in row order, fresh ids). Inverse:
    /// [`Operation::DeleteColumn`].
    InsertColumn {
        /// The table to insert into.
        table: NodeId,
        /// The 0-based column position.
        index: u32,
        /// The new grid column's width (twips), if any.
        width: Option<i32>,
        /// One new cell per row, in row order.
        cells: Vec<TableCell>,
    },
    /// Remove the column at 0-based grid `index` from a **regular** table. Refuses a
    /// table's only column. Inverse: [`Operation::InsertColumn`] carrying the removed
    /// grid width + cells, so undo restores the column verbatim.
    DeleteColumn {
        /// The table to remove from.
        table: NodeId,
        /// The 0-based column position.
        index: u32,
    },
    /// Remove the whole `table` from its container (the body, or a cell / content
    /// control it nests in). Refuses to empty a table cell (a cell's blocks are
    /// non-empty). Inverse: [`Operation::InsertTable`] carrying the removed table +
    /// its position, so undo restores it verbatim.
    DeleteTable {
        /// The table to remove.
        table: NodeId,
    },
    /// Insert `table` at 0-based `index` in `container` (`None` = the document body,
    /// `Some(id)` = the cell or content control whose blocks hold it). Inverse:
    /// [`Operation::DeleteTable`].
    InsertTable {
        /// The container: `None` for the body, else the owning cell / SDT node.
        container: Option<NodeId>,
        /// The 0-based block position within the container.
        index: u32,
        /// The table to insert (its ids must be those the inverse recorded).
        table: Box<Table>,
    },
    /// Insert a sequence of `blocks` into `container` (`None` = the document body,
    /// `Some(id)` = the owning cell / SDT node) starting at 0-based `index` (≤ the
    /// current block count). The general block-sequence primitive behind structured
    /// paste — a fragment of copied paragraphs and tables reconstructed with fresh
    /// ids. Inverse: [`Operation::DeleteBlocks`] of the same span.
    InsertBlocks {
        /// The container: `None` for the body, else the owning cell / SDT node.
        container: Option<NodeId>,
        /// The 0-based block position within the container.
        index: u32,
        /// The blocks to insert, in order (their ids must be those the inverse
        /// recorded — the caller assigns fresh ids before inserting).
        blocks: Vec<BlockNode>,
    },
    /// Remove `count` blocks from `container` (`None` = the document body) starting
    /// at 0-based `index`. The inverse vehicle for [`Operation::InsertBlocks`];
    /// undo restores the removed blocks verbatim.
    DeleteBlocks {
        /// The container: `None` for the body, else the owning cell / SDT node.
        container: Option<NodeId>,
        /// The 0-based block position of the first removed block.
        index: u32,
        /// How many consecutive blocks to remove.
        count: u32,
    },
    /// Resize an inline drawing or text box: replace its authored extent
    /// (`wp:extent`, EMU) — the geometry op behind a handle drag-resize (docs/85
    /// §5.3). Self-inverse carrying the previous extent (the retained-value
    /// pattern, like [`Operation::SetParagraphProperties`]). `None` restores the
    /// "size resolved from content" state a missing extent means.
    SetExtent {
        /// The drawing / text-box node to resize (inline or floating).
        object: NodeId,
        /// The new authored extent (`None` = defer to content-derived sizing).
        extent: Option<Extent>,
    },
    /// Move / re-wrap / re-order a **floating** object: replace the whole
    /// [`DrawingAnchor`] of an `AnchoredDrawing` or floating `TextBox` (docs/85
    /// §5.3). One op covers position, wrap mode, wrap distances, and z-order.
    /// Self-inverse carrying the previous anchor (retained-value pattern).
    SetAnchor {
        /// The floating object node to reposition/re-wrap.
        object: NodeId,
        /// The new anchor (position + wrap + z-order).
        anchor: Box<DrawingAnchor>,
    },
    /// Set or clear a picture's source-rectangle crop (`a:srcRect`) on the resolved
    /// inline drawing or floating anchored drawing — the model side (`CropRect`)
    /// shipped by P1G-OBJ-MODEL. Self-inverse carrying the previous crop (the
    /// retained-value pattern, like [`Operation::SetExtent`]); `None` clears the
    /// crop (the whole source fills the box). An identity (all-zero) crop is
    /// normalized to `None`, and every edge is clamped into the model's crop range.
    /// Rejected (doc left unchanged) if `object` is not a croppable picture.
    SetImageCrop {
        /// The picture (inline or floating drawing) to crop.
        object: NodeId,
        /// The new crop, or `None` to clear it.
        crop: Option<CropRect>,
    },
    /// Set or clear an object's alt text (`wp:docPr@descr`) on the resolved inline
    /// drawing or floating anchored drawing. Self-inverse carrying the previous
    /// descr (retained-value pattern); `None` clears it. Rejected (doc left
    /// unchanged) if `object` is not an alt-text-bearing object, or the text is
    /// empty / longer than the model's byte bound.
    SetObjectDescr {
        /// The object whose alt text is replaced.
        object: NodeId,
        /// The new alt text, or `None` to clear it.
        descr: Option<String>,
    },
    /// Remove the resolved object node (an inline drawing, floating anchored
    /// drawing, text box, or group) from its inline container. Inverse:
    /// [`Operation::InsertObjectNode`], carrying the removed node + its position, so
    /// undo restores it verbatim (the retained-content pattern, like
    /// [`Operation::DeleteTable`]). This is a pure structural removal (surrounding
    /// runs are not coalesced, so the retained inverse restores verbatim). Rejected
    /// (doc left unchanged) when removing the object would leave the model invalid:
    /// it is the sole child of a container that must stay non-empty (a hyperlink,
    /// revision, or inline content control), or removing it would leave two
    /// mergeable equal-property runs adjacent — the host merges/reformats those
    /// siblings (or uses a range delete) before removing such an object.
    DeleteObject {
        /// The object to remove.
        object: NodeId,
    },
    /// Re-insert a previously removed object node at 0-based `index` within the
    /// inline container `owner` (the paragraph, hyperlink, or revision whose inline
    /// list held it). The inverse vehicle for [`Operation::DeleteObject`]; its own
    /// inverse is a [`Operation::DeleteObject`] of the re-inserted node.
    InsertObjectNode {
        /// The inline container to insert into (a paragraph / hyperlink / revision).
        owner: NodeId,
        /// The 0-based inline position within the container.
        index: u32,
        /// The object node to insert (its id must be the removed object's).
        node: Box<InlineNode>,
    },
    /// Insert a fresh inline object node (e.g. a picture [`InlineNode::Drawing`])
    /// at the caret `at`, splitting a straddling run so it lands exactly at the
    /// offset (the same run-boundary alignment [`Operation::InsertField`] uses).
    /// Inverse: [`Operation::DeleteObject`] of the node's id. Rejected (document
    /// left unchanged) on an out-of-range offset or a result that does not
    /// validate. Boxed to keep the enum small.
    InsertInlineObject {
        /// Where the object is inserted; a run boundary is created at `at.offset`.
        at: Pos,
        /// The inline object node to insert; its id and any nested ids must be fresh.
        node: Box<InlineNode>,
    },
    /// Remove the inline object node `object`, coalescing any equal-property runs
    /// it kept apart (so a run split by [`Operation::InsertInlineObject`] is
    /// restored verbatim). The inverse vehicle for `InsertInlineObject`; its own
    /// inverse re-inserts the removed node at its exact position.
    RemoveInlineObject {
        /// The inline object to remove.
        object: NodeId,
    },
    /// Replace a table cell's properties (shading, borders, vertical alignment,
    /// margins, span/merge, …). Its own inverse (carrying the previous properties).
    /// Boxed to keep the enum small.
    SetTableCellProperties {
        /// The cell whose properties are replaced.
        cell: NodeId,
        /// The properties to install.
        properties: Box<TableCellProperties>,
    },
    /// Replace a table's properties (borders, shading, alignment, indent, …). Its own
    /// inverse. Boxed to keep the enum small.
    SetTableProperties {
        /// The table whose properties are replaced.
        table: NodeId,
        /// The properties to install.
        properties: Box<TableProperties>,
    },
    /// Replace a table's full structure with `replacement`. This is reserved for
    /// structural transforms such as merge/split cells where exact undo needs the
    /// previous row/cell topology.
    ReplaceTable {
        /// The table to replace.
        table: NodeId,
        /// The replacement table. Its id must match `table`.
        replacement: Box<Table>,
    },
    /// Replace the document's core properties (`docProps/core.xml` — title,
    /// author, subject, …). Document-global, not node-scoped. Its own inverse
    /// (carrying the previous properties); rejected (doc left unchanged) if a
    /// field would exceed the model's bounded length.
    SetCoreProperties {
        /// The properties to install.
        properties: Box<CoreProperties>,
    },
    /// Atomically replace only review-touched paragraph inlines and, when
    /// necessary, the comments map. Its inverse carries the exact prior scoped
    /// values rather than a whole-document body snapshot.
    UpdateReviewState {
        /// Paragraph-local inline replacements. Node ids must be unique.
        paragraphs: Vec<ReviewParagraphState>,
        /// Replacement comments map, or `None` when revision-only editing leaves
        /// comment definitions untouched.
        comments: Option<DefinitionMap<CommentId, Comment>>,
    },
    /// Replace one section's page size, margins, orientation, and column layout
    /// (the "Page Setup" fields) — headers/footers, borders, and the section's
    /// other properties are untouched. Its own inverse; rejected
    /// (doc left unchanged) if a value falls outside the model's domain
    /// (e.g. a page dimension over ~22in).
    SetSectionGeometry {
        /// The section to update.
        section: SectionId,
        /// The page size to install.
        page_size: PageSize,
        /// The page margins to install.
        page_margins: PageMargins,
        /// The orientation to install (`None` clears it — the model then
        /// infers portrait/landscape from the page size on export).
        orientation: Option<PageOrientation>,
        /// The column layout to install.
        columns: SectionColumns,
    },
    /// Install, replace, or remove a style definition in the document's style
    /// registry (`word/styles.xml`). `Some(style)` inserts (create) or replaces
    /// (update) the definition at `id`; `None` removes it. Document-global, not
    /// node-scoped. Its own inverse, carrying the previous value at `id` (`None`
    /// when the id was absent), so undo restores the registry exactly — updating a
    /// style reflows every paragraph that references it, and undo reverses the
    /// reflow. Boxed to keep the enum small.
    SetStyleDefinition {
        /// The style id to install, replace, or remove.
        id: StyleId,
        /// The style to install (create/update), or `None` to remove `id`.
        style: Option<Box<Style>>,
    },
    /// Create a bookmark: register `name` under the fresh `bookmark` id in
    /// `Definitions::bookmarks` and insert its paired `BookmarkStart`/`BookmarkEnd`
    /// markers (zero-width points) at `start`/`end`. The two positions may lie in
    /// the same paragraph (a selection) or in two different paragraphs (a range the
    /// inverse of a delete restores). Markers are placed at paragraph top level:
    /// each position aligns to a run boundary first (splitting a run if it lands
    /// inside one). Inverse: [`Operation::DeleteBookmark`] of the same id. `name`
    /// must be non-empty and at most 255 bytes; the `bookmark` id and both marker
    /// ids must be fresh and distinct.
    CreateBookmark {
        /// The fresh bookmark identity (the definition key, shared by both markers).
        bookmark: BookmarkId,
        /// The bookmark name (non-empty, at most 255 bytes).
        name: String,
        /// Where the start marker is inserted.
        start: Pos,
        /// The fresh identity of the start marker.
        start_id: NodeId,
        /// Where the end marker is inserted.
        end: Pos,
        /// The fresh identity of the end marker.
        end_id: NodeId,
    },
    /// Delete a bookmark by id: remove its definition entry and both of its
    /// `BookmarkStart`/`BookmarkEnd` markers, coalescing any equal-property runs the
    /// removed markers had kept apart. Inverse: [`Operation::CreateBookmark`]
    /// carrying the removed name, marker ids, and exact marker positions, so undo
    /// re-inserts the pair where it was. The markers must resolve at paragraph top
    /// level (as this crate inserts them); a bookmark whose markers were imported
    /// nested inside an inline wrapper is out of this slice's scope.
    DeleteBookmark {
        /// The bookmark to remove.
        bookmark: BookmarkId,
    },
    /// Rename a bookmark by id in `Definitions::bookmarks`. Its own inverse,
    /// carrying the previous name. `name` must be non-empty and at most 255 bytes.
    RenameBookmark {
        /// The bookmark to rename.
        bookmark: BookmarkId,
        /// The new name (non-empty, at most 255 bytes).
        name: String,
    },
    /// Insert a pre-built inline [`Field`] node at the caret `at`. The field is
    /// placed at paragraph top level (a field never nests inside an inline
    /// wrapper), aligned to a run boundary — a run the offset lands inside is
    /// split first. Build a common field with [`CommonField::build`]; this crate
    /// evaluates nothing, so a clock/context field (`DATE`/`TIME`/…) carries its
    /// caller-formatted cached display text, and `PAGE`/`NUMPAGES` carry a `"1"`
    /// placeholder the pagination field pass recomputes. Inverse:
    /// [`Operation::RemoveField`] of the field's id. Rejected (doc left unchanged)
    /// if the caret does not resolve, the offset is out of range, or the resulting
    /// document does not validate (e.g. a duplicate node id or a nested field).
    /// Boxed to keep the enum small.
    InsertField {
        /// Where the field is inserted.
        at: Pos,
        /// The field node to insert; its id and any cached-run id must be fresh.
        field: Box<Field>,
    },
    /// Remove the inline [`Field`] node with id `field` from its paragraph,
    /// coalescing any equal-property runs the field kept apart. Inverse:
    /// [`Operation::InsertField`] carrying the removed field and its exact
    /// position, so undo re-inserts it verbatim. The field must sit at paragraph
    /// top level (as this crate inserts it); an id that does not resolve to a
    /// top-level field is rejected.
    RemoveField {
        /// The field to remove.
        field: NodeId,
    },
    /// Insert a footnote or endnote at the caret `at`: install its definition
    /// entry (keyed by `note`, in `Definitions::footnotes`/`endnotes` per `kind`)
    /// holding `blocks`, and splice an [`InlineNode::NoteReference`] with identity
    /// `reference_id` into the paragraph at `at`. The reference's displayed
    /// auto-number is derived at render — this op only creates the reference and
    /// the (typically single-empty-paragraph, ready-to-type) body. Refuses a
    /// `note` id already defined for `kind`, an out-of-range caret, or a caret
    /// interior to a non-run wrapper (the reference is always a top-level inline).
    /// Inverse: [`Operation::RemoveNote`], which drops both the reference and the
    /// definition; undo therefore restores the document verbatim.
    InsertNote {
        /// Whether this is a footnote or an endnote.
        kind: NoteKind,
        /// The new note's id (must not already be defined for `kind`).
        note: NoteId,
        /// The caret where the reference is spliced.
        at: Pos,
        /// The fresh identity of the new [`InlineNode::NoteReference`] inline.
        reference_id: NodeId,
        /// The note's block content, carried verbatim so the inverse restores it.
        blocks: Vec<BlockNode>,
    },
    /// Remove the footnote/endnote `note` and its body-side reference (the
    /// [`InlineNode::NoteReference`] inline with identity `reference_id`). The
    /// inverse vehicle for [`Operation::InsertNote`]: it recovers the reference's
    /// paragraph and offset and the removed body, so undo replays an exact
    /// [`Operation::InsertNote`]. Refuses when the reference or the definition is
    /// absent (the document is left unchanged).
    RemoveNote {
        /// Whether `note` is a footnote or an endnote.
        kind: NoteKind,
        /// The note definition to remove.
        note: NoteId,
        /// The body-side reference inline to remove.
        reference_id: NodeId,
    },
    /// Mint an empty header or footer body in the matching definition map
    /// (docs/85 §8.3 `CreateHeaderFooterBody`).
    ///
    /// Creating the body is separate from linking a section to it because the
    /// two are independently useful: unlinking a section ("Link to Previous"
    /// off) creates a body AND links it, while re-linking only removes a ref and
    /// leaves the body to be collected on export. Refuses if the id is already
    /// defined, so an existing body can never be silently orphaned.
    /// Inverse: [`Operation::RemoveHeaderFooterBody`].
    CreateHeaderFooterBody {
        /// Whether the body belongs to the header or the footer map.
        region: RunningRegion,
        /// The new body's id (must not already be defined for `region`).
        id: HeaderFooterId,
        /// The body's block content, carried so the inverse restores it verbatim.
        blocks: Vec<BlockNode>,
    },
    /// Remove a header or footer body. The inverse vehicle for
    /// [`Operation::CreateHeaderFooterBody`]; it carries the removed blocks back
    /// out so undo replays an exact create. Refuses when the body is absent.
    RemoveHeaderFooterBody {
        /// Which map the body lives in.
        region: RunningRegion,
        /// The body to remove.
        id: HeaderFooterId,
    },
    /// Point a section's header/footer variant at a body, or remove the
    /// reference (docs/85 §8.3 `SetSectionRunningRef`).
    ///
    /// `None` is how "Link to Previous" is expressed: OOXML models inheritance by
    /// a section OMITTING a reference, so removing it makes the section inherit
    /// the previous one's again (docs/85 §8.4, Q7). Self-inverse — the inverse is
    /// the same op carrying the previous reference.
    SetSectionRunningRef {
        /// The section whose variant is being pointed.
        section: SectionId,
        /// Header or footer.
        region: RunningRegion,
        /// Which variant (default / first page / even page).
        kind: HeaderFooterKind,
        /// The body to link, or `None` to inherit from the previous section.
        reference: Option<HeaderFooterId>,
    },
}

/// Whether a running-content op addresses the header or the footer side.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunningRegion {
    /// The header map / a section's `headers`.
    Header,
    /// The footer map / a section's `footers`.
    Footer,
}

/// Why an edit could not be applied. No partial mutation ever occurs: an op
/// validates before it mutates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditError {
    /// The target paragraph node does not exist.
    NodeNotFound,
    /// The offset is past the paragraph's text length.
    OffsetOutOfRange,
    /// The offset does not fall on a UTF-8 character boundary.
    NotCharBoundary,
    /// An empty insert, or an empty/inverted delete range.
    EmptyEdit,
    /// The range spans more than one paragraph (slice 1 is single-paragraph).
    CrossParagraph,
    /// The position is not inside an editable top-level run (e.g. inside a
    /// hyperlink/field wrapper or a tab). Slice-1 limitation.
    Unsupported,
    /// The node-id space is exhausted.
    IdExhausted,
    /// A field exceeds the model's bounded length (e.g. a metadata property).
    ValueTooLarge,
    /// A bookmark name is empty or exceeds the model's 255-byte bound, or a
    /// bookmark id / marker id collides with one already in use.
    InvalidName,
    /// A bookmark id does not resolve to a definition (or its markers cannot be
    /// located at paragraph top level).
    BookmarkNotFound,
    /// An inserted field is rejected by the model (a duplicate node id, a nested
    /// field, or invalid cached inlines), or a field removal would leave the
    /// document invalid.
    InvalidField,
    /// A field id does not resolve to a field at paragraph top level.
    FieldNotFound,
}

/// Applies `op` to `doc`, returning the inverse operation (for undo). `ids` mints
/// new run identities when an edit must create a run (e.g. typing into an empty
/// paragraph). On `Err`, `doc` is unchanged.
pub fn apply(
    doc: &mut Document,
    ids: &mut dyn RunIds,
    op: &Operation,
) -> Result<Operation, EditError> {
    match op {
        Operation::InsertText { at, text } => {
            if text.is_empty() {
                return Err(EditError::EmptyEdit);
            }
            let para = find_paragraph_mut(blocks_owning_mut(doc, at.node)?, at.node)
                .ok_or(EditError::NodeNotFound)?;
            if at.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            insert_text(&mut para.inlines, at.offset, text, ids)?;
            let end = Pos::new(at.node, at.offset + text.len() as u32);
            Ok(Operation::DeleteText {
                range: Range { start: *at, end },
            })
        }
        Operation::DeleteText { range } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            let para =
                find_paragraph_mut(blocks_owning_mut(doc, range.start.node)?, range.start.node)
                    .ok_or(EditError::NodeNotFound)?;
            if range.end.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            // Fast path: the whole range lies inside one run — remove the substring
            // and invert with `InsertText` (no clone; the common single-char
            // backspace stays cheap).
            if let Some(removed) =
                delete_text(&mut para.inlines, range.start.offset, range.end.offset)?
            {
                return Ok(Operation::InsertText {
                    at: range.start,
                    text: removed,
                });
            }
            // General path: the range spans several runs (a formatted paragraph, the
            // tail/head of a cross-paragraph selection). Snapshot for an exact
            // inverse, split runs at both ends, then drop every inline the range
            // fully covers. The inverse restores the inlines verbatim, so undo brings
            // back each deleted run's own formatting — an `InsertText` (plain text)
            // inverse could not.
            let old = para.inlines.clone();
            ensure_run_boundary(&mut para.inlines, range.end.offset, ids)?;
            ensure_run_boundary(&mut para.inlines, range.start.offset, ids)?;
            remove_covered_range(&mut para.inlines, range.start.offset, range.end.offset)?;
            // The removal can leave two equal-property runs adjacent (or a boundary
            // split earlier did); the model forbids that, so merge them back.
            coalesce_adjacent_runs(&mut para.inlines);
            Ok(Operation::SetInlines {
                node: range.start.node,
                inlines: old,
            })
        }
        Operation::SplitParagraph { at, new_id } => {
            // Word's `w:next` (`Style::next`): pressing Enter at the END of a
            // paragraph starts the style that one is declared to be followed by —
            // which is why a heading is followed by body text rather than another
            // heading. Splitting in the MIDDLE keeps the style on both halves,
            // because that is one paragraph becoming two, not a new one starting.
            // Resolved before the mutable borrow of the body below.
            let next_style = find_paragraph_any(doc, at.node).and_then(|para| {
                if at.offset != paragraph_text_len(para) {
                    return None;
                }
                let current = para.properties.style_ref?;
                let next = doc.definitions().styles.get(&current)?.next?;
                // A style that follows itself (the common case for body styles)
                // means "carry on", so there is nothing to change.
                (next != current).then_some(next)
            });
            if !split_paragraph(
                blocks_owning_mut(doc, at.node)?,
                at.node,
                at.offset,
                *new_id,
                ids,
            )? {
                return Err(EditError::NodeNotFound);
            }
            if let Some(next) = next_style
                && let Some(para) = find_paragraph_mut(blocks_owning_mut(doc, *new_id)?, *new_id)
            {
                para.properties.style_ref = Some(next);
            }
            Ok(Operation::JoinParagraphs {
                first: at.node,
                second: *new_id,
            })
        }
        Operation::JoinParagraphs { first, second } => {
            match join_paragraphs(blocks_owning_mut(doc, *first)?, *first, *second)? {
                Some(split_at) => Ok(Operation::SplitParagraph {
                    at: Pos::new(*first, split_at),
                    new_id: *second,
                }),
                None => Err(EditError::NodeNotFound),
            }
        }
        Operation::FormatText { range, delta } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            let node = range.start.node;
            let para = find_paragraph_mut(blocks_owning_mut(doc, node)?, node)
                .ok_or(EditError::NodeNotFound)?;
            if range.end.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            // Snapshot for an exact undo, then align run boundaries to the range
            // (end first, so the start offset stays valid) and format the covered
            // runs.
            let old = para.inlines.clone();
            ensure_run_boundary(&mut para.inlines, range.end.offset, ids)?;
            ensure_run_boundary(&mut para.inlines, range.start.offset, ids)?;
            // Descend into wrappers so a run inside a pending suggestion (or a
            // hyperlink/SDT) is formatted, not silently skipped (docs/86).
            let covered = covered_run_paths(&para.inlines, range.start.offset, range.end.offset);
            for path in covered {
                if let Some(run) = run_at_path_mut(&mut para.inlines, &path) {
                    delta.apply_to(&mut run.properties);
                }
            }
            // Formatting a sub-range to match a neighbour (or the boundary split
            // above) can leave adjacent equal-property runs, which the model forbids;
            // merge them so the document stays re-validatable and export-clean.
            coalesce_adjacent_runs(&mut para.inlines);
            Ok(Operation::SetInlines { node, inlines: old })
        }
        Operation::ClearFormatting { range } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            let node = range.start.node;
            let para = find_paragraph_mut(blocks_owning_mut(doc, node)?, node)
                .ok_or(EditError::NodeNotFound)?;
            if range.end.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            let old = para.inlines.clone();
            reject_partial_atomic(&para.inlines, range.start.offset, range.end.offset)?;
            ensure_run_boundary(&mut para.inlines, range.end.offset, ids)?;
            ensure_run_boundary(&mut para.inlines, range.start.offset, ids)?;
            // Descend into wrappers (docs/86); an all-markup range with no covered
            // run is still an unsupported clear, as before.
            let covered = covered_run_paths(&para.inlines, range.start.offset, range.end.offset);
            if covered.is_empty() {
                return Err(EditError::Unsupported);
            }
            for path in covered {
                if let Some(run) = run_at_path_mut(&mut para.inlines, &path) {
                    run.properties = RunProperties::default();
                }
            }
            coalesce_adjacent_runs(&mut para.inlines);
            Ok(Operation::SetInlines { node, inlines: old })
        }
        Operation::SetHyperlink {
            range,
            id,
            target,
            tooltip,
        } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            if !valid_hyperlink_values(target.as_ref(), tooltip.as_deref()) {
                return Err(EditError::Unsupported);
            }
            let node = range.start.node;
            let para = find_paragraph_mut(blocks_owning_mut(doc, node)?, node)
                .ok_or(EditError::NodeNotFound)?;
            if range.end.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            let old = para.inlines.clone();

            // Updating/removing an existing link is exact-range only. That keeps
            // edits deterministic and avoids silently splitting an imported
            // hyperlink wrapper.
            if let Some(index) =
                exact_hyperlink_index(&para.inlines, range.start.offset, range.end.offset)
            {
                if let Some(target) = target {
                    let InlineNode::Hyperlink(link) = &mut para.inlines[index] else {
                        unreachable!("exact_hyperlink_index only returns hyperlinks");
                    };
                    link.target = target.clone();
                    link.tooltip = tooltip.clone();
                } else {
                    let InlineNode::Hyperlink(link) = para.inlines.remove(index) else {
                        unreachable!("exact_hyperlink_index only returns hyperlinks");
                    };
                    para.inlines.splice(index..index, link.inlines);
                    coalesce_adjacent_runs(&mut para.inlines);
                }
                return Ok(Operation::SetInlines { node, inlines: old });
            }

            let Some(target) = target else {
                return Err(EditError::Unsupported);
            };
            // Creating a link currently accepts top-level text runs. Align both
            // boundaries first so the wrapper covers exactly the requested bytes.
            // Selections that cut through any existing wrapper remain unsupported.
            let mut next_inlines = para.inlines.clone();
            ensure_run_boundary(&mut next_inlines, range.end.offset, ids)?;
            ensure_run_boundary(&mut next_inlines, range.start.offset, ids)?;
            let covered =
                covered_top_level_indices(&next_inlines, range.start.offset, range.end.offset)?;
            if covered.is_empty()
                || covered
                    .iter()
                    .any(|index| !matches!(next_inlines[*index], InlineNode::Run(_)))
            {
                return Err(EditError::Unsupported);
            }
            let first = covered[0];
            let last = *covered.last().expect("covered is non-empty");
            let mut children: Vec<InlineNode> = next_inlines.drain(first..=last).collect();
            // Give newly-authored links a recognizable default without clobbering
            // explicit author formatting. Imported/updated links retain their
            // existing run styling verbatim.
            for child in &mut children {
                if let InlineNode::Run(run) = child {
                    run.properties.underline.get_or_insert(true);
                    run.properties.color.get_or_insert(Color::Rgb(RgbColor {
                        r: 0x05,
                        g: 0x63,
                        b: 0xc1,
                    }));
                }
            }
            next_inlines.insert(
                first,
                InlineNode::Hyperlink(Hyperlink {
                    id: *id,
                    target: target.clone(),
                    tooltip: tooltip.clone(),
                    inlines: children,
                }),
            );
            para.inlines = next_inlines;
            Ok(Operation::SetInlines { node, inlines: old })
        }
        Operation::SetInlines { node, inlines } => {
            let para = find_paragraph_mut(blocks_owning_mut(doc, *node)?, *node)
                .ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(&mut para.inlines, inlines.clone());
            Ok(Operation::SetInlines {
                node: *node,
                inlines: previous,
            })
        }
        Operation::SetParagraphProperties { node, properties } => {
            let para = find_paragraph_mut(blocks_owning_mut(doc, *node)?, *node)
                .ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(&mut para.properties, (**properties).clone());
            Ok(Operation::SetParagraphProperties {
                node: *node,
                properties: Box::new(previous),
            })
        }
        Operation::InsertRow { table, index, row } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            let idx = *index as usize;
            if idx > t.rows.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            t.rows.insert(idx, (**row).clone());
            Ok(Operation::DeleteRow {
                table: *table,
                index: *index,
            })
        }
        Operation::DeleteRow { table, index } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            let idx = *index as usize;
            if idx >= t.rows.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            // A table's rows are non-empty; removing the last row would make it
            // invalid. Deleting a whole table is a separate op.
            if t.rows.len() == 1 {
                return Err(EditError::Unsupported);
            }
            let removed = t.rows.remove(idx);
            Ok(Operation::InsertRow {
                table: *table,
                index: *index,
                row: Box::new(removed),
            })
        }
        Operation::InsertColumn {
            table,
            index,
            width,
            cells,
        } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            ensure_regular_table(t)?;
            let idx = *index as usize;
            if idx > t.grid.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            if cells.len() != t.rows.len() {
                return Err(EditError::Unsupported);
            }
            t.grid.insert(
                idx,
                GridColumn {
                    width_twips: *width,
                },
            );
            for (row, cell) in t.rows.iter_mut().zip(cells.iter()) {
                row.cells.insert(idx, cell.clone());
            }
            Ok(Operation::DeleteColumn {
                table: *table,
                index: *index,
            })
        }
        Operation::DeleteColumn { table, index } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            ensure_regular_table(t)?;
            let idx = *index as usize;
            if idx >= t.grid.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            // A row's cells are non-empty; removing the only column is invalid.
            if t.grid.len() == 1 {
                return Err(EditError::Unsupported);
            }
            let width = t.grid.remove(idx).width_twips;
            let cells: Vec<TableCell> =
                t.rows.iter_mut().map(|row| row.cells.remove(idx)).collect();
            Ok(Operation::InsertColumn {
                table: *table,
                index: *index,
                width,
                cells,
            })
        }
        Operation::DeleteTable { table } => {
            let (container, index, removed) = remove_table(doc.body_mut(), None, *table)?;
            Ok(Operation::InsertTable {
                container,
                index,
                table: Box::new(removed),
            })
        }
        Operation::InsertTable {
            container,
            index,
            table,
        } => {
            let blocks = match container {
                None => doc.body_mut(),
                Some(id) => {
                    find_container_blocks_mut(doc.body_mut(), *id).ok_or(EditError::NodeNotFound)?
                }
            };
            let idx = *index as usize;
            if idx > blocks.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            blocks.insert(idx, BlockNode::Table((**table).clone()));
            Ok(Operation::DeleteTable { table: table.id })
        }
        Operation::InsertBlocks {
            container,
            index,
            blocks: to_insert,
        } => {
            if to_insert.is_empty() {
                return Err(EditError::EmptyEdit);
            }
            let blocks = match container {
                None => doc.body_mut(),
                Some(id) => {
                    find_container_blocks_mut(doc.body_mut(), *id).ok_or(EditError::NodeNotFound)?
                }
            };
            let idx = *index as usize;
            if idx > blocks.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            for (offset, block) in to_insert.iter().enumerate() {
                blocks.insert(idx + offset, block.clone());
            }
            Ok(Operation::DeleteBlocks {
                container: *container,
                index: *index,
                count: to_insert.len() as u32,
            })
        }
        Operation::DeleteBlocks {
            container,
            index,
            count,
        } => {
            let blocks = match container {
                None => doc.body_mut(),
                Some(id) => {
                    find_container_blocks_mut(doc.body_mut(), *id).ok_or(EditError::NodeNotFound)?
                }
            };
            let idx = *index as usize;
            let count = *count as usize;
            if count == 0 {
                return Err(EditError::EmptyEdit);
            }
            if idx + count > blocks.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            // A cell / SDT container's block list must stay non-empty.
            if container.is_some() && count >= blocks.len() {
                return Err(EditError::Unsupported);
            }
            let removed: Vec<BlockNode> = blocks
                .splice(idx..idx + count, std::iter::empty())
                .collect();
            Ok(Operation::InsertBlocks {
                container: *container,
                index: *index,
                blocks: removed,
            })
        }
        Operation::SetExtent { object, extent } => {
            let previous = set_object_extent(doc.body_mut(), *object, *extent)
                .ok_or(EditError::NodeNotFound)?;
            Ok(Operation::SetExtent {
                object: *object,
                extent: previous,
            })
        }
        Operation::SetAnchor { object, anchor } => {
            let previous = set_object_anchor(doc.body_mut(), *object, anchor)
                .ok_or(EditError::NodeNotFound)?;
            Ok(Operation::SetAnchor {
                object: *object,
                anchor: Box::new(previous),
            })
        }
        Operation::SetImageCrop { object, crop } => {
            // Normalize an identity crop to "no crop" and clamp every edge into the
            // model's round-trippable range before it is stored.
            let crop = crop.map(CropRect::clamped).filter(|c| !c.is_identity());
            let previous =
                set_object_crop(doc.body_mut(), *object, crop).ok_or(EditError::NodeNotFound)?;
            Ok(Operation::SetImageCrop {
                object: *object,
                crop: previous,
            })
        }
        Operation::SetObjectDescr { object, descr } => {
            // The model bounds alt text to non-empty and at most `MAX_DESCR_BYTES`;
            // enforce it before mutating because an inline drawing's `descr` is not
            // length-checked by `Document::validate`.
            if let Some(text) = descr
                && (text.is_empty() || text.len() > MAX_DESCR_BYTES)
            {
                return Err(EditError::ValueTooLarge);
            }
            let previous =
                set_object_descr(doc.body_mut(), *object, descr).ok_or(EditError::NodeNotFound)?;
            if let Err(_err) = doc.validate() {
                // Roll back: any residual model rule (e.g. the anchored path's own
                // bound) must not leave a partial mutation behind.
                set_object_descr(doc.body_mut(), *object, &previous);
                return Err(EditError::ValueTooLarge);
            }
            Ok(Operation::SetObjectDescr {
                object: *object,
                descr: previous,
            })
        }
        Operation::DeleteObject { object } => {
            let (owner, index, node) =
                remove_object(doc.body_mut(), *object).ok_or(EditError::NodeNotFound)?;
            if let Err(_err) = doc.validate() {
                // Roll back: removing the object emptied a container that must stay
                // non-empty (a hyperlink / revision / inline SDT's sole child).
                let _ = try_insert_object(doc.body_mut(), owner, index, &node);
                return Err(EditError::Unsupported);
            }
            Ok(Operation::InsertObjectNode {
                owner,
                index,
                node: Box::new(node),
            })
        }
        Operation::InsertObjectNode { owner, index, node } => {
            if !try_insert_object(doc.body_mut(), *owner, *index, node)? {
                return Err(EditError::NodeNotFound);
            }
            Ok(Operation::DeleteObject { object: node.id() })
        }
        Operation::SetTableCellProperties { cell, properties } => {
            let c = find_cell_mut(doc.body_mut(), *cell).ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(&mut c.properties, (**properties).clone());
            Ok(Operation::SetTableCellProperties {
                cell: *cell,
                properties: Box::new(previous),
            })
        }
        Operation::SetTableProperties { table, properties } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(&mut t.properties, (**properties).clone());
            Ok(Operation::SetTableProperties {
                table: *table,
                properties: Box::new(previous),
            })
        }
        Operation::ReplaceTable { table, replacement } => {
            if replacement.id != *table {
                return Err(EditError::Unsupported);
            }
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(t, (**replacement).clone());
            Ok(Operation::ReplaceTable {
                table: *table,
                replacement: Box::new(previous),
            })
        }
        Operation::SetCoreProperties { properties } => {
            let slot = doc.properties_mut();
            let previous = std::mem::replace(&mut slot.core, (**properties).clone());
            if let Err(_err) = doc.validate() {
                // Roll back: no partial mutation ever survives an error.
                doc.properties_mut().core = previous;
                return Err(EditError::ValueTooLarge);
            }
            Ok(Operation::SetCoreProperties {
                properties: Box::new(previous),
            })
        }
        Operation::UpdateReviewState {
            paragraphs,
            comments,
        } => {
            if paragraphs.is_empty() && comments.is_none() {
                return Err(EditError::EmptyEdit);
            }
            for (index, paragraph) in paragraphs.iter().enumerate() {
                if paragraphs[..index]
                    .iter()
                    .any(|previous| previous.node == paragraph.node)
                    || find_paragraph_any(doc, paragraph.node).is_none()
                {
                    return Err(EditError::NodeNotFound);
                }
            }

            let mut previous_paragraphs = Vec::with_capacity(paragraphs.len());
            for replacement in paragraphs {
                let paragraph = find_paragraph_mut(doc.body_mut(), replacement.node)
                    .ok_or(EditError::NodeNotFound)?;
                previous_paragraphs.push(ReviewParagraphState {
                    node: replacement.node,
                    inlines: std::mem::replace(&mut paragraph.inlines, replacement.inlines.clone()),
                });
            }
            let previous_comments = comments.as_ref().map(|replacement| {
                std::mem::replace(&mut doc.definitions_mut().comments, replacement.clone())
            });
            if doc.validate().is_err() {
                for previous in &previous_paragraphs {
                    let paragraph = find_paragraph_mut(doc.body_mut(), previous.node)
                        .expect("review paragraph was prevalidated");
                    paragraph.inlines = previous.inlines.clone();
                }
                if let Some(previous) = previous_comments {
                    doc.definitions_mut().comments = previous;
                }
                return Err(EditError::ValueTooLarge);
            }
            Ok(Operation::UpdateReviewState {
                paragraphs: previous_paragraphs,
                comments: previous_comments,
            })
        }
        Operation::SetSectionGeometry {
            section,
            page_size,
            page_margins,
            orientation,
            columns,
        } => {
            let s = doc
                .definitions_mut()
                .sections
                .iter_mut()
                .find(|s| s.id == *section)
                .ok_or(EditError::NodeNotFound)?;
            let previous = (
                s.page_size,
                s.page_margins,
                s.orientation,
                s.columns.clone(),
            );
            s.page_size = *page_size;
            s.page_margins = *page_margins;
            s.orientation = *orientation;
            s.columns = columns.clone();
            if doc.validate().is_err() {
                let s = doc
                    .definitions_mut()
                    .sections
                    .iter_mut()
                    .find(|s| s.id == *section)
                    .expect("the section we just found still exists");
                (s.page_size, s.page_margins, s.orientation, s.columns) = previous;
                return Err(EditError::ValueTooLarge);
            }
            Ok(Operation::SetSectionGeometry {
                section: *section,
                page_size: previous.0,
                page_margins: previous.1,
                orientation: previous.2,
                columns: previous.3,
            })
        }
        Operation::SetStyleDefinition { id, style } => {
            let styles = &mut doc.definitions_mut().styles;
            let previous = match style {
                Some(style) => styles.insert(*id, (**style).clone()),
                None => styles.remove(id),
            };
            if doc.validate().is_err() {
                // Roll the registry back to exactly its prior state and refuse the
                // edit — an invalid style (e.g. a based-on cycle or a bad
                // reference) must never land, and no partial mutation may persist.
                let styles = &mut doc.definitions_mut().styles;
                match &previous {
                    Some(prev) => {
                        styles.insert(*id, prev.clone());
                    }
                    None => {
                        styles.remove(id);
                    }
                }
                return Err(EditError::ValueTooLarge);
            }
            Ok(Operation::SetStyleDefinition {
                id: *id,
                style: previous.map(Box::new),
            })
        }
        Operation::CreateBookmark {
            bookmark,
            name,
            start,
            start_id,
            end,
            end_id,
        } => {
            if !valid_bookmark_name(name) {
                return Err(EditError::InvalidName);
            }
            // The definition key and both marker ids must be fresh and distinct so
            // every node id in the document stays unique (the model re-checks this).
            let bookmark_node = bookmark.node_id();
            if start_id == end_id
                || bookmark_node == *start_id
                || bookmark_node == *end_id
                || doc.definitions().bookmarks.contains_key(bookmark)
            {
                return Err(EditError::InvalidName);
            }
            // A same-paragraph range must be well-formed (end at or after start);
            // cross-paragraph ordering is the caller's responsibility (the inverse
            // of a delete supplies document order).
            if start.node == end.node && end.offset < start.offset {
                return Err(EditError::EmptyEdit);
            }
            let start_marker = InlineNode::BookmarkStart(BookmarkStart {
                id: *start_id,
                bookmark: *bookmark,
            });
            let end_marker = InlineNode::BookmarkEnd(BookmarkEnd {
                id: *end_id,
                bookmark: *bookmark,
            });
            // Snapshot the affected paragraph(s) so any failure (a bad offset, a
            // failed re-validation) rolls back to exactly the prior state.
            let start_snapshot = find_paragraph_any(doc, start.node)
                .ok_or(EditError::NodeNotFound)?
                .inlines
                .clone();
            let end_snapshot = if end.node != start.node {
                Some(
                    find_paragraph_any(doc, end.node)
                        .ok_or(EditError::NodeNotFound)?
                        .inlines
                        .clone(),
                )
            } else {
                None
            };

            doc.definitions_mut()
                .bookmarks
                .insert(*bookmark, Bookmark { name: name.clone() });

            let insertion =
                insert_bookmark_pair(doc.body_mut(), *start, start_marker, *end, end_marker, ids);
            let outcome =
                insertion.and_then(|()| doc.validate().map_err(|_| EditError::InvalidName));
            if let Err(err) = outcome {
                // Roll back both the markers and the definition; no partial mutation
                // may survive an error.
                if let Some(para) = find_paragraph_mut(doc.body_mut(), start.node) {
                    para.inlines = start_snapshot;
                }
                if let Some(end_snapshot) = end_snapshot
                    && let Some(para) = find_paragraph_mut(doc.body_mut(), end.node)
                {
                    para.inlines = end_snapshot;
                }
                doc.definitions_mut().bookmarks.remove(bookmark);
                return Err(err);
            }
            Ok(Operation::DeleteBookmark {
                bookmark: *bookmark,
            })
        }
        Operation::DeleteBookmark { bookmark } => {
            let name = doc
                .definitions()
                .bookmarks
                .get(bookmark)
                .ok_or(EditError::BookmarkNotFound)?
                .name
                .clone();
            let (start_site, end_site) = surface_block_lists(doc)
                .into_iter()
                .find_map(|blocks| locate_bookmark_markers(blocks, *bookmark))
                .ok_or(EditError::BookmarkNotFound)?;

            let start_snapshot = find_paragraph_any(doc, start_site.node)
                .ok_or(EditError::NodeNotFound)?
                .inlines
                .clone();
            let end_snapshot = if end_site.node != start_site.node {
                Some(
                    find_paragraph_any(doc, end_site.node)
                        .ok_or(EditError::NodeNotFound)?
                        .inlines
                        .clone(),
                )
            } else {
                None
            };

            if let Some(para) = find_paragraph_mut(doc.body_mut(), start_site.node) {
                remove_marker_by_id(&mut para.inlines, start_site.marker);
            }
            if let Some(para) = find_paragraph_mut(doc.body_mut(), end_site.node) {
                remove_marker_by_id(&mut para.inlines, end_site.marker);
            }
            // Removing a marker can leave two equal-property runs it had kept apart
            // adjacent, which the model forbids; coalesce each affected paragraph.
            if let Some(para) = find_paragraph_mut(doc.body_mut(), start_site.node) {
                coalesce_adjacent_runs(&mut para.inlines);
            }
            if end_site.node != start_site.node
                && let Some(para) = find_paragraph_mut(doc.body_mut(), end_site.node)
            {
                coalesce_adjacent_runs(&mut para.inlines);
            }
            let removed = doc.definitions_mut().bookmarks.remove(bookmark);

            if doc.validate().is_err() {
                if let Some(para) = find_paragraph_mut(doc.body_mut(), start_site.node) {
                    para.inlines = start_snapshot;
                }
                if let Some(end_snapshot) = end_snapshot
                    && let Some(para) = find_paragraph_mut(doc.body_mut(), end_site.node)
                {
                    para.inlines = end_snapshot;
                }
                if let Some(removed) = removed {
                    doc.definitions_mut().bookmarks.insert(*bookmark, removed);
                }
                return Err(EditError::InvalidName);
            }
            Ok(Operation::CreateBookmark {
                bookmark: *bookmark,
                name,
                start: Pos::new(start_site.node, start_site.offset),
                start_id: start_site.marker,
                end: Pos::new(end_site.node, end_site.offset),
                end_id: end_site.marker,
            })
        }
        Operation::RenameBookmark { bookmark, name } => {
            if !valid_bookmark_name(name) {
                return Err(EditError::InvalidName);
            }
            let previous = doc
                .definitions()
                .bookmarks
                .get(bookmark)
                .ok_or(EditError::BookmarkNotFound)?
                .name
                .clone();
            doc.definitions_mut()
                .bookmarks
                .insert(*bookmark, Bookmark { name: name.clone() });
            Ok(Operation::RenameBookmark {
                bookmark: *bookmark,
                name: previous,
            })
        }
        Operation::InsertField { at, field } => {
            // Snapshot the target paragraph so any failure (a bad offset, a field
            // the model rejects) rolls back to exactly the prior state; a field is
            // always inserted at paragraph top level, aligned to a run boundary.
            let snapshot = find_paragraph_any(doc, at.node)
                .ok_or(EditError::NodeNotFound)?
                .inlines
                .clone();
            let insertion = insert_field_at(doc.body_mut(), *at, (**field).clone(), ids);
            let outcome =
                insertion.and_then(|()| doc.validate().map_err(|_| EditError::InvalidField));
            if let Err(err) = outcome {
                if let Some(para) = find_paragraph_mut(blocks_owning_mut(doc, at.node)?, at.node) {
                    para.inlines = snapshot;
                }
                return Err(err);
            }
            Ok(Operation::RemoveField { field: field.id })
        }
        Operation::InsertInlineObject { at, node } => {
            // Snapshot the target paragraph so any failure (bad offset, a node the
            // model rejects) rolls back to exactly the prior state; the object is
            // inserted at paragraph top level, aligned to a run boundary.
            let snapshot = find_paragraph_any(doc, at.node)
                .ok_or(EditError::NodeNotFound)?
                .inlines
                .clone();
            let insertion = insert_inline_object_at(doc.body_mut(), *at, (**node).clone(), ids);
            let outcome =
                insertion.and_then(|()| doc.validate().map_err(|_| EditError::Unsupported));
            if let Err(err) = outcome {
                if let Some(para) = find_paragraph_mut(blocks_owning_mut(doc, at.node)?, at.node) {
                    para.inlines = snapshot;
                }
                return Err(err);
            }
            Ok(Operation::RemoveInlineObject { object: node.id() })
        }
        Operation::RemoveInlineObject { object } => {
            let (node, offset, removed) = surface_block_lists(doc)
                .into_iter()
                .find_map(|blocks| locate_inline_object(blocks, *object))
                .ok_or(EditError::NodeNotFound)?;
            let snapshot = find_paragraph_any(doc, node)
                .ok_or(EditError::NodeNotFound)?
                .inlines
                .clone();
            if let Some(para) = find_paragraph_mut(blocks_owning_mut(doc, node)?, node) {
                para.inlines
                    .retain(|i| !(is_object_node(i) && i.id() == *object));
                // Removing the object can leave the two equal-property runs it kept
                // apart adjacent, which the model forbids; coalesce them back so the
                // original run is restored verbatim.
                coalesce_adjacent_runs(&mut para.inlines);
            }
            if doc.validate().is_err() {
                if let Some(para) = find_paragraph_mut(blocks_owning_mut(doc, node)?, node) {
                    para.inlines = snapshot;
                }
                return Err(EditError::Unsupported);
            }
            Ok(Operation::InsertInlineObject {
                at: Pos::new(node, offset),
                node: Box::new(removed),
            })
        }
        Operation::RemoveField { field } => {
            let (node, offset, removed) = surface_block_lists(doc)
                .into_iter()
                .find_map(|blocks| locate_field(blocks, *field))
                .ok_or(EditError::FieldNotFound)?;
            let snapshot = find_paragraph_any(doc, node)
                .ok_or(EditError::NodeNotFound)?
                .inlines
                .clone();
            if let Some(para) = find_paragraph_mut(blocks_owning_mut(doc, node)?, node) {
                remove_field_by_id(&mut para.inlines, *field);
                // Removing the field can leave two equal-property runs it kept
                // apart adjacent, which the model forbids; coalesce them back.
                coalesce_adjacent_runs(&mut para.inlines);
            }
            if doc.validate().is_err() {
                if let Some(para) = find_paragraph_mut(blocks_owning_mut(doc, node)?, node) {
                    para.inlines = snapshot;
                }
                return Err(EditError::InvalidField);
            }
            Ok(Operation::InsertField {
                at: Pos::new(node, offset),
                field: Box::new(removed),
            })
        }
        Operation::CreateHeaderFooterBody { region, id, blocks } => {
            // Refuse rather than overwrite: an existing body silently replaced
            // would orphan whatever a section still points at.
            let map = match region {
                RunningRegion::Header => &doc.definitions().headers,
                RunningRegion::Footer => &doc.definitions().footers,
            };
            if map.contains_key(id) {
                return Err(EditError::Unsupported);
            }
            let body = HeaderFooter {
                blocks: blocks.clone(),
            };
            match region {
                RunningRegion::Header => doc.definitions_mut().headers.insert(*id, body),
                RunningRegion::Footer => doc.definitions_mut().footers.insert(*id, body),
            };
            Ok(Operation::RemoveHeaderFooterBody {
                region: *region,
                id: *id,
            })
        }
        Operation::RemoveHeaderFooterBody { region, id } => {
            let removed = match region {
                RunningRegion::Header => doc.definitions_mut().headers.remove(id),
                RunningRegion::Footer => doc.definitions_mut().footers.remove(id),
            }
            .ok_or(EditError::NodeNotFound)?;
            // Carry the blocks back out so undo replays an exact create.
            Ok(Operation::CreateHeaderFooterBody {
                region: *region,
                id: *id,
                blocks: removed.blocks,
            })
        }
        Operation::SetSectionRunningRef {
            section,
            region,
            kind,
            reference,
        } => {
            let boundary = doc
                .definitions_mut()
                .sections
                .iter_mut()
                .find(|candidate| candidate.id == *section)
                .ok_or(EditError::NodeNotFound)?;
            let refs = match region {
                RunningRegion::Header => &mut boundary.headers,
                RunningRegion::Footer => &mut boundary.footers,
            };
            let existing = refs.iter().position(|entry| entry.kind == *kind);
            let previous = existing.map(|index| refs[index].reference);
            match (existing, reference) {
                // Point the variant at a different body.
                (Some(index), Some(id)) => refs[index].reference = *id,
                // Add a variant this section did not declare.
                (None, Some(id)) => refs.push(HeaderFooterRef {
                    kind: *kind,
                    reference: *id,
                }),
                // Remove it, so the section inherits the previous section's again
                // — OOXML expresses "Link to Previous" as absence (docs/85 §8.4).
                (Some(index), None) => {
                    refs.remove(index);
                }
                (None, None) => {}
            }
            // Self-inverse: the same op carrying whatever was there before.
            Ok(Operation::SetSectionRunningRef {
                section: *section,
                region: *region,
                kind: *kind,
                reference: previous,
            })
        }
        Operation::InsertNote {
            kind,
            note,
            at,
            reference_id,
            blocks,
        } => {
            // A fresh note id must not already be defined for its kind — inserting
            // over an existing note would silently orphan the old body.
            let already_defined = match kind {
                NoteKind::Footnote => doc.definitions().footnotes.contains_key(note),
                NoteKind::Endnote => doc.definitions().endnotes.contains_key(note),
            };
            if already_defined {
                return Err(EditError::Unsupported);
            }
            let para = find_paragraph_mut(blocks_owning_mut(doc, at.node)?, at.node)
                .ok_or(EditError::NodeNotFound)?;
            if at.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            // Snapshot the paragraph's inlines so an invalid result rolls back
            // exactly (no partial mutation ever persists).
            let old_inlines = para.inlines.clone();
            ensure_run_boundary(&mut para.inlines, at.offset, ids)?;
            let Some(index) = top_level_insert_index(&para.inlines, at.offset) else {
                // The caret fell interior to a non-run wrapper (hyperlink/SDT); the
                // reference is only ever a top-level inline (slice-1 limitation).
                para.inlines = old_inlines;
                return Err(EditError::Unsupported);
            };
            para.inlines.insert(
                index,
                InlineNode::NoteReference(NoteReference {
                    id: *reference_id,
                    kind: *kind,
                    note: *note,
                }),
            );
            match kind {
                NoteKind::Footnote => {
                    doc.definitions_mut().footnotes.insert(
                        *note,
                        Note {
                            blocks: blocks.clone(),
                        },
                    );
                }
                NoteKind::Endnote => {
                    doc.definitions_mut().endnotes.insert(
                        *note,
                        Note {
                            blocks: blocks.clone(),
                        },
                    );
                }
            }
            if doc.validate().is_err() {
                // Roll back both sides: the definition entry and the reference —
                // an invalid note (a colliding id, a malformed body) must not land.
                match kind {
                    NoteKind::Footnote => {
                        doc.definitions_mut().footnotes.remove(note);
                    }
                    NoteKind::Endnote => {
                        doc.definitions_mut().endnotes.remove(note);
                    }
                }
                let para = find_paragraph_mut(blocks_owning_mut(doc, at.node)?, at.node)
                    .expect("the paragraph we just edited still exists");
                para.inlines = old_inlines;
                return Err(EditError::Unsupported);
            }
            Ok(Operation::RemoveNote {
                kind: *kind,
                note: *note,
                reference_id: *reference_id,
            })
        }
        Operation::RemoveNote {
            kind,
            note,
            reference_id,
        } => {
            // Locate and remove the body-side reference, recovering its paragraph
            // and offset for the inverse's caret.
            let Some((para_node, offset, old_inlines)) =
                remove_note_reference(doc.body_mut(), *reference_id)
            else {
                return Err(EditError::NodeNotFound);
            };
            let removed = match kind {
                NoteKind::Footnote => doc.definitions_mut().footnotes.remove(note),
                NoteKind::Endnote => doc.definitions_mut().endnotes.remove(note),
            };
            let Some(removed) = removed else {
                // The reference existed but the definition did not: restore the
                // reference and refuse (no partial mutation).
                let para = find_paragraph_mut(doc.body_mut(), para_node)
                    .expect("the paragraph we just edited still exists");
                para.inlines = old_inlines;
                return Err(EditError::NodeNotFound);
            };
            if doc.validate().is_err() {
                match kind {
                    NoteKind::Footnote => {
                        doc.definitions_mut().footnotes.insert(*note, removed);
                    }
                    NoteKind::Endnote => {
                        doc.definitions_mut().endnotes.insert(*note, removed);
                    }
                }
                let para = find_paragraph_mut(doc.body_mut(), para_node)
                    .expect("the paragraph we just edited still exists");
                para.inlines = old_inlines;
                return Err(EditError::Unsupported);
            }
            Ok(Operation::InsertNote {
                kind: *kind,
                note: *note,
                at: Pos::new(para_node, offset),
                reference_id: *reference_id,
                blocks: removed.blocks,
            })
        }
    }
}

/// The top-level insertion index in `inlines` for a caret at byte `offset`, or
/// `None` when the offset falls strictly inside a non-run wrapper (a hyperlink /
/// SDT), where no top-level boundary exists. Callers align run boundaries with
/// [`ensure_run_boundary`] first, so a run straddling the offset has already been
/// split; the index then lands exactly between the two halves.
fn top_level_insert_index(inlines: &[InlineNode], offset: u32) -> Option<usize> {
    let mut cum = 0u32;
    for (idx, inline) in inlines.iter().enumerate() {
        if cum == offset {
            return Some(idx);
        }
        cum += inline_text_len(inline);
    }
    (cum == offset).then_some(inlines.len())
}

/// Removes the top-level [`InlineNode::NoteReference`] with identity
/// `reference_id` from `blocks` or any nested cell / SDT, returning its
/// paragraph's id, the reference's byte offset within that paragraph, and a
/// snapshot of the paragraph's inlines *before* removal (for an exact rollback).
/// After removal the paragraph's runs are coalesced, so the two runs a
/// zero-width reference separated merge back — undoing an [`Operation::InsertNote`]
/// that split a run restores the original run verbatim.
fn remove_note_reference(
    blocks: &mut [BlockNode],
    reference_id: NodeId,
) -> Option<(NodeId, u32, Vec<InlineNode>)> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(para) => {
                if let Some(found) = take_note_reference(para, reference_id) {
                    return Some(found);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(found) = remove_note_reference(&mut cell.blocks, reference_id) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = remove_note_reference(&mut sdt.blocks, reference_id) {
                    return Some(found);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// Removes the top-level note reference `reference_id` from `para` if present,
/// returning `(paragraph id, reference offset, pre-removal inlines)`.
fn take_note_reference(
    para: &mut Paragraph,
    reference_id: NodeId,
) -> Option<(NodeId, u32, Vec<InlineNode>)> {
    let mut cum = 0u32;
    let mut target = None;
    for (idx, inline) in para.inlines.iter().enumerate() {
        if matches!(inline, InlineNode::NoteReference(reference) if reference.id == reference_id) {
            target = Some((idx, cum));
            break;
        }
        cum += inline_text_len(inline);
    }
    let (idx, offset) = target?;
    let snapshot = para.inlines.clone();
    para.inlines.remove(idx);
    // The reference sat between two runs the split created; with it gone they may
    // be adjacent and equal-propertied, which the model forbids — coalesce so the
    // original run is restored exactly.
    coalesce_adjacent_runs(&mut para.inlines);
    Some((para.id, offset, snapshot))
}

/// Removes the table `table_id` from `blocks` or any nested cell/SDT, returning its
/// container (`None` = this level / the body, else the owning cell/SDT id), its
/// 0-based index there, and the removed table (for the inverse). Refuses to empty a
/// nested container (a cell's/SDT's blocks are non-empty). `container` is the id of
/// the container `blocks` belongs to as the recursion descends.
fn remove_table(
    blocks: &mut Vec<BlockNode>,
    container: Option<NodeId>,
    table_id: NodeId,
) -> Result<(Option<NodeId>, u32, Table), EditError> {
    if let Some(i) = blocks
        .iter()
        .position(|b| matches!(b, BlockNode::Table(t) if t.id == table_id))
    {
        if container.is_some() && blocks.len() == 1 {
            return Err(EditError::Unsupported);
        }
        let BlockNode::Table(t) = blocks.remove(i) else {
            unreachable!("position matched a table");
        };
        return Ok((container, i as u32, t));
    }
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        match remove_table(&mut cell.blocks, Some(cell.id), table_id) {
                            Ok(found) => return Ok(found),
                            Err(EditError::NodeNotFound) => {}
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                let sid = sdt.id;
                match remove_table(&mut sdt.blocks, Some(sid), table_id) {
                    Ok(found) => return Ok(found),
                    Err(EditError::NodeNotFound) => {}
                    Err(e) => return Err(e),
                }
            }
            _ => {}
        }
    }
    Err(EditError::NodeNotFound)
}

/// Sets the authored extent of the drawing / text box `object` (inline
/// `Drawing`/`TextBox` or a floating `AnchoredDrawing`), searched recursively
/// across body paragraphs (through hyperlink/revision wrappers), table cells,
/// block SDTs, and inline text-box bodies. Returns the **previous** extent (as an
/// `Option`, so a floating drawing's always-present extent and an inline
/// object's optional extent share one inverse shape); `None` if `object` is not
/// a resizable object. [`Operation::SetExtent`]'s target.
fn set_object_extent(
    blocks: &mut [BlockNode],
    object: NodeId,
    extent: Option<Extent>,
) -> Option<Option<Extent>> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(prev) =
                    set_object_extent_in_inlines(&mut paragraph.inlines, object, extent)
                {
                    return Some(prev);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(prev) = set_object_extent(&mut cell.blocks, object, extent) {
                            return Some(prev);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(prev) = set_object_extent(&mut sdt.blocks, object, extent) {
                    return Some(prev);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

fn set_object_extent_in_inlines(
    inlines: &mut [InlineNode],
    object: NodeId,
    extent: Option<Extent>,
) -> Option<Option<Extent>> {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Drawing(drawing) if drawing.id == object => {
                return Some(core::mem::replace(&mut drawing.extent, extent));
            }
            // A floating anchored picture always carries an extent; a `None`
            // request leaves it unchanged (resize always supplies a size).
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                let previous = Some(drawing.extent);
                if let Some(new) = extent {
                    drawing.extent = new;
                }
                return Some(previous);
            }
            InlineNode::TextBox(text_box) => {
                if text_box.id == object {
                    return Some(core::mem::replace(&mut text_box.extent, extent));
                }
                if let Some(prev) = set_object_extent(&mut text_box.blocks, object, extent) {
                    return Some(prev);
                }
            }
            InlineNode::Hyperlink(hyperlink) => {
                if let Some(prev) =
                    set_object_extent_in_inlines(&mut hyperlink.inlines, object, extent)
                {
                    return Some(prev);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(prev) =
                    set_object_extent_in_inlines(&mut revision.inlines, object, extent)
                {
                    return Some(prev);
                }
            }
            _ => {}
        }
    }
    None
}

/// Replaces the [`DrawingAnchor`] of the floating object `object` (an
/// `AnchoredDrawing`, a floating `TextBox`, or an anchored `Group`), searched the
/// same way as [`set_object_extent`]. Returns the **previous** anchor; `None` if
/// `object` is not a currently floating object (an inline object has no anchor to
/// set, so a move/wrap is rejected rather than silently converting it to a float).
/// [`Operation::SetAnchor`]'s target.
fn set_object_anchor(
    blocks: &mut [BlockNode],
    object: NodeId,
    anchor: &DrawingAnchor,
) -> Option<DrawingAnchor> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(prev) =
                    set_object_anchor_in_inlines(&mut paragraph.inlines, object, anchor)
                {
                    return Some(prev);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(prev) = set_object_anchor(&mut cell.blocks, object, anchor) {
                            return Some(prev);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(prev) = set_object_anchor(&mut sdt.blocks, object, anchor) {
                    return Some(prev);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

fn set_object_anchor_in_inlines(
    inlines: &mut [InlineNode],
    object: NodeId,
    anchor: &DrawingAnchor,
) -> Option<DrawingAnchor> {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return Some(core::mem::replace(&mut drawing.anchor, anchor.clone()));
            }
            InlineNode::TextBox(text_box) => {
                if text_box.id == object {
                    // Only an already-floating text box can be re-anchored.
                    return text_box.anchor.replace(anchor.clone());
                }
                if let Some(prev) = set_object_anchor(&mut text_box.blocks, object, anchor) {
                    return Some(prev);
                }
            }
            InlineNode::Group(group) if group.id == object => {
                return group.anchor.replace(anchor.clone());
            }
            InlineNode::Hyperlink(hyperlink) => {
                if let Some(prev) =
                    set_object_anchor_in_inlines(&mut hyperlink.inlines, object, anchor)
                {
                    return Some(prev);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(prev) =
                    set_object_anchor_in_inlines(&mut revision.inlines, object, anchor)
                {
                    return Some(prev);
                }
            }
            _ => {}
        }
    }
    None
}

/// Sets or clears the source-rectangle crop of the picture `object` (an inline
/// `Drawing` or floating `AnchoredDrawing`), searched the same way as
/// [`set_object_extent`] (hyperlink/revision wrappers, text-box bodies, table
/// cells, block SDTs). Returns the **previous** crop; `None` if `object` is not a
/// croppable picture (a text box / group has no `a:srcRect`).
/// [`Operation::SetImageCrop`]'s target.
fn set_object_crop(
    blocks: &mut [BlockNode],
    object: NodeId,
    crop: Option<CropRect>,
) -> Option<Option<CropRect>> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(prev) = set_object_crop_in_inlines(&mut paragraph.inlines, object, crop)
                {
                    return Some(prev);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(prev) = set_object_crop(&mut cell.blocks, object, crop) {
                            return Some(prev);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(prev) = set_object_crop(&mut sdt.blocks, object, crop) {
                    return Some(prev);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

fn set_object_crop_in_inlines(
    inlines: &mut [InlineNode],
    object: NodeId,
    crop: Option<CropRect>,
) -> Option<Option<CropRect>> {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Drawing(drawing) if drawing.id == object => {
                return Some(core::mem::replace(&mut drawing.crop, crop));
            }
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return Some(core::mem::replace(&mut drawing.crop, crop));
            }
            InlineNode::TextBox(text_box) => {
                if let Some(prev) = set_object_crop(&mut text_box.blocks, object, crop) {
                    return Some(prev);
                }
            }
            InlineNode::Hyperlink(hyperlink) => {
                if let Some(prev) = set_object_crop_in_inlines(&mut hyperlink.inlines, object, crop)
                {
                    return Some(prev);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(prev) = set_object_crop_in_inlines(&mut revision.inlines, object, crop)
                {
                    return Some(prev);
                }
            }
            _ => {}
        }
    }
    None
}

/// Sets or clears the alt text of the object `object` (an inline `Drawing` or
/// floating `AnchoredDrawing`), searched the same way as [`set_object_extent`].
/// Returns the **previous** alt text; `None` if `object` is not an
/// alt-text-bearing object. [`Operation::SetObjectDescr`]'s target.
fn set_object_descr(
    blocks: &mut [BlockNode],
    object: NodeId,
    descr: &Option<String>,
) -> Option<Option<String>> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(prev) =
                    set_object_descr_in_inlines(&mut paragraph.inlines, object, descr)
                {
                    return Some(prev);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(prev) = set_object_descr(&mut cell.blocks, object, descr) {
                            return Some(prev);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(prev) = set_object_descr(&mut sdt.blocks, object, descr) {
                    return Some(prev);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

fn set_object_descr_in_inlines(
    inlines: &mut [InlineNode],
    object: NodeId,
    descr: &Option<String>,
) -> Option<Option<String>> {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Drawing(drawing) if drawing.id == object => {
                return Some(core::mem::replace(&mut drawing.descr, descr.clone()));
            }
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return Some(core::mem::replace(&mut drawing.descr, descr.clone()));
            }
            InlineNode::TextBox(text_box) => {
                if let Some(prev) = set_object_descr(&mut text_box.blocks, object, descr) {
                    return Some(prev);
                }
            }
            InlineNode::Hyperlink(hyperlink) => {
                if let Some(prev) =
                    set_object_descr_in_inlines(&mut hyperlink.inlines, object, descr)
                {
                    return Some(prev);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(prev) =
                    set_object_descr_in_inlines(&mut revision.inlines, object, descr)
                {
                    return Some(prev);
                }
            }
            _ => {}
        }
    }
    None
}

/// The current alt text (`wp:docPr@descr`) of the drawing `object`, or `None`
/// when the object has no alt text or does not resolve to a drawing. Read-only
/// companion to [`Operation::SetObjectDescr`], searched the same way, so a host
/// can prefill an alt-text editor with the existing description instead of
/// blind-overwriting it.
#[must_use]
pub fn object_descr(document: &Document, object: NodeId) -> Option<String> {
    object_descr_in_blocks(document.body(), object)
}

fn object_descr_in_blocks(blocks: &[BlockNode], object: NodeId) -> Option<String> {
    for block in blocks {
        let found = match block {
            BlockNode::Paragraph(paragraph) => object_descr_in_inlines(&paragraph.inlines, object),
            BlockNode::Table(table) => table.rows.iter().find_map(|row| {
                row.cells
                    .iter()
                    .find_map(|cell| object_descr_in_blocks(&cell.blocks, object))
            }),
            BlockNode::Sdt(sdt) => object_descr_in_blocks(&sdt.blocks, object),
            BlockNode::AltChunk(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn object_descr_in_inlines(inlines: &[InlineNode], object: NodeId) -> Option<String> {
    for inline in inlines {
        let found = match inline {
            InlineNode::Drawing(drawing) if drawing.id == object => return drawing.descr.clone(),
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return drawing.descr.clone();
            }
            InlineNode::TextBox(text_box) => object_descr_in_blocks(&text_box.blocks, object),
            InlineNode::Hyperlink(hyperlink) => object_descr_in_inlines(&hyperlink.inlines, object),
            InlineNode::Revision(revision) => object_descr_in_inlines(&revision.inlines, object),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Whether `node` is a removable object (the target set of
/// [`Operation::DeleteObject`]): an inline drawing, a floating anchored drawing, a
/// text box, or a DrawingML group.
fn is_object_node(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::Drawing(_)
            | InlineNode::AnchoredDrawing(_)
            | InlineNode::TextBox(_)
            | InlineNode::Group(_)
    )
}

/// Removes the object `object` from its inline container, searched the same way as
/// [`set_object_extent`]. Returns `(owner, index, node)` — the id of the inline
/// container the object was removed from (a paragraph, hyperlink, or revision), the
/// 0-based inline position it occupied, and the removed node — so
/// [`Operation::DeleteObject`] can build an exact-restore inverse. `None` if
/// `object` is not a removable object.
fn remove_object(blocks: &mut [BlockNode], object: NodeId) -> Option<(NodeId, u32, InlineNode)> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(found) =
                    remove_object_from_inlines(paragraph.id, &mut paragraph.inlines, object)
                {
                    return Some(found);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(found) = remove_object(&mut cell.blocks, object) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = remove_object(&mut sdt.blocks, object) {
                    return Some(found);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

fn remove_object_from_inlines(
    owner: NodeId,
    inlines: &mut Vec<InlineNode>,
    object: NodeId,
) -> Option<(NodeId, u32, InlineNode)> {
    // A direct child of this inline list: remove it and record its position.
    if let Some(index) = inlines
        .iter()
        .position(|node| node.id() == object && is_object_node(node))
    {
        let node = inlines.remove(index);
        return Some((owner, index as u32, node));
    }
    // Otherwise descend into wrappers (which own their own inline lists) and
    // text-box bodies, exactly as the resize/anchor resolvers do.
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Hyperlink(hyperlink) => {
                if let Some(found) =
                    remove_object_from_inlines(hyperlink.id, &mut hyperlink.inlines, object)
                {
                    return Some(found);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(found) =
                    remove_object_from_inlines(revision.id, &mut revision.inlines, object)
                {
                    return Some(found);
                }
            }
            InlineNode::TextBox(text_box) => {
                if let Some(found) = remove_object(&mut text_box.blocks, object) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// Inserts `node` at 0-based inline `index` within the container `owner` (a
/// paragraph, hyperlink, or revision), searched the same way as [`remove_object`].
/// The re-insertion target for [`Operation::InsertObjectNode`]. Returns `Ok(true)`
/// when inserted, `Ok(false)` when `owner` is not found, and `Err` when `owner` is
/// found but `index` is past the end of its inline list.
fn try_insert_object(
    blocks: &mut [BlockNode],
    owner: NodeId,
    index: u32,
    node: &InlineNode,
) -> Result<bool, EditError> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if paragraph.id == owner {
                    insert_object_at(&mut paragraph.inlines, index, node)?;
                    return Ok(true);
                }
                if try_insert_object_in_inlines(&mut paragraph.inlines, owner, index, node)? {
                    return Ok(true);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if try_insert_object(&mut cell.blocks, owner, index, node)? {
                            return Ok(true);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if try_insert_object(&mut sdt.blocks, owner, index, node)? {
                    return Ok(true);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    Ok(false)
}

fn try_insert_object_in_inlines(
    inlines: &mut [InlineNode],
    owner: NodeId,
    index: u32,
    node: &InlineNode,
) -> Result<bool, EditError> {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Hyperlink(hyperlink) => {
                if hyperlink.id == owner {
                    insert_object_at(&mut hyperlink.inlines, index, node)?;
                    return Ok(true);
                }
                if try_insert_object_in_inlines(&mut hyperlink.inlines, owner, index, node)? {
                    return Ok(true);
                }
            }
            InlineNode::Revision(revision) => {
                if revision.id == owner {
                    insert_object_at(&mut revision.inlines, index, node)?;
                    return Ok(true);
                }
                if try_insert_object_in_inlines(&mut revision.inlines, owner, index, node)? {
                    return Ok(true);
                }
            }
            InlineNode::TextBox(text_box) => {
                if try_insert_object(&mut text_box.blocks, owner, index, node)? {
                    return Ok(true);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

fn insert_object_at(
    inlines: &mut Vec<InlineNode>,
    index: u32,
    node: &InlineNode,
) -> Result<(), EditError> {
    let idx = index as usize;
    if idx > inlines.len() {
        return Err(EditError::OffsetOutOfRange);
    }
    inlines.insert(idx, node.clone());
    Ok(())
}

/// The mutable block list of the cell or SDT with id `id`, searched recursively —
/// the re-insertion target for [`Operation::InsertTable`]. `None` if not found.
fn find_container_blocks_mut(blocks: &mut [BlockNode], id: NodeId) -> Option<&mut Vec<BlockNode>> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        if cell.id == id {
                            return Some(&mut cell.blocks);
                        }
                        if let Some(found) = find_container_blocks_mut(&mut cell.blocks, id) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if sdt.id == id {
                    return Some(&mut sdt.blocks);
                }
                if let Some(found) = find_container_blocks_mut(&mut sdt.blocks, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// A table is *regular* for column edits when it has a non-empty grid, every row
/// has exactly one cell per grid column, and no cell is horizontally or vertically
/// merged. Column insert/delete on a merged table would desync the grid from the
/// cells, so those are refused (a later slice handles spans).
fn ensure_regular_table(table: &Table) -> Result<(), EditError> {
    let cols = table.grid.len();
    if cols == 0 {
        return Err(EditError::Unsupported);
    }
    for row in &table.rows {
        if row.cells.len() != cols {
            return Err(EditError::Unsupported);
        }
        for cell in &row.cells {
            if cell.properties.grid_span.is_some_and(|s| s > 1)
                || cell.properties.vertical_merge.is_some()
            {
                return Err(EditError::Unsupported);
            }
        }
    }
    Ok(())
}

/// The table with id `table`, searching the body recursively (a table can nest in
/// a cell or content control).
fn find_table_mut(blocks: &mut [BlockNode], table: NodeId) -> Option<&mut Table> {
    // First pass: a top-level table match at this level (a returned borrow in a
    // loop that also recurses trips the borrow checker, so keep the passes apart).
    if blocks
        .iter()
        .any(|b| matches!(b, BlockNode::Table(t) if t.id == table))
    {
        return blocks.iter_mut().find_map(|b| match b {
            BlockNode::Table(t) if t.id == table => Some(t),
            _ => None,
        });
    }
    // Second pass: recurse into nested tables / content controls.
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        if let Some(found) = find_table_mut(&mut cell.blocks, table) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = find_table_mut(&mut sdt.blocks, table) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// The cell with id `cell` (mutable), searching the body recursively (including
/// nested tables). Two passes so a returned borrow never collides with the
/// recursion, matching [`find_table_mut`].
fn find_cell_mut(blocks: &mut [BlockNode], cell: NodeId) -> Option<&mut TableCell> {
    // First pass: a direct cell match at any table in these blocks.
    let direct = blocks.iter().any(|b| match b {
        BlockNode::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| c.id == cell)),
        _ => false,
    });
    if direct {
        return blocks.iter_mut().find_map(|b| match b {
            BlockNode::Table(t) => t
                .rows
                .iter_mut()
                .find_map(|r| r.cells.iter_mut().find(|c| c.id == cell)),
            _ => None,
        });
    }
    // Second pass: recurse into nested cells / content controls.
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for c in &mut row.cells {
                        if let Some(found) = find_cell_mut(&mut c.blocks, cell) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = find_cell_mut(&mut sdt.blocks, cell) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// The id of the table cell that (recursively) contains paragraph `node`, and the id
/// of the innermost table it belongs to — what a host passes to
/// [`Operation::SetTableCellProperties`] / [`Operation::SetTableProperties`]. `None`
/// if the node is not inside a table cell.
#[must_use]
pub fn locate_cell(document: &Document, node: NodeId) -> Option<(NodeId, NodeId)> {
    fn walk(blocks: &[BlockNode], node: NodeId) -> Option<(NodeId, NodeId)> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if block_contains(&cell.blocks, node) {
                                return Some((table.id, cell.id));
                            }
                            if let Some(found) = walk(&cell.blocks, node) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, node) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| walk(blocks, node))
}

/// A clone of the cell `cell`'s current properties (read-only), searching the body
/// recursively — what a host reads before a modify-and-`SetTableCellProperties`
/// round-trip. `None` if no such cell exists.
#[must_use]
pub fn cell_properties(document: &Document, cell: NodeId) -> Option<TableCellProperties> {
    fn walk(blocks: &[BlockNode], cell: NodeId) -> Option<TableCellProperties> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for c in &row.cells {
                            if c.id == cell {
                                return Some(c.properties.clone());
                            }
                            if let Some(found) = walk(&c.blocks, cell) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, cell) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| walk(blocks, cell))
}

/// The table with id `table` (read-only), searching the body recursively — a host
/// query for reading a table's current rows (e.g. to place the caret after a
/// row edit). `None` if no such table exists.
#[must_use]
pub fn find_table(document: &Document, table: NodeId) -> Option<&Table> {
    fn walk(blocks: &[BlockNode], table: NodeId) -> Option<&Table> {
        for block in blocks {
            match block {
                BlockNode::Table(t) if t.id == table => return Some(t),
                BlockNode::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            if let Some(found) = walk(&cell.blocks, table) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, table) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| walk(blocks, table))
}

/// Locates the table row that (recursively) contains paragraph `node`: the table's
/// id, the 0-based row index, and a clone of that row (a host builds a matching
/// empty row from it to insert). `None` if the node is not inside a table cell.
#[must_use]
pub fn locate_table_row(document: &Document, node: NodeId) -> Option<(NodeId, u32, TableRow)> {
    fn walk(blocks: &[BlockNode], node: NodeId) -> Option<(NodeId, u32, TableRow)> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for (i, row) in table.rows.iter().enumerate() {
                        for cell in &row.cells {
                            if block_contains(&cell.blocks, node) {
                                return Some((table.id, i as u32, row.clone()));
                            }
                            // Recurse into nested tables within the cell.
                            if let Some(found) = walk(&cell.blocks, node) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, node) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| walk(blocks, node))
}

/// Whether `node` is a paragraph directly in `blocks` (not descending into nested
/// tables — the caller handles that level).
fn block_contains(blocks: &[BlockNode], node: NodeId) -> bool {
    blocks.iter().any(|b| match b {
        BlockNode::Paragraph(p) => p.id == node,
        _ => false,
    })
}

/// Locates the table cell that (recursively) contains paragraph `node`: the table's
/// id and the 0-based cell index within its row. For a regular table (the only kind
/// column edits accept) the cell index equals the grid column index. `None` if the
/// node is not inside a table cell.
#[must_use]
pub fn locate_table_cell(document: &Document, node: NodeId) -> Option<(NodeId, u32)> {
    fn walk(blocks: &[BlockNode], node: NodeId) -> Option<(NodeId, u32)> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for (ci, cell) in row.cells.iter().enumerate() {
                            if block_contains(&cell.blocks, node) {
                                return Some((table.id, ci as u32));
                            }
                            if let Some(found) = walk(&cell.blocks, node) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, node) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| walk(blocks, node))
}

/// The properties of paragraph `node` (a clone), for a host to read the current
/// alignment/spacing/… before computing a change. `None` if not a paragraph.
#[must_use]
pub fn paragraph_properties(document: &Document, node: NodeId) -> Option<ParagraphProperties> {
    // Whichever surface owns the paragraph — the body, a header or footer, a
    // note, or an inline text box. Reading only the body meant a caret in a
    // header found nothing and every caller fell back to its default, so the
    // toolbar reported a RIGHT-aligned header paragraph as left-aligned and its
    // style and font as unset. A wrong answer is worse than no answer here: it
    // invites the user to "fix" an alignment that was never wrong.
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| find_paragraph(blocks, node))
        .map(|paragraph| paragraph.properties.clone())
}

/// Ensures a run boundary exists at byte `offset` by splitting the run that
/// straddles it (the tail becomes a new run with the same properties). A no-op
/// when the offset already falls on a run boundary or outside every run.
fn ensure_run_boundary(
    inlines: &mut Vec<InlineNode>,
    offset: u32,
    ids: &mut dyn RunIds,
) -> Result<(), EditError> {
    // Find the run *strictly* containing the offset — top-level or nested inside a
    // transparent wrapper (docs/86). A boundary offset needs no split.
    let target = collect_run_paths(inlines)
        .into_iter()
        .find(|s| offset > s.start && offset < s.end)
        .map(|s| (s.path, (offset - s.start) as usize));
    let Some((path, local)) = target else {
        return Ok(());
    };
    let (idx, parent_prefix) = path.split_last().ok_or(EditError::Unsupported)?;
    let parent = vec_at_path_mut(inlines, parent_prefix).ok_or(EditError::Unsupported)?;
    let (head, tail, properties) = match &parent[*idx] {
        InlineNode::Run(run) => {
            if !run.text.is_char_boundary(local) {
                return Err(EditError::NotCharBoundary);
            }
            (
                run.text[..local].to_string(),
                run.text[local..].to_string(),
                run.properties.clone(),
            )
        }
        _ => return Ok(()),
    };
    let tail_id = ids.next().ok_or(EditError::IdExhausted)?;
    if let InlineNode::Run(run) = &mut parent[*idx] {
        run.text = head;
    }
    parent.insert(
        idx + 1,
        InlineNode::Run(Run {
            id: tail_id,
            properties,
            text: tail,
        }),
    );
    Ok(())
}

/// Whether each toggle is uniformly on across a formatted range — drives a
/// toolbar's active state and the toggle direction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FormatState {
    /// Every covered run is bold.
    pub bold: bool,
    /// Every covered run is italic.
    pub italic: bool,
    /// Every covered run is underlined.
    pub underline: bool,
    /// Every covered run is struck through.
    pub strike: bool,
}

/// The [`FormatState`] of the runs a `range` covers within one paragraph — `true`
/// for a toggle only when **every** covered run sets it (an empty range or no
/// covered runs yields all-false).
#[must_use]
pub fn format_state(document: &Document, range: Range) -> FormatState {
    if range.start.node != range.end.node || range.end.offset <= range.start.offset {
        return FormatState::default();
    }
    let covered = run_properties_in_range(document, range);
    if covered.is_empty() {
        return FormatState::default();
    }
    let all = |f: fn(&RunProperties) -> Option<bool>| covered.iter().all(|p| f(p) == Some(true));
    FormatState {
        bold: all(|p| p.bold),
        italic: all(|p| p.italic),
        underline: all(|p| p.underline),
        strike: all(|p| p.strike),
    }
}

/// The run formatting a caret at `(node, offset)` inherits — what new typing there
/// would carry. Word's rule: the run to the **left** of the caret, or (at a
/// paragraph start) the run to the right, or defaults for an empty paragraph. This
/// drives the toolbar's active state at a collapsed caret and the "type bold"
/// toggle direction, where [`format_state`] (which needs a non-empty range) returns
/// all-false.
#[must_use]
pub fn caret_format(document: &Document, node: NodeId, offset: u32) -> FormatState {
    let props = caret_run_properties(document, node, offset)
        .cloned()
        .unwrap_or_default();
    let on = |v: Option<bool>| v == Some(true);
    FormatState {
        bold: on(props.bold),
        italic: on(props.italic),
        underline: on(props.underline),
        strike: on(props.strike),
    }
}

/// The size / color / font / super-sub a caret at `(node, offset)` inherits — the
/// caret counterpart to [`run_style_state`], so a collapsed caret reflects (and can
/// arm) the same run styling a selection does. Defaults when the paragraph is empty.
#[must_use]
pub fn caret_run_style(document: &Document, node: NodeId, offset: u32) -> RunStyleState {
    let Some(props) = caret_run_properties(document, node, offset) else {
        return RunStyleState::default();
    };
    RunStyleState {
        size_half_points: props.size_half_points,
        color_rgb: match props.color {
            Some(Color::Rgb(c)) => Some(c),
            _ => None,
        },
        font: match &props.font_ref {
            Some(FontRef::Named(name)) => Some(name.name.clone()),
            _ => None,
        },
        superscript: props.vertical_alignment == Some(VerticalAlignment::Superscript),
        subscript: props.vertical_alignment == Some(VerticalAlignment::Subscript),
    }
}

/// The run properties a caret at `(node, offset)` inherits — the run to its left
/// (Word's rule), else the run to its right at a paragraph start, else the first
/// run; `None` for an empty paragraph. The shared basis of the caret-format and
/// caret-style queries (what new typing there would carry).
#[must_use]
pub fn caret_run_properties(
    document: &Document,
    node: NodeId,
    offset: u32,
) -> Option<&RunProperties> {
    let para = find_paragraph(document.body(), node)?;
    // Flatten across final-with-markup wrappers so a caret resting inside a
    // pending tracked revision reflects that run's formatting (docs/81
    // REVIEW-GAP-030), not the paragraph default.
    let segs = flatten_run_segments(&para.inlines);
    segs.iter()
        .find(|s| offset > s.start && offset <= s.end)
        .or_else(|| segs.iter().find(|s| offset >= s.start && offset < s.end))
        .or_else(|| segs.first())
        .map(|s| s.properties)
}

/// The uniform run styling across a range — each field is `Some`/`true` only when
/// **every** covered run shares that value, so a toolbar can show the current
/// size/color/font (or blank for a mixed selection).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunStyleState {
    /// Common font size in half-points, if uniform.
    pub size_half_points: Option<u32>,
    /// Common RGB text color, if uniform (theme colors count as mixed).
    pub color_rgb: Option<RgbColor>,
    /// Common font family, if uniform.
    pub font: Option<String>,
    /// Every covered run is superscript.
    pub superscript: bool,
    /// Every covered run is subscript.
    pub subscript: bool,
}

/// The [`RunStyleState`] of the runs a `range` covers within one paragraph.
#[must_use]
pub fn run_style_state(document: &Document, range: Range) -> RunStyleState {
    if range.start.node != range.end.node || range.end.offset <= range.start.offset {
        return RunStyleState::default();
    }
    let covered = run_properties_in_range(document, range);
    if covered.is_empty() {
        return RunStyleState::default();
    }
    RunStyleState {
        size_half_points: uniform(&covered, |p| p.size_half_points),
        color_rgb: uniform(&covered, |p| match p.color {
            Some(Color::Rgb(c)) => Some(c),
            _ => None,
        }),
        font: uniform(&covered, |p| match &p.font_ref {
            Some(FontRef::Named(name)) => Some(name.name.clone()),
            _ => None,
        }),
        superscript: covered
            .iter()
            .all(|p| p.vertical_alignment == Some(VerticalAlignment::Superscript)),
        subscript: covered
            .iter()
            .all(|p| p.vertical_alignment == Some(VerticalAlignment::Subscript)),
    }
}

/// The direct run properties covered by a non-empty range within one paragraph.
///
/// This is intentionally a direct-property query: hosts that need the effective
/// value visible to a user must pass each result through the document style
/// cascade. Returning references keeps the editing crate independent of layout
/// while avoiding another run-segmentation implementation at bridge layers.
#[must_use]
pub fn run_properties_in_range(document: &Document, range: Range) -> Vec<&RunProperties> {
    if range.start.node != range.end.node || range.end.offset <= range.start.offset {
        return Vec::new();
    }
    let Some(para) = find_paragraph(document.body(), range.start.node) else {
        return Vec::new();
    };
    // Descend into final-with-markup-contributing wrappers so a selection that
    // touches a run inside a pending tracked revision (or hyperlink/SDT)
    // reflects that run's real formatting, matching the copy/layout projections
    // (docs/81 REVIEW-GAP-030). The editing/split paths keep using
    // `run_segments` (top-level runs only) because revision-aware splitting is
    // separate work (REVIEW-GAP-007).
    flatten_run_segments(&para.inlines)
        .into_iter()
        .filter(|s| s.end > range.start.offset && s.start < range.end.offset && s.start < s.end)
        .map(|s| s.properties)
        .collect()
}

/// The common value of `f` across all covered runs, or `None` if any run differs
/// or leaves it unset.
fn uniform<T: PartialEq>(
    covered: &[&RunProperties],
    f: impl Fn(&RunProperties) -> Option<T>,
) -> Option<T> {
    let first = f(covered[0])?;
    covered
        .iter()
        .skip(1)
        .all(|p| f(p).as_ref() == Some(&first))
        .then_some(first)
}

/// Finds the paragraph with `id` (immutable), recursing into tables and content
/// controls.
#[must_use]
/// Which of the document's block surfaces a node lives in.
///
/// Header, footer and note content is ordinary block content in the SAME id
/// space as the body — `Document::validate` records their block ids into one
/// document-wide uniqueness set — so `Pos { node, offset }` already addresses a
/// position inside any of them. This is the same property LibreOffice Writer
/// relies on: its header, footer, footnote and floating-frame text sit in one
/// node array beside body text, and a cursor is a node index, so nothing has to
/// carry "which sub-document am I in".
///
/// So this is derived on demand rather than threaded through op signatures: the
/// closed op set (doc 45, invariant I2) keeps addressing positions by `NodeId`,
/// and resolution finds the surface that id belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Surface {
    /// The document body.
    Body,
    /// A header definition's content.
    Header(HeaderFooterId),
    /// A footer definition's content.
    Footer(HeaderFooterId),
    /// A footnote definition's content.
    Footnote(NoteId),
    /// An endnote definition's content.
    Endnote(NoteId),
}

/// Every block surface in the document, body first.
///
/// The read helpers below all used to walk `document.body()` alone, so a caret
/// in a header, footer, note or text box found nothing and each caller fell back
/// to its default — that is how a RIGHT-aligned header paragraph reported itself
/// as left-aligned. They are listed once here rather than each growing its own
/// copy of the traversal, because the same omission was made independently four
/// times.
fn surface_block_lists(document: &Document) -> Vec<&[BlockNode]> {
    let definitions = document.definitions();
    let mut out: Vec<&[BlockNode]> = vec![document.body()];
    for (_, header) in definitions.headers.iter() {
        out.push(&header.blocks);
    }
    for (_, footer) in definitions.footers.iter() {
        out.push(&footer.blocks);
    }
    for (_, note) in definitions.footnotes.iter() {
        out.push(&note.blocks);
    }
    for (_, note) in definitions.endnotes.iter() {
        out.push(&note.blocks);
    }
    out
}

/// Whether `blocks` (recursively) contains a paragraph or table with `id`.
fn blocks_contain(blocks: &[BlockNode], id: NodeId) -> bool {
    if find_paragraph(blocks, id).is_some() {
        return true;
    }
    blocks.iter().any(|block| match block {
        BlockNode::Table(table) => {
            table.id == id
                || table.rows.iter().any(|row| {
                    row.cells
                        .iter()
                        .any(|cell| blocks_contain(&cell.blocks, id))
                })
        }
        BlockNode::Sdt(sdt) => blocks_contain(&sdt.blocks, id),
        BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => false,
    })
}

/// The surface holding `id`, searched body-first because that is the
/// overwhelmingly common case. `None` means no surface owns the id.
#[must_use]
pub fn surface_of(doc: &Document, id: NodeId) -> Option<Surface> {
    if blocks_contain(doc.body(), id) {
        return Some(Surface::Body);
    }
    let definitions = doc.definitions();
    for (key, header) in definitions.headers.iter() {
        if blocks_contain(&header.blocks, id) {
            return Some(Surface::Header(*key));
        }
    }
    for (key, footer) in definitions.footers.iter() {
        if blocks_contain(&footer.blocks, id) {
            return Some(Surface::Footer(*key));
        }
    }
    for (key, note) in definitions.footnotes.iter() {
        if blocks_contain(&note.blocks, id) {
            return Some(Surface::Footnote(*key));
        }
    }
    for (key, note) in definitions.endnotes.iter() {
        if blocks_contain(&note.blocks, id) {
            return Some(Surface::Endnote(*key));
        }
    }
    None
}

/// The block list for a surface, for mutation. Split from [`surface_of`] so the
/// immutable search and the mutable borrow never overlap.
fn surface_blocks_mut<'a>(
    doc: &'a mut Document,
    surface: &Surface,
) -> Option<&'a mut Vec<BlockNode>> {
    match surface {
        Surface::Body => Some(doc.body_mut()),
        Surface::Header(key) => doc
            .definitions_mut()
            .headers
            .get_mut(key)
            .map(|header| &mut header.blocks),
        Surface::Footer(key) => doc
            .definitions_mut()
            .footers
            .get_mut(key)
            .map(|footer| &mut footer.blocks),
        Surface::Footnote(key) => doc
            .definitions_mut()
            .footnotes
            .get_mut(key)
            .map(|note| &mut note.blocks),
        Surface::Endnote(key) => doc
            .definitions_mut()
            .endnotes
            .get_mut(key)
            .map(|note| &mut note.blocks),
    }
}

/// The block list owning `id`, wherever it lives — the body-agnostic replacement
/// for `doc.body_mut()` in ops that address a position by node.
/// The paragraph `id`, wherever it lives — the immutable counterpart of
/// [`blocks_owning_mut`], for the op arms that snapshot a paragraph before
/// mutating it. Reading only the body meant those ops refused outright in a
/// header, footer or note.
fn find_paragraph_any(doc: &Document, id: NodeId) -> Option<&Paragraph> {
    surface_block_lists(doc)
        .into_iter()
        .find_map(|blocks| find_paragraph(blocks, id))
}

fn blocks_owning_mut(doc: &mut Document, id: NodeId) -> Result<&mut Vec<BlockNode>, EditError> {
    let surface = surface_of(doc, id).ok_or(EditError::NodeNotFound)?;
    surface_blocks_mut(doc, &surface).ok_or(EditError::NodeNotFound)
}

/// Searches an inline list for a paragraph inside a text box, recursing through
/// the wrappers that can hold one: hyperlinks, fields, shape groups, and text
/// boxes themselves (a text box may contain a table whose cells hold more).
/// Searches a shape group's children (recursing into nested groups) for a
/// paragraph inside one of their text boxes.
fn find_paragraph_in_group(children: &[GroupChild], id: NodeId) -> Option<&Paragraph> {
    for child in children {
        let found = match child {
            GroupChild::TextBox(text_box) => find_paragraph(&text_box.blocks, id),
            GroupChild::Group(nested) => find_paragraph_in_group(&nested.children, id),
            GroupChild::Picture(_) | GroupChild::Shape(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// The mutable twin of [`find_paragraph_in_group`].
fn find_paragraph_in_group_mut(children: &mut [GroupChild], id: NodeId) -> Option<&mut Paragraph> {
    for child in children {
        let found = match child {
            GroupChild::TextBox(text_box) => find_paragraph_mut(&mut text_box.blocks, id),
            GroupChild::Group(nested) => find_paragraph_in_group_mut(&mut nested.children, id),
            GroupChild::Picture(_) | GroupChild::Shape(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn find_paragraph_in_inlines(inlines: &[InlineNode], id: NodeId) -> Option<&Paragraph> {
    for inline in inlines {
        let found = match inline {
            InlineNode::TextBox(text_box) => find_paragraph(&text_box.blocks, id),
            InlineNode::Hyperlink(link) => find_paragraph_in_inlines(&link.inlines, id),
            InlineNode::Field(field) => find_paragraph_in_inlines(&field.inlines, id),
            // A shape GROUP can hold text boxes, and groups nest, so a paragraph
            // can be arbitrarily deep inside one. Their ids are in the same
            // document-wide space (`record_group_ids`), so only the walk was
            // missing.
            InlineNode::Group(group) => find_paragraph_in_group(&group.children, id),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

pub fn find_paragraph(blocks: &[BlockNode], id: NodeId) -> Option<&Paragraph> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(p) if p.id == id => return Some(p),
            // A paragraph can also live INSIDE a paragraph, in an inline text
            // box (or one nested in a shape group). Its ids are in the same
            // document-wide space as every other block's — `record_inline_ids`
            // puts them there — so a position inside one is an ordinary
            // `Pos { node, offset }` and resolution just has to look.
            BlockNode::Paragraph(paragraph) => {
                if let Some(found) = find_paragraph_in_inlines(&paragraph.inlines, id) {
                    return Some(found);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(p) = find_paragraph(&cell.blocks, id) {
                            return Some(p);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(p) = find_paragraph(&sdt.blocks, id) {
                    return Some(p);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// A source of fresh run identities. Backed by
/// [`IdGenerator`](casual_doc_model::IdGenerator) in practice; a trait so the
/// edit crate does not dictate the id-allocation policy.
pub trait RunIds {
    /// Returns a fresh, unique node id, or `None` if the space is exhausted.
    fn next(&mut self) -> Option<NodeId>;
}

impl RunIds for casual_doc_model::IdGenerator {
    fn next(&mut self) -> Option<NodeId> {
        self.next_id().ok()
    }
}

/// The byte range of one top-level [`InlineNode::Run`] in a paragraph's text.
struct RunSeg {
    /// Index into the paragraph's `inlines`.
    idx: usize,
    /// Byte offset of the run's first byte in the concatenated text.
    start: u32,
    /// Byte offset one past the run's last byte.
    end: u32,
}

/// The text bytes a single inline contributes — identical to
/// `node_plain_text`'s accounting, so offsets align with hit-testing.
fn inline_text_len(inline: &InlineNode) -> u32 {
    match inline {
        InlineNode::Run(run) => run.text.len() as u32,
        InlineNode::Tab(_) => 1,
        InlineNode::Symbol(symbol) => {
            char::from_u32(symbol.char).map_or(0, |c| c.len_utf8() as u32)
        }
        InlineNode::Hyperlink(hyperlink) => nested_len(&hyperlink.inlines),
        InlineNode::Revision(revision)
            if revision
                .kind
                .contributes_to(ReviewProjection::FinalWithMarkup) =>
        {
            nested_len(&revision.inlines)
        }
        InlineNode::Revision(_) => 0,
        InlineNode::Sdt(sdt) => nested_len(&sdt.inlines),
        _ => 0,
    }
}

fn nested_len(inlines: &[InlineNode]) -> u32 {
    inlines.iter().map(inline_text_len).sum()
}

/// The paragraph's total shaped-text byte length.
fn paragraph_text_len(para: &Paragraph) -> u32 {
    para.inlines.iter().map(inline_text_len).sum()
}

/// One run's projected byte range and its direct properties, flattened across
/// the final-with-markup-contributing wrappers so a run inside a pending tracked
/// revision (or hyperlink/SDT) is visible to read-only reflection queries
/// (docs/81 REVIEW-GAP-030). Distinct from [`RunSeg`], which the editing/split
/// paths use for top-level runs only.
struct FlatRun<'a> {
    /// Byte offset of the run's first byte in the projected paragraph text.
    start: u32,
    /// Byte offset one past the run's last byte.
    end: u32,
    /// The run's direct properties.
    properties: &'a RunProperties,
}

/// Every run in projected order — top-level and nested inside
/// final-with-markup-contributing `Revision`/`Hyperlink`/`Sdt` wrappers — with
/// cumulative byte offsets aligned to [`inline_text_len`], mirroring the copy
/// path's `walk_inlines_rich`. Used by the reflection/caret-property queries so
/// formatting inside a pending suggestion is not silently dropped.
fn flatten_run_segments(inlines: &[InlineNode]) -> Vec<FlatRun<'_>> {
    let mut out = Vec::new();
    let mut cum = 0u32;
    push_run_segments(inlines, &mut cum, &mut out);
    out
}

fn push_run_segments<'a>(inlines: &'a [InlineNode], cum: &mut u32, out: &mut Vec<FlatRun<'a>>) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                let start = *cum;
                let end = start.saturating_add(run.text.len() as u32);
                *cum = end;
                out.push(FlatRun {
                    start,
                    end,
                    properties: &run.properties,
                });
            }
            InlineNode::Hyperlink(link) => push_run_segments(&link.inlines, cum, out),
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup) =>
            {
                push_run_segments(&revision.inlines, cum, out);
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => push_run_segments(&sdt.inlines, cum, out),
            other => {
                *cum = cum.saturating_add(inline_text_len(other));
            }
        }
    }
}

/// The byte ranges of the paragraph's top-level runs (the editable segments).
fn run_segments(inlines: &[InlineNode]) -> Vec<RunSeg> {
    let mut segs = Vec::new();
    let mut cum = 0u32;
    for (idx, inline) in inlines.iter().enumerate() {
        let len = inline_text_len(inline);
        if matches!(inline, InlineNode::Run(_)) {
            segs.push(RunSeg {
                idx,
                start: cum,
                end: cum + len,
            });
        }
        cum += len;
    }
    segs
}

/// A run leaf located by its index path from a paragraph's top-level `inlines`,
/// descending through editing-transparent wrappers. `path[0]` indexes the
/// paragraph's inlines; each later element indexes that wrapper's `inlines`.
/// The mutation-capable counterpart of [`FlatRun`] (docs/86 REVIEW-GAP-007):
/// where `flatten_run_segments` yields `&properties` for read-only reflection,
/// this yields the path so the editing/split paths can descend into a run
/// nested inside a `Revision`/`Hyperlink`/`Sdt` wrapper and mutate it.
struct RunPathSeg {
    path: Vec<usize>,
    /// Byte offset of the run's first byte in the projected paragraph text.
    start: u32,
    /// Byte offset one past the run's last byte.
    end: u32,
}

/// Whether an inline is an editing-transparent wrapper whose `inlines` the
/// split/edit paths may descend into — the same set `push_run_segments`
/// flattens: hyperlinks, inline SDTs, and final-with-markup-contributing
/// revisions (a pending `Insertion`/`MoveTo`). A non-contributing revision
/// (`Deletion`/`MoveFrom`) is zero active width, so no offset resolves into it.
fn transparent_children(inline: &InlineNode) -> Option<&[InlineNode]> {
    match inline {
        InlineNode::Hyperlink(link) => Some(&link.inlines),
        InlineNode::Revision(revision)
            if revision
                .kind
                .contributes_to(ReviewProjection::FinalWithMarkup) =>
        {
            Some(&revision.inlines)
        }
        InlineNode::Sdt(sdt) => Some(&sdt.inlines),
        _ => None,
    }
}

fn transparent_children_mut(inline: &mut InlineNode) -> Option<&mut Vec<InlineNode>> {
    match inline {
        InlineNode::Hyperlink(link) => Some(&mut link.inlines),
        InlineNode::Revision(revision)
            if revision
                .kind
                .contributes_to(ReviewProjection::FinalWithMarkup) =>
        {
            Some(&mut revision.inlines)
        }
        InlineNode::Sdt(sdt) => Some(&mut sdt.inlines),
        _ => None,
    }
}

/// Every run in projected order — top-level and nested inside editing-transparent
/// wrappers — with its index path and cumulative byte range. The write-side
/// mirror of [`flatten_run_segments`]; offsets use the same [`inline_text_len`]
/// accounting so they align with hit-testing and the read-side queries.
fn collect_run_paths(inlines: &[InlineNode]) -> Vec<RunPathSeg> {
    let mut out = Vec::new();
    let mut prefix = Vec::new();
    let mut cum = 0u32;
    push_run_paths(inlines, &mut prefix, &mut cum, &mut out);
    out
}

fn push_run_paths(
    inlines: &[InlineNode],
    prefix: &mut Vec<usize>,
    cum: &mut u32,
    out: &mut Vec<RunPathSeg>,
) {
    for (idx, inline) in inlines.iter().enumerate() {
        prefix.push(idx);
        match inline {
            InlineNode::Run(run) => {
                let start = *cum;
                let end = start.saturating_add(run.text.len() as u32);
                *cum = end;
                out.push(RunPathSeg {
                    path: prefix.clone(),
                    start,
                    end,
                });
            }
            _ => {
                if let Some(children) = transparent_children(inline) {
                    push_run_paths(children, prefix, cum, out);
                } else {
                    *cum = cum.saturating_add(inline_text_len(inline));
                }
            }
        }
        prefix.pop();
    }
}

/// The `Vec<InlineNode>` addressed by `path` — the empty path is the top-level
/// `inlines`; otherwise the `inlines` of the wrapper reached by following `path`.
/// Returns `None` if the path leaves the transparent-wrapper set.
fn vec_at_path_mut<'a>(
    inlines: &'a mut Vec<InlineNode>,
    path: &[usize],
) -> Option<&'a mut Vec<InlineNode>> {
    let Some((first, rest)) = path.split_first() else {
        return Some(inlines);
    };
    let children = transparent_children_mut(inlines.get_mut(*first)?)?;
    vec_at_path_mut(children, rest)
}

/// The mutable `Run` at `path` (its last element indexes the run within its
/// parent wrapper's `inlines`), or `None` if the path does not end at a run.
fn run_at_path_mut<'a>(inlines: &'a mut Vec<InlineNode>, path: &[usize]) -> Option<&'a mut Run> {
    let (idx, parent) = path.split_last()?;
    let parent_vec = vec_at_path_mut(inlines, parent)?;
    match parent_vec.get_mut(*idx)? {
        InlineNode::Run(run) => Some(run),
        _ => None,
    }
}

fn valid_hyperlink_values(target: Option<&HyperlinkTarget>, tooltip: Option<&str>) -> bool {
    let target_valid = target.is_none_or(|target| match target {
        HyperlinkTarget::External(external) => {
            !external.url.is_empty() && external.url.len() <= 2048
        }
        HyperlinkTarget::Internal(internal) => {
            !internal.anchor.is_empty() && internal.anchor.len() <= 255
        }
    });
    let tooltip_valid = tooltip.is_none_or(|value| !value.is_empty() && value.len() <= 255);
    target_valid && tooltip_valid
}

/// Returns the top-level hyperlink whose cumulative text range exactly matches
/// `[start, end)`.
fn exact_hyperlink_index(inlines: &[InlineNode], start: u32, end: u32) -> Option<usize> {
    let mut offset = 0u32;
    for (index, inline) in inlines.iter().enumerate() {
        let next = offset.saturating_add(inline_text_len(inline));
        if matches!(inline, InlineNode::Hyperlink(_)) && offset == start && next == end {
            return Some(index);
        }
        offset = next;
    }
    None
}

/// Returns the contiguous top-level inline indices exactly covered by
/// `[start, end)`. A partial overlap with a non-run wrapper is rejected. Used by
/// hyperlink creation, which wraps a contiguous top-level run span and (by
/// design) refuses a selection that cuts through an existing wrapper.
fn covered_top_level_indices(
    inlines: &[InlineNode],
    start: u32,
    end: u32,
) -> Result<Vec<usize>, EditError> {
    let mut covered = Vec::new();
    let mut offset = 0u32;
    for (index, inline) in inlines.iter().enumerate() {
        let len = inline_text_len(inline);
        let next = offset.saturating_add(len);
        if len > 0 && offset < end && next > start {
            if offset < start || next > end {
                return Err(EditError::Unsupported);
            }
            covered.push(index);
        }
        offset = next;
    }
    Ok(covered)
}

/// The index paths of every run — top-level or nested in a transparent wrapper —
/// that lies fully within `[start, end)`. The deep counterpart of the old
/// top-level-only cover query (docs/86); the caller aligns run boundaries first,
/// so a run is either fully inside or fully outside.
fn covered_run_paths(inlines: &[InlineNode], start: u32, end: u32) -> Vec<Vec<usize>> {
    collect_run_paths(inlines)
        .into_iter()
        .filter(|s| s.start >= start && s.end <= end)
        .map(|s| s.path)
        .collect()
}

/// Rejects a range that partially cuts an *atomic* leaf that cannot be split — a
/// tab or symbol whose interior a boundary would fall inside. Transparent
/// wrappers are exempt: they are descended into, not split at this level.
fn reject_partial_atomic(inlines: &[InlineNode], start: u32, end: u32) -> Result<(), EditError> {
    let mut offset = 0u32;
    for inline in inlines {
        let len = inline_text_len(inline);
        let next = offset.saturating_add(len);
        let straddles =
            len > 0 && offset < end && next > start && !(offset >= start && next <= end);
        if straddles {
            match inline {
                InlineNode::Run(_) => {}
                _ if transparent_children(inline).is_some() => {}
                _ => return Err(EditError::Unsupported),
            }
        }
        offset = next;
    }
    Ok(())
}

/// Inserts `text` at `offset` into a paragraph's inlines, splicing into the run
/// the offset lands in (or the nearest run, or a new run for an empty paragraph).
fn insert_text(
    inlines: &mut Vec<InlineNode>,
    offset: u32,
    text: &str,
    ids: &mut dyn RunIds,
) -> Result<(), EditError> {
    // Strict interior first, descending into wrappers: an offset *inside* a run
    // nested in a pending insertion / hyperlink / SDT splices into that run, so
    // typing inside a suggestion lands there instead of appending a stray
    // default-property run at the paragraph end (docs/86 REVIEW-GAP-007). A
    // boundary offset falls through to the top-level logic below, which keeps
    // today's behaviour of not silently entering an adjacent wrapper.
    if let Some(seg) = collect_run_paths(inlines)
        .into_iter()
        .find(|s| offset > s.start && offset < s.end)
    {
        let local = (offset - seg.start) as usize;
        let run = run_at_path_mut(inlines, &seg.path).ok_or(EditError::Unsupported)?;
        if !run.text.is_char_boundary(local) {
            return Err(EditError::NotCharBoundary);
        }
        run.text.insert_str(local, text);
        return Ok(());
    }

    let segs = run_segments(inlines);

    // The run whose range contains the offset (interior or either boundary).
    if let Some(seg) = segs.iter().find(|s| offset >= s.start && offset <= s.end) {
        let local = (offset - seg.start) as usize;
        if let InlineNode::Run(run) = &mut inlines[seg.idx] {
            if !run.text.is_char_boundary(local) {
                return Err(EditError::NotCharBoundary);
            }
            run.text.insert_str(local, text);
            return Ok(());
        }
    }
    // Offset sits exactly at a non-run boundary (e.g. right after/before a
    // hyperlink or tab): extend the run truly touching that edge, not merely
    // the nearest one — `<=`/`>=` here would let the insert jump *across* an
    // intervening non-run inline (a trailing hyperlink, say) into a run that
    // only looks "nearest" by position, silently absorbing new text into the
    // wrong (and wrongly-formatted) run.
    if let Some(seg) = segs.iter().find(|s| s.end == offset)
        && let InlineNode::Run(run) = &mut inlines[seg.idx]
    {
        run.text.push_str(text);
        return Ok(());
    }
    if let Some(seg) = segs.iter().find(|s| s.start == offset)
        && let InlineNode::Run(run) = &mut inlines[seg.idx]
    {
        run.text.insert_str(0, text);
        return Ok(());
    }
    // …else no run touches `offset` at all (it sits at the edge of a non-run
    // inline with no adjacent run — e.g. a paragraph ending in a hyperlink, or
    // an empty paragraph): insert a fresh run at the matching top-level
    // position, not always at the front.
    let mut cum = 0u32;
    let mut insert_at = inlines.len();
    for (idx, inline) in inlines.iter().enumerate() {
        if cum == offset {
            insert_at = idx;
            break;
        }
        cum += inline_text_len(inline);
    }
    let id = ids.next().ok_or(EditError::IdExhausted)?;
    inlines.insert(
        insert_at,
        InlineNode::Run(Run {
            id,
            properties: RunProperties::default(),
            text: text.to_string(),
        }),
    );
    Ok(())
}

/// Deletes `[start, end)` when it lies within a single top-level run, returning
/// `Some(removed_text)`. Returns `None` when the range spans more than one run (or
/// a non-run inline), so the caller falls to the general multi-run path; a
/// mid-character offset is still a hard `NotCharBoundary` error.
fn delete_text(
    inlines: &mut [InlineNode],
    start: u32,
    end: u32,
) -> Result<Option<String>, EditError> {
    let segs = run_segments(inlines);
    let Some(seg) = segs.iter().find(|s| start >= s.start && end <= s.end) else {
        return Ok(None);
    };
    // Deleting the whole run would remove it and could leave its neighbours adjacent
    // (and possibly equal-propertied, which the model forbids). Defer that to the
    // general path — its `SetInlines` inverse stays exact through the coalescing.
    if start == seg.start && end == seg.end {
        return Ok(None);
    }
    let (from, to) = ((start - seg.start) as usize, (end - seg.start) as usize);
    let idx = seg.idx;

    let InlineNode::Run(run) = &mut inlines[idx] else {
        return Ok(None);
    };
    if !run.text.is_char_boundary(from) || !run.text.is_char_boundary(to) {
        return Err(EditError::NotCharBoundary);
    }
    let removed = run.text[from..to].to_string();
    run.text.replace_range(from..to, "");
    // The run keeps text (full-run deletion bailed above), so no neighbours merge
    // and the plain-text `InsertText` inverse is exact.
    Ok(Some(removed))
}

/// Merges adjacent top-level runs with equal properties into one. The model forbids
/// adjacent equal-property runs, and a delete that drops the content separating two
/// such runs (or empties a run between them) would otherwise leave them adjacent —
/// so the delete path coalesces before returning. Text and total length are
/// unchanged, so byte offsets are preserved; the merged run keeps the first's id.
fn coalesce_adjacent_runs(inlines: &mut Vec<InlineNode>) {
    let mut i = 0;
    while i + 1 < inlines.len() {
        let mergeable = matches!(
            (&inlines[i], &inlines[i + 1]),
            (InlineNode::Run(a), InlineNode::Run(b)) if a.properties == b.properties
        );
        if mergeable {
            let InlineNode::Run(next) = inlines.remove(i + 1) else {
                unreachable!("matched a run above");
            };
            if let InlineNode::Run(cur) = &mut inlines[i] {
                cur.text.push_str(&next.text);
            }
        } else {
            i += 1;
        }
    }
    // Recurse into transparent wrappers so runs left adjacent *inside* a
    // hyperlink/revision/SDT by a nested edit are merged too. Merging never
    // crosses a wrapper boundary (a wrapper between two runs is not a run), so
    // distinct suggestions and links stay distinct (docs/86 REVIEW-GAP-007).
    for inline in inlines.iter_mut() {
        if let Some(children) = transparent_children_mut(inline) {
            coalesce_adjacent_runs(children);
        }
    }
}

/// Removes every run that lies fully inside `[start, end)`, by cumulative text
/// length. The caller runs [`ensure_run_boundary`] at both ends first, so every
/// **run** is then wholly inside or wholly outside the range. A transparent
/// wrapper (hyperlink, contributing revision, SDT) only *partially* covered is
/// descended into — its covered runs are removed and the wrapper is pruned if it
/// empties — so a delete spanning pending and normal text works (docs/86
/// REVIEW-GAP-007). A partially-covered *atomic* leaf (tab, symbol) still can't
/// be split, so it is refused (`Unsupported`) rather than silently mis-deleted.
fn remove_covered_range(
    inlines: &mut Vec<InlineNode>,
    start: u32,
    end: u32,
) -> Result<(), EditError> {
    remove_covered_in(inlines, start, end, &mut 0u32)
}

fn remove_covered_in(
    inlines: &mut Vec<InlineNode>,
    start: u32,
    end: u32,
    cum: &mut u32,
) -> Result<(), EditError> {
    let mut i = 0;
    while i < inlines.len() {
        let base = *cum;
        let len = inline_text_len(&inlines[i]);
        let (s, e) = (base, base.saturating_add(len));
        // Advance the cumulative cursor by the *original* projected length so a
        // removal inside this inline does not shift the offsets of its siblings
        // (`start`/`end` stay in original-projection coordinates for this pass).
        *cum = e;
        if len == 0 {
            i += 1;
            continue; // zero-width markup (e.g. a pending deletion) — untouched.
        }
        if s >= start && e <= end {
            inlines.remove(i);
            continue; // fully covered — drop; next inline shifts into index `i`.
        }
        if s < end && e > start {
            // Partial overlap. Runs were boundary-split by the caller, so this is
            // a transparent wrapper we descend into; an atomic leaf here cannot be
            // split and is refused.
            let Some(children) = transparent_children_mut(&mut inlines[i]) else {
                return Err(EditError::Unsupported);
            };
            let mut child_cum = base;
            remove_covered_in(children, start, end, &mut child_cum)?;
            let now_empty = transparent_children(&inlines[i]).is_some_and(<[_]>::is_empty);
            if now_empty {
                inlines.remove(i);
                continue; // wrapper emptied — prune (wrapper inlines stay non-empty).
            }
        }
        i += 1;
    }
    Ok(())
}

/// Splits the paragraph `id` at byte `offset` into two, inserting the trailing
/// half as a new paragraph `new_id` immediately after. Recurses into tables and
/// content controls to find the paragraph's container. Returns whether it was
/// found and split.
fn split_paragraph(
    blocks: &mut Vec<BlockNode>,
    id: NodeId,
    offset: u32,
    new_id: NodeId,
    ids: &mut dyn RunIds,
) -> Result<bool, EditError> {
    if let Some(index) = blocks
        .iter()
        .position(|b| matches!(b, BlockNode::Paragraph(p) if p.id == id))
    {
        let BlockNode::Paragraph(para) = &mut blocks[index] else {
            unreachable!("index selected a paragraph");
        };
        if offset > paragraph_text_len(para) {
            return Err(EditError::OffsetOutOfRange);
        }
        let inlines = std::mem::take(&mut para.inlines);
        let (left, right) = split_inlines(inlines, offset, ids)?;
        let properties = para.properties.clone();
        para.inlines = left;
        blocks.insert(
            index + 1,
            BlockNode::Paragraph(Paragraph {
                id: new_id,
                properties,
                inlines: right,
            }),
        );
        return Ok(true);
    }
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if split_paragraph(&mut cell.blocks, id, offset, new_id, ids)? {
                            return Ok(true);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if split_paragraph(&mut sdt.blocks, id, offset, new_id, ids)? {
                    return Ok(true);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

/// Joins `second` (which must immediately follow `first`) into `first`, removing
/// `second`. Returns the byte offset at which `first` ended before the join (the
/// inverse split point), or `None` if `first` was not found. `Err(Unsupported)`
/// if `second` is not the adjacent next paragraph.
fn join_paragraphs(
    blocks: &mut Vec<BlockNode>,
    first: NodeId,
    second: NodeId,
) -> Result<Option<u32>, EditError> {
    if let Some(i) = blocks
        .iter()
        .position(|b| matches!(b, BlockNode::Paragraph(p) if p.id == first))
    {
        if !matches!(blocks.get(i + 1), Some(BlockNode::Paragraph(p)) if p.id == second) {
            return Err(EditError::Unsupported);
        }
        let BlockNode::Paragraph(second_para) = blocks.remove(i + 1) else {
            unreachable!("checked it is a paragraph");
        };
        let BlockNode::Paragraph(first_para) = &mut blocks[i] else {
            unreachable!("position found a paragraph");
        };
        let split_at = paragraph_text_len(first_para);
        first_para.inlines.extend(second_para.inlines);
        return Ok(Some(split_at));
    }
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(at) = join_paragraphs(&mut cell.blocks, first, second)? {
                            return Ok(Some(at));
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(at) = join_paragraphs(&mut sdt.blocks, first, second)? {
                    return Ok(Some(at));
                }
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Splits a paragraph's inline content at byte `offset` into (left, right). A run
/// straddling the offset is split (the right half gets a fresh id). A transparent
/// wrapper straddling the offset is split recursively into two wrappers, one per
/// side, each keeping the wrapper's identity and the trailing half taking a fresh
/// id — so pressing Enter inside a pending suggestion, a hyperlink, or an SDT
/// splits it across the two paragraphs instead of failing (docs/86
/// REVIEW-GAP-007). At a boundary the inline goes wholly to one side. An atomic
/// leaf cannot straddle a char-aligned offset, so it remains `Unsupported`.
fn split_inlines(
    inlines: Vec<InlineNode>,
    offset: u32,
    ids: &mut dyn RunIds,
) -> Result<(Vec<InlineNode>, Vec<InlineNode>), EditError> {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut cum = 0u32;
    for inline in inlines {
        let len = inline_text_len(&inline);
        if cum >= offset {
            right.push(inline);
        } else if cum + len <= offset {
            left.push(inline);
        } else {
            let local = offset - cum;
            match inline {
                InlineNode::Run(run) => {
                    let at = local as usize;
                    if !run.text.is_char_boundary(at) {
                        return Err(EditError::NotCharBoundary);
                    }
                    let (head, tail) = run.text.split_at(at);
                    left.push(InlineNode::Run(Run {
                        id: run.id,
                        properties: run.properties.clone(),
                        text: head.to_string(),
                    }));
                    let tail_id = ids.next().ok_or(EditError::IdExhausted)?;
                    right.push(InlineNode::Run(Run {
                        id: tail_id,
                        properties: run.properties,
                        text: tail.to_string(),
                    }));
                }
                InlineNode::Hyperlink(mut link) => {
                    let (lc, rc) = split_inlines(std::mem::take(&mut link.inlines), local, ids)?;
                    let mut tail = link.clone();
                    if !lc.is_empty() {
                        link.inlines = lc;
                        left.push(InlineNode::Hyperlink(link));
                    }
                    if !rc.is_empty() {
                        tail.id = ids.next().ok_or(EditError::IdExhausted)?;
                        tail.inlines = rc;
                        right.push(InlineNode::Hyperlink(tail));
                    }
                }
                InlineNode::Revision(mut revision) => {
                    let (lc, rc) =
                        split_inlines(std::mem::take(&mut revision.inlines), local, ids)?;
                    let mut tail = revision.clone();
                    if !lc.is_empty() {
                        revision.inlines = lc;
                        left.push(InlineNode::Revision(revision));
                    }
                    if !rc.is_empty() {
                        tail.id = ids.next().ok_or(EditError::IdExhausted)?;
                        tail.inlines = rc;
                        right.push(InlineNode::Revision(tail));
                    }
                }
                InlineNode::Sdt(mut sdt) => {
                    let (lc, rc) = split_inlines(std::mem::take(&mut sdt.inlines), local, ids)?;
                    let mut tail = sdt.clone();
                    if !lc.is_empty() {
                        sdt.inlines = lc;
                        left.push(InlineNode::Sdt(sdt));
                    }
                    if !rc.is_empty() {
                        tail.id = ids.next().ok_or(EditError::IdExhausted)?;
                        tail.inlines = rc;
                        right.push(InlineNode::Sdt(tail));
                    }
                }
                _ => return Err(EditError::Unsupported),
            }
        }
        cum += len;
    }
    Ok((left, right))
}

/// Finds the paragraph with `id`, recursing into table cells and block content
/// controls (document order), for in-place mutation.
/// The mutable twin of [`find_paragraph_in_inlines`].
fn find_paragraph_in_inlines_mut(inlines: &mut [InlineNode], id: NodeId) -> Option<&mut Paragraph> {
    for inline in inlines {
        let found = match inline {
            InlineNode::TextBox(text_box) => find_paragraph_mut(&mut text_box.blocks, id),
            InlineNode::Hyperlink(link) => find_paragraph_in_inlines_mut(&mut link.inlines, id),
            InlineNode::Field(field) => find_paragraph_in_inlines_mut(&mut field.inlines, id),
            InlineNode::Group(group) => find_paragraph_in_group_mut(&mut group.children, id),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn find_paragraph_mut(blocks: &mut [BlockNode], id: NodeId) -> Option<&mut Paragraph> {
    // Two passes, as `find_table_mut` does: a returned borrow in a loop that also
    // recurses trips the borrow checker, so the direct hit at this level is found
    // by index first and only then borrowed.
    if let Some(index) = blocks
        .iter()
        .position(|block| matches!(block, BlockNode::Paragraph(p) if p.id == id))
    {
        let BlockNode::Paragraph(paragraph) = &mut blocks[index] else {
            unreachable!("the position matched a paragraph");
        };
        return Some(paragraph);
    }
    for block in blocks {
        match block {
            // Text-box content is reached through the paragraph that holds the
            // box, not from the block list — see `find_paragraph`.
            BlockNode::Paragraph(paragraph) => {
                if let Some(found) = find_paragraph_in_inlines_mut(&mut paragraph.inlines, id) {
                    return Some(found);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(p) = find_paragraph_mut(&mut cell.blocks, id) {
                            return Some(p);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(p) = find_paragraph_mut(&mut sdt.blocks, id) {
                    return Some(p);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// Whether `name` is a valid bookmark name: non-empty and at most 255 bytes —
/// the model's bookmark-name domain (`validate_bookmarks`).
fn valid_bookmark_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 255
}

/// A located bookmark marker: its paragraph, its byte offset within that
/// paragraph's projected text, and the marker node's own id.
#[derive(Clone, Copy)]
struct MarkerSite {
    node: NodeId,
    offset: u32,
    marker: NodeId,
}

/// The paragraph-local index at which a zero-width marker sits at `offset`: the
/// first inline whose cumulative preceding text length equals `offset`. `None`
/// when `offset` falls *inside* an inline (e.g. mid-tab); the caller aligns run
/// boundaries first, so a run boundary always yields an index.
fn marker_insert_index(inlines: &[InlineNode], offset: u32) -> Option<usize> {
    let mut cumulative = 0u32;
    for (index, inline) in inlines.iter().enumerate() {
        if cumulative == offset {
            return Some(index);
        }
        cumulative = cumulative.saturating_add(inline_text_len(inline));
    }
    (cumulative == offset).then_some(inlines.len())
}

/// Inserts a single zero-width marker at `offset` in one paragraph's `inlines`,
/// splitting the run the offset lands in (if any) so the marker sits on a run
/// boundary. Returns the vec index it was inserted at.
fn insert_marker_at(
    inlines: &mut Vec<InlineNode>,
    offset: u32,
    marker: InlineNode,
    ids: &mut dyn RunIds,
) -> Result<usize, EditError> {
    ensure_run_boundary(inlines, offset, ids)?;
    let index = marker_insert_index(inlines, offset).ok_or(EditError::Unsupported)?;
    inlines.insert(index, marker);
    Ok(index)
}

/// Inserts a bookmark's start/end marker pair at `start`/`end`. Same-paragraph
/// ranges align both boundaries first (end before start, so the start offset
/// stays valid) then insert the end marker and the start marker so the pair
/// wraps exactly `[start, end)`; cross-paragraph ranges insert each marker into
/// its own paragraph independently.
fn insert_bookmark_pair(
    blocks: &mut [BlockNode],
    start: Pos,
    start_marker: InlineNode,
    end: Pos,
    end_marker: InlineNode,
    ids: &mut dyn RunIds,
) -> Result<(), EditError> {
    if start.node == end.node {
        let para = find_paragraph_mut(blocks, start.node).ok_or(EditError::NodeNotFound)?;
        if end.offset > paragraph_text_len(para) {
            return Err(EditError::OffsetOutOfRange);
        }
        // Align both boundaries first (end before start, so aligning the start does
        // not invalidate the end offset), then place the end marker and the start
        // marker. Inserting the start (at the lower-or-equal index) shifts the end
        // marker after it, so the pair reads start-then-end.
        ensure_run_boundary(&mut para.inlines, end.offset, ids)?;
        ensure_run_boundary(&mut para.inlines, start.offset, ids)?;
        let end_index =
            marker_insert_index(&para.inlines, end.offset).ok_or(EditError::Unsupported)?;
        para.inlines.insert(end_index, end_marker);
        let start_index =
            marker_insert_index(&para.inlines, start.offset).ok_or(EditError::Unsupported)?;
        para.inlines.insert(start_index, start_marker);
        return Ok(());
    }
    {
        let para = find_paragraph_mut(blocks, start.node).ok_or(EditError::NodeNotFound)?;
        if start.offset > paragraph_text_len(para) {
            return Err(EditError::OffsetOutOfRange);
        }
        insert_marker_at(&mut para.inlines, start.offset, start_marker, ids)?;
    }
    {
        let para = find_paragraph_mut(blocks, end.node).ok_or(EditError::NodeNotFound)?;
        if end.offset > paragraph_text_len(para) {
            return Err(EditError::OffsetOutOfRange);
        }
        insert_marker_at(&mut para.inlines, end.offset, end_marker, ids)?;
    }
    Ok(())
}

/// Inserts a pre-built [`Field`] node at `at`, aligning the caret to a run
/// boundary first (splitting the run the offset lands in) so the field sits at
/// paragraph top level. [`Operation::InsertField`]'s mutation.
fn insert_field_at(
    blocks: &mut [BlockNode],
    at: Pos,
    field: Field,
    ids: &mut dyn RunIds,
) -> Result<(), EditError> {
    let para = find_paragraph_mut(blocks, at.node).ok_or(EditError::NodeNotFound)?;
    if at.offset > paragraph_text_len(para) {
        return Err(EditError::OffsetOutOfRange);
    }
    insert_marker_at(&mut para.inlines, at.offset, InlineNode::Field(field), ids)?;
    Ok(())
}

/// Inserts an arbitrary inline object node at `at`, splitting a straddling run so
/// it lands exactly at the offset. [`Operation::InsertInlineObject`]'s mutation
/// (the generic sibling of [`insert_field_at`]).
fn insert_inline_object_at(
    blocks: &mut [BlockNode],
    at: Pos,
    node: InlineNode,
    ids: &mut dyn RunIds,
) -> Result<(), EditError> {
    let para = find_paragraph_mut(blocks, at.node).ok_or(EditError::NodeNotFound)?;
    if at.offset > paragraph_text_len(para) {
        return Err(EditError::OffsetOutOfRange);
    }
    insert_marker_at(&mut para.inlines, at.offset, node, ids)?;
    Ok(())
}

/// Locates the top-level inline object node with id `object` among the body's
/// paragraph inlines (descending into tables and block SDTs), returning its
/// paragraph, its byte offset in that paragraph, and a clone of the node (for
/// [`Operation::RemoveInlineObject`]'s inverse). The read-side sibling of
/// [`locate_field`].
fn locate_inline_object(blocks: &[BlockNode], object: NodeId) -> Option<(NodeId, u32, InlineNode)> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut offset = 0u32;
                for inline in &paragraph.inlines {
                    if is_object_node(inline) && inline.id() == object {
                        return Some((paragraph.id, offset, inline.clone()));
                    }
                    offset = offset.saturating_add(inline_text_len(inline));
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(found) = locate_inline_object(&cell.blocks, object) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = locate_inline_object(&sdt.blocks, object) {
                    return Some(found);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// Locates the top-level [`InlineNode::Field`] with id `field` among the body's
/// paragraph inlines (descending into tables and block SDTs), returning its
/// paragraph, its byte offset in that paragraph, and a clone of the field (for
/// [`Operation::RemoveField`]'s inverse). `None` unless it resolves at paragraph
/// top level.
fn locate_field(blocks: &[BlockNode], field: NodeId) -> Option<(NodeId, u32, Field)> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut offset = 0u32;
                for inline in &paragraph.inlines {
                    if let InlineNode::Field(found) = inline
                        && found.id == field
                    {
                        return Some((paragraph.id, offset, found.clone()));
                    }
                    offset = offset.saturating_add(inline_text_len(inline));
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(found) = locate_field(&cell.blocks, field) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = locate_field(&sdt.blocks, field) {
                    return Some(found);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// Removes the top-level [`InlineNode::Field`] whose own id is `field` from
/// `inlines`.
fn remove_field_by_id(inlines: &mut Vec<InlineNode>, field: NodeId) {
    inlines.retain(|inline| !matches!(inline, InlineNode::Field(f) if f.id == field));
}

/// Removes the top-level bookmark marker whose own id is `marker` from `inlines`.
fn remove_marker_by_id(inlines: &mut Vec<InlineNode>, marker: NodeId) {
    inlines.retain(|inline| match inline {
        InlineNode::BookmarkStart(m) => m.id != marker,
        InlineNode::BookmarkEnd(m) => m.id != marker,
        _ => true,
    });
}

/// Locates a bookmark's start and end markers among the body's top-level
/// paragraph inlines (descending into tables and block SDTs), returning each
/// marker's site. `None` unless both markers resolve at paragraph top level.
fn locate_bookmark_markers(
    blocks: &[BlockNode],
    bookmark: BookmarkId,
) -> Option<(MarkerSite, MarkerSite)> {
    let mut start = None;
    let mut end = None;
    scan_bookmark_markers(blocks, bookmark, &mut start, &mut end);
    Some((start?, end?))
}

fn scan_bookmark_markers(
    blocks: &[BlockNode],
    bookmark: BookmarkId,
    start: &mut Option<MarkerSite>,
    end: &mut Option<MarkerSite>,
) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut offset = 0u32;
                for inline in &paragraph.inlines {
                    match inline {
                        InlineNode::BookmarkStart(m) if m.bookmark == bookmark => {
                            *start = Some(MarkerSite {
                                node: paragraph.id,
                                offset,
                                marker: m.id,
                            });
                        }
                        InlineNode::BookmarkEnd(m) if m.bookmark == bookmark => {
                            *end = Some(MarkerSite {
                                node: paragraph.id,
                                offset,
                                marker: m.id,
                            });
                        }
                        _ => {}
                    }
                    offset = offset.saturating_add(inline_text_len(inline));
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        scan_bookmark_markers(&cell.blocks, bookmark, start, end);
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                scan_bookmark_markers(&sdt.blocks, bookmark, start, end);
            }
            BlockNode::AltChunk(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::{
        Definitions, DocGrid, LineNumbering, NoteProperties, PageBorders, PageNumbering,
        PaperSource, ParagraphProperties, Revision, RevisionKind, SectionBoundary, SectionColumns,
        StyleKind,
    };

    fn n(counter: u64) -> NodeId {
        NodeId::from_parts(7, counter).unwrap()
    }

    fn run(id: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: n(id),
            properties: RunProperties::default(),
            text: text.to_string(),
        })
    }

    fn revision(id: u64, run_id: u64, kind: RevisionKind, text: &str) -> InlineNode {
        InlineNode::Revision(Revision {
            id: n(id),
            kind,
            author: Some("Reviewer".to_owned()),
            date: None,
            revision_id: Some(id.to_string()),
            editor_group: None,
            inlines: vec![run(run_id, text)],
        })
    }

    fn para(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: n(id),
            properties: ParagraphProperties::default(),
            inlines,
        })
    }

    fn doc(paragraphs: Vec<BlockNode>) -> Document {
        Document::new(n(1000), paragraphs, Definitions::default()).expect("valid document")
    }

    fn external(url: &str) -> HyperlinkTarget {
        HyperlinkTarget::External(casual_doc_model::v1::ExternalTarget {
            url: url.to_owned(),
            anchor: None,
        })
    }

    /// The concatenated text of paragraph `id` (top-level runs), for assertions.
    /// The concatenated run text of an inline list, for sub-document assertions.
    fn runs_text(inlines: &[InlineNode]) -> String {
        inlines
            .iter()
            .filter_map(|inline| match inline {
                InlineNode::Run(run) => Some(run.text.clone()),
                _ => None,
            })
            .collect()
    }

    fn text_of(document: &Document, id: NodeId) -> String {
        fn walk(blocks: &[BlockNode], id: NodeId) -> Option<String> {
            for block in blocks {
                match block {
                    BlockNode::Paragraph(p) if p.id == id => {
                        return Some(
                            p.inlines
                                .iter()
                                .filter_map(|i| match i {
                                    InlineNode::Run(r) => Some(r.text.clone()),
                                    _ => None,
                                })
                                .collect(),
                        );
                    }
                    BlockNode::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                if let Some(s) = walk(&cell.blocks, id) {
                                    return Some(s);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        walk(document.body(), id).unwrap_or_default()
    }

    #[test]
    fn set_extent_resizes_an_inline_drawing_and_inverse_restores_it() {
        use casual_doc_model::v1::{Drawing, Extent, MediaId, MediaReference};
        let media = MediaId::new(NodeId::from_parts(7, 900).unwrap());
        let drawing_id = n(50);
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rId9".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let mut d = Document::new(
            n(1000),
            vec![BlockNode::Paragraph(Paragraph {
                id: n(2),
                properties: ParagraphProperties::default(),
                inlines: vec![
                    run(3, "before"),
                    InlineNode::Drawing(Drawing {
                        id: drawing_id,
                        media,
                        extent: Some(Extent {
                            width_emu: 914_400,
                            height_emu: 457_200,
                        }),
                        descr: None,
                        crop: None,
                        border: None,
                        flip_h: false,
                        flip_v: false,
                        rotation: None,
                    }),
                ],
            })],
            definitions,
        )
        .expect("valid document with a registered media part");
        let mut ids = IdGenerator::new(9);

        let new_extent = Some(Extent {
            width_emu: 1_828_800,
            height_emu: 914_400,
        });
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetExtent {
                object: drawing_id,
                extent: new_extent,
            },
        )
        .expect("resize the drawing");
        // The inverse carries the previous extent.
        assert_eq!(
            inverse,
            Operation::SetExtent {
                object: drawing_id,
                extent: Some(Extent {
                    width_emu: 914_400,
                    height_emu: 457_200,
                }),
            }
        );
        // Applying the inverse restores the original extent (undo).
        apply(&mut d, &mut ids, &inverse).expect("undo the resize");
        let BlockNode::Paragraph(p) = &d.body()[0] else {
            unreachable!()
        };
        let InlineNode::Drawing(drawing) = &p.inlines[1] else {
            panic!("the drawing is intact");
        };
        assert_eq!(drawing.extent.unwrap().width_emu, 914_400);

        // An unknown object is rejected.
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::SetExtent {
                    object: n(999),
                    extent: new_extent,
                },
            ),
            Err(EditError::NodeNotFound)
        ));
    }

    #[test]
    fn set_anchor_moves_and_rewraps_a_floating_drawing_with_exact_inverse() {
        use casual_doc_model::v1::{
            AnchorHorizontal, AnchorVertical, AnchoredDrawing, DrawingAnchor, Extent,
            HorizontalAnchor, HorizontalPosition, MediaId, MediaReference, VerticalAnchor,
            VerticalPosition, WrapDistances, WrapMode,
        };
        let media = MediaId::new(NodeId::from_parts(7, 901).unwrap());
        let float_id = n(60);
        let original = DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Column,
                position: HorizontalPosition::Offset(100_000),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Paragraph,
                position: VerticalPosition::Offset(50_000),
            },
            wrap: WrapMode::Square,
            wrap_distances: WrapDistances::default(),
            wrap_polygon: None,
            behind_doc: false,
        };
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rId9".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let mut d = Document::new(
            n(1000),
            vec![BlockNode::Paragraph(Paragraph {
                id: n(2),
                properties: ParagraphProperties::default(),
                inlines: vec![
                    run(3, "anchor"),
                    InlineNode::AnchoredDrawing(AnchoredDrawing {
                        id: float_id,
                        media,
                        extent: Extent {
                            width_emu: 914_400,
                            height_emu: 457_200,
                        },
                        anchor: original.clone(),
                        descr: None,
                        relative_height: None,
                        crop: None,
                        border: None,
                        flip_h: false,
                        flip_v: false,
                        rotation: None,
                    }),
                ],
            })],
            definitions,
        )
        .expect("valid document with a floating drawing");
        let mut ids = IdGenerator::new(9);

        // Move to an absolute page position + change wrap to behind-text.
        let moved = DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Page,
                position: HorizontalPosition::Offset(2_000_000),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Page,
                position: VerticalPosition::Offset(3_000_000),
            },
            wrap: WrapMode::None,
            wrap_distances: WrapDistances::default(),
            wrap_polygon: None,
            behind_doc: true,
        };
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetAnchor {
                object: float_id,
                anchor: Box::new(moved.clone()),
            },
        )
        .expect("move + re-wrap the float");
        // The inverse carries the original anchor.
        assert_eq!(
            inverse,
            Operation::SetAnchor {
                object: float_id,
                anchor: Box::new(original.clone()),
            }
        );
        // Applying the inverse restores it exactly (undo).
        apply(&mut d, &mut ids, &inverse).expect("undo the move");
        let BlockNode::Paragraph(p) = &d.body()[0] else {
            unreachable!()
        };
        let InlineNode::AnchoredDrawing(drawing) = &p.inlines[1] else {
            panic!("the float is intact");
        };
        assert_eq!(drawing.anchor, original);

        // An inline (non-floating) object has no anchor to set — rejected.
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::SetAnchor {
                    object: n(3), // a plain run, not floating
                    anchor: Box::new(moved),
                },
            ),
            Err(EditError::NodeNotFound)
        ));
    }

    /// Registers one PNG media part and returns the definitions holding it.
    fn media_defs(media: casual_doc_model::v1::MediaId) -> Definitions {
        use casual_doc_model::v1::MediaReference;
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rId9".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        definitions
    }

    /// An inline drawing referencing `media`, with the given optional crop/descr.
    fn drawing(
        id: u64,
        media: casual_doc_model::v1::MediaId,
        descr: Option<String>,
        crop: Option<CropRect>,
    ) -> InlineNode {
        use casual_doc_model::v1::{Drawing, Extent};
        InlineNode::Drawing(Drawing {
            id: n(id),
            media,
            extent: Some(Extent {
                width_emu: 914_400,
                height_emu: 457_200,
            }),
            descr,
            crop,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        })
    }

    #[test]
    fn set_image_crop_sets_clears_clamps_and_round_trips() {
        use casual_doc_model::v1::{CROP_MAX, MediaId};
        let media = MediaId::new(NodeId::from_parts(7, 900).unwrap());
        let drawing_id = n(50);
        let mut d = Document::new(
            n(1000),
            vec![para(
                2,
                vec![run(3, "before"), drawing(50, media, None, None)],
            )],
            media_defs(media),
        )
        .expect("valid document with a registered media part");
        let mut ids = IdGenerator::new(9);

        // An out-of-range edge is clamped into the model's crop range.
        let requested = CropRect {
            left: 10_000,
            top: 20_000,
            right: 5_000,
            bottom: 999_999,
        };
        let stored = CropRect {
            left: 10_000,
            top: 20_000,
            right: 5_000,
            bottom: CROP_MAX,
        };
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetImageCrop {
                object: drawing_id,
                crop: Some(requested),
            },
        )
        .expect("crop the drawing");
        // A freshly cropped image had no prior crop.
        assert_eq!(
            inverse,
            Operation::SetImageCrop {
                object: drawing_id,
                crop: None,
            }
        );
        let crop_of = |doc: &Document| {
            let BlockNode::Paragraph(p) = &doc.body()[0] else {
                unreachable!()
            };
            let InlineNode::Drawing(dr) = &p.inlines[1] else {
                panic!("the drawing is intact");
            };
            dr.crop
        };
        assert_eq!(crop_of(&d), Some(stored));

        // Undo restores the un-cropped state, and redo re-applies the clamped crop.
        apply(&mut d, &mut ids, &inverse).expect("undo the crop");
        assert_eq!(crop_of(&d), None);
        let clear = apply(
            &mut d,
            &mut ids,
            &Operation::SetImageCrop {
                object: drawing_id,
                crop: Some(requested),
            },
        )
        .expect("redo the crop");
        assert_eq!(crop_of(&d), Some(stored));

        // Clearing carries the previous crop, so its own inverse restores it.
        assert_eq!(
            clear,
            Operation::SetImageCrop {
                object: drawing_id,
                crop: None,
            }
        );
        let restore = apply(
            &mut d,
            &mut ids,
            &Operation::SetImageCrop {
                object: drawing_id,
                crop: None,
            },
        )
        .expect("clear the crop");
        assert_eq!(crop_of(&d), None);
        assert_eq!(
            restore,
            Operation::SetImageCrop {
                object: drawing_id,
                crop: Some(stored),
            }
        );

        // An identity (all-zero) crop is normalized to "no crop".
        apply(
            &mut d,
            &mut ids,
            &Operation::SetImageCrop {
                object: drawing_id,
                crop: Some(CropRect::default()),
            },
        )
        .expect("identity crop is a no-op clear");
        assert_eq!(crop_of(&d), None);

        // A text box has no `a:srcRect`; an unknown object is rejected.
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::SetImageCrop {
                    object: n(999),
                    crop: Some(stored),
                },
            ),
            Err(EditError::NodeNotFound)
        ));
    }

    #[test]
    fn set_object_descr_sets_clears_round_trips_and_bounds_length() {
        use casual_doc_model::v1::{MAX_DESCR_BYTES, MediaId};
        let media = MediaId::new(NodeId::from_parts(7, 901).unwrap());
        let drawing_id = n(50);
        let mut d = Document::new(
            n(1000),
            vec![para(2, vec![drawing(50, media, None, None)])],
            media_defs(media),
        )
        .expect("valid document with a registered media part");
        let mut ids = IdGenerator::new(9);

        let descr_of = |doc: &Document| {
            let BlockNode::Paragraph(p) = &doc.body()[0] else {
                unreachable!()
            };
            let InlineNode::Drawing(dr) = &p.inlines[0] else {
                panic!("the drawing is intact");
            };
            dr.descr.clone()
        };

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetObjectDescr {
                object: drawing_id,
                descr: Some("Company logo".to_owned()),
            },
        )
        .expect("set the alt text");
        assert_eq!(
            inverse,
            Operation::SetObjectDescr {
                object: drawing_id,
                descr: None,
            }
        );
        assert_eq!(descr_of(&d), Some("Company logo".to_owned()));

        // Undo clears it again; redo restores it.
        apply(&mut d, &mut ids, &inverse).expect("undo the alt text");
        assert_eq!(descr_of(&d), None);
        apply(
            &mut d,
            &mut ids,
            &Operation::SetObjectDescr {
                object: drawing_id,
                descr: Some("Company logo".to_owned()),
            },
        )
        .expect("redo the alt text");
        assert_eq!(descr_of(&d), Some("Company logo".to_owned()));

        // An empty alt text is rejected and the prior value survives.
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::SetObjectDescr {
                    object: drawing_id,
                    descr: Some(String::new()),
                },
            ),
            Err(EditError::ValueTooLarge)
        ));
        assert_eq!(descr_of(&d), Some("Company logo".to_owned()));

        // An over-long alt text is rejected the same way.
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::SetObjectDescr {
                    object: drawing_id,
                    descr: Some("x".repeat(MAX_DESCR_BYTES + 1)),
                },
            ),
            Err(EditError::ValueTooLarge)
        ));
        assert_eq!(descr_of(&d), Some("Company logo".to_owned()));
    }

    #[test]
    fn object_descr_reads_the_current_alt_text() {
        use casual_doc_model::v1::MediaId;
        let media = MediaId::new(NodeId::from_parts(7, 903).unwrap());
        let drawing_id = n(50);
        // A drawing that carries alt text: the getter returns it.
        let with_alt = Document::new(
            n(1000),
            vec![para(
                2,
                vec![drawing(50, media, Some("Company logo".to_owned()), None)],
            )],
            media_defs(media),
        )
        .expect("valid document");
        assert_eq!(
            object_descr(&with_alt, drawing_id),
            Some("Company logo".to_owned())
        );
        // An unknown node id resolves to no alt text (not a panic).
        assert_eq!(object_descr(&with_alt, n(999)), None);
        // A drawing with no alt text returns None.
        let without_alt = Document::new(
            n(1001),
            vec![para(2, vec![drawing(50, media, None, None)])],
            media_defs(media),
        )
        .expect("valid document");
        assert_eq!(object_descr(&without_alt, drawing_id), None);
    }

    #[test]
    fn insert_inline_object_splits_the_run_and_inverse_removes_the_drawing() {
        use casual_doc_model::v1::MediaId;
        let media = MediaId::new(NodeId::from_parts(7, 950).unwrap());
        let drawing_id = n(60);
        let mut d = Document::new(
            n(1000),
            vec![para(2, vec![run(3, "AB")])],
            media_defs(media),
        )
        .expect("valid document with registered media");
        let original = d.clone();
        let mut ids = IdGenerator::new(30);

        // Insert a picture mid-run (between A and B): the run splits and the
        // drawing lands at the offset. The inverse is a DeleteObject of its id.
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertInlineObject {
                at: Pos::new(n(2), 1),
                node: Box::new(drawing(60, media, None, None)),
            },
        )
        .expect("insert the image");
        assert!(
            inlines_of(&d, n(2))
                .iter()
                .any(|i| matches!(i, InlineNode::Drawing(dr) if dr.id == drawing_id)),
            "the drawing was inserted"
        );
        assert_eq!(
            text_of(&d, n(2)),
            "AB",
            "the run's text is preserved across the split"
        );
        assert!(
            matches!(inverse, Operation::RemoveInlineObject { object } if object == drawing_id)
        );

        // The inverse removes the drawing and coalesces the split run back: the
        // paragraph is one run of "AB" again, with no drawing, and the document
        // validates. (Run identity may differ after a split/coalesce round trip,
        // which is not user-visible, so this checks structure, not raw id equality.)
        apply(&mut d, &mut ids, &inverse).expect("remove the image (inverse)");
        let restored = inlines_of(&d, n(2));
        assert_eq!(restored.len(), 1, "the split run coalesced back to one run");
        assert!(matches!(&restored[0], InlineNode::Run(_)));
        assert_eq!(text_of(&d, n(2)), "AB");
        assert!(
            !restored.iter().any(|i| matches!(i, InlineNode::Drawing(_))),
            "the drawing is gone"
        );
        d.validate()
            .expect("the document is valid after the round trip");
        let _ = original;
    }

    #[test]
    fn delete_object_removes_an_inline_drawing_and_inverse_restores_it() {
        use casual_doc_model::v1::MediaId;
        let media = MediaId::new(NodeId::from_parts(7, 902).unwrap());
        let drawing_id = n(50);
        let original = drawing(50, media, Some("Alt".to_owned()), None);
        // The surrounding runs carry different properties so removing the object
        // between them does not leave two mergeable equal-property runs adjacent.
        let bold_before = InlineNode::Run(Run {
            id: n(3),
            properties: RunProperties {
                bold: Some(true),
                ..RunProperties::default()
            },
            text: "before".to_owned(),
        });
        let mut d = Document::new(
            n(1000),
            vec![para(
                2,
                vec![bold_before, original.clone(), run(4, "after")],
            )],
            media_defs(media),
        )
        .expect("valid document with an inline drawing");
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteObject { object: drawing_id },
        )
        .expect("delete the drawing");
        // The inverse re-inserts the exact node at its original inline position.
        assert_eq!(
            inverse,
            Operation::InsertObjectNode {
                owner: n(2),
                index: 1,
                node: Box::new(original.clone()),
            }
        );
        let BlockNode::Paragraph(p) = &d.body()[0] else {
            unreachable!()
        };
        assert_eq!(p.inlines.len(), 2);
        assert!(!p.inlines.iter().any(|node| node.id() == drawing_id));

        // Undo restores the drawing verbatim at index 1.
        let redo = apply(&mut d, &mut ids, &inverse).expect("undo the delete");
        assert_eq!(redo, Operation::DeleteObject { object: drawing_id });
        let BlockNode::Paragraph(p) = &d.body()[0] else {
            unreachable!()
        };
        assert_eq!(p.inlines.len(), 3);
        assert_eq!(p.inlines[1], original);

        // Redo removes it again.
        apply(&mut d, &mut ids, &redo).expect("redo the delete");
        let BlockNode::Paragraph(p) = &d.body()[0] else {
            unreachable!()
        };
        assert_eq!(p.inlines.len(), 2);

        // An unknown object is rejected.
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::DeleteObject { object: n(999) }
            ),
            Err(EditError::NodeNotFound)
        ));

        // Removing an object wedged between two equal-property runs would leave them
        // mergeable-adjacent (model-invalid); the op is refused and rolls back.
        let wedged_id = n(80);
        let mut wedged = Document::new(
            n(1001),
            vec![para(
                5,
                vec![
                    run(6, "left"),
                    drawing(80, media, None, None),
                    run(7, "right"),
                ],
            )],
            media_defs(media),
        )
        .expect("valid document with a wedged drawing");
        assert!(matches!(
            apply(
                &mut wedged,
                &mut ids,
                &Operation::DeleteObject { object: wedged_id },
            ),
            Err(EditError::Unsupported)
        ));
        let BlockNode::Paragraph(p) = &wedged.body()[0] else {
            unreachable!()
        };
        assert_eq!(p.inlines.len(), 3);
        assert_eq!(p.inlines[1].id(), wedged_id);
    }

    #[test]
    fn delete_object_removes_an_anchored_float_and_rejects_emptying_a_wrapper() {
        use casual_doc_model::v1::{
            AnchorHorizontal, AnchorVertical, AnchoredDrawing, DrawingAnchor, Extent,
            HorizontalAnchor, HorizontalPosition, MediaId, VerticalAnchor, VerticalPosition,
            WrapDistances, WrapMode,
        };
        let media = MediaId::new(NodeId::from_parts(7, 903).unwrap());
        let float_id = n(60);
        let float = InlineNode::AnchoredDrawing(AnchoredDrawing {
            id: float_id,
            media,
            extent: Extent {
                width_emu: 914_400,
                height_emu: 457_200,
            },
            anchor: DrawingAnchor {
                horizontal: AnchorHorizontal {
                    relative_from: HorizontalAnchor::Page,
                    position: HorizontalPosition::Offset(100_000),
                },
                vertical: AnchorVertical {
                    relative_from: VerticalAnchor::Page,
                    position: VerticalPosition::Offset(50_000),
                },
                wrap: WrapMode::None,
                wrap_distances: WrapDistances::default(),
                wrap_polygon: None,
                behind_doc: true,
            },
            descr: None,
            relative_height: None,
            crop: None,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        });
        // A clickable image: a hyperlink whose only child is a drawing. Removing it
        // would empty the hyperlink, which the model forbids.
        let linked_id = n(70);
        let linked_drawing = drawing(70, media, None, None);
        let hyperlink = InlineNode::Hyperlink(Hyperlink {
            id: n(71),
            target: external("https://example.com"),
            tooltip: None,
            inlines: vec![linked_drawing],
        });
        let mut d = Document::new(
            n(1000),
            vec![
                para(2, vec![run(3, "anchor"), float.clone()]),
                para(4, vec![hyperlink]),
            ],
            media_defs(media),
        )
        .expect("valid document with a floating drawing");
        let mut ids = IdGenerator::new(9);

        // The float round-trips through delete + undo verbatim.
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteObject { object: float_id },
        )
        .expect("delete the float");
        assert_eq!(
            inverse,
            Operation::InsertObjectNode {
                owner: n(2),
                index: 1,
                node: Box::new(float.clone()),
            }
        );
        apply(&mut d, &mut ids, &inverse).expect("undo the float delete");
        let BlockNode::Paragraph(p) = &d.body()[0] else {
            unreachable!()
        };
        assert_eq!(p.inlines[1], float);

        // Deleting the sole child of a hyperlink is refused, and the document is
        // left unchanged (the drawing is restored on rollback).
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::DeleteObject { object: linked_id }
            ),
            Err(EditError::Unsupported)
        ));
        let BlockNode::Paragraph(p) = &d.body()[1] else {
            unreachable!()
        };
        let InlineNode::Hyperlink(link) = &p.inlines[0] else {
            panic!("the hyperlink is intact");
        };
        assert_eq!(link.inlines.len(), 1);
        assert_eq!(link.inlines[0].id(), linked_id);
    }

    #[test]
    fn cropped_and_alt_texted_image_survives_a_model_write_reopen() {
        use casual_doc_model::v1::MediaId;
        let media = MediaId::new(NodeId::from_parts(7, 904).unwrap());
        let crop = CropRect {
            left: 10_000,
            top: 20_000,
            right: 5_000,
            bottom: 15_000,
        };
        let m1 = Document::new(
            n(1000),
            vec![para(
                2,
                vec![drawing(
                    50,
                    media,
                    Some("Quarterly chart".to_owned()),
                    Some(crop),
                )],
            )],
            media_defs(media),
        )
        .expect("valid document with a cropped, alt-texted image");
        // Write (serialize) then reopen (deserialize) the model: the crop + alt text
        // round-trip verbatim.
        let json = serde_json::to_string(&m1).expect("serialize the model");
        let m2: Document = serde_json::from_str(&json).expect("reopen the model");
        assert_eq!(m1, m2);
    }

    #[test]
    fn insert_blocks_splices_a_sequence_and_inverse_removes_it() {
        // Body: [P2, P3]. Insert two blocks between them at index 1.
        let mut d = doc(vec![
            para(2, vec![run(3, "one")]),
            para(4, vec![run(5, "two")]),
        ]);
        let mut ids = IdGenerator::new(9);

        let inserted = vec![para(6, vec![run(7, "A")]), para(8, vec![run(9, "B")])];
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertBlocks {
                container: None,
                index: 1,
                blocks: inserted.clone(),
            },
        )
        .expect("insert blocks");
        assert_eq!(d.body().len(), 4, "two blocks spliced in");
        assert_eq!(text_of(&d, n(6)), "A");
        assert_eq!(text_of(&d, n(8)), "B");
        assert_eq!(
            inverse,
            Operation::DeleteBlocks {
                container: None,
                index: 1,
                count: 2,
            }
        );

        // Applying the inverse removes exactly the two inserted blocks (undo),
        // and its own inverse restores them verbatim (redo).
        let redo = apply(&mut d, &mut ids, &inverse).expect("delete blocks");
        assert_eq!(d.body().len(), 2, "the two inserted blocks are gone");
        assert_eq!(text_of(&d, n(2)), "one");
        assert_eq!(text_of(&d, n(4)), "two");
        assert_eq!(
            redo,
            Operation::InsertBlocks {
                container: None,
                index: 1,
                blocks: inserted,
            }
        );
        apply(&mut d, &mut ids, &redo).expect("redo");
        assert_eq!(d.body().len(), 4);
        assert_eq!(text_of(&d, n(6)), "A");
    }

    #[test]
    fn insert_blocks_rejects_an_out_of_range_index_and_empty_edit() {
        let mut d = doc(vec![para(2, vec![run(3, "x")])]);
        let mut ids = IdGenerator::new(9);
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertBlocks {
                    container: None,
                    index: 9,
                    blocks: vec![para(4, vec![run(5, "y")])],
                },
            ),
            Err(EditError::OffsetOutOfRange)
        ));
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertBlocks {
                    container: None,
                    index: 0,
                    blocks: Vec::new(),
                },
            ),
            Err(EditError::EmptyEdit)
        ));
    }

    #[test]
    fn insert_splices_and_inverse_removes() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Helloworld")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 5),
                text: " brave ".to_string(),
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "Hello brave world");
        assert_eq!(
            inverse,
            Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 5),
                    end: Pos::new(p, 12), // 5 + len(" brave ")
                },
            }
        );

        // Applying the inverse restores the original text (undo).
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(text_of(&d, p), "Helloworld");
    }

    #[test]
    fn edits_after_hidden_deletion_use_final_projected_offsets() {
        let p = n(2);
        let mut d = doc(vec![para(
            2,
            vec![
                revision(3, 4, RevisionKind::Deletion, "removed"),
                run(5, "B"),
            ],
        )]);
        let mut ids = IdGenerator::new(9);
        let paragraph = find_paragraph(d.body(), p).expect("paragraph");
        assert_eq!(paragraph_text_len(paragraph), 1);

        apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 0),
                text: "A".to_owned(),
            },
        )
        .expect("insert at projected start");
        assert_eq!(text_of(&d, p), "AB");
        let paragraph = find_paragraph(d.body(), p).expect("paragraph");
        assert_eq!(paragraph_text_len(paragraph), 2);
    }

    #[test]
    fn set_hyperlink_create_update_remove_and_inverse_are_exact() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello world")])]);
        let mut ids = IdGenerator::new(9);
        let range = Range {
            start: Pos::new(p, 6),
            end: Pos::new(p, 11),
        };

        let original = d.clone();
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetHyperlink {
                range,
                id: n(4),
                target: Some(external("https://example.com/one")),
                tooltip: Some("Example".to_owned()),
            },
        )
        .unwrap();
        d.validate().unwrap();
        let para = find_paragraph(d.body(), p).unwrap();
        let InlineNode::Hyperlink(link) = &para.inlines[1] else {
            panic!("selected text was not wrapped");
        };
        assert_eq!(link.id, n(4));
        assert_eq!(link.target, external("https://example.com/one"));
        assert_eq!(nested_len(&link.inlines), 5);
        let created = d.clone();

        let update_inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetHyperlink {
                range,
                id: n(99),
                target: Some(external("https://example.com/two")),
                tooltip: None,
            },
        )
        .unwrap();
        let para = find_paragraph(d.body(), p).unwrap();
        let InlineNode::Hyperlink(link) = &para.inlines[1] else {
            panic!("existing link disappeared");
        };
        assert_eq!(link.id, n(4), "updates preserve the imported link identity");
        assert_eq!(link.target, external("https://example.com/two"));

        apply(&mut d, &mut ids, &update_inverse).unwrap();
        assert_eq!(d, created, "inverse restores the post-create inline tree");

        let remove_inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetHyperlink {
                range,
                id: n(4),
                target: None,
                tooltip: None,
            },
        )
        .unwrap();
        d.validate().unwrap();
        assert!(
            find_paragraph(d.body(), p)
                .unwrap()
                .inlines
                .iter()
                .all(|inline| !matches!(inline, InlineNode::Hyperlink(_)))
        );
        apply(&mut d, &mut ids, &remove_inverse).unwrap();
        assert!(matches!(
            find_paragraph(d.body(), p).unwrap().inlines[1],
            InlineNode::Hyperlink(_)
        ));

        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(d, original);
    }

    #[test]
    fn set_hyperlink_rejects_partial_existing_wrapper_without_mutation() {
        let p = n(2);
        let linked = InlineNode::Hyperlink(Hyperlink {
            id: n(4),
            target: external("https://example.com"),
            tooltip: None,
            inlines: vec![run(5, "linked")],
        });
        let mut d = doc(vec![para(2, vec![run(3, "A "), linked])]);
        let before = d.clone();
        let mut ids = IdGenerator::new(9);
        let result = apply(
            &mut d,
            &mut ids,
            &Operation::SetHyperlink {
                range: Range {
                    start: Pos::new(p, 3),
                    end: Pos::new(p, 8),
                },
                id: n(6),
                target: Some(external("https://other.example")),
                tooltip: None,
            },
        );
        assert_eq!(result, Err(EditError::Unsupported));
        assert_eq!(d, before);
    }

    #[test]
    fn delete_within_run_and_inverse_reinserts() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello world")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 5),
                    end: Pos::new(p, 11),
                },
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "Hello");
        assert_eq!(
            inverse,
            Operation::InsertText {
                at: Pos::new(p, 5),
                text: " world".to_string(),
            }
        );
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(text_of(&d, p), "Hello world");
    }

    /// The runs (text, bold flag) of paragraph `id`, for formatting assertions.
    fn runs_of(document: &Document, id: NodeId) -> Vec<(String, Option<bool>)> {
        document
            .body()
            .iter()
            .find_map(|b| match b {
                BlockNode::Paragraph(p) if p.id == id => Some(
                    p.inlines
                        .iter()
                        .filter_map(|i| match i {
                            InlineNode::Run(r) => Some((r.text.clone(), r.properties.bold)),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn format_splits_runs_and_undo_restores() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "HelloWorld")])]);
        let mut ids = IdGenerator::new(9);

        // Bold the first 5 bytes: the run splits, only "Hello" becomes bold.
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::FormatText {
                range: Range {
                    start: Pos::new(p, 0),
                    end: Pos::new(p, 5),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    ..FormatDelta::default()
                },
            },
        )
        .unwrap();
        assert_eq!(
            runs_of(&d, p),
            vec![
                ("Hello".to_string(), Some(true)),
                ("World".to_string(), None),
            ]
        );

        // The inverse restores the original single, unformatted run.
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(runs_of(&d, p), vec![("HelloWorld".to_string(), None)]);
    }

    #[test]
    fn clear_formatting_restores_direct_defaults_and_undoes() {
        let p = n(2);
        let mut styled = run(3, "Styled text");
        if let InlineNode::Run(run) = &mut styled {
            run.properties.bold = Some(true);
            run.properties.italic = Some(true);
            run.properties.size_half_points = Some(28);
        }
        let original = styled.clone();
        let mut d = doc(vec![para(2, vec![styled])]);
        let mut ids = IdGenerator::new(9);
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::ClearFormatting {
                range: Range {
                    start: Pos::new(p, 0),
                    end: Pos::new(p, 11),
                },
            },
        )
        .unwrap();
        let BlockNode::Paragraph(paragraph) = &d.body()[0] else {
            panic!("expected paragraph");
        };
        let InlineNode::Run(cleared) = &paragraph.inlines[0] else {
            panic!("expected cleared run");
        };
        assert_eq!(cleared.properties, RunProperties::default());
        apply(&mut d, &mut ids, &inverse).unwrap();
        let BlockNode::Paragraph(paragraph) = &d.body()[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(paragraph.inlines[0], original);
    }

    #[test]
    fn split_and_join_are_inverses() {
        let p = n(2);
        let new = n(50);
        let mut d = doc(vec![para(2, vec![run(3, "HelloWorld")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 5),
                new_id: new,
            },
        )
        .unwrap();
        assert_eq!(d.body().len(), 2, "one paragraph became two");
        assert_eq!(text_of(&d, p), "Hello");
        assert_eq!(text_of(&d, new), "World");
        assert_eq!(
            inverse,
            Operation::JoinParagraphs {
                first: p,
                second: new
            }
        );

        // The inverse join restores a single paragraph with the joined text.
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(d.body().len(), 1);
        assert_eq!(text_of(&d, p), "HelloWorld");
    }

    /// Word's `w:next`: Enter at the END of a heading starts the style the
    /// heading declares it is followed by, which is why typing a heading and
    /// pressing Enter puts you in body text rather than in a second heading.
    /// Reading a paragraph's properties must work wherever it lives. This is the
    /// defect a user hit: a RIGHT-aligned header paragraph reported itself as
    /// left-aligned, because the read walked the body alone, found nothing, and
    /// the caller fell back to its default. A wrong answer is worse than none —
    /// it invites "fixing" an alignment that was never wrong.
    #[test]
    fn paragraph_properties_are_read_from_whichever_surface_owns_them() {
        let header_id = HeaderFooterId::new(n(970));
        let para_id = n(971);
        let mut definitions = Definitions::default();
        let properties = ParagraphProperties {
            alignment: Some(casual_doc_model::v1::Alignment::End),
            ..ParagraphProperties::default()
        };
        definitions.headers.insert(
            header_id,
            casual_doc_model::v1::HeaderFooter {
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: para_id,
                    properties,
                    inlines: vec![run(972, "Right aligned")],
                })],
            },
        );
        let d = Document::new(n(1000), vec![para(2, vec![run(3, "body")])], definitions)
            .expect("valid document");

        assert_eq!(
            paragraph_properties(&d, para_id).and_then(|p| p.alignment),
            Some(casual_doc_model::v1::Alignment::End),
            "the header paragraph's own alignment, not a default"
        );
        // The body still reads correctly.
        assert!(paragraph_properties(&d, n(2)).is_some());
        // And an id no surface owns is still None.
        assert!(paragraph_properties(&d, n(4242)).is_none());
    }

    /// A paragraph inside an inline TEXT BOX is ordinary block content in the same
    /// id space too — `record_inline_ids` records it there — so an op that
    /// addresses it by `NodeId` should reach it. Resolution walked only block
    /// lists, never into the paragraph that HOLDS a box, so every position inside
    /// one answered `NodeNotFound`: the reason a text box could not be typed in.
    #[test]
    fn text_ops_reach_a_paragraph_inside_an_inline_text_box() {
        let inner = n(960);
        let box_id = n(961);
        let host = n(2);
        let mut d = doc(vec![BlockNode::Paragraph(Paragraph {
            id: host,
            properties: ParagraphProperties::default(),
            inlines: vec![
                run(3, "before"),
                InlineNode::TextBox(casual_doc_model::v1::TextBox {
                    id: box_id,
                    anchor: None,
                    relative_height: None,
                    extent: None,
                    fill: None,
                    border: None,
                    body_properties: casual_doc_model::v1::TextBoxBodyProperties::default(),
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: inner,
                        properties: ParagraphProperties::default(),
                        inlines: vec![run(962, "Caption")],
                    })],
                }),
            ],
        })]);
        let mut ids = IdGenerator::new(9);

        assert_eq!(
            surface_of(&d, inner),
            Some(Surface::Body),
            "the box lives in the body, so its content resolves to the body surface"
        );

        apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(inner, 7),
                text: " text".to_owned(),
            },
        )
        .unwrap();

        let paragraph = find_paragraph(d.body(), inner).expect("the box's paragraph");
        assert_eq!(runs_text(&paragraph.inlines), "Caption text");
        // The paragraph holding the box is untouched.
        assert_eq!(text_of(&d, host), "before");
    }

    /// Header, footer and note content is ordinary block content in the same id
    /// space as the body, so an op that addresses a position by `NodeId` should
    /// reach it. Resolution used to start at `doc.body_mut()` unconditionally, so
    /// every one of these positions answered `NodeNotFound` — the reason nothing
    /// outside the body could be typed into.
    /// docs/85 §8.3: creating a header body and linking a section to it are
    /// separate ops, and both retain exact inverses.
    #[test]
    fn creating_a_header_body_round_trips_through_its_inverse() {
        let id = HeaderFooterId::new(n(920));
        let mut d = doc(vec![para(2, vec![run(3, "body")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::CreateHeaderFooterBody {
                region: RunningRegion::Header,
                id,
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: n(921),
                    properties: ParagraphProperties::default(),
                    inlines: vec![run(922, "Running title")],
                })],
            },
        )
        .unwrap();
        assert!(d.definitions().headers.get(&id).is_some());
        assert_eq!(
            inverse,
            Operation::RemoveHeaderFooterBody {
                region: RunningRegion::Header,
                id
            }
        );

        // Undo removes it, and its own inverse carries the content back.
        let redo = apply(&mut d, &mut ids, &inverse).unwrap();
        assert!(d.definitions().headers.get(&id).is_none());
        let Operation::CreateHeaderFooterBody { blocks, .. } = &redo else {
            panic!("the inverse of a remove is a create");
        };
        assert_eq!(blocks.len(), 1, "the removed body is carried back out");
    }

    /// Creating over an existing id is refused, so a body a section still points
    /// at can never be silently orphaned.
    #[test]
    fn creating_a_header_body_twice_is_refused() {
        let id = HeaderFooterId::new(n(930));
        let mut d = doc(vec![para(2, vec![run(3, "body")])]);
        let mut ids = IdGenerator::new(9);
        let op = Operation::CreateHeaderFooterBody {
            region: RunningRegion::Header,
            id,
            blocks: Vec::new(),
        };
        apply(&mut d, &mut ids, &op).unwrap();
        assert_eq!(apply(&mut d, &mut ids, &op), Err(EditError::Unsupported));
    }

    /// "Link to Previous" is reference ABSENCE (docs/85 §8.4, Q7): removing a
    /// section's ref makes it inherit again, and the op is self-inverse.
    #[test]
    fn pointing_and_unpointing_a_section_running_ref_is_self_inverse() {
        let body = HeaderFooterId::new(n(940));
        let section_num = 941_u64;
        let mut definitions = Definitions::default();
        definitions.headers.insert(
            body,
            casual_doc_model::v1::HeaderFooter { blocks: Vec::new() },
        );
        let mut boundary = section(section_num);
        boundary.headers.clear();
        let section_id = boundary.id;
        definitions.sections.push(boundary);
        let mut d = Document::new(n(1000), vec![para(2, vec![run(3, "body")])], definitions)
            .expect("valid document");
        let mut ids = IdGenerator::new(9);

        let link = Operation::SetSectionRunningRef {
            section: section_id,
            region: RunningRegion::Header,
            kind: HeaderFooterKind::Default,
            reference: Some(body),
        };
        let inverse = apply(&mut d, &mut ids, &link).unwrap();
        assert_eq!(
            d.definitions().sections[0].headers.len(),
            1,
            "the section now declares its own header"
        );
        // The inverse removes it again — which is exactly "Link to Previous" on.
        assert_eq!(
            inverse,
            Operation::SetSectionRunningRef {
                section: section_id,
                region: RunningRegion::Header,
                kind: HeaderFooterKind::Default,
                reference: None,
            }
        );
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert!(
            d.definitions().sections[0].headers.is_empty(),
            "removing the ref restores inheritance"
        );
    }

    #[test]
    fn text_ops_reach_a_paragraph_inside_a_header() {
        let header_id = HeaderFooterId::new(n(900));
        let para_id = n(901);
        let mut definitions = Definitions::default();
        definitions.headers.insert(
            header_id,
            casual_doc_model::v1::HeaderFooter {
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: para_id,
                    properties: ParagraphProperties::default(),
                    inlines: vec![run(902, "Draft")],
                })],
            },
        );
        let mut d = Document::new(n(1000), vec![para(2, vec![run(3, "body")])], definitions)
            .expect("valid document");
        let mut ids = IdGenerator::new(9);

        assert_eq!(
            surface_of(&d, para_id),
            Some(Surface::Header(header_id)),
            "the header's paragraph resolves to the header surface"
        );

        apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(para_id, 5),
                text: " copy".to_owned(),
            },
        )
        .unwrap();

        let header = d
            .definitions()
            .headers
            .get(&header_id)
            .expect("header definition");
        let BlockNode::Paragraph(paragraph) = &header.blocks[0] else {
            unreachable!("the header holds one paragraph");
        };
        assert_eq!(runs_text(&paragraph.inlines), "Draft copy");
        // The body is untouched by an edit addressed at the header.
        assert_eq!(text_of(&d, n(2)), "body");
    }

    /// The same for a footnote body — the surface a note-insert command would
    /// need before it could put the caret in the note it just created.
    #[test]
    fn text_ops_reach_a_paragraph_inside_a_footnote() {
        let note_id = NoteId::new(n(910));
        let para_id = n(911);
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note_id,
            Note {
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: para_id,
                    properties: ParagraphProperties::default(),
                    inlines: vec![run(912, "See")],
                })],
            },
        );
        let mut d = Document::new(n(1000), vec![para(2, vec![run(3, "body")])], definitions)
            .expect("valid document");
        let mut ids = IdGenerator::new(9);

        assert_eq!(surface_of(&d, para_id), Some(Surface::Footnote(note_id)));

        apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(para_id, 3),
                text: " also".to_owned(),
            },
        )
        .unwrap();

        let note = d
            .definitions()
            .footnotes
            .get(&note_id)
            .expect("footnote definition");
        let BlockNode::Paragraph(paragraph) = &note.blocks[0] else {
            unreachable!("the note holds one paragraph");
        };
        assert_eq!(runs_text(&paragraph.inlines), "See also");
    }

    /// A node no surface owns is still `NodeNotFound`: resolution must not become
    /// a way for a bad position to succeed somewhere unexpected.
    #[test]
    fn an_unknown_node_still_resolves_to_nothing() {
        let d = doc(vec![para(2, vec![run(3, "body")])]);
        assert_eq!(surface_of(&d, n(4242)), None);
    }

    #[test]
    fn split_at_end_starts_the_style_the_current_one_is_followed_by() {
        let heading = StyleId::new(n(700));
        let body = StyleId::new(n(701));
        let mut definitions = Definitions::default();
        let mut heading_style = paragraph_style("Heading 1", None);
        heading_style.next = Some(body);
        definitions.styles.insert(heading, heading_style);
        definitions
            .styles
            .insert(body, paragraph_style("Body", None));

        let p = n(2);
        let new = n(50);
        let mut block = para(2, vec![run(3, "Title")]);
        let BlockNode::Paragraph(paragraph) = &mut block else {
            unreachable!("para() builds a paragraph");
        };
        paragraph.properties.style_ref = Some(heading);
        let mut d = Document::new(n(1000), vec![block], definitions).expect("valid document");
        let mut ids = IdGenerator::new(9);

        apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 5),
                new_id: new,
            },
        )
        .unwrap();

        let original = find_paragraph(d.body(), p).expect("original paragraph");
        let started = find_paragraph(d.body(), new).expect("new paragraph");
        assert_eq!(
            original.properties.style_ref.as_ref(),
            Some(&heading),
            "the heading itself is untouched"
        );
        assert_eq!(
            started.properties.style_ref.as_ref(),
            Some(&body),
            "the paragraph Enter started takes the heading's next style"
        );
    }

    /// Splitting in the MIDDLE is one paragraph becoming two, not a new one
    /// starting, so both halves keep the style — as Word does.
    #[test]
    fn split_in_the_middle_keeps_the_style_on_both_halves() {
        let heading = StyleId::new(n(700));
        let body = StyleId::new(n(701));
        let mut definitions = Definitions::default();
        let mut heading_style = paragraph_style("Heading 1", None);
        heading_style.next = Some(body);
        definitions.styles.insert(heading, heading_style);
        definitions
            .styles
            .insert(body, paragraph_style("Body", None));

        let p = n(2);
        let new = n(50);
        let mut block = para(2, vec![run(3, "Title")]);
        let BlockNode::Paragraph(paragraph) = &mut block else {
            unreachable!("para() builds a paragraph");
        };
        paragraph.properties.style_ref = Some(heading);
        let mut d = Document::new(n(1000), vec![block], definitions).expect("valid document");
        let mut ids = IdGenerator::new(9);

        apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 2),
                new_id: new,
            },
        )
        .unwrap();

        assert_eq!(
            find_paragraph(d.body(), new)
                .expect("new paragraph")
                .properties
                .style_ref
                .as_ref(),
            Some(&heading),
            "a mid-paragraph split keeps the style"
        );
    }

    /// A style declared to be followed by itself — the common case for body
    /// styles — means "carry on", so nothing changes.
    #[test]
    fn a_style_that_follows_itself_leaves_the_new_paragraph_alone() {
        let body = StyleId::new(n(701));
        let mut definitions = Definitions::default();
        let mut body_style = paragraph_style("Body", None);
        body_style.next = Some(body);
        definitions.styles.insert(body, body_style);

        let p = n(2);
        let new = n(50);
        let mut block = para(2, vec![run(3, "text")]);
        let BlockNode::Paragraph(paragraph) = &mut block else {
            unreachable!("para() builds a paragraph");
        };
        paragraph.properties.style_ref = Some(body);
        let mut d = Document::new(n(1000), vec![block], definitions).expect("valid document");
        let mut ids = IdGenerator::new(9);

        apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 4),
                new_id: new,
            },
        )
        .unwrap();

        assert_eq!(
            find_paragraph(d.body(), new)
                .expect("new paragraph")
                .properties
                .style_ref
                .as_ref(),
            Some(&body),
        );
    }

    #[test]
    fn split_at_start_leaves_an_empty_leading_paragraph() {
        let p = n(2);
        let new = n(50);
        let mut d = doc(vec![para(2, vec![run(3, "abc")])]);
        let mut ids = IdGenerator::new(9);
        apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 0),
                new_id: new,
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "");
        assert_eq!(text_of(&d, new), "abc");
    }

    #[test]
    fn join_requires_the_second_to_be_adjacent() {
        let mut d = doc(vec![
            para(2, vec![run(3, "a")]),
            para(4, vec![run(5, "b")]),
            para(6, vec![run(7, "c")]),
        ]);
        let mut ids = IdGenerator::new(9);
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::JoinParagraphs {
                    first: n(2),
                    second: n(6), // not adjacent to 2
                }
            ),
            Err(EditError::Unsupported)
        );
        assert_eq!(d.body().len(), 3, "no mutation on error");
    }

    #[test]
    fn typing_into_an_empty_paragraph_creates_a_run() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![])]);
        let mut ids = IdGenerator::new(9);
        apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 0),
                text: "hi".to_string(),
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "hi");
    }

    #[test]
    fn out_of_range_and_missing_node_are_errors_and_do_not_mutate() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "abc")])]);
        let mut ids = IdGenerator::new(9);

        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertText {
                    at: Pos::new(p, 99),
                    text: "x".into()
                }
            ),
            Err(EditError::OffsetOutOfRange)
        );
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertText {
                    at: Pos::new(n(404), 0),
                    text: "x".into()
                }
            ),
            Err(EditError::NodeNotFound)
        );
        // A cross-paragraph delete range is rejected.
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::DeleteText {
                    range: Range {
                        start: Pos::new(p, 0),
                        end: Pos::new(n(3), 1)
                    }
                }
            ),
            Err(EditError::CrossParagraph)
        );
        assert_eq!(text_of(&d, p), "abc", "no mutation on error");
    }

    #[test]
    fn delete_across_runs_removes_range_and_undo_restores_formatting() {
        // A formatted paragraph: "Hello" (bold) + "World" (normal). Deleting a range
        // that spans both runs must work (this is what a multi-paragraph selection's
        // tail/head reduces to) and undo must bring back the bold run's formatting —
        // the reason the inverse is `SetInlines`, not a plain-text `InsertText`.
        let bold = RunProperties {
            bold: Some(true),
            ..RunProperties::default()
        };
        let p = n(2);
        let mut d = doc(vec![BlockNode::Paragraph(Paragraph {
            id: p,
            properties: ParagraphProperties::default(),
            inlines: vec![
                InlineNode::Run(Run {
                    id: n(3),
                    properties: bold,
                    text: "Hello".into(),
                }),
                run(4, "World"),
            ],
        })]);
        let mut ids = IdGenerator::new(9);

        // [3, 7) = "lo" (from bold "Hello") + "Wo" (from "World") → "Helrld".
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 3),
                    end: Pos::new(p, 7),
                },
            },
        )
        .expect("multi-run delete succeeds");
        assert_eq!(text_of(&d, p), "Helrld");

        apply(&mut d, &mut ids, &inverse).expect("undo restores");
        assert_eq!(text_of(&d, p), "HelloWorld");
        let BlockNode::Paragraph(para) = &d.body()[0] else {
            panic!("paragraph");
        };
        let InlineNode::Run(first) = &para.inlines[0] else {
            panic!("run");
        };
        assert_eq!(
            first.properties.bold,
            Some(true),
            "undo restored the run's bold, not just its text"
        );
    }

    #[test]
    fn delete_whole_paragraph_text_leaves_it_empty() {
        // Deleting a paragraph's entire content (what a cross-paragraph selection does
        // to each whole middle paragraph before joining) empties its inlines cleanly.
        // (Single run — the model forbids adjacent equal-property runs, so "alphabeta"
        // is one run, not two.)
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "alphabeta")])]);
        let mut ids = IdGenerator::new(9);
        apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 0),
                    end: Pos::new(p, 9),
                },
            },
        )
        .expect("full delete");
        assert_eq!(text_of(&d, p), "");
    }

    #[test]
    fn format_to_match_neighbour_coalesces_to_stay_valid() {
        // "abc"(bold) + "def"(normal): bolding [3,6) makes the second run match the
        // first — which the model forbids as two adjacent equal-property runs. The
        // format must coalesce them into one bold run, and the document stay valid.
        let bold = RunProperties {
            bold: Some(true),
            ..RunProperties::default()
        };
        let p = n(2);
        let mut d = doc(vec![BlockNode::Paragraph(Paragraph {
            id: p,
            properties: ParagraphProperties::default(),
            inlines: vec![
                InlineNode::Run(Run {
                    id: n(3),
                    properties: bold,
                    text: "abc".into(),
                }),
                run(4, "def"),
            ],
        })]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::FormatText {
                range: Range {
                    start: Pos::new(p, 3),
                    end: Pos::new(p, 6),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    ..FormatDelta::default()
                },
            },
        )
        .expect("bold the second run");
        let BlockNode::Paragraph(para) = &d.body()[0] else {
            panic!("paragraph");
        };
        assert_eq!(para.inlines.len(), 1, "the two bold runs must coalesce");
        assert_eq!(text_of(&d, p), "abcdef");
        Document::new(
            n(1001),
            d.body().to_vec(),
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("stays valid after formatting");

        // Undo restores the original two-run structure exactly.
        apply(&mut d, &mut ids, &inverse).expect("undo");
        let BlockNode::Paragraph(para) = &d.body()[0] else {
            panic!("paragraph");
        };
        assert_eq!(para.inlines.len(), 2, "undo restores the split");
    }

    #[test]
    fn delete_between_equal_runs_coalesces_to_stay_valid() {
        // "a"(normal) "BOLD"(bold) "c"(normal): deleting the whole bold middle would
        // leave the two normal runs adjacent — which the model forbids. The delete
        // must coalesce them into one run so the document stays re-validatable.
        let bold = RunProperties {
            bold: Some(true),
            ..RunProperties::default()
        };
        let p = n(2);
        let mut d = doc(vec![BlockNode::Paragraph(Paragraph {
            id: p,
            properties: ParagraphProperties::default(),
            inlines: vec![
                run(3, "a"),
                InlineNode::Run(Run {
                    id: n(4),
                    properties: bold,
                    text: "BOLD".into(),
                }),
                run(5, "c"),
            ],
        })]);
        let mut ids = IdGenerator::new(9);

        // [1, 5) = the whole bold run.
        apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 1),
                    end: Pos::new(p, 5),
                },
            },
        )
        .expect("delete the middle run");
        assert_eq!(text_of(&d, p), "ac");

        let BlockNode::Paragraph(para) = &d.body()[0] else {
            panic!("paragraph");
        };
        assert_eq!(
            para.inlines.len(),
            1,
            "the two equal-property runs must coalesce into one"
        );
        // The whole document must still validate (the invariant we just protected).
        Document::new(
            n(1001),
            d.body().to_vec(),
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("document stays valid after the delete");
    }

    #[test]
    fn set_core_properties_applies_and_inverse_restores() {
        let mut d = doc(vec![para(2, vec![run(3, "text")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetCoreProperties {
                properties: Box::new(CoreProperties {
                    title: Some("Quarterly Report".to_string()),
                    creator: Some("Ada Lovelace".to_string()),
                    ..CoreProperties::default()
                }),
            },
        )
        .expect("core properties install");
        assert_eq!(
            d.properties().unwrap().core.title.as_deref(),
            Some("Quarterly Report")
        );

        // Inverse restores the empty starting state.
        apply(&mut d, &mut ids, &inverse).expect("undo restores previous properties");
        assert!(d.properties().is_none_or(|p| p.core.is_empty()));
    }

    #[test]
    fn set_core_properties_rejects_an_oversized_field_and_leaves_doc_unchanged() {
        let mut d = doc(vec![para(2, vec![run(3, "text")])]);
        let mut ids = IdGenerator::new(9);
        let huge = "x".repeat(5_000); // over MAX_META_TEXT (4096)

        let err = apply(
            &mut d,
            &mut ids,
            &Operation::SetCoreProperties {
                properties: Box::new(CoreProperties {
                    title: Some(huge),
                    ..CoreProperties::default()
                }),
            },
        )
        .unwrap_err();
        assert_eq!(err, EditError::ValueTooLarge);
        // No partial mutation survives an error.
        assert!(d.properties().is_none_or(|p| p.core.is_empty()));
    }

    #[test]
    fn scoped_review_state_applies_inverts_and_rolls_back_atomically() {
        let paragraph = n(2);
        let mut d = doc(vec![
            para(2, vec![run(3, "before")]),
            para(4, vec![run(5, "untouched")]),
        ]);
        let original = d.clone();
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::UpdateReviewState {
                paragraphs: vec![ReviewParagraphState {
                    node: paragraph,
                    inlines: vec![run(6, "after")],
                }],
                comments: None,
            },
        )
        .expect("scoped review update");
        assert_eq!(text_of(&d, paragraph), "after");
        assert_eq!(text_of(&d, n(4)), "untouched");
        assert!(matches!(
            &inverse,
            Operation::UpdateReviewState {
                paragraphs,
                comments: None,
            } if paragraphs.len() == 1 && paragraphs[0].node == paragraph
        ));

        apply(&mut d, &mut ids, &inverse).expect("exact review undo");
        assert_eq!(d, original);

        let invalid = Operation::UpdateReviewState {
            paragraphs: vec![ReviewParagraphState {
                node: paragraph,
                // Adjacent runs with equal properties violate the normalized
                // model invariant and must roll the entire operation back.
                inlines: vec![run(10, "a"), run(11, "b")],
            }],
            comments: None,
        };
        assert_eq!(
            apply(&mut d, &mut ids, &invalid),
            Err(EditError::ValueTooLarge)
        );
        assert_eq!(d, original, "failed review update leaves no partial state");
    }

    fn section(id: u64) -> SectionBoundary {
        SectionBoundary {
            id: SectionId::new(n(id)),
            page_size: PageSize {
                width_twips: 12_240,
                height_twips: 15_840,
            },
            page_margins: PageMargins {
                top_twips: 1_440,
                bottom_twips: 1_440,
                start_twips: 1_440,
                end_twips: 1_440,
                header_twips: None,
                footer_twips: None,
                gutter_twips: None,
            },
            columns: SectionColumns {
                count: 1,
                space_twips: None,
                separator: None,
                equal_width: None,
                columns: Vec::new(),
            },
            headers: Vec::new(),
            footers: Vec::new(),
            section_type: None,
            title_page: None,
            vertical_alignment: None,
            page_numbering: PageNumbering::default(),
            doc_grid: DocGrid::default(),
            orientation: None,
            paper_source: PaperSource::default(),
            page_borders: PageBorders::default(),
            line_numbering: LineNumbering::default(),
            footnote_props: NoteProperties::default(),
            endnote_props: NoteProperties::default(),
            text_direction: None,
            bidi: false,
            section_change: None,
        }
    }

    fn doc_with_section(paragraphs: Vec<BlockNode>, section_id: u64) -> Document {
        let mut definitions = Definitions::default();
        definitions.sections.push(section(section_id));
        Document::new(n(1000), paragraphs, definitions).expect("valid document")
    }

    #[test]
    fn set_section_geometry_applies_and_inverse_restores() {
        let mut d = doc_with_section(vec![para(2, vec![run(3, "text")])], 500);
        let mut ids = IdGenerator::new(9);
        let sid = SectionId::new(n(500));
        let previous_columns = d.definitions().sections[0].columns.clone();
        let columns = SectionColumns {
            count: 2,
            space_twips: Some(360),
            separator: Some(true),
            equal_width: Some(true),
            columns: Vec::new(),
        };

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetSectionGeometry {
                section: sid,
                page_size: PageSize {
                    width_twips: 15_840,
                    height_twips: 12_240,
                },
                page_margins: PageMargins {
                    top_twips: 720,
                    bottom_twips: 720,
                    start_twips: 720,
                    end_twips: 720,
                    header_twips: None,
                    footer_twips: None,
                    gutter_twips: None,
                },
                orientation: Some(PageOrientation::Landscape),
                columns,
            },
        )
        .expect("section geometry install");

        let installed = &d.definitions().sections[0];
        assert_eq!(installed.page_size.width_twips, 15_840);
        assert_eq!(installed.page_margins.top_twips, 720);
        assert_eq!(installed.orientation, Some(PageOrientation::Landscape));
        assert_eq!(installed.columns.count, 2);
        assert_eq!(installed.columns.space_twips, Some(360));
        assert_eq!(installed.columns.separator, Some(true));

        apply(&mut d, &mut ids, &inverse).expect("undo restores previous geometry");
        let restored = &d.definitions().sections[0];
        assert_eq!(restored.page_size.width_twips, 12_240);
        assert_eq!(restored.page_margins.top_twips, 1_440);
        assert_eq!(restored.orientation, None);
        assert_eq!(restored.columns, previous_columns);
    }

    #[test]
    fn set_section_geometry_rejects_an_oversized_page_and_leaves_doc_unchanged() {
        let mut d = doc_with_section(vec![para(2, vec![run(3, "text")])], 500);
        let mut ids = IdGenerator::new(9);
        let sid = SectionId::new(n(500));
        let columns = d.definitions().sections[0].columns.clone();

        let original_margins = d.definitions().sections[0].page_margins;
        let err = apply(
            &mut d,
            &mut ids,
            &Operation::SetSectionGeometry {
                section: sid,
                page_size: PageSize {
                    width_twips: 999_999, // over the ~22in (31_680 twip) domain bound
                    height_twips: 15_840,
                },
                page_margins: original_margins,
                orientation: None,
                columns,
            },
        )
        .unwrap_err();
        assert_eq!(err, EditError::ValueTooLarge);
        assert_eq!(d.definitions().sections[0].page_size.width_twips, 12_240);
    }

    #[test]
    fn set_section_geometry_rejects_an_unknown_section() {
        let mut d = doc_with_section(vec![para(2, vec![run(3, "text")])], 500);
        let mut ids = IdGenerator::new(9);
        let original = d.definitions().sections[0].clone();

        let err = apply(
            &mut d,
            &mut ids,
            &Operation::SetSectionGeometry {
                section: SectionId::new(n(999)),
                page_size: original.page_size,
                page_margins: original.page_margins,
                orientation: None,
                columns: original.columns.clone(),
            },
        )
        .unwrap_err();
        assert_eq!(err, EditError::NodeNotFound);
    }

    /// A minimal paragraph style carrying `run` overrides, for the style-op tests.
    fn paragraph_style(name: &str, run: Option<RunProperties>) -> Style {
        Style {
            kind: StyleKind::Paragraph,
            is_default: false,
            name: Some(name.to_owned()),
            aliases: None,
            based_on: None,
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: None,
            run,
            table: None,
            table_row: None,
            table_cell: None,
            conditional: Vec::new(),
        }
    }

    #[test]
    fn set_style_definition_updates_and_inverse_restores() {
        let mut definitions = Definitions::default();
        let sid = StyleId::new(n(700));
        definitions.styles.insert(
            sid,
            paragraph_style(
                "Body",
                Some(RunProperties {
                    size_half_points: Some(24),
                    ..RunProperties::default()
                }),
            ),
        );
        let mut d = Document::new(n(1000), vec![para(2, vec![run(3, "text")])], definitions)
            .expect("valid document");
        let mut ids = IdGenerator::new(9);

        // Redefine the style's run to bold 16pt (Word's "update to match selection").
        let updated = paragraph_style(
            "Body",
            Some(RunProperties {
                bold: Some(true),
                size_half_points: Some(32),
                ..RunProperties::default()
            }),
        );
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetStyleDefinition {
                id: sid,
                style: Some(Box::new(updated)),
            },
        )
        .expect("update style");

        let now = d.definitions().styles.get(&sid).expect("style present");
        assert_eq!(now.run.as_ref().unwrap().bold, Some(true));
        assert_eq!(now.run.as_ref().unwrap().size_half_points, Some(32));

        // The inverse restores the prior definition exactly.
        assert_eq!(
            inverse,
            Operation::SetStyleDefinition {
                id: sid,
                style: Some(Box::new(paragraph_style(
                    "Body",
                    Some(RunProperties {
                        size_half_points: Some(24),
                        ..RunProperties::default()
                    }),
                ))),
            }
        );
        apply(&mut d, &mut ids, &inverse).expect("undo restores style");
        let restored = d.definitions().styles.get(&sid).expect("style present");
        assert_eq!(restored.run.as_ref().unwrap().bold, None);
        assert_eq!(restored.run.as_ref().unwrap().size_half_points, Some(24));
    }

    #[test]
    fn set_style_definition_creates_and_inverse_removes() {
        let mut d = doc(vec![para(2, vec![run(3, "text")])]);
        let mut ids = IdGenerator::new(9);
        let sid = StyleId::new(n(800));
        assert!(!d.definitions().styles.contains_key(&sid));

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetStyleDefinition {
                id: sid,
                style: Some(Box::new(paragraph_style(
                    "Callout",
                    Some(RunProperties {
                        italic: Some(true),
                        ..RunProperties::default()
                    }),
                ))),
            },
        )
        .expect("create style");

        assert!(d.definitions().styles.contains_key(&sid));
        // Creating a fresh id inverts to removal (the id was absent before).
        assert_eq!(
            inverse,
            Operation::SetStyleDefinition {
                id: sid,
                style: None,
            }
        );
        apply(&mut d, &mut ids, &inverse).expect("undo removes the created style");
        assert!(!d.definitions().styles.contains_key(&sid));
    }

    /// REVIEW-GAP-030: the toolbar-reflection queries must descend into a
    /// pending tracked revision so selecting (or resting a caret in) suggested
    /// text reflects its real run formatting, not the paragraph default. Before
    /// the fix `run_properties_in_range`/`caret_run_properties` matched only a
    /// top-level `InlineNode::Run` and silently skipped the wrapped run.
    #[test]
    fn reflection_sees_formatting_inside_a_pending_revision() {
        // "Hi " (0..3) + a pending bold insertion "bold" (3..7) + " tail" (7..12).
        let bold_run = InlineNode::Run(Run {
            id: n(20),
            properties: RunProperties {
                bold: Some(true),
                ..RunProperties::default()
            },
            text: "bold".to_string(),
        });
        let insertion = InlineNode::Revision(Revision {
            id: n(21),
            kind: RevisionKind::Insertion,
            author: Some("Reviewer".to_owned()),
            date: None,
            revision_id: Some("21".to_owned()),
            editor_group: None,
            inlines: vec![bold_run],
        });
        let p = n(10);
        let document = doc(vec![para(
            10,
            vec![run(19, "Hi "), insertion, run(22, " tail")],
        )]);

        // A selection wholly inside the pending insertion.
        let inside = Range {
            start: Pos::new(p, 3),
            end: Pos::new(p, 7),
        };
        let covered = run_properties_in_range(&document, inside);
        assert_eq!(covered.len(), 1, "the wrapped run is now covered");
        assert_eq!(covered[0].bold, Some(true));
        assert!(
            format_state(&document, inside).bold,
            "the toolbar reflects bold for a selection inside a suggestion"
        );

        // A caret resting inside the pending insertion reflects it too.
        assert!(
            caret_format(&document, p, 5).bold,
            "the toolbar reflects bold at a caret inside a suggestion"
        );

        // A selection spanning plain + pending + plain text sees all three runs,
        // and is correctly reported as mixed (not uniformly bold).
        let whole = Range {
            start: Pos::new(p, 0),
            end: Pos::new(p, 12),
        };
        let across = run_properties_in_range(&document, whole);
        assert_eq!(across.len(), 3, "top-level and wrapped runs both covered");
        assert_eq!(
            across
                .iter()
                .filter(|props| props.bold == Some(true))
                .count(),
            1
        );
        assert!(
            !format_state(&document, whole).bold,
            "a mixed selection is not reported as uniformly bold"
        );

        // A rejected/zero-width deletion still contributes nothing to the
        // projected offsets, so plain-text reflection is unchanged.
        let plain = Range {
            start: Pos::new(p, 7),
            end: Pos::new(p, 12),
        };
        assert!(!format_state(&document, plain).bold);
    }

    // --- REVIEW-GAP-007: revision-aware range splitting (docs/86) -------------

    /// A hyperlink wrapping one run, for the wrapper-descent tests.
    fn hyperlink(id: u64, run_id: u64, text: &str) -> InlineNode {
        InlineNode::Hyperlink(Hyperlink {
            id: n(id),
            target: external("https://example.com/"),
            tooltip: None,
            inlines: vec![run(run_id, text)],
        })
    }

    /// The projected (final-with-markup) text of a paragraph's inlines, descending
    /// into hyperlinks, SDTs, and contributing revisions — so assertions can see a
    /// run nested inside a pending suggestion.
    fn deep_text(inlines: &[InlineNode]) -> String {
        let mut out = String::new();
        for inline in inlines {
            match inline {
                InlineNode::Run(run) => out.push_str(&run.text),
                InlineNode::Hyperlink(link) => out.push_str(&deep_text(&link.inlines)),
                InlineNode::Sdt(sdt) => out.push_str(&deep_text(&sdt.inlines)),
                InlineNode::Revision(revision)
                    if revision
                        .kind
                        .contributes_to(ReviewProjection::FinalWithMarkup) =>
                {
                    out.push_str(&deep_text(&revision.inlines));
                }
                _ => {}
            }
        }
        out
    }

    fn inlines_of(document: &Document, id: NodeId) -> Vec<InlineNode> {
        for block in document.body() {
            if let BlockNode::Paragraph(p) = block
                && p.id == id
            {
                return p.inlines.clone();
            }
        }
        panic!("paragraph not found");
    }

    #[test]
    fn typing_inside_a_pending_insertion_splices_into_the_suggestion() {
        // "AB" + «insertion "CD"» + "EF" → projected "ABCDEF". Typing "X" at offset
        // 3 (between C and D, inside the suggestion) must land inside that same
        // revision, not append a stray default-property run at the paragraph end.
        let p = n(2);
        let mut d = doc(vec![para(
            2,
            vec![
                run(3, "AB"),
                revision(10, 4, RevisionKind::Insertion, "CD"),
                run(5, "EF"),
            ],
        )]);
        let mut ids = IdGenerator::new(20);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 3),
                text: "X".into(),
            },
        )
        .expect("insert inside a suggestion succeeds");

        let after = inlines_of(&d, p);
        assert_eq!(deep_text(&after), "ABCXDEF");
        assert_eq!(after.len(), 3, "no stray top-level run was appended");
        let InlineNode::Revision(rev) = &after[1] else {
            panic!("the pending insertion is still a revision");
        };
        assert_eq!(
            deep_text(&rev.inlines),
            "CXD",
            "text went into the suggestion"
        );

        apply(&mut d, &mut ids, &inverse).expect("undo removes the typed char");
        assert_eq!(deep_text(&inlines_of(&d, p)), "ABCDEF");
    }

    #[test]
    fn deleting_partway_into_a_pending_insertion_keeps_the_suggestion() {
        // Delete a range that starts in normal text and ends inside a pending
        // insertion: the covered normal + suggested bytes go, the suggestion
        // survives around what remains, and the inverse restores it verbatim.
        let p = n(2);
        let original = vec![
            run(3, "AB"),
            revision(10, 4, RevisionKind::Insertion, "CD"),
            run(5, "EF"),
        ];
        let mut d = doc(vec![para(2, original.clone())]);
        let mut ids = IdGenerator::new(20);

        // [1, 3) = "B" (normal) + "C" (inside the insertion).
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 1),
                    end: Pos::new(p, 3),
                },
            },
        )
        .expect("delete spanning normal + pending text succeeds");

        let after = inlines_of(&d, p);
        assert_eq!(deep_text(&after), "ADEF");
        let InlineNode::Revision(rev) = &after[1] else {
            panic!("the insertion survived");
        };
        assert_eq!(deep_text(&rev.inlines), "D");

        apply(&mut d, &mut ids, &inverse).expect("undo");
        assert_eq!(
            inlines_of(&d, p),
            original,
            "inverse restores the tree exactly"
        );
    }

    #[test]
    fn deleting_the_whole_pending_insertion_prunes_the_empty_wrapper() {
        // Deleting exactly the suggested span removes the now-empty revision (a
        // wrapper's inlines must stay non-empty) and coalesces the two default
        // runs left adjacent across the gap; the inverse restores it exactly.
        let p = n(2);
        let original = vec![
            run(3, "AB"),
            revision(10, 4, RevisionKind::Insertion, "CD"),
            run(5, "EF"),
        ];
        let mut d = doc(vec![para(2, original.clone())]);
        let mut ids = IdGenerator::new(20);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 2),
                    end: Pos::new(p, 4),
                },
            },
        )
        .expect("delete the whole suggestion");

        let after = inlines_of(&d, p);
        assert_eq!(deep_text(&after), "ABEF");
        assert_eq!(
            after.len(),
            1,
            "the emptied revision was pruned and runs merged"
        );

        apply(&mut d, &mut ids, &inverse).expect("undo");
        assert_eq!(inlines_of(&d, p), original);
    }

    #[test]
    fn formatting_a_pending_suggestion_bolds_the_nested_run() {
        // Formatting a range that is a pending insertion applies to the nested run
        // (previously a silent no-op), and the inverse restores it exactly.
        let p = n(2);
        let original = vec![
            run(3, "AB"),
            revision(10, 4, RevisionKind::Insertion, "CD"),
            run(5, "EF"),
        ];
        let mut d = doc(vec![para(2, original.clone())]);
        let mut ids = IdGenerator::new(20);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::FormatText {
                range: Range {
                    start: Pos::new(p, 2),
                    end: Pos::new(p, 4),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    ..FormatDelta::default()
                },
            },
        )
        .expect("formatting a suggestion succeeds");

        let after = inlines_of(&d, p);
        let InlineNode::Revision(rev) = &after[1] else {
            panic!("still a revision");
        };
        let InlineNode::Run(nested) = &rev.inlines[0] else {
            panic!("nested run");
        };
        assert_eq!(
            nested.properties.bold,
            Some(true),
            "the suggestion is now bold"
        );

        apply(&mut d, &mut ids, &inverse).expect("undo");
        assert_eq!(inlines_of(&d, p), original);
    }

    #[test]
    fn formatting_across_a_hyperlink_boundary_splits_and_formats() {
        // A range crossing into and out of a hyperlink no longer fails: the outer
        // runs split at the boundaries and the hyperlink's nested run is formatted.
        let p = n(2);
        let original = vec![run(3, "AB"), hyperlink(10, 4, "CD"), run(5, "EF")];
        let mut d = doc(vec![para(2, original.clone())]);
        let mut ids = IdGenerator::new(20);

        // [1, 5) = "B" + "CD" (the whole link) + "E".
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::FormatText {
                range: Range {
                    start: Pos::new(p, 1),
                    end: Pos::new(p, 5),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    ..FormatDelta::default()
                },
            },
        )
        .expect("formatting across a hyperlink boundary succeeds");

        let after = inlines_of(&d, p);
        let link = after
            .iter()
            .find_map(|i| match i {
                InlineNode::Hyperlink(h) => Some(h),
                _ => None,
            })
            .expect("hyperlink preserved");
        let InlineNode::Run(nested) = &link.inlines[0] else {
            panic!("nested run");
        };
        assert_eq!(
            nested.properties.bold,
            Some(true),
            "link text was formatted"
        );
        assert_eq!(deep_text(&after), "ABCDEF", "no text was lost");

        apply(&mut d, &mut ids, &inverse).expect("undo");
        assert_eq!(inlines_of(&d, p), original);
    }

    #[test]
    fn splitting_a_paragraph_inside_a_hyperlink_splits_the_link() {
        // Pressing Enter inside a hyperlink splits it across the two paragraphs
        // instead of returning Unsupported; each half stays a hyperlink.
        let p = n(2);
        let new = n(50);
        let mut d = doc(vec![para(2, vec![hyperlink(10, 3, "ABCD")])]);
        let mut ids = IdGenerator::new(20);

        apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 2),
                new_id: new,
            },
        )
        .expect("split inside a hyperlink succeeds");

        assert_eq!(d.body().len(), 2, "one paragraph became two");
        let left = inlines_of(&d, p);
        let right = inlines_of(&d, new);
        assert_eq!(deep_text(&left), "AB");
        assert_eq!(deep_text(&right), "CD");
        assert!(
            matches!(left.first(), Some(InlineNode::Hyperlink(_)))
                && matches!(right.first(), Some(InlineNode::Hyperlink(_))),
            "each half is still a hyperlink"
        );
    }

    #[test]
    fn coalescing_never_merges_two_runs_across_a_surviving_revision() {
        // A revision between two equal-property runs is a semantic boundary:
        // normalizing after an edit must not merge them through it.
        let p = n(2);
        let mut d = doc(vec![para(
            2,
            vec![
                run(3, "A"),
                revision(10, 4, RevisionKind::Insertion, "B"),
                run(5, "C"),
            ],
        )]);
        let mut ids = IdGenerator::new(20);

        // Format just the middle (the suggestion); coalescing runs afterwards.
        apply(
            &mut d,
            &mut ids,
            &Operation::FormatText {
                range: Range {
                    start: Pos::new(p, 1),
                    end: Pos::new(p, 2),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    ..FormatDelta::default()
                },
            },
        )
        .expect("format the middle suggestion");

        let after = inlines_of(&d, p);
        assert_eq!(
            after.len(),
            3,
            "outer runs were not merged across the revision"
        );
        assert!(matches!(&after[0], InlineNode::Run(r) if r.text == "A"));
        assert!(matches!(&after[2], InlineNode::Run(r) if r.text == "C"));
    }

    // ---- Bookmarks ---------------------------------------------------------

    fn bkid(counter: u64) -> BookmarkId {
        BookmarkId::new(n(counter))
    }

    fn bookmark_name(document: &Document, bookmark: BookmarkId) -> Option<String> {
        document
            .definitions()
            .bookmarks
            .get(&bookmark)
            .map(|b| b.name.clone())
    }

    #[test]
    fn create_bookmark_wraps_the_range_and_inverse_removes_it_verbatim() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let original = d.clone();
        let mut ids = IdGenerator::new(20);
        let bookmark = bkid(50);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::CreateBookmark {
                bookmark,
                name: "anchor".to_owned(),
                start: Pos::new(p, 0),
                start_id: n(51),
                end: Pos::new(p, 5),
                end_id: n(52),
            },
        )
        .expect("create bookmark over the whole run");

        // Definition registered; markers wrap exactly the run.
        assert_eq!(bookmark_name(&d, bookmark).as_deref(), Some("anchor"));
        let after = inlines_of(&d, p);
        assert!(matches!(&after[0], InlineNode::BookmarkStart(m) if m.bookmark == bookmark));
        assert!(matches!(&after[1], InlineNode::Run(r) if r.text == "Hello"));
        assert!(matches!(&after[2], InlineNode::BookmarkEnd(m) if m.bookmark == bookmark));
        d.validate().expect("created bookmark validates");

        // Inverse removes the pair and the definition, restoring the doc verbatim.
        assert_eq!(
            inverse,
            Operation::DeleteBookmark { bookmark },
            "create inverts to a delete of the same id"
        );
        apply(&mut d, &mut ids, &inverse).expect("inverse removes the bookmark");
        assert_eq!(d, original, "create + inverse is a verbatim round-trip");
    }

    #[test]
    fn create_bookmark_splits_an_interior_range_and_round_trips() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let original = d.clone();
        let mut ids = IdGenerator::new(20);
        let bookmark = bkid(50);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::CreateBookmark {
                bookmark,
                name: "mid".to_owned(),
                start: Pos::new(p, 2),
                start_id: n(51),
                end: Pos::new(p, 4),
                end_id: n(52),
            },
        )
        .expect("create bookmark over an interior range");

        // "He" | <start> | "ll" | <end> | "o".
        let after = inlines_of(&d, p);
        assert!(matches!(&after[0], InlineNode::Run(r) if r.text == "He"));
        assert!(matches!(&after[1], InlineNode::BookmarkStart(_)));
        assert!(matches!(&after[2], InlineNode::Run(r) if r.text == "ll"));
        assert!(matches!(&after[3], InlineNode::BookmarkEnd(_)));
        assert!(matches!(&after[4], InlineNode::Run(r) if r.text == "o"));
        d.validate().expect("interior bookmark validates");

        apply(&mut d, &mut ids, &inverse).expect("inverse removes and re-coalesces");
        assert_eq!(
            d, original,
            "interior create + inverse restores the single run"
        );
    }

    #[test]
    fn delete_bookmark_inverse_reinserts_at_the_same_positions() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let mut ids = IdGenerator::new(20);
        let bookmark = bkid(50);

        apply(
            &mut d,
            &mut ids,
            &Operation::CreateBookmark {
                bookmark,
                name: "anchor".to_owned(),
                start: Pos::new(p, 0),
                start_id: n(51),
                end: Pos::new(p, 5),
                end_id: n(52),
            },
        )
        .expect("create bookmark");
        let with_bookmark = d.clone();

        let inverse = apply(&mut d, &mut ids, &Operation::DeleteBookmark { bookmark })
            .expect("delete the bookmark");
        assert!(bookmark_name(&d, bookmark).is_none(), "definition removed");
        assert!(
            !inlines_of(&d, p)
                .iter()
                .any(|i| matches!(i, InlineNode::BookmarkStart(_) | InlineNode::BookmarkEnd(_))),
            "both markers removed"
        );

        // The inverse is a create carrying the exact positions, ids, and name.
        assert_eq!(
            inverse,
            Operation::CreateBookmark {
                bookmark,
                name: "anchor".to_owned(),
                start: Pos::new(p, 0),
                start_id: n(51),
                end: Pos::new(p, 5),
                end_id: n(52),
            }
        );
        apply(&mut d, &mut ids, &inverse).expect("inverse re-inserts the bookmark");
        assert_eq!(
            d, with_bookmark,
            "delete + inverse is a verbatim round-trip"
        );
    }

    #[test]
    fn delete_bookmark_spanning_two_paragraphs_round_trips() {
        let a = n(2);
        let b = n(4);
        // Two paragraphs; the start marker sits after "Hi" in A, the end before
        // "there" in B (distinct-property flanks are not needed — different
        // paragraphs never coalesce across the boundary).
        let bookmark = bkid(50);
        let mut definitions = Definitions::default();
        definitions.bookmarks.insert(
            bookmark,
            Bookmark {
                name: "span".to_owned(),
            },
        );
        let mut d = Document::new(
            n(1000),
            vec![
                para(
                    2,
                    vec![
                        run(3, "Hi"),
                        InlineNode::BookmarkStart(BookmarkStart {
                            id: n(60),
                            bookmark,
                        }),
                    ],
                ),
                para(
                    4,
                    vec![
                        InlineNode::BookmarkEnd(BookmarkEnd {
                            id: n(61),
                            bookmark,
                        }),
                        run(5, "there"),
                    ],
                ),
            ],
            definitions,
        )
        .expect("cross-paragraph bookmark validates");
        let original = d.clone();
        let mut ids = IdGenerator::new(80);

        let inverse = apply(&mut d, &mut ids, &Operation::DeleteBookmark { bookmark })
            .expect("delete the cross-paragraph bookmark");
        assert!(bookmark_name(&d, bookmark).is_none());
        assert_eq!(
            inverse,
            Operation::CreateBookmark {
                bookmark,
                name: "span".to_owned(),
                start: Pos::new(a, 2),
                start_id: n(60),
                end: Pos::new(b, 0),
                end_id: n(61),
            }
        );
        apply(&mut d, &mut ids, &inverse).expect("inverse re-inserts across paragraphs");
        assert_eq!(d, original, "cross-paragraph delete + inverse round-trips");
    }

    #[test]
    fn rename_bookmark_inverse_restores_the_previous_name() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let mut ids = IdGenerator::new(20);
        let bookmark = bkid(50);
        apply(
            &mut d,
            &mut ids,
            &Operation::CreateBookmark {
                bookmark,
                name: "before".to_owned(),
                start: Pos::new(p, 0),
                start_id: n(51),
                end: Pos::new(p, 5),
                end_id: n(52),
            },
        )
        .expect("create bookmark");

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::RenameBookmark {
                bookmark,
                name: "after".to_owned(),
            },
        )
        .expect("rename the bookmark");
        assert_eq!(bookmark_name(&d, bookmark).as_deref(), Some("after"));
        assert_eq!(
            inverse,
            Operation::RenameBookmark {
                bookmark,
                name: "before".to_owned(),
            }
        );
        apply(&mut d, &mut ids, &inverse).expect("inverse restores the name");
        assert_eq!(bookmark_name(&d, bookmark).as_deref(), Some("before"));
    }

    #[test]
    fn create_bookmark_rejects_an_empty_or_oversized_name() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let original = d.clone();
        let mut ids = IdGenerator::new(20);

        let empty = apply(
            &mut d,
            &mut ids,
            &Operation::CreateBookmark {
                bookmark: bkid(50),
                name: String::new(),
                start: Pos::new(p, 0),
                start_id: n(51),
                end: Pos::new(p, 5),
                end_id: n(52),
            },
        );
        assert_eq!(empty, Err(EditError::InvalidName));

        let oversized = apply(
            &mut d,
            &mut ids,
            &Operation::CreateBookmark {
                bookmark: bkid(50),
                name: "x".repeat(256),
                start: Pos::new(p, 0),
                start_id: n(51),
                end: Pos::new(p, 5),
                end_id: n(52),
            },
        );
        assert_eq!(oversized, Err(EditError::InvalidName));
        assert_eq!(
            d, original,
            "a rejected create leaves the document untouched"
        );
    }

    #[test]
    fn rename_and_delete_reject_an_unknown_bookmark() {
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let mut ids = IdGenerator::new(20);
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::DeleteBookmark { bookmark: bkid(99) }
            ),
            Err(EditError::BookmarkNotFound)
        );
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::RenameBookmark {
                    bookmark: bkid(99),
                    name: "x".to_owned(),
                }
            ),
            Err(EditError::BookmarkNotFound)
        );
    }

    #[test]
    fn created_bookmark_survives_a_json_write_reopen() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let mut ids = IdGenerator::new(20);
        apply(
            &mut d,
            &mut ids,
            &Operation::CreateBookmark {
                bookmark: bkid(50),
                name: "anchor".to_owned(),
                start: Pos::new(p, 1),
                start_id: n(51),
                end: Pos::new(p, 4),
                end_id: n(52),
            },
        )
        .expect("create bookmark");

        let json = d.to_json().expect("serialize");
        let reopened = Document::from_json(&json, casual_doc_model::SnapshotLimits::default())
            .expect("reopen");
        assert_eq!(d, reopened, "the created bookmark survives write -> reopen");
    }

    // ---- Fields ------------------------------------------------------------

    /// The single top-level field in paragraph `id`, if any.
    fn field_of(document: &Document, id: NodeId) -> Option<Field> {
        inlines_of(document, id).into_iter().find_map(|inline| {
            if let InlineNode::Field(field) = inline {
                Some(field)
            } else {
                None
            }
        })
    }

    /// Applies `InsertField` for `common` at `at`, asserts the field lands with the
    /// expected instruction/kind, then asserts the returned inverse restores the
    /// paragraph verbatim and that re-applying the forward op is idempotent.
    fn round_trip_field(common: CommonField, instruction: &str) {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let original = d.clone();
        let mut ids = IdGenerator::new(20);
        let field = common.build(n(50), n(51));

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertField {
                at: Pos::new(p, 2),
                field: Box::new(field.clone()),
            },
        )
        .expect("insert field");

        let inserted = field_of(&d, p).expect("field present after insert");
        assert_eq!(inserted.instruction, instruction, "instruction string");
        assert_eq!(
            inserted.kind,
            FieldKind::parse(instruction),
            "kind projection agrees with the instruction"
        );
        assert_eq!(inverse, Operation::RemoveField { field: n(50) });
        // The caret text is unchanged: a field is zero-width in the anchor space.
        assert_eq!(text_of(&d, p), "Hello");

        // The inverse removes the field and restores the paragraph exactly.
        let redo = apply(&mut d, &mut ids, &inverse).expect("remove field");
        assert!(field_of(&d, p).is_none(), "field gone after inverse");
        assert_eq!(d, original, "inverse restores the document verbatim");

        // The inverse-of-the-inverse re-inserts an equal field.
        apply(&mut d, &mut ids, &redo).expect("re-insert field");
        assert_eq!(
            field_of(&d, p).expect("field back after redo").instruction,
            instruction,
        );
    }

    #[test]
    fn insert_field_round_trips_for_each_common_kind() {
        round_trip_field(CommonField::Page, "PAGE \\* MERGEFORMAT");
        round_trip_field(CommonField::NumPages, "NUMPAGES \\* MERGEFORMAT");
        round_trip_field(
            CommonField::Date {
                format: None,
                result: "1/2/2026".to_owned(),
            },
            "DATE",
        );
        round_trip_field(
            CommonField::Date {
                format: Some("M/d/yyyy".to_owned()),
                result: "1/2/2026".to_owned(),
            },
            "DATE \\@ \"M/d/yyyy\"",
        );
        round_trip_field(
            CommonField::Time {
                format: None,
                result: "3:04 PM".to_owned(),
            },
            "TIME",
        );
        round_trip_field(
            CommonField::FileName {
                result: "report.docx".to_owned(),
            },
            "FILENAME \\* MERGEFORMAT",
        );
        round_trip_field(
            CommonField::Author {
                result: "Ada".to_owned(),
            },
            "AUTHOR \\* MERGEFORMAT",
        );
    }

    #[test]
    fn page_and_numpages_seed_a_recomputable_placeholder() {
        // The pagination field pass overwrites these, but the seeded cached value
        // renders before that pass — a `"1"` placeholder, not empty.
        let page = CommonField::Page.build(n(50), n(51));
        assert!(
            matches!(page.inlines.as_slice(), [InlineNode::Run(r)] if r.text == "1"),
            "PAGE seeds a \"1\" placeholder run",
        );
        assert_eq!(page.kind, FieldKind::Page);
        let total = CommonField::NumPages.build(n(52), n(53));
        assert_eq!(total.kind, FieldKind::NumPages);
    }

    #[test]
    fn date_field_caches_the_caller_supplied_display_text() {
        // The engine reads no clock: the formatted value arrives as a parameter and
        // is cached verbatim as the field's leaf run.
        let field = CommonField::Date {
            format: Some("M/d/yyyy".to_owned()),
            result: "8/6/2026".to_owned(),
        }
        .build(n(50), n(51));
        assert!(matches!(field.inlines.as_slice(), [InlineNode::Run(r)] if r.text == "8/6/2026"),);
    }

    #[test]
    fn insert_field_into_an_empty_paragraph_leaves_only_the_field() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![])]);
        let mut ids = IdGenerator::new(20);
        apply(
            &mut d,
            &mut ids,
            &Operation::InsertField {
                at: Pos::new(p, 0),
                field: Box::new(CommonField::Page.build(n(50), n(51))),
            },
        )
        .expect("insert into empty paragraph");
        assert!(matches!(
            inlines_of(&d, p).as_slice(),
            [InlineNode::Field(_)]
        ));
    }

    #[test]
    fn insert_field_rejects_a_missing_node_and_out_of_range_offset() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hi")])]);
        let mut ids = IdGenerator::new(20);
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertField {
                    at: Pos::new(n(999), 0),
                    field: Box::new(CommonField::Page.build(n(50), n(51))),
                },
            ),
            Err(EditError::NodeNotFound),
        );
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertField {
                    at: Pos::new(p, 99),
                    field: Box::new(CommonField::Page.build(n(50), n(51))),
                },
            ),
            Err(EditError::OffsetOutOfRange),
        );
    }

    #[test]
    fn remove_field_rejects_an_unknown_id() {
        let mut d = doc(vec![para(2, vec![run(3, "Hi")])]);
        let mut ids = IdGenerator::new(20);
        assert_eq!(
            apply(&mut d, &mut ids, &Operation::RemoveField { field: n(777) }),
            Err(EditError::FieldNotFound),
        );
    }

    #[test]
    fn inserted_field_survives_write_reopen() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let mut ids = IdGenerator::new(20);
        apply(
            &mut d,
            &mut ids,
            &Operation::InsertField {
                at: Pos::new(p, 2),
                field: Box::new(
                    CommonField::Date {
                        format: Some("M/d/yyyy".to_owned()),
                        result: "8/6/2026".to_owned(),
                    }
                    .build(n(50), n(51)),
                ),
            },
        )
        .expect("insert field");

        let json = d.to_json().expect("serialize");
        let reopened = Document::from_json(&json, casual_doc_model::SnapshotLimits::default())
            .expect("reopen");
        assert_eq!(d, reopened, "the inserted field survives write -> reopen");
    }

    // ---- Footnotes / endnotes ----------------------------------------------

    /// A single empty paragraph — the ready-to-type body of a freshly inserted
    /// note. `id` comes from the edit id source so it stays globally unique.
    fn empty_note_body(id: NodeId) -> Vec<BlockNode> {
        vec![BlockNode::Paragraph(Paragraph {
            id,
            properties: ParagraphProperties::default(),
            inlines: Vec::new(),
        })]
    }

    fn note_references_in(document: &Document, id: NodeId) -> usize {
        inlines_of(document, id)
            .into_iter()
            .filter(|inline| matches!(inline, InlineNode::NoteReference(_)))
            .count()
    }

    #[test]
    fn insert_footnote_creates_reference_and_definition_and_inverse_removes_both() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let original = d.clone();
        let mut ids = IdGenerator::new(9);

        let note = NoteId::new(ids.next_id().unwrap());
        let reference_id = ids.next_id().unwrap();
        let body = empty_note_body(ids.next_id().unwrap());

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertNote {
                kind: NoteKind::Footnote,
                note,
                at: Pos::new(p, 2), // mid-run: "He|llo"
                reference_id,
                blocks: body,
            },
        )
        .expect("insert footnote");

        // The definition is created (in the footnotes map, not endnotes) with an
        // empty, ready-to-type body; the reference is spliced without changing the
        // paragraph's shaped text (a note reference is zero-width).
        assert_eq!(d.definitions().footnotes.len(), 1);
        assert!(d.definitions().footnotes.contains_key(&note));
        assert!(d.definitions().endnotes.is_empty());
        assert_eq!(note_references_in(&d, p), 1);
        assert_eq!(text_of(&d, p), "Hello");
        d.validate()
            .expect("document is valid after inserting a footnote");

        // The inverse removes both the reference and the definition, restoring the
        // document verbatim (the split run coalesces back to the original).
        assert!(matches!(
            inverse,
            Operation::RemoveNote {
                kind: NoteKind::Footnote,
                note: inverse_note,
                reference_id: inverse_ref,
            } if inverse_note == note && inverse_ref == reference_id
        ));
        apply(&mut d, &mut ids, &inverse).expect("remove footnote (inverse)");
        assert_eq!(d, original, "the inverse restored the original document");
    }

    #[test]
    fn insert_endnote_creates_reference_and_definition_and_inverse_removes_both() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello")])]);
        let original = d.clone();
        let mut ids = IdGenerator::new(9);

        let note = NoteId::new(ids.next_id().unwrap());
        let reference_id = ids.next_id().unwrap();
        let body = empty_note_body(ids.next_id().unwrap());

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertNote {
                kind: NoteKind::Endnote,
                note,
                at: Pos::new(p, 5), // paragraph end
                reference_id,
                blocks: body,
            },
        )
        .expect("insert endnote");

        assert_eq!(d.definitions().endnotes.len(), 1);
        assert!(d.definitions().endnotes.contains_key(&note));
        assert!(d.definitions().footnotes.is_empty());
        assert_eq!(note_references_in(&d, p), 1);
        d.validate()
            .expect("document is valid after inserting an endnote");

        assert!(matches!(
            inverse,
            Operation::RemoveNote {
                kind: NoteKind::Endnote,
                ..
            }
        ));
        apply(&mut d, &mut ids, &inverse).expect("remove endnote (inverse)");
        assert_eq!(d, original, "the inverse restored the original document");
    }

    #[test]
    fn insert_note_into_an_empty_paragraph_and_inverse_round_trips() {
        let p = n(2);
        let mut d = doc(vec![para(2, Vec::new())]);
        let original = d.clone();
        let mut ids = IdGenerator::new(9);

        let note = NoteId::new(ids.next_id().unwrap());
        let reference_id = ids.next_id().unwrap();
        let body = empty_note_body(ids.next_id().unwrap());

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertNote {
                kind: NoteKind::Footnote,
                note,
                at: Pos::new(p, 0),
                reference_id,
                blocks: body,
            },
        )
        .expect("insert footnote into an empty paragraph");
        assert_eq!(note_references_in(&d, p), 1);

        apply(&mut d, &mut ids, &inverse).expect("remove footnote (inverse)");
        assert_eq!(d, original, "the inverse restored the empty paragraph");
    }

    #[test]
    fn insert_note_rejects_an_out_of_range_caret() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hi")])]);
        let before = d.clone();
        let mut ids = IdGenerator::new(9);
        let note = NoteId::new(ids.next_id().unwrap());
        let reference_id = ids.next_id().unwrap();
        let body = empty_note_body(ids.next_id().unwrap());

        let result = apply(
            &mut d,
            &mut ids,
            &Operation::InsertNote {
                kind: NoteKind::Footnote,
                note,
                at: Pos::new(p, 99),
                reference_id,
                blocks: body,
            },
        );
        assert_eq!(result, Err(EditError::OffsetOutOfRange));
        assert_eq!(d, before, "a rejected insert leaves the document unchanged");
    }

    #[test]
    fn insert_note_rejects_a_duplicate_note_id() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hi")])]);
        let mut ids = IdGenerator::new(9);
        let note = NoteId::new(ids.next_id().unwrap());
        let first_ref = ids.next_id().unwrap();
        let first_body = empty_note_body(ids.next_id().unwrap());

        apply(
            &mut d,
            &mut ids,
            &Operation::InsertNote {
                kind: NoteKind::Footnote,
                note,
                at: Pos::new(p, 0),
                reference_id: first_ref,
                blocks: first_body,
            },
        )
        .expect("first footnote inserted");
        let after_first = d.clone();

        let second_ref = ids.next_id().unwrap();
        let second_body = empty_note_body(ids.next_id().unwrap());
        let result = apply(
            &mut d,
            &mut ids,
            &Operation::InsertNote {
                kind: NoteKind::Footnote,
                note, // same id — must be refused
                at: Pos::new(p, 2),
                reference_id: second_ref,
                blocks: second_body,
            },
        );
        assert_eq!(result, Err(EditError::Unsupported));
        assert_eq!(
            d, after_first,
            "a rejected duplicate-id insert leaves the document unchanged"
        );
    }

    #[test]
    fn inserted_footnote_survives_a_write_reopen_round_trip() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Body")])]);
        let mut ids = IdGenerator::new(9);
        let note = NoteId::new(ids.next_id().unwrap());
        let reference_id = ids.next_id().unwrap();
        let body_para = ids.next_id().unwrap();

        apply(
            &mut d,
            &mut ids,
            &Operation::InsertNote {
                kind: NoteKind::Footnote,
                note,
                at: Pos::new(p, 4),
                reference_id,
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: body_para,
                    properties: ParagraphProperties::default(),
                    inlines: vec![run(700, "footnote text")],
                })],
            },
        )
        .expect("insert footnote");

        let bytes = d.to_json().expect("serialize the document");
        let reopened = Document::from_json(&bytes, casual_doc_model::SnapshotLimits::default())
            .expect("reopen the document");
        assert_eq!(d, reopened, "m1 == m2 after a write -> reopen round trip");
        assert!(
            reopened.definitions().footnotes.contains_key(&note),
            "the created footnote survived the round trip"
        );
    }
}
