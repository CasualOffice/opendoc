// SPDX-License-Identifier: Apache-2.0

//! What an EDITED save of `sample.docx` keeps, against what the import report
//! promised (`109` FID-AT-07).
//!
//! # Why this file exists
//!
//! The owner opened `sample.docx` in the webapp, read the compatibility panel —
//! every finding said "Kept in the file", or "Shown approximately; the original
//! is kept in the file" — then edited and saved. The webapp opens a document
//! with `retain_source` (`ImportMode::Retention`), under which the import report
//! resolves every finding to `preserved`, because the verbatim source snapshot
//! does keep it — for a save of the UNCHANGED file, which returns the original
//! bytes. An edited save regenerates the body, the settings, the theme and the
//! document properties from the model, so whatever the model does not carry is
//! not in it. Before FID-AT-07 that save's own report was empty: the panel had
//! promised preservation, and the loss was silent.
//!
//! # What is asserted
//!
//! 1. **An unchanged save is the original file**, so it keeps everything
//!    (`exact_if_unchanged`, which is what the webapp tries first).
//! 2. **The honesty invariant, generically:** every import finding that claims
//!    `preserved` is either present in the edited save, or named `not-retained`
//!    by the edited save's own report. Nothing promised is lost in silence.
//! 3. **The table is pinned.** [`TABLE`] lists every finding `sample.docx`
//!    raises and whether an edited save carries it. A finding that appears, or
//!    one that moves between "kept" and "reported", fails here until the table
//!    is changed — so the movement is a decision in a diff, not drift.
//!
//! # How presence is judged
//!
//! Each row carries a probe over the edited package: the exact markup the
//! source carries, as the writer emits it. The theme's two names are judged by
//! the theme PART being the source's bytes, not by the name values: the
//! regenerated theme writes the fixed names `Office Theme` and `Office`, which
//! this file happens to use, so a value probe could not tell retention from
//! coincidence.
//!
//! # Complexity
//!
//! One import and three exports of a 75 KB package; test-time only.

use std::collections::BTreeMap;
use std::io::{Cursor, Read as _};

use casual_doc_io::{
    CompatibilityReport, DetectionRequest, ExportMode, ExportRequest, FormatId, FormatSelection,
    ImportArtifact, RetentionOutcome, builtin_registry, formats,
};
use casual_doc_model::v1::{BlockNode, InlineNode};

/// The owner's file, committed at the repository root.
const SAMPLE: &[u8] = include_bytes!("../../../sample.docx");

/// A package as part name to bytes.
type Package = BTreeMap<String, Vec<u8>>;

/// What the import report says about a construct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AtImport {
    /// A finding, which under `retain_source` resolves to `preserved`.
    Reported,
    /// Carried by the model, so not a finding at all. A row that was `Reported`
    /// becomes this when the model learns the construct: the row stays, so the
    /// table keeps saying what an edited save does with it.
    Modelled,
}

/// One construct `sample.docx` carries that the import report has named, and
/// what an edited save does with it.
struct Row {
    /// The import report's feature identifier.
    feature: &'static str,
    /// What the import report says about it today.
    at_import: AtImport,
    /// Whether the edited save carries it. `false` means the edited save's own
    /// report must name it `not-retained` — invariant 2 holds either way.
    kept_by_an_edited_save: bool,
    /// Whether `package` (an edited save) carries the construct.
    probe: fn(&Package, &Package) -> bool,
}

use AtImport::{Modelled, Reported};

/// Every construct `sample.docx` raised a finding for when the owner opened it
/// as the webapp opens it (`retain_source`).
///
/// The columns are the honesty table: the import verdict (`Reported` is
/// `preserved` under the byte floor; `Modelled` is no finding), and "present in
/// an edited save?". An unchanged save is the source file, so it keeps every
/// row.
const TABLE: &[Row] = &[
    // Revision-save ids: Word bookkeeping for compare/merge, deliberately not
    // modelled (`35-DISPOSITION-TAXONOMY.md`). Reported by the edited save.
    Row {
        at_import: Reported,
        feature: "docx.rsid",
        kept_by_an_edited_save: false,
        probe: |edited, _| part_contains(edited, "word/document.xml", "w:rsidR="),
    },
    // The picture's own name, `opendoc_rendering_pipeline.png`, beside the
    // frame's `Picture 1` (python-docx's shape): `ObjectName::inner_name`,
    // FID-AT-08.
    Row {
        at_import: Modelled,
        feature: "cNvPr/@name",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            count(
                edited,
                "word/document.xml",
                r#"name="opendoc_rendering_pipeline.png""#,
            ) == 2
        },
    },
    // `noChangeAspect` on each picture's frame: `ObjectName::locks`,
    // FID-AT-09 — what keeps a corner drag proportional.
    Row {
        at_import: Modelled,
        feature: "graphicFrameLocks",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            count(
                edited,
                "word/document.xml",
                r#"<a:graphicFrameLocks noChangeAspect="1"/>"#,
            ) == 2
        },
    },
    // `docProps/app.xml`'s `HyperlinksChanged`: `AppProperties`, FID-AT-10.
    Row {
        at_import: Modelled,
        feature: "HyperlinksChanged",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            part_contains(
                edited,
                "docProps/app.xml",
                "<HyperlinksChanged>false</HyperlinksChanged>",
            )
        },
    },
    // `word/settings.xml` children Word writes into every document it saves:
    // typed settings, and `m:mathPr`/`w:shapeDefaults` retained verbatim
    // (FID-AT-10).
    Row {
        at_import: Modelled,
        feature: "decimalSymbol",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            part_contains(
                edited,
                "word/settings.xml",
                r#"<w:decimalSymbol w:val="."/>"#,
            )
        },
    },
    Row {
        at_import: Modelled,
        feature: "listSeparator",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            part_contains(
                edited,
                "word/settings.xml",
                r#"<w:listSeparator w:val=","/>"#,
            )
        },
    },
    Row {
        at_import: Modelled,
        feature: "defaultImageDpi",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            part_contains(
                edited,
                "word/settings.xml",
                r#"<w14:defaultImageDpi w14:val="300"/>"#,
            )
        },
    },
    Row {
        at_import: Modelled,
        feature: "doNotAutoCompressPictures",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            part_contains(
                edited,
                "word/settings.xml",
                "<w:doNotAutoCompressPictures/>",
            )
        },
    },
    Row {
        at_import: Modelled,
        feature: "docId",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            part_contains(
                edited,
                "word/settings.xml",
                r#"<w14:docId w14:val="24062061"/>"#,
            )
        },
    },
    Row {
        at_import: Modelled,
        feature: "mathPr",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            part_contains(
                edited,
                "word/settings.xml",
                r#"<m:mathPr><m:mathFont m:val="Cambria Math"/><m:brkBin m:val="before"/>"#,
            ) && part_contains(
                edited,
                "word/settings.xml",
                r#"<m:naryLim m:val="undOvr"/></m:mathPr>"#,
            )
        },
    },
    Row {
        at_import: Modelled,
        feature: "savePreviewPicture",
        kept_by_an_edited_save: true,
        probe: |edited, _| part_contains(edited, "word/settings.xml", "<w:savePreviewPicture/>"),
    },
    Row {
        at_import: Modelled,
        feature: "shapeDefaults",
        kept_by_an_edited_save: true,
        probe: |edited, _| {
            part_contains(
                edited,
                "word/settings.xml",
                r#"<o:shapedefaults v:ext="edit" spidmax="1027"/>"#,
            ) && part_contains(
                edited,
                "word/settings.xml",
                r#"<o:idmap v:ext="edit" data="1"/>"#,
            )
        },
    },
    Row {
        at_import: Modelled,
        feature: "useFELayout",
        kept_by_an_edited_save: true,
        probe: |edited, _| part_contains(edited, "word/settings.xml", "<w:useFELayout/>"),
    },
    // The theme part's own detail. Still findings — the model does not carry
    // it — but `preserved` against the theme part's own record, because the
    // save writes the source part back while the model's theme is unchanged
    // (`RetainedTheme`, FID-AT-03).
    Row {
        at_import: Reported,
        feature: "fontScheme/@name",
        kept_by_an_edited_save: true,
        probe: same_theme_part,
    },
    Row {
        at_import: Reported,
        feature: "theme/@name",
        kept_by_an_edited_save: true,
        probe: same_theme_part,
    },
    Row {
        at_import: Reported,
        feature: "objectDefaults",
        kept_by_an_edited_save: true,
        probe: |edited, source| {
            same_theme_part(edited, source)
                && part_contains(edited, "word/theme/theme1.xml", "<a:spDef>")
        },
    },
    // Whole parts the opaque side-table carries verbatim through any save.
    Row {
        at_import: Reported,
        // An empty bibliography store, reported under its class since `109`
        // FID-AT-18; the probe still asks about the part itself.
        feature: "docx.customXml.bibliography.empty",
        kept_by_an_edited_save: true,
        probe: |edited, source| same_part(edited, source, "customXml/item1.xml"),
    },
    Row {
        at_import: Reported,
        // Its identity record, likewise.
        feature: "docx.customXml.storeIdentity",
        kept_by_an_edited_save: true,
        probe: |edited, source| same_part(edited, source, "customXml/itemProps1.xml"),
    },
    Row {
        at_import: Reported,
        feature: "word/webSettings.xml",
        kept_by_an_edited_save: true,
        probe: |edited, source| same_part(edited, source, "word/webSettings.xml"),
    },
    // Derived from the content, so an edit makes them stale: left behind and
    // named by the edited save (`105` FID-R-05), kept by an unchanged one.
    Row {
        at_import: Reported,
        feature: "docProps/thumbnail.jpeg",
        kept_by_an_edited_save: false,
        probe: |edited, source| same_part(edited, source, "docProps/thumbnail.jpeg"),
    },
    Row {
        at_import: Reported,
        feature: "word/stylesWithEffects.xml",
        kept_by_an_edited_save: false,
        probe: |edited, source| same_part(edited, source, "word/stylesWithEffects.xml"),
    },
];

/// The export features that name a derived part an edit left behind. They are
/// the edited save's statement of the two stale rows, under the export's own
/// identifiers (`casual_doc_import::STALE_THUMBNAIL` and its sibling).
const STALE_PART_FEATURES: &[(&str, &str)] = &[
    ("docProps/thumbnail.jpeg", "docx.export.stale.thumbnail"),
    (
        "word/stylesWithEffects.xml",
        "docx.export.stale.styles_with_effects",
    ),
];

#[test]
fn an_unchanged_save_is_the_source_file() {
    let imported = import();
    let saved = save(&imported, ExportMode::ExactIfUnchanged, true);
    assert!(
        saved.0 == SAMPLE,
        "an unchanged save returns the original bytes, so it keeps every finding"
    );
    assert!(
        saved.1.entries.is_empty(),
        "and loses nothing: {:?}",
        saved.1
    );
}

#[test]
fn every_finding_an_edited_save_does_not_keep_is_named_by_that_save() {
    let mut imported = import();
    edit(&mut imported);
    let (bytes, report) = save(&imported, ExportMode::PreserveWhenSafe, false);
    let edited = unzip(&bytes);
    let source = unzip(SAMPLE);

    let mut silent: Vec<String> = Vec::new();
    for entry in &imported.report.entries {
        if entry.retention_outcome != RetentionOutcome::Preserved {
            continue;
        }
        let kept = TABLE
            .iter()
            .find(|row| row.feature == entry.feature)
            .is_some_and(|row| (row.probe)(&edited, &source));
        if !kept && !names_as_not_retained(&report, &entry.feature) {
            silent.push(format!(
                "{} ×{} ({:?})",
                entry.feature, entry.occurrences, entry.location.part_name
            ));
        }
    }
    assert!(
        silent.is_empty(),
        "the import report called these preserved; the edited save dropped them and \
         its report does not say so:\n  {}\nthe save's report: {:#?}",
        silent.join("\n  "),
        report.entries
    );
}

#[test]
fn the_table_is_what_an_edited_save_of_sample_docx_does() {
    let mut imported = import();
    let mut in_report: Vec<&str> = imported
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect();
    in_report.sort_unstable();
    let mut in_table: Vec<&str> = TABLE
        .iter()
        .filter(|row| row.at_import == Reported)
        .map(|row| row.feature)
        .collect();
    in_table.sort_unstable();
    assert_eq!(
        in_report, in_table,
        "every finding sample.docx raises has a `Reported` row, and every `Reported` \
         row is a finding"
    );
    for entry in &imported.report.entries {
        assert_eq!(
            entry.retention_outcome,
            RetentionOutcome::Preserved,
            "{}: the retention-mode verdict at import",
            entry.feature
        );
    }

    edit(&mut imported);
    let (bytes, report) = save(&imported, ExportMode::PreserveWhenSafe, false);
    let edited = unzip(&bytes);
    let source = unzip(SAMPLE);
    let mut moved: Vec<String> = Vec::new();
    for row in TABLE {
        let kept = (row.probe)(&edited, &source);
        let named = names_as_not_retained(&report, row.feature);
        if kept != row.kept_by_an_edited_save {
            moved.push(format!(
                "{}: the table says kept={}, the edited save has kept={kept} (named by its \
                 report: {named})",
                row.feature, row.kept_by_an_edited_save
            ));
        }
        if kept && named {
            moved.push(format!(
                "{}: carried by the edited save AND reported lost by it",
                row.feature
            ));
        }
    }
    assert!(
        moved.is_empty(),
        "the honesty table moved; change TABLE if that is intended:\n  {}",
        moved.join("\n  ")
    );
}

/// A save that regenerates without an edit loses the same detail, because
/// `source_unchanged` decides which DERIVED parts travel, not whether the body
/// is regenerated — so it must say so too.
#[test]
fn a_regenerating_save_of_an_unchanged_document_names_the_same_losses() {
    let imported = import();
    let (bytes, report) = save(&imported, ExportMode::PreserveWhenSafe, true);
    let saved = unzip(&bytes);
    let source = unzip(SAMPLE);
    for row in TABLE {
        if STALE_PART_FEATURES
            .iter()
            .any(|(feature, _)| *feature == row.feature)
        {
            // Not stale without an edit, so carried.
            assert!((row.probe)(&saved, &source), "{} is carried", row.feature);
            continue;
        }
        assert!(
            (row.probe)(&saved, &source) || names_as_not_retained(&report, row.feature),
            "{} is neither carried nor named",
            row.feature
        );
    }
}

/// A `semantic` save carries no side-table, so it regenerates even an unchanged
/// theme — and must name the theme's detail it therefore loses, which the
/// import called `preserved` against the theme part's own record (FID-AT-03).
#[test]
fn a_semantic_save_names_the_theme_detail_it_regenerates() {
    let imported = import();
    let (bytes, report) = save(&imported, ExportMode::Semantic, true);
    let saved = unzip(&bytes);
    let source = unzip(SAMPLE);
    assert!(
        !same_theme_part(&saved, &source),
        "a semantic save regenerates the theme"
    );
    for feature in ["theme/@name", "fontScheme/@name", "objectDefaults"] {
        assert!(
            names_as_not_retained(&report, feature),
            "{feature} is named by the semantic save: {:#?}",
            report.entries
        );
    }
}

fn import() -> ImportArtifact {
    builtin_registry()
        .import(
            DetectionRequest {
                bytes: SAMPLE,
                selection: FormatSelection::Auto,
                file_name_hint: Some("sample.docx"),
                mime_hint: None,
            },
            // The webapp's open: `retain_source`, so `ImportMode::Retention`.
            true,
        )
        .expect("sample.docx opens")
}

/// A real edit: one character typed into the first run of the body.
fn edit(imported: &mut ImportArtifact) {
    for block in imported.document.body_mut().iter_mut() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &mut paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    run.text.push('!');
                    return;
                }
            }
        }
    }
    panic!("sample.docx has a body run to edit");
}

fn save(
    imported: &ImportArtifact,
    mode: ExportMode,
    source_unchanged: bool,
) -> (Vec<u8>, CompatibilityReport) {
    let artifact = builtin_registry()
        .export(
            &FormatId::new(formats::DOCX).expect("the DOCX id"),
            ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: Some(&imported.source),
                source_unchanged,
                mode,
            },
        )
        .expect("the document saves");
    (artifact.bytes, artifact.report)
}

/// Whether the save's report names `feature` as not kept — under the import's
/// own identifier, or, for a derived part, under the export's stale-part one.
fn names_as_not_retained(report: &CompatibilityReport, feature: &str) -> bool {
    let stale = STALE_PART_FEATURES
        .iter()
        .find(|(part, _)| *part == feature)
        .map(|(_, stale)| *stale);
    report.entries.iter().any(|entry| {
        (entry.feature == feature || Some(entry.feature.as_str()) == stale)
            && entry.retention_outcome == RetentionOutcome::NotRetained
    })
}

fn unzip(bytes: &[u8]) -> Package {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a ZIP package");
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).expect("an entry");
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).expect("a readable entry");
            (file.name().to_owned(), contents)
        })
        .collect()
}

fn part_text<'a>(package: &'a Package, name: &str) -> &'a str {
    package
        .get(name)
        .map_or("", |bytes| std::str::from_utf8(bytes).unwrap_or(""))
}

fn part_contains(package: &Package, name: &str, needle: &str) -> bool {
    part_text(package, name).contains(needle)
}

fn count(package: &Package, name: &str, needle: &str) -> usize {
    part_text(package, name).matches(needle).count()
}

fn same_part(edited: &Package, source: &Package, name: &str) -> bool {
    edited.contains_key(name) && edited.get(name) == source.get(name)
}

fn same_theme_part(edited: &Package, source: &Package) -> bool {
    same_part(edited, source, "word/theme/theme1.xml")
}
