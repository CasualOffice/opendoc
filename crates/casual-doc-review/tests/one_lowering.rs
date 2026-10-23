// SPDX-License-Identifier: Apache-2.0

//! `docs/178` §12 R1: two lowerings of a suggestion drift, and an agent's
//! tracked change stops matching a person's. The splice primitives that turn a
//! review command into a review body live in this crate, and the editor reaches
//! them only through `casual_doc_review::lower`. This guard fails the build if
//! the editor's crate calls a primitive itself again, which is the shape a
//! second lowering would take.

use std::path::{Path, PathBuf};

/// The primitives a lowering is made of. A caller that names one of these is
/// building a review body by hand instead of calling `lower`.
const PRIMITIVES: &[&str] = &[
    "insert_review_revision(",
    "wrap_review_deletion(",
    "apply_review_format_change(",
    "extend_review_group_insertion(",
    "extend_authored_insertion_interior(",
    "remove_authored_review_insertion(",
    "strip_authored_insertions_in_range(",
    "insert_review_comment_markers(",
    "insert_comment_markers_at(",
    "decide_review_revision(",
    "decide_review_format_change(",
    "decide_all_review_revisions(",
    "remove_review_comment_markers(",
];

fn production_lines(path: &Path) -> Vec<(usize, String)> {
    let text = std::fs::read_to_string(path).expect("a readable source file");
    let lines: Vec<&str> = text.lines().collect();
    // Production code ends at the first `#[cfg(test)]` that opens a module.
    let end = (0..lines.len())
        .find(|&index| {
            lines[index].trim_end() == "#[cfg(test)]"
                && lines
                    .get(index + 1)
                    .is_some_and(|next| next.starts_with("mod "))
        })
        .unwrap_or(lines.len());
    lines[..end]
        .iter()
        .enumerate()
        .map(|(index, line)| (index + 1, (*line).to_owned()))
        .collect()
}

#[test]
fn the_editor_lowers_no_review_command_itself() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../casual-doc-wasm/src");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&src)
        .expect("the editor crate's sources are readable")
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        // Whole-file test modules (`#[path = "x_tests.rs"] mod tests;`) are test code.
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with("_tests.rs"))
        })
        .collect();
    files.sort();
    assert!(
        files.iter().any(|path| path.ends_with("lib.rs")),
        "the scan must include the editor's lib.rs, or it passes for the wrong reason: {files:?}"
    );
    let mut found = Vec::new();
    for path in &files {
        for (number, line) in production_lines(path) {
            let code = line.split("//").next().unwrap_or("");
            for primitive in PRIMITIVES {
                if code.contains(primitive) {
                    found.push(format!(
                        "{}:{number}: {}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "casual-doc-wasm builds a review body itself instead of calling \
         `casual_doc_review::lower`, which is a second lowering that will drift from \
         the one the Native SDK and the MCP server use (docs/178 §12 R1). Move it \
         into `lower.rs` and call it from both:\n{}",
        found.join("\n")
    );
}
