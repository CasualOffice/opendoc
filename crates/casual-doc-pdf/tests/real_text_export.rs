//! End-to-end guards for real-text PDF export (`docs/104` HF-030).
//!
//! Every guard here asserts the **guarantee**, not the mechanism: the produced
//! file is parsed back with [`casual_doc_pdf::inspect`] and interrogated the
//! way a reader would. A test that only asserted "a PDF was produced" would
//! prove nothing about the defect this backend fixes — the old print path
//! produced a perfectly valid PDF too, and it was a picture of a document.

use std::collections::BTreeSet;

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::FontId;
use casual_doc_model::v1::Document;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_pdf::inspect;
use casual_doc_pdf::{
    MapMediaSource, NoMediaSource, PdfError, PdfExport, PdfExportOptions, PdfFontSource, PdfPage,
    export_document, write_pdf,
};

/// A real-producer document: headings, body text, a nested table, and one
/// embedded picture. Latin-only, so no font-fallback tier can change the
/// result between a developer's machine and CI.
const RICH_DOCX: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");
/// A document whose `docProps/core.xml` carries real metadata.
const METADATA_DOCX: &[u8] =
    include_bytes!("../../../fixtures/corpus/synthetic-rich-metadata.docx");

/// Imports a fixture and serves its `word/media` parts.
fn open(bytes: &[u8]) -> (Document, MapMediaSource) {
    let mut package = DocxPackage::open(bytes, PackageLimits::default()).expect("open package");
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("import fixture");
    let mut media = MapMediaSource::new();
    for (_id, reference) in imported.document.definitions().media.iter() {
        if let Ok(part) = package.read_part(&reference.part_name) {
            media.insert(reference.part_name.clone(), part);
        }
    }
    (imported.document, media)
}

fn export_rich() -> PdfExport {
    let (document, media) = open(RICH_DOCX);
    export_document(&document, &media, &PdfExportOptions::default()).expect("export rich fixture")
}

#[test]
fn the_pdf_carries_extractable_text_matching_the_document() {
    let export = export_rich();
    let pdf = inspect::parse(&export.bytes).expect("the export parses as a PDF");
    let text: String = pdf
        .pages()
        .iter()
        .map(|page| pdf.page_contents(page).plain_text())
        .collect::<Vec<_>>()
        .join("\n");

    // These are the document's own words, recovered from the file through each
    // font's ToUnicode CMap -- which is exactly what a reader's select, search
    // and copy do.
    for expected in [
        "Rich Document",
        "Paragraph with an image:",
        "Nested A",
        "Nested B",
        "After the nested table.",
    ] {
        assert!(
            text.contains(expected),
            "extracted text is missing {expected:?}\n--- extracted ---\n{text}"
        );
    }
}

#[test]
fn every_text_run_lands_where_the_layout_pass_put_it() {
    // The editor<->PDF parity gate: the exporter transcribes the shared display
    // list, so every run's origin in the file must equal the run's origin in
    // the display list, converted twips -> points with the y axis flipped. Any
    // drift means the exporter did layout of its own, which it is never
    // allowed to do.
    let (document, media) = open(RICH_DOCX);
    let export = export_document(&document, &media, &PdfExportOptions::default()).expect("export");
    let pdf = inspect::parse(&export.bytes).expect("parse");

    let shaper = ParleyShaper::new();
    let laid_out = paginate_document(&document, &shaper);
    let pages = pdf.pages();
    assert_eq!(pages.len(), laid_out.pages.len(), "page count");

    let mut compared = 0_usize;
    for (page, laid) in pages.iter().zip(&laid_out.pages) {
        let contents = pdf.page_contents(page);
        let height = f64::from(laid.page_size.height.raw()) / 20.0;
        let expected: Vec<(f64, f64)> = compose_page(laid)
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Glyphs { run } if !run.glyphs.is_empty() => Some((
                    f64::from(run.origin.x.raw()) / 20.0,
                    height - f64::from(run.origin.y.raw()) / 20.0,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            contents.text.len(),
            expected.len(),
            "one text object per glyph run on page {}",
            laid.number
        );
        for (item, (x, y)) in contents.text.iter().zip(&expected) {
            assert!(
                (item.x - x).abs() < 0.01 && (item.y - y).abs() < 0.01,
                "run {:?} sits at ({}, {}) but layout put it at ({x}, {y})",
                item.text,
                item.x,
                item.y
            );
            compared += 1;
        }
    }
    assert!(compared >= 7, "the fixture exercises {compared} runs");
}

#[test]
fn fonts_are_embedded_and_subsetted() {
    let export = export_rich();
    let pdf = inspect::parse(&export.bytes).expect("parse");
    let fonts = pdf.embedded_fonts();
    assert!(!fonts.is_empty(), "the document draws text with some face");
    for font in &fonts {
        let program = font
            .program_bytes
            .unwrap_or_else(|| panic!("`{}` embeds no font program", font.base_font));
        assert_eq!(font.subtype, "CIDFontType2", "{}", font.base_font);
        assert!(font.has_to_unicode, "`{}` has no ToUnicode", font.base_font);
        // A subset tag is six uppercase letters and a `+`.
        let (tag, _) = font
            .base_font
            .split_once('+')
            .unwrap_or_else(|| panic!("`{}` carries no subset tag", font.base_font));
        assert!(
            tag.len() == 6 && tag.chars().all(|ch| ch.is_ascii_uppercase()),
            "`{}` has a malformed subset tag",
            font.base_font
        );
        // The whole face is one or two hundred kilobytes; a subset of the
        // glyphs one short document uses is a small fraction of that. This is
        // the difference between an export a mail server accepts and one it
        // rejects.
        assert!(
            program < 40 * 1024,
            "`{}` embedded {program} bytes -- that is a whole face, not a subset",
            font.base_font
        );
    }
    // The crate's own report agrees with what the file actually contains.
    assert_eq!(export.faces.len(), fonts.len());
    assert!(export.faces.iter().all(|face| face.subset));
}

#[test]
fn a_page_of_text_is_not_one_giant_image() {
    // The path this backend replaces rendered every page to a 150-DPI bitmap
    // and blitted it: such a file carries exactly one image per page and no
    // text at all. This one carries real text objects and no page-sized
    // picture anywhere.
    let export = export_rich();
    let pdf = inspect::parse(&export.bytes).expect("parse");
    let pages = pdf.pages();
    let mut runs = 0_usize;
    for page in &pages {
        let contents = pdf.page_contents(page);
        runs += contents.text.len();
        assert!(
            contents.images.is_empty(),
            "a page of prose must not be delivered as a picture"
        );
    }
    assert!(runs >= 7, "{runs} text objects");
}

#[test]
fn a_picture_is_embedded_once_however_many_times_it_is_placed() {
    // Two placements of one picture, on two pages. A raster print path stored
    // a fresh page-sized bitmap for each; this stores the picture once and
    // references it twice.
    let png = encode_png(64, 48);
    let mut media = MapMediaSource::new();
    media.insert("word/media/logo.png", png.clone());

    let list = image_list();
    let pages = [page_of(&list), page_of(&list)];
    let export = write_pdf(
        &pages,
        &casual_doc_pdf::BundledFontSource,
        &media,
        &PdfExportOptions::default(),
    )
    .expect("export");

    let pdf = inspect::parse(&export.bytes).expect("parse");
    let placed: Vec<String> = pdf
        .pages()
        .iter()
        .flat_map(|page| pdf.page_contents(page).images)
        .collect();
    assert_eq!(placed.len(), 2, "both placements are drawn");
    let distinct: BTreeSet<&String> = placed.iter().collect();
    assert_eq!(distinct.len(), 1, "one stored picture, referenced twice");
    assert!(export.findings.is_empty(), "{:?}", export.findings);
    // Two pages of a 64x48 picture, not two page-sized bitmaps.
    assert!(
        export.bytes.len() < 8 * 1024,
        "{} bytes for two pages holding one small picture",
        export.bytes.len()
    );
}

#[test]
fn an_undecodable_picture_draws_the_editor_placeholder_and_says_so() {
    // The raster backend paints a bordered box with a diagonal cross when the
    // bytes cannot be decoded. The PDF must do the same and report it -- never
    // leave a silent gap where a picture was.
    let mut media = MapMediaSource::new();
    media.insert(
        "word/media/logo.png",
        b"\x89PNG\r\n\x1a\n truncated".to_vec(),
    );
    let list = image_list();
    let pages = [page_of(&list)];
    let export = write_pdf(
        &pages,
        &casual_doc_pdf::BundledFontSource,
        &media,
        &PdfExportOptions::default(),
    )
    .expect("export");

    let codes: Vec<&str> = export
        .findings
        .iter()
        .map(|finding| finding.code.as_str())
        .collect();
    assert!(
        codes.contains(&"pdf.image.unresolved") && codes.contains(&"pdf.image.missing_bytes"),
        "{codes:?}"
    );
    let pdf = inspect::parse(&export.bytes).expect("parse");
    let page = pdf.pages().remove(0);
    assert!(
        pdf.page_contents(page).images.is_empty(),
        "nothing is embedded for an undecodable picture"
    );
    let stream = pdf
        .stream_data(pdf.get(page, "Contents").expect("contents"))
        .expect("decode content stream");
    let text = String::from_utf8_lossy(&stream);
    // The border box, then the corner-to-corner cross.
    assert!(text.contains("re S"), "{text}");
    assert_eq!(text.matches(" l\n").count(), 2, "{text}");
}

#[test]
fn the_same_document_exports_to_the_same_bytes() {
    // Determinism is a contract across this repository's writers, and for a
    // PDF it also means no clock and no random file identifier leaked in.
    let first = export_rich();
    let second = export_rich();
    assert_eq!(first.bytes, second.bytes);
    let text = String::from_utf8_lossy(&first.bytes);
    assert!(!text.contains("/CreationDate"), "no clock is read");
}

#[test]
fn the_export_is_a_fraction_of_the_raster_it_replaces() {
    // A 150-DPI Letter page is 1275x1650 px: about 8 MB of RGBA, and megabytes
    // even after compression -- per page. The ceiling here is deliberately
    // loose; it exists to catch a regression that starts embedding whole faces
    // or re-encoding pictures, not to publish a figure.
    let export = export_rich();
    assert!(
        export.bytes.len() < 200 * 1024,
        "a {}-page export is {} bytes",
        export.pages,
        export.bytes.len()
    );
}

#[test]
fn document_metadata_reaches_the_info_dictionary() {
    let (document, media) = open(METADATA_DOCX);
    let plain = export_document(&document, &media, &PdfExportOptions::default()).expect("export");
    assert!(
        inspect::parse(&plain.bytes)
            .expect("parse")
            .info()
            .and_then(|info| info.get("Title"))
            .is_none(),
        "metadata is opt-in"
    );

    let export =
        export_document(&document, &media, &PdfExportOptions::with_metadata()).expect("export");
    let pdf = inspect::parse(&export.bytes).expect("parse");
    let info = pdf.info().expect("an /Info dictionary");
    let title = document
        .properties()
        .and_then(|properties| properties.core.title.clone())
        .expect("the fixture carries a title");
    let inspect::Object::String(bytes) = &info["Title"] else {
        panic!("/Title is not a string");
    };
    assert_eq!(decode_text_string(bytes), title);
}

#[test]
fn a_face_that_forbids_embedding_is_refused_out_loud() {
    // A face whose OS/2 fsType is "restricted licence" may not be embedded.
    // The export must say so rather than quietly dropping the text, drawing
    // outlines, or substituting a different face behind the user's back.
    let (document, _) = open(RICH_DOCX);
    let shaper = ParleyShaper::new();
    let laid_out = paginate_document(&document, &shaper);
    let lists: Vec<_> = laid_out
        .pages
        .iter()
        .map(|page| (page.page_size, compose_page(page)))
        .collect();
    let pages: Vec<PdfPage<'_>> = lists
        .iter()
        .map(|(size, list)| PdfPage {
            width: size.width,
            height: size.height,
            list,
        })
        .collect();

    let restricted = RestrictedFontSource::new();
    let outcome = write_pdf(
        &pages,
        &restricted,
        &NoMediaSource,
        &PdfExportOptions::default(),
    );
    let Err(PdfError::Font(message)) = outcome else {
        panic!("a restricted face must refuse the export, got {outcome:?}");
    };
    assert!(
        message.contains("forbids embedding"),
        "the refusal must say why: {message}"
    );
}

/// Serves the bundled faces with `OS/2.fsType` rewritten to "restricted
/// licence embedding", which is the one permission value that forbids putting
/// the face in a document at all.
struct RestrictedFontSource {
    faces: Vec<Vec<u8>>,
}

impl RestrictedFontSource {
    fn new() -> Self {
        let faces = (0..24)
            .map(|id| restrict(casual_doc_layout::fonts::face_bytes(FontId(id))))
            .collect();
        Self { faces }
    }
}

impl PdfFontSource for RestrictedFontSource {
    fn font_data(&self, font: FontId) -> Option<&[u8]> {
        self.faces
            .get(usize::try_from(font.0).ok()?)
            .map(Vec::as_slice)
    }
}

/// Sets `OS/2.fsType` bit 1 in a copy of a face.
fn restrict(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let tables = usize::from(u16::from_be_bytes([out[4], out[5]]));
    for slot in 0..tables {
        let record = 12 + slot * 16;
        if &out[record..record + 4] == b"OS/2" {
            let offset = u32::from_be_bytes([
                out[record + 8],
                out[record + 9],
                out[record + 10],
                out[record + 11],
            ]) as usize;
            out[offset + 8..offset + 10].copy_from_slice(&0x0002_u16.to_be_bytes());
            return out;
        }
    }
    panic!("the bundled faces all carry an OS/2 table");
}

/// Decodes a PDF text string: UTF-16BE when it carries a byte-order mark,
/// otherwise the bytes as Latin-1.
fn decode_text_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

/// A display list placing one picture, for the picture guards.
fn image_list() -> casual_doc_layout::display::DisplayList {
    use casual_doc_layout::display::{DisplayList, PaintItem};
    use casual_doc_layout::units::{Point, Rect, Size, Twip};
    let mut list = DisplayList::new();
    list.push(PaintItem::Image {
        media: "word/media/logo.png".to_owned(),
        rect: Rect::new(
            Point::new(Twip(1440), Twip(1440)),
            Size::new(Twip(2880), Twip(2160)),
        ),
        crop: None,
        transform: None,
    });
    list
}

/// A Letter page carrying `list`.
fn page_of(list: &casual_doc_layout::display::DisplayList) -> PdfPage<'_> {
    use casual_doc_layout::units::Twip;
    PdfPage {
        width: Twip(12_240),
        height: Twip(15_840),
        list,
    }
}

/// A deterministic PNG of the given size.
fn encode_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let picture = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 241) as u8, 40])
    });
    image::DynamicImage::ImageRgb8(picture)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode png");
    bytes
}
