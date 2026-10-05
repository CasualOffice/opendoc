//! Typed DOCX package admission and part-read failures.

use std::error::Error;
use std::fmt;

/// DOCX package admission or part-read failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageError {
    /// A host limit exceeds its non-bypassable hard ceiling.
    InvalidLimitConfiguration {
        /// Stable limit name.
        limit: &'static str,
        /// Requested value.
        value: u64,
        /// Non-bypassable maximum.
        hard_ceiling: u64,
    },
    /// Package metadata exceeds an active resource limit.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value.
        observed: u64,
        /// Active allowed value.
        allowed: u64,
    },
    /// ZIP records are malformed or inconsistent.
    MalformedArchive,
    /// Package work was cooperatively cancelled.
    Cancelled,
    /// A package path is unsafe or outside the accepted profile.
    UnsafePartName,
    /// Two records resolve to the same normalized package part.
    DuplicatePart,
    /// An encrypted ZIP entry is unsupported.
    EncryptedEntry,
    /// A ZIP entry uses a compression method outside the DOCX profile.
    UnsupportedCompression,
    /// Compressed data ranges overlap.
    OverlappingEntries,
    /// A symbolic link or other special entry is unsupported.
    SpecialEntry,
    /// A macro project part is unsupported.
    MacroPart,
    /// A minimal DOCX package part is missing.
    MissingRequiredPart {
        /// Required static part name.
        part: &'static str,
    },
    /// Package-metadata XML (relationships or content types) is malformed.
    MalformedPackageXml {
        /// Static part name of the offending metadata part.
        part: &'static str,
    },
    /// No `officeDocument` relationship resolves to an admitted main document.
    MissingMainDocument,
    /// More than one `officeDocument` relationship is present.
    AmbiguousMainDocument,
    /// The discovered main document does not carry a WordprocessingML type.
    UnsupportedMainDocumentType,
    /// A requested admitted part does not exist.
    PartNotFound,
    /// A part could not be fully decompressed and verified.
    PartReadFailed,
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            } => write!(
                formatter,
                "package limit {limit} value {value} exceeds hard ceiling {hard_ceiling}"
            ),
            Self::LimitExceeded {
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "package limit {limit} exceeded: observed {observed}, allowed {allowed}"
            ),
            Self::MalformedArchive => formatter.write_str("DOCX ZIP structure is malformed"),
            Self::Cancelled => formatter.write_str("DOCX package operation was cancelled"),
            Self::UnsafePartName => formatter.write_str("DOCX package part name is unsafe"),
            Self::DuplicatePart => formatter.write_str("DOCX package contains a duplicate part"),
            Self::EncryptedEntry => formatter.write_str("encrypted DOCX entries are unsupported"),
            Self::UnsupportedCompression => {
                formatter.write_str("DOCX entry compression method is unsupported")
            }
            Self::OverlappingEntries => formatter.write_str("DOCX ZIP entry data ranges overlap"),
            Self::SpecialEntry => {
                formatter.write_str("DOCX package contains a special filesystem entry")
            }
            Self::MacroPart => formatter.write_str("DOCX macro project parts are unsupported"),
            Self::MissingRequiredPart { part } => {
                write!(formatter, "DOCX package is missing required part {part}")
            }
            Self::MalformedPackageXml { part } => {
                write!(formatter, "DOCX package metadata part {part} is malformed")
            }
            Self::MissingMainDocument => {
                formatter.write_str("DOCX package has no resolvable main document relationship")
            }
            Self::AmbiguousMainDocument => {
                formatter.write_str("DOCX package declares more than one main document")
            }
            Self::UnsupportedMainDocumentType => {
                formatter.write_str("DOCX main document content type is unsupported")
            }
            Self::PartNotFound => formatter.write_str("DOCX package part was not found"),
            Self::PartReadFailed => {
                formatter.write_str("DOCX package part could not be fully verified")
            }
        }
    }
}

impl PackageError {
    /// One sentence a reader can act on, for the refusals that survive a
    /// best-effort open.
    ///
    /// `Display` is the engineer-facing form and stays as it is — it names the
    /// part and the rule, which is what a log needs. This is the other audience.
    /// A host that refuses to open a file must be able to say *why* in terms the
    /// reader shares: "DOCX ZIP structure is malformed" in front of someone who
    /// double-clicked a file is the same defect as showing them `ValueTooLarge`,
    /// which this repository already has one of and does not need two.
    ///
    /// The sentences also say what the reader can **do**, because every surviving
    /// refusal has a different answer: a macro-bearing file can be re-saved as
    /// `.docx`, an encrypted one needs its password removed, and a file that is
    /// not a package at all is simply a different kind of file.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::InvalidLimitConfiguration { .. } => {
                "This application asked for a limit the document engine does not allow. This \
                 is a configuration problem, not a problem with the file."
                    .to_owned()
            }
            Self::LimitExceeded { limit, .. } => format!(
                "This file is too large to open safely: it exceeds the limit on {limit}. The \
                 limit protects against a file that would expand without bound once opened."
            ),
            Self::MalformedArchive => {
                "This file is not a readable document package. Enough of its structure is \
                 missing that no part of it could be found — it may be a different kind of \
                 file, or damaged beyond the point where anything is recoverable."
                    .to_owned()
            }
            Self::Cancelled => "Opening this file was cancelled.".to_owned(),
            Self::UnsafePartName => {
                "This file names one of its parts in a way that would write outside the \
                 document when unpacked. It was not opened, because that is how a document \
                 is used to reach the rest of your computer."
                    .to_owned()
            }
            Self::DuplicatePart => {
                "This file contains the same part twice, so there is no way to tell which \
                 copy is the document. It was not opened, because picking one could show you \
                 content the file does not actually hold."
                    .to_owned()
            }
            Self::EncryptedEntry => {
                "This document is encrypted. Opening it needs the password it was encrypted \
                 with; nothing is damaged, and removing the password in the application that \
                 set it will make the file readable here."
                    .to_owned()
            }
            Self::UnsupportedCompression => {
                "This file compresses its contents in a way this engine does not read. \
                 Re-saving it from the application that produced it will usually fix that."
                    .to_owned()
            }
            Self::OverlappingEntries => {
                "The parts of this file overlap one another, which no genuine document does. \
                 It was not opened, because the overlap can be used to make two readers see \
                 two different documents in one file."
                    .to_owned()
            }
            Self::SpecialEntry => {
                "This file contains a link to somewhere else on the filesystem rather than a \
                 document part. It was not opened, because following it would read a file \
                 you did not choose."
                    .to_owned()
            }
            Self::MacroPart => {
                "This document carries macros (a .docm file). Macro-bearing documents are \
                 not opened here. Saving it as a .docx from Word drops the macros and makes \
                 it readable."
                    .to_owned()
            }
            Self::MissingRequiredPart { part } => {
                format!("This file is missing the part that holds {part}.")
            }
            Self::MalformedPackageXml { part } => {
                format!("This file's {part} is damaged beyond reading.")
            }
            Self::MissingMainDocument => {
                "This file does not contain a Word document. It is a package of some kind, \
                 but nothing in it is document text — it may be a spreadsheet, a \
                 presentation, or another format saved with the wrong name."
                    .to_owned()
            }
            Self::AmbiguousMainDocument => {
                "This file names more than one main document, so there is no way to tell \
                 which is meant."
                    .to_owned()
            }
            Self::UnsupportedMainDocumentType => {
                "This file's main part is not a Word document.".to_owned()
            }
            Self::PartNotFound => {
                "A part this document refers to is not in the file.".to_owned()
            }
            Self::PartReadFailed => {
                "A part of this document could not be unpacked; its compressed data does not \
                 match what the file says it should be."
                    .to_owned()
            }
        }
    }
}

impl Error for PackageError {}

impl From<casual_doc_package::PackageError> for PackageError {
    fn from(error: casual_doc_package::PackageError) -> Self {
        match error {
            casual_doc_package::PackageError::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            } => Self::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            },
            casual_doc_package::PackageError::LimitExceeded {
                limit,
                observed,
                allowed,
            } => Self::LimitExceeded {
                limit,
                observed,
                allowed,
            },
            casual_doc_package::PackageError::MalformedArchive => Self::MalformedArchive,
            casual_doc_package::PackageError::Cancelled => Self::Cancelled,
            casual_doc_package::PackageError::UnsafePartName => Self::UnsafePartName,
            casual_doc_package::PackageError::DuplicatePart => Self::DuplicatePart,
            casual_doc_package::PackageError::EncryptedEntry => Self::EncryptedEntry,
            casual_doc_package::PackageError::UnsupportedCompression => {
                Self::UnsupportedCompression
            }
            casual_doc_package::PackageError::OverlappingEntries => Self::OverlappingEntries,
            casual_doc_package::PackageError::SpecialEntry => Self::SpecialEntry,
            casual_doc_package::PackageError::PartNotFound => Self::PartNotFound,
            casual_doc_package::PackageError::PartReadFailed => Self::PartReadFailed,
        }
    }
}
