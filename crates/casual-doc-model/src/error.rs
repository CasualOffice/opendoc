//! Normalized model construction and validation failures.

use std::error::Error;
use std::fmt;

use crate::NodeId;

/// Normalized model construction or validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelError {
    /// A node ID was zero.
    ZeroNodeId,
    /// A serialized node ID did not use the required representation.
    InvalidNodeId,
    /// An ID namespace exhausted its counter.
    IdSpaceExhausted,
    /// A deserialized schema version is not supported.
    UnsupportedSchemaVersion(u32),
    /// A document contained no body blocks.
    EmptyDocumentBody,
    /// A node ID appeared more than once.
    DuplicateNodeId(NodeId),
    /// A text run was empty.
    EmptyTextRun,
    /// Equivalent neighboring text runs were not normalized.
    AdjacentEquivalentTextRuns(NodeId),
    /// A grapheme offset was outside a paragraph.
    InvalidGraphemeOffset {
        /// Requested boundary.
        offset: usize,
        /// Paragraph grapheme length.
        length: usize,
    },
    /// A grapheme range was empty or reversed.
    InvalidGraphemeRange {
        /// Start boundary.
        start: usize,
        /// End boundary.
        end: usize,
    },
    /// A paragraph ID did not resolve.
    UnknownParagraph(NodeId),
    /// Two paragraphs were not adjacent in the required order.
    ParagraphsNotAdjacent {
        /// First requested paragraph.
        first: NodeId,
        /// Second requested paragraph.
        second: NodeId,
    },
    /// A paragraph length exceeded public position representation.
    GraphemeCountOverflow(NodeId),
    /// A style, numbering, media, or section reference did not resolve (v1).
    DanglingStyleRef(NodeId),
    /// A paragraph numbering instance reference did not resolve (v1).
    DanglingNumberingRef(NodeId),
    /// A numbering instance's abstract reference did not resolve (v1).
    DanglingAbstractNumberingRef(NodeId),
    /// A media reference did not resolve (v1).
    DanglingMediaRef(NodeId),
    /// A section reference did not resolve (v1).
    DanglingSectionRef(NodeId),
    /// A numbering level was referenced but not defined (v1).
    NumberingLevelUndefined {
        /// Referencing node.
        reference: NodeId,
        /// Missing level.
        level: u8,
    },
    /// A style `based_on` chain formed a cycle (v1).
    StyleBasedOnCycle(NodeId),
    /// A style inherited from a style of a different kind (v1).
    StyleBasedOnKindMismatch {
        /// The inheriting style.
        style: NodeId,
        /// The referenced base style.
        based_on: NodeId,
    },
    /// A measured property value fell outside its declared domain (v1).
    PropertyValueOutOfDomain {
        /// Stable property name.
        property: &'static str,
    },
    /// A grapheme offset fell outside its node's text (v1).
    GraphemeOffsetOutOfRange {
        /// Owning node.
        node: NodeId,
        /// Requested offset.
        offset: u32,
        /// Node grapheme length.
        length: u32,
    },
    /// A v0 source with a non-empty extension map cannot migrate to v1.
    UnsupportedV0Extensions,
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroNodeId => formatter.write_str("node ID must be non-zero"),
            Self::InvalidNodeId => formatter.write_str("node ID must be 32 lowercase hex digits"),
            Self::IdSpaceExhausted => formatter.write_str("node ID counter is exhausted"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported schema version {version}")
            }
            Self::EmptyDocumentBody => formatter.write_str("document body must not be empty"),
            Self::DuplicateNodeId(id) => write!(formatter, "duplicate node ID {id}"),
            Self::EmptyTextRun => formatter.write_str("text runs must not be empty"),
            Self::AdjacentEquivalentTextRuns(id) => {
                write!(formatter, "paragraph {id} has non-normalized text runs")
            }
            Self::InvalidGraphemeOffset { offset, length } => {
                write!(
                    formatter,
                    "grapheme offset {offset} exceeds length {length}"
                )
            }
            Self::InvalidGraphemeRange { start, end } => {
                write!(formatter, "grapheme range {start}..{end} is invalid")
            }
            Self::UnknownParagraph(id) => write!(formatter, "paragraph {id} does not exist"),
            Self::ParagraphsNotAdjacent { first, second } => {
                write!(
                    formatter,
                    "paragraphs {first} and {second} are not adjacent"
                )
            }
            Self::GraphemeCountOverflow(id) => {
                write!(
                    formatter,
                    "paragraph {id} exceeds addressable grapheme count"
                )
            }
            Self::DanglingStyleRef(id) => {
                write!(formatter, "style reference {id} does not resolve")
            }
            Self::DanglingNumberingRef(id) => {
                write!(formatter, "numbering reference {id} does not resolve")
            }
            Self::DanglingAbstractNumberingRef(id) => {
                write!(
                    formatter,
                    "abstract numbering reference {id} does not resolve"
                )
            }
            Self::DanglingMediaRef(id) => {
                write!(formatter, "media reference {id} does not resolve")
            }
            Self::DanglingSectionRef(id) => {
                write!(formatter, "section reference {id} does not resolve")
            }
            Self::NumberingLevelUndefined { reference, level } => {
                write!(
                    formatter,
                    "numbering reference {reference} level {level} is undefined"
                )
            }
            Self::StyleBasedOnCycle(id) => write!(formatter, "style {id} basedOn chain is cyclic"),
            Self::StyleBasedOnKindMismatch { style, based_on } => {
                write!(
                    formatter,
                    "style {style} inherits mismatched kind from {based_on}"
                )
            }
            Self::PropertyValueOutOfDomain { property } => {
                write!(formatter, "property {property} value is out of domain")
            }
            Self::GraphemeOffsetOutOfRange {
                node,
                offset,
                length,
            } => write!(
                formatter,
                "grapheme offset {offset} exceeds node {node} length {length}"
            ),
            Self::UnsupportedV0Extensions => {
                formatter.write_str("v0 extension map cannot be represented in schema v1")
            }
        }
    }
}

impl Error for ModelError {}
