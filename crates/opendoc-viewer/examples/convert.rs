//! The headless render/convert call, as a binary: the artefact `docs/176` §4
//! measures for the native cut (size, peak memory, time to first page).
//!
//! ```sh
//! cargo run -p opendoc-viewer --release --example convert -- in.docx out-dir [--pdf]
//! ```
//!
//! Writes `page-N.png` (96 dpi) for every page and, with `--pdf` (feature `pdf`),
//! `document.pdf`. Prints one line: pages, open time, render time.

use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(args.next().ok_or("usage: convert <in> <out-dir> [--pdf]")?);
    let outdir = PathBuf::from(args.next().ok_or("usage: convert <in> <out-dir> [--pdf]")?);
    let want_pdf = args.any(|arg| arg == "--pdf");
    std::fs::create_dir_all(&outdir)?;

    let bytes = std::fs::read(&input)?;
    let started = Instant::now();
    let doc = opendoc_viewer::ViewerDocument::open(&bytes)?;
    let opened = started.elapsed();
    for index in 0..doc.page_count() {
        let png = doc.render_page_png(index, 96.0)?;
        std::fs::write(outdir.join(format!("page-{}.png", index + 1)), png)?;
    }
    let rendered = started.elapsed() - opened;
    if want_pdf {
        #[cfg(feature = "pdf")]
        std::fs::write(outdir.join("document.pdf"), doc.to_pdf()?)?;
        #[cfg(not(feature = "pdf"))]
        return Err("built without the `pdf` feature".into());
    }
    let summary = format!(
        "pages={} open_ms={} render_ms={}\n",
        doc.page_count(),
        opened.as_millis(),
        rendered.as_millis()
    );
    std::io::Write::write_all(&mut std::io::stdout(), summary.as_bytes())?;
    Ok(())
}
