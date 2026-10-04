// SPDX-License-Identifier: Apache-2.0

//! PresentationML import failures.

use std::error::Error;
use std::fmt;

use casual_doc_model::ModelError;
use casual_doc_package::PackageError;
use casual_pres_model::PresentationError;

/// What went wrong importing a `.pptx` package.
///
/// A separate type from [`PresentationError`] rather than more variants on it:
/// the model's failures are about a presentation that cannot be valid, and these
/// are about a package that cannot be read. A caller distinguishing "this file is
/// not a presentation" from "this presentation is malformed" needs both, and
/// collapsing them would make the second indistinguishable from the first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportError {
    /// The ZIP container was refused at the admission boundary.
    Package(PackageError),
    /// `[Content_Types].xml` or `_rels/.rels` is absent. OPC fixes exactly these
    /// two names, and nothing else about the layout of a package.
    MissingRequiredPart {
        /// The absent part's fixed name.
        part: &'static str,
    },
    /// `_rels/.rels` declares no internal `officeDocument` relationship, so there
    /// is no presentation part to read.
    MissingPresentationPart,
    /// `_rels/.rels` declares more than one `officeDocument` relationship.
    /// Refused rather than resolved by a tie-break: OPC admits exactly one, and
    /// picking one of two would open a different deck than another reader does.
    AmbiguousPresentationPart,
    /// The `officeDocument` part's declared content type is not PresentationML's.
    /// A `.docx` renamed to `.pptx` lands here, which is the point.
    NotAPresentation {
        /// The declared content type, or `None` when `[Content_Types].xml`
        /// resolves none.
        content_type: Option<String>,
    },
    /// A part's XML is malformed, truncated, or declares a DTD (refused
    /// unconditionally — an external entity is an exfiltration primitive, not a
    /// document feature).
    MalformedPartXml {
        /// The normalized part name.
        part: String,
    },
    /// A part's XML exceeded a bound in `ImportLimits`.
    PartLimitExceeded {
        /// The normalized part name.
        part: String,
        /// Stable limit name.
        limit: &'static str,
        /// The observed value.
        observed: u64,
        /// The active allowed value.
        allowed: u64,
    },
    /// `ppt/presentation.xml` declared no `p:sldSz`. Required rather than
    /// defaulted: every slide's geometry is expressed on that surface, so
    /// substituting 16:9 would silently reposition every shape in a 4:3 deck.
    MissingSlideSize,
    /// A `p:sldId`/`p:sldLayoutId`/`p:sldMasterId` `r:id` resolves to no
    /// relationship, or to one whose target is not an admitted part.
    ///
    /// This is the reference that makes the deck a deck: slide ORDER comes from
    /// `p:sldIdLst`, resolved through the relationship ids, and a dangling one
    /// means the order cannot be reconstructed.
    UnresolvedRelationship {
        /// The part whose relationship list was consulted.
        part: String,
        /// The unresolved `r:id`.
        relationship_id: String,
    },
    /// A slide declared no `slideLayout` relationship. `Slide::layout` is
    /// required by the model, and PowerPoint always writes one.
    SlideWithoutLayout {
        /// The slide part.
        part: String,
    },
    /// A layout declared no `slideMaster` relationship.
    LayoutWithoutMaster {
        /// The layout part.
        part: String,
    },
    /// Node-id allocation overflowed its id space.
    Model(ModelError),
    /// The imported deck failed the presentation model's own validation. Carried
    /// through rather than flattened so a caller sees exactly which invariant a
    /// real package violated.
    Presentation(PresentationError),
    /// The compatibility report claimed a preservation the ledger does not
    /// license. `35-DISPOSITION-TAXONOMY.md` is explicit that this is an
    /// internal error and must fail import rather than be reported.
    IllegalDisposition {
        /// The violation, rendered.
        violation: String,
    },
}

impl From<PackageError> for ImportError {
    fn from(error: PackageError) -> Self {
        Self::Package(error)
    }
}

impl From<ModelError> for ImportError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

impl From<PresentationError> for ImportError {
    fn from(error: PresentationError) -> Self {
        Self::Presentation(error)
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package(error) => write!(formatter, "{error}"),
            Self::MissingRequiredPart { part } => {
                write!(formatter, "package is missing the required part {part}")
            }
            Self::MissingPresentationPart => {
                formatter.write_str("package declares no presentation part")
            }
            Self::AmbiguousPresentationPart => {
                formatter.write_str("package declares more than one presentation part")
            }
            Self::NotAPresentation { content_type } => match content_type {
                Some(content_type) => write!(
                    formatter,
                    "main part content type {content_type} is not PresentationML"
                ),
                None => formatter.write_str("main part declares no content type"),
            },
            Self::MalformedPartXml { part } => write!(formatter, "part {part} XML is malformed"),
            Self::PartLimitExceeded {
                part,
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "part {part} exceeded limit {limit}: observed {observed}, allowed {allowed}"
            ),
            Self::MissingSlideSize => {
                formatter.write_str("presentation part declares no slide size")
            }
            Self::UnresolvedRelationship {
                part,
                relationship_id,
            } => write!(
                formatter,
                "part {part} references relationship {relationship_id}, which does not resolve"
            ),
            Self::SlideWithoutLayout { part } => {
                write!(formatter, "slide {part} references no slide layout")
            }
            Self::LayoutWithoutMaster { part } => {
                write!(formatter, "layout {part} references no slide master")
            }
            Self::Model(error) => write!(formatter, "{error}"),
            Self::Presentation(error) => write!(formatter, "{error}"),
            Self::IllegalDisposition { violation } => write!(
                formatter,
                "import produced an illegal compatibility disposition: {violation}"
            ),
        }
    }
}

impl Error for ImportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Package(error) => Some(error),
            Self::Model(error) => Some(error),
            Self::Presentation(error) => Some(error),
            _ => None,
        }
    }
}
