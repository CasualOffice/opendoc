// SPDX-License-Identifier: Apache-2.0

//! `opendoc`: the OpenDoc engine on the command line, and the server-side
//! preview engine (`docs/176` §12.6).
//!
//! ```text
//! opendoc convert  IN OUT [--to FMT] [--mode exact|preserve|semantic] [--report FILE]
//! opendoc render   IN [--out DIR] [--format png|pdf] [--dpi N] [--pages 1-3,5]
//! opendoc info     IN [--json]
//! opendoc diff     OLD NEW [--json]
//! opendoc template TEMPLATE DATA.json OUT [--delimiters "[[,]]"] [--strict] [--null TEXT]
//! opendoc build    SPEC.json OUT [--media FILE]...
//! opendoc formats
//! ```
//!
//! Common options: `--system-fonts` (lay out with the machine's fonts too;
//! pixels then depend on the machine), `--font FILE` (add a face; repeatable),
//! `--author NAME`.
//!
//! Exit codes: 0 success, 1 a document or engine error (the message names its
//! stable code), 2 a usage error, 3 `convert --strict-loss` refused a lossy save.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use opendoc::{Document, Format, OpenOptions, RenderOptions, SaveMode, TemplateOptions};
use serde_json::{Value, json};

const USAGE: &str = "usage:
  opendoc convert  IN OUT [--to FMT] [--mode exact|preserve|semantic] [--report FILE] [--strict-loss]
  opendoc render   IN [--out DIR] [--format png|pdf] [--dpi N] [--pages 1-3,5]
  opendoc info     IN [--json]
  opendoc diff     OLD NEW [--json]
  opendoc template TEMPLATE DATA.json OUT [--delimiters \"[[,]]\"] [--strict] [--null TEXT]
  opendoc build    SPEC.json OUT [--media FILE]...
  opendoc formats
common: --system-fonts  --font FILE  --author NAME
formats: docx dotx odt rtf txt json html md pdf";

/// A failure: what to print and the exit code.
struct Failure {
    code: u8,
    message: String,
}

impl From<opendoc::Error> for Failure {
    fn from(error: opendoc::Error) -> Self {
        Self {
            code: 1,
            message: error.to_string(),
        }
    }
}

fn usage(message: impl Into<String>) -> Failure {
    Failure {
        code: 2,
        message: format!("{}\n{USAGE}", message.into()),
    }
}

fn io(path: &Path, error: &std::io::Error) -> Failure {
    Failure {
        code: 1,
        message: format!("{}: {error}", path.display()),
    }
}

/// Parsed arguments: positionals, flags, and repeatable options.
#[derive(Default)]
struct Args {
    positional: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

/// Options that take a value.
const VALUED: &[&str] = &[
    "--to",
    "--mode",
    "--report",
    "--out",
    "--format",
    "--dpi",
    "--pages",
    "--delimiters",
    "--null",
    "--media",
    "--font",
    "--author",
];

impl Args {
    fn parse(raw: impl Iterator<Item = String>) -> Result<Self, Failure> {
        let mut args = Self::default();
        let mut raw = raw.peekable();
        while let Some(arg) = raw.next() {
            if let Some((name, value)) = arg.split_once('=').filter(|_| arg.starts_with("--")) {
                args.options.push((name.to_owned(), Some(value.to_owned())));
            } else if arg.starts_with("--") {
                if VALUED.contains(&arg.as_str()) {
                    let value = raw
                        .next()
                        .ok_or_else(|| usage(format!("{arg} needs a value")))?;
                    args.options.push((arg, Some(value)));
                } else {
                    args.options.push((arg, None));
                }
            } else {
                args.positional.push(arg);
            }
        }
        Ok(args)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.options
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| v.as_deref())
    }

    fn values(&self, name: &str) -> Vec<&str> {
        self.options
            .iter()
            .filter(|(n, _)| n == name)
            .filter_map(|(_, v)| v.as_deref())
            .collect()
    }

    fn flag(&self, name: &str) -> bool {
        self.options.iter().any(|(n, _)| n == name)
    }

    fn check(&self, allowed: &[&str], positionals: usize) -> Result<(), Failure> {
        let common = ["--system-fonts", "--font", "--author"];
        for (name, _) in &self.options {
            if !allowed.contains(&name.as_str()) && !common.contains(&name.as_str()) {
                return Err(usage(format!("unknown option {name}")));
            }
        }
        if self.positional.len() != positionals {
            return Err(usage(format!(
                "expected {positionals} argument(s), got {}",
                self.positional.len()
            )));
        }
        Ok(())
    }

    fn open_options(&self, path: &str) -> Result<OpenOptions, Failure> {
        let mut fonts = Vec::new();
        for font in self.values("--font") {
            let path = PathBuf::from(font);
            fonts.push(std::fs::read(&path).map_err(|e| io(&path, &e))?);
        }
        Ok(OpenOptions {
            format: None,
            file_name: Some(path.to_owned()),
            author: self.value("--author").map(str::to_owned),
            render: RenderOptions {
                system_fonts: self.flag("--system-fonts"),
                fonts,
            },
        })
    }

    fn open(&self, path: &str) -> Result<Document, Failure> {
        let file = PathBuf::from(path);
        let bytes = std::fs::read(&file).map_err(|e| io(&file, &e))?;
        Ok(Document::open_with(&bytes, &self.open_options(path)?)?)
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| io(parent, &e))?;
    }
    std::fs::write(path, bytes).map_err(|e| io(path, &e))
}

fn format_of(path: &str, explicit: Option<&str>) -> Result<Format, Failure> {
    match explicit {
        Some(name) => Ok(name.parse()?),
        None => Format::from_path(path).ok_or_else(|| {
            usage(format!(
                "cannot tell the format of {path} from its extension; pass --to"
            ))
        }),
    }
}

/// `1-3,5` → zero-based page indices, bounded by `count`.
fn pages(spec: Option<&str>, count: usize) -> Result<Vec<usize>, Failure> {
    let Some(spec) = spec else {
        return Ok((0..count).collect());
    };
    let mut out = Vec::new();
    for part in spec.split(',') {
        let (a, b) = part.split_once('-').unwrap_or((part, part));
        let parse = |s: &str| {
            s.trim()
                .parse::<usize>()
                .ok()
                .filter(|n| *n >= 1)
                .ok_or_else(|| usage(format!("\"{part}\" is not a page or a range like 2-4")))
        };
        let (a, b) = (parse(a)?, parse(b)?);
        if a > b || b > count {
            return Err(usage(format!(
                "pages {part} are outside 1-{count} (the document has {count})"
            )));
        }
        out.extend(a - 1..b);
    }
    Ok(out)
}

fn convert(args: &Args, out: &mut dyn Write) -> Result<(), Failure> {
    args.check(&["--to", "--mode", "--report", "--strict-loss"], 2)?;
    let (input, output) = (&args.positional[0], &args.positional[1]);
    let doc = args.open(input)?;
    let to = format_of(output, args.value("--to"))?;
    let mode = match args.value("--mode") {
        Some(mode) => mode.parse()?,
        None if doc.source_format() == Some(to) => SaveMode::PreserveWhenSafe,
        None => SaveMode::Semantic,
    };
    let (bytes, report) = doc.save(to, mode)?;
    if let Some(path) = args.value("--report") {
        let json = serde_json::to_vec_pretty(&report).unwrap_or_default();
        write(Path::new(path), &json)?;
    }
    if args.flag("--strict-loss") && !report.losses.is_empty() && !report.lossy_by_nature {
        return Err(Failure {
            code: 3,
            message: format!(
                "refused: the save would lose {}",
                report
                    .losses
                    .iter()
                    .map(|l| l.feature.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    write(Path::new(output), &bytes)?;
    let _ = writeln!(
        out,
        "{input} ({}) -> {output} ({to}, {}): {} bytes, {}",
        doc.source_format().map_or("?", Format::name),
        report.mode,
        bytes.len(),
        if report.losses.is_empty() {
            "nothing lost".to_owned()
        } else {
            format!("{} loss(es)", report.losses.len())
        }
    );
    Ok(())
}

fn render(args: &Args, out: &mut dyn Write) -> Result<(), Failure> {
    args.check(&["--out", "--format", "--dpi", "--pages"], 1)?;
    let input = &args.positional[0];
    let doc = args.open(input)?;
    let dir = PathBuf::from(args.value("--out").unwrap_or("."));
    let stem = Path::new(input)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("page");
    match args.value("--format").unwrap_or("png") {
        "pdf" => {
            let path = dir.join(format!("{stem}.pdf"));
            let pdf = doc.to_pdf()?;
            write(&path, &pdf)?;
            let _ = writeln!(out, "{} ({} pages)", path.display(), doc.page_count());
        }
        "png" => {
            let dpi: f32 = args
                .value("--dpi")
                .unwrap_or("96")
                .parse()
                .ok()
                .filter(|d: &f32| d.is_finite() && *d > 0.0 && *d <= 1_200.0)
                .ok_or_else(|| usage("--dpi is a number from 1 to 1200"))?;
            for page in pages(args.value("--pages"), doc.page_count())? {
                let path = dir.join(format!("{stem}-{}.png", page + 1));
                write(&path, &doc.render_page_png(page, dpi)?)?;
                let _ = writeln!(out, "{}", path.display());
            }
        }
        other => return Err(usage(format!("--format is png or pdf, not {other}"))),
    }
    Ok(())
}

fn info(args: &Args, out: &mut dyn Write) -> Result<(), Failure> {
    args.check(&["--json"], 1)?;
    let doc = args.open(&args.positional[0])?;
    let report = doc.import_report();
    let paragraphs = doc.paragraphs();
    let value = json!({
        "format": doc.source_format().map(Format::name),
        "pages": doc.page_count(),
        "paragraphs": paragraphs.len(),
        "tables": doc.tables().len(),
        "sections": doc.sections().len(),
        "comments": doc.comments().len(),
        "revisions": doc.revisions().len(),
        "headersFooters": doc.headers_footers().len(),
        "notes": doc.notes().len(),
        "media": doc.media().len(),
        "properties": doc.properties(),
        "outline": doc.outline().into_iter().map(|(level, text, page)| json!({"level": level, "text": text, "page": page.map(|p| p + 1)})).collect::<Vec<_>>(),
        "fontsSubstituted": doc.font_needs(),
        "importFindings": report.findings.len(),
        "importLosses": report.losses().map(|f| f.feature.clone()).collect::<Vec<_>>(),
        "repairs": report.repairs.iter().map(|r| r.summary.clone()).collect::<Vec<_>>(),
    });
    if args.flag("--json") {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return Ok(());
    }
    let n = |key: &str| value[key].as_u64().unwrap_or(0);
    let _ = writeln!(
        out,
        "format      {}\npages       {}\nparagraphs  {}\ntables      {}\nsections    {}\ncomments    {}\nrevisions   {}\nnotes       {}\nmedia       {}",
        value["format"].as_str().unwrap_or("?"),
        n("pages"),
        n("paragraphs"),
        n("tables"),
        n("sections"),
        n("comments"),
        n("revisions"),
        n("notes"),
        n("media"),
    );
    if let Some(title) = doc.properties().title {
        let _ = writeln!(out, "title       {title}");
    }
    let losses = value["importLosses"].as_array().map_or(0, Vec::len);
    let _ = writeln!(
        out,
        "import      {} finding(s), {losses} not carried, {} repair(s)",
        n("importFindings"),
        report.repairs.len()
    );
    for (level, text, page) in doc.outline() {
        let _ = writeln!(
            out,
            "{}{text}  (p. {})",
            "  ".repeat(usize::from(level.max(1)) - 1),
            page.map_or("?".to_owned(), |p| (p + 1).to_string())
        );
    }
    Ok(())
}

fn diff(args: &Args, out: &mut dyn Write) -> Result<(), Failure> {
    args.check(&["--json"], 2)?;
    let old = args.open(&args.positional[0])?;
    let new = args.open(&args.positional[1])?;
    let diff = old.compare(&new)?;
    if args.flag("--json") {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&diff).unwrap_or_default()
        );
        return Ok(());
    }
    let changes = diff["changes"].as_array().cloned().unwrap_or_default();
    let _ = writeln!(
        out,
        "{} change(s){}",
        changes.len(),
        if diff["complete"].as_bool() == Some(false) {
            " (incomplete: see --json findings)"
        } else {
            ""
        }
    );
    for change in &changes {
        let kind = change["kind"].as_str().unwrap_or("change");
        let text = |side: &str| {
            change[side]
                .as_str()
                .map(|t| t.chars().take(80).collect::<String>())
        };
        match (text("leftText"), text("rightText")) {
            (Some(l), Some(r)) if l != r => {
                let _ = writeln!(out, "~ {kind}: \"{l}\" -> \"{r}\"");
            }
            (None, Some(r)) => {
                let _ = writeln!(out, "+ {kind}: \"{r}\"");
            }
            (Some(l), None) => {
                let _ = writeln!(out, "- {kind}: \"{l}\"");
            }
            _ => {
                let _ = writeln!(out, "~ {kind}");
            }
        }
    }
    Ok(())
}

fn template(args: &Args, out: &mut dyn Write) -> Result<(), Failure> {
    args.check(&["--delimiters", "--strict", "--null"], 3)?;
    let mut doc = args.open(&args.positional[0])?;
    let data_path = PathBuf::from(&args.positional[1]);
    let data: Value =
        serde_json::from_slice(&std::fs::read(&data_path).map_err(|e| io(&data_path, &e))?)
            .map_err(|e| usage(format!("{} is not JSON: {e}", data_path.display())))?;
    let mut options = TemplateOptions {
        strict: args.flag("--strict"),
        ..TemplateOptions::default()
    };
    if let Some(delimiters) = args.value("--delimiters") {
        let (start, end) = delimiters
            .split_once(',')
            .ok_or_else(|| usage("--delimiters is START,END, like \"[[,]]\""))?;
        options.start = start.to_owned();
        options.end = end.to_owned();
    }
    if let Some(null) = args.value("--null") {
        options.null_value = null.to_owned();
    }
    doc.render_template(&data, &options)?;
    let output = &args.positional[2];
    let to = format_of(output, None)?;
    let (bytes, _) = doc.save(
        to,
        if doc.source_format() == Some(to) {
            SaveMode::PreserveWhenSafe
        } else {
            SaveMode::Semantic
        },
    )?;
    write(Path::new(output), &bytes)?;
    let _ = writeln!(out, "{output}: {} bytes", bytes.len());
    Ok(())
}

fn build(args: &Args, out: &mut dyn Write) -> Result<(), Failure> {
    args.check(&["--media"], 2)?;
    let spec_path = PathBuf::from(&args.positional[0]);
    let spec = std::fs::read_to_string(&spec_path).map_err(|e| io(&spec_path, &e))?;
    let mut media = Vec::new();
    for path in args.values("--media") {
        let path = PathBuf::from(path);
        media.push(std::fs::read(&path).map_err(|e| io(&path, &e))?);
    }
    let mut doc = Document::from_spec_json(&spec, &media)?;
    doc.set_render_options(args.open_options(&args.positional[1])?.render);
    let output = &args.positional[1];
    let to = format_of(output, None)?;
    let mode = if to == Format::Docx {
        SaveMode::Exact
    } else {
        SaveMode::Semantic
    };
    let (bytes, _) = doc.save(to, mode)?;
    write(Path::new(output), &bytes)?;
    let _ = writeln!(out, "{output}: {} bytes", bytes.len());
    Ok(())
}

fn formats(args: &Args, out: &mut dyn Write) -> Result<(), Failure> {
    args.check(&[], 0)?;
    let _ = writeln!(out, "format  open  save");
    for format in Format::ALL {
        let mark = |b: bool| if b { "yes" } else { "-" };
        let _ = writeln!(
            out,
            "{:<7} {:<5} {}",
            format.name(),
            mark(format.can_open()),
            mark(format.can_save())
        );
    }
    Ok(())
}

fn run(mut raw: impl Iterator<Item = String>, out: &mut dyn Write) -> Result<(), Failure> {
    let command = raw.next().ok_or_else(|| usage("no command"))?;
    let args = Args::parse(raw)?;
    match command.as_str() {
        "convert" => convert(&args, out),
        "render" => render(&args, out),
        "info" => info(&args, out),
        "diff" => diff(&args, out),
        "template" => template(&args, out),
        "build" => build(&args, out),
        "formats" => formats(&args, out),
        "--version" | "version" => {
            let _ = writeln!(out, "opendoc {}", opendoc::VERSION);
            Ok(())
        }
        "--help" | "-h" | "help" => {
            let _ = writeln!(out, "{USAGE}");
            Ok(())
        }
        other => Err(usage(format!("unknown command {other}"))),
    }
}

fn main() -> ExitCode {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match run(std::env::args().skip(1), &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            let _ = writeln!(std::io::stderr(), "opendoc: {}", failure.message);
            ExitCode::from(failure.code)
        }
    }
}
