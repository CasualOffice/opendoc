// SPDX-License-Identifier: Apache-2.0

//! SVG pages (`docs/176` §12.13): well-formed, the page's size, real text
//! over the glyph outlines, pictures embedded, and the same bytes on every
//! platform, held by `svg.golden`.
//!
//! `svg.golden` holds an FNV-1a hash and the byte length of each SVG page for
//! the first three pages of the committed `sample.docx` and for one built
//! document, with the bundled faces only. It is written by running this test
//! with `OPENDOC_BLESS=1`, never by hand (SKILL §5). Its `#` lines record
//! where it was blessed; CI's Linux `test` job and the macOS and Windows
//! `platform` jobs check it. The SVG is text built from integers and one-
//! decimal numbers, with glyph outlines read by skrifa from the bundled faces,
//! so a red here on one platform alone is a real finding, as for the pixels.

use opendoc::Document;
use quick_xml::Reader;
use quick_xml::events::Event;

const SAMPLE: &[u8] = include_bytes!("../../../sample.docx");
const GOLDEN: &str = include_str!("svg.golden");

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn built() -> Document {
    Document::from_spec_json(
        r#"{"sections":[{"children":[
            {"type":"paragraph","heading":"Heading1","text":"Vector pages"},
            {"type":"paragraph","children":[
                {"type":"text","text":"Body text with "},
                {"type":"text","text":"bold","bold":true},
                {"type":"text","text":", underlined","underline":{}},
                {"type":"text","text":" & <escaped> characters."}]},
            {"type":"table","rows":[
                {"children":[{"children":[{"type":"paragraph","text":"A1"}]},
                             {"children":[{"type":"paragraph","text":"B1"}]}]}]}
        ]}]}"#,
        &[],
    )
    .expect("builds")
}

/// Every element name, in order, and the text of every `<text>` element; fails
/// on anything that is not well-formed XML.
fn parse(svg: &str) -> (Vec<String>, Vec<String>) {
    let mut reader = Reader::from_str(svg);
    let mut names = Vec::new();
    let mut texts = Vec::new();
    let mut in_text = false;
    loop {
        match reader.read_event().expect("well-formed XML") {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                in_text = name == "text";
                names.push(name);
            }
            Event::Empty(e) => names.push(String::from_utf8_lossy(e.name().as_ref()).into_owned()),
            Event::Text(t) if in_text => {
                texts.push(t.decode().expect("text").into_owned());
            }
            Event::GeneralRef(r) if in_text => {
                let entity = String::from_utf8_lossy(r.as_ref()).into_owned();
                let ch = match entity.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    _ => "?",
                };
                if let Some(last) = texts.last_mut() {
                    last.push_str(ch);
                }
            }
            Event::End(_) => in_text = false,
            Event::Eof => break,
            _ => {}
        }
    }
    (names, texts)
}

#[test]
fn an_svg_page_is_well_formed_sized_and_carries_real_text() {
    let doc = built();
    let svg = doc.render_page_svg(0).unwrap();
    let (names, texts) = parse(&svg);
    assert_eq!(names.first().map(String::as_str), Some("svg"));
    let size = &doc.page_sizes()[0];
    assert!(
        svg.contains(&format!("width=\"{}pt\"", size.width_pt.round()))
            || svg.contains(&format!("width=\"{}pt\"", size.width_pt)),
        "the page's width in points"
    );
    assert!(
        names.iter().filter(|n| *n == "use").count() > 20,
        "glyph outlines are placed"
    );
    let joined = texts.join("");
    for words in [
        "Vector pages",
        "Body text with",
        "bold",
        "A1",
        "B1",
        "& <escaped> characters.",
    ] {
        assert!(
            joined.contains(words),
            "the text layer reads {words:?}: {joined:?}"
        );
    }
    assert!(
        doc.render_page_svg(1).is_err(),
        "past the last page is an error"
    );
}

#[test]
fn pictures_are_embedded_and_every_sample_page_renders() {
    let doc = Document::open(SAMPLE).unwrap();
    let with_images = (0..doc.page_count())
        .map(|page| doc.render_page_svg(page).unwrap())
        .inspect(|svg| {
            parse(svg);
        })
        .filter(|svg| svg.contains("data:image/"))
        .count();
    assert!(with_images > 0, "the sample's pictures are embedded");
}

#[test]
fn svg_pages_are_the_same_bytes_on_every_platform() {
    let mut lines = Vec::new();
    let sample = Document::open(SAMPLE).unwrap();
    for (name, doc) in [("sample.docx", &sample), ("built", &built())] {
        for page in 0..doc.page_count().min(3) {
            let svg = doc.render_page_svg(page).unwrap();
            lines.push(format!(
                "{name} page {page} {} bytes {:016x}",
                svg.len(),
                fnv1a(svg.as_bytes())
            ));
        }
    }
    let actual = lines.join("\n") + "\n";
    if std::env::var_os("OPENDOC_BLESS").is_some() {
        let header = format!(
            "# SVG page hashes (FNV-1a) and lengths; bundled faces only.\n\
             # Blessed on {} {} by `OPENDOC_BLESS=1 cargo test -p opendoc --test svg`.\n\
             # Checked in CI on Linux x64 (test), macOS arm64 and Windows x64 (platform).\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/svg.golden");
        std::fs::write(path, header + &actual).expect("writes the golden");
        return;
    }
    let expected: String = GOLDEN
        .replace("\r\n", "\n")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(
        actual,
        expected,
        "SVG pages differ from the golden on {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

/// `(mean absolute difference per channel, share of pixels differing by more
/// than a quarter of the range)` between the PNG raster of a page and the SVG
/// of the same page rasterized by resvg, at 72 dpi.
fn raster_vs_svg(doc: &Document, page: usize) -> (f64, f64) {
    let png = doc.render_page(page, 72.0).unwrap();
    let svg = doc.render_page_svg(page).unwrap();
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(png.width, png.height).unwrap();
    // usvg reads `pt` at 96 px per inch; scale its page onto the 72 dpi raster.
    let scale = png.width as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    if let Some(dir) = std::env::var_os("OPENDOC_SVG_DUMP") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::write(
            dir.join(format!("p{page}-svg.png")),
            pixmap.encode_png().unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join(format!("p{page}-raster.png")),
            doc.render_page_png(page, 72.0).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join(format!("p{page}.svg")), &svg).unwrap();
    }
    let ours = pixmap.data();
    assert_eq!(ours.len(), png.rgba.len());
    let mut total = 0_u64;
    let mut far = 0_u64;
    for (a, b) in png.rgba.chunks_exact(4).zip(ours.chunks_exact(4)) {
        let worst = a
            .iter()
            .zip(b)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        total += a
            .iter()
            .zip(b)
            .map(|(x, y)| u64::from(x.abs_diff(*y)))
            .sum::<u64>();
        if worst > 64 {
            far += 1;
        }
    }
    let pixels = (png.rgba.len() / 4) as f64;
    (total as f64 / (pixels * 4.0), far as f64 / pixels)
}

#[test]
fn an_svg_page_draws_what_the_png_page_draws() {
    // The SVG is the same layout as the PNG: rasterized by an independent
    // renderer (resvg, built without a text stack, so the transparent text
    // layer adds nothing), it matches the engine's own raster up to
    // antialiasing. Pages without pictures, because this resvg build decodes
    // no raster images.
    let sample = Document::open(SAMPLE).unwrap();
    for (name, doc, page) in [("built", &built(), 0), ("sample.docx", &sample, 1)] {
        let (mean, far) = raster_vs_svg(doc, page);
        assert!(
            mean < 1.0,
            "{name} page {page}: mean channel difference {mean:.3}"
        );
        assert!(
            far < 0.002,
            "{name} page {page}: {:.3}% of pixels differ visibly",
            far * 100.0
        );
    }
}
