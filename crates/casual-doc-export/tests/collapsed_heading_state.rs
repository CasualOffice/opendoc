// SPDX-License-Identifier: Apache-2.0

//! `w15:collapsed` — Word's saved folded-heading state — is read, modelled as a
//! tri-state, written back, and a fixed point across a save.
//!
//! # Why this file exists
//!
//! `docs/154` §3.4 measured that **no `.docx` in `fixtures/` carried a `collapsed`
//! element at all** — 38 packages, every XML part, zero hits — and drew the right
//! conclusion: the loss-coverage gate (`source_element_coverage.rs`) refuses an
//! element name that vanishes from a written package without a report entry naming
//! it, but it had **never had the opportunity to fire on this one**. ADR-049 named
//! adding such a fixture as the cheapest item in the folding lane, because it turns
//! an unknown into either a report or a red gate.
//!
//! `fixtures/generated/collapsed-headings.docx` is that fixture. What it found, in
//! order, is worth recording because the first answer was not the final one:
//!
//! 1. **The drop was already reported, not silent.** `body.rs`'s generic `w:pPr`
//!    long-tail arm (`apply_paragraph_property` returns `false`, caller calls
//!    `reporter.report_element`) produced one `collapsed` finding with disposition
//!    `OmittedNotRetained` and `occurrences = 3`. So the coverage gate stayed green,
//!    and the honest finding was narrower than ADR-049 assumed.
//! 2. **A reported loss is still a loss.** ADR-049 requires the state to be
//!    *modelled, round-tripped and exported*, because it is the document default a
//!    viewer's fold state layers over. So `ParagraphProperties::collapsed` was added,
//!    `apply_paragraph_property` grew a `b"collapsed"` arm, and
//!    `write_paragraph_properties` emits `w15:collapsed` beside `w:outlineLvl`.
//!
//! This file is the guard for step 2, and it is written so that step 1's weaker
//! outcome would also pass [`collapsed_survives_the_save_or_is_reported`] — because
//! the invariant that matters is "never silently", and a guard pinned to today's
//! mechanism reddens on a change that removes nothing.
//!
//! # What this file does NOT claim
//!
//! It does not claim folding is **reachable**. Nothing hides a folded subtree yet:
//! `casual-doc-layout` owes the block-visibility filter and `webapp/` owes the
//! disclosure chrome, and both are other lanes. `docs/157` §6 states precisely what
//! each owes. Reading and writing the document default is the half that had to come
//! first, because it is the half that was losing data.
//!
//! # Complexity
//!
//! `O(package bytes)` per test: one import and one export of one small fixture.
//! Test-time only; nothing here is on an edit path.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use casual_doc_export::export_document;
use casual_doc_import::{CompatibilityEntry, ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const COLLAPSED_HEADINGS_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/collapsed-headings.docx");

/// The OOXML local name Word saves a folded heading under. The `w15` prefix is not
/// part of it: every reader here matches local names, which is why the project's
/// earlier guess of a bare `w:collapsed` would have matched anyway, and why the
/// namespace has to be pinned by the fixture rather than by a parser arm.
const COLLAPSED: &str = "collapsed";

/// How many `w15:collapsed` elements the fixture carries.
///
/// Pinned so that a fixture edited down to nothing cannot satisfy this file by having
/// nothing to find — the vacuous pass that makes a guard worse than no guard, and
/// precisely the state `154` §3.4 found the whole corpus in.
const EXPECTED_OCCURRENCES: usize = 3;

/// The fold state of the fixture's four headings, in document order. All four
/// `CT_OnOff` states, because the tri-state is the whole point: `Some(true)` folded,
/// `Some(false)` an explicit unfold that CANCELS an inherited fold, `None` absent.
const EXPECTED_FOLD_STATE: &[Option<bool>] = &[Some(true), Some(true), Some(false), None];

/// The outline levels those headings must import with. Folding keys on the heading
/// tree; a fixture whose headings did not import as headings would not be about
/// folding.
const EXPECTED_OUTLINE_LEVELS: &[Option<u8>] = &[Some(0), Some(1), Some(0), Some(0)];

/// One import of a package: the model, and the compatibility findings.
fn import(bytes: &[u8]) -> (Document, Vec<CompatibilityEntry>) {
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
    (import.document, import.report.entries)
}

/// Every paragraph's `(outline_level, collapsed)` pair, in body order, for the
/// paragraphs that are headings. Derived rather than indexed, so inserting a
/// paragraph into the fixture cannot silently shift an assertion onto the wrong one.
fn heading_states(document: &Document) -> (Vec<Option<u8>>, Vec<Option<bool>>) {
    let mut levels = Vec::new();
    let mut folds = Vec::new();
    for block in document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
            if paragraph.properties.outline_level.is_some() {
                levels.push(paragraph.properties.outline_level);
                folds.push(paragraph.properties.collapsed);
            }
        }
    }
    (levels, folds)
}

/// The `.xml` and `.rels` parts of a package, keyed by name.
fn xml_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut parts = BTreeMap::new();
    for name in names {
        if !(name.ends_with(".xml") || name.ends_with(".rels")) {
            continue;
        }
        let mut buffer = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut buffer)
            .expect("a readable entry");
        parts.insert(name, buffer);
    }
    parts
}

/// Every XML part concatenated as raw text, for questions about a package's literal
/// shape — which spelling of `CT_OnOff` it wrote, whether `mc:Ignorable` is declared.
///
/// **Never use this to ask whether a construct survived.** See [`element_local_names`].
fn xml_text(bytes: &[u8]) -> String {
    let mut text = String::new();
    for part in xml_parts(bytes).values() {
        text.push_str(&String::from_utf8_lossy(part));
    }
    text
}

/// The set of XML **element local names** in a package.
///
/// This is the function the survival question must be asked through, and the reason is
/// a trap this file fell into on the way to being written. The first draft asked
/// `xml_text(written).contains("collapsed")` and it **failed**, because the fixture's
/// fourth heading reads *"Heading with no collapsed state"*: the written package held
/// the substring in a `w:t` and the guard claimed a round-trip that had not happened.
/// That is `SKILL` §4's "a guard measuring the fixture rather than the guarantee",
/// caught only by driving it.
///
/// **The fixture still carries that wording, deliberately.** It is now an adversarial
/// property: any future guard that reaches for a substring instead of an element name
/// passes for the wrong reason and is caught here rather than in a published claim.
fn element_local_names(bytes: &[u8]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for part in xml_parts(bytes).values() {
        let mut reader = quick_xml::Reader::from_reader(part.as_slice());
        let mut buffer = Vec::new();
        loop {
            match reader.read_event_into(&mut buffer) {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(quick_xml::events::Event::Start(element))
                | Ok(quick_xml::events::Event::Empty(element)) => {
                    let local = element.local_name();
                    names.insert(String::from_utf8_lossy(local.as_ref()).into_owned());
                }
                Ok(_) => {}
                Err(error) => panic!("a package part must be well-formed XML: {error}"),
            }
            buffer.clear();
        }
    }
    names
}

/// The fixture really carries the element, in the three on/off/implied shapes the
/// design cares about, on paragraphs that really are headings.
///
/// Without this, every other assertion here could be satisfied by an empty document:
/// `154` §3.4's whole point is that a gate with no fixture to fire on says nothing,
/// and a fixture that stopped carrying the element would reproduce that silence
/// exactly.
#[test]
fn the_fixture_carries_collapsed_on_real_headings() {
    let source = xml_text(COLLAPSED_HEADINGS_DOCX);
    let found = source.matches("w15:collapsed").count();
    assert_eq!(
        found, EXPECTED_OCCURRENCES,
        "the fixture must carry exactly {EXPECTED_OCCURRENCES} `w15:collapsed` \
         elements (explicit on, implied on, explicit off); it carries {found}"
    );
    assert!(
        source.contains(r#"<w15:collapsed w:val="1"/>"#),
        "the explicit-on shape must be present"
    );
    assert!(
        source.contains("<w15:collapsed/>"),
        "the implied-on shape (no `w:val`) must be present — `CT_OnOff` is ON when \
         the attribute is absent (ECMA-376 §17.17.4), so a reader that demands \
         `w:val=\"1\"` has to be able to fail here"
    );
    assert!(
        source.contains(r#"<w15:collapsed w:val="0"/>"#),
        "the explicit-off shape must be present — the tri-state case a plain `bool` \
         cannot represent, which CANCELS an inherited fold rather than saying nothing"
    );
    assert!(
        source.contains(r#"mc:Ignorable="w15""#),
        "the fixture must carry `mc:Ignorable`, as Word does: it is a licence to \
         ignore the element, and the reader must be shown not to take it"
    );

    let (document, _) = import(COLLAPSED_HEADINGS_DOCX);
    let (levels, _) = heading_states(&document);
    assert_eq!(
        levels, EXPECTED_OUTLINE_LEVELS,
        "the fixture's four headings must import with their outline levels, so the \
         heading tree a fold operates on exists"
    );
}

/// Import reads all four `CT_OnOff` states onto the right headings.
///
/// The explicit-off entry is the load-bearing one. An importer that treated presence
/// as truth would read `Some(true)` there, and a document that had deliberately
/// unfolded a section would silently re-fold on reopen — the same shape of bug as the
/// `w:numId="0"` and `w:contextualSpacing` cancellations the model documents at
/// length.
#[test]
fn import_reads_the_tri_state() {
    let (document, _) = import(COLLAPSED_HEADINGS_DOCX);
    let (_, folds) = heading_states(&document);
    assert_eq!(
        folds, EXPECTED_FOLD_STATE,
        "the four headings' fold states, in document order, must be folded / folded \
         (implied) / explicitly unfolded / absent"
    );
}

/// The load-bearing invariant: `w15:collapsed` is either written back out or **named**
/// by a compatibility finding. Silence fails.
///
/// A disjunction on purpose, and it is not a weaker test — it is the invariant stated
/// at the altitude it actually holds at. Before the model field existed the report
/// branch was taken; now the round-trip branch is. Both are acceptable outcomes of the
/// no-silent-loss rule, and a guard pinned to whichever one is current would redden on
/// a change that removed nothing (`SKILL`: a guard asserts the guarantee, not the
/// circumstance). What it cannot be satisfied by is losing the element quietly.
#[test]
fn collapsed_survives_the_save_or_is_reported() {
    let (document, entries) = import(COLLAPSED_HEADINGS_DOCX);
    let reported = entries
        .iter()
        .any(|entry| entry.location.element.as_deref() == Some(COLLAPSED));

    let written = export_document(&document, &BTreeMap::new()).expect("the model is writable");
    let round_tripped = element_local_names(&written.bytes).contains(COLLAPSED);

    assert!(
        round_tripped || reported,
        "`w15:collapsed` is in the source, is absent from the written package, and no \
         compatibility finding names it — a silent drop of saved document state, \
         which the engineering priority order forbids outright. Either model it and \
         write it, or report it.\nfindings: {:?}",
        entries
            .iter()
            .map(|entry| entry.feature.as_str())
            .collect::<Vec<_>>()
    );
}

/// The save is a **fixed point**: the three states come back identical, by value, in
/// the right places.
///
/// This is the assertion `collapsed_survives_the_save_or_is_reported` cannot make.
/// That one is about an element *name* surviving somewhere in the package, which is
/// all the coverage gate can express; this one is about the explicit-off heading still
/// being explicitly off. A writer that emitted a bare `<w15:collapsed/>` for every
/// `Some(_)` would satisfy the name test and silently fold a section the document had
/// unfolded.
#[test]
fn the_fold_state_is_a_fixed_point_across_a_save() {
    let (document, _) = import(COLLAPSED_HEADINGS_DOCX);
    let written = export_document(&document, &BTreeMap::new()).expect("the model is writable");

    let text = xml_text(&written.bytes);
    assert_eq!(
        text.matches("w15:collapsed").count(),
        EXPECTED_OCCURRENCES,
        "the writer must emit one element per `Some(_)` and none for `None`"
    );
    assert!(
        text.contains(r#"<w15:collapsed w:val="0"/>"#),
        "the explicit off must be written as an explicit off; a bare element there \
         would re-fold a section the document unfolded"
    );

    let (reopened, _) = import(&written.bytes);
    let (levels, folds) = heading_states(&reopened);
    assert_eq!(
        folds, EXPECTED_FOLD_STATE,
        "reopening the written package must give back the same four fold states"
    );
    assert_eq!(
        levels, EXPECTED_OUTLINE_LEVELS,
        "and the same four outline levels, since the fold is only meaningful against \
         them"
    );
}

/// No written part may use an undeclared XML prefix.
///
/// The general form of a specific risk. `w15:collapsed` can be emitted from any part
/// that carries a `w:pPr` — the main document, a header or footer, a footnote or
/// endnote body, a comment body, a style, a numbering level — and a prefix declared on
/// five of those six roots produces a part Word refuses, from a writer that looks
/// right. `declare_fold_namespaces` exists so the declaration is made in one place;
/// this test is what makes a seventh pPr-bearing part added later fail loudly instead
/// of shipping malformed XML.
///
/// Stated over prefixes in general rather than over `w15` in particular, because the
/// next instance of this bug will be some other prefix.
#[test]
fn no_written_part_uses_an_undeclared_prefix() {
    let (document, _) = import(COLLAPSED_HEADINGS_DOCX);
    let written = export_document(&document, &BTreeMap::new()).expect("the model is writable");

    let mut checked = 0usize;
    for (part, bytes) in xml_parts(&written.bytes) {
        let mut reader = quick_xml::NsReader::from_reader(bytes.as_slice());
        let mut buffer = Vec::new();
        loop {
            match reader.read_resolved_event_into(&mut buffer) {
                Ok((_, quick_xml::events::Event::Eof)) => break,
                Ok((resolved, event)) => {
                    if matches!(
                        event,
                        quick_xml::events::Event::Start(_) | quick_xml::events::Event::Empty(_)
                    ) {
                        assert!(
                            !matches!(resolved, quick_xml::name::ResolveResult::Unknown(_)),
                            "part `{part}` uses an XML prefix its root does not \
                             declare — Word refuses such a part, and nothing else in \
                             this suite would notice"
                        );
                        checked += 1;
                    }
                }
                Err(error) => panic!("part `{part}` must be well-formed XML: {error}"),
            }
            buffer.clear();
        }
    }
    assert!(
        checked > 0,
        "the walk found no elements at all, so it proved nothing — the written \
         package or the part filter is wrong"
    );
}

/// `mc:Ignorable="w15"` must not become a silent-drop licence.
///
/// The trap the fixture was shaped to catch. Word writes `mc:Ignorable="w14 w15 …"`
/// on `w:document`, and ECMA-376 Part 3 lets a consumer that does not understand a
/// listed prefix ignore its elements — so an input carrying the directive is the input
/// most likely to lose the state. `body.rs` acts only on `mc:AlternateContent`
/// branches and records in a doc comment that `mc:Ignorable` is plumbing; a doc
/// comment is prose. If a future change honoured the directive by skipping
/// ignorable-namespace elements, every `w15` construct would start vanishing with no
/// finding, and nothing else would notice: the coverage gate's exception table is
/// about names, and this would be about a namespace.
#[test]
fn mc_ignorable_does_not_suppress_the_fold() {
    assert!(
        xml_text(COLLAPSED_HEADINGS_DOCX).contains(r#"mc:Ignorable="w15""#),
        "this test is only meaningful against an input that carries the directive"
    );
    let (document, _) = import(COLLAPSED_HEADINGS_DOCX);
    let (_, folds) = heading_states(&document);
    assert!(
        folds.iter().any(Option::is_some),
        "a `w15` element under `mc:Ignorable=\"w15\"` was dropped: `mc:Ignorable` \
         names prefixes a consumer MAY ignore, not state it may lose in silence"
    );
}
