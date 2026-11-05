// SPDX-License-Identifier: Apache-2.0

//! `pagebreaks`: our page starts against Word's own (`docs/94` §H2d).
//!
//! Word records, in every file it saves, where its own pagination last broke
//! the pages: a `w:lastRenderedPageBreak` marker at the start of each page's
//! first run (ECMA-376 §17.3.3.13). No Word reference is committed to this
//! repository (`109` FID-P-02, Q2 undecided), but every Word-saved document a
//! user has already carries one. This mode reads those markers and compares
//! them, page by page, with where our paginator starts each page.
//!
//! # Coordinates
//!
//! Both sides are reduced to the same pair: the paragraph's index in document
//! order, counting every body paragraph including those in table cells and
//! content controls (but not text-box content, which is not in the page flow),
//! and the character offset of the page start inside that paragraph. Agreement
//! is decided on the paragraph alone; the offsets are printed so a page that
//! starts on the right paragraph but a different line can still be seen.
//!
//! # What it refuses
//!
//! A marker is only Word's pagination if Word wrote it. A file whose
//! `docProps/app.xml` names another producer (python-docx copies its template's
//! markers verbatim, LibreOffice writes none) is refused unless
//! `--any-producer` is given.

use std::collections::HashMap;
use std::error::Error;

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::model::ModelPos;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use quick_xml::Reader;
use quick_xml::events::Event;

/// A page start: the paragraph's document-order index and the character offset
/// inside it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageStart {
    /// 0-based index among the document's flow paragraphs.
    pub paragraph: usize,
    /// Characters before the page start within that paragraph.
    pub offset: usize,
}

/// What `pagebreaks` measured for one document.
#[derive(Debug)]
pub struct Comparison {
    /// The application `docProps/app.xml` names, if any.
    pub producer: Option<String>,
    /// Word's page starts after the first page, in order.
    pub word: Vec<PageStart>,
    /// Ours after the first page, in order.
    pub ours: Vec<PageStart>,
    /// Flow paragraphs counted in `document.xml` and in our model; a mismatch
    /// means the two coordinate systems cannot be trusted to line up.
    pub paragraphs: (usize, usize),
    /// Our page count and Word's own (`docProps/app.xml` `Pages`).
    pub pages: (usize, Option<usize>),
}

impl Comparison {
    /// How many of Word's page starts we reproduce on the same paragraph, and
    /// the first (1-based page, Word, ours) that we do not.
    #[must_use]
    pub fn agreement(&self) -> (usize, Option<(usize, PageStart, Option<PageStart>)>) {
        let mut agreed = 0;
        let mut first = None;
        for (index, word) in self.word.iter().enumerate() {
            let ours = self.ours.get(index).copied();
            if ours.is_some_and(|ours| ours.paragraph == word.paragraph) {
                agreed += 1;
            } else if first.is_none() {
                first = Some((index + 2, *word, ours));
            }
        }
        (agreed, first)
    }
}

/// Whether `producer` is Microsoft Word.
#[must_use]
pub fn is_word(producer: Option<&str>) -> bool {
    producer.is_some_and(|name| name.contains("Microsoft Office Word") || name == "Microsoft Word")
}

/// Measures one `.docx`.
///
/// # Errors
/// The package does not open or import, or a part is not well-formed XML.
pub fn measure(docx: &[u8]) -> Result<Comparison, Box<dyn Error>> {
    let mut package = DocxPackage::open(docx, PackageLimits::default())?;
    let main = package.main_document_part().to_owned();
    let document_xml = package.read_part(&main)?;
    let app_xml = package.read_part("docProps/app.xml").ok();
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )?;
    let shaper = ParleyShaper::new();
    crate::register_package_fonts(&shaper, &imported.document, &mut package);
    let layout = paginate_document(&imported.document, &shaper);
    compare_parts(
        &document_xml,
        app_xml.as_deref(),
        &imported.document,
        &layout,
    )
}

/// The comparison over already-read parts and an already-paginated model.
///
/// # Errors
/// `document_xml` or `app_xml` is not well-formed XML.
pub fn compare_parts(
    document_xml: &[u8],
    app_xml: Option<&[u8]>,
    document: &Document,
    layout: &PaginatedLayout,
) -> Result<Comparison, Box<dyn Error>> {
    let (word, xml_paragraphs) = word_page_starts(document_xml)?;
    let (producer, word_pages) = match app_xml {
        Some(xml) => app_properties(xml)?,
        None => (None, None),
    };
    let index = paragraph_index(document);
    let ours = layout
        .pages
        .iter()
        .skip(1)
        .filter_map(|page| page.placed.first())
        .filter_map(|placed| first_position(&placed.fragment))
        .filter_map(|pos| our_start(document, &index, pos))
        .collect();
    Ok(Comparison {
        producer,
        word,
        ours,
        paragraphs: (xml_paragraphs, index.len()),
        pages: (layout.page_count(), word_pages),
    })
}

/// Every `w:lastRenderedPageBreak` in `document.xml` as a page start, and the
/// number of flow paragraphs counted. O(document size).
fn word_page_starts(xml: &[u8]) -> Result<(Vec<PageStart>, usize), Box<dyn Error>> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut starts = Vec::new();
    let mut paragraphs = 0_usize;
    let mut in_text_box = 0_usize;
    let mut in_text = false;
    let mut offset = 0_usize;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(element) => match element.local_name().as_ref() {
                b"txbxContent" => in_text_box += 1,
                b"p" if in_text_box == 0 => {
                    paragraphs += 1;
                    offset = 0;
                }
                b"t" if in_text_box == 0 => in_text = true,
                _ => {}
            },
            Event::End(element) => match element.local_name().as_ref() {
                b"txbxContent" => in_text_box = in_text_box.saturating_sub(1),
                b"t" => in_text = false,
                _ => {}
            },
            Event::Empty(element) if in_text_box == 0 => match element.local_name().as_ref() {
                b"p" => {
                    paragraphs += 1;
                    offset = 0;
                }
                b"lastRenderedPageBreak" if paragraphs > 0 => starts.push(PageStart {
                    paragraph: paragraphs - 1,
                    offset,
                }),
                b"tab" | b"br" | b"cr" | b"noBreakHyphen" => offset += 1,
                _ => {}
            },
            Event::Text(text) if in_text && in_text_box == 0 => {
                offset += String::from_utf8_lossy(text.as_ref()).chars().count();
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok((starts, paragraphs))
}

/// `docProps/app.xml`'s `Application` and `Pages`.
fn app_properties(xml: &[u8]) -> Result<(Option<String>, Option<usize>), Box<dyn Error>> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut current: Option<Vec<u8>> = None;
    let (mut application, mut pages) = (None, None);
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(element) => current = Some(element.local_name().as_ref().to_vec()),
            Event::End(_) => current = None,
            Event::Text(text) => {
                let value = String::from_utf8_lossy(text.as_ref()).trim().to_owned();
                match current.as_deref() {
                    Some(b"Application") => application = Some(value),
                    Some(b"Pages") => pages = value.parse().ok(),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok((application, pages))
}

/// Every flow paragraph's document-order index, walking tables and content
/// controls in the order `document.xml` lists them.
fn paragraph_index(document: &Document) -> HashMap<NodeId, usize> {
    fn walk(blocks: &[BlockNode], out: &mut HashMap<NodeId, usize>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    let next = out.len();
                    out.insert(paragraph.id, next);
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => walk(&sdt.blocks, out),
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    let mut out = HashMap::new();
    walk(document.body(), &mut out);
    out
}

/// The first model position a placed fragment shows: a paragraph's first line,
/// or a row's first cell's first block.
fn first_position(fragment: &BlockFragment) -> Option<ModelPos> {
    match fragment {
        BlockFragment::Paragraph { lines, .. } => lines.lines.first().map(|line| line.range.start),
        BlockFragment::TableRow { cells, .. } => cells
            .iter()
            .find_map(|cell| cell.blocks.first().and_then(first_position)),
    }
}

/// `pos` as a page start, its byte offset turned into characters of the
/// paragraph's run text.
fn our_start(
    document: &Document,
    index: &HashMap<NodeId, usize>,
    pos: ModelPos,
) -> Option<PageStart> {
    let paragraph = *index.get(&pos.node)?;
    let text = paragraph_text(document, pos.node).unwrap_or_default();
    let bytes = usize::try_from(pos.offset)
        .unwrap_or(usize::MAX)
        .min(text.len());
    let offset = text
        .char_indices()
        .take_while(|(at, _)| *at < bytes)
        .count();
    Some(PageStart { paragraph, offset })
}

/// The concatenated run text of paragraph `id`, wherever it sits.
fn paragraph_text(document: &Document, id: NodeId) -> Option<String> {
    fn find(blocks: &[BlockNode], id: NodeId) -> Option<String> {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) if paragraph.id == id => {
                    let mut text = String::new();
                    inline_text(&paragraph.inlines, &mut text);
                    return Some(text);
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if let Some(text) = find(&cell.blocks, id) {
                                return Some(text);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(text) = find(&sdt.blocks, id) {
                        return Some(text);
                    }
                }
                _ => {}
            }
        }
        None
    }
    find(document.body(), id)
}

/// The run text of `inlines`, hyperlinks included, as layout addresses it.
fn inline_text(inlines: &[InlineNode], out: &mut String) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Hyperlink(link) => inline_text(&link.inlines, out),
            _ => {}
        }
    }
}

/// The human report.
#[must_use]
pub fn report(name: &str, comparison: &Comparison) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let (agreed, first) = comparison.agreement();
    let _ = writeln!(
        out,
        "{name}: producer {:?}; pages ours {} / Word {}",
        comparison.producer.as_deref().unwrap_or("?"),
        comparison.pages.0,
        comparison
            .pages
            .1
            .map_or_else(|| "?".to_owned(), |pages| pages.to_string()),
    );
    let _ = writeln!(
        out,
        "  Word page starts recorded: {}; reproduced on the same paragraph: {agreed}",
        comparison.word.len()
    );
    if comparison.paragraphs.0 != comparison.paragraphs.1 {
        let _ = writeln!(
            out,
            "  WARNING paragraph counts differ (document.xml {}, model {}): positions may not line up",
            comparison.paragraphs.0, comparison.paragraphs.1
        );
    }
    if let Some((page, word, ours)) = first {
        let _ = writeln!(
            out,
            "  first divergence: page {page} starts at Word paragraph {} offset {}, ours {}",
            word.paragraph,
            word.offset,
            ours.map_or_else(
                || "has no such page".to_owned(),
                |ours| format!("paragraph {} offset {}", ours.paragraph, ours.offset)
            )
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;

    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    const ROOT_RELATIONSHIPS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    const APP: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Office Word</Application><Pages>2</Pages></Properties>"#;

    /// A four-paragraph document whose third paragraph starts page 2
    /// (`w:pageBreakBefore`), with Word's page-2 marker in paragraph `marker`
    /// (0-based).
    fn docx(marker: usize) -> Vec<u8> {
        let mut body = String::new();
        for index in 0..4 {
            let break_before = if index == 2 {
                "<w:pPr><w:pageBreakBefore/></w:pPr>"
            } else {
                ""
            };
            let rendered = if index == marker {
                "<w:lastRenderedPageBreak/>"
            } else {
                ""
            };
            body.push_str(&format!(
                "<w:p>{break_before}<w:r>{rendered}<w:t>Paragraph {index}</w:t></w:r></w:p>"
            ));
        }
        let document = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
        );
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("[Content_Types].xml", CONTENT_TYPES),
            ("_rels/.rels", ROOT_RELATIONSHIPS),
            ("word/document.xml", document.as_str()),
            ("docProps/app.xml", APP),
        ] {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(bytes.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn a_page_start_word_recorded_on_the_same_paragraph_agrees() {
        let comparison = measure(&docx(2)).expect("the synthetic package measures");
        assert!(is_word(comparison.producer.as_deref()));
        assert_eq!(comparison.pages, (2, Some(2)));
        assert_eq!(comparison.paragraphs, (4, 4));
        assert_eq!(
            comparison.word,
            vec![PageStart {
                paragraph: 2,
                offset: 0
            }]
        );
        assert_eq!(comparison.agreement(), (1, None), "100% agreement");
    }

    #[test]
    fn a_marker_one_paragraph_early_is_reported_as_the_first_divergence() {
        let comparison = measure(&docx(1)).expect("the synthetic package measures");
        let (agreed, first) = comparison.agreement();
        assert_eq!(agreed, 0);
        assert_eq!(
            first,
            Some((
                2,
                PageStart {
                    paragraph: 1,
                    offset: 0
                },
                Some(PageStart {
                    paragraph: 2,
                    offset: 0
                })
            )),
            "page 2: Word starts at paragraph 1, ours at paragraph 2"
        );
    }

    #[test]
    fn a_mid_paragraph_marker_records_its_character_offset() {
        let xml = format!(
            r#"<w:document xmlns:w="{W}"><w:body><w:p><w:r><w:t>abc</w:t><w:tab/><w:lastRenderedPageBreak/><w:t>def</w:t></w:r></w:p><w:p><w:txbxContent><w:p/></w:txbxContent></w:p></w:body></w:document>"#
        );
        let (starts, paragraphs) = word_page_starts(xml.as_bytes()).unwrap();
        assert_eq!(
            starts,
            vec![PageStart {
                paragraph: 0,
                offset: 4
            }]
        );
        assert_eq!(paragraphs, 2, "text-box paragraphs are not flow paragraphs");
    }

    #[test]
    fn only_word_counts_as_word() {
        assert!(is_word(Some("Microsoft Office Word")));
        assert!(!is_word(Some("python-docx")));
        assert!(!is_word(Some("LibreOffice/7.6")));
        assert!(!is_word(None));
    }
}
