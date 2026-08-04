//! Batch corpus renderer — the committed generalization of the
//! `casual-doc-render` `render_docx_page` example, for ongoing visual-regression
//! checks over a folder of real `.docx` files.
//!
//! For each file it runs the full native pipeline — import ->
//! [`paginate_document`] -> [`compose_page`] -> raster — and writes one PNG per
//! page to `<outdir>/<name>_p<page>.png`. Fonts are served through a
//! [`RegistryFontSource`] taken from the shaper's dynamic registry *after*
//! pagination, so glyphs the shaper resolved to an OS fallback face (CJK,
//! complex scripts, symbols — the native system-font tier is on by default)
//! rasterize from that face instead of `.notdef` tofu.
//!
//! Each file is wrapped in [`std::panic::catch_unwind`] so one malformed
//! document cannot abort the whole batch; the run ends with an
//! `N ok, M error, K panic` tally and a non-zero exit on any failure.
//!
//! This is an evaluation tool, not a CI unit test. It renders user-supplied
//! files at runtime; no `.docx` or `.png` is committed.
//!
//! ```text
//! opendoc-render <outdir> <file.docx> [more.docx ...] [--dpi <f32>] [--max-pages <n>]
//! ```

// A CLI reporting/eval tool legitimately writes to stdout/stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::any::Any;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use casual_doc_import::{ImportConfig, ImportMode, ModelOutcome, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};

/// Default raster resolution (device pixels per inch). Overridable with `--dpi`.
const DEFAULT_DPI: f32 = 110.0;

/// The most feature names to list in a per-file disposition summary before it is
/// truncated with an ellipsis, so the line stays compact.
const MAX_LISTED_FEATURES: usize = 10;

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            print!("{USAGE}");
            return;
        }
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    if let Err(error) = fs::create_dir_all(&options.outdir) {
        eprintln!(
            "error: cannot create output directory {}: {error}",
            options.outdir.display()
        );
        std::process::exit(2);
    }

    // Suppress the default panic hook's backtrace so a per-file panic reads as a
    // single tidy `PANIC <name>: ...` line; the payload is recovered below.
    std::panic::set_hook(Box::new(|_| {}));

    let (mut ok, mut errors, mut panics) = (0_usize, 0_usize, 0_usize);
    for file in &options.files {
        let name = file_name(file);
        let outcome = std::panic::catch_unwind(|| {
            render_file(file, &options.outdir, options.dpi, options.max_pages)
        });
        match outcome {
            Ok(Ok(summary)) => {
                ok += 1;
                println!("OK {name}: {}p{}", summary.pages, summary.disposition);
            }
            Ok(Err(error)) => {
                errors += 1;
                println!("ERROR {name}: {error}");
            }
            Err(payload) => {
                panics += 1;
                println!("PANIC {name}: {}", panic_message(&payload));
            }
        }
    }

    println!("{ok} ok, {errors} error, {panics} panic");
    if errors > 0 || panics > 0 {
        std::process::exit(1);
    }
}

const USAGE: &str = "opendoc-render — batch corpus renderer for visual-regression checks\n\
\n\
USAGE:\n\
\x20   opendoc-render <outdir> <file.docx> [more.docx ...] [OPTIONS]\n\
\n\
Renders every page of each .docx to <outdir>/<name>_p<page>.png through the full\n\
import -> paginate -> compose -> raster pipeline, serving OS fallback fonts (CJK,\n\
complex scripts) via the system-fonts tier (on by default). One bad file cannot\n\
abort the batch; the run ends with an `N ok, M error, K panic` tally.\n\
\n\
OPTIONS:\n\
\x20   --dpi <f32>        raster resolution (default 110)\n\
\x20   --max-pages <n>    render at most n pages per file (default: all)\n\
\x20   -h, --help         show this help\n\
\n\
No .docx or .png is committed; user files are rendered at runtime.\n";

/// The parsed command line.
struct Options {
    outdir: PathBuf,
    files: Vec<PathBuf>,
    dpi: f32,
    max_pages: Option<usize>,
}

impl Options {
    /// Parses arguments, returning `Ok(None)` when help was requested.
    fn parse(args: impl Iterator<Item = String>) -> Result<Option<Self>, String> {
        let mut positionals: Vec<PathBuf> = Vec::new();
        let mut dpi = DEFAULT_DPI;
        let mut max_pages = None;
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "--dpi" => {
                    let value = args.next().ok_or("--dpi needs a value")?;
                    dpi = value
                        .parse::<f32>()
                        .map_err(|_| format!("invalid --dpi value: {value}"))?;
                    if !(dpi.is_finite() && dpi > 0.0) {
                        return Err(format!("--dpi must be a positive number, got {value}"));
                    }
                }
                "--max-pages" => {
                    let value = args.next().ok_or("--max-pages needs a value")?;
                    max_pages = Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| format!("invalid --max-pages value: {value}"))?,
                    );
                }
                other if other.starts_with("--") => {
                    return Err(format!("unknown option: {other}"));
                }
                _ => positionals.push(PathBuf::from(arg)),
            }
        }
        let mut positionals = positionals.into_iter();
        let outdir = positionals.next().ok_or("missing <outdir>")?;
        let files: Vec<PathBuf> = positionals.collect();
        if files.is_empty() {
            return Err("no input .docx files given".to_owned());
        }
        Ok(Some(Options {
            outdir,
            files,
            dpi,
            max_pages,
        }))
    }
}

/// The per-file result reported on an `OK` line.
struct FileSummary {
    pages: usize,
    /// A compact ` [omitted N: …; degraded M: …]` suffix, or empty when the
    /// document mapped cleanly.
    disposition: String,
}

/// Runs the full pipeline for one file and writes its page PNGs.
fn render_file(
    path: &Path,
    outdir: &Path,
    dpi: f32,
    max_pages: Option<usize>,
) -> Result<FileSummary, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let mut package = DocxPackage::open(&bytes, PackageLimits::default())?;
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )?;
    let document = imported.document;

    // Serve the package's embedded pictures (`word/media/*`) to the renderer,
    // keyed by the part name the display list references.
    let mut media = MapMediaSource::new();
    for (_id, reference) in document.definitions().media.iter() {
        if let Ok(part) = package.read_part(&reference.part_name) {
            media.insert(reference.part_name.clone(), part);
        }
    }

    let shaper = ParleyShaper::new();
    let pages = paginate_document(&document, &shaper);
    let page_count = pages.page_count();

    // Snapshot the registry AFTER pagination, when every fallback face the
    // document needs has been interned, then serve those faces to the renderer.
    let registry = shaper.registry();
    let fonts = RegistryFontSource::new(&registry);

    let name = file_stem(path);
    let limit = max_pages.unwrap_or(usize::MAX);
    for page in pages.pages.iter().take(limit) {
        let width = page.page_size.width.to_device_px(dpi).ceil().max(1.0) as u32;
        let height = page.page_size.height.to_device_px(dpi).ceil().max(1.0) as u32;
        // `RenderError` is `Debug`-only (no `Error` impl), so surface those two
        // failures as messages rather than boxing them directly.
        let mut surface = Surface::new(width, height)
            .map_err(|error| format!("surface {width}x{height}: {error:?}"))?;
        render(&compose_page(page), &mut surface, dpi, &fonts, &media);
        let png = surface
            .encode_png()
            .map_err(|error| format!("png encode: {error:?}"))?;
        let out = outdir.join(format!("{name}_p{}.png", page.number));
        fs::write(&out, png)?;
    }

    Ok(FileSummary {
        pages: page_count,
        disposition: disposition_summary(&imported.report.entries),
    })
}

/// A compact summary of the report entries that did not map cleanly:
/// `  [omitted 5: chart×2, oleObject; degraded 1: tableStyle]`. Empty when the
/// document has no omitted/degraded features.
fn disposition_summary(entries: &[casual_doc_import::CompatibilityEntry]) -> String {
    let mut omitted: BTreeMap<&str, u32> = BTreeMap::new();
    let mut degraded: BTreeMap<&str, u32> = BTreeMap::new();
    for entry in entries {
        let bucket = match entry.model_outcome {
            ModelOutcome::Omitted => &mut omitted,
            ModelOutcome::Degraded => &mut degraded,
            ModelOutcome::Mapped => continue,
        };
        *bucket.entry(entry.feature.as_str()).or_insert(0) += entry.occurrences;
    }

    let mut parts = Vec::new();
    if !omitted.is_empty() {
        parts.push(format!(
            "omitted {}: {}",
            total(&omitted),
            features(&omitted)
        ));
    }
    if !degraded.is_empty() {
        parts.push(format!(
            "degraded {}: {}",
            total(&degraded),
            features(&degraded)
        ));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("  [{}]", parts.join("; "))
    }
}

fn total(counts: &BTreeMap<&str, u32>) -> u32 {
    counts.values().sum()
}

/// Formats up to [`MAX_LISTED_FEATURES`] `feature×count` pairs (dropping the `×1`
/// for singletons), then `…` if more remain.
fn features(counts: &BTreeMap<&str, u32>) -> String {
    let mut listed: Vec<String> = counts
        .iter()
        .take(MAX_LISTED_FEATURES)
        .map(|(feature, count)| {
            if *count > 1 {
                format!("{feature}\u{00d7}{count}")
            } else {
                (*feature).to_owned()
            }
        })
        .collect();
    if counts.len() > MAX_LISTED_FEATURES {
        listed.push("\u{2026}".to_owned());
    }
    listed.join(", ")
}

/// The file's stem (`report.docx` -> `report`) for the PNG basename; falls back
/// to `page` for a path with no usable stem.
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "page".to_owned())
}

/// The file's display name for report lines.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Recovers a readable message from a caught panic payload.
fn panic_message(payload: &Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "panicked".to_owned()
    }
}
