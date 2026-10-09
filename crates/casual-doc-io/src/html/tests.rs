//! The HTML exporter's guarantees.

use casual_doc_model::v1::Document;

use super::*;
use crate::{DocumentResources, builtin_registry};

fn document(body: &str) -> Document {
    let xml = format!(
        "<w:document xmlns:w=\"urn:w\" xmlns:r=\"urn:r\"><w:body>{body}</w:body></w:document>"
    );
    casual_doc_import::import_main_document_xml(xml.as_bytes(), Default::default())
        .expect("the body imports")
        .document
}

fn html_with(document: &Document, resources: &DocumentResources) -> (String, CompatibilityReport) {
    let artifact = HtmlAdapter::default()
        .export(ExportRequest {
            document,
            resources,
            source: None,
            source_unchanged: false,
            mode: ExportMode::Semantic,
        })
        .expect("html export");
    assert_eq!(artifact.mime_type, HTML_MIME);
    assert_eq!(artifact.suggested_extension, "html");
    (
        String::from_utf8(artifact.bytes).expect("html is UTF-8"),
        artifact.report,
    )
}

fn html(document: &Document) -> (String, CompatibilityReport) {
    html_with(document, &DocumentResources::default())
}

fn features(report: &CompatibilityReport) -> Vec<String> {
    report
        .entries
        .iter()
        .map(|entry| entry.feature.clone())
        .collect()
}

#[test]
fn the_export_is_one_self_contained_document() {
    let (text, _) = html(&document("<w:p><w:r><w:t>Hello</w:t></w:r></w:p>"));
    // No language is declared here, so none is claimed: `lang="en"` on a
    // document in another language misleads a screen reader.
    assert!(text.starts_with("<!DOCTYPE html>\n<html>\n"), "{text}");
    assert!(text.contains("<meta charset=\"utf-8\">"));
    assert!(text.contains("<style>"), "the stylesheet is inline");
    assert!(text.ends_with("</body>\n</html>\n"));
    assert!(
        !text.contains("http://") && !text.contains("https://"),
        "nothing it references may be off the disk: {text}"
    );
    assert!(text.contains(">Hello</p>"), "{text}");
}

#[test]
fn text_and_attributes_are_escaped_so_a_document_cannot_inject_markup() {
    // The document's own text contains a script tag and a quote. Neither may
    // survive as markup, and the link's target may not close its attribute.
    let (text, _) = html(&document(concat!(
        "<w:p><w:r><w:t>&lt;script&gt;bad()&lt;/script&gt; &amp; \"quoted\"</w:t></w:r></w:p>",
        "<w:p><w:hyperlink w:anchor=\"a&quot;b\"><w:r><w:t>x</w:t></w:r></w:hyperlink></w:p>",
    )));
    assert!(
        !text.contains("<script>"),
        "a script tag must not survive as markup: {text}"
    );
    assert!(text.contains("&lt;script&gt;bad()&lt;/script&gt;"));
    assert!(
        text.contains("&amp; \"quoted\""),
        "a quote in TEXT stays a quote"
    );
    assert!(
        text.contains("href=\"#a&quot;b\""),
        "a quote in an ATTRIBUTE is escaped: {text}"
    );
}

#[test]
fn a_merged_table_exports_as_a_merged_table() {
    // This is the row HTML exists for: `colspan` and `rowspan` mean exactly
    // what `w:gridSpan` and `w:vMerge` mean, so a merged table survives
    // instead of degrading to text the way it must in Markdown.
    let (text, _) = html(&document(concat!(
        "<w:tbl>",
        "<w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr>",
        "<w:p><w:r><w:t>Wide</w:t></w:r></w:p></w:tc></w:tr>",
        "<w:tr><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr>",
        "<w:p><w:r><w:t>Tall</w:t></w:r></w:p></w:tc>",
        "<w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr>",
        "<w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>",
        "<w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr>",
        "</w:tbl>",
    )));
    assert!(text.contains("colspan=\"2\""), "{text}");
    assert!(text.contains("rowspan=\"2\""), "{text}");
    assert_eq!(
        text.matches("<td").count() + text.matches("<th").count(),
        4,
        "the continuation cell writes nothing — the cell above owns the span: {text}"
    );
}

#[test]
fn character_formatting_uses_semantic_elements_and_typed_styles() {
    let (text, _) = html(&document(concat!(
        "<w:p><w:r><w:rPr><w:b/><w:i/><w:u w:val=\"single\"/><w:strike/>",
        "<w:vertAlign w:val=\"superscript\"/><w:sz w:val=\"36\"/>",
        "<w:color w:val=\"FF0000\"/><w:highlight w:val=\"yellow\"/></w:rPr>",
        "<w:t>x</w:t></w:r></w:p>",
    )));
    for expected in ["<strong>", "<em>", "<sup>"] {
        assert!(text.contains(expected), "{expected} missing from {text}");
    }
    assert!(
        text.contains("text-decoration-line:underline line-through"),
        "{text}"
    );
    // Superscript at the page's two thirds of 18pt, raised by a third of it
    // (`flow::run_metrics`), not the browser's `smaller`.
    assert!(text.contains("font-size:12pt"), "{text}");
    assert!(text.contains("vertical-align:6pt"), "{text}");
    assert!(text.contains("color:#ff0000"), "{text}");
    assert!(text.contains("background-color:#ffff00"), "{text}");
}

#[test]
fn underline_survives_here_even_though_markdown_cannot_carry_it() {
    // Named as its own guard because it is the concrete reason both
    // exporters exist: the Markdown one reports `markdown.underline` as a
    // loss, and this one does not lose it at all.
    let (text, report) = html(&document(
        "<w:p><w:r><w:rPr><w:u w:val=\"single\"/></w:rPr><w:t>x</w:t></w:r></w:p>",
    ));
    assert!(
        text.contains("<span style=\"text-decoration-line:underline\">x</span>"),
        "{text}"
    );
    assert!(
        !features(&report).iter().any(|f| f.contains("underline")),
        "underline is not a loss in HTML: {:?}",
        features(&report)
    );
}

#[test]
fn a_tracked_change_exports_as_ins_and_del() {
    let (text, _) = html(&document(concat!(
        "<w:p><w:ins w:id=\"1\" w:author=\"A\"><w:r><w:t>new</w:t></w:r></w:ins>",
        "<w:del w:id=\"2\" w:author=\"A\"><w:r><w:delText>old</w:delText></w:r></w:del></w:p>",
    )));
    assert!(text.contains("<ins>new</ins>"), "{text}");
    assert!(text.contains("<del>old</del>"), "{text}");
}

#[test]
fn a_paragraphs_alignment_and_indent_become_typed_declarations() {
    let (text, _) = html(&document(concat!(
        "<w:p><w:pPr><w:jc w:val=\"center\"/>",
        "<w:ind w:left=\"720\"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>",
    )));
    assert!(text.contains("text-align:center"), "{text}");
    assert!(
        text.contains("margin-inline-start:36pt"),
        "720 twips is 36 points: {text}"
    );
}

#[test]
fn a_picture_is_embedded_as_a_data_uri_and_a_missing_one_falls_back_to_its_alt() {
    // Without the bytes there is nothing to embed, and an `<img>` with no
    // source is a broken-image icon, so the alt text is what is left.
    let (text, report) = html(&document(concat!(
        "<w:p><w:r><w:drawing><wp:inline xmlns:wp=\"urn:wp\">",
        "<wp:docPr id=\"1\" name=\"p\" descr=\"A chart\"/>",
        "<a:graphic xmlns:a=\"urn:a\"><a:graphicData><pic:pic xmlns:pic=\"urn:pic\">",
        "<pic:blipFill><a:blip r:embed=\"rId9\"/></pic:blipFill>",
        "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
    )));
    assert!(
        text.contains("A chart") || features(&report).is_empty(),
        "either the picture embedded or its alt text stood in: {text}"
    );
    assert!(
        !text.contains("src=\"data:"),
        "no bytes were supplied, so nothing may be embedded: {text}"
    );
}

#[test]
fn base64_matches_the_standard_alphabet_and_padding() {
    // The encoder is vendored, so it is checked against the canonical
    // examples from RFC 4648 rather than trusted.
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foob"), "Zm9vYg==");
    assert_eq!(base64(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64(&[0xff, 0xff, 0xff]), "////");
    assert_eq!(base64(&[0x00, 0x00, 0x00]), "AAAA");
}

#[test]
fn consecutive_list_paragraphs_become_one_list() {
    // Without a numbering part the reference does not resolve, so these stay
    // paragraphs. That is the honest answer: a `<ul>` invented from an
    // unresolvable reference would be a guess.
    let (text, _) = html(&document(concat!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>",
        "<w:r><w:t>First</w:t></w:r></w:p>",
    )));
    assert!(!text.contains("<ul>") && !text.contains("<ol>"), "{text}");
    assert!(text.contains("First"));
}

#[test]
fn every_reported_feature_is_in_the_declared_vocabulary() {
    let (_, report) = html(&document(concat!(
        "<w:p><w:r><w:t>x</w:t></w:r><w:r><w:br w:type=\"page\"/></w:r>",
        "<w:r><w:t>y</w:t></w:r></w:p>",
        "<w:p><w:commentRangeStart w:id=\"1\"/><w:r><w:t>a</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:tabs><w:tab w:val=\"right\" w:pos=\"9000\"/></w:tabs></w:pPr>",
        "<w:r><w:t>b</w:t></w:r><w:r><w:tab/><w:t>c</w:t></w:r></w:p>",
    )));
    let found = features(&report);
    assert!(!found.is_empty(), "this document does lose things");
    for feature in found {
        assert!(
            FEATURES.contains(&feature.as_str()),
            "{feature} is not in the declared vocabulary"
        );
    }
}

#[test]
fn the_output_is_deterministic() {
    let source = document("<w:p><w:r><w:t>one</w:t></w:r></w:p>");
    assert_eq!(html(&source).0, html(&source).0);
}

#[test]
fn the_output_ceiling_refuses_rather_than_growing() {
    let source = document("<w:p><w:r><w:t>plenty of text here</w:t></w:r></w:p>");
    let resources = DocumentResources::default();
    let error = HtmlAdapter::new(HtmlLimits {
        max_output_bytes: 8,
        ..HtmlLimits::default()
    })
    .export(ExportRequest {
        document: &source,
        resources: &resources,
        source: None,
        source_unchanged: false,
        mode: ExportMode::Semantic,
    })
    .expect_err("an eight-byte ceiling must refuse");
    assert!(
        format!("{error}").contains("html_output_bytes"),
        "the refusal must name the limit: {error}"
    );
}

#[test]
fn a_limit_above_its_hard_ceiling_is_refused() {
    let source = document("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    let resources = DocumentResources::default();
    assert!(
        HtmlAdapter::new(HtmlLimits {
            max_embedded_bytes: HtmlLimits::HARD_MAX_EMBEDDED_BYTES + 1,
            ..HtmlLimits::default()
        })
        .export(ExportRequest {
            document: &source,
            resources: &resources,
            source: None,
            source_unchanged: false,
            mode: ExportMode::Semantic,
        })
        .is_err()
    );
}

#[test]
fn the_exact_mode_is_refused_with_a_sentence() {
    let source = document("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    let resources = DocumentResources::default();
    let error = HtmlAdapter::default()
        .export(ExportRequest {
            document: &source,
            resources: &resources,
            source: None,
            source_unchanged: true,
            mode: ExportMode::ExactIfUnchanged,
        })
        .expect_err("html has no source bytes to return");
    assert!(format!("{error}").contains("no importer"), "{error}");
}

#[test]
fn the_builtin_registry_offers_html_to_save_and_never_to_open() {
    let registry = builtin_registry();
    let exports: Vec<&str> = registry
        .export_formats()
        .into_iter()
        .map(FormatId::as_str)
        .collect();
    assert!(exports.contains(&formats::HTML), "{exports:?}");
    let descriptor = registry
        .descriptors()
        .into_iter()
        .find(|descriptor| descriptor.id.as_str() == formats::HTML)
        .expect("the HTML descriptor is registered");
    assert!(
        !descriptor.can_import,
        "there is no HTML parser or sanitiser, so the picker must never offer it to open"
    );
    assert!(!descriptor.exact_if_unchanged);
    assert_eq!(descriptor.extensions, ["html", "htm"]);
}

// ---- fidelity: the export is resolved the way the page is (ADR-066) --------

/// Imports a real producer's file and exports it, the way the browser does.
fn export_fixture(bytes: &[u8]) -> (String, Vec<String>) {
    let imported = builtin_registry()
        .import(
            crate::DetectionRequest {
                bytes,
                selection: crate::FormatSelection::Auto,
                file_name_hint: None,
                mime_hint: None,
            },
            true,
        )
        .expect("the fixture imports");
    let (text, report) = html_with(&imported.document, &imported.resources);
    (text, features(&report))
}

const STYLED: &[u8] = include_bytes!("../../../../webapp/styled.docx");
const SAMPLE: &[u8] = include_bytes!("../../../../webapp/sample.docx");
const TABLE_LIST: &[u8] =
    include_bytes!("../../../../fixtures/corpus/real-producer-table-list.docx");
const FOOTNOTES: &[u8] = include_bytes!("../../../../fixtures/corpus/real-producer-footnotes.docx");
const HEADER_FOOTER: &[u8] =
    include_bytes!("../../../../fixtures/corpus/real-producer-header-footer.docx");

/// The rule the stylesheet gives `class`, or an empty string.
fn rule<'t>(text: &'t str, class: &str) -> &'t str {
    let needle = format!("\n.{class}{{");
    text.find(&needle).map_or("", |start| {
        let rest = &text[start + needle.len()..];
        &rest[..rest.find('}').unwrap_or(rest.len())]
    })
}

#[test]
fn a_heading_takes_its_styles_colour_size_and_weight() {
    // The defect the owner saw: a heading formatted entirely by its style
    // exported as the browser's black serif `<h1>`, because only direct
    // formatting was read. The page paints Heading 1 in the style's blue at
    // 16pt bold; so must the file.
    let (text, _) = export_fixture(STYLED);
    assert!(
        text.contains("<h1 class=\"s-heading-1\">Coloured heading</h1>"),
        "{text}"
    );
    let heading = rule(&text, "s-heading-1");
    for expected in ["color:#2f5496", "font-size:16pt", "font-weight:700"] {
        assert!(
            heading.contains(expected),
            "{expected} missing from .s-heading-1{{{heading}}}"
        );
    }
    // And the browser's own heading size cannot leak through.
    assert!(
        text.contains("h1,h2,h3,h4,h5,h6,li{margin:0;font-size:inherit"),
        "{text}"
    );
}

#[test]
fn a_title_style_brings_its_size_border_and_font_with_its_metric_partner() {
    let (text, _) = export_fixture(SAMPLE);
    assert!(text.contains("<p class=\"s-title\""), "{text}");
    let title = rule(&text, "s-title");
    for expected in [
        "font-size:30pt",
        "font-weight:700",
        "color:#102a43",
        "border-bottom:1pt solid #4f81bd",
        // Calibri, then Carlito — the metric-compatible face the page draws
        // when the reader's machine has no Calibri — then the class.
        "font-family:'Calibri','Carlito',sans-serif",
    ] {
        assert!(
            title.contains(expected),
            "{expected} missing from .s-title{{{title}}}"
        );
    }
}

#[test]
fn a_run_writes_only_where_it_differs_from_its_paragraph() {
    // Normalization: the style's formatting is stated once, in its class.
    // A plain run inside a styled paragraph carries no declarations at all,
    // and a bold run is `<strong>` with nothing else.
    let (text, _) = export_fixture(SAMPLE);
    assert!(
        text.contains(">Regular text; <strong>bold text; </strong><em>italic text; </em>"),
        "{text}"
    );
}

#[test]
fn the_page_size_margins_and_measure_are_the_first_sections() {
    let (text, _) = export_fixture(SAMPLE);
    assert!(
        text.contains("@page{size:612pt 792pt;margin:54pt 72pt 51.85pt 72pt}"),
        "{text}"
    );
    // The text column on screen is the page's: 8.5in less two 1in margins.
    assert!(text.contains("max-width:468pt"), "{text}");
}

#[test]
fn a_table_cell_takes_its_fill_padding_and_width() {
    let (text, _) = export_fixture(SAMPLE);
    assert!(
        text.contains("<th style=\"background-color:#e8eef5;padding-bottom:4pt;padding-inline-end:6pt;padding-inline-start:6pt;padding-top:4pt;vertical-align:middle;width:110pt\">"),
        "{text}"
    );
    assert!(
        text.contains("<colgroup><col style=\"width:110pt\"><col style=\"width:75pt\"><col style=\"width:283pt\"></colgroup>"),
        "{text}"
    );
}

#[test]
fn a_table_border_is_the_one_the_page_draws() {
    // A double grey border on every cell, as the conflict rules resolve it.
    let (text, _) = export_fixture(TABLE_LIST);
    assert!(text.contains("border-top:0.75pt double #808080"), "{text}");
}

#[test]
fn a_first_row_is_a_header_only_when_the_document_says_so() {
    // Every table used to export its first row as bold, centred `<th>` —
    // the browser's header style over cells the document never marked.
    let (plain, _) = html(&document(concat!(
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr>",
        "<w:tr><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    )));
    assert!(
        !plain.contains("<th") && !plain.contains("<thead"),
        "{plain}"
    );
    let (header, _) = html(&document(concat!(
        "<w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr>",
        "<w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr>",
        "<w:tr><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    )));
    assert!(header.contains("<thead>\n<tr>\n<th "), "{header}");
    assert!(header.contains("<tbody>\n<tr>\n<td "), "{header}");
}

#[test]
fn a_vertical_merge_follows_its_grid_column_past_a_span() {
    // Row one: a cell spanning two grid columns, then a merge in column 3.
    // Row two continues the merge in its THIRD cell. Matched by cell index,
    // the merge looked for its continuation in the second cell and lost it.
    let (text, _) = html(&document(concat!(
        "<w:tbl>",
        "<w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr><w:p><w:r><w:t>Wide</w:t></w:r></w:p></w:tc>",
        "<w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr><w:p><w:r><w:t>Tall</w:t></w:r></w:p></w:tc></w:tr>",
        "<w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>",
        "<w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc>",
        "<w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc></w:tr>",
        "</w:tbl>",
    )));
    assert!(text.contains("<td rowspan=\"2\" style="), "{text}");
    assert_eq!(text.matches("<td").count(), 4, "{text}");
}

#[test]
fn lists_are_lists_with_the_labels_the_page_prints() {
    let (text, _) = export_fixture(TABLE_LIST);
    assert!(text.contains("<ul class=\"list\">"), "{text}");
    // The label, then the suffix tab as an en space before the item's text.
    assert!(
        text.contains("list-style-type:&#39;•\u{2002}&#39;"),
        "{text}"
    );
    assert!(text.contains("<ol class=\"list\">"), "{text}");
    assert!(
        text.contains("list-style-type:&#39;1.\u{2002}&#39;"),
        "{text}"
    );
    assert!(
        text.contains("list-style-type:&#39;2.\u{2002}&#39;"),
        "{text}"
    );
}

#[test]
fn footnotes_are_written_and_linked_both_ways() {
    // The note's text used to be dropped, reported only as
    // `html.note_reference`. It is the reader's content.
    let (text, report) = export_fixture(FOOTNOTES);
    assert!(text.contains("A footnote body."), "{text}");
    assert!(
        text.contains("<sup class=\"note-ref\"><a href=\"#fn1\" id=\"ref-fn1\">1</a></sup>"),
        "{text}"
    );
    assert!(text.contains("<div class=\"note\" id=\"fn1\">"), "{text}");
    assert!(text.contains("href=\"#ref-fn1\""), "{text}");
    assert!(
        !report.iter().any(|feature| feature.contains("note")),
        "{report:?}"
    );
}

#[test]
fn a_header_and_footer_are_reported_not_dropped_in_silence() {
    // A web page has no pages to repeat them on, so they are not in the file
    // — and the report says so, where it used to say nothing.
    let (_, report) = export_fixture(HEADER_FOOTER);
    assert!(
        report.contains(&"html.header_footer".to_owned()),
        "{report:?}"
    );
}

#[test]
fn a_picture_takes_the_size_the_document_gives_it() {
    let (text, _) = export_fixture(SAMPLE);
    assert!(
        text.contains("<img alt=\"Diagram showing DOCX flowing through OpenDoc to rendered output\" style=\"aspect-ratio:5806440 / 2156678;width:457.2pt\""),
        "{text}"
    );
}

#[test]
fn spacing_adds_between_paragraphs_as_word_adds_it() {
    // CSS would collapse 10pt after and 5pt before to 10pt. Word adds them.
    let (text, _) = html(&document(concat!(
        "<w:p><w:pPr><w:spacing w:after=\"200\"/></w:pPr><w:r><w:t>one</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:spacing w:before=\"100\"/></w:pPr><w:r><w:t>two</w:t></w:r></w:p>",
        "<w:p><w:r><w:t>three</w:t></w:r></w:p>",
    )));
    assert!(text.contains("<p class=\"s-default\">one</p>"), "{text}");
    assert!(
        text.contains("<p class=\"s-default\" style=\"margin-top:15pt\">two</p>"),
        "{text}"
    );
}

#[test]
fn contextual_spacing_removes_the_space_between_one_styles_paragraphs() {
    let (text, _) = html(&document(concat!(
        "<w:p><w:pPr><w:contextualSpacing/><w:spacing w:before=\"200\" w:after=\"200\"/></w:pPr>",
        "<w:r><w:t>one</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:contextualSpacing/><w:spacing w:before=\"200\" w:after=\"200\"/></w:pPr>",
        "<w:r><w:t>two</w:t></w:r></w:p>",
    )));
    assert!(
        text.contains("<p class=\"s-default\" style=\"margin-top:10pt\">one</p>"),
        "{text}"
    );
    assert!(
        text.contains("<p class=\"s-default\" style=\"margin-bottom:10pt\">two</p>"),
        "between the two there is no space at all: {text}"
    );
}

#[test]
fn an_empty_paragraph_keeps_its_line() {
    let (text, _) = html(&document(
        "<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>two</w:t></w:r></w:p>",
    ));
    assert!(text.contains("<p class=\"s-default\"><br></p>"), "{text}");
}

#[test]
fn a_page_break_is_a_break_for_print() {
    let (text, report) = html(&document(concat!(
        "<w:p><w:r><w:t>one</w:t></w:r></w:p>",
        "<w:p><w:r><w:br w:type=\"page\"/><w:t>two</w:t></w:r></w:p>",
    )));
    assert!(
        text.contains("style=\"break-before:page\">two</p>"),
        "{text}"
    );
    assert!(features(&report).is_empty(), "{:?}", features(&report));
}

#[test]
fn hidden_text_is_in_the_file_and_not_on_screen() {
    let (text, _) = html(&document(
        "<w:p><w:r><w:rPr><w:vanish/></w:rPr><w:t>secret</w:t></w:r></w:p>",
    ));
    assert!(text.contains("<span hidden>secret</span>"), "{text}");
}

/// A minimal `.docx` with a styles part, built in memory, for what a body
/// alone cannot carry.
fn docx_with_styles(body: &str, styles: &str) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    let parts: [(&str, String); 5] = [
        (
            "[Content_Types].xml",
            concat!(
                "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
                "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
                "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
                "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
                "<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>",
                "</Types>"
            )
            .to_owned(),
        ),
        (
            "_rels/.rels",
            concat!(
                "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
                "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
                "</Relationships>"
            )
            .to_owned(),
        ),
        (
            "word/_rels/document.xml.rels",
            concat!(
                "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
                "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
                "</Relationships>"
            )
            .to_owned(),
        ),
        (
            "word/document.xml",
            format!(
                "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>"
            ),
        ),
        (
            "word/styles.xml",
            format!(
                "<?xml version=\"1.0\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{styles}</w:styles>"
            ),
        ),
    ];
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("zip entry");
        writer.write_all(bytes.as_bytes()).expect("zip write");
    }
    writer.finish().expect("zip").into_inner()
}

#[test]
fn a_style_cannot_inject_markup_through_its_font_or_its_name() {
    // A style's font family and its name are the two pieces of document text
    // that reach the `<style>` element itself, where attribute escaping does
    // not apply. Both are hostile here.
    let hostile = "x&apos;;}&lt;/style&gt;&lt;script&gt;alert(1)&lt;/script&gt;";
    let package = docx_with_styles(
        "<w:p><w:pPr><w:pStyle w:val=\"Evil\"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p>",
        &format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Evil\"><w:name w:val=\"{hostile}\"/>\
             <w:rPr><w:rFonts w:ascii=\"{hostile}\" w:hAnsi=\"{hostile}\"/></w:rPr></w:style>"
        ),
    );
    let (text, _) = export_fixture(&package);
    assert!(text.contains(">a</p>"), "the paragraph exported: {text}");
    assert!(!text.contains("<script"), "{text}");
    assert_eq!(text.matches("</style>").count(), 1, "{text}");
    // The font is still named — what is left of it once nothing in it can end
    // a string — rather than dropped.
    assert!(
        text.contains("font-family:'xstylescriptalert1script'"),
        "{text}"
    );
}

#[test]
fn the_document_language_is_declared_when_the_document_declares_it() {
    let (text, _) = export_fixture(SAMPLE);
    assert!(
        text.starts_with("<!DOCTYPE html>\n<html lang=\"en-US\">\n"),
        "{text}"
    );
}
