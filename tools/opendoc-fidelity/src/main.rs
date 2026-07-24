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
                    "{status} {path}  (ours={} chars, libre={} chars, line-match={:.0}%)",
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
    let ours_norm = normalize(&ours);
    let libre_norm = normalize(&libre);
    let matches = ours_norm == libre_norm;
    let similarity = line_similarity(&ours_norm, &libre_norm);
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
    for block in import.document.body() {
        let BlockNode::Paragraph(paragraph) = block;
        for inline in &paragraph.inlines {
            push_inline_text(inline, &mut out);
        }
        out.push('\n');
    }
    Ok(out)
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

/// Fraction of normalized lines that appear (as a multiset) in both texts.
fn line_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut remaining: Vec<&String> = b.iter().collect();
    let mut hits = 0_usize;
    for line in a {
        if let Some(position) = remaining.iter().position(|other| *other == line) {
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
