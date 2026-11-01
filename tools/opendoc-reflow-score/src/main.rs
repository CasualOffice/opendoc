// SPDX-License-Identifier: Apache-2.0
//! PDF reflow, measured (`docs/182` §18.10).
//!
//! ```text
//! opendoc-reflow-score loop <source.docx> <out-dir> [label=file.pdf ...] [--soffice]
//! opendoc-reflow-score corpus <pdf-dir> <results.json>
//! opendoc-reflow-score dump <file.pdf>
//! opendoc-reflow-score docx <file.pdf> <out.docx>
//! ```
//!
//! **loop**: the ground truth. The DOCX is imported (its own model is the
//! answer), printed to PDF by our writer (`ours.pdf`) and, with `--soffice`,
//! by LibreOffice (`libreoffice.pdf`), and every PDF (plus any given) is opened
//! the way every host opens one, as a flowing document; each is scored against
//! the source per family (`score.rs`) into `<out-dir>/scores.json`, and a table
//! is printed.
//!
//! **corpus**: every PDF under a directory is opened the same way and
//! recorded: opened or the error, milliseconds, the findings, the structure
//! counts, and the sanity checks a flowing document must pass (no text boxes,
//! no U+FFFD, tables where there are ruled grids, headings).
//!
//! Prints to stdout: it is a measuring tool.

#![allow(clippy::print_stdout, clippy::print_stderr)]

mod model;
mod score;

use std::path::{Path, PathBuf};
use std::time::Instant;

use casual_doc_io::{DetectionRequest, ExportMode, ExportRequest, FormatSelection, ImportArtifact};
use serde_json::{Value, json};

fn import(bytes: &[u8], name: &str) -> Result<ImportArtifact, String> {
    let registry = casual_doc_io::builtin_registry_with_pdf_import();
    registry
        .import(
            DetectionRequest {
                bytes,
                selection: FormatSelection::Auto,
                file_name_hint: Some(name),
                mime_hint: None,
            },
            false,
        )
        .map_err(|e| e.to_string())
}

fn print_pdf(source: &ImportArtifact) -> Result<Vec<u8>, String> {
    let registry = casual_doc_io::builtin_registry();
    let pdf =
        casual_doc_io::FormatId::new(casual_doc_io::formats::PDF).map_err(|e| e.to_string())?;
    registry
        .export(
            &pdf,
            ExportRequest {
                document: &source.document,
                resources: &source.resources,
                source: None,
                source_unchanged: false,
                mode: ExportMode::Semantic,
            },
        )
        .map(|a| a.bytes)
        .map_err(|e| e.to_string())
}

const FAMILIES: [&str; 15] = [
    "text",
    "paragraphs",
    "headings",
    "lists",
    "tables",
    "grid_tables",
    "pictures",
    "header_footer",
    "notes",
    "columns",
    "page",
    "formatting",
    "special",
    "frames",
    "layout",
];

fn run_loop(args: &[String]) -> Result<(), String> {
    let docx = PathBuf::from(args.first().ok_or("loop <source.docx> <out-dir> ...")?);
    let out = PathBuf::from(args.get(1).ok_or("loop <source.docx> <out-dir> ...")?);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&docx).map_err(|e| e.to_string())?;
    let source = import(&bytes, &docx.to_string_lossy())?;
    let truth = model::summarize(&source.document);
    let mut pdfs: Vec<(String, PathBuf)> = Vec::new();
    let ours = out.join("ours.pdf");
    std::fs::write(&ours, print_pdf(&source)?).map_err(|e| e.to_string())?;
    pdfs.push(("ours".to_owned(), ours));
    for arg in &args[2..] {
        if arg == "--soffice" {
            let status = std::process::Command::new("soffice")
                .args(["--headless", "--convert-to", "pdf", "--outdir"])
                .arg(&out)
                .arg(&docx)
                .output()
                .map_err(|e| format!("soffice: {e}"))?;
            if !status.status.success() {
                return Err(format!(
                    "soffice failed: {}",
                    String::from_utf8_lossy(&status.stderr)
                ));
            }
            let stem = docx
                .file_stem()
                .ok_or("no stem")?
                .to_string_lossy()
                .into_owned();
            let target = out.join("libreoffice.pdf");
            std::fs::rename(out.join(format!("{stem}.pdf")), &target).map_err(|e| e.to_string())?;
            pdfs.push(("libreoffice".to_owned(), target));
        } else if let Some((label, path)) = arg.split_once('=') {
            pdfs.push((label.to_owned(), PathBuf::from(path)));
        }
    }
    let mut results = serde_json::Map::new();
    for (label, path) in &pdfs {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let started = Instant::now();
        let imported = import(&bytes, &path.to_string_lossy())?;
        let ms = started.elapsed().as_millis();
        let summary = model::summarize(&imported.document);
        let mut scored = score::score(&truth, &summary);
        scored["ms"] = json!(ms);
        scored["findings"] = json!(
            imported
                .report
                .entries
                .iter()
                .map(|e| format!("{} x{}", e.feature, e.occurrences))
                .collect::<Vec<_>>()
        );
        scored["output_paragraphs"] = json!(
            summary
                .paras
                .iter()
                .map(|p| format!(
                    "{}{}{} {}",
                    p.heading.map_or(String::new(), |h| format!("[H{h}]")),
                    p.list.map_or(String::new(), |l| format!("[L{l}]")),
                    if p.bold { "[b]" } else { "" },
                    p.text.chars().take(80).collect::<String>()
                ))
                .collect::<Vec<_>>()
        );
        results.insert(label.clone(), scored);
    }
    results.insert(
        "source_paragraphs".to_owned(),
        json!(
            truth
                .paras
                .iter()
                .map(|p| format!(
                    "{}{}{} {}",
                    p.heading.map_or(String::new(), |h| format!("[H{h}]")),
                    p.list.map_or(String::new(), |l| format!("[L{l}]")),
                    if p.bold { "[b]" } else { "" },
                    p.text.chars().take(80).collect::<String>()
                ))
                .collect::<Vec<_>>()
        ),
    );
    let results = Value::Object(results);
    std::fs::write(
        out.join("scores.json"),
        serde_json::to_string_pretty(&results).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    print!("{:<14}", "family");
    for (label, _) in &pdfs {
        print!("{label:>13}");
    }
    println!();
    for family in FAMILIES {
        print!("{family:<14}");
        for (label, _) in &pdfs {
            print!(
                "{:>13.3}",
                results[label][family].as_f64().unwrap_or(f64::NAN)
            );
        }
        println!();
    }
    Ok(())
}

fn pdfs_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            pdfs_under(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            out.push(path);
        }
    }
}

fn run_corpus(args: &[String]) -> Result<(), String> {
    let dir = PathBuf::from(args.first().ok_or("corpus <pdf-dir> <results.json>")?);
    let target = PathBuf::from(args.get(1).ok_or("corpus <pdf-dir> <results.json>")?);
    let mut files = Vec::new();
    pdfs_under(&dir, &mut files);
    files.sort();
    let mut rows = Vec::new();
    for path in files {
        let name = path
            .strip_prefix(&dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let started = Instant::now();
        let outcome = std::panic::catch_unwind(|| import(&bytes, &name));
        let ms = started.elapsed().as_millis();
        let row = match outcome {
            Err(_) => json!({ "file": name, "ok": false, "error": "panic", "ms": ms }),
            Ok(Err(error)) => json!({ "file": name, "ok": false, "error": error, "ms": ms }),
            Ok(Ok(imported)) => {
                let s = model::summarize(&imported.document);
                let features: Vec<String> = imported
                    .report
                    .entries
                    .iter()
                    .map(|e| format!("{} x{}", e.feature, e.occurrences))
                    .collect();
                let has = |f: &str| imported.report.entries.iter().any(|e| e.feature == f);
                // Why the flowing reconstruction failed, when it did.
                let reflow_error = has("pdf.import.flowing_unavailable").then(|| {
                    casual_doc_pdf_import::import_pdf(
                        bytes.clone(),
                        casual_doc_pdf_import::PdfReadLimits::default(),
                    )
                    .map_err(|e| e.to_string())
                    .and_then(|i| i.flowing().map(|_| ()).map_err(|e| e.to_string()))
                    .err()
                    .unwrap_or_default()
                });
                json!({
                    "file": name,
                    "ok": true,
                    "ms": ms,
                    "bytes": bytes.len(),
                    "paragraphs": s.paras.len(),
                    "headings": s.paras.iter().filter(|p| p.heading.is_some()).count(),
                    "lists": s.paras.iter().filter(|p| p.list.is_some()).count(),
                    "tables": s.tables.len(),
                    "pictures": s.pictures,
                    "text_boxes": s.text_boxes,
                    "groups": s.groups,
                    "notes": s.notes,
                    "sections": s.sections,
                    "columns": s.columns,
                    "header": !s.header.is_empty(),
                    "footer": !s.footer.is_empty(),
                    "page_fields": s.page_fields,
                    "fffd": s.bad_chars.0,
                    "pua": s.bad_chars.1,
                    "ligatures": s.bad_chars.2,
                    "flowing": has("pdf.import.layout_inferred"),
                    "short_paragraphs": s.paras.iter().filter(|p| p.text.split_whitespace().count() <= 3).count(),
                    "mean_words": if s.paras.is_empty() { 0.0 } else {
                        s.paras.iter().map(|p| p.text.split_whitespace().count()).sum::<usize>() as f64 / s.paras.len() as f64
                    },
                    "findings": features,
                    "reflow_error": reflow_error,
                })
            }
        };
        println!(
            "{:<60} {}",
            name.chars().take(60).collect::<String>(),
            if row["ok"] == true {
                format!(
                    "ok {:>6}ms paras {:>4} H {:>3} L {:>3} T {:>2} pic {:>3} tb {:>2} fffd {} pua {}",
                    row["ms"],
                    row["paragraphs"],
                    row["headings"],
                    row["lists"],
                    row["tables"],
                    row["pictures"],
                    row["text_boxes"],
                    row["fffd"],
                    row["pua"]
                )
            } else {
                format!("FAILED {}", row["error"])
            }
        );
        rows.push(row);
    }
    std::fs::write(
        &target,
        serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// `dump <file.pdf>`: the flowing document's paragraphs, one per line, with
/// their heading and list levels, and its tables' shapes, for reading a
/// corpus failure.
fn run_dump(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("dump <file.pdf>")?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let imported = import(&bytes, path)?;
    let s = model::summarize(&imported.document);
    for p in &s.paras {
        println!(
            "{}{}{} {}",
            p.heading.map_or(String::new(), |h| format!("[H{h}]")),
            p.list.map_or(String::new(), |l| format!("[L{l}]")),
            if p.bold { "[b]" } else { "" },
            p.text.chars().take(140).collect::<String>()
        );
    }
    for t in &s.tables {
        println!(
            "TABLE {}x{} first {:?}",
            t.cells.len(),
            t.cells.iter().map(Vec::len).max().unwrap_or(0),
            t.cells.first()
        );
    }
    println!(
        "columns {} sections {} header {:?} footer {:?}",
        s.columns, s.sections, s.header, s.footer
    );
    Ok(())
}

/// `docx <file.pdf> <out.docx>`: the flowing document saved as DOCX, so a
/// page renderer (`opendoc-render`) can draw what a person sees next to the
/// original page.
fn run_docx(args: &[String]) -> Result<(), String> {
    let [path, target, ..] = args else {
        return Err("docx <file.pdf> <out.docx>".to_owned());
    };
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let imported = import(&bytes, path)?;
    let docx =
        casual_doc_io::FormatId::new(casual_doc_io::formats::DOCX).map_err(|e| e.to_string())?;
    let out = casual_doc_io::builtin_registry()
        .export(
            &docx,
            ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: None,
                source_unchanged: false,
                mode: ExportMode::Semantic,
            },
        )
        .map_err(|e| e.to_string())?;
    std::fs::write(target, out.bytes).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("loop") => run_loop(&args[1..]),
        Some("dump") => run_dump(&args[1..]),
        Some("docx") => run_docx(&args[1..]),
        Some("corpus") => run_corpus(&args[1..]),
        _ => Err("usage: opendoc-reflow-score loop <source.docx> <out-dir> [label=file.pdf ...] [--soffice] | corpus <pdf-dir> <results.json>".to_owned()),
    };
    if let Err(error) = result {
        eprintln!("opendoc-reflow-score: {error}");
        std::process::exit(1);
    }
}
