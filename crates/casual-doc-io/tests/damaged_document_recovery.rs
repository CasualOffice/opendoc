// SPDX-License-Identifier: Apache-2.0

//! A damaged `.docx` must **open**, and must **say what was repaired**.
//!
//! # The rule
//!
//! Opening is best-effort recovery with a report, not a validation gate. Word
//! often simply refuses a damaged file; "we opened it and told you what we
//! repaired" is a better answer than any error dialog, and it is a competitive
//! position rather than hygiene. The two halves are inseparable:
//!
//! - **It opens.** Every recoverable defect produces a document.
//! - **It says so.** A document that opens with half its tables missing and says
//!   nothing is *worse* than a refusal, because the reader saves over the
//!   original. `35-DISPOSITION-TAXONOMY.md`'s prohibited-silent-loss rule is the
//!   same rule one level up.
//!
//! The second half is why every assertion below comes in pairs, and why the
//! healthy fixture is in the same table as the damaged ones: a gate that only
//! asserted "opening succeeds" would pass if the importer were made to swallow
//! everything in silence, which is the single most likely way to break this.
//!
//! # Why the table is derived
//!
//! [`DAMAGE`] is a list of **mutators**, each a named function from a well-formed
//! synthetic package to a damaged one. Every property is asserted by iterating
//! that list, so a damage shape joins the gate by being added to it — there is no
//! per-fixture assertion to forget to write, and no second list to keep in step.
//! Each row declares only whether the shape is expected to open, so the two
//! shapes that genuinely cannot are explicit rather than silently absent, and a
//! regression that turns an opening shape into a refusal fails here.
//!
//! # No real documents
//!
//! Every fixture is built in code from the constants below. The owner's own
//! `.docx` files are private: none is read, copied, quoted, or committed.
//!
//! # Scope this gate does not claim
//!
//! It is a floor on **damage shapes**, not a fidelity measure. "The table
//! survived the truncation before it" is not asserted, because the point of the
//! report is that the engine does not have to pretend: whatever did not survive
//! is named, and this gate checks that it is named.

use std::io::{Cursor, Write};

use casual_doc_io::{
    DetectionRequest, FormatId, FormatSelection, RepairSeverity, builtin_registry, formats,
};

// ---------------------------------------------------------------------------
// A well-formed synthetic package, and the pieces each mutator damages.
// ---------------------------------------------------------------------------

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
</Relationships>"#;

const DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Alpha</w:t></w:r></w:p>
    <w:tbl>
      <w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>
      <w:tr><w:tc><w:p><w:r><w:t>one</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>two</w:t></w:r></w:p></w:tc></w:tr>
    </w:tbl>
    <w:p><w:r><w:t>Omega</w:t></w:r></w:p>
  </w:body>
</w:document>"#;

const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Body"><w:name w:val="Body"/></w:style>
</w:styles>"#;

const NUMBERING: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
</w:numbering>"#;

/// One package part, as a mutator sees it.
type Part = (String, Vec<u8>);

fn healthy_parts() -> Vec<Part> {
    vec![
        ("[Content_Types].xml".to_owned(), CONTENT_TYPES.into()),
        ("_rels/.rels".to_owned(), ROOT_RELS.into()),
        ("word/document.xml".to_owned(), DOCUMENT.into()),
        (
            "word/_rels/document.xml.rels".to_owned(),
            DOCUMENT_RELS.into(),
        ),
        ("word/styles.xml".to_owned(), STYLES.into()),
        ("word/numbering.xml".to_owned(), NUMBERING.into()),
    ]
}

fn zip(parts: &[Part]) -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(
                name.as_str(),
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .expect("synthetic fixture entry");
        writer.write_all(bytes).expect("synthetic fixture bytes");
    }
    writer
        .finish()
        .expect("synthetic fixture archive")
        .into_inner()
}

fn without(name: &str) -> Vec<u8> {
    zip(&healthy_parts()
        .into_iter()
        .filter(|(part, _)| part != name)
        .collect::<Vec<_>>())
}

fn replacing(name: &str, bytes: &[u8]) -> Vec<u8> {
    zip(&healthy_parts()
        .into_iter()
        .map(|(part, original)| {
            if part == name {
                (part, bytes.to_vec())
            } else {
                (part, original)
            }
        })
        .collect::<Vec<_>>())
}

/// `DOCUMENT` with one substring swapped, so each mutator states exactly the
/// damage it introduces.
fn document_with(find: &str, replace: &str) -> Vec<u8> {
    assert!(
        DOCUMENT.contains(find),
        "mutator anchor {find:?} is not in the fixture document"
    );
    DOCUMENT.replace(find, replace).into_bytes()
}

fn rels_with(find: &str, replace: &str) -> Vec<u8> {
    assert!(DOCUMENT_RELS.contains(find));
    DOCUMENT_RELS.replace(find, replace).into_bytes()
}

// ---------------------------------------------------------------------------
// The damage table.
// ---------------------------------------------------------------------------

/// What is expected of opening one damage shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Expectation {
    /// A document comes back, **and** the report names the damage.
    ///
    /// Both halves, always. This is the ordinary case.
    Opens,
    /// A document comes back and the report is **empty**, because nothing was
    /// lost: the markup is read exactly as written, and there is no repair to
    /// report.
    ///
    /// This category is the one that could be abused — "nothing was lost" is
    /// what a silent drop also looks like — so it is guarded in both
    /// directions: the text that comes back must be non-empty and equal to the
    /// well-formed control's, and reporting a repair here is a failure too.
    OpensWithNothingLost,
    /// The bytes are not a document package at all. The refusal is still a
    /// sentence a host can show, never a bare internal error.
    ///
    /// This is the only legal refusal, and it exists because there is a floor:
    /// bytes that hold no package hold nothing to open.
    NotAPackage,
}

/// One damage shape: a name, a mutator, and what is expected of the open.
struct Damage {
    /// What is wrong with the file, as the row is reported.
    name: &'static str,
    /// Builds the damaged bytes.
    build: fn() -> Vec<u8>,
    /// Whether a document is expected.
    expectation: Expectation,
}

/// Every damage shape the gate covers. **A shape joins the gate by being added
/// here**; nothing else needs editing.
///
/// The list is the inventory of what `casual-doc-import`, `casual-doc-io` and
/// `casual-doc-ooxml` can refuse while opening a document, measured by walking
/// their `Err` sites rather than guessed: a truncated ZIP, trailing bytes after
/// the archive directory, a missing or unreadable content-type manifest, a
/// content type that contradicts its part, a missing or misdirected relationship
/// index, a missing main document, mismatched tags, a truncated part, an
/// undeclared namespace prefix, an encoding declaration this engine does not
/// decode, invalid UTF-8 in a run, a document-type declaration, an undeclared
/// entity, a numeric attribute out of range, a `w:tblGrid` that disagrees with
/// its rows, a missing `w:body`, a root element that is not `w:document`, a
/// damaged or absent styles part, a damaged numbering part, and relationships
/// pointing at absent image and header parts.
static DAMAGE: &[Damage] = &[
    Damage {
        name: "a well-formed package: the control, which must repair nothing",
        build: || zip(&healthy_parts()),
        expectation: Expectation::OpensWithNothingLost,
    },
    Damage {
        name: "the file is truncated, so the archive directory is gone",
        build: || {
            let whole = zip(&healthy_parts());
            whole[..whole.len() * 2 / 3].to_vec()
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "bytes are appended after the archive directory",
        build: || {
            let mut whole = zip(&healthy_parts());
            whole.extend_from_slice(b"GARBAGE");
            whole
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "the bytes are not an archive at all",
        build: || b"this is a plain text file, not a document".to_vec(),
        expectation: Expectation::NotAPackage,
    },
    Damage {
        name: "the archive holds no entries",
        build: || zip(&[]),
        expectation: Expectation::NotAPackage,
    },
    Damage {
        name: "[Content_Types].xml is missing",
        build: || without("[Content_Types].xml"),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "[Content_Types].xml is unparsable",
        build: || replacing("[Content_Types].xml", b"<Types><Default "),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "the main document's content type contradicts its extension",
        build: || {
            replacing(
                "[Content_Types].xml",
                CONTENT_TYPES
                    .replace("wordprocessingml.document.main+xml", "image/png")
                    .as_bytes(),
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "_rels/.rels is missing",
        build: || without("_rels/.rels"),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "_rels/.rels is unparsable",
        build: || replacing("_rels/.rels", b"<Relationships><Relationship Id="),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "the main-document relationship points at a part that is not there",
        build: || {
            replacing(
                "_rels/.rels",
                ROOT_RELS
                    .replace("word/document.xml", "word/absent.xml")
                    .as_bytes(),
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "word/document.xml is missing from the package",
        build: || without("word/document.xml"),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "the document closes a tag it never opened",
        build: || replacing("word/document.xml", &document_with("</w:body>", "</w:nope>")),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "word/document.xml is cut off mid-element",
        build: || {
            replacing(
                "word/document.xml",
                &DOCUMENT.as_bytes()[..DOCUMENT.len() - 60],
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        // Two documents concatenated, or a producer that wrote a declaration
        // into a subtree. Ill-formed per XML 1.0, where a declaration may only
        // be the first thing in an entity. Measured: this engine's reader treats
        // it as an ordinary processing instruction, and processing instructions
        // carry no document content, so the document that comes back is the
        // control's. Recorded here rather than left out, because "the reader
        // happens to tolerate it" is a fact worth a guard: if that ever changes,
        // this row turns into a refusal and the table says so.
        name: "an XML declaration appears in the middle of the document",
        build: || {
            replacing(
                "word/document.xml",
                &document_with(
                    "<w:p><w:r><w:t>Omega</w:t></w:r></w:p>",
                    r#"<?xml version="1.0"?><w:p><w:r><w:t>Omega</w:t></w:r></w:p>"#,
                ),
            )
        },
        expectation: Expectation::OpensWithNothingLost,
    },
    Damage {
        // Ill-formed per XML Namespaces, and a conformant reader refuses it.
        // This engine's readers match on LOCAL names, so the element is read as
        // the `w:t` it was meant to be and the document that comes back is the
        // same one. Nothing was repaired because nothing was lost, and the
        // equality assertion on this category is what establishes that rather
        // than assuming it.
        name: "the document uses a namespace prefix it never declares",
        build: || {
            replacing(
                "word/document.xml",
                &document_with("<w:t>Alpha</w:t>", "<zz:t>Alpha</zz:t>"),
            )
        },
        expectation: Expectation::OpensWithNothingLost,
    },
    Damage {
        // The declaration is a label and the bytes are what they are. These
        // bytes are UTF-8, so reading them as UTF-8 reproduces the document
        // exactly and the mislabelling costs nothing. A part whose bytes really
        // were in another encoding loses characters, and the repair pass reports
        // that when the reader trips over them.
        name: "the XML declaration names an encoding this engine does not decode",
        build: || {
            replacing(
                "word/document.xml",
                &document_with(r#"encoding="UTF-8""#, r#"encoding="Shift_JIS""#),
            )
        },
        expectation: Expectation::OpensWithNothingLost,
    },
    Damage {
        name: "a run's text holds bytes that are not valid UTF-8",
        build: || {
            let mut bytes = DOCUMENT.as_bytes().to_vec();
            let at = DOCUMENT.find("Alpha").expect("fixture anchor");
            bytes[at] = 0xFF;
            replacing("word/document.xml", &bytes)
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "the document carries a document-type declaration",
        build: || {
            replacing(
                "word/document.xml",
                &document_with("<w:document", "<!DOCTYPE w:document []>\n<w:document"),
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "a run refers to an entity the document never declares",
        build: || replacing("word/document.xml", &document_with("Alpha", "&missing;")),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "a numeric attribute is far outside its range",
        build: || {
            replacing(
                "word/document.xml",
                &document_with(
                    "<w:r><w:t>Alpha",
                    r#"<w:r><w:rPr><w:sz w:val="99999999999999999999"/></w:rPr><w:t>Alpha"#,
                ),
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        // An inconsistent file, not a lossy one: the grid is imported with the
        // one column it declares and the row with the two cells it holds, which
        // is exactly what the markup says. Word tolerates the same disagreement.
        // Nothing is repaired because nothing is dropped, and inventing a column
        // here would be the silent repair this gate exists to prevent.
        name: "the table's w:tblGrid declares fewer columns than its rows have cells",
        build: || {
            replacing(
                "word/document.xml",
                &document_with(
                    r#"<w:gridCol w:w="2000"/><w:gridCol w:w="2000"/>"#,
                    r#"<w:gridCol w:w="2000"/>"#,
                ),
            )
        },
        expectation: Expectation::OpensWithNothingLost,
    },
    Damage {
        name: "the main document has no w:body",
        build: || {
            replacing(
                "word/document.xml",
                br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#,
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "the main document's root element is not w:document",
        build: || {
            replacing(
                "word/document.xml",
                br#"<?xml version="1.0"?><html><body>not a word document</body></html>"#,
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "word/styles.xml is unparsable",
        build: || replacing("word/styles.xml", b"<w:styles><w:style"),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "the styles relationship points at a part that is not there",
        build: || without("word/styles.xml"),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "word/numbering.xml is unparsable",
        build: || replacing("word/numbering.xml", b"<w:numbering><<<"),
        expectation: Expectation::Opens,
    },
    Damage {
        name: "an image relationship points at a part that is not there",
        build: || {
            replacing(
                "word/_rels/document.xml.rels",
                &rels_with(
                    "</Relationships>",
                    r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/absent.png"/></Relationships>"#,
                ),
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "a header relationship points at a part that is not there",
        build: || {
            replacing(
                "word/_rels/document.xml.rels",
                &rels_with(
                    "</Relationships>",
                    r#"<Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#,
                ),
            )
        },
        expectation: Expectation::Opens,
    },
    Damage {
        name: "only word/document.xml is left: no manifest, no relationships",
        build: || zip(&[("word/document.xml".to_owned(), DOCUMENT.into())]),
        expectation: Expectation::Opens,
    },
];

// ---------------------------------------------------------------------------
// The open under test: the product path, end to end.
// ---------------------------------------------------------------------------

/// What one open produced.
struct Opened {
    blocks: usize,
    text: String,
    repairs: Vec<(String, String, RepairSeverity)>,
    compatibility_findings: usize,
}

/// Opens `bytes` through the whole product path, as a `.docx`.
///
/// The format is **explicit** rather than detected, and that is the question this
/// gate asks: the reader handed the application a file called `.docx`, so the
/// DOCX open path is what must not refuse. Detection is a separate property with
/// its own test — without it the recovery would be unreachable, but with
/// detection left on `Auto` a row like "these bytes are plain text" would be
/// answered by the plain-text adapter and the DOCX path would never be measured.
fn open(bytes: &[u8]) -> Result<Opened, String> {
    let registry = builtin_registry();
    let docx = FormatId::new(formats::DOCX).expect("the built-in DOCX id is valid");
    let artifact = registry
        .import(
            DetectionRequest {
                bytes,
                selection: FormatSelection::Explicit(docx),
                file_name_hint: None,
                mime_hint: None,
            },
            false,
        )
        .map_err(|error| match error {
            casual_doc_io::IoError::ImportFailed { source, .. } => source.message().to_owned(),
            other => other.to_string(),
        })?;
    let text = document_text(&artifact.document);
    Ok(Opened {
        blocks: artifact.document.body().len(),
        text,
        repairs: artifact
            .recovery
            .repairs
            .iter()
            .map(|repair| (repair.token.clone(), repair.summary.clone(), repair.severity))
            .collect(),
        compatibility_findings: artifact.report.entries.len(),
    })
}

/// Every character the document's paragraphs hold, so a recovery that produced a
/// document with nothing in it cannot pass as one that recovered content.
fn document_text(document: &casual_doc_model::v1::Document) -> String {
    use casual_doc_model::v1::{BlockNode, InlineNode};
    fn blocks(nodes: &[BlockNode], out: &mut String) {
        for node in nodes {
            match node {
                BlockNode::Paragraph(paragraph) => {
                    for inline in &paragraph.inlines {
                        if let InlineNode::Run(run) = inline {
                            out.push_str(&run.text);
                        }
                    }
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            blocks(&cell.blocks, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = String::new();
    blocks(document.body(), &mut out);
    out
}

// ---------------------------------------------------------------------------
// The gate.
// ---------------------------------------------------------------------------

/// Every damage shape in [`DAMAGE`] opens, except the two that hold no package —
/// and those refuse with a sentence rather than an internal error name.
#[test]
fn every_damaged_document_opens() {
    let mut refused: Vec<&str> = Vec::new();
    for damage in DAMAGE {
        let outcome = open(&(damage.build)());
        match (damage.expectation, &outcome) {
            (Expectation::Opens | Expectation::OpensWithNothingLost, Err(error)) => {
                panic!("{}: refused instead of opening: {error}", damage.name)
            }
            (Expectation::NotAPackage, Ok(_)) => {
                panic!("{}: opened, but the table says it cannot", damage.name)
            }
            (Expectation::NotAPackage, Err(error)) => {
                refused.push(damage.name);
                assert_sentence(error, damage.name);
            }
            (Expectation::Opens | Expectation::OpensWithNothingLost, Ok(_)) => {}
        }
    }
    // The floor is a property of the table, not a count to maintain: every shape
    // that is not "these bytes hold no package" must open.
    assert_eq!(
        refused.len(),
        DAMAGE
            .iter()
            .filter(|damage| damage.expectation == Expectation::NotAPackage)
            .count(),
        "refusals: {refused:?}"
    );
}

/// Every damaged shape that opens **reports** the damage.
///
/// This is the half that keeps the other half honest. A gate asserting only
/// "opening succeeds" passes if the importer were made to swallow everything
/// silently, and a recovery with no finding is the defect this lane is most
/// likely to introduce — so the report is asserted for every row, and the
/// well-formed control is asserted to report **nothing**.
#[test]
fn every_damaged_document_reports_its_damage() {
    for damage in DAMAGE {
        if damage.expectation == Expectation::NotAPackage {
            continue;
        }
        let opened = open(&(damage.build)()).expect("a shape the table says opens");
        match damage.expectation {
            Expectation::Opens => assert!(
                !opened.repairs.is_empty() || opened.compatibility_findings > 0,
                "{}: opened in silence — nothing in the recovery report and nothing in \
                 the compatibility report, so a reader would save over the original \
                 believing the file was clean",
                damage.name
            ),
            // The other direction of the same rule: a shape the table says loses
            // nothing must not start reporting repairs either, because a repair
            // is a claim that something was wrong with a document the reader is
            // about to save.
            Expectation::OpensWithNothingLost => assert!(
                opened.repairs.is_empty(),
                "{}: the table says nothing was lost, but the open reported repairs: {:?}",
                damage.name,
                opened.repairs
            ),
            Expectation::NotAPackage => unreachable!("filtered above"),
        }
    }
}

/// Either content came back, or the report says it did not.
///
/// The derived form of "do not silently repair": a recovery is allowed to lose
/// the whole body, and is not allowed to lose it quietly. One of the two has to
/// hold for every row, and nothing has to be hand-listed for it to hold.
#[test]
fn an_empty_recovery_is_always_accounted_for() {
    for damage in DAMAGE {
        if damage.expectation == Expectation::NotAPackage {
            continue;
        }
        let opened = open(&(damage.build)()).expect("a shape the table says opens");
        if !opened.text.is_empty() {
            continue;
        }
        let accounted = opened
            .repairs
            .iter()
            .any(|(_, _, severity)| *severity == RepairSeverity::BodyLost)
            || opened.repairs.iter().any(|(token, _, _)| {
                matches!(token.as_str(), "body-missing" | "body-unreadable")
            });
        assert!(
            accounted,
            "{}: opened with no text at all and nothing in the report says the body was \
             lost. Repairs were {:?}",
            damage.name, opened.repairs
        );
    }
}

/// A repair's text is a sentence for a reader, not an internal name.
///
/// There is an existing defect of exactly this kind elsewhere in this tree — an
/// edit refused with an internal `ValueTooLarge` shown to the reader as one
/// generic sentence — so the rule is asserted mechanically rather than trusted to
/// review: no summary may contain its own machine token, an underscore, a path
/// separator, or a Rust-style identifier, and each must read as a sentence.
#[test]
fn every_repair_speaks_to_a_reader() {
    let mut seen = 0_usize;
    for damage in DAMAGE {
        if damage.expectation == Expectation::NotAPackage {
            continue;
        }
        let opened = open(&(damage.build)()).expect("a shape the table says opens");
        for (token, summary, _) in &opened.repairs {
            seen += 1;
            assert_sentence(summary, damage.name);
            assert!(
                !summary.contains(token.as_str()),
                "{}: the repair summary shows its own machine token {token:?}: {summary:?}",
                damage.name
            );
        }
    }
    // The guard is only worth anything if it saw repairs. A table whose rows all
    // stopped producing them would otherwise pass this test by vacuity.
    assert!(
        seen >= DAMAGE.len(),
        "only {seen} repairs across {} damage shapes: the gate is not reaching the \
         reporting path",
        DAMAGE.len()
    );
}

/// A body the repair pass cannot help recovers the blocks before the damage.
///
/// The table above goes through the full ladder, where the byte-level repair pass
/// runs first and makes almost every damaged main document well-formed — which
/// means the body parser's **own** recovery arm is not reached by any row in it.
/// Measured, not assumed: a mutation that restored that arm's refusal left every
/// table guard green.
///
/// `import_main_document_xml` is the entry point with no ladder: it hands bytes
/// straight to the body parser. That is the arm under test, and it is the one
/// that decides whether a part the repair pass cannot fix costs the reader
/// everything or only what follows the damage.
#[test]
fn a_body_the_repair_pass_cannot_help_keeps_what_came_before_the_damage() {
    use casual_doc_import::{ImportConfig, ImportMode, import_main_document_xml};

    // A stray end tag mid-body: the reader stops there, and the paragraph before
    // it is already built.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>Alpha</w:t></w:r></w:p>
        </w:nope>
        <w:p><w:r><w:t>Omega</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let config = ImportConfig {
        mode: ImportMode::Semantic,
        recover: true,
        ..ImportConfig::default()
    };
    assert_eq!(
        import_main_document_xml(
            xml,
            ImportConfig {
                recover: false,
                ..config
            }
        )
        .err()
        .map(|error| error.to_string()),
        Some("document XML is malformed".to_owned()),
        "the precondition: without recovery these bytes are refused, so the guard is \
         measuring recovery rather than a reader that never minded"
    );
    let import = import_main_document_xml(xml, config).expect("recovering, it opens");
    assert_eq!(
        import.document.body().len(),
        1,
        "the paragraph before the damage was not kept"
    );
    assert!(
        !import.recovery.is_empty(),
        "the body stopped at damage and said nothing"
    );
    let repair = &import.recovery.repairs()[0];
    assert_eq!(repair.kind.token(), "body-stopped-at-damage");
    assert_eq!(
        repair.severity(),
        casual_doc_import::Severity::ContentDropped,
        "content after the damage is gone, and the severity has to say so"
    );
}

/// A shape the table says loses nothing really loses nothing.
///
/// Without this, [`Expectation::OpensWithNothingLost`] would be an escape hatch:
/// a row could be moved into it to silence the reporting assertion, which is the
/// exact failure this whole file is built to catch. The claim is therefore
/// checked against the well-formed control rather than trusted — the text that
/// comes back has to be the control's, character for character.
#[test]
fn a_shape_that_loses_nothing_returns_the_control_document() {
    let control = open(&zip(&healthy_parts())).expect("the control opens");
    assert!(
        !control.text.is_empty(),
        "the control document has no text, so this guard would hold vacuously"
    );
    for damage in DAMAGE {
        if damage.expectation != Expectation::OpensWithNothingLost {
            continue;
        }
        let opened = open(&(damage.build)()).expect("a shape the table says opens");
        assert_eq!(
            opened.text, control.text,
            "{}: the table says nothing was lost, but the text differs from the \
             well-formed control's",
            damage.name
        );
        assert_eq!(
            opened.blocks, control.blocks,
            "{}: the table says nothing was lost, but the block count differs from the \
             well-formed control's",
            damage.name
        );
    }
}

/// Random byte damage never panics, and never opens in silence.
///
/// # Why a sweep as well as a table
///
/// [`DAMAGE`] covers the damage shapes the inventory *found*. A recovery path
/// also has to survive the ones nobody thought of, and it has more exposure to
/// them than a refusing parser does: it deliberately continues past damage, so
/// every arm that used to end in a refusal now runs on bytes no producer wrote.
///
/// `fuzz/fuzz_targets/docx_recovering_import.rs` is the real instrument for that,
/// but a fuzz target only runs in the scheduled job and the `fuzz-build` gate
/// merely compiles it. This is the deterministic version that runs in the
/// ordinary test sweep, so a panic introduced on a branch fails that branch
/// rather than next week's fuzz run. **A panic is never an acceptable outcome of
/// opening a document**, and a panic here fails the test by existing.
///
/// Deterministic by construction: one fixed seed and a linear congruential
/// generator, so a failure is reproducible from the iteration number in the
/// message rather than being a flake.
#[test]
fn random_byte_damage_never_panics_and_never_opens_in_silence() {
    let control_bytes = zip(&healthy_parts());
    let control = open(&control_bytes).expect("the control opens");
    // Numerical Recipes' LCG constants: any full-period generator does, and a
    // named one makes the sequence auditable.
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };
    let mut opened_count = 0_usize;
    for iteration in 0..2_000_usize {
        let mut bytes = control_bytes.clone();
        // Between one and eight byte edits, plus a one-in-four chance of a
        // truncation, which is the most common real damage and the one that
        // reaches the archive-directory rebuild.
        let edits = 1 + next() % 8;
        for _ in 0..edits {
            let at = next() % bytes.len();
            bytes[at] = match next() % 4 {
                0 => 0x00,
                1 => 0xFF,
                2 => b'<',
                _ => (next() % 256) as u8,
            };
        }
        if next() % 4 == 0 {
            let keep = 1 + next() % bytes.len();
            bytes.truncate(keep);
        }
        // The assertion is that this call returns. A panic fails the test.
        if let Ok(opened) = open(&bytes) {
            opened_count += 1;
            let unchanged = opened.text == control.text && opened.blocks == control.blocks;
            assert!(
                unchanged || !opened.repairs.is_empty() || opened.compatibility_findings > 0,
                "iteration {iteration}: a document came back that is not the control's and \
                 nothing in either report says why"
            );
        }
    }
    // The sweep is worthless if the mutations never produced an openable file:
    // it would then be asserting nothing about the recovery path at all.
    assert!(
        opened_count >= 200,
        "only {opened_count} of 2000 mutated packages opened; the sweep is not reaching the \
         recovery path"
    );
}

/// The shared sentence check, used for repair summaries and for the refusal text.
fn assert_sentence(text: &str, context: &str) {
    assert!(
        text.ends_with('.') || text.ends_with(".)"),
        "{context}: not a sentence (no full stop): {text:?}"
    );
    assert!(
        text.split_whitespace().count() >= 5,
        "{context}: too terse to be a sentence: {text:?}"
    );
    assert!(
        text.chars().next().is_some_and(char::is_uppercase),
        "{context}: a sentence starts with a capital: {text:?}"
    );
    // A part name or an element name appears in parentheses, and those are
    // strings from the file itself — `word/_rels/document.xml.rels` is what a
    // reader sees in any file listing. Only the prose around them is checked for
    // internal vocabulary.
    let prose: String = {
        let mut prose = String::new();
        let mut depth = 0_usize;
        for character in text.chars() {
            match character {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if depth == 0 => prose.push(character),
                _ => {}
            }
        }
        prose
    };
    for forbidden in ["::", "_", "Err(", "Some(", "ImportError", "PackageError"] {
        assert!(
            !prose.contains(forbidden),
            "{context}: internal vocabulary {forbidden:?} in reader-facing text: {text:?}"
        );
    }
    // A Rust-style type name — two capitalised humps with no space — is the
    // shape of every internal error name this must not show (`ValueTooLarge`,
    // `MalformedXml`, `PartUnparsable`).
    for word in prose.split_whitespace() {
        let humps = word
            .chars()
            .zip(word.chars().skip(1))
            .filter(|(left, right)| left.is_lowercase() && right.is_uppercase())
            .count();
        assert!(
            humps == 0,
            "{context}: {word:?} reads as an internal type name, not prose: {text:?}"
        );
    }
}

/// A damaged package is still **detected** as a `.docx`.
///
/// Without this the recovery is unreachable: detection runs before import, so a
/// truncated file that no adapter claims is reported as an unrecognised format
/// and never reaches an importer that could recover it. This was the one ordering
/// mistake that would have made every assertion above pass against a product
/// that still refused the file.
#[test]
fn a_damaged_package_is_still_detected_as_a_word_document() {
    let registry = builtin_registry();
    let whole = zip(&healthy_parts());
    let truncated = &whole[..whole.len() * 2 / 3];
    let format = registry
        .detect(DetectionRequest {
            bytes: truncated,
            selection: FormatSelection::Auto,
            file_name_hint: None,
            mime_hint: None,
        })
        .expect("a truncated .docx is still a .docx");
    assert_eq!(format.as_str(), formats::DOCX);
}

/// Recovery does not relax a resource bound.
///
/// A limit is a refusal on purpose, and "nothing should throw" does not reach
/// it: recovering past a bound would turn a defence against an expansion bomb
/// into a suggestion. The engineering priority order puts security and resource
/// bounds above compatibility for this reason.
#[test]
fn a_resource_bound_still_refuses() {
    use casual_doc_import::{ImportConfig, ImportMode, import_package};
    use casual_doc_ooxml::{DocxPackage, PackageLimits};

    let bytes = zip(&healthy_parts());
    let mut package =
        DocxPackage::open(&bytes, PackageLimits::default()).expect("the control package opens");
    // Ten elements: fewer than the fixture's twenty-odd, so the bound is
    // genuinely crossed, and MORE than the two in the empty main document the
    // ladder's last rung reads. The second half matters — with a bound of one,
    // even that last rung fails, so the test would pass whatever the policy on
    // bounds was.
    let refused = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            recover: true,
            max_elements: 10,
            ..ImportConfig::default()
        },
    );
    assert_eq!(
        refused.err().map(|error| error.to_string()),
        Some("import limit xml_elements exceeded".to_owned()),
        "a resource bound was recovered past instead of refusing"
    );
}

/// The well-formed control takes the same path it took before recovery existed.
///
/// The safety property of the whole feature: the strict read runs first, so no
/// amount of leniency can change what a healthy document imports to. Asserted by
/// comparing the recovering open against the strict one, construct for construct.
#[test]
fn recovery_does_not_change_what_a_healthy_document_imports_to() {
    use casual_doc_import::{ImportConfig, ImportMode, import_package};
    use casual_doc_ooxml::{DocxPackage, PackageLimits};

    let bytes = zip(&healthy_parts());
    let import = |recover: bool| {
        let mut package =
            DocxPackage::open(&bytes, PackageLimits::default()).expect("the control opens");
        import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                recover,
                ..ImportConfig::default()
            },
        )
        .expect("the control imports")
    };
    let strict = import(false);
    let recovering = import(true);
    assert_eq!(strict.document, recovering.document);
    assert_eq!(strict.report, recovering.report);
    assert!(recovering.recovery.is_empty());
}
