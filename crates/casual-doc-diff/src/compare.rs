//! Typed comparison of properties and definitions.
//!
//! # Typed fields are reflected, not listed
//!
//! [`differing_field_paths`] serializes two values of the same model type and
//! reports the **field paths** that differ: `alignment`, `spacing.beforeTwips`,
//! `pageSize.orientation`. The field names come from the type's own `serde`
//! attributes, which are the same names the JSON snapshot format and the wasm
//! facade already use.
//!
//! This is deliberately *not* "diff by serialized JSON", which `docs/140` §19
//! rejects — that rejection is about aligning a document's **structure** by its
//! serialization, where whitespace and ordering noise swamp the semantics. Here
//! the structure is aligned semantically by [`crate::align`], and reflection is
//! used only inside a property bag, where the serde field names *are* the typed
//! fields, one to one.
//!
//! The reason it must be reflection and not a hand-written list is the defect
//! SKILL §5a describes: adding a field to a struct is a breaking change to every
//! literal that constructs it, and a *silent* change to every hand-maintained
//! list that reads it. A property added to `ParagraphProperties` is compared by
//! this crate on the day it exists, without this crate being edited.
//!
//! # Definitions are checked for completeness by a guard, not by reflection
//!
//! `Definitions` holds the headers, footers, notes and comments, whose content is
//! the whole document; serializing it to reflect its field names would be an
//! O(document) JSON encode of both sides. So the field names are named here
//! instead — and [`DEFINITION_FIELDS`] plus [`STORY_FIELDS`] is asserted against
//! a fully-populated `Definitions`' own serde keys by
//! `definition_field_coverage_is_complete`. Adding a field to the model fails
//! that test until this module says what happens to it.
//!
//! # Complexity
//!
//! [`differing_field_paths`] is **O(size of the two values)** — a property struct,
//! never a document. [`compare_definitions`] is **O(definitions)**: styles,
//! numbering, sections, media, bookmarks and the theme parts, none of which scale
//! with body length.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use casual_doc_model::v1::Document;
use serde::Serialize;
use serde_json::Value;

use crate::record::{
    ChangeSpec, DiffAnchor, DiffChange, DiffFamily, DiffKind, FindingCode, FindingSet, Story,
};

/// How many differing field paths one record may name before the rest is folded
/// into a `Truncated` finding.
pub const MAX_FIELD_PATHS: usize = 24;

/// `Definitions` fields whose content is compared by the story pipeline, because
/// they hold block content rather than definitions.
pub const STORY_FIELDS: &[&str] = &["headers", "footers", "footnotes", "endnotes", "comments"];

/// `Definitions` fields compared by [`compare_definitions`].
pub const DEFINITION_FIELDS: &[&str] = &[
    "styles",
    "charts",
    "abstractNumbering",
    "numbering",
    "sections",
    "media",
    "bookmarks",
    "fieldRanges",
    "documentDefaults",
    "latentStyles",
    "fontTable",
    "fontScheme",
    "colorScheme",
    "formatSchemeXml",
    "formatScheme",
    "shapeStyles",
    "settings",
    "people",
];

/// Constructs this engine retains **verbatim** rather than modeling, so a
/// difference inside one can be located but not characterised. Every one of
/// these produces a `NotCompared` finding alongside its change record, which is
/// the same contract the import compatibility report keeps: loss is reported, not
/// quietly absorbed.
pub const OPAQUE_CONSTRUCTS: &[&str] = &[
    "formatSchemeXml",
    "abstractNumbering",
    "numbering",
    "fieldRanges",
    // Shape theme-style references are keyed by a `NodeId` the parse minted, so two
    // files' tables cannot be paired — the same reason `numbering` and `fieldRanges`
    // are here. A difference is therefore located, not characterised.
    "shapeStyles",
    // A chart projection, for both of the reasons this list exists at once. It is
    // keyed by a `ChartId` the parse minted, so two files' projections cannot be
    // paired — the same reason `numbering` and `fieldRanges` are here. And it is a
    // *read projection of a retained part* (`docs/155` §6.1): the authority for a
    // chart's content is `word/charts/chartN.xml`, whose bytes this engine does not
    // hold, so even a paired projection could not characterise the whole change.
    "charts",
];

/// Media part digests, keyed by package part name.
///
/// The `Document` models a media *reference* (part name, content type,
/// relationship id) and not its bytes, which live in the host's resource table.
/// A caller that can supply digests gets image replacement detected; a caller
/// that cannot gets a `MissingResource` finding saying so, rather than a diff
/// that quietly calls two different images the same image.
pub type MediaDigests = BTreeMap<String, u128>;

/// The field paths at which two values of the same type differ.
///
/// Returns the paths and whether the list was cut short at [`MAX_FIELD_PATHS`].
#[must_use]
pub fn differing_field_paths<T: Serialize>(left: &T, right: &T) -> (Vec<String>, bool) {
    let left = serde_json::to_value(left).unwrap_or(Value::Null);
    let right = serde_json::to_value(right).unwrap_or(Value::Null);
    let mut paths = Vec::new();
    walk(&left, &right, &mut String::new(), &mut paths);
    let truncated = paths.len() > MAX_FIELD_PATHS;
    paths.truncate(MAX_FIELD_PATHS);
    (paths, truncated)
}

/// Recursively collects differing paths. Stops descending once a path is
/// recorded, so one record names `spacing.beforeTwips` rather than that plus
/// `spacing`.
fn walk(left: &Value, right: &Value, prefix: &mut String, out: &mut Vec<String>) {
    if left == right {
        return;
    }
    match (left, right) {
        (Value::Object(left_map), Value::Object(right_map)) => {
            let keys: BTreeSet<&String> = left_map.keys().chain(right_map.keys()).collect();
            for key in keys {
                let base = prefix.len();
                if !prefix.is_empty() {
                    prefix.push('.');
                }
                prefix.push_str(key);
                let null = Value::Null;
                walk(
                    left_map.get(key).unwrap_or(&null),
                    right_map.get(key).unwrap_or(&null),
                    prefix,
                    out,
                );
                prefix.truncate(base);
            }
        }
        // One side absent and the other an object is the ordinary shape of an
        // `Option<Spacing>`: the reader wants `spacing.beforeTwips`, not
        // `spacing`. Descending against an empty object gives the exact path.
        (Value::Null, Value::Object(_)) | (Value::Object(_), Value::Null) => {
            let empty = Value::Object(serde_json::Map::new());
            let (left, right) = if left.is_null() {
                (&empty, right)
            } else {
                (left, &empty)
            };
            walk(left, right, prefix, out);
        }
        (Value::Array(left_items), Value::Array(right_items))
            if left_items.len() == right_items.len() =>
        {
            for (index, (left_item, right_item)) in
                left_items.iter().zip(right_items.iter()).enumerate()
            {
                let base = prefix.len();
                prefix.push_str(&format!("[{index}]"));
                walk(left_item, right_item, prefix, out);
                prefix.truncate(base);
            }
        }
        _ => out.push(if prefix.is_empty() {
            "value".to_owned()
        } else {
            prefix.clone()
        }),
    }
}

/// An anchor that is not in a story: a definition, a section, or metadata.
fn definitions_anchor() -> DiffAnchor {
    DiffAnchor {
        story: Story::Definitions,
        path: Vec::new(),
        node: None,
        start: 0,
        end: 0,
    }
}

/// One definition-level record.
fn definition_change(
    family: DiffFamily,
    kind: DiffKind,
    name: &str,
    fields: Vec<String>,
    present_left: bool,
    present_right: bool,
) -> DiffChange {
    let mut fields = fields;
    fields.insert(0, name.to_owned());
    ChangeSpec {
        family: Some(family),
        kind: Some(kind),
        left: present_left.then(definitions_anchor),
        right: present_right.then(definitions_anchor),
        fields,
        ..ChangeSpec::default()
    }
    .build()
}

/// Compares everything that is not story content: styles, numbering, sections,
/// media, bookmarks, the theme parts, settings, and document metadata.
///
/// **O(definitions)**.
pub fn compare_definitions(
    left: &Document,
    right: &Document,
    digests: Option<(&MediaDigests, &MediaDigests)>,
    findings: &mut FindingSet,
) -> Vec<DiffChange> {
    let mut changes = Vec::new();
    let (left_definitions, right_definitions) = (left.definitions(), right.definitions());

    // Styles: keyed by kind plus the human name Word writes, which is stable
    // across two files. `StyleId` is not — it wraps a NodeId the parse minted.
    let left_styles = style_index(left);
    let right_styles = style_index(right);
    for (key, style) in &left_styles {
        match right_styles.get(key) {
            None => changes.push(definition_change(
                DiffFamily::Definition,
                DiffKind::Deletion,
                &format!("styles.{key}"),
                Vec::new(),
                true,
                false,
            )),
            Some(other) => {
                let (fields, truncated) = differing_field_paths(style, other);
                if !fields.is_empty() {
                    if truncated {
                        findings.note(FindingCode::Truncated, "styleFields");
                    }
                    changes.push(definition_change(
                        DiffFamily::Definition,
                        DiffKind::Property,
                        &format!("styles.{key}"),
                        fields,
                        true,
                        true,
                    ));
                }
            }
        }
    }
    for key in right_styles.keys() {
        if !left_styles.contains_key(key) {
            changes.push(definition_change(
                DiffFamily::Definition,
                DiffKind::Insertion,
                &format!("styles.{key}"),
                Vec::new(),
                false,
                true,
            ));
        }
    }

    // Sections: an ordered list, so index pairing IS the semantics. A section
    // added or removed shifts the ones after it, which is reported as such
    // rather than pretended away.
    let left_sections = &left_definitions.sections;
    let right_sections = &right_definitions.sections;
    for index in 0..left_sections.len().max(right_sections.len()) {
        match (left_sections.get(index), right_sections.get(index)) {
            (Some(left_section), Some(right_section)) => {
                let (fields, truncated) = differing_field_paths(left_section, right_section);
                if !fields.is_empty() {
                    if truncated {
                        findings.note(FindingCode::Truncated, "sectionFields");
                    }
                    changes.push(definition_change(
                        DiffFamily::Section,
                        DiffKind::Property,
                        &format!("sections[{index}]"),
                        fields,
                        true,
                        true,
                    ));
                }
            }
            (Some(_), None) => changes.push(definition_change(
                DiffFamily::Section,
                DiffKind::Deletion,
                &format!("sections[{index}]"),
                Vec::new(),
                true,
                false,
            )),
            (None, Some(_)) => changes.push(definition_change(
                DiffFamily::Section,
                DiffKind::Insertion,
                &format!("sections[{index}]"),
                Vec::new(),
                false,
                true,
            )),
            (None, None) => {}
        }
    }

    // Media: keyed by package part name, which the source writes.
    let left_media = media_index(left);
    let right_media = media_index(right);
    let mut digest_gap_reported = false;
    for (part, reference) in &left_media {
        match right_media.get(part) {
            None => changes.push(definition_change(
                DiffFamily::Resource,
                DiffKind::Deletion,
                &format!("media.{part}"),
                Vec::new(),
                true,
                false,
            )),
            Some(other) => {
                let (mut fields, _) = differing_field_paths(reference, other);
                match digests {
                    Some((left_digests, right_digests)) => {
                        if left_digests.get(*part) != right_digests.get(*part) {
                            fields.push("bytes".to_owned());
                        }
                    }
                    None if !digest_gap_reported => {
                        // An image whose bytes were replaced under the same part
                        // name is invisible without digests. Say so once.
                        findings.note(FindingCode::MissingResource, "mediaBytes");
                        digest_gap_reported = true;
                    }
                    None => {}
                }
                if !fields.is_empty() {
                    changes.push(definition_change(
                        DiffFamily::Resource,
                        DiffKind::Property,
                        &format!("media.{part}"),
                        fields,
                        true,
                        true,
                    ));
                }
            }
        }
    }
    for part in right_media.keys() {
        if !left_media.contains_key(part) {
            changes.push(definition_change(
                DiffFamily::Resource,
                DiffKind::Insertion,
                &format!("media.{part}"),
                Vec::new(),
                false,
                true,
            ));
        }
    }

    // Bookmarks: keyed by the name as written.
    let left_bookmarks: BTreeSet<&str> = left_definitions
        .bookmarks
        .iter()
        .map(|(_, bookmark)| bookmark.name.as_str())
        .collect();
    let right_bookmarks: BTreeSet<&str> = right_definitions
        .bookmarks
        .iter()
        .map(|(_, bookmark)| bookmark.name.as_str())
        .collect();
    for name in left_bookmarks.difference(&right_bookmarks) {
        changes.push(definition_change(
            DiffFamily::Definition,
            DiffKind::Deletion,
            &format!("bookmarks.{name}"),
            Vec::new(),
            true,
            false,
        ));
    }
    for name in right_bookmarks.difference(&left_bookmarks) {
        changes.push(definition_change(
            DiffFamily::Definition,
            DiffKind::Insertion,
            &format!("bookmarks.{name}"),
            Vec::new(),
            false,
            true,
        ));
    }

    // The remaining definition fields, each compared as one typed value. The
    // four keyed by a parse-minted id, plus the theme's verbatim XML, are
    // located but not characterised — and each one says so.
    compare_field(
        "abstractNumbering",
        &left_definitions.abstract_numbering,
        &right_definitions.abstract_numbering,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    compare_field(
        "numbering",
        &left_definitions.numbering,
        &right_definitions.numbering,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    compare_field(
        "fieldRanges",
        &left_definitions.field_ranges,
        &right_definitions.field_ranges,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    // Charts: located, not characterised, per `OPAQUE_CONSTRUCTS` above. The
    // serialization this does is O(chart data), which is bounded by the model's own
    // per-chart ceilings and does not scale with body length, and it only happens
    // when the two sides actually differ.
    compare_field(
        "charts",
        &left_definitions.charts,
        &right_definitions.charts,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    // Shape theme-style references: located, not characterised, per
    // `OPAQUE_CONSTRUCTS` above.
    compare_field(
        "shapeStyles",
        &left_definitions.shape_styles,
        &right_definitions.shape_styles,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    // `formatScheme` is deliberately NOT compared here, and that is the whole story
    // for it. It is a read projection of `formatSchemeXml`, which IS compared a few
    // lines above and is itself opaque, so a change to the theme's style lists is
    // already reported once. Comparing the projection as well would report one
    // authored change twice — as a `formatSchemeXml` difference and again as a
    // `formatScheme` difference — which is a worse answer than reporting it once.
    // It is listed in `DEFINITION_FIELDS` so the structural guard can see that this
    // decision was made rather than forgotten.
    compare_field(
        "documentDefaults",
        &left_definitions.document_defaults,
        &right_definitions.document_defaults,
        DiffFamily::Formatting,
        &mut changes,
        findings,
    );
    compare_field(
        "latentStyles",
        &left_definitions.latent_styles,
        &right_definitions.latent_styles,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    compare_field(
        "fontTable",
        &left_definitions.font_table,
        &right_definitions.font_table,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    compare_field(
        "fontScheme",
        &left_definitions.font_scheme,
        &right_definitions.font_scheme,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    compare_field(
        "colorScheme",
        &left_definitions.color_scheme,
        &right_definitions.color_scheme,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    compare_field(
        "formatSchemeXml",
        &left_definitions.format_scheme_xml,
        &right_definitions.format_scheme_xml,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    compare_field(
        "settings",
        &left_definitions.settings,
        &right_definitions.settings,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );
    compare_field(
        "people",
        &left_definitions.people,
        &right_definitions.people,
        DiffFamily::Definition,
        &mut changes,
        findings,
    );

    // Document metadata, and the page background, which is a document-level
    // property outside `Definitions`.
    compare_field(
        "properties",
        &left.properties(),
        &right.properties(),
        DiffFamily::Metadata,
        &mut changes,
        findings,
    );
    compare_field(
        "background",
        &left.background(),
        &right.background(),
        DiffFamily::Formatting,
        &mut changes,
        findings,
    );

    changes
}

/// Compares one named field and records a change plus, for an opaque construct,
/// a finding.
fn compare_field<T: PartialEq + Serialize>(
    name: &str,
    left: &T,
    right: &T,
    family: DiffFamily,
    changes: &mut Vec<DiffChange>,
    findings: &mut FindingSet,
) {
    if left == right {
        return;
    }
    let (fields, truncated) = differing_field_paths(left, right);
    if truncated {
        findings.note(FindingCode::Truncated, name);
    }
    if OPAQUE_CONSTRUCTS.contains(&name) {
        findings.note(FindingCode::NotCompared, name);
    }
    changes.push(definition_change(
        family,
        DiffKind::Property,
        name,
        fields,
        true,
        true,
    ));
}

/// Styles by `kind/name`, falling back to an ordinal for an unnamed style.
fn style_index(document: &Document) -> BTreeMap<String, &casual_doc_model::v1::Style> {
    document
        .definitions()
        .styles
        .iter()
        .enumerate()
        .map(|(ordinal, (_, style))| {
            let name = style.name.clone().unwrap_or_else(|| format!("#{ordinal}"));
            (format!("{:?}/{name}", style.kind), style)
        })
        .collect()
}

/// Media references by package part name.
fn media_index(document: &Document) -> BTreeMap<&str, &casual_doc_model::v1::MediaReference> {
    document
        .definitions()
        .media
        .iter()
        .map(|(_, reference)| (reference.part_name.as_str(), reference))
        .collect()
}
