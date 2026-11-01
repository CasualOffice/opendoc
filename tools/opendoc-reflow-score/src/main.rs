// SPDX-License-Identifier: Apache-2.0
//! PDF reflow, measured (`docs/182` §18.10).
//!
//! ```text
//! opendoc-reflow-score loop <source.docx> <out-dir> [label=file.pdf ...] [--soffice]
//! opendoc-reflow-score corpus <pdf-dir> <results.json>
//! opendoc-reflow-score dump <file.pdf>
//! opendoc-reflow-score docx <file.pdf> <out.docx>
//! opendoc-reflow-score synth <out-dir>
//! opendoc-reflow-score runs <file.pdf> [page]
//! ```
//!
//! **runs**: prints a page's glyph runs and pictures as the reader sees them
//! (origin, size, rotation, fill and alpha, text), for reading one failure.
//!
//! **synth**: writes the synthetic ground truths (`synth.rs`: a watermarked
//! multi-page document, aligned numeric tables, mixed spacing and sizes) for
//! `loop` to measure.
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
// The score report is one `json!` literal, deeper than the default limit.
#![recursion_limit = "256"]

mod model;
mod score;
mod synth;

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

const FAMILIES: [&str; 20] = [
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
    "spacing",
    "indents",
    "sizes",
    "table_layout",
    "watermark",
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

/// Non-whitespace glyphs per size (half points, rounded) over every page
/// of `bytes` as the reader sees them, upright and rotated. O(glyphs).
fn pdf_sizes(bytes: &[u8]) -> std::collections::BTreeMap<u32, usize> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(mut source) = casual_doc_pdf_read::PdfSource::open(
        bytes.to_vec(),
        casual_doc_pdf_read::PdfReadLimits::default(),
    ) else {
        return out;
    };
    for index in 0..source.page_count() {
        let Ok(page) = source.page_evidence(index) else {
            continue;
        };
        for item in &page.items {
            if let casual_doc_pdf_read::PaintEvidence::Glyphs(run) = item {
                if run.mode == casual_doc_pdf_read::RenderMode::Invisible {
                    continue;
                }
                let n = run
                    .glyphs
                    .iter()
                    .filter_map(|g| g.text.as_deref())
                    .flat_map(str::chars)
                    .filter(|c| !c.is_whitespace())
                    .count();
                if n > 0 {
                    *out.entry((run.size * 2.0).round() as u32).or_default() += n;
                }
            }
        }
    }
    out
}

/// The share of the larger histogram's mass the two have in common, size
/// by size: 1 when every character kept its exact size. O(sizes).
fn histogram_agreement(
    a: &std::collections::BTreeMap<u32, usize>,
    b: &std::collections::BTreeMap<u32, usize>,
) -> f64 {
    let total = a.values().sum::<usize>().max(b.values().sum::<usize>());
    if total == 0 {
        return 1.0;
    }
    let common: usize = a
        .iter()
        .map(|(k, n)| (*n).min(b.get(k).copied().unwrap_or(0)))
        .sum();
    common as f64 / total as f64
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
                let pdf_hist = pdf_sizes(&bytes);
                let model_hist = model::size_histogram(&imported.document);
                let size_agreement = histogram_agreement(&pdf_hist, &model_hist);
                let (spacing_modal, spacing_distinct, indent_modal, indent_distinct) =
                    model::consistency(&s);
                let table_cells: usize = s
                    .tables
                    .iter()
                    .map(|t| t.aligns.iter().map(Vec::len).sum::<usize>())
                    .sum();
                let aligned_cells: usize = s
                    .tables
                    .iter()
                    .map(|t| {
                        t.aligns
                            .iter()
                            .flatten()
                            .filter(|a| **a == 1 || **a == 2)
                            .count()
                    })
                    .sum();
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
                    "size_agreement": size_agreement,
                    "pdf_sizes": format!("{:?}", pdf_hist),
                    "model_sizes": format!("{:?}", model_hist),
                    "spacing_modal": spacing_modal,
                    "spacing_distinct": spacing_distinct,
                    "indent_modal": indent_modal,
                    "indent_distinct": indent_distinct,
                    "table_cells": table_cells,
                    "aligned_cells": aligned_cells,
                    "watermark": s.watermark.as_ref().map(|w| w.1),
                    "rotated_runs": imported.report.entries.iter().filter(|e| e.feature == "pdf.import.text_rotated").map(|e| e.occurrences).sum::<u32>(),
                })
            }
        };
        println!(
            "{:<60} {}",
            name.chars().take(60).collect::<String>(),
            if row["ok"] == true {
                format!(
                    "ok {:>6}ms paras {:>4} H {:>3} L {:>3} T {:>2} pic {:>3} tb {:>2} fffd {} pua {} size {:.3} sp {:.2}/{} ind {:.2}/{} cells {}/{} wm {} rot {}",
                    row["ms"],
                    row["paragraphs"],
                    row["headings"],
                    row["lists"],
                    row["tables"],
                    row["pictures"],
                    row["text_boxes"],
                    row["fffd"],
                    row["pua"],
                    row["size_agreement"].as_f64().unwrap_or(0.0),
                    row["spacing_modal"].as_f64().unwrap_or(0.0),
                    row["spacing_distinct"],
                    row["indent_modal"].as_f64().unwrap_or(0.0),
                    row["indent_distinct"],
                    row["aligned_cells"],
                    row["table_cells"],
                    row["watermark"],
                    row["rotated_runs"]
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

/// `runs <file.pdf> [page]`: the reader's evidence for one page (or all).
fn run_runs(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("runs <file.pdf> [page]")?;
    let only: Option<usize> = args.get(1).and_then(|p| p.parse().ok());
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut source =
        casual_doc_pdf_read::PdfSource::open(bytes, casual_doc_pdf_read::PdfReadLimits::default())
            .map_err(|e| e.to_string())?;
    for index in 0..source.page_count() {
        if only.is_some_and(|p| p != index) {
            continue;
        }
        let page = source.page_evidence(index).map_err(|e| e.to_string())?;
        println!("page {index} {:.1} x {:.1}", page.width, page.height);
        for item in &page.items {
            match item {
                casual_doc_pdf_read::PaintEvidence::Glyphs(run) => {
                    let text: String = run
                        .glyphs
                        .iter()
                        .filter_map(|g| g.text.as_deref())
                        .collect();
                    let first = run.glyphs.first().map(|g| g.origin).unwrap_or_default();
                    let last = run
                        .glyphs
                        .last()
                        .map(|g| (g.origin.x + g.advance, g.origin.y))
                        .unwrap_or_default();
                    println!(
                        "  T ({:.2},{:.2})-({:.2},{:.2}) size {:.3} rot {:.1} fill {:?} font {} {:?}",
                        first.x,
                        first.y,
                        last.0,
                        last.1,
                        run.size,
                        run.rotation_degrees,
                        run.fill,
                        source
                            .fonts()
                            .fonts
                            .get(run.font)
                            .map_or("", |f| f.name.as_str()),
                        text.chars().take(70).collect::<String>()
                    );
                }
                casual_doc_pdf_read::PaintEvidence::Image(image) => println!(
                    "  I {:.1},{:.1} {:.1}x{:.1} rot {:.1} px {:?} opacity {:.2}",
                    image.bounds.x0,
                    image.bounds.y0,
                    image.size.0,
                    image.size.1,
                    image.rotation_degrees,
                    image.pixels,
                    image.opacity
                ),
                casual_doc_pdf_read::PaintEvidence::Path(path) => println!(
                    "  P {:.1},{:.1}-{:.1},{:.1} fill {:?} stroke {:?}",
                    path.bounds.x0,
                    path.bounds.y0,
                    path.bounds.x1,
                    path.bounds.y1,
                    path.fill,
                    path.stroke.as_ref().map(|s| (s.width, s.color))
                ),
            }
        }
    }
    Ok(())
}

/// `synth <out-dir>`: the synthetic ground truths, as DOCX files.
fn run_synth(args: &[String]) -> Result<(), String> {
    let out = PathBuf::from(args.first().ok_or("synth <out-dir>")?);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for (name, bytes) in synth::all()? {
        std::fs::write(out.join(name), bytes).map_err(|e| e.to_string())?;
        println!("{}", out.join(name).display());
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("loop") => run_loop(&args[1..]),
        Some("dump") => run_dump(&args[1..]),
        Some("docx") => run_docx(&args[1..]),
        Some("corpus") => run_corpus(&args[1..]),
        Some("synth") => run_synth(&args[1..]),
        Some("runs") => run_runs(&args[1..]),
        _ => Err("usage: opendoc-reflow-score loop <source.docx> <out-dir> [label=file.pdf ...] [--soffice] | corpus <pdf-dir> <results.json>".to_owned()),
    };
    if let Err(error) = result {
        eprintln!("opendoc-reflow-score: {error}");
        std::process::exit(1);
    }
}
