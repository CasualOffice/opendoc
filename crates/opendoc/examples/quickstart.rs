// SPDX-License-Identifier: Apache-2.0

//! Build a document, edit it in one all-or-nothing batch, then save it as DOCX
//! with a loss report, as PDF and as a page image, all from one layout.
//!
//! `cargo run -p opendoc --example quickstart -- out-dir`. `tests/sdk.rs` runs
//! the same `run` function, so this file cannot rot.

use std::path::Path;

use opendoc::{Document, Error, ErrorCode, Format, Position, RunFormat, SaveMode};

/// Writes `report.docx`, `report.pdf` and `page-1.png` into `out`; returns
/// the page count and the features the DOCX save lost (none here).
pub fn run(out: &Path) -> opendoc::Result<(usize, Vec<String>)> {
    let mut doc = Document::create()?;
    doc.edit(|e| {
        e.append_heading("Quarterly report", 1)?;
        let p = e.append_paragraph("Revenue grew ", None)?;
        e.append_run(&p, "12%", &RunFormat::bold())?;
        e.insert_table(&Position::End, 2, 2, &[vec!["Region".into(), "Q2".into()]])?;
        Ok(())
    })?;

    let (docx, report) = doc.save(Format::Docx, SaveMode::PreserveWhenSafe)?;
    let write = |name: &str, bytes: &[u8]| {
        std::fs::create_dir_all(out)
            .and_then(|()| std::fs::write(out.join(name), bytes))
            .map_err(|e| Error::new(ErrorCode::InvalidArgument, e.to_string()))
    };
    write("report.docx", &docx)?;
    write("report.pdf", &doc.to_pdf()?)?;
    write("page-1.png", &doc.render_page_png(0, 96.0)?)?;
    let lost = report.losses.iter().map(|l| l.feature.clone()).collect();
    Ok((doc.page_count(), lost))
}

#[allow(dead_code)]
fn main() -> opendoc::Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "out".to_owned());
    run(Path::new(&out)).map(|_| ())
}
