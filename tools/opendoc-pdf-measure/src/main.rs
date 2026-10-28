//! P0 measurement harness for PDF import (`docs/182` §10).
//!
//! ```sh
//! # regenerate the committed rights-safe corpus under fixtures/pdf/
//! cargo run --release -p opendoc-pdf-measure -- generate fixtures/pdf
//! # measure it and rewrite the committed artifact
//! cargo run --release -p opendoc-pdf-measure -- measure fixtures/pdf --out fixtures/pdf/measurement.json
//! # measure a private folder: aggregate numbers only, no file names
//! cargo run --release -p opendoc-pdf-measure -- measure ~/Downloads --aggregate-only
//! ```
//!
//! The `deterministic` half of every record is re-derived by this crate's test
//! against the committed artifact; the `timing` half is machine-dependent and
//! is quoted in docs only as an order of magnitude.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::{Path, PathBuf};

use peak_alloc::PeakAlloc;
use serde_json::{Value, json};

mod measure;
mod synth;

/// Counts heap bytes so a page's peak can be read (`measure.rs`).
#[global_allocator]
pub(crate) static PEAK: PeakAlloc = PeakAlloc;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Fixture DOCX files rendered by this repository's own PDF writer.
fn writer_corpus() -> Vec<(String, Vec<u8>)> {
    let dir = repo_root().join("fixtures/corpus");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixtures/corpus")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "docx"))
        .collect();
    names.sort();
    let registry = casual_doc_io::builtin_registry();
    let pdf = casual_doc_io::FormatId::new(casual_doc_io::formats::PDF).expect("pdf id");
    let mut out = Vec::new();
    for path in names {
        let bytes = std::fs::read(&path).expect("read fixture");
        let Ok(imported) = registry.import(
            casual_doc_io::DetectionRequest {
                bytes: &bytes,
                selection: casual_doc_io::FormatSelection::Auto,
                file_name_hint: None,
                mime_hint: None,
            },
            false,
        ) else {
            continue;
        };
        let Ok(exported) = registry.export(
            &pdf,
            casual_doc_io::ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: None,
                source_unchanged: false,
                mode: casual_doc_io::ExportMode::Semantic,
            },
        ) else {
            continue;
        };
        let stem = path.file_stem().expect("stem").to_string_lossy();
        out.push((format!("writer-{stem}.pdf"), exported.bytes));
    }
    out
}

fn generate(dir: &Path) {
    std::fs::create_dir_all(dir.join("hostile")).expect("create corpus dir");
    for (name, bytes) in synth::construct_corpus() {
        std::fs::write(dir.join(name), bytes).expect("write");
    }
    for (name, bytes) in synth::hostile_corpus() {
        std::fs::write(dir.join("hostile").join(name), bytes).expect("write");
    }
    for (name, bytes) in writer_corpus() {
        std::fs::write(dir.join(name), bytes).expect("write");
    }
}

/// Every `*.pdf` directly in `dir` and in `dir/hostile`, sorted by path.
pub fn corpus_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for sub in [dir.to_path_buf(), dir.join("hostile")] {
        let Ok(entries) = std::fs::read_dir(&sub) else {
            continue;
        };
        files.extend(
            entries
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| {
                    path.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
                }),
        );
    }
    files.sort();
    files
}

fn sum(records: &[Value], key: &str) -> u64 {
    records
        .iter()
        .filter_map(|r| r["deterministic"][key].as_u64())
        .sum()
}

fn max(records: &[Value], half: &str, key: &str) -> u64 {
    records
        .iter()
        .filter_map(|r| r[half][key].as_u64())
        .max()
        .unwrap_or(0)
}

/// Aggregates per-file records into the numbers docs/182 §16 quotes.
pub fn aggregate(records: &[Value]) -> Value {
    let parsed = records
        .iter()
        .filter(|r| r["deterministic"]["outcome"] == "parsed")
        .count();
    let glyphs = sum(records, "glyphs");
    let unicode = sum(records, "unicode_glyphs");
    let mut codecs: Vec<String> = records
        .iter()
        .filter_map(|r| r["deterministic"]["codecs"].as_array())
        .flatten()
        .filter_map(|c| c.as_str().map(str::to_owned))
        .collect();
    codecs.sort();
    codecs.dedup();
    json!({
        "files": records.len(),
        "parsed": parsed,
        "refused": records.len() - parsed,
        "pages": sum(records, "pages"),
        "glyphs": glyphs,
        "unicode_glyphs": unicode,
        "unicode_share_per_mille": (unicode * 1000).checked_div(glyphs).unwrap_or(0),
        "fonts": sum(records, "fonts"),
        "fonts_registrable_for_editing": sum(records, "fonts_registrable_for_editing"),
        "max_page_operators": max(records, "deterministic", "max_page_operators"),
        "max_image_pixels": max(records, "deterministic", "max_image_pixels"),
        "codecs": codecs,
        "max_page_us": max(records, "timing", "max_page_us"),
        "max_page_peak_heap_bytes": max(records, "timing", "max_page_peak_heap_bytes"),
    })
}

/// Measures a directory's corpus.
pub fn measure_dir(dir: &Path) -> Vec<Value> {
    corpus_files(dir)
        .iter()
        .map(|path| {
            let bytes = std::fs::read(path).expect("read pdf");
            let name = path
                .strip_prefix(dir)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            measure::measure(&name, &bytes)
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("generate") => generate(Path::new(args.get(1).expect("output dir"))),
        Some("measure") => {
            let dir = Path::new(args.get(1).expect("corpus dir"));
            let records = measure_dir(dir);
            let aggregate_only = args.iter().any(|a| a == "--aggregate-only");
            let report = if aggregate_only {
                json!({ "aggregate": aggregate(&records) })
            } else {
                json!({
                    "harness": "opendoc-pdf-measure",
                    "provider": "hayro-syntax 0.8.0 + hayro-interpret 0.8.0",
                    "aggregate": aggregate(&records),
                    "files": records,
                })
            };
            let text = serde_json::to_string_pretty(&report).expect("json") + "\n";
            match args.iter().position(|a| a == "--out") {
                Some(at) => std::fs::write(args.get(at + 1).expect("--out path"), text)
                    .expect("write report"),
                None => print!("{text}"),
            }
        }
        _ => eprintln!(
            "usage: opendoc-pdf-measure generate <dir> | measure <dir> [--out f] [--aggregate-only]"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed artifact's deterministic numbers are what a fresh run over
    /// the committed corpus produces, so a docs number quoted from it cannot go
    /// stale silently (SKILL §9.1).
    #[test]
    fn the_committed_measurement_matches_a_fresh_run() {
        let dir = repo_root().join("fixtures/pdf");
        let committed: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("measurement.json")).expect("artifact"),
        )
        .expect("json");
        let fresh = measure_dir(&dir);
        let committed_files = committed["files"].as_array().expect("files");
        assert_eq!(committed_files.len(), fresh.len(), "corpus file count");
        for (old, new) in committed_files.iter().zip(&fresh) {
            assert_eq!(old["name"], new["name"]);
            assert_eq!(
                old["deterministic"], new["deterministic"],
                "{} drifted: re-run the harness",
                new["name"]
            );
        }
    }

    /// The synthetic corpus regenerates byte for byte, so the committed files
    /// are exactly what this code writes.
    #[test]
    fn the_synthetic_corpus_is_deterministic() {
        let dir = repo_root().join("fixtures/pdf");
        for (name, bytes) in synth::construct_corpus() {
            assert_eq!(std::fs::read(dir.join(name)).expect(name), bytes, "{name}");
        }
        for (name, bytes) in synth::hostile_corpus() {
            assert_eq!(
                std::fs::read(dir.join("hostile").join(name)).expect(name),
                bytes,
                "{name}"
            );
        }
    }
}
