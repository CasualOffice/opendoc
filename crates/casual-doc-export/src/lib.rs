//! DOCX package writer for the no-edit round-trip case.
//!
//! This is the "exact no-op return" that Retention mode enables: given the
//! source parts retained verbatim at import ([`RetainedSource`]), it
//! reconstructs a valid DOCX package with byte-identical part contents. It does
//! NOT regenerate OOXML from the model — that is the Phase-2 semantic writer.
//! Combined with the importer, it makes round-trip end-to-end verifiable:
//! `import (Retention) -> write_package -> reopen -> identical model`.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::io::{Cursor, Write};

use casual_doc_import::RetainedSource;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

mod semantic;
pub use semantic::{write_document, write_document_with_retained_parts};

/// A package-writing failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportError {
    /// No parts were retained (source was not imported in Retention mode).
    NoRetainedParts,
    /// The ZIP package could not be assembled.
    Package,
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRetainedParts => {
                formatter.write_str("no retained parts; import in Retention mode to reconstruct")
            }
            Self::Package => formatter.write_str("DOCX package could not be assembled"),
        }
    }
}

impl Error for ExportError {}

/// Reconstructs a valid DOCX package from a retained source, byte-identical in
/// part content. Parts are written in deterministic (sorted) order with a fixed
/// timestamp, so the output bytes are reproducible.
pub fn write_package(source: &RetainedSource) -> Result<Vec<u8>, ExportError> {
    if source.parts.is_empty() {
        return Err(ExportError::NoRetainedParts);
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default());
    // `source.parts` is a BTreeMap, so iteration is already sorted by name.
    for (name, bytes) in &source.parts {
        writer
            .start_file(name, options)
            .map_err(|_| ExportError::Package)?;
        writer.write_all(bytes).map_err(|_| ExportError::Package)?;
    }
    Ok(writer
        .finish()
        .map_err(|_| ExportError::Package)?
        .into_inner())
}

#[cfg(test)]
mod semantic_tests {
    use std::collections::BTreeMap;
    use std::io::{Cursor, Write};

    use casual_doc_import::{ImportConfig, ImportMode, import_main_document_xml, import_package};
    use casual_doc_ooxml::{DocxPackage, PackageLimits};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use crate::{write_document, write_document_with_retained_parts};

    /// Zips a minimal DOCX around a `word/document.xml` body and its
    /// `document.xml.rels`, so a source document that references external
    /// relationships (hyperlinks) can be imported into a model.
    fn pack(document_xml: &[u8], document_rels: &[u8]) -> Vec<u8> {
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", root_rels.as_slice()),
            ("word/document.xml", document_xml),
            ("word/_rels/document.xml.rels", document_rels),
        ] {
            zw.start_file(name, opts).unwrap();
            zw.write_all(bytes).unwrap();
        }
        zw.finish().unwrap().into_inner()
    }

    /// Zips the named parts verbatim into a DOCX package (for a source that needs
    /// a custom `[Content_Types].xml` and extra parts, e.g. `fontTable.xml`).
    fn zip_named(parts: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in parts {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(bytes).unwrap();
        }
        zw.finish().unwrap().into_inner()
    }

    /// Opens a DOCX package and imports it in Semantic mode, returning the model.
    fn reopen(bytes: &[u8]) -> casual_doc_model::v1::Document {
        let mut package = DocxPackage::open(bytes, PackageLimits::default()).unwrap();
        import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document
    }

    /// The semantic fixed point over a whole package: `import(Semantic)` ->
    /// `write_document` -> reopen == identical model. Media bytes are not needed
    /// (the model holds only reference metadata).
    fn assert_corpus_round_trip(source: &[u8], name: &str) {
        let m1 = reopen(source);
        let written = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&written);
        assert_eq!(m1, m2, "{name}: real-producer doc survives write -> reopen");
    }

    #[test]
    fn real_producer_corpus_survives_the_semantic_round_trip() {
        // The writer must round-trip real Word/LibreOffice documents, not just
        // hand-crafted fixtures. Each is imported semantically, written, and
        // reopened; the model must be identical.
        for (name, bytes) in [
            (
                "table-merges",
                include_bytes!("../../../fixtures/corpus/real-producer-table-merges.docx")
                    .as_slice(),
            ),
            (
                "table-list",
                include_bytes!("../../../fixtures/corpus/real-producer-table-list.docx").as_slice(),
            ),
            (
                "rich",
                include_bytes!("../../../fixtures/corpus/real-producer-rich.docx").as_slice(),
            ),
            (
                "hyperlinks",
                include_bytes!("../../../fixtures/corpus/real-producer-hyperlinks.docx").as_slice(),
            ),
            (
                "header-footer",
                include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx")
                    .as_slice(),
            ),
            (
                "footnotes",
                include_bytes!("../../../fixtures/corpus/real-producer-footnotes.docx").as_slice(),
            ),
            (
                "libreoffice",
                include_bytes!("../../../fixtures/corpus/real-producer-libreoffice.docx")
                    .as_slice(),
            ),
        ] {
            assert_corpus_round_trip(bytes, name);
        }
    }

    #[test]
    fn document_properties_survive_the_semantic_round_trip() {
        // The key deliverable: a package with rich core/app/custom metadata is
        // imported, written by the semantic writer, and reopened; the modeled
        // `DocumentProperties` must be identical (a semantic fixed point). This
        // proves the import -> model -> write -> reopen path no longer drops
        // title/author/dates/company/counts/custom properties.
        let source =
            include_bytes!("../../../fixtures/corpus/synthetic-rich-metadata.docx").as_slice();
        let m1 = reopen(source);
        let properties = m1.properties().expect("rich metadata imported");
        assert_eq!(
            properties.core.title.as_deref(),
            Some("Annual Metadata Report")
        );
        assert_eq!(properties.core.creator.as_deref(), Some("Ada Lovelace"));
        assert_eq!(
            properties.app.company.as_deref(),
            Some("Analytical Engines Ltd")
        );
        assert_eq!(properties.app.words, Some(3200));
        assert_eq!(properties.app.heading_pairs.len(), 2);
        assert_eq!(properties.custom.len(), 5);

        let written = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&written);
        assert_eq!(
            m1.properties(),
            m2.properties(),
            "document properties survive write -> reopen"
        );
        assert_eq!(m1, m2, "the whole model (incl. metadata) is a fixed point");
    }

    #[test]
    fn corpus_docprops_bytes_survive_the_retention_round_trip() {
        // A byte-oriented check: in Retention mode the raw `docProps/*` bytes of
        // a real producer file are preserved verbatim and reproduced by the
        // retention package writer.
        let source =
            include_bytes!("../../../fixtures/corpus/real-producer-footnotes.docx").as_slice();
        let mut package = DocxPackage::open(source, PackageLimits::default()).unwrap();
        let retained = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Retention,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .retained_source
        .expect("retention mode retains the source");

        let rebuilt = crate::write_package(&retained).unwrap();
        let mut reopened = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
        for part in [
            "docProps/core.xml",
            "docProps/app.xml",
            "docProps/custom.xml",
        ] {
            let original = retained.parts.get(part).expect("docProps part retained");
            assert_eq!(
                &reopened.read_part(part).unwrap(),
                original,
                "{part} survives the retention round trip byte-for-byte"
            );
        }
    }

    /// Locates a LibreOffice `soffice` binary, or returns `None` so the caller
    /// can skip (LibreOffice is not a build/CI dependency).
    fn find_soffice() -> Option<std::path::PathBuf> {
        for candidate in [
            "/opt/homebrew/bin/soffice",
            "/usr/bin/soffice",
            "/usr/local/bin/soffice",
            "/Applications/LibreOffice.app/Contents/MacOS/soffice",
        ] {
            let path = std::path::PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// External validity gate: the writer's output must open in a real word
    /// processor, not merely round-trip through our own importer. Each corpus
    /// document is imported, re-written, and handed to LibreOffice for headless
    /// conversion; a non-zero exit or missing output means we emitted a package
    /// LibreOffice rejects.
    ///
    /// Ignored by default (LibreOffice is slow and not a CI dependency); run with
    /// `cargo test -p casual-doc-export -- --ignored soffice`.
    #[test]
    #[ignore = "requires a local LibreOffice (soffice) install"]
    #[allow(clippy::print_stderr)] // a skip diagnostic in an ignored, on-demand test
    fn writer_output_opens_in_libreoffice() {
        let Some(soffice) = find_soffice() else {
            eprintln!("skipping: no soffice binary found");
            return;
        };
        let tmp = std::env::temp_dir().join("opendoc-soffice-validity");
        std::fs::create_dir_all(&tmp).unwrap();
        let profile = tmp.join("profile");

        for (name, bytes) in [
            (
                "table-merges",
                include_bytes!("../../../fixtures/corpus/real-producer-table-merges.docx")
                    .as_slice(),
            ),
            (
                "rich",
                include_bytes!("../../../fixtures/corpus/real-producer-rich.docx").as_slice(),
            ),
            (
                "hyperlinks",
                include_bytes!("../../../fixtures/corpus/real-producer-hyperlinks.docx").as_slice(),
            ),
            (
                "header-footer",
                include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx")
                    .as_slice(),
            ),
            (
                "footnotes",
                include_bytes!("../../../fixtures/corpus/real-producer-footnotes.docx").as_slice(),
            ),
            (
                "libreoffice",
                include_bytes!("../../../fixtures/corpus/real-producer-libreoffice.docx")
                    .as_slice(),
            ),
        ] {
            let model = reopen(bytes);
            let written = write_document(&model, &BTreeMap::new()).unwrap();
            let docx = tmp.join(format!("{name}.docx"));
            std::fs::write(&docx, &written).unwrap();

            let out = tmp.join(name);
            std::fs::create_dir_all(&out).unwrap();
            let status = std::process::Command::new(&soffice)
                .arg("--headless")
                .arg(format!(
                    "-env:UserInstallation=file://{}",
                    profile.display()
                ))
                .arg("--convert-to")
                .arg("txt:Text")
                .arg("--outdir")
                .arg(&out)
                .arg(&docx)
                .status()
                .unwrap();
            assert!(status.success(), "{name}: soffice rejected the package");
            let txt = out.join(format!("{name}.txt"));
            assert!(
                txt.exists(),
                "{name}: soffice produced no output (invalid package)"
            );
        }
    }

    /// The Phase-1B semantic fixed point: importing a document, writing it, and
    /// reopening yields the identical model. The importer allocates ids in
    /// document order, so a writer that emits the body in that same order
    /// reproduces every id — the strongest correctness gate for the writer.
    #[test]
    fn core_body_survives_the_semantic_round_trip() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:jc w:val="center"/><w:keepNext/><w:outlineLvl w:val="2"/></w:pPr>
                <w:r><w:rPr><w:b/><w:i w:val="0"/><w:u/><w:color w:val="FF0000"/><w:sz w:val="28"/></w:rPr>
                    <w:t xml:space="preserve">Hello </w:t></w:r>
                <w:r><w:t>world</w:t></w:r><w:r><w:tab/></w:r>
                <w:r><w:t>after</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>
            <w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;

        assert_eq!(m1, m2, "the model survives write -> reopen unchanged");
    }

    #[test]
    fn omml_math_survives_the_semantic_round_trip() {
        // A paragraph with a run, an opaque equation (`m:oMathPara`), and a
        // trailing run. The retained OMML must be written back verbatim and the
        // whole model must survive write -> reopen unchanged (ids included).
        let xml = br#"<w:document xmlns:w="urn:w" xmlns:m="urn:m"><w:body>
            <w:p><w:r><w:t>before</w:t></w:r>
                <m:oMathPara><m:oMath><m:r><m:t>a=b</m:t></m:r></m:oMath></m:oMathPara>
                <w:r><w:t>after</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;

        assert_eq!(m1, m2, "the equation survives write -> reopen unchanged");

        // The reopened equation retains the OMML subtree verbatim, and its text
        // never leaked into the surrounding runs.
        use casual_doc_model::v1::{BlockNode, InlineNode};
        let BlockNode::Paragraph(para) = &m2.body()[0] else {
            panic!("expected a paragraph");
        };
        let InlineNode::Math(math) = &para.inlines[1] else {
            panic!("expected the opaque math node between the two runs");
        };
        assert_eq!(
            math.omml,
            "<m:oMathPara><m:oMath><m:r><m:t>a=b</m:t></m:r></m:oMath></m:oMathPara>"
        );
        let run_text: String = para
            .inlines
            .iter()
            .filter_map(|inline| match inline {
                InlineNode::Run(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(run_text, "beforeafter");
    }

    #[test]
    fn tables_survive_the_semantic_round_trip() {
        // A table exercising the grid, table/row/cell properties (borders,
        // shading, margins, merges, vAlign, layout, look), and a nested table
        // inside a cell — all must survive write -> reopen unchanged.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:tbl>
                <w:tblPr>
                    <w:jc w:val="center"/><w:tblW w:type="dxa" w:w="9000"/>
                    <w:tblLayout w:type="fixed"/><w:tblLook w:firstRow="1" w:noVBand="1"/>
                    <w:tblBorders><w:top w:val="single" w:sz="8" w:color="112233" w:space="4"/>
                        <w:insideH w:val="dotted"/></w:tblBorders>
                    <w:shd w:val="clear" w:fill="EEEEEE"/>
                    <w:tblCellMar><w:top w:type="dxa" w:w="120"/><w:start w:type="dxa" w:w="60"/></w:tblCellMar>
                </w:tblPr>
                <w:tblGrid><w:gridCol w:w="4500"/><w:gridCol w:w="4500"/></w:tblGrid>
                <w:tr>
                    <w:trPr><w:trHeight w:val="500" w:hRule="atLeast"/><w:tblHeader/></w:trPr>
                    <w:tc>
                        <w:tcPr><w:gridSpan w:val="2"/><w:tcW w:type="dxa" w:w="9000"/>
                            <w:shd w:val="clear" w:fill="FF0000"/><w:vAlign w:val="center"/><w:noWrap/>
                            <w:tcBorders><w:bottom w:val="double" w:sz="16"/></w:tcBorders>
                            <w:tcMar><w:end w:type="dxa" w:w="90"/></w:tcMar></w:tcPr>
                        <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Header</w:t></w:r></w:p>
                    </w:tc>
                </w:tr>
                <w:tr>
                    <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr>
                        <w:tbl><w:tr><w:tc><w:p><w:r><w:t>nested</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
                        <w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>
                    <w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc>
                </w:tr>
            </w:tbl>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "the table model survives write -> reopen unchanged");
    }

    #[test]
    fn expanded_table_properties_survive_the_semantic_round_trip() {
        // The additive table/row/cell properties: tblOverlap, tblCellSpacing,
        // tblInd, tblCaption/tblDescription; a per-row jc + tblCellSpacing; and
        // per-cell tcFitText/hideMark.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:tbl>
                <w:tblPr>
                    <w:tblOverlap w:val="never"/>
                    <w:tblW w:type="dxa" w:w="9000"/>
                    <w:tblCellSpacing w:type="dxa" w:w="15"/>
                    <w:tblInd w:type="dxa" w:w="240"/>
                    <w:tblCaption w:val="Quarterly figures"/>
                    <w:tblDescription w:val="Revenue by region and quarter"/>
                </w:tblPr>
                <w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid>
                <w:tr>
                    <w:trPr><w:tblCellSpacing w:type="dxa" w:w="20"/><w:jc w:val="center"/></w:trPr>
                    <w:tc>
                        <w:tcPr><w:tcFitText/><w:hideMark/></w:tcPr>
                        <w:p><w:r><w:t>x</w:t></w:r></w:p>
                    </w:tc>
                </w:tr>
            </w:tbl>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "expanded table properties survive write -> reopen unchanged"
        );
    }

    #[test]
    fn writer_is_deterministic() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let m = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        assert_eq!(
            write_document(&m, &BTreeMap::new()).unwrap(),
            write_document(&m, &BTreeMap::new()).unwrap(),
            "the same model writes identical bytes"
        );
    }

    #[test]
    fn inline_constructs_survive_the_semantic_round_trip() {
        // A paragraph exercising every self-contained inline construct: an
        // internal hyperlink (anchor + tooltip) wrapping a run, a bookmark range
        // around it, a simple field with a cached result, an insertion and a
        // deletion revision (the deletion's run is delText), and an inline
        // content control with typed properties — all survive write -> reopen.
        let xml = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
            <w:p>
                <w:bookmarkStart w:id="7" w:name="anchor"/>
                <w:hyperlink w:anchor="anchor" w:tooltip="jump">
                    <w:r><w:t xml:space="preserve">see anchor</w:t></w:r></w:hyperlink>
                <w:bookmarkEnd w:id="7"/>
                <w:fldSimple w:instr=" PAGE \* MERGEFORMAT ">
                    <w:r><w:t>1</w:t></w:r></w:fldSimple>
                <w:ins w:author="alice" w:date="2020-01-02T03:04:05Z" w:id="3">
                    <w:r><w:t xml:space="preserve">inserted</w:t></w:r></w:ins>
                <w:del w:author="bob">
                    <w:r><w:delText xml:space="preserve">deleted</w:delText></w:r></w:del>
                <w:sdt>
                    <w:sdtPr><w:alias w:val="Company"/><w:tag w:val="c"/><w:id w:val="99"/><w:text/></w:sdtPr>
                    <w:sdtContent><w:r><w:rPr><w:b/></w:rPr><w:t>Acme</w:t></w:r></w:sdtContent></w:sdt>
            </w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(
            m1, m2,
            "the inline-construct model survives write -> reopen unchanged"
        );
    }

    #[test]
    fn external_hyperlink_survives_the_semantic_round_trip() {
        // An external hyperlink resolves through a relationship; the writer must
        // regenerate `document.xml.rels` so the reopened model carries the same
        // URL. Source is a full package (the URL only resolves with its rels).
        let document = br#"<w:document xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>
            <w:p><w:hyperlink r:id="rId100" w:tooltip="visit">
                <w:r><w:t>Example</w:t></w:r></w:hyperlink></w:p>
        </w:body></w:document>"#;
        let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId100" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/a" TargetMode="External"/></Relationships>"#;
        let source = pack(document, rels);
        let mut src_package = DocxPackage::open(&source, PackageLimits::default()).unwrap();
        let m1 = import_package(
            &mut src_package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(
            m1, m2,
            "the external-hyperlink model survives write -> reopen unchanged"
        );
    }

    #[test]
    fn attribute_entities_survive_the_semantic_round_trip() {
        // Every attribute-carried string (field instruction, bookmark name,
        // hyperlink tooltip/anchor, revision author, sdt alias) may contain XML
        // metacharacters. The writer escapes them; the importer must unescape so
        // the value round-trips instead of gaining an `amp;` layer each pass.
        let xml = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
            <w:p>
                <w:bookmarkStart w:id="1" w:name="A &amp; B &lt;x&gt;"/>
                <w:hyperlink w:anchor="A &amp; B &lt;x&gt;" w:tooltip="quote &quot;here&quot;">
                    <w:r><w:t>link</w:t></w:r></w:hyperlink>
                <w:bookmarkEnd w:id="1"/>
                <w:fldSimple w:instr=" HYPERLINK &quot;http://x/a?u=1&amp;v=2&quot; ">
                    <w:r><w:t>r</w:t></w:r></w:fldSimple>
                <w:ins w:author="a &amp; b"><w:r><w:t>i</w:t></w:r></w:ins>
                <w:sdt><w:sdtPr><w:alias w:val="Tom &amp; Jerry"/><w:text/></w:sdtPr>
                    <w:sdtContent><w:r><w:t>s</w:t></w:r></w:sdtContent></w:sdt>
            </w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(
            m1, m2,
            "attribute values with XML metacharacters survive write -> reopen"
        );
    }

    #[test]
    fn run_fonts_survive_the_semantic_round_trip() {
        // A run whose `w:rFonts` mixes named and theme slots across all four axes
        // plus a `@hint` — the writer must emit `w:rFonts` (it emitted none
        // before) so the model round-trips.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr>
                <w:rFonts w:ascii="Calibri" w:hAnsiTheme="minorHAnsi"
                    w:eastAsia="MS Mincho" w:csTheme="majorBidi" w:hint="eastAsia"/>
            </w:rPr><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "the run-fonts model survives write -> reopen");
    }

    #[test]
    fn font_table_survives_the_semantic_round_trip() {
        // A fontTable.xml with a fully-populated descriptor (altName, panose1,
        // charset, family, pitch, sig, notTrueType) and a minimal one must
        // survive: the writer regenerates the part, its content-type override,
        // and the /fontTable relationship.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/></Relationships>"#;
        let font_table = br#"<w:fonts xmlns:w="urn:w">
            <w:font w:name="Calibri">
                <w:altName w:val="Carlito"/><w:panose1 w:val="020F0502020204030204"/>
                <w:charset w:val="00"/><w:family w:val="swiss"/><w:pitch w:val="variable"/>
                <w:sig w:usb0="E4002EFF" w:usb1="C000247B" w:csb0="0000019F"/>
                <w:notTrueType/></w:font>
            <w:font w:name="Symbol"><w:family w:val="roman"/></w:font>
        </w:fonts>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/fontTable.xml", font_table),
        ]);
        let mut src_package = DocxPackage::open(&source, PackageLimits::default()).unwrap();
        let m1 = import_package(
            &mut src_package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1.definitions().font_table.len(), 2, "both fonts imported");

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "the font-table model survives write -> reopen");
    }

    #[test]
    fn embedded_fonts_survive_the_semantic_round_trip() {
        // A fontTable font with an embedded regular face whose .odttf resolves
        // through fontTable.xml.rels. The writer regenerates fontTable.xml (with
        // the w:embedRegular), fontTable.xml.rels (/font), the odttf content-type
        // Default, and the .odttf part.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="odttf" ContentType="application/vnd.openxmlformats-officedocument.obfuscatedFont"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/></Relationships>"#;
        let font_table = br#"<w:fonts xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
            <w:font w:name="Calibri"><w:panose1 w:val="020F0502"/>
                <w:embedRegular r:id="rIdF1" w:fontKey="{6C99A02D-4E1B-4E5A-9F0C-1234567890AB}" w:subsetted="true"/>
                <w:embedBold r:id="rIdF2" w:fontKey="{AB99A02D-4E1B-4E5A-9F0C-1234567890AB}"/></w:font>
        </w:fonts>"#;
        let font_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdF1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/font" Target="fonts/font1.odttf"/><Relationship Id="rIdF2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/font" Target="fonts/font2.odttf"/></Relationships>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/fontTable.xml", font_table),
            ("word/_rels/fontTable.xml.rels", font_rels),
            ("word/fonts/font1.odttf", b"ODTTF-DATA-1"),
            ("word/fonts/font2.odttf", b"ODTTF-DATA-2"),
        ]);
        let m1 = reopen(&source);
        let font = &m1.definitions().font_table[0];
        assert!(font.embedded.regular.is_some() && font.embedded.bold.is_some());
        assert_eq!(
            font.embedded.regular.as_ref().unwrap().part_name,
            "word/fonts/font1.odttf"
        );
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "embedded fonts survive write -> reopen");
    }

    #[test]
    fn settings_font_embedding_flags_survive_the_semantic_round_trip() {
        // word/settings.xml carrying the font-embedding CT_OnOff flags: one bare
        // (present => true), one explicitly true, one explicitly false (=> the
        // model records false and the writer omits it). The writer regenerates
        // settings.xml, its content-type override, and the /settings relationship.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
        let settings = br#"<w:settings xmlns:w="urn:w"><w:embedTrueTypeFonts/><w:saveSubsetFonts w:val="true"/><w:embedSystemFonts w:val="false"/></w:settings>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/settings.xml", settings),
        ]);
        let m1 = reopen(&source);
        let flags = &m1.definitions().settings;
        assert!(flags.embed_true_type_fonts, "bare flag => true");
        assert!(flags.save_subset_fonts, "explicit true");
        assert!(!flags.embed_system_fonts, "explicit false");
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "settings flags survive write -> reopen");
    }

    #[test]
    fn theme_font_scheme_survives_the_semantic_round_trip() {
        // A theme with a fontScheme (major + minor, base entries with hints, a
        // per-script override, empty ea/cs) plus a minimal clrScheme (now also
        // modeled) — the writer regenerates theme1.xml, its content-type
        // override, and the /theme relationship, and the whole theme round-trips.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/></Relationships>"#;
        let theme = br#"<a:theme xmlns:a="urn:a" name="Office Theme"><a:themeElements>
            <a:fontScheme name="Office">
                <a:majorFont>
                    <a:latin typeface="Calibri Light" panose="020F0302020204030204" pitchFamily="34" charset="0"/>
                    <a:ea typeface=""/><a:cs typeface=""/>
                    <a:font script="Jpan" typeface="Yu Gothic Light"/></a:majorFont>
                <a:minorFont>
                    <a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont>
            </a:fontScheme>
            <a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText"/></a:dk1></a:clrScheme>
        </a:themeElements></a:theme>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/theme/theme1.xml", theme),
        ]);
        let mut src_package = DocxPackage::open(&source, PackageLimits::default()).unwrap();
        let m1 = import_package(
            &mut src_package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        let scheme = m1.definitions().font_scheme.as_ref().unwrap();
        assert_eq!(scheme.major.latin.typeface, "Calibri Light");
        assert_eq!(scheme.major.script_overrides.len(), 1);
        assert_eq!(scheme.minor.latin.typeface, "Calibri");

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "the theme font scheme survives write -> reopen");
    }

    #[test]
    fn styles_survive_the_semantic_round_trip() {
        // A paragraph style (basedOn another, with pPr + rPr), a character style,
        // and body paragraphs/runs referencing them via w:pStyle / w:rStyle. The
        // writer regenerates styles.xml and derives a stable w:styleId from each
        // StyleId so the references resolve back to the same styles.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>
            <w:p><w:r><w:rPr><w:rStyle w:val="Emphasis"/></w:rPr><w:t>em</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
        let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
            <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/>
                <w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/></w:pPr>
                <w:rPr><w:b/></w:rPr></w:style>
            <w:style w:type="character" w:styleId="Emphasis"><w:name w:val="Emphasis"/>
                <w:rPr><w:i/></w:rPr></w:style>
        </w:styles>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/styles.xml", styles),
        ]);
        let mut src_package = DocxPackage::open(&source, PackageLimits::default()).unwrap();
        let m1 = import_package(
            &mut src_package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        // Sanity: three styles imported and the body references resolved.
        assert_eq!(m1.definitions().styles.iter().count(), 3);

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "styles + style references survive write -> reopen");
    }

    #[test]
    fn theme_color_and_format_schemes_survive_the_semantic_round_trip() {
        // A full theme: a 12-slot clrScheme (sysClr with lastClr for dk1/lt1,
        // srgbClr for the rest), a fontScheme, and an fmtScheme. The clrScheme is
        // modeled and the fmtScheme is retained verbatim, so every theme colour
        // reference resolves and the format scheme round-trips byte-for-byte.
        use casual_doc_model::v1::{RgbColor, SchemeColor};

        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/></Relationships>"#;
        let theme = br#"<a:theme xmlns:a="urn:a" name="Office Theme"><a:themeElements>
            <a:clrScheme name="Office">
                <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
                <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
                <a:dk2><a:srgbClr val="44546A"/></a:dk2>
                <a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
                <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
                <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
                <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
                <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
                <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>
                <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
                <a:hlink><a:srgbClr val="0563C1"/></a:hlink>
                <a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
            </a:clrScheme>
            <a:fontScheme name="Office">
                <a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>
                <a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont>
            </a:fontScheme>
            <a:fmtScheme name="Office">
                <a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>
                <a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>
                <a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>
                <a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>
            </a:fmtScheme>
        </a:themeElements></a:theme>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/theme/theme1.xml", theme),
        ]);
        let m1 = reopen(&source);
        let defs = m1.definitions();
        let scheme = defs.color_scheme.as_ref().expect("clrScheme modeled");
        assert_eq!(scheme.name, "Office");
        match &scheme.dark1 {
            SchemeColor::System(system) => {
                assert_eq!(system.value, "windowText");
                assert_eq!(system.last_color, Some(RgbColor { r: 0, g: 0, b: 0 }));
            }
            other => panic!("dk1 should be a sysClr, got {other:?}"),
        }
        assert_eq!(
            scheme.accent1,
            SchemeColor::Srgb(RgbColor {
                r: 0x44,
                g: 0x72,
                b: 0xC4
            })
        );
        assert_eq!(
            scheme.followed_hyperlink,
            SchemeColor::Srgb(RgbColor {
                r: 0x95,
                g: 0x4F,
                b: 0x72
            })
        );
        let retained = defs.format_scheme_xml.as_ref().expect("fmtScheme retained");
        assert!(retained.contains("fillStyleLst"), "fmtScheme captured");

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m2.definitions().format_scheme_xml,
            defs.format_scheme_xml,
            "the retained fmtScheme is preserved across write -> reopen"
        );
        assert_eq!(
            m1, m2,
            "the theme colour + format schemes survive write -> reopen"
        );
    }

    #[test]
    fn doc_defaults_survive_the_semantic_round_trip() {
        // A styles.xml with w:docDefaults (a default font + size in rPrDefault and
        // default paragraph spacing in pPrDefault). Losing docDefaults shifts the
        // whole document's base formatting, so it must be modeled and re-emitted.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
        let styles = br#"<w:styles xmlns:w="urn:w">
            <w:docDefaults>
                <w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/></w:rPr></w:rPrDefault>
                <w:pPrDefault><w:pPr><w:spacing w:before="120" w:after="240"/></w:pPr></w:pPrDefault>
            </w:docDefaults>
            <w:style w:type="paragraph" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
        </w:styles>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/styles.xml", styles),
        ]);
        let m1 = reopen(&source);
        let defaults = m1
            .definitions()
            .document_defaults
            .as_ref()
            .expect("docDefaults modeled");
        let run = defaults.run.as_ref().expect("rPrDefault modeled");
        assert_eq!(run.size_half_points, Some(24));
        let paragraph = defaults.paragraph.as_ref().expect("pPrDefault modeled");
        assert_eq!(paragraph.spacing.and_then(|s| s.before_twips), Some(120));
        assert_eq!(paragraph.spacing.and_then(|s| s.after_twips), Some(240));

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m2.definitions().document_defaults,
            m1.definitions().document_defaults,
            "docDefaults survive write -> reopen"
        );
        assert_eq!(
            m1, m2,
            "the whole model (incl. docDefaults) is a fixed point"
        );
    }

    #[test]
    fn numbering_survives_the_semantic_round_trip() {
        // An abstract definition with two levels, a numbering instance using it,
        // and a body paragraph whose w:numPr references the instance. The writer
        // regenerates numbering.xml and derives stable numId/abstractNumId
        // strings so the num->abstract link and the body numPr resolve back.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="3"/></w:numPr></w:pPr>
                <w:r><w:t>item</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;
        let numbering = br#"<w:numbering xmlns:w="urn:w">
            <w:abstractNum w:abstractNumId="0">
                <w:lvl w:ilvl="0"><w:start w:val="1"/></w:lvl>
                <w:lvl w:ilvl="1"><w:start w:val="5"/></w:lvl></w:abstractNum>
            <w:num w:numId="3"><w:abstractNumId w:val="0"/></w:num>
        </w:numbering>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/numbering.xml", numbering),
        ]);
        let mut src_package = DocxPackage::open(&source, PackageLimits::default()).unwrap();
        let m1 = import_package(
            &mut src_package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1.definitions().abstract_numbering.iter().count(), 1);
        assert_eq!(m1.definitions().numbering.iter().count(), 1);

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "numbering + numPr survive write -> reopen");
    }

    #[test]
    fn expanded_settings_survive_the_semantic_round_trip() {
        // settings.xml carrying the newly modeled settings (evenAndOddHeaders,
        // defaultTabStop, trackChanges, documentProtection, a proofState, a zoom,
        // and a compatSetting) plus an UNMODELED setting (`w:autoHyphenation`). The
        // modeled settings survive write -> reopen as a fixed point; the unmodeled
        // one produces a compat-report entry (omitted, not-retained in Semantic).
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
        let settings = br#"<w:settings xmlns:w="urn:w">
            <w:zoom w:percent="120"/>
            <w:proofState w:spelling="clean" w:grammar="dirty"/>
            <w:trackChanges/>
            <w:documentProtection w:edit="comments" w:enforcement="1"/>
            <w:defaultTabStop w:val="708"/>
            <w:autoHyphenation/>
            <w:evenAndOddHeaders/>
            <w:compat><w:compatSetting w:name="compatibilityMode" w:uri="urn:x" w:val="15"/></w:compat>
        </w:settings>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/settings.xml", settings),
        ]);
        let mut src_package = DocxPackage::open(&source, PackageLimits::default()).unwrap();
        let import = import_package(
            &mut src_package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap();
        let m1 = import.document;
        let s = m1.definitions().settings.clone();
        assert!(s.even_and_odd_headers);
        assert!(s.track_changes);
        assert_eq!(s.default_tab_stop, Some(708));
        assert_eq!(s.compat.len(), 1);
        // The unmodeled setting is reported, not silently dropped.
        assert!(
            import
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == "autoHyphenation"),
            "the unmodeled setting produces a compat-report entry"
        );

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "expanded settings survive write -> reopen");
    }

    #[test]
    fn multi_level_numbering_detail_survives_the_semantic_round_trip() {
        // A multi-level list: level 0 decimal "%1." with an indent pPr, level 1 a
        // bullet whose lvlText is a symbol glyph and whose rPr selects the symbol
        // font. The writer regenerates numbering.xml with the full level detail; the
        // format/text/justify/properties survive write -> reopen as a fixed point.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr>
                <w:r><w:t>a</w:t></w:r></w:p>
            <w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="2"/></w:numPr></w:pPr>
                <w:r><w:t>b</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;
        let numbering = br#"<w:numbering xmlns:w="urn:w">
            <w:abstractNum w:abstractNumId="7">
                <w:lvl w:ilvl="0">
                    <w:start w:val="1"/>
                    <w:numFmt w:val="decimal"/>
                    <w:lvlText w:val="%1."/>
                    <w:lvlJc w:val="left"/>
                    <w:pPr><w:ind w:start="720" w:hanging="360"/></w:pPr>
                </w:lvl>
                <w:lvl w:ilvl="1">
                    <w:start w:val="1"/>
                    <w:numFmt w:val="bullet"/>
                    <w:suff w:val="tab"/>
                    <w:lvlText w:val="&#61623;"/>
                    <w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol"/></w:rPr>
                </w:lvl>
            </w:abstractNum>
            <w:num w:numId="2"><w:abstractNumId w:val="7"/></w:num>
        </w:numbering>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/numbering.xml", numbering),
        ]);
        let m1 = reopen(&source);
        let (_, abstract_num) = m1.definitions().abstract_numbering.iter().next().unwrap();
        assert_eq!(abstract_num.levels.len(), 2);
        assert_eq!(abstract_num.levels[0].lvl_text.as_deref(), Some("%1."));
        assert_eq!(abstract_num.levels[1].lvl_text.as_deref(), Some("\u{f0b7}"));
        assert!(abstract_num.levels[0].paragraph_properties.is_some());
        assert!(abstract_num.levels[1].run_properties.is_some());

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "multi-level numbering detail survives write -> reopen"
        );
    }

    #[test]
    fn notes_survive_the_semantic_round_trip() {
        // A footnote and an endnote, each referenced from the body. The writer
        // regenerates footnotes.xml/endnotes.xml with ids derived from the NoteId
        // and the body w:footnoteReference/w:endnoteReference reference them.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/><Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>a</w:t></w:r><w:r><w:footnoteReference w:id="2"/></w:r></w:p>
            <w:p><w:r><w:t>b</w:t></w:r><w:r><w:endnoteReference w:id="5"/></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/></Relationships>"#;
        let footnotes = br#"<w:footnotes xmlns:w="urn:w"><w:footnote w:id="2"><w:p><w:r><w:t>fn body</w:t></w:r></w:p></w:footnote></w:footnotes>"#;
        let endnotes = br#"<w:endnotes xmlns:w="urn:w"><w:endnote w:id="5"><w:p><w:r><w:t>en body</w:t></w:r></w:p></w:endnote></w:endnotes>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/footnotes.xml", footnotes),
            ("word/endnotes.xml", endnotes),
        ]);
        let m1 = reopen(&source);
        assert_eq!(m1.definitions().footnotes.iter().count(), 1);
        assert_eq!(m1.definitions().endnotes.iter().count(), 1);
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "footnotes + endnotes + refs survive write -> reopen"
        );
    }

    #[test]
    fn comments_survive_the_semantic_round_trip() {
        // A comment with author/initials/date, referenced from the body.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r><w:r><w:commentReference w:id="1"/></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/></Relationships>"#;
        let comments = br#"<w:comments xmlns:w="urn:w"><w:comment w:id="1" w:author="Alice" w:initials="AC" w:date="2020-01-02T03:04:05Z"><w:p><w:r><w:t>a note</w:t></w:r></w:p></w:comment></w:comments>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/comments.xml", comments),
        ]);
        let m1 = reopen(&source);
        assert_eq!(m1.definitions().comments.iter().count(), 1);
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "comments + refs survive write -> reopen");
    }

    #[test]
    fn note_internal_hyperlink_routes_to_the_part_own_rels() {
        // A hyperlink inside a footnote must resolve through the footnote part's
        // OWN rels (word/_rels/footnotes.xml.rels), not document.xml.rels. The
        // writer uses a per-part Ctx and regenerates the part-own rels file.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:footnoteReference w:id="2"/></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/></Relationships>"#;
        let footnotes = br#"<w:footnotes xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:footnote w:id="2"><w:p><w:hyperlink r:id="rIdX"><w:r><w:t>src</w:t></w:r></w:hyperlink></w:p></w:footnote></w:footnotes>"#;
        let footnotes_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdX" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/n" TargetMode="External"/></Relationships>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/footnotes.xml", footnotes),
            ("word/_rels/footnotes.xml.rels", footnotes_rels),
        ]);
        let m1 = reopen(&source);
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "a note-internal hyperlink survives via the part-own rels"
        );
    }

    #[test]
    fn inline_drawing_survives_the_semantic_round_trip() {
        // An inline embedded picture: the writer regenerates the media part, its
        // content-type Default, the /image relationship (verbatim id), and the
        // w:drawing scaffold whose a:blip@r:embed points back at the media.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body>
            <w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="685800"/>
                <a:graphic><a:graphicData><pic:pic><pic:blipFill>
                    <a:blip r:embed="rId7"/></pic:blipFill></pic:pic></a:graphicData></a:graphic>
            </wp:inline></w:drawing></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/media/image1.png", b"PNGDATA"),
        ]);
        let m1 = reopen(&source);
        assert_eq!(m1.definitions().media.iter().count(), 1);
        // The writer needs no bytes for the model to round-trip (MediaReference
        // holds no bytes); it emits an empty media part.
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "an inline drawing survives write -> reopen");
    }

    #[test]
    fn text_box_survives_the_semantic_round_trip() {
        // An inline text box (w:txbxContent) holding block content — the writer
        // regenerates the DrawingML shape scaffold the importer triggers on.
        let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
            <w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent>
                    <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>boxed text</w:t></w:r></w:p>
                    <w:p><w:r><w:t>second line</w:t></w:r></w:p>
                </w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        // The paragraph's inline should be a text box.
        assert!(matches!(
            m1.body().first(),
            Some(casual_doc_model::v1::BlockNode::Paragraph(p))
                if matches!(p.inlines.first(), Some(casual_doc_model::v1::InlineNode::TextBox(_)))
        ));
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "an inline text box survives write -> reopen");
    }

    #[test]
    fn section_geometry_survives_the_semantic_round_trip() {
        // A body-level w:sectPr (page size, margins, columns) — the writer
        // emitted none before, so a section was silently dropped on write.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p>
            <w:sectPr>
                <w:pgSz w:w="12240" w:h="15840"/>
                <w:pgMar w:top="1440" w:bottom="1440" w:start="1800" w:end="1800"/>
                <w:cols w:num="2"/>
            </w:sectPr>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        assert_eq!(m1.definitions().sections.len(), 1);
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "section geometry survives write -> reopen");
    }

    #[test]
    fn expanded_section_properties_survive_the_semantic_round_trip() {
        // The additive sectPr coverage: w:type, w:cols @space/@sep, w:pgNumType,
        // w:vAlign, w:titlePg (explicit off), and w:docGrid.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p>
            <w:sectPr>
                <w:type w:val="continuous"/>
                <w:pgSz w:w="12240" w:h="15840"/>
                <w:pgMar w:top="1440" w:bottom="1440" w:start="1440" w:end="1440"/>
                <w:pgNumType w:fmt="lowerRoman" w:start="3"/>
                <w:cols w:num="2" w:space="720" w:sep="1"/>
                <w:vAlign w:val="center"/>
                <w:titlePg w:val="0"/>
                <w:docGrid w:type="lines" w:linePitch="360" w:charSpace="20"/>
            </w:sectPr>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let section = &m1.definitions().sections[0];
        assert_eq!(
            section.section_type,
            Some(casual_doc_model::v1::SectionType::Continuous)
        );
        assert_eq!(section.page_numbering.start, Some(3));
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "expanded section properties survive write -> reopen"
        );
    }

    #[test]
    fn multi_section_survives_the_semantic_round_trip() {
        // Two sections: the first ends at paragraph one via a nested
        // w:pPr > w:sectPr (distinct geometry + columns); the second is the
        // trailing body-level section (landscape). The writer must emit the
        // first section inside the paragraph's pPr and the second at body end,
        // and both section ids must reproduce in document order.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p>
                <w:pPr>
                    <w:sectPr>
                        <w:pgSz w:w="12240" w:h="15840"/>
                        <w:pgMar w:top="1440" w:bottom="1440" w:start="1800" w:end="1800"/>
                        <w:cols w:num="2"/>
                    </w:sectPr>
                </w:pPr>
                <w:r><w:t>section one</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>section two</w:t></w:r></w:p>
            <w:sectPr>
                <w:pgSz w:w="15840" w:h="12240"/>
                <w:pgMar w:top="720" w:bottom="720" w:start="720" w:end="720"/>
                <w:cols w:num="1"/>
            </w:sectPr>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        assert_eq!(m1.definitions().sections.len(), 2, "two sections modeled");
        // The first paragraph carries the break to the first section.
        let first = m1.definitions().sections[0].id;
        let casual_doc_model::v1::BlockNode::Paragraph(paragraph) = &m1.body()[0] else {
            panic!("expected a paragraph");
        };
        assert_eq!(
            paragraph.properties.section_break,
            Some(first),
            "paragraph one ends section one"
        );
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "multi-section survives write -> reopen");
    }

    #[test]
    fn headers_footers_survive_the_semantic_round_trip() {
        // A header and a footer referenced from the body sectPr. The writer
        // regenerates the parts, their document relationships, and the sectPr
        // references, deriving a stable relationship id from each HeaderFooterId.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p>
            <w:sectPr>
                <w:headerReference w:type="default" r:id="rIdHa"/>
                <w:footerReference w:type="default" r:id="rIdFa"/>
                <w:pgSz w:w="12240" w:h="15840"/>
                <w:pgMar w:top="1440" w:bottom="1440" w:start="1440" w:end="1440"/>
                <w:cols w:num="1"/>
            </w:sectPr>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdHa" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rIdFa" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#;
        let header =
            br#"<w:hdr xmlns:w="urn:w"><w:p><w:r><w:t>the header</w:t></w:r></w:p></w:hdr>"#;
        let footer =
            br#"<w:ftr xmlns:w="urn:w"><w:p><w:r><w:t>the footer</w:t></w:r></w:p></w:ftr>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/header1.xml", header),
            ("word/footer1.xml", footer),
        ]);
        let m1 = reopen(&source);
        assert_eq!(m1.definitions().headers.iter().count(), 1);
        assert_eq!(m1.definitions().footers.iter().count(), 1);
        assert_eq!(m1.definitions().sections[0].headers.len(), 1);
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "headers/footers + sectPr refs survive write -> reopen"
        );
    }

    #[test]
    fn style_with_empty_properties_survives_the_round_trip() {
        // Regression (review): a style whose pPr/rPr is present but all-default
        // imports as Some(default); the writer must still emit the (empty)
        // element so re-import keeps Some(default), not None.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
        // Heading1: present but empty pPr -> Some(default); Emphasis: empty rPr.
        let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="h1"/><w:pPr/></w:style>
            <w:style w:type="character" w:styleId="Emphasis"><w:name w:val="em"/><w:rPr/></w:style>
        </w:styles>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/styles.xml", styles),
        ]);
        let m1 = reopen(&source);
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "a style with an empty pPr/rPr survives write -> reopen"
        );
    }

    #[test]
    fn paragraph_spacing_borders_shading_tabs_survive_the_round_trip() {
        // The structured paragraph properties the writer previously dropped:
        // w:spacing (incl. a non-round line percent to exercise the ceiling
        // round-trip), w:pBdr, w:shd, and w:tabs.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr>
                <w:spacing w:before="120" w:after="240" w:line="360" w:lineRule="auto"/>
                <w:pBdr>
                    <w:top w:val="single" w:sz="8" w:color="112233" w:space="4"/>
                    <w:bar w:val="dotted"/></w:pBdr>
                <w:shd w:val="clear" w:color="auto" w:fill="EEEEEE"/>
                <w:tabs>
                    <w:tab w:val="center" w:pos="2160" w:leader="dot"/>
                    <w:tab w:val="end" w:pos="9360"/></w:tabs>
            </w:pPr><w:r><w:t>a</w:t></w:r></w:p>
            <w:p><w:pPr><w:spacing w:line="100" w:lineRule="auto"/></w:pPr>
                <w:r><w:t>b</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "paragraph spacing/borders/shading/tabs survive write -> reopen"
        );
    }

    #[test]
    fn new_paragraph_properties_survive_the_semantic_round_trip() {
        // The additive pPr coverage: the tri-state CT_OnOff toggles (an explicit
        // off via w:val="0" on a default-ON toggle must survive) and
        // w:textAlignment.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr>
                <w:bidi/><w:wordWrap w:val="0"/><w:kinsoku/><w:snapToGrid w:val="0"/>
                <w:mirrorIndents/><w:adjustRightInd w:val="0"/><w:suppressAutoHyphens/>
                <w:overflowPunct w:val="0"/><w:topLinePunct/><w:autoSpaceDE w:val="0"/>
                <w:autoSpaceDN/><w:textAlignment w:val="center"/>
            </w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "every new paragraph property survives write -> reopen"
        );
    }

    #[test]
    fn paragraph_mark_run_properties_survive_the_semantic_round_trip() {
        // The paragraph-mark w:rPr (the pilcrow's own formatting): a formatted
        // mark on the first paragraph and a present-but-empty mark on the second
        // (which must round-trip as Some(default), not None).
        use casual_doc_model::v1::BlockNode;
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:rPr><w:b/><w:sz w:val="28"/><w:color w:val="FF0000"/></w:rPr></w:pPr>
                <w:r><w:t>a</w:t></w:r></w:p>
            <w:p><w:pPr><w:rPr/></w:pPr><w:r><w:t>b</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let BlockNode::Paragraph(p0) = &m1.body()[0] else {
            panic!("expected a paragraph");
        };
        let mark = p0.properties.mark_run.as_ref().expect("mark run modeled");
        assert_eq!(mark.bold, Some(true));
        assert_eq!(mark.size_half_points, Some(28));
        let BlockNode::Paragraph(p1) = &m1.body()[1] else {
            panic!("expected a paragraph");
        };
        assert!(
            p1.properties.mark_run.is_some(),
            "a present-but-empty mark rPr is Some(default), not None"
        );
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "paragraph-mark run properties survive write -> reopen"
        );
    }

    #[test]
    fn block_content_control_survives_the_semantic_round_trip() {
        // A block-level content control (w:sdt wrapping block content) with typed
        // properties — the writer previously emitted only the inner blocks,
        // dropping the wrapper + properties (found by the completeness audit).
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:sdt>
                <w:sdtPr><w:alias w:val="Section"/><w:tag w:val="sec"/><w:id w:val="42"/><w:richText/></w:sdtPr>
                <w:sdtContent>
                    <w:p><w:r><w:t>inside the control</w:t></w:r></w:p>
                    <w:p><w:r><w:t>second block</w:t></w:r></w:p>
                </w:sdtContent>
            </w:sdt>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        assert!(matches!(
            m1.body().first(),
            Some(casual_doc_model::v1::BlockNode::Sdt(_))
        ));
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(
            m1, m2,
            "a block content control + properties survive write -> reopen"
        );
    }

    #[test]
    fn all_run_properties_survive_the_semantic_round_trip() {
        // Every modeled run property (toggles on AND off, the value-carrying
        // vocabularies, the typographic metrics, and w:lang's three tags) must
        // round-trip; write_run_properties previously emitted only a handful.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr>
                <w:b/><w:i w:val="0"/><w:strike/><w:dstrike/>
                <w:caps/><w:smallCaps w:val="0"/><w:vanish/><w:webHidden/>
                <w:u/><w:color w:val="AABBCC"/><w:sz w:val="24"/>
                <w:vertAlign w:val="superscript"/><w:highlight w:val="cyan"/><w:em w:val="dot"/>
                <w:spacing w:val="20"/><w:position w:val="-6"/><w:kern w:val="18"/>
                <w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/>
            </w:rPr><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "every run property survives write -> reopen");
    }

    #[test]
    fn new_run_properties_survive_the_semantic_round_trip() {
        // The additive rPr coverage: the CT_OnOff effect toggles (on AND an
        // explicit off via w:val="0"), rtl/snapToGrid/specVanish, and the two
        // complex children w:bdr (a border edge) and w:shd (fill).
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr>
                <w:outline/><w:shadow w:val="0"/><w:emboss/><w:imprint/>
                <w:snapToGrid w:val="0"/><w:rtl/><w:specVanish/>
                <w:bdr w:val="single" w:sz="8" w:color="FF0000" w:space="4"/>
                <w:shd w:val="clear" w:color="auto" w:fill="00FF00"/>
            </w:rPr><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "every new run property survives write -> reopen");
    }

    #[test]
    fn standard_cstheme_spelling_is_captured_and_normalized() {
        // Real Word writes the complex-script theme slot as `w:cstheme` (all
        // lowercase — the one rFonts theme attribute that breaks the camelCase
        // pattern). The importer must read it (previously it only read the
        // legacy `w:csTheme`, silently dropping the slot on genuine files), and
        // the writer must emit the standard spelling so it round-trips.
        use casual_doc_model::v1::{BlockNode, FontRef, InlineNode, ThemeFontRef};
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr>
                <w:rFonts w:cstheme="majorBidi"/>
            </w:rPr><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let BlockNode::Paragraph(paragraph) = &m1.body()[0] else {
            panic!("expected a paragraph");
        };
        let InlineNode::Run(run) = &paragraph.inlines[0] else {
            panic!("expected a run");
        };
        assert!(
            matches!(
                run.properties.font_ref_cs,
                Some(FontRef::Theme(ref t)) if t.slot == ThemeFontRef::MajorBidi
            ),
            "standard w:cstheme is captured into the cs slot"
        );
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let m2 = reopen(&bytes);
        assert_eq!(m1, m2, "cstheme survives write -> reopen");
    }

    #[test]
    fn external_hyperlink_url_with_ampersand_survives_the_round_trip() {
        // A query-string URL carries `&` (escaped `&amp;` in the rels Target).
        // The package parser must unescape it so the regenerated relationship
        // resolves to the identical URL.
        let document = br#"<w:document xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>
            <w:p><w:hyperlink r:id="rId100"><w:r><w:t>q</w:t></w:r></w:hyperlink></w:p>
        </w:body></w:document>"#;
        let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId100" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/s?u=1&amp;v=2&amp;w=3" TargetMode="External"/></Relationships>"#;
        let source = pack(document, rels);
        let mut src_package = DocxPackage::open(&source, PackageLimits::default()).unwrap();
        let m1 = import_package(
            &mut src_package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "an external URL with `&` survives write -> reopen");
    }

    // --- Opaque part side-table (P1F-2) --------------------------------------

    /// Imports a whole package in Semantic mode, returning the full import result
    /// (model + opaque part side-table).
    fn import_semantic(bytes: &[u8]) -> casual_doc_import::Import {
        let mut package = DocxPackage::open(bytes, PackageLimits::default()).unwrap();
        import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
    }

    /// A package with an unmodeled customXml data store: `customXml/item1.xml`
    /// (referenced from `document.xml.rels`), its owned rels to
    /// `customXml/itemProps1.xml`, and the props part. None of it is consumed by
    /// the semantic model.
    fn customxml_package() -> Vec<u8> {
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/customXml/itemProps1.xml" ContentType="application/vnd.openxmlformats-officedocument.customXmlProperties+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../customXml/item1.xml"/></Relationships>"#;
        let item = br#"<root xmlns="urn:enterprise-template"><field>value</field></root>"#;
        let item_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps" Target="itemProps1.xml"/></Relationships>"#;
        let item_props = br#"<ds:datastoreItem ds:itemID="{GUID}" xmlns:ds="http://schemas.openxmlformats.org/officeDocument/2006/customXml"/>"#;
        zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("customXml/item1.xml", item),
            ("customXml/_rels/item1.xml.rels", item_rels),
            ("customXml/itemProps1.xml", item_props),
        ])
    }

    #[test]
    fn opaque_customxml_store_survives_the_semantic_round_trip() {
        // The key deliverable: an unmodeled customXml store (data part + its owned
        // rels + props part) is imported, written by the semantic writer via the
        // side-table, and reopened; every part is byte-identical and its
        // content-type resolves. Before P1F-2 the whole store was dropped on the
        // first semantic edit→save.
        let source = customxml_package();
        let original = DocxPackage::open(&source, PackageLimits::default()).unwrap();
        let item_bytes = {
            let mut package = original;
            package.read_part("customXml/item1.xml").unwrap()
        };

        let import = import_semantic(&source);
        // The side-table carries the data part (with its owned rels) and the props
        // part; the customXml relationship keeps the store reachable.
        assert_eq!(import.retained_parts.parts.len(), 2);
        let names: Vec<&str> = import
            .retained_parts
            .parts
            .iter()
            .map(|part| part.part_name.as_str())
            .collect();
        assert_eq!(names, ["customXml/item1.xml", "customXml/itemProps1.xml"]);
        let item = &import.retained_parts.parts[0];
        assert_eq!(
            item.rels.as_ref().unwrap().part_name,
            "customXml/_rels/item1.xml.rels"
        );
        assert_eq!(import.retained_parts.relationships.len(), 1);
        assert_eq!(
            import.retained_parts.relationships[0].relationship_type,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml"
        );

        let written = write_document_with_retained_parts(
            &import.document,
            &BTreeMap::new(),
            &import.retained_parts,
        )
        .unwrap();

        // The written package opens and every preserved part is byte-identical
        // with its content-type present.
        let mut reopened = DocxPackage::open(&written, PackageLimits::default()).unwrap();
        assert_eq!(
            reopened.read_part("customXml/item1.xml").unwrap(),
            item_bytes
        );
        assert_eq!(
            reopened.content_type("customXml/item1.xml"),
            Some("application/xml")
        );
        assert_eq!(
            reopened.content_type("customXml/itemProps1.xml"),
            Some("application/vnd.openxmlformats-officedocument.customXmlProperties+xml")
        );
        // The owned rels (item1 -> itemProps1) is preserved verbatim, so the props
        // part stays reachable.
        assert_eq!(
            reopened
                .read_part("customXml/_rels/item1.xml.rels")
                .unwrap(),
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps" Target="itemProps1.xml"/></Relationships>"#
        );
        // The customXml relationship is re-added to document.xml.rels, so the
        // store is reachable from the document part.
        let doc_rels = reopened.read_part("word/_rels/document.xml.rels").unwrap();
        assert!(String::from_utf8_lossy(&doc_rels).contains("../customXml/item1.xml"));
    }

    #[test]
    fn preserved_part_is_reported_preserved_and_survives_a_reimport() {
        // After a semantic write, reopening and re-importing still preserves the
        // part and reports it `preserved` (not `not-retained`) — the disposition
        // flip is stable across a full round trip.
        let source = customxml_package();
        let import = import_semantic(&source);
        let written = write_document_with_retained_parts(
            &import.document,
            &BTreeMap::new(),
            &import.retained_parts,
        )
        .unwrap();

        let reimport = import_semantic(&written);
        let entry = reimport
            .report
            .entries
            .iter()
            .find(|entry| {
                entry
                    .part
                    .as_ref()
                    .is_some_and(|part| part.part_name == "customXml/item1.xml")
            })
            .expect("the customXml part is dispositioned");
        assert_eq!(
            entry.retention_outcome,
            casual_doc_import::RetentionOutcome::Preserved
        );
        assert_eq!(reimport.retained_parts.parts.len(), 2);
    }

    #[test]
    fn opaque_embedding_and_glossary_parts_survive_the_semantic_round_trip() {
        // An OLE embedding (`word/embeddings/oleObject1.bin`, a `.bin` whose only
        // content-type source is an `Override`) and a glossary document
        // (`word/glossary/document.xml`), neither modeled. Both survive verbatim
        // with their content-types merged into the manifest.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/embeddings/oleObject1.bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/><Override PartName="/word/glossary/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject" Target="embeddings/oleObject1.bin"/><Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/glossaryDocument" Target="glossary/document.xml"/></Relationships>"#;
        let ole = b"\x00\x01OLE-BINARY-BLOB\x02\x03";
        let glossary = br#"<w:glossaryDocument xmlns:w="urn:w"><w:docParts/></w:glossaryDocument>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/embeddings/oleObject1.bin", ole),
            ("word/glossary/document.xml", glossary),
        ]);

        let import = import_semantic(&source);
        let names: Vec<&str> = import
            .retained_parts
            .parts
            .iter()
            .map(|part| part.part_name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "word/embeddings/oleObject1.bin",
                "word/glossary/document.xml"
            ]
        );

        let written = write_document_with_retained_parts(
            &import.document,
            &BTreeMap::new(),
            &import.retained_parts,
        )
        .unwrap();
        let mut reopened = DocxPackage::open(&written, PackageLimits::default()).unwrap();
        assert_eq!(
            reopened
                .read_part("word/embeddings/oleObject1.bin")
                .unwrap(),
            ole
        );
        assert_eq!(
            reopened.content_type("word/embeddings/oleObject1.bin"),
            Some("application/vnd.openxmlformats-officedocument.oleObject")
        );
        assert_eq!(
            reopened.read_part("word/glossary/document.xml").unwrap(),
            glossary
        );
        assert_eq!(
            reopened.content_type("word/glossary/document.xml"),
            Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml"
            )
        );
    }

    /// Returns the first inline node of the first paragraph of a document body.
    fn first_inline(
        document: &casual_doc_model::v1::Document,
    ) -> &casual_doc_model::v1::InlineNode {
        use casual_doc_model::v1::BlockNode;
        let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
            panic!("expected a paragraph");
        };
        &paragraph.inlines[0]
    }

    #[test]
    fn chart_drawing_round_trips_as_an_editable_reference() {
        use casual_doc_model::v1::{EmbeddedKind, InlineNode};

        // A drawing whose `a:graphicData` carries a `c:chart` reference to a chart
        // part, which itself references an embedded workbook via its own `_rels`.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><w:body><w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="304800"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId5"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="charts/chart1.xml"/></Relationships>"#;
        let chart = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart/></c:chartSpace>"#;
        let chart_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/Microsoft_Excel_Worksheet.xlsx"/></Relationships>"#;
        let workbook = b"PK\x03\x04-fake-xlsx";
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/charts/chart1.xml", chart),
            ("word/charts/_rels/chart1.xml.rels", chart_rels),
            ("word/embeddings/Microsoft_Excel_Worksheet.xlsx", workbook),
        ]);

        // Import: the drawing is a first-class chart reference, not dropped.
        let import = import_semantic(&source);
        let InlineNode::EmbeddedObject(object) = first_inline(&import.document) else {
            panic!("expected an embedded object");
        };
        assert_eq!(object.kind, EmbeddedKind::Chart);
        assert_eq!(object.part.part_name, "word/charts/chart1.xml");
        assert_eq!(object.extent.width_emu, 914400);
        assert_eq!(object.extent.height_emu, 304800);
        // The chart part is byte-preserved by the side-table...
        assert!(
            import
                .retained_parts
                .parts
                .iter()
                .any(|part| part.part_name == "word/charts/chart1.xml")
        );
        // ...but its referencing relationship is NOT re-added as an orphan (the
        // writer emits it from the node instead — no double-emission).
        assert!(
            !import
                .retained_parts
                .relationships
                .iter()
                .any(|rel| rel.target.contains("charts/chart1.xml"))
        );

        // Write + reopen: the package opens, the chart part is present AND
        // referenced by a relationship (not orphaned).
        let written = write_document_with_retained_parts(
            &import.document,
            &BTreeMap::new(),
            &import.retained_parts,
        )
        .unwrap();
        let mut reopened = DocxPackage::open(&written, PackageLimits::default()).unwrap();
        assert_eq!(reopened.read_part("word/charts/chart1.xml").unwrap(), chart);
        // The embedded workbook (referenced through the chart's own carried rels)
        // survives too.
        assert_eq!(
            reopened
                .read_part("word/embeddings/Microsoft_Excel_Worksheet.xlsx")
                .unwrap(),
            workbook
        );
        let doc_rels_out = reopened.read_part("word/_rels/document.xml.rels").unwrap();
        let doc_rels_out = String::from_utf8_lossy(&doc_rels_out);
        assert!(doc_rels_out.contains("charts/chart1.xml"));
        assert!(doc_rels_out.contains("/relationships/chart"));
        // The chart relationship is emitted exactly once.
        assert_eq!(doc_rels_out.matches("charts/chart1.xml").count(), 1);

        // Re-import the written package: the chart reference still round-trips.
        let reimport = import_semantic(&written);
        let InlineNode::EmbeddedObject(object) = first_inline(&reimport.document) else {
            panic!("expected an embedded object after re-import");
        };
        assert_eq!(object.kind, EmbeddedKind::Chart);
        assert_eq!(object.part.part_name, "word/charts/chart1.xml");
    }

    #[test]
    fn smartart_diagram_round_trips_with_its_parts_referenced() {
        use casual_doc_model::v1::{EmbeddedKind, InlineNode};

        // A drawing whose `a:graphicData` carries a `dgm:relIds` naming the four
        // diagram parts (data / layout / quick-style / colors).
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/diagrams/data1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml"/><Override PartName="/word/diagrams/layout1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml"/><Override PartName="/word/diagrams/quickStyle1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml"/><Override PartName="/word/diagrams/colors1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram"><w:body><w:p><w:r><w:drawing><wp:inline><wp:extent cx="5486400" cy="3200400"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds r:dm="rId4" r:lo="rId5" r:qs="rId6" r:cs="rId7"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramData" Target="diagrams/data1.xml"/><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramLayout" Target="diagrams/layout1.xml"/><Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramQuickStyle" Target="diagrams/quickStyle1.xml"/><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramColors" Target="diagrams/colors1.xml"/></Relationships>"#;
        let data = br#"<dsp:dataModel xmlns:dsp="urn:d"/>"#;
        let layout = br#"<dgm:layoutDef xmlns:dgm="urn:l"/>"#;
        let quick_style = br#"<dgm:styleDef xmlns:dgm="urn:s"/>"#;
        let colors = br#"<dgm:colorsDef xmlns:dgm="urn:c"/>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/diagrams/data1.xml", data),
            ("word/diagrams/layout1.xml", layout),
            ("word/diagrams/quickStyle1.xml", quick_style),
            ("word/diagrams/colors1.xml", colors),
        ]);

        let import = import_semantic(&source);
        let InlineNode::EmbeddedObject(object) = first_inline(&import.document) else {
            panic!("expected an embedded object");
        };
        assert_eq!(object.kind, EmbeddedKind::Diagram);
        assert_eq!(object.part.part_name, "word/diagrams/data1.xml");
        assert_eq!(object.extra_parts.len(), 3);
        // None of the four diagram parts is re-added as an orphan relationship.
        for part_name in ["data1", "layout1", "quickStyle1", "colors1"] {
            assert!(
                !import
                    .retained_parts
                    .relationships
                    .iter()
                    .any(|rel| rel.target.contains(part_name))
            );
        }

        let written = write_document_with_retained_parts(
            &import.document,
            &BTreeMap::new(),
            &import.retained_parts,
        )
        .unwrap();
        let mut reopened = DocxPackage::open(&written, PackageLimits::default()).unwrap();
        for part_name in [
            "word/diagrams/data1.xml",
            "word/diagrams/layout1.xml",
            "word/diagrams/quickStyle1.xml",
            "word/diagrams/colors1.xml",
        ] {
            assert!(reopened.read_part(part_name).is_ok(), "{part_name} present");
        }
        let doc_rels_out = reopened.read_part("word/_rels/document.xml.rels").unwrap();
        let doc_rels_out = String::from_utf8_lossy(&doc_rels_out);
        for (target, count) in [
            ("diagrams/data1.xml", 1),
            ("diagrams/layout1.xml", 1),
            ("diagrams/quickStyle1.xml", 1),
            ("diagrams/colors1.xml", 1),
        ] {
            // Each part's relationship is emitted exactly once (no double-emit).
            assert_eq!(doc_rels_out.matches(target).count(), count, "{target}");
        }

        let reimport = import_semantic(&written);
        let InlineNode::EmbeddedObject(object) = first_inline(&reimport.document) else {
            panic!("expected an embedded object after re-import");
        };
        assert_eq!(object.kind, EmbeddedKind::Diagram);
        assert_eq!(object.extra_parts.len(), 3);
    }

    #[test]
    fn ole_object_round_trips_with_progid_and_preview() {
        use casual_doc_model::v1::{EmbeddedKind, InlineNode};

        // A `w:object` with a `v:shape`/`v:imagedata` preview and an `o:OLEObject`
        // naming the embedding part and its `ProgID`.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="emf" ContentType="image/x-emf"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/embeddings/oleObject1.bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br##"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office"><w:body><w:p><w:r><w:object w:dxaOrig="1440" w:dyaOrig="720"><v:shape id="_x0000_i1025" type="#_x0000_t75" style="width:72pt;height:36pt"><v:imagedata r:id="rId6" o:title=""/></v:shape><o:OLEObject Type="Embed" ProgID="Excel.Sheet.12" ShapeID="_x0000_i1025" DrawAspect="Content" r:id="rId7"/></w:object></w:r></w:p></w:body></w:document>"##;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.emf"/><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject" Target="embeddings/oleObject1.bin"/></Relationships>"#;
        let preview = b"\x01\x00\x00\x00-fake-emf";
        let ole = b"\x00\x01OLE-BINARY-BLOB\x02\x03";
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/media/image1.emf", preview),
            ("word/embeddings/oleObject1.bin", ole),
        ]);

        let import = import_semantic(&source);
        let InlineNode::EmbeddedObject(object) = first_inline(&import.document) else {
            panic!("expected an embedded object");
        };
        assert_eq!(object.kind, EmbeddedKind::OleObject);
        assert_eq!(object.prog_id.as_deref(), Some("Excel.Sheet.12"));
        assert!(object.preview.is_some());
        assert_eq!(object.part.part_name, "word/embeddings/oleObject1.bin");
        // 1440 twips * 635 EMU/twip.
        assert_eq!(object.extent.width_emu, 914400);
        // The embedding is not re-added as an orphan relationship.
        assert!(
            !import
                .retained_parts
                .relationships
                .iter()
                .any(|rel| rel.target.contains("oleObject1.bin"))
        );

        let written = write_document_with_retained_parts(
            &import.document,
            &BTreeMap::from([("word/media/image1.emf".to_owned(), preview.to_vec())]),
            &import.retained_parts,
        )
        .unwrap();
        let mut reopened = DocxPackage::open(&written, PackageLimits::default()).unwrap();
        assert_eq!(
            reopened
                .read_part("word/embeddings/oleObject1.bin")
                .unwrap(),
            ole
        );
        let doc_rels_out = reopened.read_part("word/_rels/document.xml.rels").unwrap();
        let doc_rels_out = String::from_utf8_lossy(&doc_rels_out);
        assert!(doc_rels_out.contains("embeddings/oleObject1.bin"));
        assert!(doc_rels_out.contains("media/image1.emf"));
        assert_eq!(doc_rels_out.matches("oleObject1.bin").count(), 1);

        let reimport = import_semantic(&written);
        let InlineNode::EmbeddedObject(object) = first_inline(&reimport.document) else {
            panic!("expected an embedded object after re-import");
        };
        assert_eq!(object.kind, EmbeddedKind::OleObject);
        assert_eq!(object.prog_id.as_deref(), Some("Excel.Sheet.12"));
        assert!(object.preview.is_some());
    }

    #[test]
    fn digital_signature_is_not_preserved_and_is_reported_dropped() {
        // A signed package: editing regenerates the body, which invalidates any
        // signature, so signatures are deliberately NOT preserved on the semantic
        // path. They are dropped and reported `not-retained`.
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/_xmlsignatures/origin.sigs" ContentType="application/vnd.openxmlformats-package.digital-signature-origin"/><Override PartName="/_xmlsignatures/sig1.xml" ContentType="application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/origin" Target="_xmlsignatures/origin.sigs"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#;
        let origin = b"signature-origin";
        let origin_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/signature" Target="sig1.xml"/></Relationships>"#;
        let sig = br#"<Signature xmlns="http://www.w3.org/2000/09/xmldsig#"/>"#;
        let source = zip_named(&[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", root_rels),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", doc_rels),
            ("_xmlsignatures/origin.sigs", origin),
            ("_xmlsignatures/_rels/origin.sigs.rels", origin_rels),
            ("_xmlsignatures/sig1.xml", sig),
        ]);

        let import = import_semantic(&source);
        // No signature part is preserved via the side-table.
        assert!(import.retained_parts.is_empty());
        assert!(import.retained_parts.relationships.is_empty());
        // Both signature content parts are dispositioned `not-retained` (dropped).
        for part_name in ["_xmlsignatures/origin.sigs", "_xmlsignatures/sig1.xml"] {
            let entry = import
                .report
                .entries
                .iter()
                .find(|entry| {
                    entry
                        .part
                        .as_ref()
                        .is_some_and(|part| part.part_name == part_name)
                })
                .unwrap_or_else(|| panic!("{part_name} is dispositioned"));
            assert_eq!(
                entry.retention_outcome,
                casual_doc_import::RetentionOutcome::NotRetained,
                "{part_name} is reported dropped"
            );
        }

        // The written package opens and carries no signature parts.
        let written = write_document_with_retained_parts(
            &import.document,
            &BTreeMap::new(),
            &import.retained_parts,
        )
        .unwrap();
        let reopened = DocxPackage::open(&written, PackageLimits::default()).unwrap();
        assert!(
            !reopened
                .entries()
                .iter()
                .any(|entry| entry.part_name.starts_with("_xmlsignatures/")),
            "no signature part is emitted"
        );
    }

    #[test]
    fn write_document_without_side_table_is_unchanged() {
        // The no-side-table wrapper is byte-identical to writing with an empty
        // side-table (the additive path does not perturb the base writer).
        let source = customxml_package();
        let import = import_semantic(&source);
        let plain = write_document(&import.document, &BTreeMap::new()).unwrap();
        let empty = write_document_with_retained_parts(
            &import.document,
            &BTreeMap::new(),
            &casual_doc_import::RetainedParts::default(),
        )
        .unwrap();
        assert_eq!(plain, empty);
        // Without the side-table the customXml store is absent from the output.
        let reopened = DocxPackage::open(&plain, PackageLimits::default()).unwrap();
        assert!(
            !reopened
                .entries()
                .iter()
                .any(|entry| entry.part_name.starts_with("customXml/"))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use casual_doc_import::{ImportConfig, ImportMode, import_package};
    use casual_doc_model::v1::{BlockNode, Document, InlineNode, Table, VerticalMerge};
    use casual_doc_ooxml::{DocxPackage, PackageLimits};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    /// Whether any inline node (recursing into hyperlinks) matches `predicate`.
    fn any_inline(document: &Document, predicate: impl Fn(&InlineNode) -> bool + Copy) -> bool {
        fn walk(inline: &InlineNode, predicate: impl Fn(&InlineNode) -> bool + Copy) -> bool {
            if predicate(inline) {
                return true;
            }
            match inline {
                InlineNode::Hyperlink(link) => {
                    return link.inlines.iter().any(|child| walk(child, predicate));
                }
                InlineNode::Field(field) => {
                    return field.inlines.iter().any(|child| walk(child, predicate));
                }
                InlineNode::TextBox(text_box) => {
                    return walk_blocks(&text_box.blocks, predicate);
                }
                InlineNode::Sdt(sdt) => {
                    return sdt.inlines.iter().any(|child| walk(child, predicate));
                }
                _ => {}
            }
            false
        }
        fn walk_blocks(
            blocks: &[BlockNode],
            predicate: impl Fn(&InlineNode) -> bool + Copy,
        ) -> bool {
            blocks.iter().any(|block| match block {
                BlockNode::Paragraph(paragraph) => paragraph
                    .inlines
                    .iter()
                    .any(|inline| walk(inline, predicate)),
                BlockNode::Table(table) => table.rows.iter().any(|row| {
                    row.cells
                        .iter()
                        .any(|cell| walk_blocks(&cell.blocks, predicate))
                }),
                BlockNode::Sdt(sdt) => walk_blocks(&sdt.blocks, predicate),
            })
        }
        walk_blocks(document.body(), predicate)
    }

    fn sample_package() -> Vec<u8> {
        let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>round trip</w:t></w:r></w:p></w:body></w:document>"#;

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", rels.as_slice()),
            ("word/document.xml", document.as_slice()),
        ] {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn no_edit_round_trip_reproduces_the_model() {
        let original = sample_package();
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(&original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        // Reconstruct a DOCX from the retained parts and re-import it.
        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        // The reconstructed package imports to an identical model: round-trip.
        assert_eq!(first.document, second.document);
        // Part contents are byte-identical to the source.
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );
        // Writing is deterministic.
        assert_eq!(
            write_package(first.retained_source.as_ref().unwrap()).unwrap(),
            rebuilt
        );
    }

    fn first_table(document: &Document) -> &Table {
        document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Table(table) => Some(table),
                BlockNode::Paragraph(_) | BlockNode::Sdt(_) => None,
            })
            .expect("a table in the body")
    }

    #[test]
    fn table_with_merged_cells_round_trips() {
        // A table with a horizontal (gridSpan) and vertical (vMerge) merge. The
        // structure is now modeled AND the source round-trips byte-for-byte via
        // Retention, so the merge geometry survives import -> write -> reopen.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-table-merges.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );

        // The reopened model still carries the modeled merge geometry.
        let table = first_table(&second.document);
        assert_eq!(table.rows[0].cells[0].properties.grid_span, Some(2));
        assert_eq!(
            table.rows[1].cells[0].properties.vertical_merge,
            Some(VerticalMerge::Restart)
        );
        assert_eq!(
            table.rows[2].cells[0].properties.vertical_merge,
            Some(VerticalMerge::Continue)
        );
    }

    #[test]
    fn real_document_with_tables_and_lists_round_trips() {
        // A real LibreOffice .docx with tables, bullet/numbered lists, styles,
        // and numbering. Retention + reconstruction round-trips ALL of it —
        // every tag, nested element, and part — regardless of what the semantic
        // model captures yet.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-table-list.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        // Every retained part (document, styles, numbering, ...) is reproduced
        // byte-identically, so nothing is lost across the round trip.
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );
    }

    #[test]
    fn rich_document_with_nested_tables_and_image_round_trips() {
        // Nested tables, an embedded image (word/media/image1.png), and styles.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        // The embedded image relationship is mapped into the media table and
        // the picture is modeled as a first-class inline drawing.
        assert!(!first.document.definitions().media.is_empty());
        assert!(any_inline(&first.document, |inline| matches!(
            inline,
            InlineNode::Drawing(_)
        )));

        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        // The image binary and every other part round-trip byte-identically.
        let parts = &first.retained_source.as_ref().unwrap().parts;
        assert!(parts.contains_key("word/media/image1.png"));
        assert_eq!(&second.retained_source.as_ref().unwrap().parts, parts);
    }

    #[test]
    fn document_with_external_and_internal_hyperlinks_round_trips() {
        // External (`r:id` -> an External-mode relationship) and internal
        // (`w:anchor`) hyperlinks. They round-trip verbatim today; the semantic
        // model captures them as first-class inlines as modeling lands.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-hyperlinks.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        // The links are modeled as first-class inline hyperlinks.
        assert!(any_inline(&first.document, |inline| matches!(
            inline,
            InlineNode::Hyperlink(_)
        )));
        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );
    }

    #[test]
    fn document_with_header_and_footer_parts_round_trips() {
        // Separate word/header1.xml and word/footer1.xml parts referenced from
        // the section. Retention retains every part, so the header/footer parts
        // round-trip byte-identically even before they are modeled.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        let parts = &first.retained_source.as_ref().unwrap().parts;
        assert!(parts.contains_key("word/header1.xml"));
        assert!(parts.contains_key("word/footer1.xml"));

        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        assert_eq!(&second.retained_source.as_ref().unwrap().parts, parts);
    }

    #[test]
    fn document_with_footnotes_part_round_trips() {
        // A footnote reference in the body plus a separate word/footnotes.xml
        // part. Retention retains the part so it round-trips before modeling.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-footnotes.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        assert!(
            first
                .retained_source
                .as_ref()
                .unwrap()
                .parts
                .contains_key("word/footnotes.xml")
        );

        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );
    }

    #[test]
    fn semantic_mode_has_no_retained_parts() {
        let source = casual_doc_import::RetainedSource {
            main_document: Vec::new(),
            parts: std::collections::BTreeMap::new(),
        };
        assert_eq!(write_package(&source), Err(ExportError::NoRetainedParts));
    }
}
