// SPDX-License-Identifier: Apache-2.0

//! The formats the SDK opens and saves, and the save modes.

use std::fmt;
use std::str::FromStr;

use casual_doc_io::formats;

use crate::error::{Error, ErrorCode};

/// A document format.
///
/// Which directions each format supports is stated by [`Format::can_open`] and
/// [`Format::can_save`], which mirror the engine's registry (a test holds them
/// equal), so a support table generated from them cannot overstate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Format {
    /// Office Open XML word processing (`.docx`). Open and save, loss-aware.
    Docx,
    /// Office Open XML template (`.dotx`). Save only.
    Dotx,
    /// OpenDocument Text (`.odt`). Open and save.
    Odt,
    /// Rich Text Format (`.rtf`). Open only.
    Rtf,
    /// Plain UTF-8 text (`.txt`). Open and save.
    Txt,
    /// The engine's normalized JSON snapshot. Open and save, lossless.
    Json,
    /// Single-file HTML5 (`.html`). Save only.
    Html,
    /// CommonMark (`.md`). Save only.
    Markdown,
    /// PDF. Save always (real text, from the engine's layout); open only in a
    /// build with the `pdf-import` feature.
    Pdf,
}

impl Format {
    /// Every format, in a stable order.
    pub const ALL: [Self; 9] = [
        Self::Docx,
        Self::Dotx,
        Self::Odt,
        Self::Rtf,
        Self::Txt,
        Self::Json,
        Self::Html,
        Self::Markdown,
        Self::Pdf,
    ];

    /// The engine registry id.
    #[must_use]
    pub const fn registry_id(self) -> &'static str {
        match self {
            Self::Docx => formats::DOCX,
            Self::Dotx => formats::DOTX,
            Self::Odt => formats::ODT,
            Self::Rtf => formats::RTF,
            Self::Txt => formats::TEXT,
            Self::Json => formats::NORMALIZED_JSON,
            Self::Html => formats::HTML,
            Self::Markdown => formats::MARKDOWN,
            Self::Pdf => formats::PDF,
        }
    }

    /// The short name used by every binding and the CLI (`"docx"`, `"md"`, ...).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Dotx => "dotx",
            Self::Odt => "odt",
            Self::Rtf => "rtf",
            Self::Txt => "txt",
            Self::Json => "json",
            Self::Html => "html",
            Self::Markdown => "md",
            Self::Pdf => "pdf",
        }
    }

    /// The MIME type of a saved file.
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Dotx => "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
            Self::Odt => "application/vnd.oasis.opendocument.text",
            Self::Rtf => "application/rtf",
            Self::Txt => "text/plain",
            Self::Json => "application/json",
            Self::Html => "text/html",
            Self::Markdown => "text/markdown",
            Self::Pdf => "application/pdf",
        }
    }

    /// Whether this build opens the format.
    #[must_use]
    pub const fn can_open(self) -> bool {
        match self {
            Self::Docx | Self::Odt | Self::Rtf | Self::Txt | Self::Json => true,
            // HTML and Markdown import are registered (RM-47).
            Self::Html | Self::Markdown => true,
            Self::Pdf => cfg!(feature = "pdf-import"),
            Self::Dotx => false,
        }
    }

    /// Whether this build saves the format.
    #[must_use]
    pub const fn can_save(self) -> bool {
        !matches!(self, Self::Rtf)
    }

    /// Whether a save to this format can carry the whole model (DOCX, ODT and
    /// JSON). The others are lossy by nature, and their reports say so.
    #[must_use]
    pub const fn carries_document(self) -> bool {
        matches!(self, Self::Docx | Self::Dotx | Self::Odt | Self::Json)
    }

    /// The format a file name's extension names.
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        let extension = path.rsplit_once('.')?.1;
        extension.parse().ok()
    }

    pub(crate) fn from_registry_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.registry_id() == id)
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Format {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self, Error> {
        Ok(match name.trim().to_ascii_lowercase().as_str() {
            "docx" => Self::Docx,
            "dotx" => Self::Dotx,
            "odt" => Self::Odt,
            "rtf" => Self::Rtf,
            "txt" | "text" => Self::Txt,
            "json" => Self::Json,
            "html" | "htm" => Self::Html,
            "md" | "markdown" => Self::Markdown,
            "pdf" => Self::Pdf,
            other => {
                return Err(Error::new(
                    ErrorCode::UnsupportedFormat,
                    format!("\"{other}\" is not a format this SDK knows"),
                ));
            }
        })
    }
}

/// How a save treats what the engine kept from the source file.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum SaveMode {
    /// The original bytes, exactly, when the document is unchanged and the
    /// target is its source format; otherwise the save is refused.
    Exact,
    /// Regenerate from the model and carry safe, verbatim-retained source parts
    /// the model does not represent (the default for a same-format save).
    #[default]
    PreserveWhenSafe,
    /// Regenerate from the model alone. Anything the model does not carry is
    /// dropped, and the report says what.
    Semantic,
}

impl SaveMode {
    /// The stable string form.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::PreserveWhenSafe => "preserve",
            Self::Semantic => "semantic",
        }
    }
}

impl FromStr for SaveMode {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self, Error> {
        match name.trim().to_ascii_lowercase().as_str() {
            "exact" => Ok(Self::Exact),
            "preserve" | "preserve-when-safe" | "preservewhensafe" => Ok(Self::PreserveWhenSafe),
            "semantic" => Ok(Self::Semantic),
            other => Err(Error::invalid(format!(
                "\"{other}\" is not a save mode (exact, preserve, semantic)"
            ))),
        }
    }
}
