//! Document metadata: the OPC/Office document-property groups carried in the
//! `docProps/*` parts — core properties (`docProps/core.xml`, Dublin Core + OPC
//! `cp:`), extended/application properties (`docProps/app.xml`), and custom
//! properties (`docProps/custom.xml`).
//!
//! Every field is optional and additive: a document with no metadata carries
//! `None`, and each group is omitted from serialization when empty so existing
//! snapshots stay byte-identical. Dates and floating-point custom values are
//! stored verbatim as the producer wrote them (W3CDTF/ISO-8601 strings) so the
//! bytes round-trip without a lossy parse and the model stays `Eq`.

use serde::{Deserialize, Serialize};

use crate::ModelError;

/// Byte bound for a free-text metadata field (title, creator, description,
/// company, a custom text value, a titles-of-parts entry, ...).
const MAX_META_TEXT: usize = 4_096;
/// Byte bound for a short token metadata field (language, version, revision, a
/// verbatim date, a custom property name).
const MAX_META_TOKEN: usize = 255;
/// Upper bound on the length of a metadata vector (titles-of-parts, heading
/// pairs, custom properties).
const MAX_META_ITEMS: usize = 4_096;

/// Package core properties (`docProps/core.xml`): the Dublin Core (`dc:`/
/// `dcterms:`) and OPC (`cp:`) document metadata. Every field is optional; dates
/// are retained verbatim as the producer wrote them (W3CDTF/ISO-8601).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreProperties {
    /// `dc:title`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `dc:subject`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// `dc:creator` — the document author(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    /// `cp:keywords`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    /// `dc:description`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `cp:lastModifiedBy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
    /// `cp:revision` (a producer-local revision token, retained verbatim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// `dcterms:created` — a W3CDTF/ISO-8601 timestamp, retained verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// `dcterms:modified` — a W3CDTF/ISO-8601 timestamp, retained verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// `cp:lastPrinted` — a W3CDTF/ISO-8601 timestamp, retained verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_printed: Option<String>,
    /// `cp:category`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// `cp:contentStatus`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_status: Option<String>,
    /// `dc:language`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// `cp:version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl CoreProperties {
    /// Whether no core property is set (so `docProps/core.xml` is omitted).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn validate(&self) -> Result<(), ModelError> {
        for (value, field) in [
            (&self.title, "core.title"),
            (&self.subject, "core.subject"),
            (&self.creator, "core.creator"),
            (&self.keywords, "core.keywords"),
            (&self.description, "core.description"),
            (&self.last_modified_by, "core.lastModifiedBy"),
            (&self.category, "core.category"),
            (&self.content_status, "core.contentStatus"),
        ] {
            check_text(value, MAX_META_TEXT, field)?;
        }
        for (value, field) in [
            (&self.revision, "core.revision"),
            (&self.created, "core.created"),
            (&self.modified, "core.modified"),
            (&self.last_printed, "core.lastPrinted"),
            (&self.language, "core.language"),
            (&self.version, "core.version"),
        ] {
            check_text(value, MAX_META_TOKEN, field)?;
        }
        Ok(())
    }
}

/// One `HeadingPairs` entry (`app.xml`): a heading-group name and the count of
/// document parts under it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeadingPair {
    /// The heading-group name (`vt:lpstr`).
    pub name: String,
    /// The number of parts in the group (`vt:i4`).
    pub count: i32,
}

/// Extended (application) properties (`docProps/app.xml`). Every field is
/// optional; numeric counts are modeled as integers, and the application version
/// is a producer token (e.g. `"16.0000"`) retained verbatim as a string.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppProperties {
    /// `Application` — the producing application.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application: Option<String>,
    /// `AppVersion` — the producer version token, retained verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    /// `Company`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    /// `Manager`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manager: Option<String>,
    /// `Template`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// `TotalTime` — total editing time in minutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_time: Option<i64>,
    /// `Pages`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<i64>,
    /// `Words`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<i64>,
    /// `Characters`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characters: Option<i64>,
    /// `CharactersWithSpaces`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characters_with_spaces: Option<i64>,
    /// `Lines`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<i64>,
    /// `Paragraphs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraphs: Option<i64>,
    /// `DocSecurity` — the document-security flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_security: Option<i32>,
    /// `HyperlinkBase`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink_base: Option<String>,
    /// `ScaleCrop`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_crop: Option<bool>,
    /// `LinksUpToDate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links_up_to_date: Option<bool>,
    /// `SharedDoc`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_doc: Option<bool>,
    /// `TitlesOfParts` — the `vt:vector` of document-part titles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub titles_of_parts: Vec<String>,
    /// `HeadingPairs` — the `vt:vector` of (heading-group, count) pairs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heading_pairs: Vec<HeadingPair>,
}

impl AppProperties {
    /// Whether no application property is set (so `docProps/app.xml` is omitted).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn validate(&self) -> Result<(), ModelError> {
        for (value, field) in [
            (&self.application, "app.application"),
            (&self.company, "app.company"),
            (&self.manager, "app.manager"),
            (&self.template, "app.template"),
            (&self.hyperlink_base, "app.hyperlinkBase"),
        ] {
            check_text(value, MAX_META_TEXT, field)?;
        }
        check_text(&self.app_version, MAX_META_TOKEN, "app.appVersion")?;
        for (value, field) in [
            (self.total_time, "app.totalTime"),
            (self.pages, "app.pages"),
            (self.words, "app.words"),
            (self.characters, "app.characters"),
            (self.characters_with_spaces, "app.charactersWithSpaces"),
            (self.lines, "app.lines"),
            (self.paragraphs, "app.paragraphs"),
        ] {
            if let Some(value) = value {
                check_domain(value >= 0, field)?;
            }
        }
        if let Some(value) = self.doc_security {
            check_domain(value >= 0, "app.docSecurity")?;
        }
        check_domain(
            self.titles_of_parts.len() <= MAX_META_ITEMS,
            "app.titlesOfParts",
        )?;
        for title in &self.titles_of_parts {
            check_domain(title.len() <= MAX_META_TEXT, "app.titlesOfParts")?;
        }
        check_domain(
            self.heading_pairs.len() <= MAX_META_ITEMS,
            "app.headingPairs",
        )?;
        for pair in &self.heading_pairs {
            check_domain(
                !pair.name.is_empty() && pair.name.len() <= MAX_META_TEXT,
                "app.headingPairs.name",
            )?;
        }
        Ok(())
    }
}

/// A typed custom-property value (`docProps/custom.xml`). The common `vt:*`
/// variants are modeled directly; anything else is preserved through `Other`
/// (its `vt:` local name plus verbatim text). Floating-point (`r8`) and
/// timestamp (`filetime`) values are kept verbatim as strings so the exact bytes
/// round-trip and the model stays `Eq`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CustomValue {
    /// A text value (`vt:lpwstr` / `vt:lpstr`).
    Text {
        /// The string value.
        value: String,
    },
    /// A 32-bit signed integer (`vt:i4`).
    I4 {
        /// The integer value.
        value: i32,
    },
    /// An IEEE double (`vt:r8`), retained verbatim as written.
    R8 {
        /// The number as written.
        value: String,
    },
    /// A boolean (`vt:bool`).
    Bool {
        /// The boolean value.
        value: bool,
    },
    /// A timestamp (`vt:filetime`), retained verbatim as written.
    FileTime {
        /// The timestamp as written.
        value: String,
    },
    /// Any other `vt:*` variant, preserving its local name and verbatim text.
    Other {
        /// The `vt:` element local name (e.g. `lpstr`, `ui4`, `date`).
        kind: String,
        /// The value as written.
        value: String,
    },
}

/// One custom document property (`docProps/custom.xml`): a name and a typed
/// value. The OPC `fmtid`/`pid` bookkeeping is regenerated deterministically on
/// write and is not modeled.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomProperty {
    /// The property name (`property/@name`).
    pub name: String,
    /// The typed value.
    pub value: CustomValue,
}

/// Document metadata: the three OPC/Office property groups. Additive and
/// optional on `Document`; a group is omitted from the package when empty,
/// matching producer behavior.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentProperties {
    /// Package core properties (`docProps/core.xml`).
    #[serde(default, skip_serializing_if = "CoreProperties::is_empty")]
    pub core: CoreProperties,
    /// Extended/application properties (`docProps/app.xml`).
    #[serde(default, skip_serializing_if = "AppProperties::is_empty")]
    pub app: AppProperties,
    /// Custom properties (`docProps/custom.xml`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<CustomProperty>,
}

impl DocumentProperties {
    /// Whether every group is empty (so no `docProps/*` part is written and the
    /// value need not be attached to a document at all).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.core.is_empty() && self.app.is_empty() && self.custom.is_empty()
    }

    /// Validates every metadata invariant (length and domain bounds), first
    /// failure wins.
    pub fn validate(&self) -> Result<(), ModelError> {
        self.core.validate()?;
        self.app.validate()?;
        check_domain(self.custom.len() <= MAX_META_ITEMS, "custom")?;
        for property in &self.custom {
            check_domain(
                !property.name.is_empty() && property.name.len() <= MAX_META_TOKEN,
                "custom.name",
            )?;
            match &property.value {
                CustomValue::Text { value }
                | CustomValue::R8 { value }
                | CustomValue::FileTime { value } => {
                    check_domain(value.len() <= MAX_META_TEXT, "custom.value")?;
                }
                CustomValue::Other { kind, value } => {
                    check_domain(
                        !kind.is_empty() && kind.len() <= MAX_META_TOKEN,
                        "custom.value.kind",
                    )?;
                    check_domain(value.len() <= MAX_META_TEXT, "custom.value")?;
                }
                CustomValue::I4 { .. } | CustomValue::Bool { .. } => {}
            }
        }
        Ok(())
    }
}

fn check_text(value: &Option<String>, max: usize, field: &'static str) -> Result<(), ModelError> {
    if let Some(value) = value {
        check_domain(value.len() <= max, field)?;
    }
    Ok(())
}

fn check_domain(condition: bool, property: &'static str) -> Result<(), ModelError> {
    if condition {
        Ok(())
    } else {
        Err(ModelError::PropertyValueOutOfDomain { property })
    }
}
