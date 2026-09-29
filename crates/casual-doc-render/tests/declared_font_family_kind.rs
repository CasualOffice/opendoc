//! What a document *declares* a missing face to be decides the substitute — and
//! both the shaped family and the painted face are measured, not read.
//!
//! # The divergence this closes
//!
//! A real customer document paginated to 16 pages against LibreOffice's 15. Band,
//! line pitch (414 twips) and paragraph gap (574 twips) were identical in both
//! engines; one paragraph on page 3 wrapped to **four** lines for us and three for
//! LibreOffice, which pushed the next heading onto page 4 and never recovered.
//!
//! The font attribution reached for first — that a Calibri run gets a
//! wrong-metric face — is disproven and guarded by
//! `calibri_shapes_with_carlito.rs`: Calibri paints with Carlito, the same
//! metric-compatible face LibreOffice embeds. The actual cause was the
//! document's `Normal` style, which names **Bookman Old Style** 12pt (Calibri is
//! only in `docDefaults`). That face is a serif, is not installed, has no metric
//! partner, is absent from `known_family`, and `classify_generic` looks for the
//! substring `serif` — which "bookman old style" does not contain. So it fell to
//! the sans-serif default, Liberation Sans. LibreOffice substitutes Times New
//! Roman (confirmed with `pdffonts`), whose metric partner is the bundled
//! Liberation Serif. A sans face at the same size sets wider, so the line wrapped.
//!
//! `word/fontTable.xml` declared `<w:family w:val="roman"/>` for that face all
//! along. Import had parsed it into `FontDescriptor::family` since the font table
//! landed; layout never read the field. The fix threads that declared class into
//! the substitution decision, so a name the table does not know is classified by
//! what the DOCUMENT says it is instead of by a substring of its name — the
//! family of defects, not the one name. No `bookman old style` entry was added,
//! deliberately: an entry would have fixed the case and left the family open.
//!
//! # Why this fixture, and why it is host-independent
//!
//! The customer document is the owner's and is not committed, nor is anything
//! derived from it. This fixture reproduces its *shape* in a few KB: a `Normal`
//! style naming an uninstalled serif face at 12pt, `<w:family w:val="roman"/>` for
//! that face in the font table, and one paragraph whose wrap differs between a
//! serif and a sans face at the same size.
//!
//! The family name is deliberately invented. `calibri_shapes_with_carlito.rs` has
//! to tolerate `"<host face>"` because a real Arial or Calibri may genuinely be
//! installed and then legitimately wins in `ParleyShaper::pick_family`. A name no
//! font vendor ships can never be installed, so this guard is exact on every
//! host — including a Linux CI runner — and a line count is safe to assert.
//!
//! The price of an invented name is that this fixture is **not** a LibreOffice
//! oracle: converted with `soffice --headless --convert-to pdf`, LibreOffice
//! matches the invented name against its own substitution table and embeds
//! Corsiva Hebrew for it, declaration or no declaration. So the LibreOffice
//! agreement is established on the customer document, where the name is real:
//! `pdffonts` shows LibreOffice embedding Times New Roman for Bookman Old Style
//! and `pdfinfo` 15 pages, and with this change the engine paginates that
//! document at 15 with Liberation Serif — 16 with Liberation Sans before it. What
//! this fixture asserts is the *mechanism*, on a name chosen so the assertion is
//! deterministic everywhere.
//!
//! # Both seams are asserted, because wiring only one of them is the plausible
//! half-fix
//!
//! The declared class has to reach two consumers that must agree:
//!
//! - `ParleyShaper::pick_family` picks the family `parley` **shapes** with. That
//!   choice both sets the advances (so it is the wrap, and the page count) and
//!   rides to the compositor as the glyph run's face, so it is measured twice
//!   here: as a painted family off the composed display list
//!   ([`the_declared_serif_is_painted_with_liberation_serif`]) and as a line count
//!   ([`a_declared_serif_wraps_one_line_tighter_than_the_sans_default`]).
//! - `FontResolver` picks the `FontId` the run carries, which drives the line's
//!   font metrics, is the compositor's face when the shaper's substitute is not
//!   registered, and is what the substitution **report** records. That seam is
//!   measured through the report
//!   ([`a_substitution_driven_by_the_declaration_is_still_reported`]).
//!
//! Neither measurement sees the other seam, which was checked by mutation and not
//! assumed: passing `None` at `pick_family` leaves the report right and reddens
//! the paint and the wrap; passing `None` at `FaceRequest::declared` leaves the
//! paint and the wrap right and reddens the report. So
//! [`the_shaped_family_and_the_resolved_face_agree`] asserts the pairing itself —
//! it is the assertion that fails for *either* one-sided wiring, which is the
//! failure the whole change exists to make impossible.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::flow::build_galley_with_report;
use casual_doc_layout::fonts::family_name;
use casual_doc_layout::page::Page;
use casual_doc_layout::resolve::Disposition;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::v1::Document;
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;
use std::collections::BTreeSet;
use std::io::Cursor;
use std::io::Write;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// A serif family name no font vendor ships, so no host can have it installed and
/// every host must substitute. It contains none of `serif` / `sans` / `mono`, so
/// the name heuristic alone reads it as sans — exactly the trap the customer
/// document fell into.
const MISSING_SERIF: &str = "Chancery Antiqua Deed";

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/>
</Relationships>"#;

/// The customer document's shape: `docDefaults` names Calibri, and the `Normal`
/// style overrides it with the uninstalled serif at 12pt. Reading only
/// `docDefaults` is what made the first investigation chase Calibri.
fn styles_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/>
<w:rPr><w:rFonts w:ascii="{MISSING_SERIF}" w:hAnsi="{MISSING_SERIF}"/><w:sz w:val="24"/></w:rPr></w:style>
</w:styles>"#
    )
}

/// `word/fontTable.xml`. `declare_roman` writes the `<w:family w:val="roman"/>`
/// the customer document carries; without it the document says nothing about the
/// face and the name heuristic is all there is — which is what
/// [`an_undeclared_face_still_falls_to_the_name_heuristic`] pins.
fn font_table_xml(declare_roman: bool) -> String {
    let family = if declare_roman {
        r#"<w:family w:val="roman"/>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:font w:name="{MISSING_SERIF}"><w:charset w:val="00"/>{family}<w:pitch w:val="variable"/></w:font>
<w:font w:name="Calibri"><w:charset w:val="00"/><w:family w:val="swiss"/><w:pitch w:val="variable"/></w:font>
</w:fonts>"#
    )
}

/// The paragraph whose wrap moved. A4 with 1,440-twip margins leaves a
/// 9,026-twip measure; this text is exactly three full lines of Liberation Serif
/// at 12pt and spills to a fourth in the wider-setting Liberation Sans — the
/// single-line difference that cost the customer document a page.
///
/// It necessarily sits ON that boundary: measured with this measure and size, the
/// serif setting turns to four lines one word later and the sans setting turned
/// to four one word earlier, so there is a two-word window and this text is in
/// it. A wrap guard cannot be written away from the boundary, and the customer
/// paragraph was at one too. If a legitimate metrics change moves it, the fix is
/// to re-measure the window (`serif == sans - 1`), never to relax the assertion
/// to "three or four".
const PARAGRAPH: &str = "The Recipient acknowledges that the Confidential Information \
disclosed under this Agreement is and shall remain the exclusive property of the Disclosing \
Party, and that no licence, assignment or other right in respect of it is granted or implied \
by this Agreement or";

fn document_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
{body}
<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>
</w:body></w:document>"#
    )
}

/// The fixture's body: the one paragraph whose wrap moved.
fn one_paragraph() -> String {
    format!(r#"<w:p><w:r><w:t xml:space="preserve">{PARAGRAPH}</w:t></w:r></w:p>"#)
}

fn package_with(declare_roman: bool, body: &str) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", CONTENT_TYPES.to_owned()),
        ("_rels/.rels", ROOT_RELS.to_owned()),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS.to_owned()),
        ("word/document.xml", document_xml(body)),
        ("word/styles.xml", styles_xml()),
        ("word/fontTable.xml", font_table_xml(declare_roman)),
    ] {
        zip.start_file(name, options).expect("a fresh zip entry");
        zip.write_all(body.as_bytes()).expect("writing a zip entry");
    }
    zip.finish().expect("finishing the zip");
    buffer.into_inner()
}

fn imported(declare_roman: bool) -> Document {
    imported_body(declare_roman, &one_paragraph())
}

fn imported_body(declare_roman: bool, body: &str) -> Document {
    let bytes = package_with(declare_roman, body);
    let mut docx = DocxPackage::open(&bytes, PackageLimits::default()).expect("a valid package");
    import_package(
        &mut docx,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the fixture imports")
    .document
}

fn pages(declare_roman: bool) -> Vec<Page> {
    paginate_document(&imported(declare_roman), &ParleyShaper::new()).pages
}

/// The distinct bundled families painted on `page`, read off the composed display
/// list — the faces the renderer actually outlines with.
///
/// Complexity: linear in the page's paint items.
fn painted_families(page: &Page) -> BTreeSet<&'static str> {
    compose_page(page)
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } => Some(
                family_name(run.font)
                    .expect("no host face can supply an invented family, so this is bundled"),
            ),
            _ => None,
        })
        .collect()
}

/// How many lines the fixture's single body paragraph wrapped to.
fn body_lines(page: &Page) -> usize {
    page.placed
        .iter()
        .filter_map(|placed| match &placed.fragment {
            BlockFragment::Paragraph { lines, .. } => Some(lines.lines.len()),
            BlockFragment::TableRow { .. } => None,
        })
        .sum()
}

/// The resolver seam: the face the renderer OUTLINES with must be the serif the
/// document declared, not the sans the name heuristic guesses.
#[test]
fn the_declared_serif_is_painted_with_liberation_serif() {
    let pages = pages(true);
    let page = pages.first().expect("one page");
    let families = painted_families(page);
    assert_eq!(
        families,
        BTreeSet::from(["Liberation Serif"]),
        "the font table declares <w:family w:val=\"roman\"/> for {MISSING_SERIF}, so it must be \
         painted with the bundled serif (Times New Roman's metric partner, which is what \
         LibreOffice substitutes) — painting it with a sans face means the declared class never \
         reached FontResolver"
    );
}

/// The shaper seam: the family `parley` SHAPES with sets the advances, so the wrap
/// — and through it the page count — is what measures whether the declared class
/// reached `pick_family`. Three lines here against four in
/// [`an_undeclared_face_still_falls_to_the_name_heuristic`] is the same one-line
/// difference that cost the customer document a page; on that document the counts
/// are 15 with this fix and 16 without, against LibreOffice's 15.
#[test]
fn a_declared_serif_wraps_one_line_tighter_than_the_sans_default() {
    let declared = pages(true);
    assert_eq!(declared.len(), 1, "the fixture is one page");
    assert_eq!(
        body_lines(&declared[0]),
        3,
        "shaped with the declared serif this paragraph is three lines; four means the shaper \
         classified the face by its name and set it in a wider sans"
    );
}

/// The control, and the thing that proves the test is measuring the declaration
/// and not the family name: strip `<w:family w:val="roman"/>` and the same
/// document falls back to the name heuristic — sans, four lines. This is the
/// behaviour that shipped, kept here as the baseline the fix moves.
#[test]
fn an_undeclared_face_still_falls_to_the_name_heuristic() {
    let undeclared = pages(false);
    let page = undeclared.first().expect("one page");
    assert_eq!(
        painted_families(page),
        BTreeSet::from(["Liberation Sans"]),
        "with nothing declared there is no signal but the name, which contains no generic \
         substring, so the sans default stands"
    );
    assert_eq!(
        body_lines(page),
        4,
        "the wider sans setting is what wrapped the customer document's paragraph to four lines"
    );
}

/// The family the resolver chose for the fixture's only face, as the layout
/// report surfaces it. This is the resolver seam made observable.
fn reported_family(declare_roman: bool) -> (&'static str, Disposition) {
    let document = imported(declare_roman);
    let (_galley, report) = build_galley_with_report(
        &document,
        &ParleyShaper::new(),
        // The fixture's measure: A4 less its 1,440-twip left and right margins.
        Twip(9_026),
    );
    let record = report
        .substitutions()
        .find(|record| record.requested == MISSING_SERIF)
        .expect("a face we do not have must be reported, never silently swapped");
    (record.resolved_family, record.disposition)
}

/// Loss stays reported, and the resolver seam is what reports it. A substitution
/// driven by the declared class is a better guess, not a correct answer, so it
/// must still surface as a `Fallback` — under the family the document asked for,
/// not the one we chose.
#[test]
fn a_substitution_driven_by_the_declaration_is_still_reported() {
    assert_eq!(
        reported_family(true),
        ("Liberation Serif", Disposition::Fallback),
        "a generic-class match is not metric-compatible and must not claim to be,          and it must not go silent just because it is now a better guess"
    );
    // The control: the same document with nothing declared is still reported, and
    // still as a fallback — the fix must not have turned one report into none.
    assert_eq!(
        reported_family(false),
        ("Liberation Sans", Disposition::Fallback)
    );
}

/// The invariant the whole change exists for: the family the run is SHAPED with
/// and the family the resolver picked the outlined face from are the same one.
///
/// A declared class that reached only one of the two consumers would break this
/// while leaving every other assertion in this file green for the seam it does
/// cover, so this is the guard that cannot be satisfied by half the fix.
#[test]
fn the_shaped_family_and_the_resolved_face_agree() {
    for declare_roman in [true, false] {
        let pages = pages(declare_roman);
        let page = pages.first().expect("one page");
        let painted = painted_families(page);
        let (resolved, _) = reported_family(declare_roman);
        assert_eq!(
            painted,
            BTreeSet::from([resolved]),
            "the shaper painted {painted:?} while the resolver resolved to              {resolved:?} (declared roman: {declare_roman}) — the shaped and the              outlined face have diverged, which is the failure the declared class              has to reach BOTH seams to prevent"
        );
    }
}

/// The other half of the complexity gate (SKILL section 8): the declared-class
/// lookup is per RUN, not per glyph or per character.
///
/// `font_substitution`'s own doubling test pins the cost of one lookup; this pins
/// how many happen. The substitution report counts one occurrence per resolution,
/// so a document whose single run is twice as long must report the same single
/// occurrence -- doubling the text, not timing it. Resolving per character (or per
/// glyph inside the shaper) would make the count track the text length, which is
/// the failure this catches.
#[test]
fn doubling_a_run_length_does_not_double_the_substitution_lookups() {
    fn occurrences(repeats: usize) -> u32 {
        let text = PARAGRAPH.repeat(repeats);
        let body = format!(r#"<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#);
        let document = imported_body(true, &body);
        let (_galley, report) =
            build_galley_with_report(&document, &ParleyShaper::new(), Twip(9_026));
        report
            .substitutions()
            .find(|record| record.requested == MISSING_SERIF)
            .expect("the substitution is reported")
            .occurrences
    }

    let single = occurrences(1);
    let doubled = occurrences(2);
    assert_eq!(single, 1, "one run resolves its family exactly once");
    assert_eq!(
        doubled, single,
        "doubling the run's text must not change how many times its family is \
         resolved ({single} -> {doubled}); a count that tracks the text length \
         means the lookup moved onto the per-character or per-glyph path"
    );
}
