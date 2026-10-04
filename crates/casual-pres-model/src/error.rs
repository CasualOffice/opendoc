// SPDX-License-Identifier: Apache-2.0

//! Presentation model construction and validation failures.

use std::error::Error;
use std::fmt;

use casual_doc_model::{ModelError, NodeId};

use crate::{PlaceholderKind, SlideId, SlideLayoutId};

/// Which axis of the slide size a domain failure was charged to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlideAxis {
    /// `p:sldSz@cx`.
    Width,
    /// `p:sldSz@cy`.
    Height,
}

impl fmt::Display for SlideAxis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Width => "width",
            Self::Height => "height",
        })
    }
}

/// Which axis of a table a merge or size failure was charged to.
///
/// A table's two merge axes are validated by ONE routine run twice, so the axis
/// has to be a value rather than two near-identical error variants — which is
/// also what stops a failure being reported as the wrong axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableAxis {
    /// Rows: `a:tc@rowSpan` and `a:tc@vMerge`, and `a:tr` count.
    Row,
    /// Grid columns: `a:tc@gridSpan` and `a:tc@hMerge`, and `a:gridCol` count.
    Column,
}

impl fmt::Display for TableAxis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Row => "row",
            Self::Column => "column",
        })
    }
}

/// Which of a shape's two paintable properties a [`crate::SlidePaint`] failure was
/// charged to.
///
/// A value rather than two near-identical error variants, for the reason
/// [`TableAxis`] is one: the two properties are validated by one routine, so the
/// axis has to be data or the failure can be reported as the wrong half.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaintProperty {
    /// The fill: `a:solidFill` and its siblings, or `<a:noFill/>`.
    Fill,
    /// The outline: `a:ln`.
    Outline,
}

impl fmt::Display for PaintProperty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Fill => "fill",
            Self::Outline => "outline",
        })
    }
}

/// Presentation model construction or validation failure.
///
/// Deliberately a separate type from [`ModelError`] rather than more variants on it:
/// the two document classes are independent (ADR-055), so a presentation failure must
/// not widen the error a DOCX caller matches on. Failures arising from the reused
/// DrawingML and definition types are carried through [`PresentationError::Model`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentationError {
    /// A deserialized schema version is not supported.
    UnsupportedSchemaVersion(u32),
    /// A presentation carried no slides.
    ///
    /// `p:sldIdLst` is formally optional, but PowerPoint cannot open a package with
    /// an empty one — there is no first slide to show — so an empty presentation is
    /// refused at construction rather than written out and found unopenable.
    EmptyPresentation,
    /// A slide dimension fell outside `ST_SlideSizeCoordinate`.
    SlideSizeOutOfDomain {
        /// Which dimension.
        axis: SlideAxis,
        /// The offending value, in EMU.
        value: i64,
    },
    /// A node ID appeared more than once anywhere in the presentation.
    DuplicateNodeId(NodeId),
    /// A slide's layout reference did not resolve.
    DanglingLayoutRef(SlideId),
    /// A layout's master reference did not resolve.
    DanglingMasterRef(SlideLayoutId),
    /// A media reference did not resolve in `Definitions::media`.
    DanglingMediaRef(NodeId),
    /// A colour-mapping entry was keyed by a part the presentation does not hold.
    ///
    /// Refused rather than ignored, because the failure mode is a wrong answer and
    /// not a missing one: `ColorMapping::in_force` falls through to the tier above,
    /// so a dangling key resolves `tx1` to the wrong theme slot and repaints the
    /// slide with nothing reporting it.
    DanglingColorMapRef(NodeId),
    /// A shape tree had no root transform child space to position children in —
    /// `a:chExt` was zero on an axis while the tree has children.
    ///
    /// Children are mapped from the child space into the parent box by ratio, so a
    /// zero child extent collapses every child to a point. Word writes a non-zero
    /// `a:chExt` whenever a group has children; a zero one is a malformed package,
    /// and silently painting a stack of zero-size shapes would look deliberate.
    DegenerateChildSpace(NodeId),
    /// Two shapes in one tree claimed the same placeholder slot.
    ///
    /// Inheritance resolves a slide placeholder against its layout by `(type, idx)`,
    /// so two shapes sharing a slot make the inherited position, size and text
    /// properties ambiguous — there is no rule that picks a winner.
    DuplicatePlaceholder {
        /// The shape tree the collision is in.
        tree: NodeId,
        /// The contested placeholder type.
        kind: PlaceholderKind,
        /// The contested `p:ph@idx`.
        index: u32,
    },
    /// One shape tree carried more than one title placeholder.
    ///
    /// `title` and `ctrTitle` are both titles, so either repeated — or one of each —
    /// is refused. A deliberate divergence worth naming: PowerPoint's own object
    /// model exposes a slide's title as a single `Shapes.Title`, so a second title is
    /// not addressable there either.
    DuplicateTitlePlaceholder(NodeId),
    /// A shape said its fill or outline was explicitly nothing
    /// ([`crate::SlidePaint::Suppressed`]) while the drawing carried a value for
    /// it.
    ///
    /// Refused rather than resolved by precedence, because there is no file this
    /// model could be a reading of: `p:spPr`'s fill is a schema CHOICE, so a shape
    /// states `<a:noFill/>` or it states a fill, never both. A painter reaching
    /// this pair would have to invent a rule, and an invented rule is how the
    /// fourth state a `bool` beside an `Option` admits gets quietly settled two
    /// different ways in two crates.
    SuppressedPaintCarriesValue {
        /// The offending shape.
        shape: NodeId,
        /// Which property disagreed with the drawing.
        property: PaintProperty,
    },
    /// A DrawingML group nested deeper than the supported bound.
    GroupNestingTooDeep(NodeId),
    /// A slide carried a `GroupChild::TextBox`, the document model's text-box
    /// shape, instead of putting its text in `SlideNode::text`.
    ///
    /// Refused deliberately rather than tolerated. `GroupTextBox` holds
    /// `Vec<BlockNode>` — WordprocessingML paragraphs — and slide text is `a:txBody`,
    /// whose paragraphs carry an outline level, an inline bullet and a nine-level
    /// list style that `w:p` cannot express. It was the interim carrier while
    /// [`crate::TextBody`] did not exist; refusing it now is what makes the interim
    /// state unreachable instead of merely discouraged.
    TextBoxShapeOnSlide(NodeId),
    /// An `a:txBody` carried no `a:p`. PowerPoint writes an empty paragraph for
    /// empty text rather than omitting it, so zero paragraphs is malformed.
    EmptyTextBody,
    /// A text run's `a:t` was empty.
    EmptyTextRun(NodeId),
    /// An `a:lstStyle` declared more than the nine levels DrawingML defines.
    TooManyTextLevels(usize),
    /// An `a:pPr@lvl` was above the eight the schema permits (it is zero-based).
    TextLevelOutOfRange(u8),
    /// An `a:pPr` margin or indent fell outside `ST_TextMargin`/`ST_TextIndent`.
    TextMarginOutOfDomain(i64),
    /// An `a:rPr@sz` fell outside `ST_TextFontSize` (1pt to 4000pt, in hundredths).
    ///
    /// Worth a named failure rather than a clamp: `w:sz` is in HALF-points and
    /// `a:rPr@sz` in HUNDREDTHS, so a value carried across without conversion is
    /// wrong by fifty and this is the check that catches it.
    FontSizeOutOfDomain(u32),
    /// An `a:rPr@spc` fell outside `ST_TextPoint`.
    TextSpacingOutOfDomain(i32),
    /// An `a:tbl` declared no `a:tblGrid` columns.
    ///
    /// There is no width anywhere else in an `a:tbl` — the frame's `p:xfrm` is the
    /// frame's, not the grid's — so a table with no columns has no geometry at all
    /// and would place every cell at zero width.
    EmptyTableGrid(NodeId),
    /// A table declared more rows or grid columns than the model bounds.
    TableTooLarge {
        /// The offending table.
        table: NodeId,
        /// Which axis overflowed.
        axis: TableAxis,
        /// The declared count.
        count: usize,
    },
    /// A row's cell count did not match the grid's column count.
    ///
    /// PresentationML requires one `a:tc` per `a:gridCol`, continuations included,
    /// because that is what makes a merge's coverage computable by position. A
    /// short row shifts every cell after it into the wrong column.
    TableRowWidthMismatch {
        /// The table the row is in.
        table: NodeId,
        /// The offending row.
        row: NodeId,
        /// The `a:tc` count found.
        cells: usize,
        /// The `a:gridCol` count expected.
        columns: usize,
    },
    /// A merge origin declared a span below two.
    ///
    /// `@gridSpan="1"` is not a merge, it is the default, and admitting it would
    /// make an unmerged cell and a one-cell "merge" two models of one fact.
    CellSpanOutOfDomain {
        /// The offending cell.
        cell: NodeId,
        /// The declared span.
        span: u32,
    },
    /// A continuation cell (`@hMerge`/`@vMerge`) had no origin covering it.
    UnanchoredCellMerge {
        /// The offending cell.
        cell: NodeId,
        /// Which axis the continuation was on.
        axis: TableAxis,
    },
    /// A merge origin's span ran over a cell that is not its continuation, or past
    /// the edge of the row or column.
    OverlappingCellMerge {
        /// The cell the overlap was detected at.
        cell: NodeId,
        /// Which axis the overlap is on.
        axis: TableAxis,
    },
    /// An `a:gridCol@w` or `a:tr@h` was negative.
    TableMeasureOutOfDomain(i64),
    /// Two `a:tblStyle` entries declared the same GUID.
    DuplicateTableStyleId(String),
    /// A failure in a reused document-model type.
    Model(ModelError),
}

impl From<ModelError> for PresentationError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

impl fmt::Display for PresentationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "unsupported presentation schema version {version}"
                )
            }
            Self::EmptyPresentation => {
                formatter.write_str("presentation must carry at least one slide")
            }
            Self::SlideSizeOutOfDomain { axis, value } => {
                write!(formatter, "slide {axis} {value} EMU is out of domain")
            }
            Self::DuplicateNodeId(id) => write!(formatter, "duplicate node ID {id}"),
            Self::DanglingLayoutRef(slide) => {
                write!(
                    formatter,
                    "slide {} layout does not resolve",
                    slide.node_id()
                )
            }
            Self::DanglingMasterRef(layout) => {
                write!(
                    formatter,
                    "layout {} master does not resolve",
                    layout.node_id()
                )
            }
            Self::DanglingMediaRef(id) => {
                write!(formatter, "media reference {id} does not resolve")
            }
            Self::DanglingColorMapRef(id) => {
                write!(formatter, "colour map is keyed by unknown part {id}")
            }
            Self::DegenerateChildSpace(id) => {
                write!(formatter, "shape tree {id} has a zero child extent")
            }
            Self::DuplicatePlaceholder { tree, kind, index } => {
                write!(
                    formatter,
                    "shape tree {tree} has two {} placeholders at index {index}",
                    kind.token()
                )
            }
            Self::DuplicateTitlePlaceholder(id) => {
                write!(formatter, "shape tree {id} has more than one title")
            }
            Self::SuppressedPaintCarriesValue { shape, property } => write!(
                formatter,
                "shape {shape} states no {property} and carries one"
            ),
            Self::GroupNestingTooDeep(id) => {
                write!(
                    formatter,
                    "group {id} nests deeper than the supported bound"
                )
            }
            Self::TextBoxShapeOnSlide(id) => write!(
                formatter,
                "shape {id} is a document text box; slide text belongs in SlideNode::text"
            ),
            Self::EmptyTextBody => {
                formatter.write_str("a text body must carry at least one paragraph")
            }
            Self::EmptyTextRun(id) => write!(formatter, "text run {id} is empty"),
            Self::TooManyTextLevels(count) => {
                write!(formatter, "list style declares {count} levels, above nine")
            }
            Self::TextLevelOutOfRange(level) => {
                write!(formatter, "outline level {level} is above eight")
            }
            Self::TextMarginOutOfDomain(value) => {
                write!(formatter, "text margin {value} EMU is out of domain")
            }
            Self::FontSizeOutOfDomain(size) => write!(
                formatter,
                "font size {size} hundredths of a point is out of domain"
            ),
            Self::TextSpacingOutOfDomain(value) => write!(
                formatter,
                "letter spacing {value} hundredths of a point is out of domain"
            ),
            Self::EmptyTableGrid(id) => {
                write!(formatter, "table {id} declares no grid columns")
            }
            Self::TableTooLarge { table, axis, count } => {
                write!(formatter, "table {table} declares {count} {axis}s")
            }
            Self::TableRowWidthMismatch {
                table,
                row,
                cells,
                columns,
            } => write!(
                formatter,
                "table {table} row {row} has {cells} cells for {columns} grid columns"
            ),
            Self::CellSpanOutOfDomain { cell, span } => {
                write!(formatter, "cell {cell} declares a span of {span}")
            }
            Self::UnanchoredCellMerge { cell, axis } => write!(
                formatter,
                "cell {cell} continues a {axis} merge that no origin covers"
            ),
            Self::OverlappingCellMerge { cell, axis } => write!(
                formatter,
                "cell {cell} is inside a {axis} merge that already covers it"
            ),
            Self::TableMeasureOutOfDomain(value) => {
                write!(formatter, "table measure {value} EMU is negative")
            }
            Self::DuplicateTableStyleId(id) => {
                write!(formatter, "two table styles declare the id {id}")
            }
            Self::Model(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for PresentationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Model(error) => Some(error),
            _ => None,
        }
    }
}
