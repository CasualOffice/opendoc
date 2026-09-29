//! `CT_OnOff` toggles cancelled with `w:val="0"` — asserted on the PAINTED page.
//!
//! # The defect family this guards
//!
//! ECMA-376 §17.17.4 gives every `CT_OnOff` element three states, not two: the
//! element absent means "inherit whatever the style chain said", a bare element
//! means on, and `w:val="0"`/`"false"`/`"off"` means **off here**, cancelling an
//! inherited value. `w:keepNext`, `w:keepLines`, `w:pageBreakBefore`,
//! `w:contextualSpacing` and `w:suppressLineNumbers` were plain `bool`s in the
//! model, which collapses absent and off into the same value — so the cascade
//! could only ever OR a flag ON and the cancellation was unrepresentable. This
//! is `w:numId="0"` again (see `numbering_cancelled_by_style.rs`): a
//! cancellation read as silence.
//!
//! The instance that made it worth fixing is `w:contextualSpacing`. Word's
//! built-in `ListParagraph` style sets it, and a document that wants real gaps
//! between list items cancels it per paragraph. The owner's NDA does exactly
//! that **51 times**, each beside a `w:spacing w:after="160"` — so 51 paragraph
//! gaps of 8pt were being suppressed by an inherited flag the document had
//! explicitly turned off.
//!
//! # Why this asserts the paint
//!
//! A model-level assertion could not see any of this. The paragraph's own
//! `contextual_spacing` read `false` both when the document cancelled the flag
//! and when it said nothing, and the difference only becomes visible once the
//! cascade has run and the page is composed. So these read glyph-run origins and
//! per-page glyph counts off the composed display list — where the glyph is,
//! and which page it is on.
//!
//! Every guard here is paired with a control that pins the *un*-cancelled
//! behaviour, so a fix that over-reaches (dropping the inherited flag entirely
//! rather than only where it is cancelled) fails just as loudly.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;
use std::io::Cursor;
use std::io::Write;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

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
</Relationships>"#;

/// Four styles, each turning ON one toggle a paragraph below then cancels.
///
/// Every style pins an **exact** line rule so the fixtures are independent of
/// the font actually resolved on the machine running the test: `w:lineRule`
/// `exact` with `w:line="240"` makes one line exactly 12pt tall whatever face is
/// substituted, which is what lets the page-fitting guards below count lines.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/>
<w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr>
</w:style>
<w:style w:type="paragraph" w:styleId="ListPara"><w:name w:val="List Paragraph"/>
<w:pPr><w:spacing w:before="0" w:after="160" w:line="240" w:lineRule="exact"/><w:contextualSpacing/></w:pPr>
</w:style>
<w:style w:type="paragraph" w:styleId="KeepPara"><w:name w:val="Keep"/>
<w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:keepNext/></w:pPr>
</w:style>
<w:style w:type="paragraph" w:styleId="KeepLinesPara"><w:name w:val="Keep Lines"/>
<w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:keepLines/><w:widowControl w:val="0"/></w:pPr>
</w:style>
<w:style w:type="paragraph" w:styleId="BreakPara"><w:name w:val="Break"/>
<w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:pageBreakBefore/></w:pPr>
</w:style>
</w:styles>"#;

/// A section exactly three 12pt lines tall with no margins, so "which page is
/// this line on" is decided by arithmetic rather than by font metrics.
const THREE_LINE_SECTION: &str = r#"<w:sectPr><w:pgSz w:w="12240" w:h="720"/><w:pgMar w:top="0" w:right="0" w:bottom="0" w:left="0" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr>"#;

/// A full-page section, for the guards that measure a gap rather than a break.
const TALL_SECTION: &str = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="0" w:right="0" w:bottom="0" w:left="0" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr>"#;

/// Zips the four parts into a `.docx` in memory.
fn package(document: &str) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/document.xml", document),
        ("word/styles.xml", STYLES),
    ] {
        zip.start_file(name, options).expect("a fresh zip entry");
        zip.write_all(body.as_bytes()).expect("writing a zip entry");
    }
    zip.finish().expect("finishing the zip");
    buffer.into_inner()
}

/// Wraps `body` in a `w:document`, terminated by `section`.
fn document_xml(body: &str, section: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}{section}</w:body></w:document>"#
    )
}

/// One paragraph: `style`, an optional extra `w:pPr` fragment (the cancellation
/// under test), and `text`.
fn para(style: &str, extra: &str, text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{style}"/>{extra}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
    )
}

/// The baseline y of every non-marker glyph run painted on every page, in page
/// order then paint order.
///
/// Complexity: linear in the document's paint items; the fixtures are a handful
/// of paragraphs.
fn painted_baselines(document: &str) -> Vec<Vec<i32>> {
    let bytes = package(document);
    let mut docx = DocxPackage::open(&bytes, PackageLimits::default()).expect("a valid package");
    let imported = import_package(
        &mut docx,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the fixture imports");
    let shaper = ParleyShaper::new();
    let pages = paginate_document(&imported.document, &shaper);
    pages
        .pages
        .iter()
        .map(|page| {
            compose_page(page)
                .items
                .iter()
                .filter_map(|item| match item {
                    PaintItem::Glyphs { run } if !run.is_marker => Some(run.origin.y.raw()),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

/// How many glyph runs are painted on each page.
fn painted_per_page(document: &str) -> Vec<usize> {
    painted_baselines(document)
        .iter()
        .map(Vec::len)
        .collect::<Vec<_>>()
}

// ---------------------------------------------------------------------------
// w:contextualSpacing — the instance found on the owner's NDA
// ---------------------------------------------------------------------------

/// The gap between the two `ListParagraph` baselines, given the `w:pPr`
/// fragment each paragraph carries.
fn list_gap(extra: &str) -> i32 {
    let body = format!(
        "{}{}",
        para("ListPara", extra, "First item"),
        para("ListPara", extra, "Second item")
    );
    let pages = painted_baselines(&document_xml(&body, TALL_SECTION));
    let first = pages.first().expect("one page");
    assert_eq!(first.len(), 2, "both list items must paint");
    first[1] - first[0]
}

/// The control: with the style's `w:contextualSpacing` left alone, two adjacent
/// same-style paragraphs must still be collapsed together. A fix that simply
/// stopped honouring the style's flag would pass the guard below and fail here.
#[test]
fn adjacent_list_paragraphs_collapse_when_the_style_flag_stands() {
    let gap = list_gap("");
    assert_eq!(
        gap, 240,
        "w:contextualSpacing from the style must suppress the 160-twip \
         w:after between two adjacent ListParagraph paragraphs, leaving only \
         the exact 240-twip line; got {gap}"
    );
}

/// The guard. Each paragraph cancels the inherited `w:contextualSpacing` with
/// `w:val="0"`, so the style's `w:after="160"` must be painted.
#[test]
fn cancelling_contextual_spacing_restores_the_gap_between_list_items() {
    let gap = list_gap(r#"<w:contextualSpacing w:val="0"/>"#);
    assert_eq!(
        gap, 400,
        "a paragraph that cancels w:contextualSpacing (ECMA-376 §17.17.4) must \
         get its w:after back: 240 twips of exact line plus 160 of spacing. A \
         gap of 240 here is the inherited flag surviving a cancellation the \
         cascade read as silence — the shape that cost the owner's NDA 8pt in \
         51 places; got {gap}"
    );
}

// ---------------------------------------------------------------------------
// w:pageBreakBefore
// ---------------------------------------------------------------------------

/// The control: the style's `w:pageBreakBefore` still starts a new page.
#[test]
fn a_style_page_break_before_still_breaks() {
    let body = format!(
        "{}{}",
        para("Normal", "", "Before"),
        para("BreakPara", "", "After")
    );
    assert_eq!(
        painted_per_page(&document_xml(&body, TALL_SECTION)),
        vec![1, 1],
        "a style that declares w:pageBreakBefore must still break the page"
    );
}

/// The guard: the paragraph cancels it, so both paragraphs stay on one page.
#[test]
fn cancelling_page_break_before_keeps_the_paragraph_on_the_same_page() {
    let body = format!(
        "{}{}",
        para("Normal", "", "Before"),
        para("BreakPara", r#"<w:pageBreakBefore w:val="0"/>"#, "After")
    );
    assert_eq!(
        painted_per_page(&document_xml(&body, TALL_SECTION)),
        vec![2],
        "w:pageBreakBefore w:val=\"0\" cancels the break the style contributes; \
         a second page here is a cancellation that never reached the paginator"
    );
}

// ---------------------------------------------------------------------------
// w:keepNext
// ---------------------------------------------------------------------------

/// Four one-line paragraphs in a three-line section. The third carries `style`
/// plus `extra`; the question is whether it is pushed onto page 2.
fn four_lines(style: &str, extra: &str) -> Vec<usize> {
    let body = format!(
        "{}{}{}{}",
        para("Normal", "", "One"),
        para("Normal", "", "Two"),
        para(style, extra, "Three"),
        para("Normal", "", "Four")
    );
    painted_per_page(&document_xml(&body, THREE_LINE_SECTION))
}

/// The control: a `w:keepNext` style pulls its paragraph onto the next page so
/// it is not separated from the paragraph that follows.
#[test]
fn a_style_keep_next_pulls_the_paragraph_to_the_next_page() {
    assert_eq!(
        four_lines("KeepPara", ""),
        vec![2, 2],
        "the third paragraph must move to page 2 to stay with the fourth"
    );
}

/// The guard: the paragraph cancels `w:keepNext`, so it stays on page 1 and
/// fills the section's third line.
#[test]
fn cancelling_keep_next_lets_the_paragraph_stay_on_its_page() {
    assert_eq!(
        four_lines("KeepPara", r#"<w:keepNext w:val="0"/>"#),
        vec![3, 1],
        "w:keepNext w:val=\"0\" cancels the style's keep-with-next; a 2/2 split \
         here is the inherited flag OR-ed back on"
    );
}

// ---------------------------------------------------------------------------
// w:keepLines
// ---------------------------------------------------------------------------

/// Two one-line paragraphs then a two-line paragraph (split by an explicit
/// `w:br`, so the second line does not depend on measuring a wrap) in a
/// three-line section.
fn two_then_split(extra: &str) -> Vec<usize> {
    let split = format!(
        r#"<w:p><w:pPr><w:pStyle w:val="KeepLinesPara"/>{extra}</w:pPr><w:r><w:t>Three</w:t><w:br/><w:t>Four</w:t></w:r></w:p>"#
    );
    let body = format!(
        "{}{}{split}",
        para("Normal", "", "One"),
        para("Normal", "", "Two"),
    );
    painted_per_page(&document_xml(&body, THREE_LINE_SECTION))
}

/// The control: `w:keepLines` from the style keeps both of the last
/// paragraph's lines together, so the whole paragraph moves to page 2.
#[test]
fn a_style_keep_lines_moves_the_whole_paragraph() {
    assert_eq!(
        two_then_split(""),
        vec![2, 2],
        "a w:keepLines paragraph must not be split across the page boundary"
    );
}

/// The guard: cancelled, the paragraph is allowed to split again.
#[test]
fn cancelling_keep_lines_lets_the_paragraph_split_again() {
    assert_eq!(
        two_then_split(r#"<w:keepLines w:val="0"/>"#),
        vec![3, 1],
        "w:keepLines w:val=\"0\" cancels the style's keep-lines-together; a 2/2 \
         split here is the inherited flag surviving the cancellation"
    );
}
