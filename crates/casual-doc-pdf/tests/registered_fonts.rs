//! A PDF export embeds the faces the HOST registered (`docs/109` HF-293).
//!
//! A browser has no system fonts: it fetches Roboto and Noto CJK at runtime and
//! registers them on its session's shaper. The export used to lay the document
//! out again on a fresh shaper, which had never seen those faces, so every
//! Hangul syllable became `.notdef` and the report said nothing. These guards run
//! on [`ParleyShaper::without_system_fonts`], which is exactly the browser's
//! position, so neither a developer's installed CJK fonts nor CI's lack of them
//! changes the result.
//!
//! They assert what a reader gets, read back out of the file: the text extracts
//! as Hangul, the embedded program is the registered face, and the code each
//! syllable is drawn with reaches a glyph with ink however the reader resolves a
//! CID — through the program's charset (MuPDF, the specification) or as a glyph
//! id (Poppler). The charset is read with fontations, independently of the
//! writer's own CFF reader.

use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties,
};
use casual_doc_pdf::inspect;
use casual_doc_pdf::{
    NoMediaSource, PdfExport, PdfExportOptions, PdfSeverity, export_document_with_shaper,
};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{LocationRef, Size};
use skrifa::raw::TableProvider;
use skrifa::raw::ps::cff::CffFontRef;
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// Noto Sans CJK KR, subset to the sentence below (`fixtures/fonts/README.md`).
const KOREAN_FACE: &[u8] = include_bytes!("../../../fixtures/fonts/NotoSansCJKkr-TestSubset.otf");
/// Every syllable the Korean subset covers, in a sentence.
const KOREAN: &str = "대한민국 국세청 한글 문서 내보내기 검사";
/// The production face's pinned size (`webapp/src/web_fonts.mjs`, bucket `kr`,
/// which also pins its SHA-256; check that with `sha256sum` before running).
const PRODUCTION_KR_BYTES: usize = 16_433_112;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// One paragraph holding `text`, mixed Hangul and Latin.
fn document(text: &str) -> Document {
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(2),
        properties: ParagraphProperties::default().into(),
        inlines: vec![InlineNode::Run(Run {
            id: node(3),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })],
    });
    Document::new(node(1), vec![paragraph], Definitions::default()).unwrap()
}

/// The browser's shaper: bundled faces only, plus whatever the host registers.
fn browser_shaper(korean: Option<&[u8]>) -> ParleyShaper {
    let shaper = ParleyShaper::without_system_fonts();
    if let Some(face) = korean {
        shaper.register_fallback_font(face.to_vec(), &["Hani", "Hang"]);
    }
    shaper
}

fn export(document: &Document, shaper: &ParleyShaper) -> PdfExport {
    export_document_with_shaper(
        document,
        shaper,
        &NoMediaSource,
        &PdfExportOptions::default(),
    )
    .expect("export")
}

fn text_of(export: &PdfExport) -> String {
    let pdf = inspect::parse(&export.bytes).expect("parse");
    pdf.pages()
        .iter()
        .map(|page| pdf.page_contents(page).plain_text())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The words of `text`: the inspector joins each drawn run with a space, and a
/// space glyph is a run of its own.
fn words(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

fn missing_glyphs(export: &PdfExport) -> Option<(usize, PdfSeverity)> {
    export
        .findings
        .iter()
        .find(|finding| finding.code == "pdf.font.missing_glyphs")
        .map(|finding| (finding.occurrences, finding.severity))
}

/// The embedded program whose name carries `postscript`.
fn program(export: &PdfExport, postscript: &str) -> Option<Vec<u8>> {
    inspect::parse(&export.bytes)
        .expect("parse")
        .embedded_font_programs()
        .into_iter()
        .find(|(name, _)| name.ends_with(&format!("+{postscript}")))
        .map(|(_, bytes)| bytes)
}

fn hangul(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars()
        .filter(|ch| ('\u{ac00}'..='\u{d7a3}').contains(ch))
}

#[derive(Default)]
struct Ink(usize);

impl OutlinePen for Ink {
    fn move_to(&mut self, _: f32, _: f32) {}
    fn line_to(&mut self, _: f32, _: f32) {
        self.0 += 1;
    }
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }
    fn close(&mut self) {}
}

/// Asserts that, in the embedded `program`, every Hangul syllable of `text`
/// is drawn by a glyph whose CID is its glyph id and which has ink — so a
/// reader that resolves the code through the charset and one that treats it
/// as a glyph id both draw the syllable.
fn assert_every_syllable_draws(program: &[u8], text: &str) {
    let font = FontRef::new(program).expect("the embedded program is a font");
    let cff = font.cff().expect("a CFF program").offset_data().as_bytes();
    let cff = CffFontRef::new_cff(cff, 0, None).expect("CFF");
    assert!(cff.is_cid(), "Noto Sans CJK is CID-keyed");
    let charset = cff.charset().expect("a CID-keyed program has a charset");
    let outlines = font.outline_glyphs();
    let charmap = font.charmap();
    for syllable in hangul(text) {
        let glyph = charmap
            .map(syllable)
            .unwrap_or_else(|| panic!("the program maps {syllable}"));
        let cid = charset.string_id(glyph).expect("charset entry").to_u16();
        assert_eq!(
            u32::from(cid),
            glyph.to_u32(),
            "{syllable}: CID {cid} != glyph {glyph:?}, so a charset-following reader (MuPDF) and a glyph-id reader (Poppler) draw different glyphs"
        );
        let mut ink = Ink::default();
        outlines
            .get(GlyphId::new(u32::from(cid)))
            .expect("an outline for the CID")
            .draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                &mut ink,
            )
            .expect("draw");
        assert!(ink.0 > 0, "{syllable} draws no ink");
    }
}

#[test]
fn a_registered_korean_face_is_measured_embedded_and_extracts_as_hangul() {
    let text = format!("{KOREAN} PDF");
    let document = document(&text);
    let export = export(&document, &browser_shaper(Some(KOREAN_FACE)));

    assert_eq!(
        words(&text_of(&export)),
        words(&text),
        "the text layer is the document's"
    );
    assert_eq!(missing_glyphs(&export), None, "{:?}", export.findings);
    let korean = export
        .faces
        .iter()
        .find(|face| face.base_font.ends_with("+NotoSansCJKkr-Regular"))
        .unwrap_or_else(|| panic!("the registered face is embedded: {:?}", export.faces));
    assert_eq!(
        korean.source_bytes,
        KOREAN_FACE.len(),
        "the bytes the host registered"
    );
    assert_eq!(korean.unmapped_glyphs, 0);
    let program = program(&export, "NotoSansCJKkr-Regular").expect("program");
    assert_every_syllable_draws(&program, &text);
}

#[test]
fn the_registered_subset_needed_its_charset_rewritten() {
    // The fixture keeps its source CIDs, so the previous guard is only a guard
    // because of the rewrite: prove the source would have failed it.
    let font = FontRef::new(KOREAN_FACE).unwrap();
    let cff = CffFontRef::new_cff(font.cff().unwrap().offset_data().as_bytes(), 0, None).unwrap();
    let glyph = font.charmap().map('한').unwrap();
    let cid = cff.charset().unwrap().string_id(glyph).unwrap().to_u16();
    assert_ne!(
        u32::from(cid),
        glyph.to_u32(),
        "the source CID is not the glyph id"
    );
}

#[test]
fn without_the_face_the_hangul_is_drawn_as_notdef_and_reported() {
    // The browser before its Korean download lands, and every export before this
    // fix: the export must not look clean.
    let export = export(&document(KOREAN), &browser_shaper(None));
    let syllables = hangul(KOREAN).count();
    assert_eq!(
        missing_glyphs(&export),
        Some((syllables, PdfSeverity::Degraded)),
        "{:?}",
        export.findings
    );
    assert!(
        export
            .faces
            .iter()
            .all(|face| !face.base_font.contains("CJK")),
        "{:?}",
        export.faces
    );
}

#[test]
fn a_face_registered_for_one_document_does_not_reach_another() {
    // One shaper per document (the WASM session owns one): registering Korean
    // for A must neither reach B's export nor be disturbed by it.
    let a = document(KOREAN);
    let b = document(KOREAN);
    let a_shaper = browser_shaper(Some(KOREAN_FACE));
    let b_shaper = browser_shaper(None);

    let a_first = export(&a, &a_shaper);
    let b_export = export(&b, &b_shaper);
    let a_again = export(&a, &a_shaper);

    assert_eq!(missing_glyphs(&a_first), None);
    assert!(
        missing_glyphs(&b_export).is_some(),
        "B never registered Korean"
    );
    assert_eq!(
        a_first.bytes, a_again.bytes,
        "deterministic, and B changed nothing"
    );
}

/// The production face, when a local copy is named. Opt-in because the face is
/// 16 MB and deliberately not committed. The size is checked here; the SHA-256
/// the web manifest pins is checked by whoever supplies the file
/// (`fixtures/fonts/README.md`), so as not to carry a digest implementation for
/// an optional test.
///
/// `OPENDOC_NOTO_CJK_KR=/path/NotoSansCJKkr-Regular.otf cargo test -p
/// casual-doc-pdf --test registered_fonts -- --ignored`
#[test]
#[ignore = "needs OPENDOC_NOTO_CJK_KR: the 16 MB production face is not committed"]
fn the_production_korean_face_exports_whole_and_extracts_as_hangul() {
    let path = std::env::var("OPENDOC_NOTO_CJK_KR")
        .expect("set OPENDOC_NOTO_CJK_KR to the production NotoSansCJKkr-Regular.otf");
    let face = std::fs::read(&path).expect("read the production face");
    assert_eq!(face.len(), PRODUCTION_KR_BYTES, "not the pinned face");

    let text = format!("{KOREAN} 가나다 PDF");
    let export = export(&document(&text), &browser_shaper(Some(&face)));
    assert_eq!(words(&text_of(&export)), words(&text));
    assert_eq!(missing_glyphs(&export), None, "{:?}", export.findings);
    let korean = export
        .faces
        .iter()
        .find(|face| face.base_font.ends_with("+NotoSansCJKkr-Regular"))
        .expect("embedded");
    // CFF is embedded whole and the production charset already is the
    // identity, so the program is the registered file byte for byte.
    assert!(!korean.subset);
    let program = program(&export, "NotoSansCJKkr-Regular").expect("program");
    assert!(
        program == face,
        "the production face is embedded unmodified"
    );
    assert_every_syllable_draws(&program, &text);
}
