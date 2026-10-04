// SPDX-License-Identifier: Apache-2.0

//! Two booleans that were read nowhere in the crate: `w:style@w:customStyle`
//! (the author's style versus an application built-in) and
//! `w:hyperlink@w:history` (add this link to the viewed-hyperlinks list).
//!
//! # Why this file exists
//!
//! `docs/161` §4 is the implementation record; the measurement it rests on is
//! `160` §3.2 (a sibling branch, not yet merged), which ranked both as real
//! losses on the **attribute** axis — the
//! axis `casual-doc-export/tests/source_element_coverage.rs` says in its own
//! module doc it does not gate, because it asks whether an element NAME vanished
//! without a finding. An attribute can therefore disappear with a compatibility
//! report of zero entries, which is what both of these did.
//!
//! The two take different fixes, and `docs/161` §4 records why rather than
//! leaving it to look like a preference:
//!
//! * **`@w:customStyle` is modelled.** Measured at 274 `w:style` elements across
//!   twelve of the owner's nineteen documents. Reporting 274 occurrences of a
//!   construct this common is the report-noise class HF-174 is about, and the
//!   flag is not cosmetic: Word uses it to decide which styles a template
//!   re-attach may replace.
//! * **`@w:history` is reported.** Adding a field to `v1::Hyperlink` breaks every
//!   struct literal of it — Rust has no source-compatible way to add one
//!   (`SKILL` §5a) — and there are 37 across `casual-doc-edit`,
//!   `casual-doc-transaction` and `casual-doc-wasm`, three crates other lanes
//!   own. So the silence is closed and the model half waits for a lane that owns
//!   those files.
//!
//! # The trap this file is written around
//!
//! A sibling guard asked whether written XML `contains("collapsed")` and passed
//! because a *heading's title text* held the substring. So the probe's body text
//! below reads `customStyle history` on purpose, and every question here is
//! asked through [`styles`] and [`hyperlinks`], which walk XML element and
//! attribute names.
//!
//! The report question has its own version of the same trap, and it is worse: a
//! feature string is `element/@attribute`, so a substring test for `history`
//! matches any future `*/@history` and a test for `hyperlink` matches the
//! element-level finding raised when a hyperlink's target does not resolve.
//! [`finding_for`] therefore matches on the entry's **`location` pair** —
//! `(element, attribute)` — and reads the feature only to report it.
//!
//! # What this file does NOT claim
//!
//! It does not claim `@w:history` survives a save: it does not, and the finding
//! is the point. Nor does it claim the editor marks a style it mints as custom —
//! `casual-doc-wasm`'s create-style-from-selection path still writes
//! `custom_style: false`, which is wrong and is recorded as the open remainder of
//! `109` FID-R-11 rather than fixed from outside that lane.
//!
//! # Complexity
//!
//! `O(package bytes)` per test: one import and one export of one small in-test
//! package. Test-time only.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use casual_doc_export::export_document_with_retained_parts;
use casual_doc_import::{CompatibilityEntry, ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{Document, StyleKind};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use quick_xml::Reader;
use quick_xml::events::Event;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.invalid/one" TargetMode="External"/>
</Relationships>"#;

/// Two paragraph styles: the author's, and a built-in. The built-in is the
/// control — it carries no `@w:customStyle`, and must still carry none after a
/// save, so a writer that stamped the attribute on everything fails here.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:styleId="AuthorVoice" w:customStyle="1"><w:name w:val="Author Voice"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style>
</w:styles>"#;

/// Two hyperlinks: one carrying `@w:history`, one not. The body text names both
/// attributes so a substring guard passes while proving nothing.
const DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
 xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>
<w:p><w:pPr><w:pStyle w:val="AuthorVoice"/></w:pPr>
<w:hyperlink r:id="rId9" w:history="1"><w:r><w:t>customStyle history</w:t></w:r></w:hyperlink>
</w:p>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
<w:bookmarkStart w:id="1" w:name="target"/><w:bookmarkEnd w:id="1"/>
<w:hyperlink w:anchor="target"><w:r><w:t>plain</w:t></w:r></w:hyperlink>
</w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
</w:body></w:document>"#;

fn package() -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/document.xml", DOCUMENT),
        ("word/styles.xml", STYLES),
    ] {
        zip.start_file(name, options).expect("a fresh zip entry");
        zip.write_all(body.as_bytes()).expect("writing a zip entry");
    }
    zip.finish().expect("finishing the zip");
    buffer.into_inner()
}

struct Imported {
    document: Document,
    retained: casual_doc_import::RetainedParts,
    entries: Vec<CompatibilityEntry>,
}

fn import(bytes: &[u8]) -> Imported {
    let mut package =
        DocxPackage::open(bytes, PackageLimits::default()).expect("the package is admitted");
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the package imports");
    Imported {
        document: import.document,
        retained: import.retained_parts,
        entries: import.report.entries,
    }
}

fn write_back(imported: &Imported) -> Vec<u8> {
    export_document_with_retained_parts(&imported.document, &BTreeMap::new(), &imported.retained)
        .expect("the model is writable")
        .bytes
}

/// One XML part of a package, by the suffix of its name.
fn part(bytes: &[u8], suffix: &str) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let name = archive
        .file_names()
        .map(str::to_owned)
        .find(|name| name.ends_with(suffix))
        .unwrap_or_else(|| panic!("the package has a part ending {suffix}"));
    let mut body = Vec::new();
    archive
        .by_name(&name)
        .expect("a listed entry")
        .read_to_end(&mut body)
        .expect("a readable entry");
    body
}

/// Every element of `local` name in `xml`, as a map from the attribute named by
/// `key` to the value of the attribute named by `flag` (absent as `None`).
///
/// Walks element and attribute names; never searches the bytes for a substring.
/// Both names are compared **as written**, so the `w:` prefix is pinned.
fn flagged(xml: &[u8], local: &str, key: &[u8], flag: &[u8]) -> Vec<(String, Option<String>)> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut found = Vec::new();
    loop {
        let element = match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element) | Event::Empty(element)) => element,
            Ok(Event::Eof) => break,
            Ok(_) => continue,
            Err(error) => panic!("the part is well-formed XML: {error}"),
        };
        if element.local_name().as_ref() != local.as_bytes() {
            continue;
        }
        let mut id = None;
        let mut value = None;
        for attribute in element.attributes() {
            let attribute = attribute.expect("a well-formed attribute");
            let name = attribute.key.as_ref();
            if name == key {
                id = Some(String::from_utf8_lossy(attribute.value.as_ref()).into_owned());
            } else if name == flag {
                value = Some(String::from_utf8_lossy(attribute.value.as_ref()).into_owned());
            }
        }
        if let Some(id) = id {
            found.push((id, value));
        }
    }
    found
}

/// The styles of a package, as `(w:name@w:val, @w:customStyle)`, sorted.
///
/// Keyed by the style's **name**, not by its `w:styleId`, and that is not a
/// detail: the exporter derives `w:styleId` from the model's internal id, so the
/// written token is a number and never the source's string. A guard keyed on
/// `w:styleId` compares the id-allocation scheme rather than the flag — the
/// "pinned to the circumstance" failure — and the first draft of this file did
/// exactly that and failed with `left: [("18446744073709551618", …)]`. The name
/// round-trips, so it is what identifies a style across a save.
fn styles(bytes: &[u8]) -> Vec<(String, Option<String>)> {
    let xml = part(bytes, "styles.xml");
    let mut reader = Reader::from_reader(xml.as_slice());
    let mut buffer = Vec::new();
    let mut found = Vec::new();
    let mut open: Option<Option<String>> = None;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) if element.local_name().as_ref() == b"style" => {
                open = Some(attribute_as_written(&element, b"w:customStyle"));
            }
            Ok(Event::Empty(element) | Event::Start(element))
                if element.local_name().as_ref() == b"name" =>
            {
                if let Some(flag) = open.clone()
                    && let Some(name) = attribute_as_written(&element, b"w:val")
                {
                    found.push((name, flag));
                }
            }
            Ok(Event::End(element)) if element.local_name().as_ref() == b"style" => open = None,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => panic!("the styles part is well-formed XML: {error}"),
        }
    }
    found.sort();
    found
}

/// One attribute of `element`, matched on its name **as written** so the `w:`
/// prefix is pinned.
fn attribute_as_written(
    element: &quick_xml::events::BytesStart<'_>,
    name: &[u8],
) -> Option<String> {
    element.attributes().find_map(|attribute| {
        let attribute = attribute.expect("a well-formed attribute");
        (attribute.key.as_ref() == name)
            .then(|| String::from_utf8_lossy(attribute.value.as_ref()).into_owned())
    })
}

/// The hyperlinks of a package's body, as `(r:id or w:anchor, @w:history)`.
///
/// Keyed by whichever of the two target attributes the link carries, so the two
/// probe links are distinguishable without depending on document order.
fn hyperlinks(bytes: &[u8]) -> Vec<(String, Option<String>)> {
    let body = part(bytes, "document.xml");
    let mut found = flagged(&body, "hyperlink", b"r:id", b"w:history");
    found.extend(flagged(&body, "hyperlink", b"w:anchor", b"w:history"));
    found.sort();
    found
}

/// The one report entry charged to `element/@attribute`, found by its
/// **location pair** rather than by its feature text.
///
/// The feature string is `element/@attribute`, so a substring search for
/// `history` would also match a future `*/@history` and a search for `hyperlink`
/// matches the ELEMENT-level finding the importer raises when a link's target
/// does not resolve. Matching the structured location cannot confuse the two.
fn finding_for<'a>(
    entries: &'a [CompatibilityEntry],
    element: &str,
    attribute: &str,
) -> Option<&'a CompatibilityEntry> {
    entries.iter().find(|entry| {
        entry.location.element.as_deref() == Some(element)
            && entry.location.attribute.as_deref() == Some(attribute)
    })
}

/// The pin on the source side: the probe carries one flagged and one unflagged
/// instance of each construct, so nothing below can pass vacuously.
#[test]
fn the_probe_package_carries_one_flagged_and_one_unflagged_of_each() {
    assert_eq!(
        styles(&package()),
        vec![
            ("Author Voice".to_owned(), Some("1".to_owned())),
            ("heading 1".to_owned(), None),
        ],
        "one custom style and one built-in control"
    );
    assert_eq!(
        hyperlinks(&package()),
        vec![
            ("rId9".to_owned(), Some("1".to_owned())),
            ("target".to_owned(), None),
        ],
        "one hyperlink with `@w:history` and one without"
    );
}

/// `@w:customStyle` reaches the model, and the built-in control does not acquire
/// it.
#[test]
fn a_custom_style_is_marked_custom_in_the_model_and_a_builtin_is_not() {
    let imported = import(&package());
    let by_name: BTreeMap<&str, bool> = imported
        .document
        .definitions()
        .styles
        .iter()
        .filter(|(_, style)| style.kind == StyleKind::Paragraph)
        .filter_map(|(_, style)| Some((style.name.as_deref()?, style.custom_style)))
        .collect();
    assert_eq!(
        by_name.get("Author Voice"),
        Some(&true),
        "the author's style is marked custom; styles seen: {by_name:?}"
    );
    assert_eq!(
        by_name.get("heading 1"),
        Some(&false),
        "the built-in is not marked custom; styles seen: {by_name:?}"
    );
}

/// The invariant at the altitude that matters: **the flag survives the save**,
/// on the style that had it and only on that one. Compared as a whole list, so a
/// writer that stamped `@w:customStyle` on every style fails here too.
#[test]
fn the_custom_style_flag_survives_a_save() {
    let written = write_back(&import(&package()));
    assert_eq!(
        styles(&written),
        styles(&package()),
        "the written package's styles carry the same `@w:customStyle` the source's did"
    );
}

/// A second save changes nothing, so the flag is not being re-derived
/// differently on each pass.
#[test]
fn a_second_save_keeps_the_custom_style_flag() {
    let once = write_back(&import(&package()));
    let twice = write_back(&import(&once));
    assert_eq!(
        styles(&twice),
        styles(&once),
        "re-importing and re-writing leaves `@w:customStyle` unchanged"
    );
}

/// `@w:history` does not survive the save — and **says so**. The guarantee is
/// "the value survives the save OR is reported", which stays green under a
/// wrong-value mutation and red under silence, and silence is what this was.
#[test]
fn a_hyperlinks_history_flag_is_reported_when_it_cannot_be_kept() {
    let imported = import(&package());

    let entry = finding_for(&imported.entries, "hyperlink", "history").unwrap_or_else(|| {
        panic!(
            "`w:hyperlink@w:history` must be reported: it is dropped, and a drop with no \
             finding is the silent loss `SKILL` §1 says the local-first position depends on \
             detecting. Entries: {:?}",
            imported
                .entries
                .iter()
                .map(|entry| entry.feature.as_str())
                .collect::<Vec<_>>()
        )
    });
    assert_eq!(
        entry.feature, "hyperlink/@history",
        "the feature string names the element and the attribute"
    );
    assert_eq!(
        entry.occurrences, 1,
        "ONE of the probe's two hyperlinks carries the attribute, and the finding counts \
         occurrences rather than raising one entry per link — which is what keeps a common \
         attribute from becoming the report noise HF-174 is about"
    );

    // And the drop it is reporting is real, so the finding is not decorative.
    let written = write_back(&imported);
    assert!(
        hyperlinks(&written)
            .iter()
            .all(|(_, history)| history.is_none()),
        "the writer emits no `@w:history`, which is exactly why the finding has to exist"
    );
}

/// The element-level hyperlink finding and the attribute-level one are different
/// findings, and the location pair is what tells them apart.
///
/// Without this, `finding_for` could be satisfied by the wrong entry on a
/// document whose link target does not resolve — the confusion the module doc
/// describes, made into a check rather than a comment.
#[test]
fn the_attribute_finding_is_not_the_element_level_hyperlink_finding() {
    let imported = import(&package());
    let attribute = finding_for(&imported.entries, "hyperlink", "history")
        .expect("the attribute finding exists");
    assert_eq!(attribute.location.attribute.as_deref(), Some("history"));
    let element_level = imported.entries.iter().find(|entry| {
        entry.location.element.as_deref() == Some("hyperlink") && entry.location.attribute.is_none()
    });
    assert!(
        element_level.is_none(),
        "this probe's links both resolve, so there must be NO element-level `hyperlink` \
         finding — if one appears, the attribute guard above could be matching it instead: \
         {element_level:?}"
    );
}
