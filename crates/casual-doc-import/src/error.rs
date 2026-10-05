//! Import failures.

use std::error::Error;
use std::fmt;

use casual_doc_model::ModelError;
use casual_doc_ooxml::PackageError;

use crate::report::DispositionViolation;

/// A WordprocessingML import failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportError {
    /// The import configuration exceeded a hard ceiling.
    InvalidConfig,
    /// The package could not provide a required part.
    Package(PackageError),
    /// Main-document or styles XML was malformed or DTD-bearing.
    MalformedXml,
    /// A configured import bound was exceeded.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
    },
    /// The constructed model violated a v1 invariant.
    Model(ModelError),
    /// The compatibility report violated the `35-DISPOSITION-TAXONOMY.md`
    /// contract. `35` requires such a pairing to "fail import, not be reported",
    /// because a report that claims a preservation nothing performed is worse
    /// than no report: it is a false clean bill of health on the exact axis the
    /// fidelity architecture exists to measure.
    Disposition(DispositionViolation),
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => {
                formatter.write_str("import configuration exceeds a hard ceiling")
            }
            Self::Package(error) => write!(formatter, "package error: {error}"),
            Self::MalformedXml => formatter.write_str("document XML is malformed"),
            Self::LimitExceeded { limit } => write!(formatter, "import limit {limit} exceeded"),
            Self::Model(error) => write!(formatter, "imported model is invalid: {error}"),
            Self::Disposition(violation) => {
                write!(formatter, "disposition contract violated: {violation}")
            }
        }
    }
}

impl ImportError {
    /// One sentence a reader can act on, for the refusals that survive a
    /// best-effort open.
    ///
    /// `Display` stays as it is — it names the limit and the rule, which is what
    /// a log needs. This is the other audience, and it exists for the same reason
    /// [`casual_doc_ooxml::PackageError::summary`] does: a host renders this
    /// verbatim, and "document XML is malformed" in front of someone who
    /// double-clicked a file is the same defect as showing them an internal error
    /// name. This repository already has one of those and does not need two.
    ///
    /// With [`crate::ImportConfig::recover`] on, only the bounds and the internal
    /// invariants can reach here at all; the damage variants are recovered from
    /// and reported instead.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::InvalidConfig => {
                "This application asked for a limit the document engine does not allow. This \
                 is a configuration problem, not a problem with the file."
                    .to_owned()
            }
            Self::Package(error) => error.summary(),
            Self::MalformedXml => "This document's text is damaged beyond reading.".to_owned(),
            Self::LimitExceeded { limit } => format!(
                "This document is too large to open safely: it exceeds the limit on {limit}. \
                 The limit protects against a file that would expand without bound once \
                 opened."
            ),
            Self::Model(_) | Self::Disposition(_) => {
                "This document could not be opened because of a fault in the document \
                 engine rather than in the file. The file itself is most likely fine."
                    .to_owned()
            }
        }
    }
}

impl Error for ImportError {}
