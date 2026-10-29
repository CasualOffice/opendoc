//! Markdown import: the mapping, the bounds, the registry's selection, and
//! the MD -> model -> writer -> model fixed point.

use std::fmt::Write as _;

use casual_doc_model::v1::{
    BlockNode, BreakKind, Definitions, Document, HyperlinkTarget, InlineNode, NumberFormat,
    SdtControlData, VerticalAlignment,
};

use crate::{
    CompatibilityReport, DetectionRequest, DocumentResources, ExportMode, ExportRequest,
    FormatExporter, FormatImporter, FormatSelection, ImportRequest, MarkdownAdapter,
    MarkdownLimits, builtin_registry, formats,
};

fn import(markdown: &str) -> (Document, DocumentResources, CompatibilityReport) {
    let artifact = MarkdownAdapter::default()
        .import(ImportRequest {
            bytes: markdown.as_bytes(),
            retain_source: false,
        })
        .expect("markdown imports");
    (artifact.document, artifact.resources, artifact.report)
}

fn features(report: &CompatibilityReport) -> Vec<&str> {
    report.entries.iter().map(|e| e.feature.as_str()).collect()
}

fn export(document: &Document, resources: &DocumentResources) -> String {
    let artifact = MarkdownAdapter::default()
        .export(ExportRequest {
            document,
            resources,
            source: None,
            source_unchanged: false,
            mode: ExportMode::Semantic,
        })
        .expect("markdown exports");
    String::from_utf8(artifact.bytes).expect("utf-8")
}

/// What a reader of the document sees, as text: every block's shape and every
/// inline's meaning, with ids left out (they are minted from the source bytes,
/// so two different spellings of one document have different ids).
fn projection(document: &Document) -> String {
    let mut out = String::new();
    blocks(document.body(), document.definitions(), &mut out);
    for (_, note) in document.definitions().footnotes.iter() {
        out.push_str("NOTE{");
        blocks(&note.blocks, document.definitions(), &mut out);
        out.push('}');
    }
    out
}

fn blocks(blocks_in: &[BlockNode], definitions: &Definitions, out: &mut String) {
    for block in blocks_in {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let properties = &paragraph.properties;
                let style = properties
                    .style_ref
                    .and_then(|id| definitions.styles.get(&id))
                    .and_then(|style| style.name.clone())
                    .unwrap_or_default();
                let list = properties.numbering.map(|reference| {
                    let level = definitions.numbering_resolver().level(reference);
                    let ordered =
                        level.is_some_and(|level| level.num_fmt == Some(NumberFormat::Decimal));
                    format!("{}{}", if ordered { "ol" } else { "ul" }, reference.level)
                });
                let _ = write!(
                    out,
                    "P[{style}|{}|{:?}|{}](",
                    list.unwrap_or_default(),
                    properties.alignment,
                    properties.borders.bottom.is_some(),
                );
                inlines(&paragraph.inlines, definitions, out);
                out.push_str(")\n");
            }
            BlockNode::Table(table) => {
                out.push_str("T{");
                for row in &table.rows {
                    let _ = write!(out, "R{}:", row.properties.header);
                    for cell in &row.cells {
                        out.push_str("C{");
                        blocks(&cell.blocks, definitions, out);
                        out.push('}');
                    }
                }
                out.push_str("}\n");
            }
            BlockNode::Sdt(sdt) => blocks(&sdt.blocks, definitions, out),
            BlockNode::AltChunk(_) => out.push_str("ALT\n"),
        }
    }
}

fn inlines(nodes: &[InlineNode], definitions: &Definitions, out: &mut String) {
    for node in nodes {
        match node {
            InlineNode::Run(run) => {
                let p = &run.properties;
                let style = p
                    .style_ref
                    .and_then(|id| definitions.styles.get(&id))
                    .and_then(|style| style.name.clone())
                    .unwrap_or_default();
                let _ = write!(
                    out,
                    "<{}{}{}{}:{style}>{}",
                    if p.bold == Some(true) { "b" } else { "" },
                    if p.italic == Some(true) { "i" } else { "" },
                    if p.strike == Some(true) { "s" } else { "" },
                    if p.vertical_alignment == Some(VerticalAlignment::Superscript) {
                        "^"
                    } else {
                        ""
                    },
                    run.text
                );
            }
            InlineNode::Hyperlink(link) => {
                let target = match &link.target {
                    HyperlinkTarget::External(external) => external.url.clone(),
                    HyperlinkTarget::Internal(internal) => format!("#{}", internal.anchor),
                };
                let _ = write!(out, "[{target}:");
                inlines(&link.inlines, definitions, out);
                out.push(']');
            }
            InlineNode::Break(node) if node.kind == BreakKind::Line => out.push_str("<br>"),
            InlineNode::Tab(_) => out.push_str("<tab>"),
            InlineNode::Sdt(sdt) => match &sdt.properties.data {
                Some(SdtControlData::Checkbox(checkbox)) => {
                    let _ = write!(out, "<box {}>", checkbox.checked);
                }
                _ => inlines(&sdt.inlines, definitions, out),
            },
            InlineNode::NoteReference(_) => out.push_str("<noteref>"),
            InlineNode::NoteNumberMark(_) => out.push_str("<notemark>"),
            InlineNode::Drawing(drawing) => {
                let _ = write!(out, "<img {:?}>", drawing.descr);
            }
            other => {
                let _ = write!(out, "<{other:?}>");
            }
        }
    }
}

fn styles_of(document: &Document) -> Vec<String> {
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
fn atx_headings_take_heading_styles_with_their_outline_level() {
    let (document, _, _) = import("# One\n\n## Two\n\n###### Six\n\nBody\n");
    assert_eq!(
        styles_of(&document),
        ["Heading 1", "Heading 2", "Heading 6", ""]
    );
    let first = match &document.body()[0] {
        BlockNode::Paragraph(paragraph) => paragraph.properties.style_ref.unwrap(),
        _ => unreachable!(),
    };
    let style = document.definitions().styles.get(&first).unwrap();
    assert_eq!(
        style.paragraph.as_ref().unwrap().outline_level,
        Some(0),
        "the outline level is what makes it a heading to the navigation pane"
    );
}

#[test]
fn lists_are_real_numbering_at_their_levels() {
    let (document, _, _) = import("- a\n- b\n    1. c\n    2. d\n- e\n\n3. three\n");
    let text = projection(&document);
    assert!(text.contains("P[|ul0|None|false](<:>a)"), "{text}");
    assert!(text.contains("P[|ol1|None|false](<:>c)"), "{text}");
    assert!(text.contains("P[|ul0|None|false](<:>e)"), "{text}");
    // A list that starts at 3 starts at 3.
    let three = document
        .body()
        .iter()
        .rev()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => paragraph.properties.numbering,
            _ => None,
        })
        .unwrap();
    let level = document
        .definitions()
        .numbering_resolver()
        .level(three)
        .unwrap();
    assert_eq!(level.start, 3);
}

#[test]
fn task_items_are_checkbox_content_controls() {
    let (document, _, _) = import("- [ ] todo\n- [x] done\n");
    let text = projection(&document);
    assert!(text.contains("(<box false><:> todo)"), "{text}");
    assert!(text.contains("(<box true><:> done)"), "{text}");
}

#[test]
fn a_gfm_table_has_a_header_row_and_column_alignment() {
    let (document, _, _) = import("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n");
    let BlockNode::Table(table) = &document.body()[0] else {
        panic!("a table: {:?}", document.body());
    };
    assert_eq!(table.rows.len(), 2);
    assert!(table.rows[0].properties.header);
    assert!(!table.rows[1].properties.header);
    let text = projection(&document);
    assert!(text.contains("Some(Start)|false](<:>1)"), "{text}");
    assert!(text.contains("Some(Center)|false](<:>2)"), "{text}");
    assert!(text.contains("Some(End)|false](<:>3)"), "{text}");
}

#[test]
fn code_blocks_are_one_preformatted_paragraph_per_line_and_inline_code_a_character_style() {
    let (document, _, report) = import("```rust\nfn a() {\n\n\tb\n}\n```\n\nUse `x` here\n");
    let text = projection(&document);
    assert!(text.contains("P[HTML Preformatted|"), "{text}");
    assert_eq!(
        styles_of(&document)
            .iter()
            .filter(|s| *s == "HTML Preformatted")
            .count(),
        4,
        "four lines, the blank one included, and no trailing empty line: {text}"
    );
    assert!(text.contains("<tab>"), "{text}");
    assert!(text.contains("<:HTML Code>x"), "{text}");
    assert!(features(&report).contains(&"markdown.import.code_language"));
}

#[test]
fn quotes_rules_emphasis_and_breaks() {
    let (document, _, _) = import("> quoted\n\n---\n\n*i* **b** ~~s~~ one\\\ntwo\n");
    let text = projection(&document);
    assert!(text.contains("P[Quote|"), "{text}");
    assert!(
        text.contains("P[||None|true]()"),
        "a rule paragraph: {text}"
    );
    assert!(
        text.contains("<i:>i<:> <b:>b<:> <s:>s<:> one<br><:>two"),
        "{text}"
    );
}

#[test]
fn links_are_hyperlinks_and_unsafe_schemes_are_not() {
    let (document, _, report) = import(
        "[site](https://example.com) [top](#intro) <me@example.com> [bad](javascript:alert(1))\n",
    );
    let text = projection(&document);
    assert!(
        text.contains("[https://example.com:<:Hyperlink>site]"),
        "{text}"
    );
    assert!(text.contains("[#intro:<:Hyperlink>top]"), "{text}");
    assert!(text.contains("[mailto:me@example.com:"), "{text}");
    assert!(!text.contains("javascript"), "{text}");
    assert!(text.contains(" bad)"), "the text survives: {text}");
    assert!(features(&report).contains(&"markdown.import.link_unsafe_scheme"));
}

/// The smallest valid PNG: a 1x1 pixel.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

#[test]
fn a_data_uri_picture_is_embedded_and_a_remote_one_is_reported_never_fetched() {
    let source = format!(
        "![dot](data:image/png;base64,{PNG}) ![far](https://example.com/a.png) ![near](a.png)\n"
    );
    let (document, resources, report) = import(&source);
    let text = projection(&document);
    assert!(text.contains("<img Some(\"dot\")>"), "{text}");
    assert_eq!(resources.as_map().len(), 1);
    assert!(
        resources
            .as_map()
            .values()
            .next()
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    assert!(
        text.contains("[https://example.com/a.png:<:Hyperlink>far]"),
        "{text}"
    );
    let found = features(&report);
    assert!(found.contains(&"markdown.import.image_remote"), "{found:?}");
    assert!(
        found.contains(&"markdown.import.image_unresolved"),
        "{found:?}"
    );
}

#[test]
fn a_host_resolver_supplies_a_local_picture() {
    let png = crate::flow_builder::decode_data_uri(&format!("data:image/png;base64,{PNG}"))
        .unwrap()
        .1;
    let adapter =
        MarkdownAdapter::default().with_local_images(std::sync::Arc::new(move |path: &str| {
            (path == "img/a.png").then(|| png.clone())
        }));
    let artifact = adapter
        .import(ImportRequest {
            bytes: b"![a](img/a.png)\n",
            retain_source: false,
        })
        .unwrap();
    assert_eq!(artifact.resources.as_map().len(), 1);
    assert!(projection(&artifact.document).contains("<img Some(\"a\")>"));
}

#[test]
fn gfm_footnotes_are_footnotes() {
    let (document, _, _) = import("Text[^n].\n\n[^n]: The note.\n");
    assert_eq!(document.definitions().footnotes.len(), 1);
    let text = projection(&document);
    assert!(text.contains("<:>Text<noteref><:>."), "{text}");
    assert!(
        text.contains("NOTE{P[||None|false](<notemark><:> The note.)"),
        "{text}"
    );
}

#[test]
fn raw_html_is_reported_and_never_carried_as_markup() {
    let (document, _, report) = import("<script>alert(1)</script>\n\nafter <b>x</b>\n");
    let text = projection(&document);
    assert!(!text.contains("alert"), "{text}");
    assert!(text.contains("after "), "{text}");
    let found = features(&report);
    assert!(
        found.contains(&"markdown.import.raw_html_block"),
        "{found:?}"
    );
    assert!(
        found.contains(&"markdown.import.raw_html_inline"),
        "{found:?}"
    );
}

#[test]
fn nesting_and_size_are_bounded() {
    let deep = format!("{}x\n", "> ".repeat(40));
    let error = MarkdownAdapter::default()
        .import(ImportRequest {
            bytes: deep.as_bytes(),
            retain_source: false,
        })
        .expect_err("40 quote levels is past the default 32");
    assert!(
        error.to_string().contains("markdown_nesting_depth"),
        "{error}"
    );
    let small = MarkdownAdapter::new(MarkdownLimits {
        max_input_bytes: 4,
        ..MarkdownLimits::default()
    });
    let error = small
        .import(ImportRequest {
            bytes: b"# heading",
            retain_source: false,
        })
        .expect_err("over the input limit");
    assert!(
        error.to_string().contains("markdown_input_bytes"),
        "{error}"
    );
}

#[test]
fn the_name_selects_markdown_and_a_txt_stays_plain_text() {
    let registry = builtin_registry();
    let detect = |name: Option<&str>| {
        registry
            .detect(DetectionRequest {
                bytes: b"# Title\n",
                selection: FormatSelection::Auto,
                file_name_hint: name,
                mime_hint: None,
            })
            .unwrap()
    };
    assert_eq!(detect(Some("notes.md")).as_str(), formats::MARKDOWN);
    assert_eq!(detect(Some("notes.markdown")).as_str(), formats::MARKDOWN);
    assert_eq!(detect(Some("notes.txt")).as_str(), formats::TEXT);
    assert_eq!(
        detect(None).as_str(),
        formats::TEXT,
        "no name: plain text, as before Markdown could be read"
    );
    let descriptor = registry
        .descriptors()
        .into_iter()
        .find(|d| d.id.as_str() == formats::MARKDOWN)
        .unwrap();
    assert!(descriptor.can_import && descriptor.can_export);
}

/// Markdown -> model -> the Markdown writer -> model is a fixed point for what
/// Markdown expresses. Two spellings of one document must read back the same.
#[test]
fn markdown_to_model_to_markdown_to_model_is_a_fixed_point() {
    let source = concat!(
        "# Title\n\n",
        "Intro with *italic*, **bold**, ~~gone~~, `code` and a [link](https://example.com).\\\n",
        "Second line[^1].\n\n",
        "## Lists\n\n",
        "- one\n",
        "- two\n",
        "    - nested\n",
        "- [ ] task\n",
        "- [x] done\n\n",
        "1. first\n",
        "2. second\n",
        // Nested under an ORDERED item, whose content column is 3, not 2.
        "    - under ordered\n\n",
        // A paragraph straight after a list: without a blank line between
        // them the writer's output would make it the item's lazy continuation.
        "After the list.\n\n",
        "> A quote.\n\n",
        "---\n\n",
        "```\nlet a = 1;\n\nlet b = `x`;\n```\n\n",
        "| Left | Centre | Right |\n",
        "| :--- | :---: | ---: |\n",
        "| a | b | c |\n\n",
        "Closing paragraph.\n\n",
        "[^1]: The note body.\n",
    );
    let (first, resources, _) = import(source);
    let written = export(&first, &resources);
    let (second, _, _) = import(&written);
    assert_eq!(
        projection(&first),
        projection(&second),
        "the writer's output reads back differently:\n{written}"
    );
    // And a second trip changes nothing more.
    let (third, _, _) = import(&export(&second, &resources));
    assert_eq!(projection(&second), projection(&third));
}
