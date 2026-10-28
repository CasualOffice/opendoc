// SPDX-License-Identifier: Apache-2.0

//! `opendoc-command-gen`: writes, or with `--check` verifies, the files the
//! capability registry generates (`docs/181` §6, G3, G4). See
//! `casual_doc_command::schema::artifacts` for the list.
//!
//! ```text
//! cargo run -p casual-doc-command --bin opendoc-command-gen            # write
//! cargo run -p casual-doc-command --bin opendoc-command-gen -- --check # verify; exit 1 on drift
//! ```
//!
//! `--root DIR` names the repository root; the default is this crate's
//! `../..`. `webapp/build.sh` runs `--check`.

#![forbid(unsafe_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use casual_doc_command::schema::artifacts::{self, DIRECTORY, REGENERATE};

/// One line to stdout or stderr (the workspace forbids the print macros).
fn say(out: &mut impl Write, line: std::fmt::Arguments<'_>) {
    let _ = writeln!(out, "{line}");
}

fn write(root: &Path) -> Result<usize, String> {
    let all = artifacts::all()?;
    for directory in [DIRECTORY.to_owned(), format!("{DIRECTORY}/edit-schema")] {
        let path = root.join(&directory);
        std::fs::create_dir_all(&path).map_err(|e| format!("{directory}: {e}"))?;
        // A family that lost its last command leaves no stale schema behind.
        for entry in std::fs::read_dir(&path).map_err(|e| format!("{directory}: {e}"))? {
            let found = entry.map_err(|e| format!("{directory}: {e}"))?.path();
            let name = found
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let relative = format!("{directory}/{name}");
            if found.is_file() && !all.iter().any(|artifact| artifact.path == relative) {
                std::fs::remove_file(&found).map_err(|e| format!("{relative}: {e}"))?;
            }
        }
    }
    for artifact in &all {
        std::fs::write(root.join(&artifact.path), &artifact.contents)
            .map_err(|e| format!("{}: {e}", artifact.path))?;
    }
    Ok(all.len())
}

fn main() -> ExitCode {
    let mut check = false;
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--root" => match args.next() {
                Some(dir) => root = PathBuf::from(dir),
                None => {
                    say(
                        &mut std::io::stderr(),
                        format_args!("--root needs a directory"),
                    );
                    return ExitCode::from(2);
                }
            },
            _ => {
                say(
                    &mut std::io::stderr(),
                    format_args!("usage: opendoc-command-gen [--check] [--root DIR]"),
                );
                return ExitCode::from(2);
            }
        }
    }
    if check {
        match artifacts::check(&root) {
            Ok(drift) if drift.is_empty() => {
                say(
                    &mut std::io::stdout(),
                    format_args!("opendoc-command-gen: {DIRECTORY} is current"),
                );
                ExitCode::SUCCESS
            }
            Ok(drift) => {
                for line in drift {
                    say(
                        &mut std::io::stderr(),
                        format_args!("opendoc-command-gen: {line}"),
                    );
                }
                say(
                    &mut std::io::stderr(),
                    format_args!("opendoc-command-gen: regenerate with `{REGENERATE}`"),
                );
                ExitCode::FAILURE
            }
            Err(error) => {
                say(
                    &mut std::io::stderr(),
                    format_args!("opendoc-command-gen: {error}"),
                );
                ExitCode::FAILURE
            }
        }
    } else {
        match write(&root) {
            Ok(count) => {
                say(
                    &mut std::io::stdout(),
                    format_args!("opendoc-command-gen: wrote {count} files under {DIRECTORY}"),
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                say(
                    &mut std::io::stderr(),
                    format_args!("opendoc-command-gen: {error}"),
                );
                ExitCode::FAILURE
            }
        }
    }
}
