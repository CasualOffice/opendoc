//! HTML file import: the paste path's mapping, sanitised and bounded.

use casual_doc_model::v1::{BlockNode, Document, InlineNode};

use crate::{
    CompatibilityReport, DetectionRequest, FormatImporter, FormatSelection, HtmlAdapter,
    HtmlLimits, ImportRequest, MarkdownAdapter, builtin_registry, formats,
};

fn import(html: &str) -> (Document, CompatibilityReport) {
    let artifact = HtmlAdapter::default()
        .import(ImportRequest {
            bytes: html.as_bytes(),
            retain_source: false,
        })
        .expect("html imports");
    (artifact.document, artifact.report)
}

fn features(report: &CompatibilityReport) -> Vec<&str> {
    report.entries.iter().map(|e| e.feature.as_str()).collect()
}

fn plain(document: &Document) -> String {
    let mut out = String::new();
    for block in document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
            out.push_str(&casual_doc_model::v1::node_plain_text(&paragraph.inlines));
            out.push('\n');
        }
    }
    out
}

fn style_names(document: &Document) -> Vec<String> {
    document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(
                paragraph
                    .properties
                    .style_ref
                    .and_then(|id| document.definitions().styles.get(&id))
                    .and_then(|style| style.name.clone())
                    .unwrap_or_default(),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn scripts_styles_and_head_never_reach_the_document() {
    let (document, report) = import(concat!(
        "<!doctype html><html><head><title>T</title><style>p{color:red}</style>",
        "<script>document.write('<p>evil</p>')</script></head>",
        "<body onload=\"alert(1)\"><p>kept</p>",
        "<script>if (a < b && \"</p>\") {}</script><p>also</p></body></html>",
    ));
    let text = plain(&document);
    assert_eq!(text, "kept\nalso\n");
    assert!(features(&report).contains(&"html.import.skipped.script"));
}

#[test]
fn headings_lists_quotes_code_and_rules_take_the_shared_mapping() {
    let (document, _) = import(concat!(
        "<h1>Title</h1><h3>Sub</h3>",
        "<ul><li>one<li>two<ol><li>inner</ol></ul>",
        "<blockquote>quoted</blockquote>",
        "<pre>line 1\n  line 2</pre><hr><p>after</p>",
    ));
    assert_eq!(
        style_names(&document),
        [
            "Heading 1",
            "Heading 3",
            "",
            "",
            "",
            "Quote",
            "HTML Preformatted",
            "HTML Preformatted",
            "",
            ""
        ]
    );
    let numbered: Vec<Option<u8>> = document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                Some(paragraph.properties.numbering.map(|n| n.level))
            }
            _ => None,
        })
        .collect();
    assert_eq!(&numbered[2..5], &[Some(0), Some(0), Some(1)]);
    assert!(
        plain(&document).contains("  line 2"),
        "pre keeps its spaces"
    );
}

#[test]
fn inline_tags_and_inline_styles_set_run_properties() {
    let (document, _) = import(concat!(
        "<p><b>b</b> <i>i</i> <u>u</u> <s>s</s> ",
        "<span style=\"font-weight:700;font-style:italic\">both</span></p>",
    ));
    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!()
    };
    let run = |text: &str| {
        paragraph
            .inlines
            .iter()
            .find_map(|inline| match inline {
                InlineNode::Run(run) if run.text == text => Some(run.properties.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no run {text}: {:?}", paragraph.inlines))
    };
    assert_eq!(run("b").bold, Some(true));
    assert_eq!(run("i").italic, Some(true));
    assert_eq!(run("u").underline, Some(true));
    assert_eq!(run("s").strike, Some(true));
    let both = run("both");
    assert_eq!((both.bold, both.italic), (Some(true), Some(true)));
}

#[test]
fn links_keep_safe_schemes_only_and_remote_pictures_are_never_fetched() {
    let (document, report) = import(concat!(
        "<p><a href=\"https://example.com\">ok</a> <a href=\"javascript:alert(1)\">bad</a> ",
        "<img src=\"https://example.com/x.png\" alt=\"far\"></p>",
    ));
    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!()
    };
    let links = paragraph
        .inlines
        .iter()
        .filter(|inline| matches!(inline, InlineNode::Hyperlink(_)))
        .count();
    assert_eq!(
        links, 2,
        "the safe link and the picture's link: {:?}",
        paragraph.inlines
    );
    assert!(plain(&document).contains("bad"));
    let found = features(&report);
    assert!(
        found.contains(&"html.import.link_unsafe_scheme"),
        "{found:?}"
    );
    assert!(found.contains(&"html.import.image_remote"), "{found:?}");
}

#[test]
fn a_table_has_its_header_row_alignment_and_column_span() {
    let (document, _) = import(concat!(
        "<table><tr><th>a<th align=right>b</tr>",
        "<tr><td colspan=2 style=\"text-align:center\">wide</td></tr></table>",
    ));
    let BlockNode::Table(table) = &document.body()[0] else {
        panic!("{:?}", document.body())
    };
    assert_eq!(table.rows.len(), 2);
    assert!(table.rows[0].properties.header);
    assert!(!table.rows[1].properties.header);
    assert_eq!(table.grid.len(), 2);
    assert_eq!(table.rows[1].cells[0].properties.grid_span, Some(2));
}

#[test]
fn entities_and_whitespace_read_as_a_browser_reads_them() {
    let (document, _) = import("<p>  a&amp;b &lt;c&gt;\n   d&#x41;&nbsp;e  </p>");
    assert_eq!(plain(&document), "a&b <c> dA\u{a0}e\n");
}

#[test]
fn nesting_and_size_are_bounded() {
    let deep = "<div>".repeat(600);
    let error = HtmlAdapter::default()
        .import(ImportRequest {
            bytes: deep.as_bytes(),
            retain_source: false,
        })
        .expect_err("600 open divs is past the default 512");
    assert!(
        error.to_string().contains("html_input_nesting_depth"),
        "{error}"
    );
    let error = HtmlAdapter::new(HtmlLimits {
        max_input_bytes: 8,
        ..HtmlLimits::default()
    })
    .import(ImportRequest {
        bytes: b"<p>plenty</p>",
        retain_source: false,
    })
    .expect_err("over the input limit");
    assert!(error.to_string().contains("html_input_bytes"), "{error}");
}

#[test]
fn a_non_utf8_page_is_read_as_windows_1252_and_says_so() {
    let artifact = HtmlAdapter::default()
        .import(ImportRequest {
            bytes: b"<p>caf\xe9 \x93q\x94</p>",
            retain_source: false,
        })
        .unwrap();
    assert_eq!(plain(&artifact.document), "caf\u{e9} \u{201c}q\u{201d}\n");
    assert!(features(&artifact.report).contains(&"html.import.decoded_as_windows_1252"));
}

#[test]
fn the_name_selects_html() {
    let registry = builtin_registry();
    for (name, expected) in [
        ("page.html", formats::HTML),
        ("page.htm", formats::HTML),
        ("page.txt", formats::TEXT),
    ] {
        let detected = registry
            .detect(DetectionRequest {
                bytes: b"<p>x</p>",
                selection: FormatSelection::Auto,
                file_name_hint: Some(name),
                mime_hint: None,
            })
            .unwrap();
        assert_eq!(detected.as_str(), expected, "{name}");
    }
}

/// The same document written in Markdown and in HTML opens as the same
/// structure: one builder, one mapping.
#[test]
fn html_and_markdown_spelling_one_document_open_alike() {
    let html = HtmlAdapter::default()
        .import(ImportRequest {
            bytes: b"<h2>Head</h2><ul><li>a</li><li>b</li></ul><blockquote><p>q</p></blockquote>",
            retain_source: false,
        })
        .unwrap()
        .document;
    let markdown = MarkdownAdapter::default()
        .import(ImportRequest {
            bytes: b"## Head\n\n- a\n- b\n\n> q\n",
            retain_source: false,
        })
        .unwrap()
        .document;
    assert_eq!(style_names(&html), style_names(&markdown));
    assert_eq!(plain(&html), plain(&markdown));
}

/// Our own single-file HTML export of a real producer's document opens back
/// with its structure: the tables and the pictures (embedded as `data:` URIs)
/// survive the trip, and the text does.
#[test]
fn our_own_html_export_of_a_real_document_opens_back_with_its_structure() {
    use crate::{ExportMode, ExportRequest, FormatExporter};
    const SAMPLE: &[u8] = include_bytes!("../../../../sample.docx");
    let registry = builtin_registry();
    let imported = registry
        .import(
            DetectionRequest {
                bytes: SAMPLE,
                selection: FormatSelection::Auto,
                file_name_hint: Some("sample.docx"),
                mime_hint: None,
            },
            false,
        )
        .expect("the sample imports");
    let html = HtmlAdapter::default()
        .export(ExportRequest {
            document: &imported.document,
            resources: &imported.resources,
            source: None,
            source_unchanged: false,
            mode: ExportMode::Semantic,
        })
        .expect("html export");
    let back = HtmlAdapter::default()
        .import(ImportRequest {
            bytes: &html.bytes,
            retain_source: false,
        })
        .expect("our own HTML opens");
    let tables = |document: &Document| {
        document
            .body()
            .iter()
            .filter(|block| matches!(block, BlockNode::Table(_)))
            .count()
    };
    assert_eq!(tables(&back.document), tables(&imported.document));
    // At least as many: a picture the document shows twice is embedded twice
    // in a single-file page, and reads back as two.
    let pictures = imported
        .resources
        .as_map()
        .keys()
        .filter(|name| name.contains("media/"))
        .count();
    assert!(
        back.resources.as_map().len() >= pictures && pictures > 0,
        "every embedded picture comes back as a picture: {pictures} -> {}",
        back.resources.as_map().len()
    );
    let words = |document: &Document| {
        let mut text = String::new();
        for block in document.body() {
            if let BlockNode::Paragraph(paragraph) = block {
                text.push_str(&casual_doc_model::v1::node_plain_text(&paragraph.inlines));
                text.push(' ');
            }
        }
        text.split_whitespace().count()
    };
    let (original, reread) = (words(&imported.document), words(&back.document));
    assert!(
        reread * 10 >= original * 9,
        "body words: {original} exported, {reread} read back"
    );
}
