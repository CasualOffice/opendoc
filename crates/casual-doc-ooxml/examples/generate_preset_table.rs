// SPDX-License-Identifier: Apache-2.0

//! Writes the committed preset-geometry table from the vendored ECMA-376 data.
//!
//! Input: `fixtures/spec/presetShapeDefinitions.xml` (see that directory's README for
//! provenance). Output: `crates/casual-doc-layout/src/shape_preset_table.txt`.
//!
//! A thin wrapper: the derivation itself is
//! [`casual_doc_ooxml::preset_table::derive_preset_table`], so this generator and the
//! guard that checks its output run the same code rather than two copies that could be
//! wrong together.

// A generator legitimately writes to stdout.
#![allow(clippy::print_stdout)]

use std::fs;
use std::path::PathBuf;

use casual_doc_ooxml::preset_table::derive_preset_table;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let input = root.join("fixtures/spec/presetShapeDefinitions.xml");
    let output = root.join("crates/casual-doc-layout/src/shape_preset_table.txt");
    let table = derive_preset_table(&fs::read_to_string(&input)?)?;
    fs::write(&output, &table)?;
    println!(
        "wrote {} presets, {} bytes to {}",
        table.lines().filter(|line| line.starts_with("P\t")).count(),
        table.len(),
        output.display()
    );
    Ok(())
}
