// SPDX-License-Identifier: Apache-2.0

//! The **oracle side** of the comparison: drive LibreOffice over a document and
//! reduce the PDF it produces to the same quantity
//! [`casual_doc_layout::text_region`] reduces one of our laid-out pages to.
//!
//! # Prior art, named before inventing anything
//!
//! This is **reference-oracle (golden-image) testing with tolerances** — the same
//! shape as any rendering regression suite: run the input through an independent
//! implementation, reduce both outputs to a comparable quantity, and report the
//! difference as a number rather than as an impression. The only project-specific
//! decisions are *which* quantity (the text region, not ink and not block boxes —
//! see [`casual_doc_layout::text_region`]) and *which* oracle (LibreOffice, as a
//! layout proxy for Word, chosen in `docs/46`).
//!
//! # The pipeline, and why each step
//!
//! ```text
//! .docx --soffice --convert-to pdf--> .pdf --pdftotext -bbox--> word boxes
//!                                          --pdffonts---------> embedded faces
//! ```
//!
//! `pdftotext -bbox` is the measurement instrument: it reports, per **word**, the
//! pen extents and the font descriptor's ascent/descent about the baseline. That
//! is reproducible from our shaped runs exactly, which is what makes the two
//! sides comparable without a fudge factor.
//!
//! `pdffonts` is *provenance*: it names the faces LibreOffice actually embedded,
//! read off the artifact rather than off the machine. Installing a font package
//! is not the same as the renderer using it — that has already gone wrong here
//! once, with geometry coming back ~17% wide because fontconfig substituted.
//!
//! # Relationship to `scripts/oracle/extract-geometry.sh`
//!
//! That script performs the same reduction in Python and is the **blessing**
//! path: its output is what `fixtures/oracle/*.geom.json` holds and what the CI
//! geometry gate compares against. This module is the **investigation** path: it
//! reduces the same PDF for an arbitrary document, at line granularity, with no
//! committed reference involved.
//!
//! Two implementations of one rule diverge, so they are pinned to each other:
//! `tests::the_rust_reduction_reproduces_the_blessing_scripts_numbers` runs this
//! reducer over a committed sample of the script's own inputs
//! (`fixtures/oracle/samples/`) and asserts it reproduces the script's committed
//! output. If the script's semantics change, that test goes red.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One word box as `pdftotext -bbox` reports it, in PDF points.
#[derive(Clone, Debug, PartialEq)]
pub struct WordBox {
    /// Left pen edge.
    pub x0: f64,
    /// Top edge (baseline − the descriptor's ascent).
    pub y0: f64,
    /// Right pen edge.
    pub x1: f64,
    /// Bottom edge (baseline + the descriptor's descent).
    pub y1: f64,
    /// The word's characters, used for the font-parity coverage test.
    pub text: String,
}

/// One PDF page's word boxes and size, in PDF points.
#[derive(Clone, Debug, PartialEq)]
pub struct WordPage {
    /// Page width in points.
    pub width: f64,
    /// Page height in points.
    pub height: f64,
    /// Every word box on the page, in the extractor's order.
    pub words: Vec<WordBox>,
}

/// One oracle line: a group of word boxes sharing a band, in twips.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleLine {
    /// Left edge of the leftmost word.
    pub x0: i32,
    /// Top edge of the topmost word.
    pub y0: i32,
    /// Right edge of the rightmost word.
    pub x1: i32,
    /// Bottom edge of the bottommost word.
    pub y1: i32,
    /// Whether every code point on the line is covered by the pinned faces.
    pub pinned: bool,
    /// Words on the line.
    pub words: usize,
}

/// One oracle page reduced to the comparable quantity, in twips.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OraclePage {
    /// `[width, height]`.
    pub size: [i32; 2],
    /// The union of the pinned lines' boxes, or `None` when the page has none.
    pub content_bbox: Option<[i32; 4]>,
    /// The summed vertical extent of the lines excluded for font parity.
    pub excluded_extent: i32,
    /// How many lines were excluded (review context; grouping is per-renderer).
    pub excluded_lines: usize,
    /// Every line, excluded ones included, top to bottom.
    pub lines: Vec<OracleLine>,
}

/// A whole document as the oracle renders it.
#[derive(Clone, Debug)]
pub struct OracleDocument {
    /// The `soffice --version` string that produced it.
    pub version: String,
    /// The faces the PDF embedded, subset prefix stripped, sorted.
    pub fonts: Vec<String>,
    /// One entry per page.
    pub pages: Vec<OraclePage>,
}

/// Code points the pinned faces (Liberation Sans/Serif/Mono, Carlito, Caladea)
/// all cover, as inclusive ranges.
///
/// Deliberately conservative and **identical to the blessing script's
/// `PINNED_RANGES`**: a range left out only excludes more text from the
/// comparison, whereas a range wrongly claimed here silently compares glyphs one
/// of the two renderers had to substitute.
const PINNED_RANGES: &[(u32, u32)] = &[
    (0x0020, 0x007E), // Basic Latin (printable)
    (0x00A0, 0x017F), // Latin-1 Supplement + Latin Extended-A
    (0x2010, 0x2027), // General Punctuation: dashes, quotes, bullets, ellipsis
    (0x2030, 0x205E), // General Punctuation: per-mille, primes, spaces, marks
    (0x20AC, 0x20AC), // Euro sign
];

/// Whether every code point in `text` is covered by the pinned faces. Whitespace
/// is ignored: it carries no glyph of its own, and an extractor may hand back
/// markup whitespace around a word's character data.
#[must_use]
pub fn is_pinned_text(text: &str) -> bool {
    text.chars().filter(|c| !c.is_whitespace()).all(|c| {
        let cp = c as u32;
        PINNED_RANGES.iter().any(|&(lo, hi)| lo <= cp && cp <= hi)
    })
}

/// PDF points (1/72in) to twips (1/1440in), rounded to the nearest twip.
#[must_use]
pub fn twips(points: f64) -> i32 {
    // `round` then truncate: the values are page coordinates, far inside i32.
    #[allow(clippy::cast_possible_truncation)]
    {
        (points * 20.0).round() as i32
    }
}

/// The tools this module shells out to, checked up front so a missing one is a
/// clear message rather than a confusing failure three steps later.
const REQUIRED_TOOLS: [&str; 2] = ["pdftotext", "pdffonts"];

/// Where LibreOffice lives, preferring an explicit `SOFFICE` override, then the
/// macOS app bundle, then whatever is on `PATH`.
#[must_use]
pub fn soffice_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("SOFFICE") {
        return PathBuf::from(explicit);
    }
    let bundled = PathBuf::from("/Applications/LibreOffice.app/Contents/MacOS/soffice");
    if bundled.is_file() {
        return bundled;
    }
    PathBuf::from("soffice")
}

/// `soffice --version`, trimmed.
///
/// # Errors
/// If LibreOffice cannot be run.
pub fn soffice_version() -> Result<String, Box<dyn Error>> {
    let output = Command::new(soffice_path()).arg("--version").output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Renders `input` with LibreOffice and reduces the resulting PDF to per-page
/// comparable geometry.
///
/// `workdir` receives the intermediate `.pdf`, `.bbox.html` and `.fonts.txt`, so
/// a caller can keep them for inspection.
///
/// # Errors
/// If a required tool is missing, LibreOffice produces no PDF, or an extractor
/// fails.
pub fn render(input: &Path, workdir: &Path) -> Result<OracleDocument, Box<dyn Error>> {
    for tool in REQUIRED_TOOLS {
        if Command::new(tool).arg("-v").output().is_err() {
            return Err(
                format!("required tool not found on PATH: {tool} (install poppler-utils)").into(),
            );
        }
    }
    std::fs::create_dir_all(workdir)?;

    let status = Command::new(soffice_path())
        .args([
            "--headless",
            "--norestore",
            "--nolockcheck",
            "--convert-to",
            "pdf",
            "--outdir",
        ])
        .arg(workdir)
        .arg(input)
        .output()?;
    let stem = input
        .file_stem()
        .ok_or("the input path has no file name")?
        .to_string_lossy()
        .into_owned();
    let pdf = workdir.join(format!("{stem}.pdf"));
    if !pdf.is_file() {
        return Err(format!(
            "LibreOffice produced no PDF for {}: {}",
            input.display(),
            String::from_utf8_lossy(&status.stderr).trim()
        )
        .into());
    }

    let bbox = workdir.join("bbox.html");
    let bbox_status = Command::new("pdftotext")
        .arg("-bbox")
        .arg(&pdf)
        .arg(&bbox)
        .status()?;
    if !bbox_status.success() {
        return Err("pdftotext -bbox failed".into());
    }
    let fonts_output = Command::new("pdffonts").arg(&pdf).output()?;
    if !fonts_output.status.success() {
        return Err("pdffonts failed".into());
    }
    let fonts_text = String::from_utf8_lossy(&fonts_output.stdout).into_owned();
    std::fs::write(workdir.join("fonts.txt"), &fonts_text)?;

    let pages = parse_word_boxes(&std::fs::read_to_string(&bbox)?)?;
    Ok(OracleDocument {
        version: soffice_version().unwrap_or_default(),
        fonts: embedded_fonts(&fonts_text),
        pages: pages.iter().map(reduce_page).collect(),
    })
}

/// The distinct faces a `pdffonts` listing names, subset prefix stripped, sorted.
///
/// `pdffonts` prints a two-line header, then one row per font whose first column
/// is the (possibly subsetted) base font name. `BAAAAA+LiberationSerif` becomes
/// `LiberationSerif`: a subset tag is six letters chosen per-document and carries
/// no information.
#[must_use]
pub fn embedded_fonts(listing: &str) -> Vec<String> {
    listing
        .lines()
        .skip(2)
        .filter_map(|line| {
            let name = line.split(' ').next().unwrap_or_default().trim();
            (!name.is_empty()).then(|| {
                name.split_once('+')
                    .map_or(name, |(_, base)| base)
                    .to_owned()
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Parses `pdftotext -bbox` XHTML into per-page word boxes.
///
/// # Errors
/// If the XML is malformed or a `<page>`/`<word>` lacks its geometry attributes.
pub fn parse_word_boxes(xhtml: &str) -> Result<Vec<WordPage>, Box<dyn Error>> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xhtml);
    reader.config_mut().check_end_names = false;
    let mut pages: Vec<WordPage> = Vec::new();
    let mut in_word = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            Event::Start(tag) | Event::Empty(tag) => {
                let name = tag.local_name();
                match name.as_ref() {
                    b"page" => {
                        pages.push(WordPage {
                            width: attr_f64(&tag, b"width")?,
                            height: attr_f64(&tag, b"height")?,
                            words: Vec::new(),
                        });
                    }
                    b"word" => {
                        let page = pages.last_mut().ok_or("a <word> outside any <page>")?;
                        page.words.push(WordBox {
                            x0: attr_f64(&tag, b"xMin")?,
                            y0: attr_f64(&tag, b"yMin")?,
                            x1: attr_f64(&tag, b"xMax")?,
                            y1: attr_f64(&tag, b"yMax")?,
                            text: String::new(),
                        });
                        in_word = true;
                    }
                    _ => {}
                }
            }
            Event::Text(text) if in_word => {
                if let Some(word) = pages.last_mut().and_then(|p| p.words.last_mut()) {
                    word.text.push_str(&text.decode()?);
                }
            }
            Event::End(tag) if tag.local_name().as_ref() == b"word" => in_word = false,
            _ => {}
        }
        buf.clear();
    }
    Ok(pages)
}

/// Reads one `f64` attribute off a start tag.
fn attr_f64(tag: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Result<f64, Box<dyn Error>> {
    let attribute = tag
        .try_get_attribute(key)?
        .ok_or_else(|| format!("missing attribute {}", String::from_utf8_lossy(key)))?;
    Ok(std::str::from_utf8(&attribute.value)?.parse::<f64>()?)
}

/// Groups word boxes into lines by vertical overlap — the same rule
/// [`casual_doc_layout::text_region`] applies to our glyph runs, and the same
/// rule the blessing script applies to these very boxes: sort by top edge, then
/// start a new line whenever a box begins at or below the running bottom of the
/// current one.
fn group_into_lines(words: &[WordBox]) -> Vec<Vec<&WordBox>> {
    let mut ordered: Vec<&WordBox> = words.iter().collect();
    ordered.sort_by(|a, b| {
        a.y0.partial_cmp(&b.y0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut lines: Vec<Vec<&WordBox>> = Vec::new();
    let mut bottom = f64::NEG_INFINITY;
    for word in ordered {
        match lines.last_mut() {
            Some(line) if word.y0 < bottom => {
                bottom = bottom.max(word.y1);
                line.push(word);
            }
            _ => {
                bottom = word.y1;
                lines.push(vec![word]);
            }
        }
    }
    lines
}

/// Reduces one page of word boxes to the comparable quantity, in twips.
#[must_use]
pub fn reduce_page(page: &WordPage) -> OraclePage {
    let mut content_bbox: Option<[f64; 4]> = None;
    let mut excluded_extent = 0.0_f64;
    let mut excluded_lines = 0;
    let mut lines = Vec::new();

    for line in group_into_lines(&page.words) {
        let pinned = line.iter().all(|word| is_pinned_text(&word.text));
        let x0 = line.iter().map(|w| w.x0).fold(f64::INFINITY, f64::min);
        let y0 = line.iter().map(|w| w.y0).fold(f64::INFINITY, f64::min);
        let x1 = line.iter().map(|w| w.x1).fold(f64::NEG_INFINITY, f64::max);
        let y1 = line.iter().map(|w| w.y1).fold(f64::NEG_INFINITY, f64::max);
        if pinned {
            content_bbox = Some(match content_bbox {
                None => [x0, y0, x1, y1],
                Some([bx0, by0, bx1, by1]) => [bx0.min(x0), by0.min(y0), bx1.max(x1), by1.max(y1)],
            });
        } else {
            excluded_lines += 1;
            excluded_extent += y1 - y0;
        }
        lines.push(OracleLine {
            x0: twips(x0),
            y0: twips(y0),
            x1: twips(x1),
            y1: twips(y1),
            pinned,
            words: line.len(),
        });
    }

    OraclePage {
        size: [twips(page.width), twips(page.height)],
        content_bbox: content_bbox.map(|b| b.map(twips)),
        excluded_extent: twips(excluded_extent),
        excluded_lines,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The blessing script's own inputs and output, committed so the two
    /// implementations of the reduction are pinned to each other without needing
    /// LibreOffice or Python in the test environment. See the module docs.
    const SAMPLE_BBOX: &str =
        include_str!("../../../fixtures/oracle/samples/real-producer-hyperlinks.bbox.html");
    const SAMPLE_FONTS: &str =
        include_str!("../../../fixtures/oracle/samples/real-producer-hyperlinks.fonts.txt");
    const SAMPLE_REDUCED: &str =
        include_str!("../../../fixtures/oracle/samples/real-producer-hyperlinks.reduced.json");

    #[test]
    fn the_rust_reduction_reproduces_the_blessing_scripts_numbers() {
        let expected: serde_json::Value =
            serde_json::from_str(SAMPLE_REDUCED).expect("the committed reduction is valid JSON");
        let pages = parse_word_boxes(SAMPLE_BBOX).expect("the committed sample parses");
        let ours: Vec<OraclePage> = pages.iter().map(reduce_page).collect();

        assert_eq!(
            embedded_fonts(SAMPLE_FONTS),
            expected["fonts"]
                .as_array()
                .expect("fonts array")
                .iter()
                .map(|f| f.as_str().expect("font name").to_owned())
                .collect::<Vec<_>>(),
            "font provenance must be read the same way by both reducers"
        );

        let expected_pages = expected["pages"].as_array().expect("pages array");
        assert_eq!(
            ours.len(),
            expected_pages.len(),
            "page count must match the blessing script's"
        );
        for (index, (ours, theirs)) in ours.iter().zip(expected_pages).enumerate() {
            let size: Vec<i32> = theirs["sizeTwips"]
                .as_array()
                .expect("sizeTwips")
                .iter()
                .map(|v| v.as_i64().expect("twips") as i32)
                .collect();
            assert_eq!(ours.size.to_vec(), size, "page {} size", index + 1);
            let bbox: Option<Vec<i32>> = theirs["contentBboxTwips"].as_array().map(|b| {
                b.iter()
                    .map(|v| v.as_i64().expect("twips") as i32)
                    .collect()
            });
            assert_eq!(
                ours.content_bbox.map(|b| b.to_vec()),
                bbox,
                "page {} content bbox: the Rust reducer and \
                 scripts/oracle/extract-geometry.sh must agree exactly",
                index + 1
            );
            assert_eq!(
                i64::from(ours.excluded_extent),
                theirs["excludedExtentTwips"].as_i64().expect("extent"),
                "page {} excluded extent",
                index + 1
            );
            assert_eq!(
                ours.excluded_lines as i64,
                theirs["excludedLines"].as_i64().expect("lines"),
                "page {} excluded line count",
                index + 1
            );
        }
    }

    #[test]
    fn the_sample_reduces_to_lines_with_words_on_them() {
        let pages = parse_word_boxes(SAMPLE_BBOX).expect("the committed sample parses");
        let reduced = reduce_page(&pages[0]);
        assert!(
            reduced.lines.len() >= 3,
            "the sample page has several lines, not {}",
            reduced.lines.len()
        );
        assert!(
            reduced.lines.iter().all(|line| line.words > 0),
            "every grouped line carries at least one word"
        );
        assert!(
            reduced.lines.windows(2).all(|w| w[0].y0 <= w[1].y0),
            "lines are reported top to bottom"
        );
    }

    #[test]
    fn coverage_excludes_text_the_pinned_faces_cannot_shape() {
        assert!(is_pinned_text(
            "The quick brown fox — \u{201c}jumps\u{201d}"
        ));
        assert!(is_pinned_text("caf\u{e9} na\u{ef}ve \u{20ac}12"));
        assert!(!is_pinned_text("\u{65e5}\u{672c}\u{8a9e}"));
        assert!(!is_pinned_text(
            "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}"
        ));
        // LibreOffice's list bullet lives in the private use area.
        assert!(!is_pinned_text("\u{f0b7}"));
    }

    #[test]
    fn a_subset_prefix_is_stripped_from_font_provenance() {
        let listing = "name type\n---- ----\nBAAAAA+LiberationSans-Bold TrueType\n\
                       CAAAAA+LiberationSerif TrueType\nLiberationSerif TrueType\n";
        assert_eq!(
            embedded_fonts(listing),
            ["LiberationSans-Bold", "LiberationSerif"],
            "a subset tag carries no information and the set is deduplicated"
        );
    }

    #[test]
    fn points_convert_to_twips_by_rounding() {
        assert_eq!(twips(0.0), 0);
        assert_eq!(twips(1.0), 20);
        assert_eq!(twips(56.8), 1136);
        assert_eq!(twips(595.303_937), 11906);
    }
}
