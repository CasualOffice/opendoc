// SPDX-License-Identifier: Apache-2.0

//! The `docs/176` V2 gate: the same document renders the same pixels on every
//! platform CI runs (`test` on Linux x64, `platform` on macOS arm64 and
//! Windows x64), with the bundled faces only.
//!
//! The golden is a hash of each page's RGBA, not of the PNG, so an encoder
//! change cannot move it; only a pixel can. It is written by running this test
//! with `OPENDOC_BLESS=1` and never edited by hand (SKILL §5). A red here on one
//! platform alone is the finding the gate exists to make.

use opendoc::Document;

const SAMPLE: &[u8] = include_bytes!("../../../sample.docx");
const GOLDEN: &str = include_str!("pixels.golden");
const DPI: f32 = 72.0;

/// FNV-1a, 64-bit: dependency-free and stable across platforms.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn page_hashes(name: &str, doc: &Document) -> Vec<String> {
    (0..doc.page_count().min(3))
        .map(|page| {
            let image = doc.render_page(page, DPI).expect("renders");
            format!(
                "{name} page {page} {}x{} {:016x}",
                image.width,
                image.height,
                fnv1a(&image.rgba)
            )
        })
        .collect()
}

fn built() -> Document {
    Document::from_spec_json(
        r#"{"sections":[{"children":[
            {"type":"paragraph","heading":"Heading1","text":"Deterministic pixels"},
            {"type":"paragraph","children":[
                {"type":"text","text":"Body text in Calibri's metric twin, "},
                {"type":"text","text":"bold","bold":true},
                {"type":"text","text":", and a table:"}]},
            {"type":"table","rows":[
                {"children":[{"children":[{"type":"paragraph","text":"A1"}]},
                             {"children":[{"type":"paragraph","text":"B1"}]}]},
                {"children":[{"children":[{"type":"paragraph","text":"A2"}]},
                             {"children":[{"type":"paragraph","text":"B2"}]}]}]}
        ]}]}"#,
        &[],
    )
    .expect("builds")
}

#[test]
fn pages_render_the_same_pixels_on_every_platform() {
    let mut lines = page_hashes("sample.docx", &Document::open(SAMPLE).expect("opens"));
    lines.extend(page_hashes("built", &built()));
    let actual = lines.join("\n") + "\n";
    if std::env::var_os("OPENDOC_BLESS").is_some() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pixels.golden");
        std::fs::write(path, &actual).expect("writes the golden");
        return;
    }
    assert_eq!(
        actual,
        GOLDEN,
        "page pixels differ from the golden on {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

#[test]
fn a_page_renders_identically_twice_in_one_process() {
    let doc = built();
    let a = doc.render_page(0, DPI).unwrap();
    let b = Document::from_spec_json(&serde_json::to_string(&serde_json::json!({})).unwrap(), &[])
        .map(|_| doc.render_page(0, DPI).unwrap())
        .unwrap();
    assert_eq!(a, b);
}
