//! Import failures.

use std::error::Error;
use std::fmt;

use casual_doc_model::ModelError;
use casual_doc_ooxml::PackageError;

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
        }
    }
}

impl Error for ImportError {}
