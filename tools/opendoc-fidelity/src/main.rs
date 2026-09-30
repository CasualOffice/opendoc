// SPDX-License-Identifier: Apache-2.0

//! Fidelity harnesses comparing OpenDoc against an independent reference
//! renderer.
//!
//! Two modes, both shelling out to LibreOffice, both evaluation tools rather
//! than CI unit tests:
//!
//! - `compare <file.docx>` — the **geometry** comparison (`docs/94` H2, backlog
//!   rows FID-P-01/FID-L-21). Renders the document through our pipeline and
//!   through LibreOffice and reports the differences as *measurements*: page
//!   count, per-page text extents, per-line bottoms and right edges, line counts,
//!   words per line, and the faces each side resolved. This exists because every
//!   fidelity judgement in this project used to be made by rendering two PNGs and
//!   looking at them, which cannot say by how much or where.
//! - `text <file.docx> [more.docx ...]` — the original **content** differential:
//!   extracts the text through the importer and through `soffice --convert-to
//!   txt` and reports whether they agree as a word multiset. It measures whether
//!   import recovers the document's textual content; it says nothing about where
//!   anything is on the page.
//!
//! With no subcommand the arguments are treated as `text` targets, which is how
//! this tool was invoked before `compare` existed.
//!
//! Usage:
//! ```text
//! cargo run -p opendoc-fidelity -- compare file.docx [--tolerance TWIPS] [--page N] [--keep DIR]
//! cargo run -p opendoc-fidelity -- text file.docx [more.docx ...]
//! ```

// A CLI reporting tool legitimately writes to stdout/stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod compare;
mod oracle;

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use casual_doc_import::{ImportConfig, import_package};
use casual_doc_model::v1::{BlockNode, GroupChild, InlineNode, WordprocessingGroup};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

/// The comparison band, in twips (2pt).
///
/// Sized to the *known* residual rather than to clear a case: PDF text origins
/// round to 0.1pt (~2 twips), and `parley` splits a face's line gap around the
/// text box where LibreOffice puts it all below the descent, which is `lineGap/2`
/// — ≈4 twips at 12pt, 8 at 24pt. See `crates/casual-doc-render/tests/
/// oracle_geometry.rs` for the full derivation; this harness deliberately uses
/// the same number as the gate so a finding here means the same thing there.
const DEFAULT_TOLERANCE_TWIPS: i32 = 40;

const USAGE: &str = "\
usage:
  opendoc-fidelity compare <file.docx> [--tolerance TWIPS] [--page N] [--keep DIR]
  opendoc-fidelity text <file.docx> [more.docx ...]

  compare  geometry against LibreOffice: page count, text extents, line
           positions, words per line, resolved fonts
  text     content against LibreOffice: word-multiset agreement";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None | Some("-h" | "--help") => {
            eprintln!("{USAGE}");
            2
        }
        Some("compare") => run_compare(&args[1..]),
        Some("text") => run_text(&args[1..]),
        // Back-compatible: bare paths mean the original text differential.
        Some(_) => run_text(&args),
    };
    std::process::exit(code);
}

/// The geometry comparison. Returns the process exit code: 0 when the two
/// renderings agree within tolerance, 1 when they do not, 2 on a usage or tool
/// error. A finding is a *measurement*, so a non-zero code here means "they
/// differ", not "this is a bug".
fn run_compare(args: &[String]) -> i32 {
    let mut input: Option<PathBuf> = None;
    let mut tolerance = DEFAULT_TOLERANCE_TWIPS;
    let mut keep: Option<PathBuf> = None;
    let mut detail: Option<usize> = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--tolerance" => match rest.next().and_then(|v| v.parse::<i32>().ok()) {
                Some(value) => tolerance = value,
                None => {
                    eprintln!("--tolerance needs a whole number of twips");
                    return 2;
                }
            },
            "--page" => match rest.next().and_then(|v| v.parse::<usize>().ok()) {
                Some(value) if value >= 1 => detail = Some(value),
                _ => {
                    eprintln!("--page needs a 1-based page number");
                    return 2;
                }
            },
            "--keep" => match rest.next() {
                Some(dir) => keep = Some(PathBuf::from(dir)),
                None => {
                    eprintln!("--keep needs a directory");
                    return 2;
                }
            },
            other if input.is_none() => input = Some(PathBuf::from(other)),
            other => {
                eprintln!("unexpected argument: {other}");
                return 2;
            }
        }
    }
    let Some(input) = input else {
        eprintln!("{USAGE}");
        return 2;
    };

    let keeping = keep.is_some();
    let workdir = keep.unwrap_or_else(|| {
        std::env::temp_dir().join(format!(
            "opendoc-fidelity-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ))
    });

    let outcome = compare_document(&input, &workdir, tolerance, detail);
    if !keeping {
        // A PDF per invocation adds up, and this harness is run in a loop while
        // chasing a divergence. `--keep` is how you ask for the intermediates.
        let _ = std::fs::remove_dir_all(&workdir);
    }
    match outcome {
        Ok(findings) => findings,
        Err(error) => {
            eprintln!("ERROR {}: {error}", input.display());
            2
        }
    }
}

/// Runs both renderers over `input`, prints the report, and answers with the
/// process exit code.
fn compare_document(
    input: &Path,
    workdir: &Path,
    tolerance: i32,
    detail: Option<usize>,
) -> Result<i32, Box<dyn Error>> {
    let bytes = fs::read(input)?;
    let ours = compare::our_document(&bytes)?;
    let reference = oracle::render(input, workdir)?;
    let findings = compare::compare(&ours, &reference, tolerance);
    print!(
        "{}",
        compare::report(input, &ours, &reference, &findings, tolerance)
    );
    if let Some(page) = detail {
        match (ours.pages.get(page - 1), reference.pages.get(page - 1)) {
            (Some(ours), Some(theirs)) => {
                println!();
                println!("page {page} lines:");
                print!("{}", compare::page_detail(ours, theirs));
            }
            _ => eprintln!("no page {page} on both sides"),
        }
    }
    Ok(i32::from(!findings.is_empty()))
}

/// The original text differential, over one or more documents.
fn run_text(paths: &[String]) -> i32 {
    if paths.is_empty() {
        eprintln!("{USAGE}");
        return 2;
    }
    let mut failures = 0_usize;
    for path in paths {
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
    i32::from(failures > 0)
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
fn push_group_text(group: &WordprocessingGroup, out: &mut String) {
    for child in &group.children {
        match child {
            GroupChild::TextBox(text_box) => push_blocks_text(&text_box.blocks, out),
            GroupChild::Group(nested) => push_group_text(nested, out),
            GroupChild::Picture(_) | GroupChild::Shape(_) => {}
        }
    }
}

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
            BlockNode::Sdt(sdt) => push_blocks_text(&sdt.blocks, out),
            // An alt chunk's aggregated content lives in an external part that is
            // not parsed here, so it contributes no extractable in-flow text.
            BlockNode::AltChunk(_) => {}
        }
    }
}

fn push_inline_text(inline: &InlineNode, out: &mut String) {
    match inline {
        InlineNode::Run(run) => out.push_str(&run.text),
        InlineNode::Tab(_) => out.push('\t'),
        InlineNode::Break(_) => out.push('\n'),
        InlineNode::Drawing(_) => {}
        InlineNode::AnchoredDrawing(_) => {}
        InlineNode::EmbeddedObject(_) => {}
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
        // A group's text boxes carry real text; recurse through the children.
        InlineNode::Group(group) => push_group_text(group, out),
        // A tracked-change range's content is real text (an insertion reads as
        // present text; a deletion's `w:delText` is retained), so recurse into it.
        InlineNode::Revision(revision) => {
            for child in &revision.inlines {
                push_inline_text(child, out);
            }
        }
        // A note reference renders as a mark/number, not source text; the note
        // body text is appended separately from the definitions.
        InlineNode::NoteReference(_) => {}
        // A note's own auto-number mark renders as that note's number, not source
        // text; it contributes no extractable in-flow text.
        InlineNode::NoteNumberMark(_) => {}
        // A comment reference is an anchor with no in-flow text; the comment
        // body lives in the definitions and is not part of the document text.
        InlineNode::CommentReference(_) => {}
        // A comment range marker is a zero-width span anchor with no in-flow text;
        // the commented text is the runs it brackets.
        InlineNode::CommentRangeStart(_) | InlineNode::CommentRangeEnd(_) => {}
        // A bookmark marker is a zero-width range anchor with no in-flow text.
        InlineNode::BookmarkStart(_) | InlineNode::BookmarkEnd(_) => {}
        // A paragraph-spanning field's markers are zero-width range anchors. The
        // field's cached result is the ordinary content BETWEEN them and is walked
        // in its own right, so `push_inline_text` must add nothing here — counting
        // the result at the marker too would double it.
        InlineNode::FieldRangeStart(_) | InlineNode::FieldRangeEnd(_) => {}
        // A tracked-move range marker is a zero-width anchor with no in-flow text;
        // the moved text lives in the paired `w:moveFrom`/`w:moveTo` run wrapper.
        InlineNode::MoveRangeStart(_) | InlineNode::MoveRangeEnd(_) => {}
        // A content control is a transparent wrapper; its wrapped runs are the
        // visible text, so recurse into them.
        InlineNode::Sdt(sdt) => {
            for child in &sdt.inlines {
                push_inline_text(child, out);
            }
        }
        // A math object's visible text is its plain-text fallback (the OMML
        // subtree itself is opaque markup, not in-flow text).
        InlineNode::Math(math) => out.push_str(&math.text),
        // A symbol is a font-bound glyph (typically a Private Use Area code
        // point); it has no extractable plain-text form, so it contributes none.
        InlineNode::Symbol(_) => {}
        // A horizontal rule is a graphic line with no extractable text.
        InlineNode::HorizontalRule(_) => {}
        // A non-breaking hyphen renders as a visible hyphen; a soft (optional)
        // hyphen is invisible unless the line breaks there, so it contributes no
        // in-flow text; an absolute-position tab renders as a tab.
        InlineNode::NoBreakHyphen(_) => out.push('-'),
        InlineNode::SoftHyphen(_) => {}
        InlineNode::PositionalTab(_) => out.push('\t'),
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
    if let Some((label, rest)) = line.split_once(['.', ')'])
        && !label.is_empty()
        && label.len() <= 4
        && label.chars().all(|c| c.is_ascii_alphanumeric())
        && rest.starts_with(' ')
    {
        return rest.trim_start();
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
