use casual_doc_model::v1::{
    Alignment, BlockNode, Break, BreakKind, Color, DocumentProtectionEdit, HyperlinkTarget,
    InlineNode, LevelJustification, LevelSuffix, MoveKind, NumberFormat, Paragraph,
    PositionalTabAlignment, PositionalTabLeader, PositionalTabRelativeTo, ProofState, RevisionKind,
    RgbColor, SdtControlKind, StyleKind, Symbol,
};
use casual_doc_ooxml::DocxPackage;

use crate::{
    Import, ImportConfig, ImportError, ImportMode, ModelOutcome, RetentionOutcome,
    import_main_document_xml, import_package, import_with_sources,
};

fn import(xml: &[u8]) -> Import {
    import_main_document_xml(xml, ImportConfig::default()).unwrap()
}

fn import_with_styles(document: &[u8], styles: &[u8]) -> Import {
    import_with_sources(
        document,
        Some(styles),
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &[],
        &[],
        None,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

fn import_with_numbering(document: &[u8], numbering: &[u8]) -> Import {
    import_with_sources(
        document,
        None,
        Some(numbering),
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &[],
        &[],
        None,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

fn import_with_settings(document: &[u8], settings: &[u8]) -> Import {
    import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        Some(settings),
        None,
        None,
        &[],
        &[],
        None,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

fn part_sources(xml: &[u8]) -> crate::PartSources {
    crate::PartSources {
        xml: xml.to_vec(),
        ..Default::default()
    }
}

fn import_with_notes(document: &[u8], footnotes: Option<&[u8]>, endnotes: Option<&[u8]>) -> Import {
    let footnotes = footnotes.map(part_sources);
    let endnotes = endnotes.map(part_sources);
    import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        footnotes.as_ref(),
        endnotes.as_ref(),
        &[],
        &[],
        None,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

fn import_with_comments(document: &[u8], comments: &[u8]) -> Import {
    let comments = part_sources(comments);
    import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &[],
        &[],
        Some(&comments),
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

fn features(import: &Import) -> Vec<&str> {
    import
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect()
}

fn paragraph(import: &Import, index: usize) -> &Paragraph {
    match &import.document.body()[index] {
        BlockNode::Paragraph(paragraph) => paragraph,
        BlockNode::Table(_) | BlockNode::Sdt(_) | BlockNode::AltChunk(_) => {
            panic!("expected a paragraph at index {index}")
        }
    }
}

/// Per-paragraph run text in document order, recursing through table cells.
fn collect_block_texts(blocks: &[BlockNode], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let text: String = paragraph
                    .inlines
                    .iter()
                    .filter_map(|inline| match inline {
                        InlineNode::Run(run) => Some(run.text.as_str()),
                        _ => None,
                    })
                    .collect();
                out.push(text);
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_block_texts(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_block_texts(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

fn nonempty_block_texts(import: &Import) -> Vec<String> {
    let mut texts = Vec::new();
    collect_block_texts(import.document.body(), &mut texts);
    texts.retain(|text| !text.is_empty());
    texts
}

/// The first table in the body, if any.
fn first_table(import: &Import) -> Option<&casual_doc_model::v1::Table> {
    import.document.body().iter().find_map(|block| match block {
        BlockNode::Table(table) => Some(table),
        BlockNode::Paragraph(_) | BlockNode::Sdt(_) | BlockNode::AltChunk(_) => None,
    })
}

#[test]
fn paragraphs_runs_and_run_properties_are_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hello</w:t></w:r>
                 <w:r><w:t xml:space="preserve"> world</w:t></w:r></w:p>
            <w:p><w:r><w:t>Second</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    assert_eq!(import.document.body().len(), 2);

    let first = paragraph(&import, 0);
    assert_eq!(first.inlines.len(), 2);
    let InlineNode::Run(bold) = &first.inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(bold.text, "Hello");
    assert_eq!(bold.properties.bold, Some(true));
    let InlineNode::Run(plain) = &first.inlines[1] else {
        panic!("expected run");
    };
    assert_eq!(plain.text, " world");
    assert_eq!(plain.properties.bold, None);

    assert_eq!(paragraph(&import, 1).inlines.len(), 1);
}

#[test]
fn adjacent_equal_property_runs_are_merged() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    assert_eq!(para.inlines.len(), 1);
    let InlineNode::Run(run) = &para.inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "ab");
}

#[test]
fn tabs_breaks_and_color_are_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:color w:val="FF0000"/></w:rPr><w:t>a</w:t><w:tab/><w:t>b</w:t>
                 <w:br w:type="page"/></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    assert_eq!(para.inlines.len(), 4);
    assert!(matches!(para.inlines[0], InlineNode::Run(_)));
    assert!(matches!(para.inlines[1], InlineNode::Tab(_)));
    assert!(matches!(para.inlines[2], InlineNode::Run(_)));
    assert!(matches!(
        para.inlines[3],
        InlineNode::Break(Break {
            kind: BreakKind::Page,
            ..
        })
    ));
    let InlineNode::Run(run) = &para.inlines[0] else {
        panic!();
    };
    assert_eq!(
        run.properties.color,
        Some(Color::Rgb(RgbColor { r: 255, g: 0, b: 0 }))
    );
}

/// Reads the first run's properties from paragraph 0.
fn first_run_props(import: &Import) -> &casual_doc_model::v1::RunProperties {
    match &paragraph(import, 0).inlines[0] {
        InlineNode::Run(run) => &run.properties,
        _ => panic!("expected a run"),
    }
}

#[test]
fn run_toggle_marks_are_mapped() {
    use casual_doc_model::v1::RunProperties;
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr>
            <w:caps/><w:smallCaps/><w:vanish/><w:webHidden/><w:dstrike w:val="0"/>
        </w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let p: &RunProperties = first_run_props(&import);
    assert_eq!(p.all_caps, Some(true));
    assert_eq!(p.small_caps, Some(true));
    assert_eq!(p.hidden, Some(true));
    assert_eq!(p.web_hidden, Some(true));
    assert_eq!(p.double_strike, Some(false), "val=0 clears the toggle");
}

#[test]
fn run_fonts_named_and_theme_slots_are_mapped() {
    use casual_doc_model::v1::{FontName, FontRef, ThemeFont, ThemeFontRef};
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr>
            <w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Arial" w:eastAsiaTheme="minorEastAsia"/>
        </w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let p = first_run_props(&import);
    assert_eq!(
        p.font_ref,
        Some(FontRef::Named(FontName {
            name: "Calibri".to_owned()
        })),
        "the ascii slot finally populates font_ref"
    );
    assert_eq!(
        p.font_ref_cs,
        Some(FontRef::Named(FontName {
            name: "Arial".to_owned()
        }))
    );
    assert_eq!(
        p.font_ref_east_asia,
        Some(FontRef::Theme(ThemeFont {
            slot: ThemeFontRef::MinorEastAsia
        })),
        "eastAsiaTheme=minorEastAsia -> MinorEastAsia slot"
    );
}

#[test]
fn rfonts_bogus_theme_falls_through_to_the_named_family() {
    // Regression (review, no-silent-loss): a theme value outside the
    // major*/minor* vocabulary must NOT swallow the slot's named fallback — even
    // when a sibling slot resolves (so the element is consumed and could not be
    // reported per-slot).
    use casual_doc_model::v1::{FontName, FontRef};
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr>
            <w:rFonts w:asciiTheme="bogus" w:ascii="Calibri" w:cs="Arial"/>
        </w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let p = first_run_props(&import);
    assert_eq!(
        p.font_ref,
        Some(FontRef::Named(FontName {
            name: "Calibri".to_owned()
        })),
        "bogus asciiTheme falls through to the named ascii family"
    );
    assert_eq!(
        p.font_ref_cs,
        Some(FontRef::Named(FontName {
            name: "Arial".to_owned()
        }))
    );
}

#[test]
fn rfonts_with_only_a_recognized_hint_is_modeled() {
    // A recognized `@hint` is now a first-class value: the rFonts is consumed
    // (not reported) even when no font slot resolves.
    use casual_doc_model::v1::RunFontHint;
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr><w:rFonts w:hint="eastAsia"/></w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let p = first_run_props(&import);
    assert!(p.font_ref.is_none());
    assert_eq!(p.font_hint, Some(RunFontHint::EastAsia));
    assert!(!features(&import).contains(&"rFonts"));
}

#[test]
fn rfonts_with_only_an_unknown_hint_is_reported() {
    // An rFonts carrying only unmodeled detail (an unrecognized hint, no slot)
    // resolves nothing and is reported — no silent loss.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr><w:rFonts w:hint="bogus"/></w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let p = first_run_props(&import);
    assert!(p.font_hint.is_none());
    assert!(features(&import).contains(&"rFonts"));
}

#[test]
fn font_table_descriptors_are_parsed() {
    use casual_doc_model::v1::{FontFamilyKind, FontPitch};
    let xml = br#"<w:fonts xmlns:w="urn:w">
        <w:font w:name="Calibri"><w:altName w:val="Carlito"/><w:panose1 w:val="020F0502"/>
            <w:charset w:val="00"/><w:family w:val="swiss"/><w:pitch w:val="variable"/>
            <w:sig w:usb0="E4002EFF" w:csb0="0000019F"/><w:notTrueType/></w:font>
        <w:font w:name="Symbol"/>
    </w:fonts>"#;
    let fonts = crate::font_table::parse(
        xml,
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap();
    assert_eq!(fonts.len(), 2);
    assert_eq!(fonts[0].name, "Calibri");
    assert_eq!(fonts[0].alt_name.as_deref(), Some("Carlito"));
    assert_eq!(fonts[0].panose1.as_deref(), Some("020F0502"));
    assert_eq!(fonts[0].charset.as_deref(), Some("00"));
    assert_eq!(fonts[0].family, Some(FontFamilyKind::Swiss));
    assert_eq!(fonts[0].pitch, Some(FontPitch::Variable));
    assert_eq!(fonts[0].sig.usb0.as_deref(), Some("E4002EFF"));
    assert_eq!(fonts[0].sig.csb0.as_deref(), Some("0000019F"));
    assert!(fonts[0].not_true_type);
    assert_eq!(fonts[1].name, "Symbol");
    assert!(fonts[1].alt_name.is_none() && !fonts[1].not_true_type);
}

#[test]
fn theme_font_color_and_format_schemes_are_parsed() {
    use casual_doc_model::v1::{RgbColor, SchemeColor};

    let xml = br#"<a:theme xmlns:a="urn:a"><a:themeElements>
        <a:clrScheme name="Office">
            <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
            <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
            <a:dk2><a:srgbClr val="44546A"/></a:dk2>
            <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
        </a:clrScheme>
        <a:fontScheme name="Office">
            <a:majorFont>
                <a:latin typeface="Calibri Light" panose="020F0302" pitchFamily="34" charset="0"/>
                <a:ea typeface=""/><a:cs typeface=""/>
                <a:font script="Hang" typeface="Malgun Gothic"/></a:majorFont>
            <a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont>
        </a:fontScheme>
        <a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst></a:fmtScheme>
    </a:themeElements></a:theme>"#;
    let parsed = crate::theme::parse(xml, ImportConfig::default()).unwrap();
    let scheme = parsed.font_scheme.unwrap();
    assert_eq!(scheme.major.latin.typeface, "Calibri Light");
    assert_eq!(scheme.major.latin.panose.as_deref(), Some("020F0302"));
    assert_eq!(scheme.major.latin.pitch_family.as_deref(), Some("34"));
    assert!(scheme.major.ea.typeface.is_empty());
    assert_eq!(scheme.major.script_overrides.len(), 1);
    assert_eq!(scheme.major.script_overrides[0].script, "Hang");
    assert_eq!(scheme.minor.latin.typeface, "Calibri");
    // The colour scheme is now modeled, not ignored.
    let colors = parsed.color_scheme.unwrap();
    assert_eq!(colors.name, "Office");
    match colors.dark1 {
        SchemeColor::System(system) => {
            assert_eq!(system.value, "windowText");
            assert_eq!(system.last_color, Some(RgbColor { r: 0, g: 0, b: 0 }));
        }
        other => panic!("dk1 should be a sysClr, got {other:?}"),
    }
    assert_eq!(
        colors.accent1,
        SchemeColor::Srgb(RgbColor {
            r: 0x44,
            g: 0x72,
            b: 0xC4
        })
    );
    // An unspecified slot defaults to opaque black.
    assert_eq!(
        colors.light2,
        SchemeColor::Srgb(RgbColor { r: 0, g: 0, b: 0 })
    );
    // The format scheme is retained verbatim.
    let retained = parsed.format_scheme_xml.unwrap();
    assert!(retained.contains("fillStyleLst"));
    // A theme with no scheme parts yields all-None.
    let none =
        crate::theme::parse(br#"<a:theme xmlns:a="urn:a"/>"#, ImportConfig::default()).unwrap();
    assert!(none.font_scheme.is_none());
    assert!(none.color_scheme.is_none());
    assert!(none.format_scheme_xml.is_none());
}

#[test]
fn run_named_vocabularies_are_mapped_and_unknown_values_reported() {
    use casual_doc_model::v1::{EmphasisMark, HighlightColor, VerticalAlignment};
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr>
            <w:vertAlign w:val="superscript"/><w:highlight w:val="yellow"/><w:em w:val="dot"/>
        </w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let good = import(xml);
    let p = first_run_props(&good);
    assert_eq!(p.vertical_alignment, Some(VerticalAlignment::Superscript));
    assert_eq!(p.highlight, Some(HighlightColor::Yellow));
    assert_eq!(p.emphasis, Some(EmphasisMark::Dot));

    // An unknown highlight value is reported, not mapped.
    let bad = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr><w:highlight w:val="chartreuse"/></w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let bad_import = import(bad);
    assert!(first_run_props(&bad_import).highlight.is_none());
    assert!(features(&bad_import).contains(&"highlight"));
}

#[test]
fn table_is_modeled_as_a_block_with_cell_content() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
        </w:body></w:document>"#;
    let import = import(xml);
    // The table is a first-class block; its cell paragraph is nested inside it.
    assert_eq!(import.document.body().len(), 1);
    let table = first_table(&import).expect("table modeled in body");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].cells.len(), 1);
    let cell = &table.rows[0].cells[0];
    let BlockNode::Paragraph(cell_paragraph) = &cell.blocks[0] else {
        panic!("expected a paragraph in the cell");
    };
    let InlineNode::Run(run) = &cell_paragraph.inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "cell");
    // The modeled table is not reported as unmapped.
    assert!(!features(&import).contains(&"tbl"));
}

#[test]
fn table_row_and_cell_properties_are_mapped() {
    use casual_doc_model::v1::{CellVerticalAlignment, HeightRule, RgbColor, TableLayout};
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl>
            <w:tblPr>
                <w:jc w:val="center"/>
                <w:tblW w:type="dxa" w:w="9000"/>
                <w:tblLayout w:type="fixed"/>
                <w:tblLook w:firstRow="1" w:noVBand="1"/>
                <w:shd w:val="clear" w:fill="EEEEEE"/>
            </w:tblPr>
            <w:tr>
                <w:trPr><w:trHeight w:val="500" w:hRule="atLeast"/><w:tblHeader/></w:trPr>
                <w:tc>
                    <w:tcPr>
                        <w:shd w:val="clear" w:fill="FF0000"/>
                        <w:vAlign w:val="center"/>
                        <w:noWrap/>
                        <w:textDirection w:val="tbRl"/>
                    </w:tcPr>
                    <w:p><w:r><w:t>c</w:t></w:r></w:p>
                </w:tc>
            </w:tr>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let table = first_table(&import).expect("table modeled");
    // Table properties.
    assert_eq!(table.properties.alignment, Some(Alignment::Center));
    assert_eq!(table.properties.width_twips, Some(9000));
    assert_eq!(table.properties.layout, Some(TableLayout::Fixed));
    assert!(table.properties.look.first_row);
    assert!(table.properties.look.no_v_band);
    assert!(!table.properties.look.last_row);
    assert_eq!(
        table.properties.shading.fill,
        Some(RgbColor {
            r: 0xEE,
            g: 0xEE,
            b: 0xEE
        })
    );
    // Row properties.
    let row = &table.rows[0];
    assert_eq!(row.properties.height.value_twips, Some(500));
    assert_eq!(row.properties.height.rule, Some(HeightRule::AtLeast));
    assert!(row.properties.header);
    assert!(!row.properties.cant_split);
    // Cell properties.
    let cell = &row.cells[0];
    assert_eq!(
        cell.properties.shading.fill,
        Some(RgbColor { r: 255, g: 0, b: 0 })
    );
    assert_eq!(
        cell.properties.vertical_alignment,
        Some(CellVerticalAlignment::Center)
    );
    assert!(cell.properties.no_wrap);
    assert_eq!(
        cell.properties.text_direction,
        Some(casual_doc_model::v1::TextDirection::TbRl)
    );
}

#[test]
fn floating_table_position_is_mapped_and_bounded() {
    use casual_doc_model::v1::{TableAnchor, TableXAlign};
    // A `w:tblpPr` with a named horizontal alignment, an out-of-range from-text
    // distance (clamped), and an unknown vertical anchor token (dropped). The
    // named `tblpXSpec` is kept; the invalid `vertAnchor` leaves that field None.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl>
            <w:tblPr>
                <w:tblpPr w:horzAnchor="text" w:vertAnchor="bogus"
                          w:tblpXSpec="center" w:leftFromText="99999"/>
            </w:tblPr>
            <w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc></w:tr>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let table = first_table(&import).expect("table modeled");
    let float = table
        .properties
        .float_position
        .as_ref()
        .expect("float position modeled");
    assert_eq!(float.horz_anchor, Some(TableAnchor::Text));
    assert_eq!(float.vert_anchor, None, "unknown anchor token dropped");
    assert_eq!(float.x_spec, Some(TableXAlign::Center));
    assert_eq!(float.tbl_px_twips, None, "no absolute offset given");
    assert_eq!(
        float.left_from_text_twips,
        Some(31_680),
        "from-text distance clamped to the twip bound"
    );
}

#[test]
fn cell_property_change_revision_captures_prior_without_overwriting_current() {
    // A w:tcPrChange carries a nested w:tcPr with the PRE-EDIT properties;
    // schema-ordered last, it must NOT overwrite the current cell's properties,
    // and its prior snapshot (FF0000) is modeled on `prop_change`.
    use casual_doc_model::v1::RgbColor;
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tr><w:tc>
            <w:tcPr>
                <w:shd w:val="clear" w:color="auto" w:fill="00FF00"/>
                <w:tcPrChange w:id="1" w:author="A" w:date="2020-01-01T00:00:00Z">
                    <w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="FF0000"/></w:tcPr>
                </w:tcPrChange>
            </w:tcPr>
            <w:p><w:r><w:t>c</w:t></w:r></w:p>
        </w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let cell = &first_table(&import).expect("table").rows[0].cells[0];
    assert_eq!(
        cell.properties.shading.fill,
        Some(RgbColor { r: 0, g: 255, b: 0 }),
        "current (00FF00) kept, not the historical FF0000"
    );
    let change = cell
        .properties
        .prop_change
        .as_ref()
        .expect("tcPrChange modeled");
    assert_eq!(
        change.prior.shading.fill,
        Some(RgbColor { r: 255, g: 0, b: 0 }),
        "prior (FF0000) captured on the change"
    );
    assert_eq!(change.author.as_deref(), Some("A"));
    assert_eq!(change.revision_id.as_deref(), Some("1"));
    assert!(
        !features(&import).contains(&"tcPrChange"),
        "modeled, not reported"
    );
}

#[test]
fn table_and_row_property_change_revisions_do_not_overwrite() {
    use casual_doc_model::v1::Alignment;
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl>
            <w:tblPr>
                <w:jc w:val="center"/>
                <w:tblPrChange w:id="1" w:author="A" w:date="2020-01-01T00:00:00Z">
                    <w:tblPr><w:jc w:val="start"/></w:tblPr>
                </w:tblPrChange>
            </w:tblPr>
            <w:tr>
                <w:trPr>
                    <w:tblHeader/>
                    <w:trPrChange w:id="2" w:author="A" w:date="2020-01-01T00:00:00Z">
                        <w:trPr><w:trHeight w:val="5000" w:hRule="exact"/></w:trPr>
                    </w:trPrChange>
                </w:trPr>
                <w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc>
            </w:tr>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let table = first_table(&import).expect("table");
    assert_eq!(
        table.properties.alignment,
        Some(Alignment::Center),
        "current Center kept, not the historical Start"
    );
    let row = &table.rows[0];
    assert!(row.properties.header, "current tblHeader kept");
    assert!(
        row.properties.height.value_twips.is_none(),
        "no phantom historical height"
    );
}

#[test]
fn theme_shading_is_reported_not_silently_dropped() {
    // Regression (adversarial review, major): a w:themeFill carries a visible
    // background we do not model as sRGB; it must be reported (Word emits it
    // without a duplicate w:fill).
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tr><w:tc>
            <w:tcPr><w:shd w:val="clear" w:color="auto" w:themeFill="accent1"/></w:tcPr>
            <w:p><w:r><w:t>c</w:t></w:r></w:p>
        </w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let cell = &first_table(&import).expect("table").rows[0].cells[0];
    assert!(
        cell.properties.shading.fill.is_none(),
        "theme fill not sRGB"
    );
    assert!(
        features(&import).contains(&"shd"),
        "theme shading reported, not silently dropped"
    );
}

#[test]
fn table_and_cell_borders_and_margins_are_captured_without_collision() {
    use casual_doc_model::v1::RgbColor;
    // The edge names top/start/bottom/end appear in BOTH the border container and
    // the margin container; the open scope must route each correctly.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl>
            <w:tblPr>
                <w:tblBorders>
                    <w:top w:val="single" w:sz="8" w:color="FF0000" w:space="4"/>
                    <w:insideH w:val="dotted"/>
                </w:tblBorders>
                <w:tblCellMar>
                    <w:top w:type="dxa" w:w="120"/>
                    <w:start w:type="dxa" w:w="60"/>
                </w:tblCellMar>
            </w:tblPr>
            <w:tr><w:tc>
                <w:tcPr>
                    <w:tcBorders><w:bottom w:val="double" w:sz="16"/></w:tcBorders>
                    <w:tcMar><w:end w:type="dxa" w:w="90"/></w:tcMar>
                </w:tcPr>
                <w:p><w:r><w:t>c</w:t></w:r></w:p>
            </w:tc></w:tr>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let table = first_table(&import).expect("table");
    // Table border (top) captured with size/color/space — NOT confused with the
    // margin of the same edge name.
    let top = table
        .properties
        .borders
        .top
        .as_ref()
        .expect("table top border");
    assert_eq!(top.style, "single");
    assert_eq!(top.size_eighth_points, Some(8));
    assert_eq!(top.color, Some(RgbColor { r: 255, g: 0, b: 0 }));
    assert_eq!(top.space_points, Some(4));
    assert_eq!(
        table
            .properties
            .borders
            .inside_h
            .as_ref()
            .map(|e| e.style.as_str()),
        Some("dotted")
    );
    // Table default cell margins (same top/start edge names, different container).
    assert_eq!(table.properties.cell_margins.top_twips, Some(120));
    assert_eq!(table.properties.cell_margins.start_twips, Some(60));
    // Cell-level border + margin.
    let cell = &table.rows[0].cells[0];
    assert_eq!(
        cell.properties
            .borders
            .bottom
            .as_ref()
            .map(|e| e.style.as_str()),
        Some("double")
    );
    assert_eq!(cell.properties.margins.end_twips, Some(90));
}

#[test]
fn edge_scope_does_not_leak_across_a_text_box() {
    // Regression (adversarial review): a `w:txbxContent` nested inside an open
    // border container must not let the inner table's `</w:tblBorders>` clobber
    // the outer scope — the outer table's later border must still be captured.
    // (Malformed OOXML, but the code must not silently drop the outer border.)
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:tbl><w:tblPr><w:tblBorders>
            <w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent>
                    <w:tbl><w:tblPr><w:tblBorders><w:top w:val="single"/></w:tblBorders></w:tblPr>
                        <w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>
                </w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
            <w:bottom w:val="double"/>
        </w:tblBorders></w:tblPr>
            <w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc></w:tr>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let outer = first_table(&import).expect("outer table");
    assert_eq!(
        outer
            .properties
            .borders
            .bottom
            .as_ref()
            .map(|e| e.style.as_str()),
        Some("double"),
        "outer table bottom border survives the nested text-box table"
    );
}

#[test]
fn border_edge_without_a_style_is_reported_not_modeled() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tblPr><w:tblBorders><w:top w:sz="8"/></w:tblBorders></w:tblPr>
            <w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let table = first_table(&import).expect("table");
    assert!(
        table.properties.borders.top.is_none(),
        "no style -> not modeled"
    );
    assert!(features(&import).contains(&"tblBorders"));
}

#[test]
fn degraded_table_properties_are_reported_not_silently_mapped() {
    // pct table width, a table jc=both (justify), an unknown vAlign, and a
    // patterned shd are each reported; the modeled fill is still captured.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl>
            <w:tblPr><w:tblW w:type="pct" w:w="5000"/><w:jc w:val="both"/></w:tblPr>
            <w:tr><w:tc>
                <w:tcPr><w:vAlign w:val="both"/><w:shd w:val="pct25" w:fill="00FF00"/></w:tcPr>
                <w:p><w:r><w:t>c</w:t></w:r></w:p>
            </w:tc></w:tr>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let table = first_table(&import).expect("table modeled");
    assert_eq!(table.properties.width_twips, None, "pct width not modeled");
    assert_eq!(table.properties.alignment, None, "justify not modeled");
    assert!(features(&import).contains(&"tblW"));
    assert!(features(&import).contains(&"jc"));
    assert!(features(&import).contains(&"vAlign"));
    // The patterned shd is reported but its fill is still captured (partial).
    assert!(features(&import).contains(&"shd"));
    assert_eq!(
        table.rows[0].cells[0].properties.shading.fill,
        Some(casual_doc_model::v1::RgbColor { r: 0, g: 255, b: 0 })
    );
}

#[test]
fn paragraph_direct_formatting_is_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr>
                <w:jc w:val="center"/>
                <w:ind w:left="720" w:right="360"/>
                <w:spacing w:before="120" w:after="240" w:line="360" w:lineRule="auto"/>
            </w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    let props = &paragraph(&import, 0).properties;
    assert_eq!(props.alignment, Some(Alignment::Center));
    let indentation = props.indentation.unwrap();
    assert_eq!(indentation.start_twips, Some(720));
    assert_eq!(indentation.end_twips, Some(360));
    let spacing = props.spacing.unwrap();
    assert_eq!(spacing.before_twips, Some(120));
    assert_eq!(spacing.after_twips, Some(240));
    assert_eq!(spacing.line_percent, Some(150));
    // jc/ind/spacing are mapped, so they are no longer reported.
    let features: Vec<&str> = import
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect();
    assert!(!features.contains(&"jc"));
    assert!(!features.contains(&"ind"));
    assert!(!features.contains(&"spacing"));
}

#[test]
fn paragraph_flag_and_outline_properties_are_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr>
            <w:keepNext/>
            <w:keepLines w:val="1"/>
            <w:pageBreakBefore/>
            <w:widowControl w:val="0"/>
            <w:contextualSpacing/>
            <w:suppressLineNumbers/>
            <w:outlineLvl w:val="2"/>
        </w:pPr><w:r><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let props = &paragraph(&import, 0).properties;
    assert!(props.keep_next);
    assert!(props.keep_lines);
    assert!(props.page_break_before);
    assert!(!props.widow_control, "val=0 clears the toggle");
    assert!(props.contextual_spacing);
    assert!(props.suppress_line_numbers);
    assert_eq!(props.outline_level, Some(2));
}

#[test]
fn paragraph_borders_shading_and_tabs_are_mapped() {
    use casual_doc_model::v1::{RgbColor, TabAlignment, TabLeader};
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr>
            <w:pBdr>
                <w:top w:val="single" w:sz="8" w:color="112233" w:space="4"/>
                <w:between w:val="dotted"/>
            </w:pBdr>
            <w:shd w:val="clear" w:fill="EEEEEE"/>
            <w:tabs>
                <w:tab w:val="center" w:pos="2160" w:leader="dot"/>
                <w:tab w:val="right" w:pos="4320"/>
                <w:tab w:val="clear" w:pos="100"/>
            </w:tabs>
        </w:pPr><w:r><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let p = &paragraph(&import, 0).properties;
    // Border (top) captured; between edge too.
    let top = p.borders.top.as_ref().expect("top border");
    assert_eq!(top.style, "single");
    assert_eq!(top.size_eighth_points, Some(8));
    assert_eq!(
        top.color,
        Some(RgbColor {
            r: 0x11,
            g: 0x22,
            b: 0x33
        })
    );
    assert_eq!(
        p.borders.between.as_ref().map(|e| e.style.as_str()),
        Some("dotted")
    );
    // Shading.
    assert_eq!(
        p.shading.fill,
        Some(RgbColor {
            r: 0xEE,
            g: 0xEE,
            b: 0xEE
        })
    );
    // Two modeled tab stops (the `clear` tab is reported, not modeled).
    assert_eq!(p.tabs.len(), 2);
    assert_eq!(p.tabs[0].alignment, TabAlignment::Center);
    assert_eq!(p.tabs[0].position_twips, 2160);
    assert_eq!(p.tabs[0].leader, Some(TabLeader::Dot));
    assert_eq!(p.tabs[1].alignment, TabAlignment::End);
    assert!(features(&import).contains(&"tab"), "clear tab reported");
}

#[test]
fn a_paragraph_style_pbdr_bottom_border_is_captured() {
    use casual_doc_model::v1::RgbColor;
    // Root cause: a `w:pBdr` in a STYLE's `w:pPr` is a container of edge children,
    // so the flat leaf-property reader could not read it and the whole border was
    // dropped — style-sourced heading rules never reached the paragraph. The styles
    // parser now captures the edges, so the cascade can overlay them.
    let styles = br#"<w:styles xmlns:w="urn:w">
        <w:style w:type="paragraph" w:styleId="Title">
            <w:pPr><w:pBdr>
                <w:bottom w:val="single" w:sz="8" w:space="4" w:color="4F81BD"/>
            </w:pBdr></w:pPr>
        </w:style>
    </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let style = import
        .document
        .definitions()
        .styles
        .iter()
        .map(|(_, style)| style)
        .find(|style| {
            style
                .paragraph
                .as_ref()
                .is_some_and(|p| p.borders.bottom.is_some())
        })
        .expect("the Title style carries a bottom paragraph border");
    let bottom = style
        .paragraph
        .as_ref()
        .unwrap()
        .borders
        .bottom
        .as_ref()
        .unwrap();
    assert_eq!(bottom.style, "single");
    assert_eq!(bottom.size_eighth_points, Some(8));
    assert_eq!(bottom.space_points, Some(4));
    assert_eq!(
        bottom.color,
        Some(RgbColor {
            r: 0x4F,
            g: 0x81,
            b: 0xBD
        })
    );
}

#[test]
fn paragraph_mark_rpr_shading_is_not_mapped_as_paragraph_shading() {
    // A `w:shd` inside the paragraph mark's own `w:rPr` is a RUN property, so it
    // must NOT be captured as paragraph shading (it stays reported). This is the
    // disambiguation that previously blocked paragraph shading.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:rPr><w:shd w:val="clear" w:fill="FF0000"/></w:rPr></w:pPr>
            <w:r><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(
        paragraph(&import, 0).properties.shading.fill.is_none(),
        "mark-rPr shd is not paragraph shading"
    );
    assert!(features(&import).contains(&"shd"));
}

#[test]
fn run_rpr_nested_in_a_mark_rpr_does_not_steal_the_mark_shd() {
    // Regression (review, minor): a malformed run rPr nested inside an unclosed
    // paragraph-mark rPr must not drain mark_rpr_depth, else the pilcrow's own
    // w:shd is mis-captured as paragraph shading. The close now keys on run_open.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:rPr><w:r><w:rPr/></w:r><w:shd w:fill="00FF00"/></w:rPr></w:pPr></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(
        paragraph(&import, 0).properties.shading.fill.is_none(),
        "the mark-rPr shd is not paragraph shading"
    );
}

#[test]
fn paragraph_shd_in_a_cell_is_paragraph_shading_not_cell_shading() {
    // Regression (review, minor): a paragraph-direct w:shd inside a table cell
    // must map to the paragraph, not the cell (the tblPr/tcPr shd arms now guard
    // on ppr_depth==0). Well-formed: tcPr closes before the paragraph.
    use casual_doc_model::v1::RgbColor;
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tr><w:tc>
            <w:tcPr><w:shd w:val="clear" w:fill="0000FF"/></w:tcPr>
            <w:p><w:pPr><w:shd w:val="clear" w:fill="FF0000"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let cell = &first_table(&import).expect("table").rows[0].cells[0];
    assert_eq!(
        cell.properties.shading.fill,
        Some(RgbColor { r: 0, g: 0, b: 255 }),
        "cell shd stays the cell's"
    );
    let BlockNode::Paragraph(para) = &cell.blocks[0] else {
        panic!("expected a paragraph");
    };
    assert_eq!(
        para.properties.shading.fill,
        Some(RgbColor { r: 255, g: 0, b: 0 }),
        "paragraph shd is the paragraph's, not stolen by the cell"
    );
}

#[test]
fn tabs_scope_does_not_leak_across_a_text_box() {
    // Regression (review, minor): an open w:tabs container must be saved/restored
    // across a text-box frame, so a text box inside the paragraph does not let its
    // inner finish_paragraph clear the outer in_tabs and drop the tab stop.
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p><w:pPr><w:tabs>
            <w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
            <w:tab w:val="center" w:pos="2160"/>
        </w:tabs></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert_eq!(
        paragraph(&import, 0).properties.tabs.len(),
        1,
        "the outer paragraph's tab stop survives the nested text box"
    );
}

#[test]
fn out_of_range_outline_level_is_reported_not_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:outlineLvl w:val="42"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert_eq!(paragraph(&import, 0).properties.outline_level, None);
    assert!(features(&import).contains(&"outlineLvl"));
}

#[test]
fn unmapped_paragraph_property_children_are_still_reported() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    assert!(
        import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "pStyle")
    );
    // No dangling style reference is emitted (styles are not mapped yet).
    assert_eq!(paragraph(&import, 0).properties.style_ref, None);
}

#[test]
fn styles_are_mapped_and_paragraph_style_reference_resolves() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
            <w:style w:type="paragraph" w:styleId="Heading1"><w:basedOn w:val="Normal"/>
                <w:rPr><w:b/></w:rPr></w:style>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let definitions = import.document.definitions();
    assert_eq!(definitions.styles.len(), 2);

    let style_ref = paragraph(&import, 0).properties.style_ref.unwrap();
    let heading = definitions.styles.get(&style_ref).unwrap();
    assert_eq!(heading.kind, StyleKind::Paragraph);
    assert_eq!(heading.run.as_ref().unwrap().bold, Some(true));
    let base = definitions.styles.get(&heading.based_on.unwrap()).unwrap();
    assert_eq!(base.kind, StyleKind::Paragraph);
    assert!(!features(&import).contains(&"pStyle"));
}

#[test]
fn dangling_paragraph_style_reference_is_reported_not_emitted() {
    let styles = br#"<w:styles xmlns:w="urn:w"/>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Missing"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    assert_eq!(paragraph(&import, 0).properties.style_ref, None);
    assert!(features(&import).contains(&"pStyle"));
}

#[test]
fn based_on_kind_mismatch_is_dropped_and_reported() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="H"><w:basedOn w:val="C"/></w:style>
            <w:style w:type="character" w:styleId="C"/>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="H"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let style_ref = paragraph(&import, 0).properties.style_ref.unwrap();
    assert_eq!(
        import
            .document
            .definitions()
            .styles
            .get(&style_ref)
            .unwrap()
            .based_on,
        None
    );
    assert!(features(&import).contains(&"basedOn"));
}

#[test]
fn run_style_reference_resolves() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="character" w:styleId="Strong"><w:rPr><w:b/></w:rPr></w:style>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:rStyle w:val="Strong"/></w:rPr><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert!(run.properties.style_ref.is_some());
    assert!(!features(&import).contains(&"rStyle"));
}

#[test]
fn end_to_end_with_styles_part() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let styles = br#"<w:styles xmlns:w="urn:w"><w:style w:type="paragraph" w:styleId="Heading1"><w:rPr><w:b/></w:rPr></w:style></w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Titled</w:t></w:r></w:p></w:body></w:document>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", rels.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", document_rels.as_slice()),
        ("word/styles.xml", styles.as_slice()),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let package_bytes = writer.finish().unwrap().into_inner();

    let mut package =
        DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    assert_eq!(import.document.definitions().styles.len(), 1);
    assert!(paragraph(&import, 0).properties.style_ref.is_some());
}

#[test]
fn based_on_cycle_does_not_abort_import() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="B"/></w:style>
            <w:style w:type="paragraph" w:styleId="B"><w:basedOn w:val="A"/></w:style>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    // Import succeeds (the document validated, so the basedOn graph is
    // acyclic) and the broken edge is reported.
    let import = import_with_styles(document, styles);
    assert_eq!(import.document.definitions().styles.len(), 2);
    import.document.to_json().unwrap();
    assert!(features(&import).contains(&"basedOn"));
}

#[test]
fn out_of_domain_run_size_degrades_instead_of_aborting() {
    for size in ["0", "70000"] {
        let xml = format!(
            "<w:document xmlns:w=\"urn:w\"><w:body><w:p><w:r><w:rPr>\
                 <w:sz w:val=\"{size}\"/></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"
        );
        let import = import(xml.as_bytes());
        let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
            panic!("expected run");
        };
        assert_eq!(run.text, "x");
        assert_eq!(run.properties.size_half_points, None);
    }
}

#[test]
fn run_style_reference_to_a_paragraph_style_is_rejected() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="Body"/>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:rStyle w:val="Body"/></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.properties.style_ref, None);
    assert!(features(&import).contains(&"rStyle"));
}

#[test]
fn empty_body_yields_a_single_empty_paragraph() {
    let import = import(br#"<w:document xmlns:w="urn:w"><w:body/></w:document>"#);
    assert_eq!(import.document.body().len(), 1);
    assert!(paragraph(&import, 0).inlines.is_empty());
}

#[test]
fn import_is_deterministic() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let first = import(xml).document.to_json().unwrap();
    let second = import(xml).document.to_json().unwrap();
    assert_eq!(first, second);
}

#[test]
fn dtd_bearing_xml_is_rejected() {
    let xml = br#"<!DOCTYPE w:document><w:document xmlns:w="urn:w"><w:body/></w:document>"#;
    assert_eq!(
        import_main_document_xml(xml, ImportConfig::default()),
        Err(ImportError::MalformedXml)
    );
}

#[test]
fn end_to_end_from_admitted_package() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Hello DOCX</w:t></w:r></w:p></w:body></w:document>"#;

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
    let package_bytes = writer.finish().unwrap().into_inner();

    let mut package =
        DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "Hello DOCX");
}

#[test]
fn character_spacing_in_rpr_is_the_run_metric_not_paragraph_spacing() {
    // w:spacing in rPr is character spacing (a run metric, now modeled); it must
    // map to the run's character_spacing_twips and must NOT be treated as the
    // paragraph spacing element.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr><w:spacing w:val="-20"/><w:kern w:val="28"/><w:position w:val="6"/>
            <w:lang w:val="en-US" w:eastAsia="ja-JP"/></w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let p = first_run_props(&import);
    assert_eq!(p.character_spacing_twips, Some(-20));
    assert_eq!(p.kerning_half_points, Some(28));
    assert_eq!(p.position_half_points, Some(6));
    let lang = p.language.as_ref().expect("lang modeled");
    assert_eq!(lang.value.as_deref(), Some("en-US"));
    assert_eq!(lang.east_asia.as_deref(), Some("ja-JP"));
    // The run metric is not confused with paragraph spacing.
    assert_eq!(paragraph(&import, 0).properties.spacing, None);
    assert!(!features(&import).contains(&"spacing"));
}

#[test]
fn style_metadata_is_modeled_and_truly_unmapped_constructs_are_reported() {
    // `qFormat`/`uiPriority` are now modeled (not reported); a construct we do
    // not model (`w:autoRedefine`) is still reported so nothing is silently lost.
    let styles = br#"<w:styles xmlns:w="urn:w">
        <w:style w:type="paragraph" w:styleId="A"><w:qFormat/><w:uiPriority w:val="1"/>
            <w:autoRedefine/></w:style>
    </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let (_, style) = import
        .document
        .definitions()
        .styles
        .iter()
        .next()
        .expect("one style");
    assert!(style.q_format);
    assert_eq!(style.ui_priority, Some(1));
    let feats = features(&import);
    assert!(feats.contains(&"autoRedefine"));
    assert!(!feats.contains(&"qFormat"));
    assert!(!feats.contains(&"uiPriority"));
}

#[test]
fn constructs_outside_the_body_are_reported() {
    // A theme page background is not modeled as sRGB, so it is reported (degraded)
    // rather than silently dropped.
    let xml = br#"<w:document xmlns:w="urn:w">
        <w:background w:themeColor="accent1"/>
        <w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body>
    </w:document>"#;
    let import = import(xml);
    assert!(features(&import).contains(&"background"));
}

#[test]
fn page_background_color_is_captured() {
    // A concrete `w:background@w:color` is captured on the document as an sRGB
    // page fill (and is NOT reported as an unhandled feature).
    let xml = br#"<w:document xmlns:w="urn:w">
        <w:background w:color="FFF9ED"/>
        <w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body>
    </w:document>"#;
    let import = import(xml);
    assert_eq!(
        import.document.background(),
        Some(RgbColor {
            r: 0xFF,
            g: 0xF9,
            b: 0xED
        })
    );
    assert!(!features(&import).contains(&"background"));
}

#[test]
fn cdata_text_is_captured() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t><![CDATA[hi & bye]]></w:t></w:r></w:p></w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "hi & bye");
}

#[test]
fn nested_rpr_does_not_drop_following_formatting() {
    // A malformed nested self-closing rPr must not prematurely exit run-property
    // context and silently drop the following bold.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr><w:rPr/><w:b/></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.properties.bold, Some(true));
}

#[test]
fn retention_mode_retains_source_and_marks_unmapped_preserved() {
    // The table structure is modeled; `w:tblStyle` (table-level styling) is not,
    // so it exercises the unmapped-but-preserved disposition in Retention mode.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr>
            <w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let config = ImportConfig {
        mode: ImportMode::Retention,
        ..ImportConfig::default()
    };
    let import = import_main_document_xml(xml, config).unwrap();

    // The source is retained byte-identically (tier-1 byte floor): an unedited
    // document can be reproduced verbatim.
    assert_eq!(
        import.retained_source.as_ref().unwrap().main_document,
        xml.to_vec()
    );

    // Unmapped constructs (here, table styling) are preserved rather than dropped.
    assert!(features(&import).contains(&"tblStyle"));
    assert!(!import.report.entries.is_empty());
    for entry in &import.report.entries {
        assert_eq!(entry.model_outcome, ModelOutcome::Omitted);
        assert_eq!(entry.retention_outcome, RetentionOutcome::Preserved);
    }

    // Semantic mode retains nothing and reports not-retained.
    let semantic = import_main_document_xml(xml, ImportConfig::default()).unwrap();
    assert!(semantic.retained_source.is_none());
    assert!(
        semantic
            .report
            .entries
            .iter()
            .all(|entry| entry.retention_outcome == RetentionOutcome::NotRetained)
    );
}

#[test]
fn retention_over_the_byte_ceiling_fails_closed() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let config = ImportConfig {
        mode: ImportMode::Retention,
        max_text_bytes: xml.len() - 1,
        ..ImportConfig::default()
    };
    assert_eq!(
        import_main_document_xml(xml, config),
        Err(ImportError::LimitExceeded {
            limit: "retained_bytes"
        })
    );
}

#[test]
fn retention_mode_via_package_retains_all_parts_verbatim() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>hi</w:t></w:r></w:p></w:body></w:document>"#;

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
    let package_bytes = writer.finish().unwrap().into_inner();

    let mut package =
        DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let config = ImportConfig {
        mode: ImportMode::Retention,
        ..ImportConfig::default()
    };
    let import = import_package(&mut package, config).unwrap();
    let retained = import.retained_source.unwrap();
    assert_eq!(retained.main_document, document.to_vec());
    // Every admitted part is retained byte-identically.
    assert_eq!(
        retained.parts.get("word/document.xml").map(Vec::as_slice),
        Some(document.as_slice())
    );
    assert_eq!(
        retained.parts.get("[Content_Types].xml").map(Vec::as_slice),
        Some(content_types.as_slice())
    );
    assert!(retained.parts.contains_key("_rels/.rels"));
}

#[test]
fn real_producer_libreoffice_document_imports_expected_text() {
    // A real LibreOffice-produced .docx (styles, sectPr, unicode). Locks in
    // realistic-import text extraction in CI (no soffice needed); the harness
    // separately confirms this matches LibreOffice's own text.
    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-libreoffice.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let texts = nonempty_block_texts(&import);

    assert_eq!(
        texts,
        vec![
            "OpenDoc Fidelity Sample",
            "The quick brown fox jumps over the lazy dog.",
            "Formatting: bold, italic, underline.",
            "Unicode: Cafe, resume, naive, 日本語, العربية, emoji family.",
        ]
    );
}

#[test]
fn document_properties_are_imported_from_the_package() {
    use casual_doc_model::v1::{CustomValue, HeadingPair};

    let bytes = include_bytes!("../../../fixtures/corpus/synthetic-rich-metadata.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    let properties = import.document.properties().expect("metadata imported");

    // Core (docProps/core.xml).
    let core = &properties.core;
    assert_eq!(core.title.as_deref(), Some("Annual Metadata Report"));
    assert_eq!(core.creator.as_deref(), Some("Ada Lovelace"));
    assert_eq!(
        core.keywords.as_deref(),
        Some("metadata, docprops, roundtrip")
    );
    assert_eq!(core.last_modified_by.as_deref(), Some("Grace Hopper"));
    assert_eq!(core.revision.as_deref(), Some("3"));
    assert_eq!(core.created.as_deref(), Some("2026-01-15T08:30:00Z"));
    assert_eq!(core.last_printed.as_deref(), Some("2026-07-01T00:00:00Z"));
    assert_eq!(core.content_status.as_deref(), Some("Final"));
    assert_eq!(core.language.as_deref(), Some("en-US"));
    assert_eq!(core.version.as_deref(), Some("1.2"));

    // App (docProps/app.xml), including the vt:vector groups.
    let app = &properties.app;
    assert_eq!(app.application.as_deref(), Some("OpenDoc Test Harness"));
    assert_eq!(app.app_version.as_deref(), Some("1.0000"));
    assert_eq!(app.company.as_deref(), Some("Analytical Engines Ltd"));
    assert_eq!(app.template.as_deref(), Some("Normal.dotm"));
    assert_eq!(app.total_time, Some(128));
    assert_eq!(app.pages, Some(4));
    assert_eq!(app.words, Some(3200));
    assert_eq!(app.characters_with_spaces, Some(21000));
    assert_eq!(app.doc_security, Some(0));
    assert_eq!(app.scale_crop, Some(false));
    assert_eq!(app.links_up_to_date, Some(true));
    assert_eq!(app.shared_doc, Some(false));
    assert_eq!(app.hyperlink_base.as_deref(), Some("https://example.com"));
    assert_eq!(
        app.titles_of_parts,
        vec!["Annual Metadata Report".to_owned(), "Appendix A".to_owned()]
    );
    assert_eq!(
        app.heading_pairs,
        vec![
            HeadingPair {
                name: "Title".to_owned(),
                count: 1,
            },
            HeadingPair {
                name: "Sections".to_owned(),
                count: 3,
            },
        ]
    );

    // Custom (docProps/custom.xml): the typed value set.
    let custom = &properties.custom;
    assert_eq!(custom.len(), 5);
    assert_eq!(custom[0].name, "Editor");
    assert_eq!(
        custom[0].value,
        CustomValue::Text {
            value: "Grace Hopper".to_owned()
        }
    );
    assert_eq!(custom[1].value, CustomValue::I4 { value: 7 });
    assert_eq!(
        custom[2].value,
        CustomValue::R8 {
            value: "2.5".to_owned()
        }
    );
    assert_eq!(custom[3].value, CustomValue::Bool { value: true });
    assert_eq!(
        custom[4].value,
        CustomValue::FileTime {
            value: "2026-03-01T09:00:00Z".to_owned()
        }
    );
}

#[test]
fn document_properties_discovered_by_well_known_names_without_relationships() {
    // A package whose root `_rels/.rels` declares only the main document (no
    // core/app/custom relationships) still has its docProps imported through the
    // well-known part-name fallback.
    let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#;
    let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let core = br#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Fallback Title</dc:title></cp:coreProperties>"#;
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", root_rels.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("docProps/core.xml", core.as_slice()),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let package = writer.finish().unwrap().into_inner();
    let mut package =
        DocxPackage::open(&package, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    assert_eq!(
        import
            .document
            .properties()
            .and_then(|properties| properties.core.title.as_deref()),
        Some("Fallback Title")
    );
}

#[test]
fn real_producer_table_and_lists_model_cells_and_item_text() {
    // Real LibreOffice .docx with a 2x2 table and bullet/numbered lists. The
    // table is now modeled as structure: its cell paragraphs live inside the
    // Table block (not flattened), while recursive text extraction still
    // recovers all cell and list-item content in document order.
    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-table-list.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let texts = nonempty_block_texts(&import);
    assert_eq!(
        texts,
        vec![
            "Intro paragraph.",
            "R1C1",
            "R1C2",
            "R2C1",
            "R2C2",
            "First item",
            "Second item",
            "Alpha",
            "Beta",
            "Closing paragraph.",
        ]
    );

    // The table is first-class structure in the body: a 2x2 grid of cells,
    // each cell holding its own paragraph. It is no longer reported as unmapped.
    let table = first_table(&import).expect("table modeled in body");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[1].cells.len(), 2);
    assert!(
        !import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "tbl"),
        "modeled table must not be reported as unmapped"
    );
}

#[test]
fn numbering_reference_resolves_to_a_definition() {
    let numbering = br#"<w:numbering xmlns:w="urn:w">
        <w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/>
            <w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>
        <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
    </w:numbering>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>
            <w:r><w:t>item</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import_with_numbering(document, numbering);

    let reference = paragraph(&import, 0).properties.numbering.unwrap();
    assert_eq!(reference.level, 0);
    let definitions = import.document.definitions();
    assert_eq!(definitions.abstract_numbering.len(), 1);
    assert_eq!(definitions.numbering.len(), 1);
    assert!(definitions.numbering.get(&reference.instance).is_some());
    // numFmt is now modeled level detail (the bullet format), not reported.
    let (_, abstract_num) = definitions.abstract_numbering.iter().next().unwrap();
    assert_eq!(abstract_num.levels[0].num_fmt, Some(NumberFormat::Bullet));
    assert!(!features(&import).contains(&"numFmt"));
}

#[test]
fn numbering_level_detail_is_modeled_not_reported() {
    // A level carrying numFmt/lvlText/lvlJc/suff/isLgl plus pPr (indent) and rPr
    // (bold): every piece is mapped into the level, and none is reported.
    let numbering = br#"<w:numbering xmlns:w="urn:w">
        <w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0">
            <w:start w:val="1"/>
            <w:numFmt w:val="decimal"/>
            <w:isLgl/>
            <w:suff w:val="space"/>
            <w:lvlText w:val="%1."/>
            <w:lvlJc w:val="left"/>
            <w:pPr><w:ind w:start="720" w:hanging="360"/></w:pPr>
            <w:rPr><w:b/></w:rPr>
        </w:lvl></w:abstractNum>
        <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
    </w:numbering>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>
            <w:r><w:t>item</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import_with_numbering(document, numbering);
    let definitions = import.document.definitions();
    let (_, abstract_num) = definitions.abstract_numbering.iter().next().unwrap();
    let level = &abstract_num.levels[0];
    assert_eq!(level.num_fmt, Some(NumberFormat::Decimal));
    assert_eq!(level.lvl_text.as_deref(), Some("%1."));
    assert_eq!(level.lvl_jc, Some(LevelJustification::Start));
    assert_eq!(level.suff, Some(LevelSuffix::Space));
    assert!(level.is_lgl);
    assert!(level.paragraph_properties.is_some());
    assert_eq!(level.run_properties.as_ref().unwrap().bold, Some(true));
    for feature in [
        "numFmt", "lvlText", "lvlJc", "suff", "isLgl", "pPr", "rPr", "ind", "b",
    ] {
        assert!(
            !features(&import).contains(&feature),
            "{feature} must be modeled, not reported"
        );
    }
}

#[test]
fn modeled_settings_are_captured_and_unmodeled_settings_are_reported() {
    // A settings part mixing modeled settings (header parity, default tab stop,
    // track changes, document protection, proof state, zoom, a compatSetting) with
    // an unmodeled one (`w:autoHyphenation`) plus an unmodeled `w:compat` child
    // (`w:doNotExpandShiftReturn`). The modeled ones land in the model; the two
    // unmodeled ones are reported (no silent loss).
    let settings = br#"<w:settings xmlns:w="urn:w">
        <w:writeProtection w:recommended="1"/>
        <w:zoom w:percent="150"/>
        <w:evenAndOddHeaders/>
        <w:proofState w:spelling="clean" w:grammar="dirty"/>
        <w:trackChanges/>
        <w:documentProtection w:edit="readOnly" w:enforcement="1"/>
        <w:defaultTabStop w:val="720"/>
        <w:autoHyphenation/>
        <w:compat>
            <w:compatSetting w:name="compatibilityMode" w:uri="urn:x" w:val="15"/>
            <w:doNotExpandShiftReturn/>
        </w:compat>
    </w:settings>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let import = import_with_settings(document, settings);
    let s = &import.document.definitions().settings;
    assert!(s.even_and_odd_headers);
    assert!(s.track_changes);
    assert_eq!(s.default_tab_stop, Some(720));
    assert_eq!(s.proof_state.spelling, Some(ProofState::Clean));
    assert_eq!(s.proof_state.grammar, Some(ProofState::Dirty));
    assert_eq!(s.zoom.percent, Some(150));
    assert_eq!(
        s.document_protection
            .as_ref()
            .map(|p| (p.edit, p.enforcement)),
        Some((DocumentProtectionEdit::ReadOnly, true))
    );
    assert!(s.write_protection.as_ref().is_some_and(|p| p.recommended));
    assert_eq!(s.compat.len(), 1);
    assert_eq!(s.compat[0].name, "compatibilityMode");
    // The unmodeled top-level setting and the unmodeled compat child are reported.
    assert!(features(&import).contains(&"autoHyphenation"));
    assert!(features(&import).contains(&"doNotExpandShiftReturn"));
    // The modeled compatSetting is NOT reported (it is retained as a triple).
    assert!(!features(&import).contains(&"compatSetting"));
}

#[test]
fn dangling_numbering_reference_is_reported_not_emitted() {
    let numbering = br#"<w:numbering xmlns:w="urn:w"/>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="99"/></w:numPr></w:pPr>
            <w:r><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import_with_numbering(document, numbering);
    assert_eq!(paragraph(&import, 0).properties.numbering, None);
    assert!(features(&import).contains(&"numPr"));
}

#[test]
fn body_level_section_geometry_is_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>x</w:t></w:r></w:p>
        <w:sectPr>
            <w:pgSz w:w="12240" w:h="15840"/>
            <w:pgMar w:top="1440" w:bottom="1440" w:left="1800" w:right="1800" w:header="708" w:footer="709"/>
            <w:cols w:num="2"/>
        </w:sectPr>
    </w:body></w:document>"#;
    let import = import(xml);
    let sections = &import.document.definitions().sections;
    assert_eq!(sections.len(), 1);
    let section = &sections[0];
    assert_eq!(section.page_size.width_twips, 12240);
    assert_eq!(section.page_size.height_twips, 15840);
    assert_eq!(section.page_margins.start_twips, 1800); // w:left -> start
    assert_eq!(section.page_margins.end_twips, 1800); // w:right -> end
    // w:header/w:footer band distances are captured (nested inside the margins).
    assert_eq!(section.page_margins.header_twips, Some(708));
    assert_eq!(section.page_margins.footer_twips, Some(709));
    assert_eq!(section.columns.count, 2);
    // sectPr is now mapped, so it is no longer reported.
    assert!(!features(&import).contains(&"sectPr"));
}

#[test]
fn unequal_column_widths_and_separator_are_mapped() {
    // The SDS's "narrow label + wide content" section: unequal per-column widths
    // with a separator rule.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>x</w:t></w:r></w:p>
        <w:sectPr>
            <w:pgSz w:w="12240" w:h="15840"/>
            <w:cols w:num="2" w:equalWidth="0" w:sep="1">
                <w:col w:w="3163" w:space="40"/>
                <w:col w:w="6447"/>
            </w:cols>
        </w:sectPr>
    </w:body></w:document>"#;
    let import = import(xml);
    let section = &import.document.definitions().sections[0];
    let cols = &section.columns;
    assert_eq!(cols.count, 2);
    assert_eq!(cols.equal_width, Some(false));
    assert_eq!(cols.separator, Some(true));
    assert_eq!(cols.columns.len(), 2);
    assert_eq!(cols.columns[0].width_twips, 3163);
    assert_eq!(cols.columns[0].space_twips, Some(40));
    assert_eq!(cols.columns[1].width_twips, 6447);
    assert_eq!(cols.columns[1].space_twips, None);
}

#[test]
fn image_relationships_map_to_media_references() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:drawing/></w:r></w:p></w:body></w:document>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", rels.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", document_rels.as_slice()),
        ("word/media/image1.png", b"PNGDATA".as_slice()),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let package_bytes = writer.finish().unwrap().into_inner();

    let mut package =
        DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let media = &import.document.definitions().media;
    assert_eq!(media.len(), 1);
    let (_, reference) = media.iter().next().unwrap();
    assert_eq!(reference.relationship_id, "rId7");
    assert_eq!(reference.media_type, "image/png");
    assert_eq!(reference.part_name, "word/media/image1.png");
}

/// Builds a minimal admitted DOCX package from a main-document body, the
/// main-document relationships, and any extra parts (e.g. a media binary).
fn build_package(document: &[u8], document_rels: &[u8], extra: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut write = |name: &str, bytes: &[u8]| {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    };
    write("[Content_Types].xml", content_types);
    write("_rels/.rels", rels);
    write("word/document.xml", document);
    write("word/_rels/document.xml.rels", document_rels);
    for (name, bytes) in extra {
        write(name, bytes);
    }
    writer.finish().unwrap().into_inner()
}

fn import_bytes(bytes: &[u8]) -> Import {
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    import_package(&mut package, ImportConfig::default()).unwrap()
}

const IMAGE_REL: &[u8] = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;

const DRAWING_INLINE: &str = r#"<w:drawing><wp:inline><wp:extent cx="9525" cy="19050"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rId7"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#;

#[test]
fn inline_drawing_with_embed_maps_to_a_drawing_node() {
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body><w:p><w:r>{DRAWING_INLINE}</w:r></w:p></w:body></w:document>"#
    );
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document.as_bytes(), IMAGE_REL, &media));

    let (media_id, _) = import.document.definitions().media.iter().next().unwrap();
    let inlines = &paragraph(&import, 0).inlines;
    assert_eq!(inlines.len(), 1);
    let InlineNode::Drawing(drawing) = &inlines[0] else {
        panic!("expected a drawing, got {:?}", inlines[0]);
    };
    assert_eq!(drawing.media, *media_id);
    let extent = drawing.extent.expect("extent parsed");
    assert_eq!(extent.width_emu, 9525);
    assert_eq!(extent.height_emu, 19050);
    // A resolved, fully-modeled inline drawing is mapped, not reported.
    assert!(!features(&import).contains(&"drawing"));
}

const DRAWING_ANCHOR: &str = r#"<w:drawing><wp:anchor behindDoc="1" simplePos="0"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="margin"><wp:posOffset>228600</wp:posOffset></wp:positionV><wp:extent cx="1828800" cy="1219200"/><wp:wrapNone/><wp:docPr id="1" name="Pic 1" descr="Company logo"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rId7"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing>"#;

#[test]
fn anchored_drawing_maps_to_an_anchored_drawing_node() {
    use casual_doc_model::v1::{
        HorizontalAnchor, HorizontalPosition, VerticalAnchor, VerticalPosition, WrapMode,
    };

    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body><w:p><w:r>{DRAWING_ANCHOR}</w:r></w:p></w:body></w:document>"#
    );
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document.as_bytes(), IMAGE_REL, &media));

    let (media_id, _) = import.document.definitions().media.iter().next().unwrap();
    let inlines = &paragraph(&import, 0).inlines;
    assert_eq!(inlines.len(), 1);
    let InlineNode::AnchoredDrawing(drawing) = &inlines[0] else {
        panic!("expected an anchored drawing, got {:?}", inlines[0]);
    };
    assert_eq!(drawing.media, *media_id);
    assert_eq!(drawing.extent.width_emu, 1_828_800);
    assert_eq!(drawing.extent.height_emu, 1_219_200);
    assert_eq!(
        drawing.anchor.horizontal.relative_from,
        HorizontalAnchor::Page
    );
    assert_eq!(
        drawing.anchor.horizontal.position,
        HorizontalPosition::Offset(914_400)
    );
    assert_eq!(
        drawing.anchor.vertical.relative_from,
        VerticalAnchor::Margin
    );
    assert_eq!(
        drawing.anchor.vertical.position,
        VerticalPosition::Offset(228_600)
    );
    assert_eq!(drawing.anchor.wrap, WrapMode::None);
    assert!(
        drawing.anchor.behind_doc,
        "behindDoc=\"1\" sets the z-order"
    );
    assert_eq!(drawing.descr.as_deref(), Some("Company logo"));
    // A resolved, fully-modeled anchored drawing is mapped, not reported.
    assert!(!features(&import).contains(&"drawing"));
}

#[test]
fn anchored_drawing_with_align_and_default_z_order() {
    use casual_doc_model::v1::{HorizontalAlign, HorizontalPosition, WrapMode};

    // `wp:align` instead of `wp:posOffset`, no `behindDoc`, a wrapSquare mode.
    let anchor = r#"<w:drawing><wp:anchor simplePos="0"><wp:positionH relativeFrom="margin"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="Pic 1"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rId7"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing>"#;
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body><w:p><w:r>{anchor}</w:r></w:p></w:body></w:document>"#
    );
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document.as_bytes(), IMAGE_REL, &media));

    let InlineNode::AnchoredDrawing(drawing) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected an anchored drawing");
    };
    assert_eq!(
        drawing.anchor.horizontal.position,
        HorizontalPosition::Align(HorizontalAlign::Center)
    );
    assert_eq!(drawing.anchor.wrap, WrapMode::Square);
    assert!(!drawing.anchor.behind_doc, "no behindDoc → in front");
    assert!(drawing.descr.is_none(), "no descr declared");
}

#[test]
fn wpg_group_maps_to_a_group_with_children_sized_by_their_own_extent() {
    use casual_doc_model::v1::{GroupChild, ShapeGeometry};

    // A `wpg:wgp` with a rectangle (red fill, green outline), a picture, and a
    // text box, in document order, inside a 2000000x1000000 EMU group.
    let group = r#"<w:drawing><wp:anchor behindDoc="0" relativeHeight="251659264" simplePos="0"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="2000000" cy="1000000"/><wp:wrapNone/><wp:docPr id="1" name="Group 1"/><a:graphic><a:graphicData uri="urn:wpg"><wpg:wgp><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/><a:chOff x="0" y="0"/><a:chExt cx="2000000" cy="1000000"/></a:xfrm></wpg:grpSpPr><wps:wsp><wps:cNvPr id="2" name="Rectangle"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:ln w="9525"><a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></a:ln></wps:spPr><wps:bodyPr/></wps:wsp><pic:pic><pic:nvPicPr><pic:cNvPr id="3" name="Pic"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId7"/></pic:blipFill><pic:spPr><a:xfrm><a:off x="100000" y="50000"/><a:ext cx="500000" cy="500000"/></a:xfrm><a:prstGeom prst="rect"/></pic:spPr></pic:pic><wps:wsp><wps:cNvPr id="4" name="Text Box"/><wps:spPr><a:xfrm><a:off x="200000" y="100000"/><a:ext cx="800000" cy="300000"/></a:xfrm><a:prstGeom prst="rect"/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></wpg:wgp></a:graphicData></a:graphic></wp:anchor></w:drawing>"#;
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic" xmlns:wps="urn:wps" xmlns:wpg="urn:wpg"><w:body><w:p><w:r>{group}</w:r></w:p></w:body></w:document>"#
    );
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document.as_bytes(), IMAGE_REL, &media));

    let InlineNode::Group(group) = &paragraph(&import, 0).inlines[0] else {
        panic!(
            "expected a group, got {:?}",
            paragraph(&import, 0).inlines[0]
        );
    };
    assert!(
        group.anchor.is_some(),
        "the top-level group carries the anchor"
    );
    assert_eq!(group.relative_height, Some(251_659_264));
    assert_eq!(group.extent.width_emu, 2_000_000);
    assert_eq!(group.children.len(), 3, "three children, in document order");

    // [0] the rectangle: red fill, green 9525-EMU outline.
    let GroupChild::Shape(rect) = &group.children[0] else {
        panic!("expected a shape first, got {:?}", group.children[0]);
    };
    assert_eq!(rect.geometry, ShapeGeometry::Rectangle);
    assert_eq!(rect.fill.map(|c| [c.r, c.g, c.b]), Some([255, 0, 0]));
    let stroke = rect.stroke.expect("the outline");
    assert_eq!(
        [stroke.color.r, stroke.color.g, stroke.color.b],
        [0, 255, 0]
    );
    assert_eq!(stroke.width_emu, 9525);

    // [1] the picture, sized by its OWN 500000-EMU extent (NOT the group's).
    let GroupChild::Picture(pic) = &group.children[1] else {
        panic!("expected a picture second");
    };
    assert_eq!(
        pic.extent.width_emu, 500_000,
        "picture keeps its own extent"
    );
    assert_eq!(pic.extent.height_emu, 500_000);
    assert_eq!(pic.offset.x_emu, 100_000);

    // [2] the text box with its flowed block content.
    let GroupChild::TextBox(text_box) = &group.children[2] else {
        panic!("expected a text box third");
    };
    assert_eq!(text_box.extent.width_emu, 800_000);
    assert_eq!(text_box.blocks.len(), 1, "the text box flows its paragraph");
    // The whole group is fully modeled, not reported-dropped.
    assert!(!features(&import).contains(&"drawing"));
}

#[test]
fn vml_rect_horizon_rule_maps_to_a_behind_text_shape_float() {
    use casual_doc_model::v1::{GroupChild, HorizontalAnchor, HorizontalPosition, ShapeGeometry};

    // A real horizon-rule rect from the SDS header/body: a page-relative, behind-
    // text filled rectangle inside a `w:pict`. It must become a positioned shape
    // float (a group-of-one), NOT be dropped as it was before VML paint.
    let pict = r##"<w:pict><v:rect style="position:absolute;margin-left:69.503998pt;margin-top:15.339811pt;width:456.55pt;height:.48001pt;mso-position-horizontal-relative:page;mso-position-vertical-relative:paragraph;z-index:-15728640" id="docshape10" filled="true" fillcolor="#000000" stroked="false"><v:fill type="solid"/></v:rect></w:pict>"##;
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:v="urn:v"><w:body><w:p><w:r>{pict}</w:r></w:p></w:body></w:document>"#
    );
    let import = import(document.as_bytes());

    let InlineNode::Group(group) = &paragraph(&import, 0).inlines[0] else {
        panic!(
            "expected a group-of-one shape float, got {:?}",
            paragraph(&import, 0).inlines[0]
        );
    };
    let anchor = group
        .anchor
        .expect("the shape float carries the VML anchor");
    assert!(
        anchor.behind_doc,
        "a negative z-index paints behind the text"
    );
    assert_eq!(anchor.horizontal.relative_from, HorizontalAnchor::Page);
    // 69.503998pt == 1390 twips; the offset is that in EMU (1390 * 635).
    assert_eq!(
        anchor.horizontal.position,
        HorizontalPosition::Offset(1390 * 635)
    );
    // The box is the full-width, near-zero-height rule: 9131 twips wide, 10 tall.
    assert_eq!(group.extent.width_emu, 9131 * 635);
    assert_eq!(group.extent.height_emu, 10 * 635);
    let GroupChild::Shape(shape) = &group.children[0] else {
        panic!("expected a single shape child");
    };
    assert_eq!(shape.geometry, ShapeGeometry::Rectangle);
    assert_eq!(shape.fill.map(|c| [c.r, c.g, c.b]), Some([0, 0, 0]));
    assert!(
        shape.stroke.is_none(),
        "stroked=\"false\" leaves no outline"
    );
}

#[test]
fn vml_imagedata_shape_maps_to_a_positioned_picture() {
    use casual_doc_model::v1::{HorizontalAnchor, HorizontalPosition, VerticalAnchor};

    // A positioned `v:shape` carrying `v:imagedata@r:id` must resolve through the
    // media table into an AnchoredDrawing placed at its absolute VML box (rather
    // than the old inline, size-less mapping).
    let pict = r##"<w:pict><v:shape style="position:absolute;margin-left:10pt;margin-top:20pt;width:100pt;height:50pt;z-index:-5;mso-position-horizontal-relative:page;mso-position-vertical-relative:page" type="#_x0000_t75" id="img"><v:imagedata r:id="rId7" o:title=""/></v:shape></w:pict>"##;
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:v="urn:v" xmlns:o="urn:o"><w:body><w:p><w:r>{pict}</w:r></w:p></w:body></w:document>"#
    );
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document.as_bytes(), IMAGE_REL, &media));

    let (media_id, _) = import.document.definitions().media.iter().next().unwrap();
    let InlineNode::AnchoredDrawing(drawing) = &paragraph(&import, 0).inlines[0] else {
        panic!(
            "expected a positioned picture, got {:?}",
            paragraph(&import, 0).inlines[0]
        );
    };
    assert_eq!(drawing.media, *media_id);
    // 100pt == 2000 twips, 50pt == 1000 twips.
    assert_eq!(drawing.extent.width_emu, 2000 * 635);
    assert_eq!(drawing.extent.height_emu, 1000 * 635);
    assert_eq!(
        drawing.anchor.horizontal.relative_from,
        HorizontalAnchor::Page
    );
    assert_eq!(drawing.anchor.vertical.relative_from, VerticalAnchor::Page);
    // 10pt == 200 twips from the page edge.
    assert_eq!(
        drawing.anchor.horizontal.position,
        HorizontalPosition::Offset(200 * 635)
    );
    assert!(
        drawing.anchor.behind_doc,
        "a negative z-index paints behind"
    );
}

#[test]
fn vml_textbox_maps_to_an_inline_text_box_with_flowed_content() {
    // A positioned `v:shape`/`t202` text box (an SDS header box): its
    // `w:txbxContent` flows through the shared block pipeline and is emitted as an
    // INLINE text box in document order. VML text boxes render inline rather than as
    // floats at their absolute VML position, because those box positions overlap each
    // other and the body text on real documents (the SDS content pages read as
    // overprinted mush when floated). The absolute position, box fill and border are
    // intentionally dropped — inline is the known-good, readable result.
    let pict = r##"<w:pict><v:shape style="position:absolute;margin-left:70pt;margin-top:36pt;width:157pt;height:28pt;mso-position-horizontal-relative:page;mso-position-vertical-relative:page;z-index:-16121856" type="#_x0000_t202" id="tb" filled="false" stroked="false"><v:textbox inset="0,0,0,0"><w:txbxContent><w:p><w:r><w:t>Header box</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict>"##;
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:v="urn:v"><w:body><w:p><w:r>{pict}</w:r></w:p></w:body></w:document>"#
    );
    let import = import(document.as_bytes());

    let InlineNode::TextBox(text_box) = &paragraph(&import, 0).inlines[0] else {
        panic!(
            "expected an inline text box, got {:?}",
            paragraph(&import, 0).inlines[0]
        );
    };
    assert!(
        text_box.anchor.is_none(),
        "a VML text box renders inline, not floated at its absolute VML box"
    );
    assert!(
        text_box.extent.is_none(),
        "an inline box carries no absolute extent"
    );
    assert_eq!(
        text_box.blocks.len(),
        1,
        "its txbxContent flowed one paragraph"
    );
}

#[test]
fn vml_hr_rect_maps_to_a_full_width_horizontal_rule() {
    use casual_doc_model::v1::HorizontalRuleAlign;

    // Word's "Insert → Horizontal Line": a `v:rect` with `o:hr="t"`. Its CSS
    // `width:0` is ignored (an `o:hr` spans the full content width); `height` is
    // the thickness, `fillcolor` the color, `o:hralign` the alignment. It must map
    // to a first-class inline horizontal rule, not be dropped or floated as a
    // zero-width rectangle.
    let pict = r##"<w:pict><v:rect style="width:0.0pt;height:1.5pt" o:hr="t" o:hrstd="t" o:hralign="center" fillcolor="#A0A0A0" stroked="f"/></w:pict>"##;
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:v="urn:v" xmlns:o="urn:o"><w:body><w:p><w:r>{pict}</w:r></w:p></w:body></w:document>"#
    );
    let import = import(document.as_bytes());

    let InlineNode::HorizontalRule(rule) = &paragraph(&import, 0).inlines[0] else {
        panic!(
            "expected a horizontal rule, got {:?}",
            paragraph(&import, 0).inlines[0]
        );
    };
    assert_eq!(rule.align, HorizontalRuleAlign::Center);
    // No `o:hrpct` → full width (1000 per-mille).
    assert_eq!(rule.width_permille, 1000);
    // 1.5pt == 30 twips thick, carried as EMU (30 * 635).
    assert_eq!(rule.thickness_emu, 30 * 635);
    assert_eq!(
        [rule.color.r, rule.color.g, rule.color.b],
        [0xA0, 0xA0, 0xA0]
    );
}

#[test]
fn vml_hr_rect_without_height_or_fill_uses_grey_and_default_thickness() {
    // A bare `o:hr` with no `height`/`fillcolor` falls back to Word's ~1.5pt grey
    // rule rather than vanishing.
    let pict = r##"<w:pict><v:rect style="width:0" o:hr="t"/></w:pict>"##;
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:v="urn:v" xmlns:o="urn:o"><w:body><w:p><w:r>{pict}</w:r></w:p></w:body></w:document>"#
    );
    let import = import(document.as_bytes());
    let InlineNode::HorizontalRule(rule) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a horizontal rule");
    };
    assert_eq!(rule.thickness_emu, 30 * 635);
    assert_eq!(
        [rule.color.r, rule.color.g, rule.color.b],
        [0xA0, 0xA0, 0xA0]
    );
}

#[test]
fn header_vml_text_box_with_absolute_position_is_a_positioned_float() {
    use casual_doc_model::v1::HorizontalAnchor;

    // The SDS header's date boxes: two `v:shape`/`t202` text boxes absolutely
    // positioned (page-relative, distinct `margin-left`/`top`) so they sit side by
    // side. In a header/footer part they must become floats at their VML box (so
    // the header lays them out horizontally), NOT stack inline.
    let box1 = r##"<w:pict><v:shape style="position:absolute;margin-left:257.5pt;margin-top:135.4pt;width:94.75pt;height:12pt;mso-position-horizontal-relative:page;mso-position-vertical-relative:page;z-index:-16120832" type="#_x0000_t202" id="d1" filled="false" stroked="false"><v:textbox inset="0,0,0,0"><w:txbxContent><w:p><w:r><w:t>修订日期</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict>"##;
    let header = format!(
        r#"<w:hdr xmlns:w="urn:w" xmlns:v="urn:v" xmlns:o="urn:o"><w:p><w:r>{box1}</w:r></w:p></w:hdr>"#
    );
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId2"/>
            <w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
    </w:body></w:document>"#;
    let import = import_with_header_footer(document, &[("rId2", header.as_bytes())], &[]);

    let section = &import.document.definitions().sections[0];
    let hf = import
        .document
        .definitions()
        .headers
        .get(&section.headers[0].reference)
        .expect("header definition resolves");
    let BlockNode::Paragraph(para) = &hf.blocks[0] else {
        panic!("expected a header paragraph");
    };
    let text_box = find_textbox(&para.inlines).expect("header text box is modeled");
    let anchor = text_box
        .anchor
        .expect("a positioned header text box is a float carrying its VML anchor");
    assert_eq!(anchor.horizontal.relative_from, HorizontalAnchor::Page);
    assert!(
        text_box.extent.is_some(),
        "a floating header box carries its absolute extent"
    );
    assert_eq!(tb_block_text(&text_box.blocks), "修订日期");
}

#[test]
fn body_vml_text_box_stays_inline_not_floated() {
    // The de-overlap guard: a VML text box in the BODY keeps the inline behavior
    // (no anchor), so the SDS content-page callouts do not overprint each other.
    let pict = r##"<w:pict><v:shape style="position:absolute;margin-left:70pt;margin-top:36pt;width:157pt;height:28pt;mso-position-horizontal-relative:page;mso-position-vertical-relative:page;z-index:-16121856" type="#_x0000_t202" id="tb" filled="false" stroked="false"><v:textbox inset="0,0,0,0"><w:txbxContent><w:p><w:r><w:t>Body box</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict>"##;
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:v="urn:v" xmlns:o="urn:o"><w:body><w:p><w:r>{pict}</w:r></w:p></w:body></w:document>"#
    );
    let import = import(document.as_bytes());
    let text_box =
        find_textbox(&paragraph(&import, 0).inlines).expect("body text box is modeled inline");
    assert!(
        text_box.anchor.is_none(),
        "a body VML text box stays inline (not floated at its absolute VML box)"
    );
}

#[test]
fn drawing_with_a_dangling_embed_is_reported_and_dropped() {
    // The blip references rId9, which has no relationship: no media, no node.
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:a="urn:a"><w:body><w:p><w:r><w:drawing><a:blip r:embed="rId9"/></w:drawing></w:r></w:p></w:body></w:document>"#;
    let empty_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
    let import = import_bytes(&build_package(document.as_bytes(), empty_rels, &[]));

    assert!(features(&import).contains(&"drawing"));
    assert!(
        paragraph(&import, 0)
            .inlines
            .iter()
            .all(|inline| !matches!(inline, InlineNode::Drawing(_)))
    );
}

#[test]
fn chart_drawing_maps_to_an_embedded_object_and_is_not_reported_dropped() {
    use casual_doc_model::v1::EmbeddedKind;

    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:c="urn:c"><w:body><w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="304800"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId5"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"#;
    let chart_rel = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="charts/chart1.xml"/></Relationships>"#;
    let chart = br#"<c:chartSpace xmlns:c="urn:c"/>"#;
    let import = import_bytes(&build_package(
        document.as_bytes(),
        chart_rel,
        &[("word/charts/chart1.xml", chart)],
    ));

    let inlines = &paragraph(&import, 0).inlines;
    let InlineNode::EmbeddedObject(object) = &inlines[0] else {
        panic!("expected an embedded object, got {:?}", inlines[0]);
    };
    assert_eq!(object.kind, EmbeddedKind::Chart);
    assert_eq!(object.part.relationship_id, "rId5");
    assert_eq!(object.part.part_name, "word/charts/chart1.xml");
    assert_eq!(object.extent.width_emu, 914400);
    // The chart is modeled, not reported as a dropped drawing.
    assert!(!features(&import).contains(&"drawing"));
    // The chart part is byte-preserved but NOT re-orphaned (the writer emits its
    // relationship from the node).
    assert!(
        import
            .retained_parts
            .parts
            .iter()
            .any(|part| part.part_name == "word/charts/chart1.xml")
    );
    assert!(
        !import
            .retained_parts
            .relationships
            .iter()
            .any(|rel| rel.target.contains("charts/chart1.xml"))
    );
}

const HYPERLINK_REL: &[u8] = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/docs" TargetMode="External"/></Relationships>"#;

#[test]
fn external_hyperlink_maps_to_a_hyperlink_node() {
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body><w:p><w:hyperlink r:id="rId8" w:tooltip="see docs"><w:r><w:t>docs</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#;
    let import = import_bytes(&build_package(document.as_bytes(), HYPERLINK_REL, &[]));

    let inlines = &paragraph(&import, 0).inlines;
    assert_eq!(inlines.len(), 1);
    let InlineNode::Hyperlink(link) = &inlines[0] else {
        panic!("expected a hyperlink, got {:?}", inlines[0]);
    };
    assert_eq!(
        link.target,
        HyperlinkTarget::External(casual_doc_model::v1::ExternalTarget {
            url: "https://example.com/docs".to_owned(),
        })
    );
    assert_eq!(link.tooltip.as_deref(), Some("see docs"));
    let InlineNode::Run(run) = &link.inlines[0] else {
        panic!("expected a run child");
    };
    assert_eq!(run.text, "docs");
    assert!(!features(&import).contains(&"hyperlink"));
}

#[test]
fn internal_anchor_hyperlink_maps_to_a_hyperlink_node() {
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body><w:p><w:hyperlink w:anchor="top"><w:r><w:t>top</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#;
    let empty_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
    let import = import_bytes(&build_package(document.as_bytes(), empty_rels, &[]));

    let InlineNode::Hyperlink(link) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a hyperlink");
    };
    assert_eq!(
        link.target,
        HyperlinkTarget::Internal(casual_doc_model::v1::InternalTarget {
            anchor: "top".to_owned(),
        })
    );
}

#[test]
fn unresolved_hyperlink_is_reported_and_its_text_flattened() {
    // r:id="rId9" has no relationship: the link is reported, but its text is
    // preserved as flat runs in the paragraph (never dropped).
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body><w:p><w:hyperlink r:id="rId9"><w:r><w:t>orphan</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#;
    let empty_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
    let import = import_bytes(&build_package(document.as_bytes(), empty_rels, &[]));

    assert!(features(&import).contains(&"hyperlink"));
    let inlines = &paragraph(&import, 0).inlines;
    assert!(
        inlines
            .iter()
            .all(|inline| !matches!(inline, InlineNode::Hyperlink(_)))
    );
    let InlineNode::Run(run) = &inlines[0] else {
        panic!("expected the flattened run");
    };
    assert_eq!(run.text, "orphan");
}

#[test]
fn image_inside_a_hyperlink_becomes_a_drawing_child() {
    // Proves the push_segment router: a drawing inside a hyperlink is captured
    // by the link, not the paragraph.
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body><w:p><w:hyperlink r:id="rId8"><w:r>{DRAWING_INLINE}</w:r></w:hyperlink></w:p></w:body></w:document>"#
    );
    // Two relationships: the hyperlink (rId8) and the image (rId7).
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document.as_bytes(), rels, &media));

    let InlineNode::Hyperlink(link) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a hyperlink");
    };
    assert!(matches!(link.inlines[0], InlineNode::Drawing(_)));
}

#[test]
fn real_producer_table_merges_map_grid_span_and_vertical_merge() {
    use casual_doc_model::v1::VerticalMerge;

    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-table-merges.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let table = first_table(&import).expect("table modeled in body");
    // Three grid columns (w:tblGrid), preserved in order.
    assert_eq!(
        table
            .grid
            .iter()
            .map(|column| column.width_twips)
            .collect::<Vec<_>>(),
        vec![Some(1636), Some(391), Some(436)],
    );
    assert_eq!(table.rows.len(), 3);

    // Row 1, cell 1 spans two grid columns (w:gridSpan) and carries a dxa width.
    let spanning = &table.rows[0].cells[0];
    assert_eq!(spanning.properties.grid_span, Some(2));
    assert_eq!(spanning.properties.width_twips, Some(2027));

    // Row 2 opens a vertical merge; row 3 continues it (w:vMerge).
    assert_eq!(
        table.rows[1].cells[0].properties.vertical_merge,
        Some(VerticalMerge::Restart)
    );
    assert_eq!(
        table.rows[2].cells[0].properties.vertical_merge,
        Some(VerticalMerge::Continue)
    );

    // Cell text is recoverable from the modeled structure.
    let texts = nonempty_block_texts(&import);
    assert!(texts.contains(&"Spans two columns".to_owned()));
    assert!(texts.contains(&"Spans two rows".to_owned()));

    // The modeled table is not reported as unmapped structure.
    assert!(!features(&import).contains(&"tbl"));
}

#[test]
fn nested_tables_nest_and_percent_cell_width_is_reported() {
    // A table whose cell contains a nested table; the outer cell also carries a
    // percentage width (w:tcW type="pct"), which is not modeled and is reported.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl>
            <w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid>
            <w:tr><w:tc>
                <w:tcPr><w:tcW w:w="2500" w:type="pct"/></w:tcPr>
                <w:p><w:r><w:t>outer</w:t></w:r></w:p>
                <w:tbl><w:tr><w:tc><w:p><w:r><w:t>inner</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            </w:tc></w:tr>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);

    let outer = first_table(&import).expect("outer table modeled");
    assert_eq!(outer.grid.len(), 1);
    assert_eq!(outer.grid[0].width_twips, Some(5000));
    let cell = &outer.rows[0].cells[0];
    // Percentage width is not modeled (dxa only); it stays None and is reported.
    assert_eq!(cell.properties.width_twips, None);
    assert!(features(&import).contains(&"tcW"));

    // The cell holds its paragraph and then a nested table (document order).
    assert!(matches!(cell.blocks[0], BlockNode::Paragraph(_)));
    let BlockNode::Table(inner) = &cell.blocks[1] else {
        panic!("expected a nested table as the cell's second block");
    };
    assert_eq!(inner.rows.len(), 1);
    let texts = nonempty_block_texts(&import);
    assert_eq!(texts, vec!["outer".to_owned(), "inner".to_owned()]);
}

#[test]
fn tables_nested_past_depth_bound_flatten_without_data_loss() {
    // Regression (adversarial review): a table nested past MAX_TABLE_DEPTH must
    // not corrupt the enclosing table. The over-depth subtree is suppressed and
    // its paragraph text flattens into the innermost modeled cell — no cell
    // content is silently dropped, and the model stays valid and bounded.
    use casual_doc_model::v1::MAX_TABLE_DEPTH;

    let depth = MAX_TABLE_DEPTH + 1; // 33: one level past the bound
    let mut xml = String::from(r#"<w:document xmlns:w="urn:w"><w:body>"#);
    for i in 0..depth {
        xml.push_str("<w:tbl><w:tr><w:tc>");
        xml.push_str(&format!("<w:p><w:r><w:t>L{i}</w:t></w:r></w:p>"));
    }
    for _ in 0..depth {
        xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    xml.push_str("</w:body></w:document>");

    // Imports to a valid, bounded model (validation would reject depth > bound).
    let import = import(xml.as_bytes());

    // Every level's text survives — the over-depth level flattens up, it is not
    // dropped and it does not overwrite the enclosing cell's own paragraph.
    let texts = nonempty_block_texts(&import);
    for i in 0..depth {
        assert!(texts.contains(&format!("L{i}")), "lost cell text L{i}");
    }
    // The over-depth table is reported (dispositioned), not silently absorbed.
    assert!(features(&import).contains(&"tbl"));
}

#[test]
fn malformed_table_inside_a_paragraph_does_not_desync_the_stack() {
    // A `<w:tbl>` nested inside a `<w:p>` is malformed (valid OOXML makes tables
    // and paragraphs block-level siblings). It must be suppressed WITHOUT
    // consuming the enclosing real table's `</w:tbl>`: the outer table and the
    // surrounding cell text must both survive. Regression for the adversarial
    // review's start/end asymmetry finding.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tr><w:tc>
            <w:p><w:tbl/><w:r><w:t>keep</w:t></w:r></w:p>
        </w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);

    // The real outer table is not dropped by the stray inner `</w:tbl>`.
    let table = first_table(&import).expect("outer table survives");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].cells.len(), 1);
    // Cell text after the malformed table is preserved.
    assert!(nonempty_block_texts(&import).contains(&"keep".to_owned()));
    assert!(features(&import).contains(&"tbl"));
}

/// Recursively collects run text within an inline (into hyperlinks, fields, and
/// tracked-change revisions).
fn inline_text(inline: &InlineNode, out: &mut String) {
    match inline {
        InlineNode::Run(run) => out.push_str(&run.text),
        InlineNode::Hyperlink(link) => link.inlines.iter().for_each(|c| inline_text(c, out)),
        InlineNode::Field(field) => field.inlines.iter().for_each(|c| inline_text(c, out)),
        InlineNode::Revision(revision) => revision.inlines.iter().for_each(|c| inline_text(c, out)),
        _ => {}
    }
}

#[test]
fn simple_field_maps_instruction_and_cached_result() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:fldSimple w:instr=" PAGE "><w:r><w:t>7</w:t></w:r></w:fldSimple></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Field(field) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " PAGE ");
    let mut text = String::new();
    field.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "7");
}

#[test]
fn complex_field_maps_instrtext_and_result_runs() {
    // begin -> instrText -> separate -> result -> end, spread across runs.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText> REF _Ref1 \h </w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="separate"/></w:r>
            <w:r><w:t>Section 2</w:t></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    assert_eq!(para.inlines.len(), 1);
    let InlineNode::Field(field) = &para.inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " REF _Ref1 \\h ");
    let mut text = String::new();
    field.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "Section 2");
}

#[test]
fn complex_field_without_separate_has_empty_result() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText> TIME </w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Field(field) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " TIME ");
    assert!(field.inlines.is_empty());
}

#[test]
fn complex_field_missing_end_is_flushed_without_loss() {
    // A begin/separate with no end (malformed): the field is flushed at paragraph
    // close so its cached text is not dropped.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText> PAGE </w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="separate"/></w:r>
            <w:r><w:t>3</w:t></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Field(field) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a flushed field");
    };
    assert_eq!(field.instruction, " PAGE ");
    let mut text = String::new();
    field.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "3");
}

#[test]
fn field_inside_a_hyperlink_flattens_and_is_reported() {
    // A field is not opened inside a hyperlink (wrapper-in-wrapper): its result
    // text flattens into the hyperlink and the nesting is reported.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:hyperlink w:anchor="top">
            <w:fldSimple w:instr=" PAGE "><w:r><w:t>9</w:t></w:r></w:fldSimple>
        </w:hyperlink></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Hyperlink(link) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a hyperlink");
    };
    // No nested field: the "9" flattened into the link as a plain run.
    assert!(
        link.inlines
            .iter()
            .all(|inline| !matches!(inline, InlineNode::Field(_)))
    );
    let mut text = String::new();
    link.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "9");
    assert!(features(&import).contains(&"fldSimple"));
}

#[test]
fn complex_field_display_run_before_separate_is_not_lost() {
    // Regression (adversarial review): a display run appearing before `separate`
    // (or with a missing/duplicated delimiter) must not be silently dropped.
    // Here `separate` is omitted, so "5" would previously vanish; it is now kept
    // in the cached result.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText> PAGE </w:instrText></w:r>
            <w:r><w:t>5</w:t></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Field(field) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " PAGE ");
    let mut text = String::new();
    field.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "5", "pre-separate display text must be preserved");
}

#[test]
fn nested_begin_keeps_cached_result_text() {
    // A spurious extra `begin` raises the field depth; the cached result "5"
    // must still be preserved (previously dropped because in_result stayed false).
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText>PAGE</w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="separate"/></w:r>
            <w:r><w:t>5</w:t></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    // Whatever the exact structure, the display text "5" must survive somewhere.
    let mut text = String::new();
    for inline in &paragraph(&import, 0).inlines {
        inline_text(inline, &mut text);
    }
    assert!(text.contains('5'), "cached result text must not be lost");
    // The nested begin is reported.
    assert!(features(&import).contains(&"fldChar"));
}

#[test]
fn orphan_instr_text_is_reported_not_silently_dropped() {
    // An `instrText` with no enclosing field is field code we do not model; it
    // must be reported (dispositioned), never silently discarded.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:instrText>MERGEFIELD Name</w:instrText></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(features(&import).contains(&"instrText"));
}

// ---- text boxes and alternate content ------------------------------------

fn tb_block_text(blocks: &[BlockNode]) -> String {
    fn walk_blocks(blocks: &[BlockNode], out: &mut String) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(p) => p.inlines.iter().for_each(|i| walk_inline(i, out)),
                BlockNode::Table(t) => t
                    .rows
                    .iter()
                    .for_each(|r| r.cells.iter().for_each(|c| walk_blocks(&c.blocks, out))),
                BlockNode::Sdt(sdt) => walk_blocks(&sdt.blocks, out),
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    fn walk_inline(inline: &InlineNode, out: &mut String) {
        match inline {
            InlineNode::Run(r) => out.push_str(&r.text),
            InlineNode::Hyperlink(l) => l.inlines.iter().for_each(|c| walk_inline(c, out)),
            InlineNode::Field(f) => f.inlines.iter().for_each(|c| walk_inline(c, out)),
            InlineNode::TextBox(b) => walk_blocks(&b.blocks, out),
            _ => {}
        }
    }
    let mut out = String::new();
    walk_blocks(blocks, &mut out);
    out
}

fn find_textbox(inlines: &[InlineNode]) -> Option<&casual_doc_model::v1::TextBox> {
    inlines.iter().find_map(|inline| match inline {
        InlineNode::TextBox(text_box) => Some(text_box),
        _ => None,
    })
}

#[test]
fn drawingml_text_box_is_modeled_and_does_not_corrupt_the_paragraph() {
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p>
            <w:r><w:t>Before</w:t></w:r>
            <w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/>
                <a:graphic><a:graphicData><wps:wsp>
                    <wps:spPr>
                        <a:solidFill><a:srgbClr val="112233"/></a:solidFill>
                        <a:ln w="19050"><a:solidFill><a:srgbClr val="445566"/></a:solidFill></a:ln>
                    </wps:spPr>
                    <wps:txbx>
                        <w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>
                    </wps:txbx>
                </wps:wsp></a:graphicData></a:graphic>
            </wp:inline></w:drawing></w:r>
            <w:r><w:t>After</w:t></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert_eq!(import.document.body().len(), 1);
    let para = paragraph(&import, 0);
    let outer: String = para
        .inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::Run(r) => Some(r.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(outer, "BeforeAfter");
    let text_box = find_textbox(&para.inlines).expect("text box modeled");
    assert_eq!(tb_block_text(&text_box.blocks), "Boxed");
    assert_eq!(
        text_box.extent,
        Some(casual_doc_model::v1::Extent {
            width_emu: 1_270_000,
            height_emu: 635_000,
        })
    );
    assert_eq!(
        text_box.fill,
        Some(casual_doc_model::v1::Rgba {
            r: 0x11,
            g: 0x22,
            b: 0x33,
            a: 255,
        })
    );
    assert_eq!(
        text_box.border,
        Some(casual_doc_model::v1::ShapeStroke {
            color: casual_doc_model::v1::Rgba {
                r: 0x44,
                g: 0x55,
                b: 0x66,
                a: 255,
            },
            width_emu: 19_050,
        })
    );
}

#[test]
fn image_beside_a_text_box_in_the_same_drawing_is_not_dropped() {
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps" xmlns:pic="urn:pic"><w:body>
        <w:p><w:r><w:drawing><wp:inline><wp:extent cx="100" cy="100"/><a:graphic><a:graphicData><wps:wsp>
            <a:blipFill><a:blip r:embed="rId7"/></a:blipFill>
            <wps:txbx><w:txbxContent><w:p><w:r><w:t>Caption</w:t></w:r></w:p></w:txbxContent></wps:txbx>
        </wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
    </w:body></w:document>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document, rels, &media));
    let para = paragraph(&import, 0);
    assert!(
        para.inlines
            .iter()
            .any(|i| matches!(i, InlineNode::Drawing(_))),
        "enclosing drawing image must survive"
    );
    let text_box = find_textbox(&para.inlines).expect("text box modeled");
    assert_eq!(tb_block_text(&text_box.blocks), "Caption");
}

#[test]
fn alternate_content_selects_one_branch_and_does_not_duplicate() {
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:mc="urn:mc" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps" xmlns:v="urn:v"><w:body>
        <w:p><w:r><mc:AlternateContent>
            <mc:Choice Requires="wps"><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice>
            <mc:Fallback><w:pict><v:shape><v:textbox>
                <w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>
            </v:textbox></v:shape></w:pict></mc:Fallback>
        </mc:AlternateContent></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    let boxes = para
        .inlines
        .iter()
        .filter(|i| matches!(i, InlineNode::TextBox(_)))
        .count();
    assert_eq!(
        boxes, 1,
        "alternate content must not duplicate the text box"
    );
    let text_box = find_textbox(&para.inlines).expect("text box modeled");
    assert_eq!(tb_block_text(&text_box.blocks), "Boxed");
    assert!(features(&import).contains(&"Fallback"));
}

#[test]
fn deep_tables_with_a_text_box_of_tables_import_without_hard_failure() {
    // Regression (review major): a text box restarts the table-depth budget in
    // both importer and model, so tables outside and inside a box do not sum
    // past the bound and abort the whole import.
    let mut xml = String::from(
        r#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>"#,
    );
    let outer = 20;
    for _ in 0..outer {
        xml.push_str("<w:tbl><w:tr><w:tc>");
    }
    xml.push_str("<w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx><w:txbxContent>");
    let inner = 13;
    for _ in 0..inner {
        xml.push_str("<w:tbl><w:tr><w:tc>");
    }
    xml.push_str("<w:p><w:r><w:t>deep</w:t></w:r></w:p>");
    for _ in 0..inner {
        xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    xml.push_str("</w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>");
    for _ in 0..outer {
        xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    xml.push_str("</w:body></w:document>");
    // Must import successfully (no ImportError::Model from over-summed depth).
    let import = import(xml.as_bytes());
    assert!(!import.document.body().is_empty());
}

#[test]
fn sect_pr_inside_a_text_box_is_not_a_document_section() {
    // Regression (review minor): a bare sectPr inside a text box must not push a
    // phantom SectionBoundary into the document's sections.
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx><w:txbxContent>
            <w:sectPr><w:pgSz w:w="100" w:h="100"/></w:sectPr>
            <w:p><w:r><w:t>x</w:t></w:r></w:p>
        </w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(
        import.document.definitions().sections.is_empty(),
        "sectPr inside a text box must not create a document section"
    );
    assert!(features(&import).contains(&"sectPr"));
}

// ---- footnotes / endnotes ------------------------------------------------

#[test]
fn footnote_reference_and_body_are_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>Text</w:t></w:r><w:r><w:footnoteReference w:id="2"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnotes = br#"<w:footnotes xmlns:w="urn:w">
        <w:footnote w:id="-1" w:type="separator"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>
        <w:footnote w:id="2"><w:p><w:r><w:t>The footnote body.</w:t></w:r></w:p></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);

    // The body run carries a note reference resolving to the footnote definition.
    let note_ref = paragraph(&import, 0).inlines.iter().find_map(|i| match i {
        InlineNode::NoteReference(n) => Some(n),
        _ => None,
    });
    let note_ref = note_ref.expect("footnote reference modeled");
    assert_eq!(note_ref.kind, casual_doc_model::v1::NoteKind::Footnote);
    // The footnote definition holds its body text (the separator note is skipped).
    assert_eq!(import.document.definitions().footnotes.len(), 1);
    let note = import
        .document
        .definitions()
        .footnotes
        .get(&note_ref.note)
        .expect("footnote definition resolves");
    assert_eq!(tb_block_text(&note.blocks), "The footnote body.");
}

#[test]
fn dangling_footnote_reference_is_reported_not_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:footnoteReference w:id="99"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnotes = br#"<w:footnotes xmlns:w="urn:w">
        <w:footnote w:id="2"><w:p><w:r><w:t>body</w:t></w:r></w:p></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);
    // The reference to a missing footnote id is reported, not modeled.
    assert!(
        !paragraph(&import, 0)
            .inlines
            .iter()
            .any(|i| matches!(i, InlineNode::NoteReference(_)))
    );
    assert!(features(&import).contains(&"footnoteReference"));
}

#[test]
fn endnote_reference_and_body_are_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:endnoteReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let endnotes = br#"<w:endnotes xmlns:w="urn:w">
        <w:endnote w:id="1"><w:p><w:r><w:t>An endnote.</w:t></w:r></w:p></w:endnote>
    </w:endnotes>"#;
    let import = import_with_notes(document, None, Some(endnotes));
    assert_eq!(import.document.definitions().endnotes.len(), 1);
    let note_ref = paragraph(&import, 0)
        .inlines
        .iter()
        .find_map(|i| match i {
            InlineNode::NoteReference(n) => Some(n),
            _ => None,
        })
        .expect("endnote reference modeled");
    assert_eq!(note_ref.kind, casual_doc_model::v1::NoteKind::Endnote);
    let note = import
        .document
        .definitions()
        .endnotes
        .get(&note_ref.note)
        .unwrap();
    assert_eq!(tb_block_text(&note.blocks), "An endnote.");
}

#[test]
fn footnote_containing_a_text_box_preserves_its_content() {
    // Regression (review major): closing a note must unwind text-box frames and
    // finish the restored paragraph, so a note's text box content is not dropped.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:footnoteReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnotes =
        br#"<w:footnotes xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps">
        <w:footnote w:id="1"><w:p>
            <w:r><w:t>note </w:t></w:r>
            <w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
        </w:p></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);
    let note = import
        .document
        .definitions()
        .footnotes
        .iter()
        .next()
        .map(|(_, n)| n)
        .expect("footnote");
    // Both the note run text and the text box text survive.
    assert_eq!(tb_block_text(&note.blocks), "note boxed");
}

#[test]
fn stray_body_inside_a_notes_part_is_reported_not_silently_dropped() {
    // Regression (review minor): a <w:body> in a notes part must not model then
    // discard content; it is reported instead.
    let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p/></w:body></w:document>"#;
    let footnotes = br#"<w:footnotes xmlns:w="urn:w">
        <w:body><w:p><w:r><w:t>orphan</w:t></w:r></w:p></w:body>
        <w:footnote w:id="1"><w:p><w:r><w:t>real</w:t></w:r></w:p></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);
    // Only the real footnote is modeled; the stray body is reported.
    assert_eq!(import.document.definitions().footnotes.len(), 1);
    assert!(features(&import).contains(&"body"));
}

// ---- comments ------------------------------------------------------------

#[test]
fn comment_reference_body_and_metadata_are_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:commentRangeStart w:id="1"/>
            <w:r><w:t>Reviewed</w:t></w:r>
            <w:commentRangeEnd w:id="1"/>
            <w:r><w:commentReference w:id="1"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let comments = br#"<w:comments xmlns:w="urn:w">
        <w:comment w:id="1" w:author="Ada Lovelace" w:initials="AL" w:date="2026-07-25T10:00:00Z">
            <w:p><w:r><w:t>Please clarify.</w:t></w:r></w:p>
        </w:comment>
    </w:comments>"#;
    let import = import_with_comments(document, comments);

    // The body run carries a comment reference resolving to the definition.
    let comment_ref = paragraph(&import, 0).inlines.iter().find_map(|i| match i {
        InlineNode::CommentReference(c) => Some(c),
        _ => None,
    });
    let comment_ref = comment_ref.expect("comment reference modeled");
    // The comment-range markers are modeled (not reported) and bracket the
    // commented span, each resolving to the same comment as the reference.
    let range_start = paragraph(&import, 0).inlines.iter().find_map(|i| match i {
        InlineNode::CommentRangeStart(c) => Some(c),
        _ => None,
    });
    let range_end = paragraph(&import, 0).inlines.iter().find_map(|i| match i {
        InlineNode::CommentRangeEnd(c) => Some(c),
        _ => None,
    });
    let range_start = range_start.expect("comment range start modeled");
    let range_end = range_end.expect("comment range end modeled");
    assert_eq!(range_start.comment, comment_ref.comment);
    assert_eq!(range_end.comment, comment_ref.comment);
    assert!(!features(&import).contains(&"commentRangeStart"));
    assert!(!features(&import).contains(&"commentRangeEnd"));

    assert_eq!(import.document.definitions().comments.len(), 1);
    let comment = import
        .document
        .definitions()
        .comments
        .get(&comment_ref.comment)
        .expect("comment definition resolves");
    assert_eq!(tb_block_text(&comment.blocks), "Please clarify.");
    assert_eq!(comment.author.as_deref(), Some("Ada Lovelace"));
    assert_eq!(comment.initials.as_deref(), Some("AL"));
    assert_eq!(comment.date.as_deref(), Some("2026-07-25T10:00:00Z"));
}

#[test]
fn comment_without_metadata_is_modeled_with_none_fields() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:commentReference w:id="7"/></w:r></w:p>
    </w:body></w:document>"#;
    let comments = br#"<w:comments xmlns:w="urn:w">
        <w:comment w:id="7"><w:p><w:r><w:t>No metadata.</w:t></w:r></w:p></w:comment>
    </w:comments>"#;
    let import = import_with_comments(document, comments);
    let comment = import
        .document
        .definitions()
        .comments
        .iter()
        .next()
        .map(|(_, c)| c)
        .expect("comment");
    assert_eq!(tb_block_text(&comment.blocks), "No metadata.");
    assert!(comment.author.is_none());
    assert!(comment.initials.is_none());
    assert!(comment.date.is_none());
}

#[test]
fn dangling_comment_reference_is_reported_not_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:commentReference w:id="99"/></w:r></w:p>
    </w:body></w:document>"#;
    let comments = br#"<w:comments xmlns:w="urn:w">
        <w:comment w:id="1"><w:p><w:r><w:t>body</w:t></w:r></w:p></w:comment>
    </w:comments>"#;
    let import = import_with_comments(document, comments);
    // A reference to a missing comment id is reported, not modeled.
    assert!(
        !paragraph(&import, 0)
            .inlines
            .iter()
            .any(|i| matches!(i, InlineNode::CommentReference(_)))
    );
    assert!(features(&import).contains(&"commentReference"));
}

/// Imports a document + comments part plus the three companion parts
/// (`commentsExtended`/`commentsIds`/`people`), exercising the `build_comments`
/// join directly.
fn import_with_comment_companions(
    document: &[u8],
    comments: &[u8],
    extended: &[u8],
    ids: &[u8],
    people: &[u8],
) -> Import {
    let comments = crate::PartSources {
        xml: comments.to_vec(),
        comments_extended: Some(extended.to_vec()),
        comments_ids: Some(ids.to_vec()),
        people: Some(people.to_vec()),
        ..Default::default()
    };
    import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &[],
        &[],
        Some(&comments),
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

#[test]
fn comment_companion_parts_wire_threading_and_identity() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:commentReference w:id="0"/></w:r><w:r><w:commentReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let comments = br#"<w:comments xmlns:w="urn:w" xmlns:w14="urn:w14">
        <w:comment w:id="0" w:author="Ada Lovelace"><w:p w14:paraId="00000001"><w:r><w:t>Clarify?</w:t></w:r></w:p></w:comment>
        <w:comment w:id="1" w:author="Charles Babbage"><w:p w14:paraId="00000002"><w:r><w:t>Fixed.</w:t></w:r></w:p></w:comment>
    </w:comments>"#;
    let extended = br#"<w15:commentsEx xmlns:w15="urn:w15">
        <w15:commentEx w15:paraId="00000001" w15:done="1"/>
        <w15:commentEx w15:paraId="00000002" w15:paraIdParent="00000001"/>
    </w15:commentsEx>"#;
    let ids = br#"<w16cid:commentsIds xmlns:w16cid="urn:cid">
        <w16cid:commentId w16cid:paraId="00000001" w16cid:durableId="1A2B3C4D"/>
        <w16cid:commentId w16cid:paraId="00000002" w16cid:durableId="5E6F7A8B"/>
    </w16cid:commentsIds>"#;
    let people = br#"<w15:people xmlns:w15="urn:w15">
        <w15:person w15:author="Ada Lovelace"><w15:presenceInfo w15:providerId="AD" w15:userId="S::ada::1"/></w15:person>
    </w15:people>"#;
    let import = import_with_comment_companions(document, comments, extended, ids, people);
    let defs = import.document.definitions();

    let root = defs
        .comments
        .iter()
        .map(|(_, c)| c)
        .find(|c| c.author.as_deref() == Some("Ada Lovelace"))
        .expect("root comment");
    let reply = defs
        .comments
        .iter()
        .map(|(_, c)| c)
        .find(|c| c.author.as_deref() == Some("Charles Babbage"))
        .expect("reply comment");

    assert_eq!(root.para_id.as_deref(), Some("00000001"));
    assert!(root.done);
    assert_eq!(root.durable_id.as_deref(), Some("1A2B3C4D"));
    assert_eq!(root.person.as_deref(), Some("Ada Lovelace"));
    assert_eq!(reply.parent_para_id.as_deref(), Some("00000001"));
    assert!(!reply.done);
    assert_eq!(reply.person, None);

    assert_eq!(defs.people.len(), 1);
    let presence = defs.people[0].presence.as_ref().expect("presence");
    assert_eq!(presence.provider_id, "AD");
    assert_eq!(presence.user_id, "S::ada::1");
}

#[test]
fn comment_containing_a_text_box_preserves_its_content() {
    // A comment reuses the note-container machinery, so closing it must unwind
    // text-box frames and finish the restored paragraph (content not dropped).
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:commentReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let comments =
        br#"<w:comments xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps">
        <w:comment w:id="1"><w:p>
            <w:r><w:t>see </w:t></w:r>
            <w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
        </w:p></w:comment>
    </w:comments>"#;
    let import = import_with_comments(document, comments);
    let comment = import
        .document
        .definitions()
        .comments
        .iter()
        .next()
        .map(|(_, c)| c)
        .expect("comment");
    assert_eq!(tb_block_text(&comment.blocks), "see boxed");
}

#[test]
fn oversized_comment_metadata_is_dropped_not_truncated() {
    // Author/initials over 255 bytes and date over 64 bytes are discarded so the
    // model's metadata domains hold without silent truncation of the value.
    let long_author = "a".repeat(256);
    let long_date = "2026-".to_owned() + &"0".repeat(64);
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:commentReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let comments = format!(
        r#"<w:comments xmlns:w="urn:w">
        <w:comment w:id="1" w:author="{long_author}" w:date="{long_date}">
            <w:p><w:r><w:t>c</w:t></w:r></w:p>
        </w:comment>
    </w:comments>"#
    );
    let import = import_with_comments(document, comments.as_bytes());
    let comment = import
        .document
        .definitions()
        .comments
        .iter()
        .next()
        .map(|(_, c)| c)
        .expect("comment");
    assert!(comment.author.is_none(), "oversized author dropped");
    assert!(comment.date.is_none(), "oversized date dropped");
}

#[test]
fn comment_with_an_eof_truncated_table_preserves_its_content() {
    // Regression (adversarial review, data-loss): a comment whose table is left
    // open by truncated markup (stream ends before `</w:tbl>`) must still commit
    // the table's content, not strand it in the shared TableStack. `close_note`
    // flushes any open table before taking the comment's blocks.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:commentReference w:id="0"/></w:r></w:p>
    </w:body></w:document>"#;
    // Note: no `</w:tbl>`, no `</w:comment>`, no `</w:comments>` — a clean EOF
    // truncation (quick-xml returns Eof; unclosed tags are not a mismatch error).
    let comments = br#"<w:comments xmlns:w="urn:w">
        <w:comment w:id="0"><w:tbl><w:tr><w:tc><w:p><w:r><w:t>data</w:t></w:r></w:p></w:tc></w:tr>"#;
    let import = import_with_comments(document, comments);
    let comment = import
        .document
        .definitions()
        .comments
        .iter()
        .next()
        .map(|(_, c)| c)
        .expect("comment committed despite truncation");
    // The truncated table's cell content survives (not dropped).
    assert_eq!(tb_block_text(&comment.blocks), "data");
    let has_table = comment
        .blocks
        .iter()
        .any(|b| matches!(b, BlockNode::Table(_)));
    assert!(has_table, "the open table is committed, not stranded");
}

#[test]
fn body_with_an_eof_truncated_table_preserves_its_content() {
    // Regression (adversarial review, data-loss): the same flush applies to the
    // main body parse — a table left open at EOF still commits its content.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tr><w:tc><w:p><w:r><w:t>kept</w:t></w:r></w:p></w:tc></w:tr>"#;
    let import = import(document);
    assert!(nonempty_block_texts(&import).contains(&"kept".to_owned()));
    assert!(
        import
            .document
            .body()
            .iter()
            .any(|b| matches!(b, BlockNode::Table(_))),
        "the open body table is committed at EOF"
    );
}

// ---- tracked changes (revisions) -----------------------------------------

/// Returns the first `Revision` inline in paragraph 0, if any.
fn first_revision(import: &Import) -> Option<&casual_doc_model::v1::Revision> {
    paragraph(import, 0).inlines.iter().find_map(|i| match i {
        InlineNode::Revision(r) => Some(r),
        _ => None,
    })
}

#[test]
fn inserted_run_is_modeled_as_revision_with_metadata() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:ins w:id="1" w:author="Ada" w:date="2026-07-25T00:00:00Z">
            <w:r><w:t>added</w:t></w:r>
        </w:ins></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let revision = first_revision(&import).expect("insertion modeled");
    assert_eq!(revision.kind, RevisionKind::Insertion);
    assert_eq!(revision.author.as_deref(), Some("Ada"));
    assert_eq!(revision.date.as_deref(), Some("2026-07-25T00:00:00Z"));
    assert_eq!(revision.revision_id.as_deref(), Some("1"));
    let mut text = String::new();
    inline_text(&InlineNode::Revision(revision.clone()), &mut text);
    assert_eq!(text, "added");
}

#[test]
fn deleted_run_preserves_deltext() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:del w:id="2" w:author="Bob">
            <w:r><w:delText>gone</w:delText></w:r>
        </w:del></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let revision = first_revision(&import).expect("deletion modeled");
    assert_eq!(revision.kind, RevisionKind::Deletion);
    // The deleted text is preserved verbatim in the wrapped run.
    let InlineNode::Run(run) = &revision.inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.text, "gone");
}

#[test]
fn nested_insertion_around_deletion_is_modeled() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:ins w:id="1"><w:del w:id="2">
            <w:r><w:delText>x</w:delText></w:r>
        </w:del></w:ins></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let outer = first_revision(&import).expect("outer insertion");
    assert_eq!(outer.kind, RevisionKind::Insertion);
    let InlineNode::Revision(inner) = &outer.inlines[0] else {
        panic!("expected a nested revision");
    };
    assert_eq!(inner.kind, RevisionKind::Deletion);
    let InlineNode::Run(run) = &inner.inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.text, "x");
}

#[test]
fn revision_wrapping_a_hyperlink_is_modeled() {
    // Inserting a whole link: `w:ins` wraps a `w:hyperlink`. Both the revision
    // and the hyperlink survive (the innermost-wins routing regression guard).
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:ins w:id="1"><w:hyperlink r:id="rIdLink">
            <w:r><w:t>site</w:t></w:r>
        </w:hyperlink></w:ins></w:p>
    </w:body></w:document>"#;
    let mut hyperlinks = std::collections::BTreeMap::new();
    hyperlinks.insert("rIdLink".to_owned(), "https://example.com/".to_owned());
    let import = import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &[],
        &[],
        None,
        &[],
        &hyperlinks,
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap();
    let revision = first_revision(&import).expect("insertion modeled");
    let InlineNode::Hyperlink(link) = &revision.inlines[0] else {
        panic!("expected the hyperlink inside the revision");
    };
    let mut text = String::new();
    link.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "site");
}

#[test]
fn revision_inside_a_hyperlink_is_modeled() {
    // Editing text inside an existing link: `w:hyperlink` wraps a `w:ins`. Both
    // wrappers survive, nested in the correct order.
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:hyperlink r:id="rIdLink"><w:ins w:id="1">
            <w:r><w:t>edited</w:t></w:r>
        </w:ins></w:hyperlink></w:p>
    </w:body></w:document>"#;
    let mut hyperlinks = std::collections::BTreeMap::new();
    hyperlinks.insert("rIdLink".to_owned(), "https://example.com/".to_owned());
    let import = import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &[],
        &[],
        None,
        &[],
        &hyperlinks,
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap();
    let InlineNode::Hyperlink(link) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a hyperlink");
    };
    let InlineNode::Revision(revision) = &link.inlines[0] else {
        panic!("expected a revision inside the hyperlink");
    };
    assert_eq!(revision.kind, RevisionKind::Insertion);
    let mut text = String::new();
    inline_text(&link.inlines[0], &mut text);
    assert_eq!(text, "edited");
}

#[test]
fn paragraph_mark_insertion_is_reported_and_run_property_change_is_modeled() {
    // A `w:ins` inside `w:pPr>w:rPr` (paragraph-mark insertion) is not a run
    // range: it is reported and produces no Revision node. A `w:rPrChange` on the
    // run IS modeled (its prior snapshot on `prop_change`); the text is intact.
    use casual_doc_model::v1::InlineNode;
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:pPr><w:rPr><w:ins w:id="1"/></w:rPr></w:pPr>
            <w:r><w:rPr><w:rPrChange w:id="2" w:author="A"><w:rPr/></w:rPrChange></w:rPr><w:t>body</w:t></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(
        first_revision(&import).is_none(),
        "no run-range revision is modeled"
    );
    let mut text = String::new();
    paragraph(&import, 0)
        .inlines
        .iter()
        .for_each(|i| inline_text(i, &mut text));
    assert_eq!(text, "body");
    assert!(features(&import).contains(&"ins"));
    // The run's rPrChange is modeled (empty prior), not reported.
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a run");
    };
    let change = run
        .properties
        .prop_change
        .as_ref()
        .expect("rPrChange modeled");
    assert_eq!(change.author.as_deref(), Some("A"));
    assert_eq!(
        *change.prior,
        casual_doc_model::v1::RunProperties::default()
    );
    assert!(
        !features(&import).contains(&"rPrChange"),
        "modeled, not reported"
    );
}

#[test]
fn empty_revision_is_dropped_and_reported() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:ins w:id="1"></w:ins><w:r><w:t>keep</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(first_revision(&import).is_none(), "empty range not modeled");
    assert!(features(&import).contains(&"ins"));
    // The surrounding real text is unaffected.
    let mut text = String::new();
    paragraph(&import, 0)
        .inlines
        .iter()
        .for_each(|i| inline_text(i, &mut text));
    assert_eq!(text, "keep");
}

#[test]
fn oversized_revision_metadata_is_dropped_not_truncated() {
    let long_author = "a".repeat(256);
    let xml = format!(
        r#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:ins w:id="1" w:author="{long_author}"><w:r><w:t>t</w:t></w:r></w:ins></w:p>
    </w:body></w:document>"#
    );
    let import = import(xml.as_bytes());
    let revision = first_revision(&import).expect("insertion modeled");
    assert!(revision.author.is_none(), "oversized author dropped");
}

#[test]
fn unclosed_revision_at_eof_flushes_its_runs() {
    // A `w:ins` left open by EOF-truncated markup (the stream ends before its
    // close) still commits its accumulated runs at paragraph flush — no text is
    // stranded in the wrapper stack. (A mismatched `</w:p>` inside an open
    // `w:ins` is instead rejected by quick-xml as malformed, never silent.)
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:ins w:id="1"><w:r><w:t>text</w:t></w:r>"#;
    let import = import(xml);
    let revision = first_revision(&import).expect("truncated insertion still modeled");
    let mut text = String::new();
    inline_text(&InlineNode::Revision(revision.clone()), &mut text);
    assert_eq!(text, "text", "unclosed revision's run text preserved");
}

#[test]
fn property_context_revision_marker_does_not_desync_the_enclosing_revision() {
    // Regression (adversarial review, major): a self-closing `w:ins` inside a
    // run's `w:rPr` (a run-property revision marker) is reported, not modeled —
    // and its close must NOT commit the enclosing real `w:del`. Before the
    // close-side counter, the marker's on_end committed the still-empty deletion,
    // dropping the wrapper and emitting the deleted text as a plain run.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:del w:id="1" w:author="A">
            <w:r><w:rPr><w:ins w:id="2" w:author="A"/></w:rPr><w:delText>hello</w:delText></w:r>
        </w:del></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let revision = first_revision(&import).expect("deletion still modeled");
    assert_eq!(revision.kind, RevisionKind::Deletion);
    let InlineNode::Run(run) = &revision.inlines[0] else {
        panic!("expected the deleted run inside the revision");
    };
    assert_eq!(run.text, "hello", "deleted text kept inside the deletion");
    // The property-context marker is reported.
    assert!(features(&import).contains(&"ins"));
}

#[test]
fn over_depth_nested_revision_does_not_desync_the_stack() {
    // Regression (adversarial review, major): a `w:ins` past MAX_REVISION_DEPTH is
    // refused (reported, no wrapper); its close must NOT commit the enclosing real
    // revision. Content after the refused range must stay inside its true parent.
    // MAX_REVISION_DEPTH is 8, so 9 nested `w:ins` refuse the 9th.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:ins w:id="1"><w:ins w:id="2"><w:ins w:id="3"><w:ins w:id="4">
        <w:ins w:id="5"><w:ins w:id="6"><w:ins w:id="7"><w:ins w:id="8">
        <w:ins w:id="9"><w:r><w:t>X</w:t></w:r></w:ins>
        <w:r><w:t>Y</w:t></w:r>
        </w:ins></w:ins></w:ins></w:ins></w:ins></w:ins></w:ins></w:ins></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    // Exactly one top-level revision (the outermost); both X and Y are inside the
    // revision nest, not escaped to the paragraph as bare runs.
    let revisions: Vec<_> = paragraph(&import, 0)
        .inlines
        .iter()
        .filter(|i| matches!(i, InlineNode::Revision(_)))
        .collect();
    assert_eq!(
        revisions.len(),
        1,
        "one top-level revision, no escaped runs"
    );
    let bare_runs = paragraph(&import, 0)
        .inlines
        .iter()
        .filter(|i| matches!(i, InlineNode::Run(_)))
        .count();
    assert_eq!(bare_runs, 0, "no tracked run escaped to the paragraph");
    // Both X and Y are recoverable from within the revision nest.
    let mut text = String::new();
    inline_text(&paragraph(&import, 0).inlines[0], &mut text);
    assert_eq!(text, "XY", "both inserted runs stay inside the revision");
}

#[test]
fn revision_wrapping_a_text_box_preserves_box_content() {
    // A `w:ins` wraps a run whose drawing carries a text box. The text box must
    // land inside the revision (the ContentFrame suspend/restore of the revision
    // stack across `w:txbxContent`), and neither wrapper's content is lost.
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p><w:ins w:id="1">
            <w:r><w:t>see </w:t></w:r>
            <w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
        </w:ins></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let revision = first_revision(&import).expect("insertion modeled");
    // The run text and the text box both survive inside the revision.
    let has_box = revision
        .inlines
        .iter()
        .any(|i| matches!(i, InlineNode::TextBox(_)));
    assert!(has_box, "text box lands inside the revision");
    let mut text = String::new();
    inline_text(&InlineNode::Revision(revision.clone()), &mut text);
    // `inline_text` does not recurse text boxes, so only the run text shows here.
    assert_eq!(text, "see ");
}

#[test]
fn revision_inside_a_text_box_is_modeled_in_box_content() {
    // A text box whose own content contains a `w:ins`: the box parses in a fresh
    // context, so the revision is modeled inside the box's blocks, not the outer
    // paragraph.
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
            <w:txbxContent><w:p><w:ins w:id="1"><w:r><w:t>added</w:t></w:r></w:ins></w:p></w:txbxContent>
        </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    // The outer paragraph has no revision; the box holds it.
    assert!(first_revision(&import).is_none());
    let InlineNode::TextBox(text_box) = paragraph(&import, 0)
        .inlines
        .iter()
        .find(|i| matches!(i, InlineNode::TextBox(_)))
        .expect("text box")
    else {
        unreachable!()
    };
    let BlockNode::Paragraph(inner) = &text_box.blocks[0] else {
        panic!("expected a paragraph in the box");
    };
    assert!(
        inner
            .inlines
            .iter()
            .any(|i| matches!(i, InlineNode::Revision(_))),
        "the revision is modeled inside the box content"
    );
}

// ---- tracked moves -------------------------------------------------------

/// Collects the move range markers in paragraph 0, in document order.
fn move_markers(import: &Import) -> Vec<&InlineNode> {
    paragraph(import, 0)
        .inlines
        .iter()
        .filter(|i| {
            matches!(
                i,
                InlineNode::MoveRangeStart(_) | InlineNode::MoveRangeEnd(_)
            )
        })
        .collect()
}

#[test]
fn move_from_and_move_to_wrappers_map_to_move_revision_kinds() {
    // The `w:moveFrom` source wrapper (its runs are `w:delText`) and the
    // `w:moveTo` destination wrapper (its runs are `w:t`) map to the two move
    // revision kinds, retaining author/date/id and preserving their runs.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:moveFrom w:id="4" w:author="Ada" w:date="2026-07-25T00:00:00Z">
            <w:r><w:delText>moved text</w:delText></w:r>
        </w:moveFrom></w:p>
        <w:p><w:moveTo w:id="5" w:author="Ada" w:date="2026-07-25T00:00:00Z">
            <w:r><w:t>moved text</w:t></w:r>
        </w:moveTo></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Revision(from) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a moveFrom revision");
    };
    assert_eq!(from.kind, RevisionKind::MoveFrom);
    assert_eq!(from.author.as_deref(), Some("Ada"));
    assert_eq!(from.revision_id.as_deref(), Some("4"));
    let InlineNode::Run(run) = &from.inlines[0] else {
        panic!("moveFrom run not preserved (flattened?)");
    };
    assert_eq!(run.text, "moved text");

    let InlineNode::Revision(to) = &paragraph(&import, 1).inlines[0] else {
        panic!("expected a moveTo revision");
    };
    assert_eq!(to.kind, RevisionKind::MoveTo);
    assert_eq!(to.revision_id.as_deref(), Some("5"));
    let InlineNode::Run(run) = &to.inlines[0] else {
        panic!("moveTo run not preserved (flattened?)");
    };
    assert_eq!(run.text, "moved text");
}

#[test]
fn move_range_markers_are_modeled_with_name_and_pairing_id() {
    // The four range markers around a move carry their pairing `w:id` and (on the
    // start) the correlating `w:name` + author/date; the ends carry only the id.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:moveFromRangeStart w:id="0" w:name="move1" w:author="Ada" w:date="2026-07-25T00:00:00Z"/>
            <w:moveFrom w:id="1"><w:r><w:delText>x</w:delText></w:r></w:moveFrom>
            <w:moveFromRangeEnd w:id="0"/>
        </w:p>
        <w:p>
            <w:moveToRangeStart w:id="2" w:name="move1"/>
            <w:moveTo w:id="3"><w:r><w:t>x</w:t></w:r></w:moveTo>
            <w:moveToRangeEnd w:id="2"/>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::MoveRangeStart(from_start) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected moveFromRangeStart");
    };
    assert_eq!(from_start.kind, MoveKind::From);
    assert_eq!(from_start.move_id, "0");
    assert_eq!(from_start.name, "move1");
    assert_eq!(from_start.author.as_deref(), Some("Ada"));
    assert_eq!(from_start.date.as_deref(), Some("2026-07-25T00:00:00Z"));
    let InlineNode::MoveRangeEnd(from_end) = &paragraph(&import, 0).inlines[2] else {
        panic!("expected moveFromRangeEnd");
    };
    assert_eq!(from_end.kind, MoveKind::From);
    assert_eq!(from_end.move_id, "0");

    let InlineNode::MoveRangeStart(to_start) = &paragraph(&import, 1).inlines[0] else {
        panic!("expected moveToRangeStart");
    };
    assert_eq!(to_start.kind, MoveKind::To);
    assert_eq!(to_start.name, "move1");
    assert_eq!(to_start.author, None);
}

#[test]
fn move_range_start_without_name_is_reported_not_modeled() {
    // A range start missing its required `w:name` is reported and dropped; the
    // paired end (id only) still models faithfully as an orphan marker.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:moveFromRangeStart w:id="0"/>
            <w:r><w:t>body</w:t></w:r>
            <w:moveFromRangeEnd w:id="0"/>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(features(&import).contains(&"moveFromRangeStart"));
    let markers = move_markers(&import);
    assert_eq!(markers.len(), 1, "only the end marker is modeled");
    assert!(matches!(markers[0], InlineNode::MoveRangeEnd(_)));
}

// ---- headers / footers ---------------------------------------------------

fn import_with_header_footer(
    document: &[u8],
    headers: &[(&str, &[u8])],
    footers: &[(&str, &[u8])],
) -> Import {
    let headers: Vec<(String, crate::PartSources)> = headers
        .iter()
        .map(|(id, xml)| ((*id).to_owned(), part_sources(xml)))
        .collect();
    let footers: Vec<(String, crate::PartSources)> = footers
        .iter()
        .map(|(id, xml)| ((*id).to_owned(), part_sources(xml)))
        .collect();
    import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &headers,
        &footers,
        None,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

#[test]
fn header_reference_and_body_are_modeled() {
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId2"/>
            <w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
    </w:body></w:document>"#;
    let header = br#"<w:hdr xmlns:w="urn:w"><w:p><w:r><w:t>Page header</w:t></w:r></w:p></w:hdr>"#;
    let import = import_with_header_footer(document, &[("rId2", header)], &[]);

    // The header part is modeled and the section references it by kind.
    assert_eq!(import.document.definitions().headers.len(), 1);
    let section = &import.document.definitions().sections[0];
    assert_eq!(section.headers.len(), 1);
    assert_eq!(
        section.headers[0].kind,
        casual_doc_model::v1::HeaderFooterKind::Default
    );
    let hf = import
        .document
        .definitions()
        .headers
        .get(&section.headers[0].reference)
        .expect("header definition resolves");
    assert_eq!(tb_block_text(&hf.blocks), "Page header");
}

#[test]
fn footer_reference_of_first_kind_is_modeled() {
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:footerReference w:type="first" r:id="rId3"/></w:sectPr>
    </w:body></w:document>"#;
    let footer = br#"<w:ftr xmlns:w="urn:w"><w:p><w:r><w:t>Footer</w:t></w:r></w:p></w:ftr>"#;
    let import = import_with_header_footer(document, &[], &[("rId3", footer)]);
    let section = &import.document.definitions().sections[0];
    assert_eq!(section.footers.len(), 1);
    assert_eq!(
        section.footers[0].kind,
        casual_doc_model::v1::HeaderFooterKind::First
    );
    let hf = import
        .document
        .definitions()
        .footers
        .get(&section.footers[0].reference)
        .unwrap();
    assert_eq!(tb_block_text(&hf.blocks), "Footer");
}

#[test]
fn header_reference_to_a_missing_part_is_reported() {
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId9"/></w:sectPr>
    </w:body></w:document>"#;
    // No header part with rId9 is supplied.
    let import = import_with_header_footer(document, &[], &[]);
    let section = &import.document.definitions().sections[0];
    assert!(section.headers.is_empty());
    assert!(features(&import).contains(&"headerReference"));
}

#[test]
fn real_producer_header_footer_fixture_models_both() {
    // End-to-end on a real LibreOffice .docx: its sectPr references a header and
    // a footer part, both of which are modeled with their content.
    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    let defs = import.document.definitions();
    assert_eq!(defs.headers.len(), 1, "header part modeled");
    assert_eq!(defs.footers.len(), 1, "footer part modeled");
    // The body's section references them.
    let section = defs.sections.last().expect("a section boundary");
    assert!(!section.headers.is_empty(), "section references a header");
    assert!(!section.footers.is_empty(), "section references a footer");
}

#[test]
fn stray_sect_pr_and_body_inside_a_header_part_are_reported() {
    // Regression (review): a stray w:sectPr / w:body inside a header part must be
    // reported (not build a discarded phantom section or set in_body silently).
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId2"/></w:sectPr>
    </w:body></w:document>"#;
    let header = br#"<w:hdr xmlns:w="urn:w">
        <w:p><w:r><w:t>H</w:t></w:r></w:p>
        <w:sectPr><w:pgSz w:w="100" w:h="100"/></w:sectPr>
        <w:body><w:p><w:r><w:t>orphan</w:t></w:r></w:p></w:body>
    </w:hdr>"#;
    let import = import_with_header_footer(document, &[("rId2", header)], &[]);
    // The document has exactly one real section (the body's); the header's stray
    // sectPr did not add a phantom one.
    assert_eq!(import.document.definitions().sections.len(), 1);
    // Both stray constructs are reported.
    assert!(features(&import).contains(&"sectPr"));
    assert!(features(&import).contains(&"body"));
}

// ---- legacy VML pictures -------------------------------------------------

#[test]
fn vml_pict_image_becomes_a_drawing() {
    // A legacy VML picture (w:pict > v:imagedata@r:id) resolves through the same
    // media table as a DrawingML picture and becomes a Drawing.
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:v="urn:v"><w:body>
        <w:p><w:r><w:pict><v:shape style="width:10pt"><v:imagedata r:id="rId5"/></v:shape></w:pict></w:r></w:p>
    </w:body></w:document>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document, rels, &media));
    let drawing = paragraph(&import, 0)
        .inlines
        .iter()
        .find_map(|i| match i {
            InlineNode::Drawing(d) => Some(d),
            _ => None,
        })
        .expect("VML image modeled as a Drawing");
    // VML sizes in CSS, not EMU, so no extent is captured.
    assert!(drawing.extent.is_none());
}

#[test]
fn vml_pict_with_unresolved_image_is_reported() {
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:v="urn:v"><w:body>
        <w:p><w:r><w:pict><v:shape><v:imagedata r:id="rId9"/></v:shape></w:pict></w:r></w:p>
    </w:body></w:document>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#;
    let import = import_bytes(&build_package(document, rels, &[]));
    assert!(
        !paragraph(&import, 0)
            .inlines
            .iter()
            .any(|i| matches!(i, InlineNode::Drawing(_)))
    );
    assert!(features(&import).contains(&"pict"));
}

// ---- media / hyperlinks inside extra parts -------------------------------

fn image_source(rid: &str, part: &str) -> crate::MediaSource {
    crate::MediaSource {
        relationship_id: rid.to_owned(),
        media_type: "image/png".to_owned(),
        part_name: part.to_owned(),
    }
}

#[test]
fn image_inside_a_footnote_is_modeled_via_the_notes_part_relationships() {
    // A footnote body with a DrawingML picture whose r:embed resolves through the
    // footnotes part's OWN image relationship.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:footnoteReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnote_xml = format!(
        r#"<w:footnotes xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic">
            <w:footnote w:id="1"><w:p><w:r>{DRAWING_INLINE}</w:r></w:p></w:footnote>
        </w:footnotes>"#
    );
    let footnotes = crate::PartSources {
        xml: footnote_xml.into_bytes(),
        images: vec![image_source("rId7", "word/media/image1.png")],
        hyperlinks: std::collections::BTreeMap::new(),
        ..Default::default()
    };
    let import = import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        Some(&footnotes),
        None,
        &[],
        &[],
        None,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap();

    // The footnote's block content contains a Drawing referencing shared media.
    let note = import
        .document
        .definitions()
        .footnotes
        .iter()
        .next()
        .map(|(_, n)| n)
        .unwrap();
    let has_drawing = note.blocks.iter().any(|b| matches!(
        b, BlockNode::Paragraph(p) if p.inlines.iter().any(|i| matches!(i, InlineNode::Drawing(_)))
    ));
    assert!(has_drawing, "image inside footnote modeled as a Drawing");
    assert_eq!(
        import.document.definitions().media.len(),
        1,
        "note-part image in the media table"
    );
}

#[test]
fn external_hyperlink_inside_a_header_is_modeled_via_the_header_part_relationships() {
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId2"/></w:sectPr>
    </w:body></w:document>"#;
    let header = br#"<w:hdr xmlns:w="urn:w" xmlns:r="urn:r">
        <w:p><w:hyperlink r:id="rIdLink"><w:r><w:t>site</w:t></w:r></w:hyperlink></w:p>
    </w:hdr>"#;
    let mut hyperlinks = std::collections::BTreeMap::new();
    hyperlinks.insert("rIdLink".to_owned(), "https://example.com/".to_owned());
    let header_part = crate::PartSources {
        xml: header.to_vec(),
        images: Vec::new(),
        hyperlinks,
        ..Default::default()
    };
    let import = import_with_sources(
        document,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &[("rId2".to_owned(), header_part)],
        &[],
        None,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap();

    let hf = import
        .document
        .definitions()
        .headers
        .iter()
        .next()
        .map(|(_, h)| h)
        .unwrap();
    let has_link = hf.blocks.iter().any(|b| matches!(
        b, BlockNode::Paragraph(p) if p.inlines.iter().any(|i| matches!(i, InlineNode::Hyperlink(_)))
    ));
    assert!(has_link, "external hyperlink inside a header is modeled");
}

#[test]
fn two_relationships_to_the_same_image_get_distinct_media_ids() {
    // Backward-compat (review): media is allocated per relationship (no dedup),
    // so a document referencing one image part from two relationships yields two
    // media entries, exactly as before extra-part media aggregation.
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body>
        <w:p><w:r><w:pict><v:imagedata r:id="rId5"/></w:pict></w:r>
             <w:r><w:pict><v:imagedata r:id="rId8"/></w:pict></w:r></w:p>
    </w:body></w:document>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document, rels, &media));
    assert_eq!(
        import.document.definitions().media.len(),
        2,
        "one media entry per relationship (no dedup)"
    );
}

// ---- ruby (phonetic guides) ----------------------------------------------

#[test]
fn ruby_keeps_base_text_in_order_and_reports_the_annotation() {
    // Audit fix: the base reads in document order; the annotation (phonetic
    // guide) is dropped and reported, not merged before the base.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>A</w:t></w:r>
             <w:r><w:ruby><w:rubyPr/>
                 <w:rt><w:r><w:t>anno</w:t></w:r></w:rt>
                 <w:rubyBase><w:r><w:t>base</w:t></w:r></w:rubyBase>
             </w:ruby></w:r>
             <w:r><w:t>B</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    // The paragraph reads "A" + "base" + "B" — annotation is not present/merged.
    let text: String = paragraph(&import, 0)
        .inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::Run(r) => Some(r.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "AbaseB");
    assert!(
        !text.contains("anno"),
        "annotation must not be modeled as base text"
    );
    assert!(
        features(&import).contains(&"rt"),
        "ruby annotation is reported"
    );
}

#[test]
fn nested_ruby_annotation_does_not_leak_base_text() {
    // Regression (review): a nested w:rt (valid OOXML) must not clear the outer
    // annotation early via a bool; a counter keeps all annotation text dropped.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:ruby>
            <w:rt>
                <w:r><w:ruby>
                    <w:rt><w:r><w:t>inner</w:t></w:r></w:rt>
                    <w:rubyBase><w:r><w:t>ib</w:t></w:r></w:rubyBase>
                </w:ruby></w:r>
                <w:r><w:t>outer-anno</w:t></w:r>
            </w:rt>
            <w:rubyBase><w:r><w:t>BASE</w:t></w:r></w:rubyBase>
        </w:ruby></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let text: String = paragraph(&import, 0)
        .inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::Run(r) => Some(r.text.as_str()),
            _ => None,
        })
        .collect();
    // Only the outermost base is kept; no annotation fragment leaks in.
    assert_eq!(text, "BASE");
    assert!(!text.contains("anno") && !text.contains("inner") && !text.contains("ib"));
}

// ---- bookmarks -----------------------------------------------------------

fn bookmark_start(para: &Paragraph) -> Option<&casual_doc_model::v1::BookmarkStart> {
    para.inlines.iter().find_map(|i| match i {
        InlineNode::BookmarkStart(b) => Some(b),
        _ => None,
    })
}

fn bookmark_end(para: &Paragraph) -> Option<&casual_doc_model::v1::BookmarkEnd> {
    para.inlines.iter().find_map(|i| match i {
        InlineNode::BookmarkEnd(b) => Some(b),
        _ => None,
    })
}

#[test]
fn inline_bookmark_pair_is_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:bookmarkStart w:id="1" w:name="anchor"/>
            <w:r><w:t>hello</w:t></w:r>
            <w:bookmarkEnd w:id="1"/>
        </w:p>
    </w:body></w:document>"#;
    let import = import(document);
    let para = paragraph(&import, 0);
    let start = bookmark_start(para).expect("start modeled");
    let end = bookmark_end(para).expect("end modeled");
    assert_eq!(start.bookmark, end.bookmark);
    assert_eq!(import.document.definitions().bookmarks.len(), 1);
    let bm = import
        .document
        .definitions()
        .bookmarks
        .get(&start.bookmark)
        .expect("definition resolves");
    assert_eq!(bm.name, "anchor");
    // A modeled bookmark no longer appears in the report.
    assert!(!features(&import).contains(&"bookmarkStart"));
    assert!(!features(&import).contains(&"bookmarkEnd"));
}

#[test]
fn bookmark_spanning_two_paragraphs_pairs_by_id() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:bookmarkStart w:id="1" w:name="span"/><w:r><w:t>a</w:t></w:r></w:p>
        <w:p><w:r><w:t>b</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>
    </w:body></w:document>"#;
    let import = import(document);
    let start = bookmark_start(paragraph(&import, 0)).expect("start in para 0");
    let end = bookmark_end(paragraph(&import, 1)).expect("end in para 1");
    assert_eq!(start.bookmark, end.bookmark);
    assert_eq!(import.document.definitions().bookmarks.len(), 1);
}

#[test]
fn internal_hyperlink_anchor_matching_a_bookmark_stays_lax() {
    // A hyperlink whose anchor equals a bookmark name is modeled unchanged; no
    // dangling-bookmark error and no spurious report (forward/lax resolution).
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:bookmarkStart w:id="1" w:name="target"/>
            <w:hyperlink w:anchor="target"><w:r><w:t>go</w:t></w:r></w:hyperlink>
            <w:bookmarkEnd w:id="1"/>
        </w:p>
    </w:body></w:document>"#;
    let import = import(document);
    let para = paragraph(&import, 0);
    let link = para.inlines.iter().find_map(|i| match i {
        InlineNode::Hyperlink(l) => Some(l),
        _ => None,
    });
    let link = link.expect("hyperlink modeled");
    assert!(matches!(
        &link.target,
        HyperlinkTarget::Internal(t) if t.anchor == "target"
    ));
    assert!(bookmark_start(para).is_some());
    assert!(!features(&import).contains(&"bookmarkStart"));
    assert!(!features(&import).contains(&"bookmarkEnd"));
}

#[test]
fn bookmark_inside_a_hyperlink_is_modeled_in_the_link() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:hyperlink w:anchor="a">
                <w:bookmarkStart w:id="1" w:name="inlink"/>
                <w:r><w:t>go</w:t></w:r>
                <w:bookmarkEnd w:id="1"/>
            </w:hyperlink>
        </w:p>
    </w:body></w:document>"#;
    let import = import(document);
    let para = paragraph(&import, 0);
    let link = para
        .inlines
        .iter()
        .find_map(|i| match i {
            InlineNode::Hyperlink(l) => Some(l),
            _ => None,
        })
        .expect("hyperlink modeled");
    // The markers land inside the hyperlink's inline stream, not the paragraph.
    assert!(
        link.inlines
            .iter()
            .any(|i| matches!(i, InlineNode::BookmarkStart(_)))
    );
    assert!(
        link.inlines
            .iter()
            .any(|i| matches!(i, InlineNode::BookmarkEnd(_)))
    );
    assert_eq!(import.document.definitions().bookmarks.len(), 1);
}

#[test]
fn orphan_bookmark_end_is_reported_and_dropped() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id="9"/></w:p>
    </w:body></w:document>"#;
    let import = import(document);
    assert!(bookmark_end(paragraph(&import, 0)).is_none());
    assert!(import.document.definitions().bookmarks.is_empty());
    assert!(features(&import).contains(&"bookmarkEnd"));
}

#[test]
fn bookmark_without_a_name_is_reported_and_dropped() {
    // A nameless start is dropped and its id is never registered, so the end
    // becomes an orphan and is also reported — balanced, no dangling reference.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:bookmarkStart w:id="1"/><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>
    </w:body></w:document>"#;
    let import = import(document);
    let para = paragraph(&import, 0);
    assert!(bookmark_start(para).is_none());
    assert!(bookmark_end(para).is_none());
    assert!(import.document.definitions().bookmarks.is_empty());
    assert!(features(&import).contains(&"bookmarkStart"));
    assert!(features(&import).contains(&"bookmarkEnd"));
}

#[test]
fn oversized_bookmark_name_is_reported_and_dropped() {
    let long = "a".repeat(256);
    let document = format!(
        r#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:bookmarkStart w:id="1" w:name="{long}"/><w:bookmarkEnd w:id="1"/></w:p>
    </w:body></w:document>"#
    );
    let import = import(document.as_bytes());
    assert!(import.document.definitions().bookmarks.is_empty());
    assert!(features(&import).contains(&"bookmarkStart"));
}

#[test]
fn duplicate_bookmark_id_keeps_the_first_and_reports_the_second() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:bookmarkStart w:id="1" w:name="one"/>
            <w:bookmarkStart w:id="1" w:name="two"/>
            <w:r><w:t>x</w:t></w:r>
            <w:bookmarkEnd w:id="1"/>
        </w:p>
    </w:body></w:document>"#;
    let import = import(document);
    let para = paragraph(&import, 0);
    let starts: Vec<_> = para
        .inlines
        .iter()
        .filter(|i| matches!(i, InlineNode::BookmarkStart(_)))
        .collect();
    assert_eq!(starts.len(), 1, "only the first start is modeled");
    assert_eq!(import.document.definitions().bookmarks.len(), 1);
    let (_, bm) = import
        .document
        .definitions()
        .bookmarks
        .iter()
        .next()
        .unwrap();
    assert_eq!(bm.name, "one");
    assert!(features(&import).contains(&"bookmarkStart"));
    // The single end pairs with the surviving first start.
    assert!(bookmark_end(para).is_some());
}

#[test]
fn reused_bookmark_id_after_close_models_both_ranges_without_a_phantom_end() {
    // Regression (adversarial review, major): a `w:id` reused after its first
    // range fully closed must not re-resolve its end onto the first bookmark
    // (which produced an unbalanced Start(A),End(A),End(A)). De-registering the
    // id on end makes the second range a fresh, balanced bookmark.
    let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p>
        <w:bookmarkStart w:id="1" w:name="a"/><w:r><w:t>X</w:t></w:r><w:bookmarkEnd w:id="1"/>
        <w:bookmarkStart w:id="1" w:name="b"/><w:r><w:t>Y</w:t></w:r><w:bookmarkEnd w:id="1"/>
    </w:p></w:body></w:document>"#;
    let import = import(document);
    let para = paragraph(&import, 0);
    let starts = para
        .inlines
        .iter()
        .filter(|i| matches!(i, InlineNode::BookmarkStart(_)))
        .count();
    let ends = para
        .inlines
        .iter()
        .filter(|i| matches!(i, InlineNode::BookmarkEnd(_)))
        .count();
    // Balanced: two distinct bookmarks, one end each — no phantom third marker.
    assert_eq!(starts, 2, "both reused-id ranges modeled");
    assert_eq!(ends, 2, "exactly one end per start; no phantom end");
    assert_eq!(import.document.definitions().bookmarks.len(), 2);
    // Every emitted marker resolves (no dangling) — the document validates.
    assert!(import.document.validate().is_ok());
}

#[test]
fn column_bookmark_is_modeled_by_name_and_reported() {
    // The column span (`w:colFirst`/`w:colLast`) is dropped but the bookmark is
    // still modeled by name/range; the dropped column attributes are reported.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:bookmarkStart w:id="1" w:name="col" w:colFirst="0" w:colLast="2"/>
            <w:r><w:t>x</w:t></w:r>
            <w:bookmarkEnd w:id="1"/>
        </w:p>
    </w:body></w:document>"#;
    let import = import(document);
    let para = paragraph(&import, 0);
    let start = bookmark_start(para).expect("column bookmark still modeled");
    let bm = import
        .document
        .definitions()
        .bookmarks
        .get(&start.bookmark)
        .expect("definition");
    assert_eq!(bm.name, "col");
    // The dropped column span is surfaced in the report.
    assert!(features(&import).contains(&"bookmarkStart"));
}

#[test]
fn block_level_bookmark_is_reported_not_modeled() {
    // A marker outside any paragraph (between blocks) is a deferred case: reported
    // and dropped this slice, never modeled.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:bookmarkStart w:id="1" w:name="blk"/>
        <w:p><w:r><w:t>x</w:t></w:r></w:p>
        <w:bookmarkEnd w:id="1"/>
    </w:body></w:document>"#;
    let import = import(document);
    assert!(import.document.definitions().bookmarks.is_empty());
    assert!(features(&import).contains(&"bookmarkStart"));
    assert!(features(&import).contains(&"bookmarkEnd"));
}

#[test]
fn bookmark_inside_a_text_box_is_modeled() {
    let document =
        br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
            <w:txbxContent><w:p>
                <w:bookmarkStart w:id="1" w:name="inbox"/>
                <w:r><w:t>boxed</w:t></w:r>
                <w:bookmarkEnd w:id="1"/>
            </w:p></w:txbxContent>
        </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(document);
    assert_eq!(import.document.definitions().bookmarks.len(), 1);
    let text_box = find_textbox(&paragraph(&import, 0).inlines).expect("text box modeled");
    let BlockNode::Paragraph(inner) = &text_box.blocks[0] else {
        panic!("expected a paragraph in the text box");
    };
    assert!(bookmark_start(inner).is_some());
    assert!(bookmark_end(inner).is_some());
}

#[test]
fn bookmark_defined_in_a_header_lands_in_document_bookmarks() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>body</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let header = br#"<w:hdr xmlns:w="urn:w"><w:p>
        <w:bookmarkStart w:id="1" w:name="hdrmark"/>
        <w:r><w:t>h</w:t></w:r>
        <w:bookmarkEnd w:id="1"/>
    </w:p></w:hdr>"#;
    let import = import_with_header_footer(document, &[("rId2", header)], &[]);
    assert_eq!(import.document.definitions().bookmarks.len(), 1);
    let (_, bm) = import
        .document
        .definitions()
        .bookmarks
        .iter()
        .next()
        .unwrap();
    assert_eq!(bm.name, "hdrmark");
}
// ---- content controls (structured document tags) -------------------------

#[test]
fn inline_sdt_with_a_dangling_field_inside_does_not_panic_or_desync() {
    // Regression (adversarial review, major): an unterminated complex field
    // (fldChar begin, no end) inside an inline sdt leaves WrapperKind::Field on
    // top of the wrapper stack when `</w:sdt>` fires; committing the sdt then
    // asserted the wrong top wrapper (panic in debug, desync in release). The
    // close now drains the dangling inner wrapper first.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p>
        <w:sdt><w:sdtPr><w:tag w:val="s"/></w:sdtPr><w:sdtContent>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:t>X</w:t></w:r>
        </w:sdtContent></w:sdt>
    </w:p></w:body></w:document>"#;
    // Must not panic; the control is modeled and the document validates.
    let import = import(xml);
    assert!(import.document.validate().is_ok());
    assert!(
        find_inline_sdt(&paragraph(&import, 0).inlines).is_some(),
        "the inline control is modeled (not desynced away)"
    );
}

#[test]
fn block_content_controls_in_consecutive_notes_are_each_modeled() {
    // The reused notes parser resets its sdt state at each note boundary
    // (close_note), so a content control in one note does not tighten
    // MAX_SDT_DEPTH or mis-scope a control in the next note. (The truncated-note
    // leak the reset guards against is not reachable through quick-xml's
    // end-name check — a `</w:footnote>` over an open `<w:sdt>` is rejected as
    // malformed — so this exercises the well-formed path the reset must preserve.)
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:footnoteReference w:id="1"/></w:r></w:p>
        <w:p><w:r><w:footnoteReference w:id="2"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnotes = br#"<w:footnotes xmlns:w="urn:w">
        <w:footnote w:id="1"><w:sdt><w:sdtContent><w:p><w:r><w:t>a</w:t></w:r></w:p></w:sdtContent></w:sdt></w:footnote>
        <w:footnote w:id="2"><w:sdt><w:sdtContent><w:p><w:r><w:t>b</w:t></w:r></w:p></w:sdtContent></w:sdt></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);
    for (_, note) in import.document.definitions().footnotes.iter() {
        assert!(
            note.blocks.iter().any(|b| matches!(b, BlockNode::Sdt(_))),
            "each note's content control is modeled"
        );
    }
    assert!(import.document.validate().is_ok());
}

fn find_block_sdt(blocks: &[BlockNode]) -> Option<&casual_doc_model::v1::BlockSdt> {
    blocks.iter().find_map(|block| match block {
        BlockNode::Sdt(sdt) => Some(sdt),
        _ => None,
    })
}

fn find_inline_sdt(inlines: &[InlineNode]) -> Option<&casual_doc_model::v1::InlineSdt> {
    inlines.iter().find_map(|inline| match inline {
        InlineNode::Sdt(sdt) => Some(sdt),
        _ => None,
    })
}

/// All run text under a sequence of inlines, recursing through content controls
/// and hyperlinks (used to prove wrapped/nested content is preserved).
fn deep_inline_text(inlines: &[InlineNode]) -> String {
    fn walk(inlines: &[InlineNode], out: &mut String) {
        for inline in inlines {
            match inline {
                InlineNode::Run(run) => out.push_str(&run.text),
                InlineNode::Sdt(sdt) => walk(&sdt.inlines, out),
                InlineNode::Hyperlink(link) => walk(&link.inlines, out),
                InlineNode::Field(field) => walk(&field.inlines, out),
                _ => {}
            }
        }
    }
    let mut out = String::new();
    walk(inlines, &mut out);
    out
}

#[test]
fn block_content_control_is_modeled_with_properties() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:sdt>
            <w:sdtPr>
                <w:alias w:val="Full name"/>
                <w:tag w:val="fullName"/>
                <w:id w:val="1553275"/>
                <w:richText/>
            </w:sdtPr>
            <w:sdtContent>
                <w:p><w:r><w:t>Inside</w:t></w:r></w:p>
            </w:sdtContent>
        </w:sdt>
    </w:body></w:document>"#;
    let import = import(xml);
    let sdt = find_block_sdt(import.document.body()).expect("block sdt modeled");
    assert_eq!(sdt.properties.alias.as_deref(), Some("Full name"));
    assert_eq!(sdt.properties.tag.as_deref(), Some("fullName"));
    assert_eq!(sdt.properties.control_id.as_deref(), Some("1553275"));
    assert_eq!(sdt.properties.control_kind, Some(SdtControlKind::RichText));
    assert_eq!(tb_block_text(&sdt.blocks), "Inside");
}

#[test]
fn block_content_control_data_is_modeled() {
    use casual_doc_model::v1::{SdtControlData, SdtLock};
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:w14="urn:w14"><w:body>
        <w:sdt>
            <w:sdtPr>
                <w:tag w:val="pick"/>
                <w:lock w:val="contentLocked"/>
                <w:showingPlcHdr/>
                <w:dataBinding w:prefixMappings="xmlns:ns0='urn:x'" w:xpath="/ns0:a[1]" w:storeItemID="{GUID}"/>
                <w:dropDownList>
                    <w:listItem w:displayText="One" w:value="1"/>
                    <w:listItem w:value="2"/>
                </w:dropDownList>
            </w:sdtPr>
            <w:sdtContent>
                <w:p><w:r><w:t>One</w:t></w:r></w:p>
            </w:sdtContent>
        </w:sdt>
    </w:body></w:document>"#;
    let import = import(xml);
    let sdt = find_block_sdt(import.document.body()).expect("block sdt modeled");
    assert_eq!(
        sdt.properties.control_kind,
        Some(SdtControlKind::DropDownList)
    );
    assert_eq!(sdt.properties.lock, Some(SdtLock::ContentLocked));
    assert!(sdt.properties.showing_placeholder);
    let binding = sdt.properties.data_binding.as_ref().expect("data binding");
    assert_eq!(binding.xpath, "/ns0:a[1]");
    assert_eq!(binding.store_item_id.as_deref(), Some("{GUID}"));
    assert_eq!(
        binding.prefix_mappings.as_deref(),
        Some("xmlns:ns0='urn:x'")
    );
    let Some(SdtControlData::List(items)) = &sdt.properties.data else {
        panic!("expected list entries");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].display.as_deref(), Some("One"));
    assert_eq!(items[0].value, "1");
    assert_eq!(items[1].display, None);
    assert_eq!(items[1].value, "2");
}

#[test]
fn inline_content_control_is_modeled_and_does_not_corrupt_the_paragraph() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:t>Before</w:t></w:r>
            <w:sdt>
                <w:sdtPr><w:tag w:val="pick"/><w:dropDownList/></w:sdtPr>
                <w:sdtContent><w:r><w:t>Choice</w:t></w:r></w:sdtContent>
            </w:sdt>
            <w:r><w:t>After</w:t></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    let sdt = find_inline_sdt(&para.inlines).expect("inline sdt modeled");
    assert_eq!(sdt.properties.tag.as_deref(), Some("pick"));
    assert_eq!(
        sdt.properties.control_kind,
        Some(SdtControlKind::DropDownList)
    );
    let InlineNode::Run(run) = &sdt.inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.text, "Choice");
    let outer: String = para
        .inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::Run(r) => Some(r.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(outer, "BeforeAfter");
}

#[test]
fn block_content_control_inside_a_table_cell_lands_in_the_cell() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tr><w:tc>
            <w:sdt><w:sdtPr><w:tag w:val="cellCtrl"/></w:sdtPr>
                <w:sdtContent><w:p><w:r><w:t>Celled</w:t></w:r></w:p></w:sdtContent>
            </w:sdt>
        </w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    let table = first_table(&import).expect("a table");
    let cell = &table.rows[0].cells[0];
    let sdt = find_block_sdt(&cell.blocks).expect("block sdt in the cell");
    assert_eq!(sdt.properties.tag.as_deref(), Some("cellCtrl"));
    assert_eq!(tb_block_text(&sdt.blocks), "Celled");
}

#[test]
fn inline_content_control_and_hyperlink_compose_innermost_wins() {
    // A hyperlink inside an inline content control.
    let over = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:sdt><w:sdtPr><w:tag w:val="wrap"/></w:sdtPr><w:sdtContent>
            <w:hyperlink w:anchor="bm"><w:r><w:t>link</w:t></w:r></w:hyperlink>
        </w:sdtContent></w:sdt></w:p>
    </w:body></w:document>"#;
    let over_import = import(over);
    let para = paragraph(&over_import, 0);
    let sdt = find_inline_sdt(&para.inlines).expect("inline sdt over a hyperlink");
    assert!(matches!(sdt.inlines[0], InlineNode::Hyperlink(_)));
    assert_eq!(deep_inline_text(&para.inlines), "link");

    // An inline content control inside a hyperlink (the innermost wrapper wins).
    let under = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:hyperlink w:anchor="bm2">
            <w:sdt><w:sdtPr><w:tag w:val="inner"/></w:sdtPr>
                <w:sdtContent><w:r><w:t>x</w:t></w:r></w:sdtContent></w:sdt>
        </w:hyperlink></w:p>
    </w:body></w:document>"#;
    let under_import = import(under);
    let para = paragraph(&under_import, 0);
    let InlineNode::Hyperlink(link) = &para.inlines[0] else {
        panic!("expected a hyperlink");
    };
    let sdt = find_inline_sdt(&link.inlines).expect("inline sdt inside the hyperlink");
    assert_eq!(sdt.properties.tag.as_deref(), Some("inner"));
    assert_eq!(deep_inline_text(&para.inlines), "x");
}

#[test]
fn nested_inline_content_controls_nest() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:sdt><w:sdtPr><w:tag w:val="outer"/></w:sdtPr><w:sdtContent>
            <w:sdt><w:sdtPr><w:tag w:val="inner"/></w:sdtPr><w:sdtContent>
                <w:r><w:t>deep</w:t></w:r>
            </w:sdtContent></w:sdt>
        </w:sdtContent></w:sdt></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    let outer = find_inline_sdt(&para.inlines).expect("outer sdt");
    assert_eq!(outer.properties.tag.as_deref(), Some("outer"));
    let inner = find_inline_sdt(&outer.inlines).expect("inner sdt");
    assert_eq!(inner.properties.tag.as_deref(), Some("inner"));
    assert_eq!(deep_inline_text(&para.inlines), "deep");
}

#[test]
fn deep_tables_inside_a_block_content_control_import_without_hard_failure() {
    // Regression (review-fix 1): a block sdt restarts the table-depth budget in
    // both importer and model, so tables inside a control nested deep in tables do
    // not sum past the bound and abort the whole import.
    let mut xml = String::from(r#"<w:document xmlns:w="urn:w"><w:body>"#);
    let outer = 20;
    for _ in 0..outer {
        xml.push_str("<w:tbl><w:tr><w:tc>");
    }
    xml.push_str("<w:sdt><w:sdtPr><w:tag w:val=\"ctrl\"/></w:sdtPr><w:sdtContent>");
    let inner = 20;
    for _ in 0..inner {
        xml.push_str("<w:tbl><w:tr><w:tc>");
    }
    xml.push_str("<w:p><w:r><w:t>deep</w:t></w:r></w:p>");
    for _ in 0..inner {
        xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    xml.push_str("</w:sdtContent></w:sdt>");
    for _ in 0..outer {
        xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    xml.push_str("</w:body></w:document>");
    let import = import(xml.as_bytes());
    assert!(!import.document.body().is_empty());
    assert!(nonempty_block_texts(&import).iter().any(|t| t == "deep"));
}

#[test]
fn sdt_property_long_tail_is_reported_and_rpr_does_not_leak() {
    use casual_doc_model::v1::SdtLock;
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:sdt>
            <w:sdtPr>
                <w:tag w:val="t"/>
                <w:lock w:val="sdtLocked"/>
                <w:dataBinding w:storeItemID="{X}"/>
                <w:placeholder><w:docPart w:val="Default"/></w:placeholder>
                <w:rPr><w:b/></w:rPr>
            </w:sdtPr>
            <w:sdtContent><w:r><w:t>body</w:t></w:r></w:sdtContent>
        </w:sdt></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    let sdt = find_inline_sdt(&para.inlines).expect("inline sdt");
    let InlineNode::Run(run) = &sdt.inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.text, "body");
    // The bold declared in the sdtPr's rPr must NOT leak onto the wrapped run.
    assert_eq!(run.properties.bold, None);
    // Lock and placeholder are now modeled, not reported.
    assert_eq!(sdt.properties.lock, Some(SdtLock::SdtLocked));
    assert_eq!(sdt.properties.placeholder.as_deref(), Some("Default"));
    // A dataBinding without the required `w:xpath` is meaningless: reported and
    // dropped. The end-mark `w:rPr` remains the reported long tail.
    assert!(sdt.properties.data_binding.is_none());
    let reported = features(&import);
    assert!(reported.contains(&"dataBinding"));
    assert!(reported.contains(&"rPr"));
}

#[test]
fn row_structural_content_control_is_reported_with_inner_rows_intact() {
    // An sdt wrapping table rows (a structural control) is deferred: reported as a
    // passthrough, but its inner rows/cells still parse into the table (no loss).
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl>
            <w:sdt><w:sdtPr><w:tag w:val="rowCtrl"/></w:sdtPr><w:sdtContent>
                <w:tr><w:tc><w:p><w:r><w:t>R1</w:t></w:r></w:p></w:tc></w:tr>
            </w:sdtContent></w:sdt>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(features(&import).contains(&"sdt"));
    let table = first_table(&import).expect("a table");
    assert_eq!(tb_block_text(&table.rows[0].cells[0].blocks), "R1");
}

#[test]
fn empty_content_control_is_dropped_and_reported() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:sdt><w:sdtPr><w:tag w:val="x"/></w:sdtPr><w:sdtContent></w:sdtContent></w:sdt>
        <w:p><w:r><w:t>after</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(find_block_sdt(import.document.body()).is_none());
    assert!(features(&import).contains(&"sdtContent"));
    assert_eq!(nonempty_block_texts(&import), vec!["after".to_string()]);
}

#[test]
fn content_control_over_max_depth_is_a_reported_passthrough() {
    // The 9th nested inline sdt exceeds MAX_SDT_DEPTH (8): it is a reported
    // passthrough, but its run still flows into the innermost modeled control.
    let mut xml = String::from(r#"<w:document xmlns:w="urn:w"><w:body><w:p>"#);
    let depth = 9;
    for _ in 0..depth {
        xml.push_str("<w:sdt><w:sdtContent>");
    }
    xml.push_str("<w:r><w:t>deep</w:t></w:r>");
    for _ in 0..depth {
        xml.push_str("</w:sdtContent></w:sdt>");
    }
    xml.push_str("</w:p></w:body></w:document>");
    let import = import(xml.as_bytes());
    let para = paragraph(&import, 0);
    assert_eq!(deep_inline_text(&para.inlines), "deep");
    assert!(features(&import).contains(&"sdt"));
}

#[test]
fn unclosed_inline_content_control_flushes_its_content_without_desync() {
    // A truncated document leaves an inline control open at EOF (every close tag
    // is missing). The final paragraph flush drains it via `commit_top_wrapper`
    // so its run is committed, not stranded — and the shared depth/scope counters
    // are cleared (no desync). A literally-mismatched `</w:sdt>` would instead be
    // rejected by the XML layer, so EOF truncation is the reachable open case.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:sdt><w:sdtPr><w:tag w:val="open"/></w:sdtPr><w:sdtContent><w:r><w:t>kept</w:t></w:r>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    let sdt = find_inline_sdt(&para.inlines).expect("drained inline sdt");
    assert_eq!(sdt.properties.tag.as_deref(), Some("open"));
    let InlineNode::Run(run) = &sdt.inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.text, "kept");
}

#[test]
fn block_content_control_without_sdt_pr_does_not_panic() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:sdt><w:sdtContent><w:p><w:r><w:t>noPr</w:t></w:r></w:p></w:sdtContent></w:sdt>
    </w:body></w:document>"#;
    let import = import(xml);
    let sdt = find_block_sdt(import.document.body()).expect("block sdt without sdtPr");
    assert_eq!(
        sdt.properties,
        casual_doc_model::v1::SdtProperties::default()
    );
    assert_eq!(tb_block_text(&sdt.blocks), "noPr");
}

#[test]
fn building_block_gallery_forms_report_the_lost_distinction() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:sdt><w:sdtPr><w:docPartObj/></w:sdtPr>
            <w:sdtContent><w:r><w:t>gallery</w:t></w:r></w:sdtContent></w:sdt></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    let sdt = find_inline_sdt(&para.inlines).expect("gallery sdt");
    assert_eq!(
        sdt.properties.control_kind,
        Some(SdtControlKind::BuildingBlockGallery)
    );
    // The docPartObj vs docPartList distinction collapses to one kind: reported.
    assert!(features(&import).contains(&"docPartObj"));
}

#[test]
fn content_control_inside_a_text_box_round_trips() {
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
            <w:txbxContent>
                <w:sdt><w:sdtPr><w:tag w:val="inBox"/></w:sdtPr>
                    <w:sdtContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:sdtContent>
                </w:sdt>
            </w:txbxContent>
        </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    let text_box = find_textbox(&para.inlines).expect("text box modeled");
    let sdt = find_block_sdt(&text_box.blocks).expect("block sdt inside the text box");
    assert_eq!(sdt.properties.tag.as_deref(), Some("inBox"));
    assert_eq!(tb_block_text(&sdt.blocks), "Boxed");
}

// --- Package-manifest disposition pass (P1F-1 / coverage-gap finding F2) ------

/// Builds an in-memory OPC package (a `.docx` zip) from raw part bytes, so a
/// package-level import can be exercised without a fixture on disk.
fn zip_package(parts: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// The whole-part dispositions in a report (admitted parts the semantic model
/// does not consume), as `(part_name, content_type)` pairs in report order.
fn part_dispositions(import: &Import) -> Vec<(String, Option<String>)> {
    import
        .report
        .entries
        .iter()
        .filter_map(|entry| {
            entry
                .part
                .as_ref()
                .map(|part| (part.part_name.clone(), part.content_type.clone()))
        })
        .collect()
}

/// The custom part bytes carried by [`package_with_extra_part`].
const EXTRA_CUSTOM_XML: &[u8] =
    br#"<root xmlns="urn:custom"><value>kept-only-by-retention</value></root>"#;

/// A package whose model consumes the main document and styles, plus one extra
/// admitted part (`customXml/item1.xml`) that the semantic model never opens.
fn package_with_extra_part() -> Vec<u8> {
    let content_types: &[u8] = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels: &[u8] = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document_rels: &[u8] = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let styles: &[u8] = br#"<w:styles xmlns:w="urn:w"><w:style w:type="paragraph" w:styleId="Heading1"><w:rPr><w:b/></w:rPr></w:style></w:styles>"#;
    let document: &[u8] = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p></w:body></w:document>"#;

    zip_package(&[
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", rels),
        ("word/document.xml", document),
        ("word/_rels/document.xml.rels", document_rels),
        ("word/styles.xml", styles),
        ("customXml/item1.xml", EXTRA_CUSTOM_XML),
    ])
}

#[test]
fn semantic_import_reports_each_unconsumed_admitted_part_once() {
    let bytes = package_with_extra_part();
    let mut package =
        DocxPackage::open(&bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    // The single unmodeled admitted part is dispositioned exactly once, naming
    // the part and its declared content type.
    assert_eq!(
        part_dispositions(&import),
        vec![(
            "customXml/item1.xml".to_owned(),
            Some("application/xml".to_owned()),
        )]
    );

    // In Semantic mode the part is not modeled (`omitted`) but is now carried
    // verbatim through the semantic writer via the opaque side-table (P1F-2), so
    // it is `preserved`, not dropped.
    let entry = import
        .report
        .entries
        .iter()
        .find(|entry| entry.part.is_some())
        .unwrap();
    assert_eq!(entry.feature, "customXml/item1.xml");
    assert_eq!(entry.occurrences, 1);
    assert_eq!(entry.model_outcome, ModelOutcome::Omitted);
    assert_eq!(entry.retention_outcome, RetentionOutcome::Preserved);

    // The side-table carries the part's bytes and declared content type.
    let retained = &import.retained_parts;
    assert_eq!(retained.parts.len(), 1);
    let part = &retained.parts[0];
    assert_eq!(part.part_name, "customXml/item1.xml");
    assert_eq!(part.content_type.as_deref(), Some("application/xml"));
    assert_eq!(part.bytes, EXTRA_CUSTOM_XML);
}

#[test]
fn a_consumed_part_is_not_reported_as_a_dropped_whole_part() {
    let bytes = package_with_extra_part();
    let mut package =
        DocxPackage::open(&bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let dropped: Vec<String> = part_dispositions(&import)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    // The consumed styles part, the main document, and pure package plumbing are
    // never whole-part dispositions.
    assert!(!dropped.contains(&"word/styles.xml".to_owned()));
    assert!(!dropped.contains(&"word/document.xml".to_owned()));
    assert!(!dropped.contains(&"[Content_Types].xml".to_owned()));
    assert!(!dropped.iter().any(|name| name.contains("_rels")));
}

#[test]
fn retention_mode_preserves_unconsumed_parts_and_reports_them_preserved() {
    let bytes = package_with_extra_part();
    let mut package =
        DocxPackage::open(&bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let config = ImportConfig {
        mode: ImportMode::Retention,
        ..ImportConfig::default()
    };
    let import = import_package(&mut package, config).unwrap();

    // The whole-part disposition still names the part, now marked preserved
    // (kept verbatim by the retained-source byte floor).
    let entry = import
        .report
        .entries
        .iter()
        .find(|entry| entry.part.is_some())
        .unwrap();
    assert_eq!(entry.feature, "customXml/item1.xml");
    assert_eq!(entry.retention_outcome, RetentionOutcome::Preserved);

    // Retention keeps every admitted part byte-for-byte, including the one the
    // semantic model drops.
    let retained = import.retained_source.as_ref().unwrap();
    assert_eq!(
        retained.parts.get("customXml/item1.xml").map(Vec::as_slice),
        Some(EXTRA_CUSTOM_XML),
    );
}

#[test]
fn real_producer_corpus_unconsumed_part_count_matches_manifest() {
    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-libreoffice.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();

    // Compute the expectation straight from the manifest: every admitted part,
    // minus pure OPC plumbing, minus the parts the model consumes on this
    // fixture (main document, styles, fontTable, settings, and — now that
    // document metadata is modeled and regenerated on write — the docProps
    // core/app parts).
    let manifest: Vec<String> = package
        .entries()
        .iter()
        .map(|entry| entry.part_name.clone())
        .collect();
    let plumbing = manifest
        .iter()
        .filter(|name| {
            name.as_str() == "[Content_Types].xml"
                || name.starts_with("_rels/")
                || name.contains("/_rels/")
        })
        .count();
    let consumed = [
        "word/document.xml",
        "word/styles.xml",
        "word/fontTable.xml",
        "word/settings.xml",
        "docProps/core.xml",
        "docProps/app.xml",
    ];
    let consumed_present = manifest
        .iter()
        .filter(|name| consumed.contains(&name.as_str()))
        .count();

    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    let dropped = part_dispositions(&import);

    assert_eq!(dropped.len(), manifest.len() - plumbing - consumed_present);
    // The docProps parts are now modeled (imported into DocumentProperties and
    // regenerated by the semantic writer), so they are no longer dropped; this
    // fixture has no remaining unconsumed content part.
    assert!(
        dropped.is_empty(),
        "docProps are consumed, not dropped: {dropped:?}"
    );
}

#[test]
fn omml_math_is_retained_opaquely_and_never_leaks_into_run_text() {
    // A paragraph with a visible run, an equation (`m:oMathPara` wrapping
    // `m:oMath`, whose `m:r`/`m:t` share the local names of `w:r`/`w:t`), and a
    // trailing run. Before the C1 namespace guard the math's `m:r`/`m:t` were
    // mistaken for `w:r`/`w:t` and flattened into the paragraph text; now the
    // equation is a single opaque `Math` node whose OMML round-trips verbatim.
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:m="urn:m"><w:body>
        <w:p>
            <w:r><w:t>before</w:t></w:r>
            <m:oMathPara><m:oMath><m:r><m:t>x+1</m:t></m:r></m:oMath></m:oMathPara>
            <w:r><w:t>after</w:t></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);

    // The visible run text is exactly the two `w:t` runs — the math's `x+1` text
    // did NOT leak into the paragraph runs.
    let run_text: String = para
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Run(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(run_text, "beforeafter");

    // Document order is preserved: run, equation, run.
    assert!(matches!(para.inlines[0], InlineNode::Run(_)));
    assert!(matches!(para.inlines[2], InlineNode::Run(_)));
    let InlineNode::Math(math) = &para.inlines[1] else {
        panic!("expected an opaque math node between the two runs");
    };

    // Exactly one opaque math node.
    assert_eq!(
        para.inlines
            .iter()
            .filter(|inline| matches!(inline, InlineNode::Math(_)))
            .count(),
        1
    );

    // The OMML subtree is retained verbatim (open through matching close),
    // including the inner `m:t` markup that would otherwise have been mangled.
    assert_eq!(
        math.omml,
        "<m:oMathPara><m:oMath><m:r><m:t>x+1</m:t></m:r></m:oMath></m:oMathPara>"
    );
    // The plain-text fallback is the concatenated `m:t` text.
    assert_eq!(math.text, "x+1");
}

#[test]
fn symbol_run_is_mapped_to_a_symbol_node() {
    // A `w:sym` (Wingdings checkmark, PUA `0xF0FC`) must map to a first-class
    // Symbol node — not fall through to the catch-all and vanish — with its font
    // binding and code point preserved (the hex `w:char` parsed to a scalar).
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>a</w:t></w:r>
            <w:r><w:sym w:font="Wingdings" w:char="F0FC"/></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let paragraph = paragraph(&import, 0);
    let Some(InlineNode::Symbol(Symbol { font, char, .. })) = paragraph
        .inlines
        .iter()
        .find(|inline| matches!(inline, InlineNode::Symbol(_)))
    else {
        panic!("expected a symbol node");
    };
    assert_eq!(font, "Wingdings");
    assert_eq!(*char, 0xF0FC);
    assert!(
        !features(&import).contains(&"sym"),
        "a well-formed symbol is mapped, not reported"
    );
}

#[test]
fn symbol_without_a_font_is_reported_not_mapped() {
    // A `w:sym` missing its `w:font` (or carrying an unparseable `w:char`) cannot
    // be modeled without inventing a binding; it must be reported so the loss is
    // visible rather than silently swallowed by the catch-all.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:sym w:char="F0FC"/></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let paragraph = paragraph(&import, 0);
    assert!(
        !paragraph
            .inlines
            .iter()
            .any(|inline| matches!(inline, InlineNode::Symbol(_))),
        "a font-less symbol is not modeled"
    );
    assert!(
        features(&import).contains(&"sym"),
        "the unmodeled symbol is reported"
    );
}

#[test]
fn hyphen_glyphs_and_positional_tab_are_mapped_not_reported() {
    // The two hyphen glyphs and a positional tab inside a run map to first-class
    // inline nodes; none is reported dropped.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:noBreakHyphen/><w:softHyphen/><w:ptab w:alignment="right" w:relativeTo="margin" w:leader="underscore"/></w:r></w:p></w:body></w:document>"#;
    let import = import(xml);
    let inlines = &paragraph(&import, 0).inlines;
    assert!(matches!(inlines[0], InlineNode::NoBreakHyphen(_)));
    assert!(matches!(inlines[1], InlineNode::SoftHyphen(_)));
    let InlineNode::PositionalTab(tab) = &inlines[2] else {
        panic!("expected a positional tab");
    };
    assert_eq!(tab.alignment, PositionalTabAlignment::Right);
    assert_eq!(tab.relative_to, PositionalTabRelativeTo::Margin);
    assert_eq!(tab.leader, PositionalTabLeader::Underscore);
    for name in ["noBreakHyphen", "softHyphen", "ptab"] {
        assert!(
            !features(&import).contains(&name),
            "{name} must not be reported as dropped"
        );
    }
}
