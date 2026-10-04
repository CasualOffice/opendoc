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
