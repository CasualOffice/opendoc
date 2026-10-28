//! A deterministic, rights-safe synthetic PDF corpus (docs/182 §10 P0).
//!
//! Every file here is written by this module from constants, so the corpus has
//! no third-party or user content and regenerates byte for byte. Each document
//! isolates one construct family the importer must map or report (prose,
//! columns, vectors, images, rotated text, tagged content, Type3, page boxes,
//! active content) and the hostile set exercises every limit in `docs/21`'s PDF
//! section.
//!
//! The writer is the smallest correct PDF 1.7 serializer: numbered objects, a
//! classic cross-reference table with exact byte offsets, a trailer. Complexity
//! is O(total bytes).

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;

/// A PDF under construction: objects numbered from 1 in insertion order.
#[derive(Debug, Default)]
pub struct PdfBuilder {
    objects: Vec<Option<Vec<u8>>>,
}

impl PdfBuilder {
    /// Reserves an object number to be filled later (forward references).
    pub fn reserve(&mut self) -> u32 {
        self.objects.push(None);
        u32::try_from(self.objects.len()).expect("object count fits u32")
    }

    /// Sets a reserved object's body.
    pub fn set(&mut self, id: u32, body: impl Into<Vec<u8>>) {
        self.objects[id as usize - 1] = Some(body.into());
    }

    /// Adds an object and returns its number.
    pub fn add(&mut self, body: impl Into<Vec<u8>>) -> u32 {
        let id = self.reserve();
        self.set(id, body);
        id
    }

    /// Adds a stream object with `extra` dictionary entries.
    pub fn stream(&mut self, extra: &str, data: &[u8]) -> u32 {
        self.add(stream_body(extra, data))
    }

    /// Adds a Flate-compressed stream.
    pub fn flate_stream(&mut self, extra: &str, data: &[u8]) -> u32 {
        let packed = deflate(data);
        self.stream(&format!("/Filter /FlateDecode {extra}"), &packed)
    }

    /// Serializes with `root` as the catalog and `trailer_extra` appended to
    /// the trailer dictionary.
    pub fn finish(self, root: u32, trailer_extra: &str) -> Vec<u8> {
        let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(self.objects.len());
        for (index, body) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            let body = body.as_deref().unwrap_or(b"null");
            out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", self.objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root {root} 0 R {trailer_extra} >>\nstartxref\n{xref}\n%%EOF\n",
                self.objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}

fn stream_body(extra: &str, data: &[u8]) -> Vec<u8> {
    let mut body = format!("<< /Length {} {extra} >>\nstream\n", data.len()).into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    body
}

/// zlib-compresses `data` at the best ratio (deterministic for a given input).
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).expect("in-memory write");
    encoder.finish().expect("in-memory finish")
}

/// The standard resources most documents use: Helvetica (F1) and Times (F2),
/// both standard-14 faces in WinAnsi.
const FONTS: &str = "/Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> /F2 << /Type /Font /Subtype /Type1 /BaseFont /Times-Roman /Encoding /WinAnsiEncoding >> /F3 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >> >>";

/// Builds a document of pages, each `(media box, extra page entries, content,
/// extra resources)`, with `catalog_extra` in the catalog.
fn document(
    pages: &[(String, String, Vec<u8>, String)],
    catalog_extra: &str,
    trailer_extra: &str,
) -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let mut kids = Vec::new();
    for (media_box, page_extra, content, resources) in pages {
        let content = pdf.flate_stream("", content);
        let page = pdf.add(format!(
            "<< /Type /Page /Parent {tree} 0 R /MediaBox [{media_box}] /Contents {content} 0 R /Resources << {FONTS} {resources} >> {page_extra} >>"
        ));
        kids.push(format!("{page} 0 R"));
    }
    pdf.set(
        tree,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            kids.len()
        ),
    );
    pdf.set(
        catalog,
        format!("<< /Type /Catalog /Pages {tree} 0 R {catalog_extra} >>"),
    );
    pdf.finish(catalog, trailer_extra)
}

const LETTER: &str = "0 0 612 792";

fn page(content: impl Into<Vec<u8>>) -> (String, String, Vec<u8>, String) {
    (
        LETTER.to_owned(),
        String::new(),
        content.into(),
        String::new(),
    )
}

/// Escapes a WinAnsi literal string.
fn literal(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// A left-aligned paragraph block: `lines` at `leading` from (`x`, `y`).
fn text_block(font: &str, size: f32, x: f32, y: f32, leading: f32, lines: &[&str]) -> String {
    let mut out = format!("BT /{font} {size} Tf {leading} TL {x} {y} Td\n");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push_str("T*\n");
        }
        out.push_str(&format!("({}) Tj\n", literal(line)));
    }
    out.push_str("ET\n");
    out
}

const PROSE: [&str; 8] = [
    "The quick brown fox jumps over the lazy dog while the committee",
    "reviews the quarterly figures. Every sentence in this synthetic",
    "corpus is written by the repository, so the measurement carries",
    "no third-party text and regenerates byte for byte on any machine.",
    "A second paragraph begins after a blank line so the grouping",
    "parameters see a paragraph gap that is larger than the leading",
    "between lines, which is what separates two blocks in a frame.",
    "Numbers such as 3.14159 and 2,718 sit beside punctuation; ok?",
];

/// One synthetic file: its corpus name and bytes.
pub type Generated = (&'static str, Vec<u8>);

/// The construct corpus (each file is expected to import).
pub fn construct_corpus() -> Vec<Generated> {
    vec![
        ("synthetic-prose.pdf", prose()),
        ("synthetic-two-column.pdf", two_column()),
        ("synthetic-vector.pdf", vector()),
        ("synthetic-images.pdf", images()),
        ("synthetic-rotated-text.pdf", rotated_text()),
        ("synthetic-tagged.pdf", tagged()),
        ("synthetic-type3.pdf", type3()),
        ("synthetic-page-boxes.pdf", page_boxes()),
        ("synthetic-active-content.pdf", active_content()),
        ("synthetic-embedded-truetype.pdf", embedded_truetype()),
    ]
}

/// The hostile corpus (each file is expected to be refused within budget).
pub fn hostile_corpus() -> Vec<Generated> {
    vec![
        ("hostile-encrypted.pdf", encrypted()),
        ("hostile-flate-bomb.pdf", flate_bomb()),
        ("hostile-operator-flood.pdf", operator_flood()),
        ("hostile-image-dimensions.pdf", image_dimensions()),
        ("hostile-recursive-form.pdf", recursive_form()),
        ("hostile-truncated.pdf", truncated()),
        ("hostile-garbage.pdf", garbage()),
        ("hostile-page-flood.pdf", page_flood()),
        ("hostile-huge-page.pdf", huge_page()),
    ]
}

fn prose() -> Vec<u8> {
    let first = format!(
        "{}{}{}",
        text_block("F3", 18.0, 72.0, 720.0, 22.0, &["Synthetic prose"]),
        text_block("F2", 11.0, 72.0, 690.0, 14.0, &PROSE[..4]),
        text_block("F2", 11.0, 72.0, 610.0, 14.0, &PROSE[4..]),
    );
    let second = text_block("F1", 10.0, 72.0, 720.0, 13.0, &PROSE);
    document(&[page(first), page(second)], "", "")
}

fn two_column() -> Vec<u8> {
    let content = format!(
        "{}{}{}",
        text_block("F3", 16.0, 72.0, 730.0, 20.0, &["Two columns"]),
        text_block("F2", 9.0, 72.0, 700.0, 11.0, &PROSE.map(|line| &line[..30])),
        text_block(
            "F2",
            9.0,
            320.0,
            700.0,
            11.0,
            &PROSE.map(|line| &line[30..])
        ),
    );
    document(&[page(content)], "", "")
}

fn vector() -> Vec<u8> {
    let content = "\
0.9 0.2 0.2 rg 72 600 200 100 re f
0 0 1 RG 3 w [6 3] 0 d 72 450 m 300 450 l 300 550 l S
[] 0 d 0.2 0.6 0.2 rg 400 600 m 400 655 445 700 500 700 c 555 700 600 655 600 600 c 600 545 555 500 500 500 c 445 500 400 545 400 600 c h
450 600 m 450 628 472 650 500 650 c 528 650 550 628 550 600 c 550 572 528 550 500 550 c 472 550 450 572 450 600 c h f*
0 0 0 RG 1 w 72 300 m 540 300 l S
0.5 g 72 200 150 60 re 300 200 150 60 re B
"
    .as_bytes()
    .to_vec();
    let mut content = content;
    content.extend_from_slice(
        text_block("F1", 12.0, 72.0, 740.0, 14.0, &["Vector shapes"]).as_bytes(),
    );
    document(&[page(content)], "", "")
}

/// A 4x4 RGB checker, 16x16 gray ramp (Flate), and an 8x8 baseline JPEG.
fn images() -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let mut rgb = Vec::new();
    for y in 0..4u8 {
        for x in 0..4u8 {
            if (x + y) % 2 == 0 {
                rgb.extend_from_slice(&[220, 40, 40]);
            } else {
                rgb.extend_from_slice(&[40, 40, 220]);
            }
        }
    }
    let raw = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &rgb,
    );
    let gray: Vec<u8> = (0..256u32).map(|i| (i % 16 * 16) as u8).collect();
    let flate = pdf.flate_stream(
        "/Type /XObject /Subtype /Image /Width 16 /Height 16 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &gray,
    );
    let jpeg = jpeg_bytes();
    let dct = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode",
        &jpeg,
    );
    let mut content = Vec::new();
    content.extend_from_slice(b"q 144 0 0 144 72 560 cm /Im1 Do Q\n");
    content.extend_from_slice(b"q 144 0 0 144 260 560 cm /Im2 Do Q\n");
    // The third picture is clipped to its left half: the importer's CropRect.
    content.extend_from_slice(b"q 448 560 72 144 re W n 144 0 0 144 448 560 cm /Im3 Do Q\n");
    content.extend_from_slice(
        text_block(
            "F1",
            12.0,
            72.0,
            740.0,
            14.0,
            &["Images: raw, Flate, DCT (clipped)"],
        )
        .as_bytes(),
    );
    let content = pdf.flate_stream("", &content);
    let page = pdf.add(format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [{LETTER}] /Contents {content} 0 R /Resources << {FONTS} /XObject << /Im1 {raw} 0 R /Im2 {flate} 0 R /Im3 {dct} 0 R >> >> >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    pdf.finish(catalog, "")
}

fn jpeg_bytes() -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in 0..8u8 {
        for x in 0..8u8 {
            pixels.extend_from_slice(&[x * 30, y * 30, 128]);
        }
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode(&pixels, 8, 8, image::ExtendedColorType::Rgb8)
        .expect("in-memory JPEG");
    out
}

fn rotated_text() -> Vec<u8> {
    let content = format!(
        "{}BT /F1 14 Tf 0 1 -1 0 560 200 Tm (Rotated ninety degrees) Tj ET\nBT /F1 12 Tf 0.866 0.5 -0.5 0.866 150 300 Tm (Thirty degrees) Tj ET\n",
        text_block(
            "F1",
            12.0,
            72.0,
            720.0,
            14.0,
            &["Upright text beside rotated runs"]
        )
    );
    document(&[page(content)], "", "")
}

fn tagged() -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let struct_root = pdf.reserve();
    let content = format!(
        "/H1 <</MCID 0>> BDC\n{}EMC\n/P <</MCID 1>> BDC\n{}EMC\n/Artifact BMC\n{}EMC\n",
        text_block("F3", 18.0, 72.0, 720.0, 22.0, &["Tagged heading"]),
        text_block("F2", 11.0, 72.0, 690.0, 14.0, &PROSE[..3]),
        text_block("F1", 8.0, 72.0, 40.0, 10.0, &["Page 1"]),
    );
    let content = pdf.flate_stream("", content.as_bytes());
    let page = pdf.add(format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [{LETTER}] /Contents {content} 0 R /Resources << {FONTS} >> /StructParents 0 >>"
    ));
    let heading = pdf.add(format!(
        "<< /Type /StructElem /S /H1 /P {struct_root} 0 R /Pg {page} 0 R /K 0 >>"
    ));
    let body = pdf.add(format!(
        "<< /Type /StructElem /S /P /P {struct_root} 0 R /Pg {page} 0 R /K 1 >>"
    ));
    pdf.set(
        struct_root,
        format!("<< /Type /StructTreeRoot /K [{heading} 0 R {body} 0 R] >>"),
    );
    pdf.set(
        tree,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    pdf.set(
        catalog,
        format!(
            "<< /Type /Catalog /Pages {tree} 0 R /MarkInfo << /Marked true >> /StructTreeRoot {struct_root} 0 R >>"
        ),
    );
    pdf.finish(catalog, "")
}

/// A Type3 font with two box glyphs and NO ToUnicode: its glyphs paint but
/// have no Unicode, which the importer must report (`native_text_invalid`).
fn type3() -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let square = pdf.stream("", b"1000 0 0 0 750 750 d1 0 0 750 750 re f");
    let bar = pdf.stream("", b"1000 0 0 0 750 750 d1 0 0 750 200 re f");
    let font = pdf.add(format!(
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 750 750] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /square {square} 0 R /bar {bar} 0 R >> /Encoding << /Type /Encoding /Differences [65 /square /bar] >> /FirstChar 65 /LastChar 66 /Widths [1000 1000] >>"
    ));
    let content = format!(
        "{}BT /T3 24 Tf 72 600 Td (ABABAB) Tj ET\n",
        text_block(
            "F1",
            12.0,
            72.0,
            720.0,
            14.0,
            &["Type3 glyphs without ToUnicode"]
        )
    );
    let content = pdf.flate_stream("", content.as_bytes());
    let page = pdf.add(format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [{LETTER}] /Contents {content} 0 R /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> /T3 {font} 0 R >> >> >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    pdf.finish(catalog, "")
}

/// An offset CropBox on page 1 and `/Rotate 90` on page 2 (A4).
fn page_boxes() -> Vec<u8> {
    let cropped = (
        "0 0 612 792".to_owned(),
        "/CropBox [36 36 576 756]".to_owned(),
        text_block(
            "F1",
            12.0,
            72.0,
            700.0,
            14.0,
            &["CropBox inset by half an inch"],
        )
        .into_bytes(),
        String::new(),
    );
    let rotated = (
        "0 0 595 842".to_owned(),
        "/Rotate 90".to_owned(),
        text_block(
            "F1",
            12.0,
            72.0,
            760.0,
            14.0,
            &["A4 page rotated for display"],
        )
        .into_bytes(),
        String::new(),
    );
    document(&[cropped, rotated], "", "")
}

/// JavaScript open action, an embedded file, a Launch link and an AcroForm
/// field: none may run or be extracted; all must be reported.
fn active_content() -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let js = pdf.add("<< /S /JavaScript /JS (app.alert\\('synthetic'\\);) >>");
    let file = pdf.stream("/Type /EmbeddedFile", b"attachment body");
    let spec = pdf.add(format!(
        "<< /Type /Filespec /F (note.txt) /EF << /F {file} 0 R >> >>"
    ));
    let content = text_block(
        "F1",
        12.0,
        72.0,
        720.0,
        14.0,
        &["Active content is reported, never run"],
    );
    let content = pdf.flate_stream("", content.as_bytes());
    let page = pdf.reserve();
    let link = pdf.add(format!(
        "<< /Type /Annot /Subtype /Link /Rect [72 700 300 730] /A << /S /Launch /F (calc.exe) >> /P {page} 0 R >>"
    ));
    let field = pdf.add(format!(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [72 600 300 630] /P {page} 0 R >>"
    ));
    pdf.set(
        page,
        format!(
            "<< /Type /Page /Parent {tree} 0 R /MediaBox [{LETTER}] /Contents {content} 0 R /Resources << {FONTS} >> /Annots [{link} 0 R {field} 0 R] >>"
        ),
    );
    pdf.set(
        tree,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    pdf.set(
        catalog,
        format!(
            "<< /Type /Catalog /Pages {tree} 0 R /OpenAction {js} 0 R /Names << /EmbeddedFiles << /Names [(note.txt) {spec} 0 R] >> >> /AcroForm << /Fields [{field} 0 R] >> >>"
        ),
    );
    pdf.finish(catalog, "")
}

fn encrypted() -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let content = pdf.stream("", b"BT /F1 12 Tf 72 720 Td (x) Tj ET");
    let page = pdf.add(format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [{LETTER}] /Contents {content} 0 R /Resources << {FONTS} >> >>"
    ));
    let encrypt = pdf.add(
        "<< /Filter /Standard /V 1 /R 2 /Length 40 /P -4 /O <28BF4E5E4E758A4164004E56FFFA01082E2E00B6D0683E802F0CA9FE6453697A> /U <00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF> >>",
    );
    pdf.set(
        tree,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    pdf.finish(
        catalog,
        &format!("/Encrypt {encrypt} 0 R /ID [<00112233445566778899AABBCCDDEEFF> <00112233445566778899AABBCCDDEEFF>]"),
    )
}

/// One content stream that inflates to 96 MiB of spaces.
fn flate_bomb() -> Vec<u8> {
    let content = vec![b' '; 96 * 1024 * 1024];
    document(&[page(content)], "", "")
}

/// Two million path operators on one page (about 12 MB decompressed).
fn operator_flood() -> Vec<u8> {
    let content = b"0 0 m\n".repeat(2_000_000);
    document(&[page(content)], "", "")
}

/// An image that declares 100,000 x 100,000 RGB pixels.
fn image_dimensions() -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let image = pdf.flate_stream(
        "/Type /XObject /Subtype /Image /Width 100000 /Height 100000 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &[0u8; 4096],
    );
    let content = pdf.stream("", b"q 500 0 0 500 50 50 cm /Big Do Q");
    let page = pdf.add(format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [{LETTER}] /Contents {content} 0 R /Resources << /XObject << /Big {image} 0 R >> >> >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    pdf.finish(catalog, "")
}

/// A form XObject that draws itself.
fn recursive_form() -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let form = pdf.reserve();
    pdf.set(
        form,
        stream_body(
            &format!(
                "/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Resources << /XObject << /Me {form} 0 R >> >>"
            ),
            b"0 0 10 10 re f /Me Do",
        ),
    );
    let content = pdf.stream("", b"/Me Do");
    let page = pdf.add(format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [{LETTER}] /Contents {content} 0 R /Resources << /XObject << /Me {form} 0 R >> >> >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    pdf.finish(catalog, "")
}

fn truncated() -> Vec<u8> {
    let full = prose();
    full[..full.len() / 3].to_vec()
}

/// A PDF header followed by deterministic pseudo-random bytes.
fn garbage() -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut state: u32 = 0x9E37_79B9;
    for _ in 0..8192 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        out.push((state & 0xFF) as u8);
    }
    out
}

/// 2,500 empty pages: over the page-count limit.
fn page_flood() -> Vec<u8> {
    let pages: Vec<_> = (0..2_500).map(|_| page(b"".to_vec())).collect();
    document(&pages, "", "")
}

/// A page 200,000 pt on a side: over the page-dimension limit.
fn huge_page() -> Vec<u8> {
    let huge = (
        "0 0 200000 200000".to_owned(),
        String::new(),
        text_block("F1", 12.0, 72.0, 720.0, 14.0, &["Huge page"]).into_bytes(),
        String::new(),
    );
    document(&[huge], "", "")
}

/// A simple TrueType font embedding the WHOLE Caladea Regular program
/// (Apache-2.0, bundled in `casual-doc-layout/fonts`) as `FontFile2`, with
/// WinAnsi widths read from its own `hmtx`: the case where the embedded program
/// has a Unicode cmap and can draw its own text in the editor (`docs/182` G-1).
fn embedded_truetype() -> Vec<u8> {
    use skrifa::MetadataProvider;
    let program = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/casual-doc-layout/fonts/Caladea-Regular.ttf"),
    )
    .expect("bundled Caladea");
    let face = skrifa::FontRef::new(&program).expect("sfnt");
    let charmap = face.charmap();
    let upem = f32::from(
        face.metrics(
            skrifa::instance::Size::unscaled(),
            skrifa::instance::LocationRef::default(),
        )
        .units_per_em,
    );
    let metrics = face.glyph_metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );
    let widths: Vec<String> = (32u32..=126)
        .map(|code| {
            let advance = charmap
                .map(code)
                .and_then(|gid| metrics.advance_width(gid))
                .unwrap_or(0.0);
            format!("{}", (advance * 1000.0 / upem).round() as i64)
        })
        .collect();
    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let file = pdf.flate_stream(&format!("/Length1 {}", program.len()), &program);
    let descriptor = pdf.add(format!(
        "<< /Type /FontDescriptor /FontName /Caladea-Regular /FontFamily (Caladea) /Flags 34 /FontBBox [-200 -250 1200 950] /ItalicAngle 0 /Ascent 900 /Descent -250 /CapHeight 650 /StemV 80 /FontFile2 {file} 0 R >>"
    ));
    let font = pdf.add(format!(
        "<< /Type /Font /Subtype /TrueType /BaseFont /Caladea-Regular /FirstChar 32 /LastChar 126 /Widths [{}] /Encoding /WinAnsiEncoding /FontDescriptor {descriptor} 0 R >>",
        widths.join(" ")
    ));
    let content = format!(
        "{}{}",
        text_block("C1", 20.0, 72.0, 720.0, 24.0, &["Embedded Caladea"]),
        text_block("C1", 12.0, 72.0, 680.0, 15.0, &PROSE[..4]),
    );
    let content = pdf.flate_stream("", content.as_bytes());
    let page = pdf.add(format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [{LETTER}] /Contents {content} 0 R /Resources << /Font << /C1 {font} 0 R >> >> >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    pdf.finish(catalog, "")
}
