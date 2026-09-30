// SPDX-License-Identifier: Apache-2.0

//! CLI for the ONLYOFFICE capability-parity matrix.
//!
//! See the crate documentation for what the matrix is and what it can and
//! cannot establish mechanically.

// A CLI reporting tool legitimately writes to stdout/stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use opendoc_parity::matrix::{Map, grade, regions};
use opendoc_parity::ours::read_inventory;
use opendoc_parity::surface::{Surface, extract};
use opendoc_parity::{DOC, MAP_JSON, SURFACE_JSON, splice};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = repo_root();
    let result = match args.first().map(String::as_str) {
        Some("check") => run(&repo, false),
        Some("write") => run(&repo, true),
        Some("extract") => run_extract(&repo, &args[1..]),
        _ => {
            eprintln!(
                "usage: opendoc-parity <check|write|extract --reference <dir> [--check]>\n\
                 \n\
                 check    regenerate the matrix and fail if `{DOC}` differs\n\
                 write    regenerate the matrix into `{DOC}`\n\
                 extract  re-read ONLYOFFICE's tree into `{SURFACE_JSON}`"
            );
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// The repository root: three levels up from this crate's manifest
/// (`tools/opendoc-parity` → `tools` → root).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

fn run(repo: &Path, write: bool) -> Result<(), String> {
    let map: Map =
        serde_json::from_str(&read(repo, MAP_JSON)?).map_err(|e| format!("{MAP_JSON}: {e}"))?;
    let surface: Surface = serde_json::from_str(&read(repo, SURFACE_JSON)?)
        .map_err(|e| format!("{SURFACE_JSON}: {e}"))?;
    let inventory = read_inventory(repo).map_err(|e| e.to_string())?;

    let (rows, problems) = grade(&map, &inventory, &surface, repo);
    if !problems.is_empty() {
        let mut message = format!(
            "{} contradictions between the matrix and the two inventories:\n",
            problems.len()
        );
        for problem in &problems {
            message.push_str(&format!("  {problem}\n"));
        }
        return Err(message);
    }

    let doc = read(repo, DOC)?;
    let next = splice(&doc, &regions(&rows, &map, &surface, &inventory))?;
    if next == doc {
        println!("{DOC} is up to date ({} rows).", rows.len());
        return Ok(());
    }
    if write {
        fs::write(repo.join(DOC), &next).map_err(|e| format!("{DOC}: {e}"))?;
        println!("{DOC} regenerated ({} rows).", rows.len());
        Ok(())
    } else {
        Err(format!(
            "{DOC} is stale: its generated regions are not what the inventories now produce.\n\
             Run `cargo run -p opendoc-parity -- write` and commit the result."
        ))
    }
}

fn run_extract(repo: &Path, args: &[String]) -> Result<(), String> {
    let mut reference: Option<PathBuf> = None;
    let mut check = false;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--reference" => {
                reference = Some(PathBuf::from(
                    rest.next().ok_or("`--reference` needs a directory")?,
                ));
            }
            "--check" => check = true,
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    let reference = reference.ok_or(
        "`extract` needs `--reference <dir>`, the directory holding `web-apps/` and `sdkjs/`.\n\
         Their source is not vendored here: only identifiers extracted from it are.",
    )?;

    // Only `--check` needs the committed snapshot. A plain extraction must work
    // when the snapshot's SHAPE has changed — that is exactly when it is being
    // re-run — so parsing it first would make the tool unable to produce the
    // artifact that would have fixed it.
    let committed: Option<Surface> = if check {
        Some(
            serde_json::from_str(&read(repo, SURFACE_JSON)?)
                .map_err(|e| format!("{SURFACE_JSON}: {e}"))?,
        )
    } else {
        None
    };
    let taken_at = committed
        .as_ref()
        .map_or_else(today, |snapshot| snapshot.taken_at.clone());
    let fresh = extract(&reference, &taken_at).map_err(|e| e.to_string())?;

    if let Some(committed) = committed {
        if fresh == committed {
            println!(
                "{SURFACE_JSON} matches {} ({} controls, {} locale keys, {} API methods).",
                reference.display(),
                fresh.controls.len(),
                fresh.locale_keys.len(),
                fresh.api_methods.len()
            );
            return Ok(());
        }
        return Err(format!(
            "{SURFACE_JSON} no longer matches the reference tree at {}.\n\
             committed: {} controls, {} locale keys, {} API methods, {} boot flags\n\
             now:       {} controls, {} locale keys, {} API methods, {} boot flags\n\
             Re-run `extract --reference <dir>` and re-grade any row whose anchor moved.",
            reference.display(),
            committed.controls.len(),
            committed.locale_keys.len(),
            committed.api_methods.len(),
            committed.app_options.len(),
            fresh.controls.len(),
            fresh.locale_keys.len(),
            fresh.api_methods.len(),
            fresh.app_options.len(),
        ));
    }

    let json = serde_json::to_string_pretty(&fresh).map_err(|e| e.to_string())?;
    fs::write(repo.join(SURFACE_JSON), format!("{json}\n"))
        .map_err(|e| format!("{SURFACE_JSON}: {e}"))?;
    println!(
        "{SURFACE_JSON} written: {} controls, {} locale keys, {} API methods, {} boot flags.",
        fresh.controls.len(),
        fresh.locale_keys.len(),
        fresh.api_methods.len(),
        fresh.app_options.len()
    );
    Ok(())
}

fn read(repo: &Path, rel: &str) -> Result<String, String> {
    fs::read_to_string(repo.join(rel)).map_err(|e| format!("{rel}: {e}"))
}

/// Today, as `YYYY-MM-DD`, from the clock — the snapshot records when it was
/// taken so a reader can tell how old the competitor half is.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = secs / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`, days since 1970-01-01 to a Gregorian
/// date. Vendored as arithmetic rather than as a dependency: one date, once a
/// snapshot, is not worth a crate in `deny.toml`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}
