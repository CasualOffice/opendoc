// SPDX-License-Identifier: Apache-2.0

//! A border edge's `@w:themeColor` (with `@w:themeTint`/`@w:themeShade`) is read,
//! modelled, written back, and a fixed point across a save — on both of the two
//! paths that build a border edge, the body parser's and the styles parser's.
//!
//! # Why this file exists
//!
//! `docs/161` is the implementation record and `160` the measurement it rests on
//! (`160` lands separately). `160` measured import loss over the owner's
//! twenty-document corpus and
//! the largest *real* finding was this one: five of nineteen documents lost a
//! border theme colour, in `word/document.xml` and in `word/styles.xml` alike.
//! `BorderEdge` carried `color: Option<RgbColor>` and nothing else, so the theme
//! triple fell off the end of both edge builders — which existed twice, once per
//! parser, which is why it had to be dropped twice to be dropped at all.
//!
//! What exactly was lost is worth stating precisely, because the obvious claim
//! overstates it. A scan of the corpus found **8,449** themed border edges across
//! nine documents and **every one of them also carries a concrete `@w:color`**,
//! the fallback Word writes beside the reference. So the static paint was mostly
//! already approximately right; what was certainly lost is the *reference* — the
//! document's theme can no longer repaint the border, which is the whole point of
//! a theme colour. Of the 2,057 edges that also carry a tint or shade, 94 carry
//! the **raw** slot colour in `@w:color`, so for those the tint was lost on the
//! page as well.
//!
//! # The trap this file is written around
//!
//! The sibling `w15:collapsed` guard records asking
//! `xml_text(written).contains("collapsed")` and passing because a *heading's
//! title text* held the substring. Every question here is therefore asked through
//! [`themed_edges`], which walks XML **element and attribute names** and returns
//! the attribute values keyed by the edge's element name — never a substring
//! search over the part bytes. The document body below deliberately contains the
//! words `themeColor` and `accent1` as ordinary text, so a guard that reaches for
//! a substring passes here while proving nothing, and is caught.
//!
//! The attribute test is on `attribute.key.as_ref()`, the name **as written**:
//! `w:themeColor` is `w:`-qualified (unlike `wp:wrapText`, which is not), so a
//! writer that emitted a bare `themeColor` must not satisfy this file.
//!
//! # What this file does NOT claim
//!
//! It does not claim the colour is *painted* correctly — that is
//! `casual-doc-layout`'s half, guarded by
//! `flow::tests::a_themed_run_border_paints_the_theme_slot_not_black` and
//! `flow::tests::edge_colour_applies_tint_falls_back_without_a_palette_and_ranks_by_the_resolved_colour`
//! in the same change. Nor does it claim `w:clrSchemeMapping` is honoured: the
//! `text1`/`background1` spellings resolve through the DEFAULT colour map, and a
//! document that remaps them is a separate, recorded gap.
//!
//! # Complexity
//!
//! `O(package bytes)` per test: one import and one or two exports of one small
//! in-test package. Test-time only; nothing here is on an edit path.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

use casual_doc_export::export_document_with_retained_parts;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document, StyleKind, ThemeColor, ThemeColorRef};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use quick_xml::Reader;
use quick_xml::events::Event;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// One border edge as it appears on disk: its element name **as written** and
/// its three theme attributes, each `None` when absent.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ThemedEdge {
    /// The element name as written, so `w:top` and a hypothetical `w15:top` are
    /// different edges.
    element: String,
    /// `@w:themeColor`.
    slot: Option<String>,
    /// `@w:themeTint`.
    tint: Option<String>,
    /// `@w:themeShade`.
    shade: Option<String>,
}

/// The border-edge element local names `CT_Border` is used for. `w:bar` and
/// `w:between` are paragraph-only; the rest are shared with table and page
/// borders.
const EDGE_NAMES: [&str; 10] = [
    "top", "bottom", "left", "right", "start", "end", "insideH", "insideV", "bar", "between",
];

/// The three attributes under test, `w:`-qualified exactly as the schema spells
/// them. Not bare `themeColor`.
const THEME_ATTRS: [&[u8]; 3] = [b"w:themeColor", b"w:themeTint", b"w:themeShade"];

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>
</Relationships>"#;

/// A twelve-slot colour scheme, so the theme references below resolve to
/// something rather than to a black fallback.
const THEME: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Probe">
<a:themeElements>
<a:clrScheme name="Probe">
<a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1>
<a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
<a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
<a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4>
<a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6>
<a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
</a:clrScheme>
<a:fontScheme name="Probe"><a:majorFont><a:latin typeface="Calibri Light"/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/></a:minorFont></a:fontScheme>
<a:fmtScheme name="Probe"/>
</a:themeElements>
</a:theme>"#;

/// A table style whose `w:tblBorders` carry a themed edge. This is the **styles**
/// parser's edge builder, which had its own copy of the mapping and so lost the
/// theme triple independently of the body parser's.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="table" w:styleId="ThemedGrid"><w:name w:val="Themed Grid"/>
<w:tblPr><w:tblBorders>
<w:top w:val="single" w:sz="4" w:space="0" w:color="FFC000" w:themeColor="accent4"/>
<w:insideH w:val="single" w:sz="4" w:space="0" w:color="70AD47" w:themeColor="accent6" w:themeShade="BF"/>
</w:tblBorders></w:tblPr>
</w:style>
</w:styles>"#;

/// The body: a paragraph border and a table-cell border, each themed, plus body
/// TEXT naming the attribute and the slot. The text is the substring trap — a
/// guard asking whether the written bytes contain `"themeColor"` passes on this
/// document no matter what the writer does.
const DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pBdr>
<w:top w:val="single" w:sz="8" w:space="1" w:color="4472C4" w:themeColor="accent1" w:themeTint="99"/>
<w:bottom w:val="single" w:sz="8" w:space="1" w:color="000000" w:themeColor="text1"/>
</w:pBdr></w:pPr><w:r><w:t>themeColor accent1 themeTint themeShade</w:t></w:r></w:p>
<w:tbl><w:tblPr><w:tblStyle w:val="ThemedGrid"/></w:tblPr><w:tr><w:tc>
<w:tcPr><w:tcBorders>
<w:left w:val="single" w:sz="12" w:space="0" w:color="ED7D31" w:themeColor="accent2" w:themeShade="80"/>
</w:tcBorders></w:tcPr>
<w:p><w:r><w:t>cell</w:t></w:r></w:p>
</w:tc></w:tr></w:tbl>
<w:p><w:pPr><w:pBdr>
<w:right w:val="single" w:sz="8" w:space="1" w:color="7F7F7F"/>
</w:pBdr></w:pPr><w:r><w:t>plain</w:t></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
</w:body></w:document>"#;

/// What the source says, pinned so a probe edited down to nothing cannot satisfy
/// this file by having nothing to find — the vacuous pass that makes a guard
/// worse than no guard, and the exact state the whole fixture corpus is in for
/// this construct (zero themed border edges in all 36 packages under
/// `fixtures/`).
///
/// The `w:right` entry is the control: an edge with a concrete colour and **no**
/// theme reference, which must stay without one.
fn expected() -> Vec<ThemedEdge> {
    [
        // word/document.xml
        ("w:top", Some("accent1"), Some("99"), None),
        ("w:bottom", Some("text1"), None, None),
        ("w:left", Some("accent2"), None, Some("80")),
        ("w:right", None, None, None),
        // word/styles.xml
        ("w:top", Some("accent4"), None, None),
        ("w:insideH", Some("accent6"), None, Some("BF")),
    ]
    .into_iter()
    .map(|(element, slot, tint, shade)| ThemedEdge {
        element: element.to_owned(),
        slot: slot.map(str::to_owned),
        tint: tint.map(str::to_owned),
        shade: shade.map(str::to_owned),
    })
    .collect()
}

fn package() -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/document.xml", DOCUMENT),
        ("word/styles.xml", STYLES),
        ("word/theme/theme1.xml", THEME),
    ] {
        zip.start_file(name, options).expect("a fresh zip entry");
        zip.write_all(body.as_bytes()).expect("writing a zip entry");
    }
    zip.finish().expect("finishing the zip");
    buffer.into_inner()
}

/// One import: the model and the retained parts needed to write it back.
struct Imported {
    document: Document,
    retained: casual_doc_import::RetainedParts,
}

fn import(bytes: &[u8]) -> Imported {
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
    Imported {
        document: import.document,
        retained: import.retained_parts,
    }
}

fn write_back(imported: &Imported) -> Vec<u8> {
    export_document_with_retained_parts(&imported.document, &BTreeMap::new(), &imported.retained)
        .expect("the model is writable")
        .bytes
}

/// Every border-edge element in a package, in part-name then document order, as
/// its element name **as written** and its three theme attributes.
///
/// This is the function every question in this file must be asked through. It
/// walks element and attribute names; it never searches the part bytes for a
/// substring. `attribute.key.as_ref()` is the name as written, which is what
/// pins the `w:` prefix.
fn themed_edges(bytes: &[u8]) -> Vec<ThemedEdge> {
    let mut found = Vec::new();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    // document.xml before styles.xml, which is the order `expected` is written in.
    names.sort();
    names.sort_by_key(|name| !name.ends_with("document.xml"));
    for name in names {
        if !name.ends_with(".xml") {
            continue;
        }
        let mut part = Vec::new();
        {
            use std::io::Read;
            archive
                .by_name(&name)
                .expect("a listed entry")
                .read_to_end(&mut part)
                .expect("a readable entry");
        }
        let mut reader = Reader::from_reader(part.as_slice());
        let mut buffer = Vec::new();
        loop {
            let element = match reader.read_event_into(&mut buffer) {
                Ok(Event::Start(element)) => element,
                Ok(Event::Empty(element)) => element,
                Ok(Event::Eof) => break,
                Ok(_) => continue,
                Err(error) => panic!("the written {name} is well-formed XML: {error}"),
            };
            let written = String::from_utf8_lossy(element.name().as_ref()).into_owned();
            let local = String::from_utf8_lossy(element.local_name().as_ref()).into_owned();
            if !EDGE_NAMES.contains(&local.as_str()) {
                continue;
            }
            // `w:top` is also a cell-margin and a text-direction element name, so
            // an edge is only an edge when it carries the required `w:val` style.
            let mut values: [Option<String>; 3] = [None, None, None];
            let mut is_edge = false;
            for attribute in element.attributes() {
                let attribute = attribute.expect("a well-formed attribute");
                let key = attribute.key.as_ref().to_vec();
                if key == b"w:val" {
                    is_edge = true;
                }
                if let Some(index) = THEME_ATTRS
                    .iter()
                    .position(|probe| *probe == key.as_slice())
                {
                    values[index] =
                        Some(String::from_utf8_lossy(attribute.value.as_ref()).into_owned());
                }
            }
            // A cell margin carries `w:w`/`w:type`, never `w:val`; a border always
            // carries `w:val` (the importer refuses an edge without one).
            if !is_edge && values.iter().all(Option::is_none) {
                continue;
            }
            let [slot, tint, shade] = values;
            found.push(ThemedEdge {
                element: written,
                slot,
                tint,
                shade,
            });
        }
    }
    found
}

/// The pin on the source side: the probe really does carry what [`expected`]
/// says, so every assertion below is about the engine rather than the probe.
#[test]
fn the_probe_package_carries_the_themed_edges_this_file_pins() {
    assert_eq!(
        themed_edges(&package()),
        expected(),
        "the probe's themed border edges, walked by element and attribute name"
    );
}

/// The import half: the theme reference reaches the model on both paths, and the
/// concrete `w:color` fallback is kept beside it rather than replaced by it.
#[test]
fn a_themed_border_reaches_the_model_on_both_the_body_and_styles_paths() {
    let imported = import(&package());
    let document = &imported.document;

    // The body path: the first paragraph's own `w:pBdr`.
    let borders = document
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => paragraph.properties.borders.top.as_ref(),
            _ => None,
        })
        .expect("the first paragraph carries a top border");
    assert_eq!(
        borders.theme_color,
        Some(ThemeColor {
            slot: ThemeColorRef::Accent1,
            theme_tint: Some(0x99),
            theme_shade: None,
        }),
        "the body parser carries the slot and its tint"
    );
    assert!(
        borders.color.is_some(),
        "the concrete `w:color` fallback is kept beside the reference, not replaced by it"
    );

    // The styles path: the table style's `w:tblBorders`.
    let style = document
        .definitions()
        .styles
        .iter()
        .map(|(_, style)| style)
        .find(|style| style.kind == StyleKind::Table)
        .expect("the table style imported");
    let table = style.table.as_ref().expect("the style carries tblPr");
    assert_eq!(
        table.borders.top.as_ref().and_then(|e| e.theme_color),
        Some(ThemeColor {
            slot: ThemeColorRef::Accent4,
            theme_tint: None,
            theme_shade: None,
        }),
        "the styles parser carries the slot"
    );
    assert_eq!(
        table
            .borders
            .inside_h
            .as_ref()
            .and_then(|e| e.theme_color)
            .and_then(|theme| theme.theme_shade),
        Some(0xBF),
        "the styles parser carries the shade"
    );
}

/// Two normalizations the engine applies on the way out, applied to both sides of
/// the round-trip comparison below so it asks about the *slot* rather than about
/// its spelling.
///
/// Both are pre-existing and documented, and neither loses anything:
///
/// - **Physical to logical edges.** The model normalizes `w:left`/`w:right` onto
///   `w:start`/`w:end` and the writer emits the logical names. `docs/161` §1.4 lists
///   this as the same spelling class as `w:ind@w:left`, and it is already an
///   exception in the element-coverage gate.
/// - **Mapped slot spellings.** `ST_ThemeColor` gives the same four slots two
///   names each: `text1`/`dark1`, `background1`/`light1`, `text2`/`dark2`,
///   `background2`/`light2`. `casual_doc_import`'s `theme_color_ref` resolves the
///   mapped spelling to its default-colour-map slot and the writer emits the
///   canonical one, which is documented on that function.
///
/// Stated as a function rather than folded into the walker on purpose: the walker
/// stays the honest record of what is on disk, and exactly which differences this
/// file is willing to forgive is readable in one place.
fn canonical(edges: Vec<ThemedEdge>) -> Vec<ThemedEdge> {
    edges
        .into_iter()
        .map(|edge| ThemedEdge {
            element: match edge.element.as_str() {
                "w:left" => "w:start".to_owned(),
                "w:right" => "w:end".to_owned(),
                _ => edge.element,
            },
            slot: edge.slot.map(|slot| {
                match slot.as_str() {
                    "text1" => "dark1",
                    "background1" => "light1",
                    "text2" => "dark2",
                    "background2" => "light2",
                    other => other,
                }
                .to_owned()
            }),
            ..edge
        })
        .collect()
}

/// The invariant at the altitude that matters: **the value survives the save**.
/// Every themed edge in the source is a themed edge in the written package, with
/// the same slot and the same tint/shade, and the unthemed control stays
/// unthemed. Written as a whole-list comparison so an extra theme reference the
/// writer invented is a failure too.
#[test]
fn every_themed_border_edge_survives_a_save() {
    let imported = import(&package());
    let written = write_back(&imported);
    assert_eq!(
        canonical(themed_edges(&written)),
        canonical(themed_edges(&package())),
        "the written package's themed border edges match the source's, slot and \
         tint/shade alike"
    );
}

/// A second save changes nothing: the written package is a fixed point, so the
/// attribute is not being re-derived differently on each pass (the shape that
/// makes a round trip drift one notch per save).
///
/// Compared raw, not canonically: by this point both sides are already the
/// engine's own spelling, so a drift in the spelling itself must fail here.
#[test]
fn a_second_save_is_a_fixed_point() {
    let once = write_back(&import(&package()));
    let twice = write_back(&import(&once));
    assert_eq!(
        themed_edges(&twice),
        themed_edges(&once),
        "re-importing and re-writing leaves the themed edges unchanged"
    );
}

/// The theme reference is written as `w:themeColor`, not as a bare `themeColor`
/// and not as some other prefix. A namespace this project has guessed wrong
/// before, pinned by reading the attribute key as written.
#[test]
fn the_theme_attributes_are_w_qualified_as_written() {
    let written = write_back(&import(&package()));
    let mut archive = zip::ZipArchive::new(Cursor::new(written)).expect("a zip package");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for name in names {
        if !name.ends_with("document.xml") && !name.ends_with("styles.xml") {
            continue;
        }
        let mut part = Vec::new();
        {
            use std::io::Read;
            archive
                .by_name(&name)
                .expect("a listed entry")
                .read_to_end(&mut part)
                .expect("a readable entry");
        }
        let mut reader = Reader::from_reader(part.as_slice());
        let mut buffer = Vec::new();
        loop {
            let element = match reader.read_event_into(&mut buffer) {
                Ok(Event::Start(element) | Event::Empty(element)) => element,
                Ok(Event::Eof) => break,
                Ok(_) => continue,
                Err(error) => panic!("the written {name} is well-formed XML: {error}"),
            };
            for attribute in element.attributes() {
                let attribute = attribute.expect("a well-formed attribute");
                let local =
                    String::from_utf8_lossy(attribute.key.local_name().as_ref()).into_owned();
                if matches!(local.as_str(), "themeColor" | "themeTint" | "themeShade") {
                    keys.insert(String::from_utf8_lossy(attribute.key.as_ref()).into_owned());
                }
            }
        }
    }
    assert_eq!(
        keys,
        ["w:themeColor", "w:themeShade", "w:themeTint"]
            .iter()
            .map(|key| (*key).to_owned())
            .collect::<BTreeSet<_>>(),
        "the three theme attributes are written `w:`-qualified and nothing else is"
    );
}
