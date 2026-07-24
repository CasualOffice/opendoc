//! Differential text-fidelity harness.
//!
//! For each `.docx` argument, extracts the document text two ways — through the
//! OpenDoc importer and through LibreOffice (`soffice --convert-to txt`) — and
//! reports whether they agree after whitespace normalization. Until the Phase-2
//! writer exists, text agreement is our round-trip-fidelity proxy: it measures
//! whether import recovers the document's textual content that LibreOffice sees.
//!
//! This is an evaluation tool, not a CI unit test: it shells out to `soffice`.
//! Usage: `cargo run -p opendoc-fidelity -- <file.docx> [more.docx ...]`

// A CLI reporting tool legitimately writes to stdout/stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use casual_doc_import::{ImportConfig, import_package};
use casual_doc_model::v1::{BlockNode, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: opendoc-fidelity <file.docx> [more.docx ...]");
        std::process::exit(2);
    }

    let mut failures = 0_usize;
    for path in &paths {
        match evaluate(Path::new(path)) {
            Ok(result) => {
                let status = if result.matches { "PASS" } else { "DIFF" };
                println!(
                    "{status} {path}  (ours={} chars, libre={} chars, word-match={:.0}%)",
                    result.ours.chars().count(),
                    result.libre.chars().count(),
                    result.similarity * 100.0
                );
                if !result.matches {
                    failures += 1;
                    print_diff(&result.ours, &result.libre);
                }
            }
            Err(error) => {
                failures += 1;
                println!("ERROR {path}: {error}");
            }
        }
    }
    if failures > 0 {
        std::process::exit(1);
    }
}

struct Evaluation {
    ours: String,
    libre: String,
    matches: bool,
    similarity: f64,
}

fn evaluate(path: &Path) -> Result<Evaluation, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let ours = extract_ours(&bytes)?;
    let libre = extract_libre(path)?;
    let ours_words = words(&normalize(&ours));
    let libre_words = words(&normalize(&libre));
    let similarity = word_similarity(&ours_words, &libre_words);
    let matches = {
        let (mut a, mut b) = (ours_words.clone(), libre_words.clone());
        a.sort();
        b.sort();
        a == b
    };
    Ok(Evaluation {
        ours,
        libre,
        matches,
        similarity,
    })
}

/// Extracts document text through the OpenDoc importer.
fn extract_ours(bytes: &[u8]) -> Result<String, Box<dyn Error>> {
    let mut package = DocxPackage::open(bytes, PackageLimits::default())?;
    let import = import_package(&mut package, ImportConfig::default())?;
    let mut out = String::new();
    push_blocks_text(import.document.body(), &mut out);
    // Footnote and endnote body text (LibreOffice's txt export includes it).
    for (_, note) in import.document.definitions().footnotes.iter() {
        push_blocks_text(&note.blocks, &mut out);
    }
    for (_, note) in import.document.definitions().endnotes.iter() {
        push_blocks_text(&note.blocks, &mut out);
    }
    Ok(out)
}

/// Appends the text of a block sequence, recursing through table cells so cell
/// text counts toward the fidelity comparison.
fn push_blocks_text(blocks: &[BlockNode], out: &mut String) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                for inline in &paragraph.inlines {
                    push_inline_text(inline, out);
                }
                out.push('\n');
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        push_blocks_text(&cell.blocks, out);
                    }
                }
            }
        }
    }
}

fn push_inline_text(inline: &InlineNode, out: &mut String) {
    match inline {
        InlineNode::Run(run) => out.push_str(&run.text),
        InlineNode::Tab(_) => out.push('\t'),
        InlineNode::Break(_) => out.push('\n'),
        InlineNode::Drawing(_) => {}
        InlineNode::Hyperlink(link) => {
            for child in &link.inlines {
                push_inline_text(child, out);
            }
        }
        InlineNode::Field(field) => {
            // The field's cached result is the text a reader sees.
            for child in &field.inlines {
                push_inline_text(child, out);
            }
        }
        InlineNode::TextBox(text_box) => push_blocks_text(&text_box.blocks, out),
        // A note reference renders as a mark/number, not source text; the note
        // body text is appended separately from the definitions.
        InlineNode::NoteReference(_) => {}
    }
}

/// Extracts document text through LibreOffice headless conversion.
fn extract_libre(path: &Path) -> Result<String, Box<dyn Error>> {
    let scratch = unique_temp_dir()?;
    let profile = format!("file://{}/profile", scratch.display());
    let status = Command::new("soffice")
        .args([
            "--headless",
            "--convert-to",
            "txt:Text",
            "--outdir",
            &scratch.to_string_lossy(),
            &format!("-env:UserInstallation={profile}"),
        ])
        .arg(path)
        .status()?;
    if !status.success() {
        fs::remove_dir_all(&scratch).ok();
        return Err("soffice conversion failed".into());
    }
    let stem = path
        .file_stem()
        .ok_or("input has no file stem")?
        .to_string_lossy();
    let txt = scratch.join(format!("{stem}.txt"));
    let text = fs::read_to_string(&txt)?;
    fs::remove_dir_all(&scratch).ok();
    Ok(text.trim_start_matches('\u{feff}').to_owned())
}

fn unique_temp_dir() -> Result<PathBuf, Box<dyn Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!("opendoc-fidelity-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Collapses whitespace, strips generated list markers, and drops empty lines so
/// only source text content is compared. LibreOffice renders numbering markers
/// (`• `, `1. `) that are generated from the numbering definition, not literal
/// source text, so they are removed for a content-fidelity comparison.
fn normalize(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .map(|line| strip_list_marker(&line).to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

fn strip_list_marker(line: &str) -> &str {
    // Bullet markers: "• ", "- ", "* ", "◦ ".
    for bullet in ["\u{2022} ", "\u{25e6} ", "- ", "* "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return rest;
        }
    }
    // Numeric/alpha markers: "<label>. " or "<label>) " where label is a short
    // run of digits or ascii letters (e.g. "1. ", "12) ", "a. ", "iv) ").
    if let Some((label, rest)) = line.split_once(['.', ')']) {
        if !label.is_empty()
            && label.len() <= 4
            && label.chars().all(|c| c.is_ascii_alphanumeric())
            && rest.starts_with(' ')
        {
            return rest.trim_start();
        }
    }
    line
}

/// Flattens normalized lines into their whitespace-separated words, stripping
/// footnote/endnote reference markers (auto-generated digits LibreOffice glues to
/// a word, e.g. "paragraph.1"). Digits are removed from the edges of a *mixed*
/// word only, so real numbers ("2024") and standalone tokens are preserved.
fn words(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .flat_map(|line| line.split_whitespace())
        .map(|word| {
            let trimmed = word.trim_matches(|c: char| c.is_ascii_digit());
            if trimmed.is_empty() {
                word.to_owned()
            } else {
                trimmed.to_owned()
            }
        })
        .collect()
}

/// Fraction of words that appear (as a multiset) in both texts. Comparing words
/// rather than whole lines makes the metric insensitive to how a producer groups
/// text into lines — LibreOffice joins a table row's cells onto one line while the
/// importer emits one line per cell — while still measuring recovered content, so
/// a genuine gap (text we do not extract, e.g. header/footer parts) still shows.
fn word_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut remaining: Vec<&String> = b.iter().collect();
    let mut hits = 0_usize;
    for word in a {
        if let Some(position) = remaining.iter().position(|other| *other == word) {
            remaining.swap_remove(position);
            hits += 1;
        }
    }
    let total = a.len().max(b.len());
    if total == 0 {
        1.0
    } else {
        hits as f64 / total as f64
    }
}

fn print_diff(ours: &str, libre: &str) {
    let ours = normalize(ours);
    let libre = normalize(libre);
    for (index, line) in ours.iter().enumerate() {
        if libre.get(index) != Some(line) {
            println!("  ours[{index}]:  {line:?}");
        }
    }
    for (index, line) in libre.iter().enumerate() {
        if ours.get(index) != Some(line) {
            println!("  libre[{index}]: {line:?}");
        }
    }
}
