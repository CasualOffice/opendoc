// SPDX-License-Identifier: Apache-2.0

//! What a best-effort open repaired, in words a reader shares.
//!
//! # Why this is not the compatibility report
//!
//! [`crate::CompatibilityReport`] answers *"what did this document mean that the
//! model could not carry?"* — a fidelity question about a **well-formed** file,
//! dispositioned on the two axes of `35-DISPOSITION-TAXONOMY.md`. A
//! [`RecoveryReport`] answers a different question: *"what was wrong with the
//! bytes, and what did we do about it?"* A damaged `w:tbl` that the importer
//! stopped reading halfway is not a construct outside the model — it is a
//! construct the file no longer contains in full.
//!
//! The two are kept apart for a reason the disposition document states itself:
//! an ordinary document produces an **empty** compatibility report, and a report
//! that fires on every healthy file is one every caller learns to filter out.
//! Folding "this file was damaged" into the same list would make the damage
//! indistinguishable from a lost shadow effect, and the damage is the one fact a
//! reader must see before saving over the original.
//!
//! Both are produced, and both are honest: a repair that drops document content
//! raises a `rejected` compatibility finding as well, so the taxonomy stays
//! total. See [`crate::ImportConfig::recover`].
//!
//! # The reader-facing half
//!
//! [`Repair::summary`] is a sentence, not a token. The repository already
//! carries a defect of the opposite shape — an edit refused with an internal
//! `ValueTooLarge` shown to the reader as one generic sentence — and a recovery
//! report is read by whoever just opened a damaged file, so `PartUnparsable` is
//! not an answer. The machine-readable [`Repair::kind`] is still there for a host
//! that wants to localise, and [`Repair::severity`] for a host that wants to rank.
//!
//! # Bounds
//!
//! A damaged file is adversarial input. The report aggregates on
//! `(kind, part, detail)` with a saturating occurrence count, holds at most
//! [`MAX_REPAIRS`] distinct rows, and truncates every borrowed string to
//! [`MAX_DETAIL_BYTES`]. Beyond the row ceiling an overflow row carries the
//! count, so a reader is never told a damaged file was clean.

use std::collections::BTreeMap;

/// Distinct repair rows a [`RecoveryReport`] holds before it starts counting
/// into one overflow row.
pub const MAX_REPAIRS: usize = 64;

/// Byte ceiling on a repair's part name or detail string.
pub const MAX_DETAIL_BYTES: usize = 128;

/// Which part of a damaged document a repair was applied to.
///
/// The role is carried beside the part name because the part *name* is a producer
/// convention (`word/styles.xml` is a convention, not a requirement) while the
/// role is what a reader understands: "the styles" rather than "the part at this
/// path".
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PartRole {
    /// The main document body.
    MainDocument,
    /// Style definitions.
    Styles,
    /// List and numbering definitions.
    Numbering,
    /// The theme (fonts and colour scheme).
    Theme,
    /// Document-level settings.
    Settings,
    /// The font table.
    FontTable,
    /// Footnote definitions.
    Footnotes,
    /// Endnote definitions.
    Endnotes,
    /// Comments, or one of their companion parts.
    Comments,
    /// A page header.
    Header,
    /// A page footer.
    Footer,
    /// A chart definition.
    Chart,
    /// An image or other media part.
    Media,
    /// Document properties (title, author, ...).
    DocumentProperties,
    /// A part carried verbatim for round-trip, with no modelled meaning.
    RetainedPart,
}

impl PartRole {
    /// The role as a reader would name it, for use inside a sentence.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::MainDocument => "the document text",
            Self::Styles => "the style definitions",
            Self::Numbering => "the list and numbering definitions",
            Self::Theme => "the theme fonts and colours",
            Self::Settings => "the document settings",
            Self::FontTable => "the font table",
            Self::Footnotes => "the footnotes",
            Self::Endnotes => "the endnotes",
            Self::Comments => "the comments",
            Self::Header => "a page header",
            Self::Footer => "a page footer",
            Self::Chart => "a chart",
            Self::Media => "an image",
            Self::DocumentProperties => "the document properties",
            Self::RetainedPart => "an extra part of the file",
        }
    }

    /// What the reader sees instead, once the damaged part is dropped.
    const fn consequence(self) -> &'static str {
        match self {
            Self::MainDocument => "",
            Self::Styles => "text is shown with default formatting",
            Self::Numbering => "lists are shown without their numbers and bullets",
            Self::Theme => "default fonts and colours are used",
            Self::Settings => "default settings are used",
            Self::FontTable => "fonts are matched by name instead",
            Self::Footnotes => "footnote text is not shown",
            Self::Endnotes => "endnote text is not shown",
            Self::Comments => "comments are not shown",
            Self::Header => "that header is blank",
            Self::Footer => "that footer is blank",
            Self::Chart => "the chart is not shown",
            Self::Media => "a placeholder is shown in its place",
            Self::DocumentProperties => "the title and author are blank",
            Self::RetainedPart => "it will not be written back when you save",
        }
    }
}

/// How much document meaning a repair put at risk.
///
/// A host ranks by this rather than by [`RepairKind`], so a new kind does not
/// need a new case anywhere a report is rendered.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    /// Packaging structure was rebuilt or inferred. No document meaning was at
    /// risk: the same text, formatting and layout are shown.
    Structural,
    /// Some of the document was dropped — one part, one damaged construct, or
    /// the tail of the body after the damage.
    ContentDropped,
    /// None of the document body could be read. What opened is an empty document
    /// with whatever else the file still held.
    BodyLost,
}

/// One kind of repair, as a stable machine-readable token.
///
/// Payload-free so that a report aggregates on it: the variable half of a repair
/// (which part, which element, how many) lives on [`Repair`].
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RepairKind {
    /// The ZIP directory at the end of the file was missing or unusable, and the
    /// entry list was rebuilt by scanning the file for part headers.
    ArchiveDirectoryRebuilt,
    /// `[Content_Types].xml` was missing or unreadable; content types were
    /// inferred from part extensions.
    ContentTypeManifestInferred,
    /// `_rels/.rels` was missing or unreadable; the package relationship graph
    /// was inferred.
    PackageRelationshipsInferred,
    /// No usable `officeDocument` relationship resolved, so the main document was
    /// located by its conventional name.
    MainDocumentLocatedByName,
    /// The main document's declared content type was absent or not a
    /// WordprocessingML type, and was ignored.
    MainDocumentContentTypeIgnored,
    /// More than one `officeDocument` relationship was present; the first in
    /// relationship-id order was used.
    MainDocumentAmbiguous,
    /// A part a relationship pointed at is not in the file, or could not be
    /// decompressed.
    PartMissing,
    /// A part is in the file but its XML could not be parsed.
    PartUnparsable,
    /// The main document's XML broke partway through. Everything before the break
    /// was kept.
    BodyStoppedAtDamage,
    /// Nothing at all could be read from the main document.
    BodyUnreadable,
    /// The main document has no `w:body`.
    BodyMissing,
    /// The main document's root element is not `w:document`: the file is packaged
    /// as a Word document but its content is something else.
    NotWordprocessingMl,
    /// A document-type declaration was removed before parsing. DTDs are refused
    /// for security reasons (entity expansion), and carry no document content.
    DoctypeRemoved,
    /// The XML declaration named an encoding this engine does not decode, and was
    /// rewritten as UTF-8.
    XmlDeclarationNormalized,
    /// Bytes that are not valid UTF-8 were replaced with the Unicode replacement
    /// character.
    InvalidTextBytesReplaced,
    /// A reference to an entity the file never declares was removed.
    UndeclaredEntityRemoved,
    /// An end tag with no matching start tag was removed.
    StrayEndTagRemoved,
    /// An element left open at the end of a truncated file was closed.
    UnclosedElementClosed,
    /// Bytes after the end of the XML document, or a tag cut off mid-way, were
    /// discarded.
    TrailingBytesDiscarded,
    /// A `<` that cannot begin a tag was treated as text.
    MalformedMarkupEscaped,
    /// More distinct repairs were applied than the report holds rows for.
    FurtherRepairs,
}

impl RepairKind {
    /// The stable token a host can key a translation off.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::ArchiveDirectoryRebuilt => "archive-directory-rebuilt",
            Self::ContentTypeManifestInferred => "content-type-manifest-inferred",
            Self::PackageRelationshipsInferred => "package-relationships-inferred",
            Self::MainDocumentLocatedByName => "main-document-located-by-name",
            Self::MainDocumentContentTypeIgnored => "main-document-content-type-ignored",
            Self::MainDocumentAmbiguous => "main-document-ambiguous",
            Self::PartMissing => "part-missing",
            Self::PartUnparsable => "part-unparsable",
            Self::BodyStoppedAtDamage => "body-stopped-at-damage",
            Self::BodyUnreadable => "body-unreadable",
            Self::BodyMissing => "body-missing",
            Self::NotWordprocessingMl => "not-wordprocessingml",
            Self::DoctypeRemoved => "doctype-removed",
            Self::XmlDeclarationNormalized => "xml-declaration-normalized",
            Self::InvalidTextBytesReplaced => "invalid-text-bytes-replaced",
            Self::UndeclaredEntityRemoved => "undeclared-entity-removed",
            Self::StrayEndTagRemoved => "stray-end-tag-removed",
            Self::UnclosedElementClosed => "unclosed-element-closed",
            Self::TrailingBytesDiscarded => "trailing-bytes-discarded",
            Self::MalformedMarkupEscaped => "malformed-markup-escaped",
            Self::FurtherRepairs => "further-repairs",
        }
    }

    /// How much document meaning this kind of repair put at risk.
    #[must_use]
    pub const fn severity(self) -> Severity {
        match self {
            Self::ArchiveDirectoryRebuilt
            | Self::ContentTypeManifestInferred
            | Self::PackageRelationshipsInferred
            | Self::MainDocumentLocatedByName
            | Self::MainDocumentContentTypeIgnored
            | Self::MainDocumentAmbiguous
            | Self::DoctypeRemoved
            | Self::XmlDeclarationNormalized => Severity::Structural,
            Self::PartMissing
            | Self::PartUnparsable
            | Self::BodyStoppedAtDamage
            | Self::BodyMissing
            | Self::InvalidTextBytesReplaced
            | Self::UndeclaredEntityRemoved
            | Self::StrayEndTagRemoved
            | Self::UnclosedElementClosed
            | Self::TrailingBytesDiscarded
            | Self::MalformedMarkupEscaped
            | Self::FurtherRepairs => Severity::ContentDropped,
            Self::BodyUnreadable | Self::NotWordprocessingMl => Severity::BodyLost,
        }
    }
}

/// One aggregated repair: what was wrong, where, and how often.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Repair {
    /// What was repaired.
    pub kind: RepairKind,
    /// Which role in the document the repair belongs to, where one is known.
    pub role: Option<PartRole>,
    /// The package part the repair is charged to, where the open knows one.
    /// Truncated to [`MAX_DETAIL_BYTES`].
    pub part: Option<String>,
    /// A bounded extra fact: the XML name that was dropped, the encoding that was
    /// rewritten, the root element that was not `w:document`. Truncated to
    /// [`MAX_DETAIL_BYTES`].
    pub detail: Option<String>,
    /// How many times this exact repair was applied. Saturating.
    pub occurrences: u32,
}

impl Repair {
    /// A repair with no location.
    #[must_use]
    pub const fn new(kind: RepairKind) -> Self {
        Self {
            kind,
            role: None,
            part: None,
            detail: None,
            occurrences: 1,
        }
    }

    /// A repair charged to one package part in one role.
    #[must_use]
    pub fn in_part(kind: RepairKind, role: PartRole, part: &str) -> Self {
        Self {
            kind,
            role: Some(role),
            part: Some(truncate(part)),
            detail: None,
            occurrences: 1,
        }
    }

    /// A repair in one role, with no part name known.
    #[must_use]
    pub const fn in_role(kind: RepairKind, role: PartRole) -> Self {
        Self {
            kind,
            role: Some(role),
            part: None,
            detail: None,
            occurrences: 1,
        }
    }

    /// Attaches the bounded detail string.
    #[must_use]
    pub fn with_detail(mut self, detail: &str) -> Self {
        self.detail = Some(truncate(detail));
        self
    }

    /// How much document meaning this repair put at risk.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.kind.severity()
    }

    /// One sentence a reader can act on, with no internal vocabulary in it.
    ///
    /// This is the default rendering. A host that localises keys off
    /// [`RepairKind::token`] and the structured fields instead, which is why
    /// every variable fact is also a field rather than only being inside this
    /// string.
    #[must_use]
    pub fn summary(&self) -> String {
        let times = match self.occurrences {
            0 | 1 => String::new(),
            2 => " This happened twice.".to_owned(),
            count => format!(" This happened {count} times."),
        };
        let body = match self.kind {
            RepairKind::ArchiveDirectoryRebuilt => {
                "The index at the end of the file was damaged, so the file's contents were \
                 found by scanning it. Parts of the file after the damage may be missing."
                    .to_owned()
            }
            RepairKind::ContentTypeManifestInferred => {
                "The file's list of content types was missing or damaged, so each part's \
                 type was worked out from its name."
                    .to_owned()
            }
            RepairKind::PackageRelationshipsInferred => {
                "The file's index of which part is the document was missing or damaged, so \
                 the document was located by its usual name."
                    .to_owned()
            }
            RepairKind::MainDocumentLocatedByName => format!(
                "Nothing in the file pointed at the document text, so it was located by its \
                 usual name{}.",
                self.at_part()
            ),
            RepairKind::MainDocumentContentTypeIgnored => {
                "The file does not say that its main part is a Word document. It was opened \
                 as one anyway."
                    .to_owned()
            }
            RepairKind::MainDocumentAmbiguous => {
                "The file names more than one main document. The first was used.".to_owned()
            }
            RepairKind::PartMissing => format!(
                "{} {} missing from the file{}, so {}.",
                capitalise(self.role_description()),
                self.verb_is(),
                self.at_part(),
                self.consequence()
            ),
            RepairKind::PartUnparsable => format!(
                "{} {} damaged and could not be read{}, so {}.",
                capitalise(self.role_description()),
                self.verb_is(),
                self.at_part(),
                self.consequence()
            ),
            RepairKind::BodyStoppedAtDamage => format!(
                "The document text is damaged partway through{}. Everything before that \
                 point was recovered; anything after it is not in this document.",
                self.after_detail()
            ),
            RepairKind::BodyUnreadable => {
                "None of the document text could be read. The rest of the file — its \
                 properties, styles and any headers — was opened around an empty document."
                    .to_owned()
            }
            RepairKind::BodyMissing => {
                "The document has no body, so it opened empty.".to_owned()
            }
            RepairKind::NotWordprocessingMl => format!(
                "This file is packaged as a Word document but its main part is not one{}. \
                 Only an empty document could be built from it.",
                self.after_detail()
            ),
            RepairKind::DoctypeRemoved => {
                "The document carried a document-type declaration. It was removed before \
                 reading, because such a declaration can be used to make a reader expand \
                 unbounded amounts of text, and it holds no document content."
                    .to_owned()
            }
            RepairKind::XmlDeclarationNormalized => format!(
                "The document declares a text encoding this engine does not read{}. It was \
                 read as UTF-8 instead, so some characters may be wrong.",
                self.after_detail()
            ),
            RepairKind::InvalidTextBytesReplaced => {
                "Some characters in the document are not valid text. They were replaced \
                 with the \u{fffd} placeholder."
                    .to_owned()
            }
            RepairKind::UndeclaredEntityRemoved => format!(
                "The document refers to a named piece of text it never defines{}. The \
                 reference was removed.",
                self.after_detail()
            ),
            RepairKind::StrayEndTagRemoved => format!(
                "The document closes a tag it never opened{}. The stray closing tag was \
                 ignored.",
                self.after_detail()
            ),
            RepairKind::UnclosedElementClosed => format!(
                "The document ends with a tag still open{}, which means the file is cut \
                 short. It was closed so the rest could be read.",
                self.after_detail()
            ),
            RepairKind::TrailingBytesDiscarded => {
                "There are extra bytes after the end of the document, or a tag cut off \
                 mid-way. They were discarded."
                    .to_owned()
            }
            RepairKind::MalformedMarkupEscaped => {
                "The document contains a \u{201c}<\u{201d} that does not begin a tag. It was \
                 read as ordinary text."
                    .to_owned()
            }
            RepairKind::FurtherRepairs => {
                "The file needed more separate repairs than this report lists.".to_owned()
            }
        };
        format!("{body}{times}")
    }

    fn role_description(&self) -> &'static str {
        self.role
            .map_or("part of the file", PartRole::describe)
    }

    fn consequence(&self) -> &'static str {
        self.role
            .map_or("that part of the document is not shown", PartRole::consequence)
    }

    /// `is`/`are`, so the sentence agrees with the role's own number.
    fn verb_is(&self) -> &'static str {
        match self.role {
            Some(
                PartRole::Styles
                | PartRole::Numbering
                | PartRole::Footnotes
                | PartRole::Endnotes
                | PartRole::Comments
                | PartRole::DocumentProperties,
            ) => "are",
            _ => "is",
        }
    }

    fn at_part(&self) -> String {
        self.part
            .as_ref()
            .map_or_else(String::new, |part| format!(" ({part})"))
    }

    fn after_detail(&self) -> String {
        self.detail
            .as_ref()
            .map_or_else(String::new, |detail| format!(" ({detail})"))
    }
}

/// Everything one best-effort open repaired, in deterministic order.
///
/// Empty means nothing was repaired — which is what a healthy document produces,
/// and the reason this is a separate report from the compatibility findings.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    repairs: Vec<Repair>,
}

impl RecoveryReport {
    /// Whether the open had to repair anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.repairs.is_empty()
    }

    /// The repairs, ordered by kind, then role, then part, then detail.
    #[must_use]
    pub fn repairs(&self) -> &[Repair] {
        &self.repairs
    }

    /// The worst severity any repair reached, or `None` when nothing was
    /// repaired. A host ranks its notice off this.
    #[must_use]
    pub fn severity(&self) -> Option<Severity> {
        self.repairs.iter().map(Repair::severity).max()
    }

    /// Builds a deterministic report from repairs in any order, aggregating
    /// equal `(kind, role, part, detail)` rows and folding everything past
    /// [`MAX_REPAIRS`] distinct rows into one [`RepairKind::FurtherRepairs`] row.
    #[must_use]
    pub(crate) fn from_repairs(repairs: impl IntoIterator<Item = Repair>) -> Self {
        type Key = (
            RepairKind,
            Option<PartRole>,
            Option<String>,
            Option<String>,
        );
        let mut aggregated: BTreeMap<Key, u32> = BTreeMap::new();
        let mut overflow = 0_u32;
        for repair in repairs {
            let key = (
                repair.kind,
                repair.role,
                repair.part.map(|part| truncate(&part)),
                repair.detail.map(|detail| truncate(&detail)),
            );
            if let Some(count) = aggregated.get_mut(&key) {
                *count = count.saturating_add(repair.occurrences.max(1));
            } else if aggregated.len() < MAX_REPAIRS {
                aggregated.insert(key, repair.occurrences.max(1));
            } else {
                overflow = overflow.saturating_add(1);
            }
        }
        let mut repairs: Vec<Repair> = aggregated
            .into_iter()
            .map(|((kind, role, part, detail), occurrences)| Repair {
                kind,
                role,
                part,
                detail,
                occurrences,
            })
            .collect();
        if overflow > 0 {
            repairs.push(Repair {
                kind: RepairKind::FurtherRepairs,
                role: None,
                part: None,
                detail: None,
                occurrences: overflow,
            });
        }
        repairs.sort();
        Self { repairs }
    }

    /// Folds `other`'s repairs into this report, re-aggregating.
    pub(crate) fn absorb(&mut self, other: Self) {
        if other.is_empty() {
            return;
        }
        let combined: Vec<Repair> = self.repairs.drain(..).chain(other.repairs).collect();
        *self = Self::from_repairs(combined);
    }
}

/// Truncates to [`MAX_DETAIL_BYTES`] on a character boundary, so a bounded
/// string is still a valid one.
fn truncate(value: &str) -> String {
    if value.len() <= MAX_DETAIL_BYTES {
        return value.to_owned();
    }
    let mut end = MAX_DETAIL_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

/// Upper-cases the first character, so a role description can start a sentence.
fn capitalise(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
    }
}
