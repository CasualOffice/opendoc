//! RTF import guards.
//!
//! Every guard here was driven red by mutating the production code before it
//! was accepted; the mutations and their output are recorded in the commit
//! that introduced them.

use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::InlineNode;
use casual_doc_model::v1::Paragraph;

use crate::code_page;
use crate::{RtfError, RtfImport, RtfLimits, import_rtf, is_rtf};

fn import(source: &str) -> RtfImport {
    import_rtf(source.as_bytes(), RtfLimits::default()).expect("fixture imports")
}

fn import_bytes(source: &[u8]) -> RtfImport {
    import_rtf(source, RtfLimits::default()).expect("fixture imports")
}

fn paragraphs(imported: &RtfImport) -> Vec<&Paragraph> {
    imported
        .document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .collect()
}

fn paragraph_text(paragraph: &Paragraph) -> String {
    paragraph
        .inlines
        .iter()
        .map(|inline| match inline {
            InlineNode::Run(run) => run.text.clone(),
            InlineNode::Tab(_) => "\t".to_owned(),
            InlineNode::Break(_) => "\n".to_owned(),
            _ => String::new(),
        })
        .collect()
}

fn body_text(imported: &RtfImport) -> String {
    paragraphs(imported)
        .iter()
        .map(|paragraph| paragraph_text(paragraph))
        .collect::<Vec<_>>()
        .join("\u{b6}")
}

// ---- detection ---------------------------------------------------------

#[test]
fn detection_accepts_rtf_and_refuses_everything_else() {
    assert!(is_rtf(br"{\rtf1\ansi}"));
    assert!(is_rtf(b"\xEF\xBB\xBF  { \\rtf1}"));
    assert!(!is_rtf(b"plain text"));
    assert!(!is_rtf(b"{\\rtx1}"));
    assert!(!is_rtf(b"PK\x03\x04"));
    assert!(!is_rtf(b""));
    assert_eq!(
        import_rtf(b"not rtf at all", RtfLimits::default()).unwrap_err(),
        RtfError::NotRtf
    );
}

// ---- the text layer ----------------------------------------------------

#[test]
fn ascii_paragraphs_and_breaks_map() {
    let imported = import(r"{\rtf1\ansi First\par Second\line third\tab tabbed\par}");
    assert_eq!(body_text(&imported), "First\u{b6}Second\nthird\ttabbed");
}

#[test]
fn unicode_escapes_decode_including_negative_and_surrogate_pairs() {
    // Written as fragments so no four-hex-digit run ever follows a
    // backslash-u in this file: several editors and tooling pipelines
    // rewrite that sequence as the character it names, which would
    // silently change the fixture into something that tests nothing.
    let source = concat!(
        r"{\rtf1\ansi\uc0 A\u",
        "233",
        r" B\u-3600 C\u",
        "55357",
        r" \u",
        "56832",
        r" D\par}",
    );
    // 233 is e-acute; -3600 reads back as 61936 = U+F1F0, a private-use
    // scalar real producers emit; 55357 + 56832 is the surrogate pair for
    // U+1F600.
    assert_eq!(body_text(&import(source)), "A\u{e9}B\u{f1f0}C\u{1f600}D");
}

#[test]
fn uc_fallback_characters_are_skipped_not_shown() {
    // `\uc1` means one fallback character follows each `\u`; showing it would
    // print "A?B" instead of "AéB", the single most visible RTF import bug.
    let imported = import(r"{\rtf1\ansi\uc1 A\u233 ?B\par}");
    assert_eq!(body_text(&imported), "A\u{e9}B");

    // `\uc2` skips two, including a hex escape.
    let imported = import(r"{\rtf1\ansi\uc2 A\u233 ?\'3fB\par}");
    assert_eq!(body_text(&imported), "A\u{e9}B");
}

#[test]
fn unpaired_surrogates_become_replacement_and_are_reported() {
    let source = concat!(r"{\rtf1\ansi\uc0 A\u", "55357", r" B\par}",);
    let imported = import(source);
    assert!(body_text(&imported).contains('\u{fffd}'));
    assert!(imported.report.has("rtf.text.unpaired-surrogate"));
}

#[test]
fn hex_escapes_decode_through_the_declared_code_page() {
    // 0x93/0x94 are curly quotes in Windows-1252 and Cyrillic letters in 1251.
    let latin = import(r"{\rtf1\ansi\ansicpg1252 \'93quoted\'94\par}");
    assert_eq!(body_text(&latin), "\u{201c}quoted\u{201d}");

    let cyrillic = import(r"{\rtf1\ansi\ansicpg1251 \'cf\'f0\'e8\'e2\'e5\'f2\par}");
    assert_eq!(
        body_text(&cyrillic),
        "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"
    );
}

#[test]
fn font_charset_overrides_the_document_code_page() {
    // The document says 1252, the run's font says charset 204 (Cyrillic 1251).
    // Following the document would render "Ïðèâåò" — the classic mojibake.
    let source = r"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0\fswiss\fcharset204 Arial;}}\f0 \'cf\'f0\'e8\'e2\'e5\'f2\par}";
    assert_eq!(
        body_text(&import(source)),
        "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"
    );
}

#[test]
fn undecodable_bytes_are_replaced_and_reported_never_guessed() {
    // 0x81 is undefined in Windows-1252.
    let imported = import(r"{\rtf1\ansi\ansicpg1252 a\'81b\par}");
    assert_eq!(body_text(&imported), "a\u{fffd}b");
    assert!(imported.report.has("rtf.text.undecodable-byte"));
}

#[test]
fn multi_byte_code_pages_are_reported_rather_than_decoded_wrongly() {
    let imported = import(r"{\rtf1\ansi\ansicpg932 \'82\'a0\par}");
    assert!(imported.report.has("rtf.codepage.multibyte-unsupported"));
    assert!(!body_text(&imported).contains('\u{201a}'));
}

#[test]
fn code_page_tables_pin_known_scalars() {
    // Generated tables are only trustworthy if a corrupted regeneration fails
    // the build, so pin one distinctive code point in each family.
    let cases: &[(u16, u8, char)] = &[
        (1252, 0x80, '\u{20ac}'),
        (1252, 0x93, '\u{201c}'),
        (1250, 0x8c, '\u{015a}'),
        (1251, 0xc0, '\u{0410}'),
        (1253, 0xc1, '\u{0391}'),
        (1254, 0xd0, '\u{011e}'),
        (1255, 0xe0, '\u{05d0}'),
        (1256, 0xc7, '\u{0627}'),
        (1257, 0xc0, '\u{0104}'),
        (1258, 0xcc, '\u{0300}'),
        (874, 0xa1, '\u{0e01}'),
        (437, 0x80, '\u{00c7}'),
        (850, 0x9b, '\u{00f8}'),
        (10_000, 0xa5, '\u{2022}'),
        (20_866, 0xc1, '\u{0430}'),
    ];
    for (number, byte, expected) in cases {
        let page = code_page(*number).unwrap_or_else(|| panic!("code page {number} is tabulated"));
        assert_eq!(
            page.decode(*byte),
            Some(*expected),
            "code page {number} byte {byte:#04x}"
        );
    }
    assert_eq!(code_page(1252).unwrap().decode(0x81), None);
    assert!(code_page(932).is_none());
}

#[test]
fn control_symbol_glyphs_map() {
    let imported = import(r"{\rtf1\ansi a\~b\\c\{d\}e\emdash f\bullet\par}");
    assert_eq!(body_text(&imported), "a\u{a0}b\\c{d}e\u{2014}f\u{2022}");
}

// ---- runs and paragraphs ------------------------------------------------

#[test]
fn a_formatting_span_becomes_exactly_one_run() {
    // Decoded characters are buffered until the run properties change.
    // Emitting a run per decoded character instead would produce adjacent
    // runs carrying equal properties, which the model rejects outright, so
    // this is a document-validity guard and not a tidiness one.
    let imported = import(r"{\rtf1\ansi one \b two\b0  three\par}");
    let paragraphs = paragraphs(&imported);
    let runs: Vec<_> = paragraphs[0]
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Run(run) => Some(run),
            _ => None,
        })
        .collect();
    assert_eq!(runs.len(), 3, "expected one run per span, got {runs:?}");
    assert_eq!(runs[0].text, "one ");
    assert_eq!(runs[1].text, "two");
    assert_eq!(runs[2].text, " three");
    assert_eq!(runs[0].properties.bold, None);
    assert_eq!(runs[1].properties.bold, Some(true));
    assert_eq!(runs[2].properties.bold, Some(false));
}

#[test]
fn character_formatting_maps() {
    use casual_doc_model::v1::Color;
    use casual_doc_model::v1::RgbColor;
    use casual_doc_model::v1::UnderlineStyle;
    use casual_doc_model::v1::VerticalAlignment;

    let source =
        r"{\rtf1\ansi{\colortbl ;\red255\green0\blue0;}\b\i\ul\strike\fs36\cf1\super bold\par}";
    let imported = import(source);
    let paragraphs = paragraphs(&imported);
    let InlineNode::Run(run) = &paragraphs[0].inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.properties.bold, Some(true));
    assert_eq!(run.properties.italic, Some(true));
    assert_eq!(run.properties.underline, Some(true));
    assert_eq!(run.properties.underline_style, Some(UnderlineStyle::Single));
    assert_eq!(run.properties.strike, Some(true));
    assert_eq!(run.properties.size_half_points, Some(36));
    assert_eq!(
        run.properties.color,
        Some(Color::Rgb(RgbColor { r: 255, g: 0, b: 0 }))
    );
    assert_eq!(
        run.properties.vertical_alignment,
        Some(VerticalAlignment::Superscript)
    );
}

#[test]
fn a_zero_parameter_turns_a_toggle_off() {
    use casual_doc_model::v1::UnderlineStyle;

    // `\b0`, `\i0` and `\ul0` all mean OFF. Reading `\ul0` as "underline on"
    // underlines the remainder of the paragraph, which is the shape of bug a
    // toggle table gets wrong once and nobody notices until a real document
    // opens with a rule under half a page.
    let imported = import(r"{\rtf1\ansi\b\i\ul on\b0\i0\ul0  off\par}");
    let paragraphs = paragraphs(&imported);
    let runs: Vec<_> = paragraphs[0]
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Run(run) => Some(run),
            _ => None,
        })
        .collect();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].properties.bold, Some(true));
    assert_eq!(runs[0].properties.underline, Some(true));
    assert_eq!(
        runs[0].properties.underline_style,
        Some(UnderlineStyle::Single)
    );
    assert_eq!(runs[1].properties.bold, Some(false));
    assert_eq!(runs[1].properties.italic, Some(false));
    assert_eq!(runs[1].properties.underline, Some(false));
    assert_eq!(runs[1].properties.underline_style, None);
}

#[test]
fn colour_index_zero_is_auto_not_black() {
    use casual_doc_model::v1::Color;

    // `{\colortbl ;…}` declares entry 0 as auto. Importing it as black would
    // silently repaint every default-coloured run in the document.
    let imported = import(r"{\rtf1\ansi{\colortbl ;\red255\green0\blue0;}\cf0 plain\par}");
    let paragraphs = paragraphs(&imported);
    let InlineNode::Run(run) = &paragraphs[0].inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.properties.color, Some(Color::Auto));
}

#[test]
fn paragraph_formatting_maps() {
    use casual_doc_model::v1::Alignment;
    use casual_doc_model::v1::LineRule;

    let imported =
        import(r"{\rtf1\ansi\qc\li720\ri360\fi-360\sb120\sa240\sl360\slmult1\keepn body\par}");
    let paragraphs = paragraphs(&imported);
    let properties = &paragraphs[0].properties;
    assert_eq!(properties.alignment, Some(Alignment::Center));
    let indentation = properties.indentation.expect("indentation");
    assert_eq!(indentation.start_twips, Some(720));
    assert_eq!(indentation.end_twips, Some(360));
    assert_eq!(indentation.hanging_twips, Some(360));
    assert_eq!(indentation.first_line_twips, None);
    let spacing = properties.spacing.expect("spacing");
    assert_eq!(spacing.before_twips, Some(120));
    assert_eq!(spacing.after_twips, Some(240));
    assert_eq!(spacing.line_rule, Some(LineRule::Auto));
    assert_eq!(spacing.line_percent, Some(150));
    assert!(properties.keep_next);
}

#[test]
fn tab_stops_map_with_alignment_and_leader() {
    use casual_doc_model::v1::TabAlignment;
    use casual_doc_model::v1::TabLeader;

    let imported = import(r"{\rtf1\ansi\tx720\tqr\tldot\tx4320 body\par}");
    let paragraphs = paragraphs(&imported);
    let tabs = &paragraphs[0].properties.tabs;
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[0].position_twips, 720);
    assert_eq!(tabs[0].alignment, TabAlignment::Start);
    assert_eq!(tabs[0].leader, None);
    assert_eq!(tabs[1].position_twips, 4_320);
    assert_eq!(tabs[1].alignment, TabAlignment::End);
    assert_eq!(tabs[1].leader, Some(TabLeader::Dot));
}

// ---- tables -------------------------------------------------------------

#[test]
fn tables_map_rows_cells_and_widths() {
    let source = concat!(
        r"{\rtf1\ansi",
        r"\trowd\cellx2880\cellx5760 A\cell B\cell\row",
        r"\trowd\cellx2880\cellx5760 C\cell D\cell\row",
        r"\pard after\par}"
    );
    let imported = import(source);
    let BlockNode::Table(table) = &imported.document.body()[0] else {
        panic!(
            "expected a table first, got {:?}",
            imported.document.body()[0]
        );
    };
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.grid.len(), 2);
    assert_eq!(table.grid[0].width_twips, Some(2_880));
    assert_eq!(table.grid[1].width_twips, Some(2_880));
    let BlockNode::Paragraph(first) = &table.rows[0].cells[0].blocks[0] else {
        panic!("expected a paragraph in the first cell");
    };
    assert_eq!(paragraph_text(first).trim(), "A");
    assert_eq!(imported.document.body().len(), 2);
}

#[test]
fn horizontal_and_vertical_merges_map() {
    use casual_doc_model::v1::VerticalMerge;

    let source = concat!(
        r"{\rtf1\ansi",
        r"\trowd\clmgf\cellx2880\clmrg\cellx5760 merged\cell spill\cell\row",
        r"\trowd\clvmgf\cellx2880\cellx5760 top\cell right\cell\row",
        r"\trowd\clvmrg\cellx2880\cellx5760 cont\cell right2\cell\row}"
    );
    let imported = import(source);
    let BlockNode::Table(table) = &imported.document.body()[0] else {
        panic!("expected a table");
    };
    assert_eq!(
        table.rows[0].cells.len(),
        1,
        "a \\clmrg cell folds into the \\clmgf cell"
    );
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
fn table_cell_content_is_never_lost_to_a_merge() {
    let imported =
        import(r"{\rtf1\ansi\trowd\clmgf\cellx2880\clmrg\cellx5760 left\cell right\cell\row}");
    let BlockNode::Table(table) = &imported.document.body()[0] else {
        panic!("expected a table");
    };
    let text: String = table.rows[0].cells[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph_text(paragraph)),
            _ => None,
        })
        .collect();
    assert!(
        text.contains("left"),
        "left cell text missing from {text:?}"
    );
    assert!(
        text.contains("right"),
        "merged cell's text was dropped: {text:?}"
    );
}

// ---- lists --------------------------------------------------------------

#[test]
fn lists_map_through_the_list_and_override_tables() {
    use casual_doc_model::v1::NumberFormat;

    let source = concat!(
        r"{\rtf1\ansi",
        r"{\*\listtable{\list\listtemplateid1",
        r"{\listlevel\levelnfc0\leveljc0\levelstartat3{\leveltext\'02\'00.;}}",
        r"\listid101}}",
        r"{\*\listoverridetable{\listoverride\listid101\listoverridecount0\ls7}}",
        r"\pard\ls7\ilvl0 item\par}"
    );
    let imported = import(source);
    let paragraphs = paragraphs(&imported);
    let numbering = paragraphs[0]
        .properties
        .numbering
        .expect("the paragraph carries a numbering reference");
    assert_eq!(numbering.level, 0);
    let instance = imported
        .document
        .definitions()
        .numbering
        .get(&numbering.instance)
        .expect("numbering instance exists");
    let definition = imported
        .document
        .definitions()
        .abstract_numbering
        .get(&instance.abstract_ref)
        .expect("abstract numbering exists");
    assert_eq!(definition.levels.len(), 1);
    assert_eq!(definition.levels[0].start, 3);
    assert_eq!(definition.levels[0].num_fmt, Some(NumberFormat::Decimal));
    assert_eq!(definition.levels[0].lvl_text.as_deref(), Some("%1."));
}

#[test]
fn an_unresolved_list_reference_is_reported_and_never_dangles() {
    // A `\ls` with no matching override must not produce a numbering reference
    // pointing at nothing — that is a model-validation failure, i.e. a refused
    // document, for a file Word opens fine.
    let imported = import(r"{\rtf1\ansi\pard\ls9\ilvl0 orphan\par}");
    assert!(imported.report.has("rtf.list.unresolved"));
    assert_eq!(paragraphs(&imported)[0].properties.numbering, None);
}

#[test]
fn legacy_pn_bullet_text_does_not_leak_into_the_body() {
    let imported = import(r"{\rtf1\ansi\pard{\pntext\f3\'b7\tab}item\par}");
    assert_eq!(body_text(&imported), "item");
    assert!(imported.report.has("rtf.list.legacy-pn"));
}

// ---- pictures -----------------------------------------------------------

#[test]
fn pictures_become_drawings_with_bytes_in_resources() {
    // A one-pixel PNG, hex-encoded the way RTF carries it.
    let source = concat!(
        r"{\rtf1\ansi{\pict\pngblip\picw16\pich16\picwgoal1440\pichgoal720 ",
        "89504e470d0a1a0a}\\par}"
    );
    let imported = import(source);
    let paragraphs = paragraphs(&imported);
    let drawing = paragraphs[0]
        .inlines
        .iter()
        .find_map(|inline| match inline {
            InlineNode::Drawing(drawing) => Some(drawing),
            _ => None,
        })
        .expect("the picture became a drawing");
    let extent = drawing.extent.expect("extent from picwgoal/pichgoal");
    assert_eq!(extent.width_emu, 1_440 * 635);
    assert_eq!(extent.height_emu, 720 * 635);
    let media = imported
        .document
        .definitions()
        .media
        .get(&drawing.media)
        .expect("media reference exists");
    assert_eq!(media.media_type, "image/png");
    let bytes = imported
        .resources
        .get(&media.part_name)
        .expect("picture bytes reached resources, not only a private side table");
    assert_eq!(bytes, &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
}

#[test]
fn metafile_pictures_are_carried_but_reported_as_not_renderable() {
    let source = r"{\rtf1\ansi{\pict\wmetafile8\picwgoal1440\pichgoal720 0102030405}\par}";
    let imported = import(source);
    assert!(imported.report.has("rtf.pict.not-renderable"));
    assert_eq!(imported.resources.len(), 1);
}

#[test]
fn a_binary_picture_payload_is_consumed_not_relexed() {
    // `\bin` payload bytes must leave the token stream. If they do not, the
    // `\par` spelled inside the payload below becomes a real paragraph and the
    // brace inside it corrupts the group stack.
    let mut source = br"{\rtf1\ansi{\pict\pngblip\picwgoal100\pichgoal100\bin11 ".to_vec();
    source.extend_from_slice(br"{\par}\abcd");
    source.extend_from_slice(b"}xyz\\par}");
    let imported = import_bytes(&source);
    assert_eq!(body_text(&imported), "xyz");
    let bytes = imported.resources.values().next().expect("picture bytes");
    assert_eq!(bytes.as_slice(), br"{\par}\abcd");
}

// ---- metadata and sections ----------------------------------------------

#[test]
fn info_metadata_maps() {
    let source = concat!(
        r"{\rtf1\ansi{\info{\title Quarterly Report}{\author Ada}",
        r"{\subject Numbers}{\doccomm Draft}{\company Example Ltd}}",
        r"{\*\generator OpenDoc Test;}body\par}"
    );
    let imported = import(source);
    let properties = imported.document.properties().expect("document properties");
    assert_eq!(properties.core.title.as_deref(), Some("Quarterly Report"));
    assert_eq!(properties.core.creator.as_deref(), Some("Ada"));
    assert_eq!(properties.core.subject.as_deref(), Some("Numbers"));
    assert_eq!(properties.core.description.as_deref(), Some("Draft"));
    assert_eq!(properties.app.company.as_deref(), Some("Example Ltd"));
    assert_eq!(properties.app.application.as_deref(), Some("OpenDoc Test"));
    assert_eq!(body_text(&imported), "body");
}

#[test]
fn page_setup_maps_and_a_defaulted_section_says_so() {
    use casual_doc_model::v1::PageOrientation;

    let imported = import(
        r"{\rtf1\ansi\paperw16840\paperh11900\margl1000\margr1100\margt1200\margb1300\landscape body\par}",
    );
    let section = &imported.document.definitions().sections[0];
    assert_eq!(section.page_size.width_twips, 16_840);
    assert_eq!(section.page_size.height_twips, 11_900);
    assert_eq!(section.page_margins.start_twips, 1_000);
    assert_eq!(section.page_margins.end_twips, 1_100);
    assert_eq!(section.page_margins.top_twips, 1_200);
    assert_eq!(section.page_margins.bottom_twips, 1_300);
    assert_eq!(section.orientation, Some(PageOrientation::Landscape));
    assert!(!imported.report.has("rtf.section.defaulted"));

    let defaulted = import(r"{\rtf1\ansi body\par}");
    assert!(defaulted.report.has("rtf.section.defaulted"));
}

#[test]
fn a_section_break_is_stamped_on_the_paragraph_that_ends_it() {
    let imported = import(r"{\rtf1\ansi\paperw12240 one\sect\sectd\paperw15840 two\par}");
    let paragraphs = paragraphs(&imported);
    let break_id = paragraphs[0]
        .properties
        .section_break
        .expect("the first paragraph ends the first section");
    let sections = &imported.document.definitions().sections;
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].id, break_id);
    assert_eq!(sections[0].page_size.width_twips, 12_240);
    assert_eq!(sections[1].page_size.width_twips, 15_840);
}

// ---- loss reporting ------------------------------------------------------

#[test]
fn every_dropped_family_is_reported_and_its_text_does_not_leak() {
    let source = concat!(
        r"{\rtf1\ansi",
        r"{\header header text}",
        r"{\footer footer text}",
        r"{\stylesheet{\s1 Heading;}}",
        r"{\*\bkmkstart mark}{\*\bkmkend mark}",
        r"{\*\annotation comment text}",
        r"body{\footnote note text}",
        r"{\field{\*\fldinst PAGE}{\fldrslt 7}}",
        r"{\object\objemb{\*\objdata 0102}}",
        r"{\*\unknowndest secret}",
        r"\par}"
    );
    let imported = import(source);
    for feature in [
        "rtf.section.header-footer",
        "rtf.stylesheet",
        "rtf.bookmark",
        "rtf.annotation",
        "rtf.note",
        "rtf.field.instruction",
        "rtf.object",
        "rtf.destination.ignored",
    ] {
        assert!(
            imported.report.has(feature),
            "{feature} was dropped without a finding; report: {:?}",
            imported.report.entries
        );
    }
    let text = body_text(&imported);
    for leaked in [
        "header text",
        "footer text",
        "Heading",
        "comment text",
        "note text",
        "secret",
        "PAGE",
    ] {
        assert!(
            !text.contains(leaked),
            "{leaked:?} leaked into the body: {text:?}"
        );
    }
    // The field's *visible result* must survive, unlike its instruction.
    assert!(text.contains('7'), "field result text was lost: {text:?}");
}

#[test]
fn an_unknown_control_word_is_reported_with_its_name() {
    let imported = import(r"{\rtf1\ansi\notarealcontrolword body\par}");
    let entry = imported
        .report
        .entries
        .iter()
        .find(|entry| entry.feature == "rtf.control-word.unknown")
        .expect("unknown control words are reported");
    assert_eq!(entry.control_word.as_deref(), Some("notarealcontrolword"));
    assert_eq!(body_text(&imported), "body");
}

#[test]
fn findings_aggregate_rather_than_growing_per_occurrence() {
    let mut source = String::from(r"{\rtf1\ansi");
    for _ in 0..500 {
        source.push_str(r"\notarealcontrolword x");
    }
    source.push_str(r"\par}");
    let imported = import(&source);
    let entry = imported
        .report
        .entries
        .iter()
        .find(|entry| entry.feature == "rtf.control-word.unknown")
        .expect("reported");
    assert_eq!(entry.occurrences, 500);
    assert_eq!(
        imported.report.entries.len(),
        2,
        "one unknown word plus the defaulted section"
    );
}

// ---- determinism ---------------------------------------------------------

#[test]
fn the_same_bytes_produce_the_same_identities() {
    let source = r"{\rtf1\ansi one\par two\par}";
    let first = import(source);
    let second = import(source);
    assert_eq!(first.document.id(), second.document.id());
    assert_eq!(
        first.document.to_json().unwrap(),
        second.document.to_json().unwrap()
    );
    // A different document must not reuse the same namespace.
    let other = import(r"{\rtf1\ansi one\par three\par}");
    assert_ne!(first.document.id(), other.document.id());
}

// ---- hostile input -------------------------------------------------------

#[test]
fn a_truncated_stream_is_handled_without_panicking() {
    for truncated in [
        r"{\rtf1\ansi Hello",
        r"{\rtf1\ansi Hello\",
        r"{\rtf1\ansi \'",
        r"{\rtf1\ansi \'2",
        r"{\rtf1",
        r"{\rtf",
    ] {
        let result = import_rtf(truncated.as_bytes(), RtfLimits::default());
        match result {
            Ok(imported) => assert!(!imported.document.body().is_empty()),
            Err(error) => assert!(matches!(
                error,
                RtfError::Malformed { .. } | RtfError::NotRtf
            )),
        }
    }
}

#[test]
fn unbalanced_braces_are_handled_in_both_directions() {
    let extra_close = import(r"{\rtf1\ansi a\par}}}}b\par}");
    assert!(!extra_close.document.body().is_empty());

    let extra_open = import(r"{\rtf1\ansi{{{{a\par");
    assert!(!extra_open.document.body().is_empty());
}

#[test]
fn nesting_past_the_group_bound_is_refused_not_overflowed() {
    let mut source = String::from(r"{\rtf1\ansi");
    for _ in 0..5_000 {
        source.push('{');
    }
    source.push_str("deep");
    let error = import_rtf(source.as_bytes(), RtfLimits::default()).unwrap_err();
    assert!(
        matches!(
            error,
            RtfError::LimitExceeded {
                limit: "rtf_group_depth",
                ..
            }
        ),
        "expected a depth refusal, got {error:?}"
    );
}

#[test]
fn a_huge_declared_binary_payload_is_refused_without_allocating() {
    let source = br"{\rtf1\ansi{\pict\pngblip\bin2000000000 ab}\par}";
    let error = import_rtf(source, RtfLimits::default()).unwrap_err();
    assert!(
        matches!(error, RtfError::Malformed { .. }),
        "expected a malformed refusal, got {error:?}"
    );
}

#[test]
fn an_over_limit_picture_is_refused() {
    let limits = RtfLimits {
        max_picture_bytes: 4,
        ..RtfLimits::default()
    };
    let source = br"{\rtf1\ansi{\pict\pngblip 0102030405060708}\par}";
    let error = import_rtf(source, limits).unwrap_err();
    assert!(matches!(
        error,
        RtfError::LimitExceeded {
            limit: "rtf_picture_bytes",
            ..
        }
    ));
}

#[test]
fn an_over_limit_paragraph_count_is_refused() {
    let limits = RtfLimits {
        max_paragraphs: 4,
        ..RtfLimits::default()
    };
    let mut source = String::from(r"{\rtf1\ansi");
    for _ in 0..10 {
        source.push_str(r"x\par ");
    }
    source.push('}');
    let error = import_rtf(source.as_bytes(), limits).unwrap_err();
    assert!(matches!(
        error,
        RtfError::LimitExceeded {
            limit: "rtf_paragraphs",
            ..
        }
    ));
}

#[test]
fn an_oversized_input_is_refused_before_parsing() {
    let limits = RtfLimits {
        max_input_bytes: 8,
        ..RtfLimits::default()
    };
    let error = import_rtf(br"{\rtf1\ansi hello\par}", limits).unwrap_err();
    assert!(matches!(
        error,
        RtfError::LimitExceeded {
            limit: "rtf_input_bytes",
            ..
        }
    ));
}

#[test]
fn a_limit_above_its_hard_ceiling_is_refused() {
    let limits = RtfLimits {
        max_group_depth: RtfLimits::HARD_MAX_GROUP_DEPTH + 1,
        ..RtfLimits::default()
    };
    assert!(matches!(
        import_rtf(br"{\rtf1}", limits).unwrap_err(),
        RtfError::LimitAboveCeiling {
            limit: "rtf_group_depth",
            ..
        }
    ));
}

#[test]
fn an_over_long_control_word_is_refused() {
    let mut source = String::from(r"{\rtf1\ansi\");
    source.push_str(&"a".repeat(64));
    source.push_str(r" body\par}");
    let error = import_rtf(source.as_bytes(), RtfLimits::default()).unwrap_err();
    assert!(matches!(
        error,
        RtfError::LimitExceeded {
            limit: "rtf_control_word_bytes",
            ..
        }
    ));
}

#[test]
fn an_empty_document_still_opens() {
    // The model refuses an empty body, so an empty RTF must still produce one
    // paragraph rather than failing: opening an empty file is legitimate.
    let imported = import(r"{\rtf1\ansi}");
    assert_eq!(imported.document.body().len(), 1);
}

#[test]
fn a_stream_of_random_bytes_behind_the_signature_never_panics() {
    // A cheap deterministic fuzz: every byte value in every position class.
    let mut source = Vec::from(br"{\rtf1\ansi ");
    for byte in 0_u8..=255 {
        source.push(byte);
    }
    source.push(b'}');
    let _ = import_rtf(&source, RtfLimits::default());

    let mut adversarial = Vec::from(br"{\rtf1");
    for byte in 0_u8..=255 {
        adversarial.push(b'\\');
        adversarial.push(byte);
        adversarial.push(b'{');
        adversarial.push(byte);
        adversarial.push(b'}');
    }
    let _ = import_rtf(&adversarial, RtfLimits::default());
}
