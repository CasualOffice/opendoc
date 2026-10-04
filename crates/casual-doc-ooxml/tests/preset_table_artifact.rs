// SPDX-License-Identifier: Apache-2.0

//! The committed preset-geometry table is what the generator derives from the
//! committed specification input — checked, not trusted.
//!
//! `105` EV-001: a derived artifact is re-derivable from a committed input, or it is
//! not evidence. The table is 108 KB of geometry nobody reads by eye, so without this
//! a hand edit, a half-finished regeneration, or a stale checkout would all look
//! exactly like correct data.
//!
//! Both files are `include_str!`-ed so editing either forces a recompile of this guard.

/// The vendored ECMA-376 preset definitions (`fixtures/spec/README.md` for provenance).
const INPUT: &str = include_str!("../../../fixtures/spec/presetShapeDefinitions.xml");

/// The committed table its consumer reads.
const COMMITTED: &str = include_str!("../../casual-doc-layout/src/shape_preset_table.txt");

#[test]
fn the_committed_table_is_what_the_generator_derives() {
    let derived = casual_doc_ooxml::preset_table::derive_preset_table(INPUT)
        .expect("the vendored specification data parses");
    if derived != COMMITTED {
        // A 108 KB diff is unreadable, so report the first divergence and its line.
        let first = derived
            .lines()
            .zip(COMMITTED.lines())
            .enumerate()
            .find(|(_, (left, right))| left != right);
        let detail = match first {
            Some((index, (derived_line, committed_line))) => format!(
                "first difference at line {}:\n  committed: {committed_line}\n  derived:   {derived_line}",
                index + 1
            ),
            None => format!(
                "one is a prefix of the other: committed {} lines, derived {} lines",
                COMMITTED.lines().count(),
                derived.lines().count()
            ),
        };
        panic!(
            "crates/casual-doc-layout/src/shape_preset_table.txt is stale. Run \
             `cargo run -p casual-doc-ooxml --example generate_preset_table` and commit \
             the result.\n{detail}"
        );
    }
}

#[test]
fn the_table_covers_every_preset_the_input_declares() {
    // Counted from both sides rather than written here, so the number cannot drift
    // from the data. The input declares 187 presets; the generator emitted 186 until a
    // name-based filter that was swallowing the preset literally called `rect` was
    // removed, and this is the guard that would have caught it.
    let declared = INPUT.matches("\n  <").count();
    let emitted = COMMITTED
        .lines()
        .filter(|line| line.starts_with("P\t"))
        .count();
    assert_eq!(
        emitted, 187,
        "the table carries every ECMA-376 Part 1 §20.1.9 preset"
    );
    assert!(
        declared >= emitted,
        "the input cannot declare fewer presets than the table emits: {declared} vs {emitted}"
    );
    assert!(
        COMMITTED.lines().any(|line| line == "P\trect"),
        "the preset named like a child element is present"
    );
}
