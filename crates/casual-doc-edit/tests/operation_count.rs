// SPDX-License-Identifier: Apache-2.0

//! Pins every documented size of the closed op set to the set itself.
//!
//! # Why this guard exists
//!
//! The op-set size has been published as **47**, **55** and **58** in seven
//! documents, two of which the site publishes, while the set itself grew to a
//! different number again. Nobody wrote a wrong number: each was right when it was
//! written, and each became a false claim the next time an operation was added.
//! `SKILL` §9 is explicit that counts in docs must be **derived, not
//! hand-maintained**, and `105` records two occasions when a hand-maintained figure
//! drifted into a false public claim.
//!
//! So this does not assert a number. It derives the number from
//! `casual_doc_edit::Operation` and requires the documents to agree with it, which
//! makes the next addition a test failure instead of a published falsehood.
//!
//! # Why it reads the source text
//!
//! Rust has no stable way to count an enum's variants (`std::mem::variant_count` is
//! unstable), and `Operation`'s variants carry data, so they cannot be enumerated in
//! a `const ALL` the way a plain token enum can. Reading the declaration is the
//! honest remaining option, and it is an established pattern here — `SKILL` §6
//! records a source scan guarding the "zero call sites" claim for the same reason.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The crate root, from which the source and the docs are both reachable.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Counts the top-level variants of `pub enum Operation` in the edit crate.
///
/// Depth-aware: a variant's own braces and parentheses nest, and `Operation` has
/// struct-like variants whose fields would otherwise be counted as variants.
fn derived_operation_count() -> usize {
    let source = std::fs::read_to_string(root().join("crates/casual-doc-edit/src/lib.rs"))
        .expect("the edit crate's source is readable");
    let start = source
        .find("pub enum Operation {")
        .expect("`pub enum Operation` is declared");
    let open = start + source[start..].find('{').expect("an opening brace");
    let mut depth = 0_i32;
    let mut end = open;
    for (offset, byte) in source[open..].bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &source[open + 1..end];
    let mut count = 0_usize;
    let mut depth = 0_i32;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if depth == 0 {
            let name: String = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // `trim_start` is load-bearing: the declaration writes `InsertText {`
            // with a space, so comparing the un-trimmed remainder rejected EVERY
            // variant and produced a count of zero. The implausibility assertion
            // below is what surfaced that rather than letting a silent zero through.
            let after = trimmed[name.len()..].trim_start();
            let is_variant = !name.is_empty()
                && name.starts_with(|c: char| c.is_ascii_uppercase())
                && (after.starts_with('{') || after.starts_with('(') || after.starts_with(','));
            if is_variant {
                count += 1;
            }
        }
        for byte in line.bytes() {
            match byte {
                b'{' | b'(' => depth += 1,
                b'}' | b')' => depth -= 1,
                _ => {}
            }
        }
    }
    count
}

/// Documents that state the size of the closed op set.
const DOCUMENTS: [&str; 7] = [
    "docs/08-ADR-REGISTER.md",
    "docs/105-AUDIT-2026-09-TRACKER.md",
    "docs/106-ONLYOFFICE-ALTERNATIVE-ROADMAP.md",
    "docs/107-COLLABORATION-OT-SNAPSHOT-REPLAY-DESIGN.md",
    "docs/113-WINDOWED-LAYOUT-DESIGN.md",
    "docs/125-EMBED-AND-HOST-CONTRACT-DESIGN.md",
    "docs/156-PRESENTATION-SUPPORT-AND-SHARED-DRAWING-CORE-DESIGN.md",
];

/// A line that legitimately names a number other than the total.
///
/// Two kinds exist and both are real: a **tier** count (`T1` is a subset of the set,
/// and `107` §3.1 sizes it separately) and the **other** op set — the five operations
/// `casual-doc-transaction`'s v0 model carries, which `105` CQ-002 contrasts with the
/// edit crate's deliberately. Exempting by marker rather than by line number keeps
/// the guard from breaking every time a paragraph moves.
fn names_a_subset_or_the_other_set(line: &str) -> bool {
    // Deliberately NARROW. An earlier version exempted any line containing "tier"
    // or "5 vs", which quietly excused two genuine stale totals — `08`'s
    // "Tractability over N operations" and `105` CQ-002's op-set contrast. An
    // exemption list that is too generous turns this guard into decoration, so a
    // line is exempt only when it names an APPROXIMATION or the other crate's set.
    ["~", "T1 ", "T2 ", "T3 ", "casual-doc-transaction"]
        .iter()
        .any(|marker| line.contains(marker))
}

/// Every `<n> operations` / `<n> ops` claim in a document, by line number.
fn claims(text: &str) -> BTreeMap<usize, usize> {
    let mut found = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if names_a_subset_or_the_other_set(line) {
            continue;
        }
        let bytes: Vec<char> = line.chars().collect();
        let mut position = 0;
        while position < bytes.len() {
            if bytes[position].is_ascii_digit() {
                let start = position;
                while position < bytes.len() && bytes[position].is_ascii_digit() {
                    position += 1;
                }
                let rest: String = bytes[position..].iter().collect();
                if rest.starts_with(" operations") || rest.starts_with(" ops") {
                    let number: String = bytes[start..position].iter().collect();
                    if let Ok(parsed) = number.parse::<usize>() {
                        found.insert(index + 1, parsed);
                    }
                }
            } else {
                position += 1;
            }
        }
    }
    found
}

#[test]
fn every_documented_op_set_size_matches_the_set_itself() {
    let derived = derived_operation_count();
    assert!(
        derived > 40,
        "the variant parse produced {derived}, which is implausible — the parser, not \
         the op set, is wrong"
    );

    let mut wrong: Vec<String> = Vec::new();
    let mut stated = 0_usize;
    for relative in DOCUMENTS {
        let path: &Path = relative.as_ref();
        let full = root().join(path);
        let Ok(text) = std::fs::read_to_string(&full) else {
            continue;
        };
        for (line, claimed) in claims(&text) {
            stated += 1;
            if claimed != derived {
                wrong.push(format!("{relative}:{line} says {claimed}"));
            }
        }
    }

    assert!(
        stated > 0,
        "no document states the op-set size any more, so this guard is no longer \
         guarding anything — delete it or point it at the document that does"
    );
    assert!(
        wrong.is_empty(),
        "the op set has {derived} operations; these disagree: {}\n\nThe number is \
         DERIVED from `pub enum Operation` — correct the documents, never this test.",
        wrong.join("; ")
    );
}

#[test]
fn the_variant_parser_is_not_fooled_by_struct_like_variants() {
    // The parse must count variants, not their fields. `Operation` has struct-like
    // variants carrying `node`/`range` fields that start with a lowercase letter, and
    // tuple variants whose types start with an uppercase one — so depth tracking is
    // what makes this correct, and a depth-blind parser would over-count badly.
    //
    // Guarded against the real declaration rather than a synthetic one: a count that
    // is merely self-consistent proves nothing about the file it reads.
    let derived = derived_operation_count();
    let source = std::fs::read_to_string(root().join("crates/casual-doc-edit/src/lib.rs"))
        .expect("readable");
    let naive = source
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            t.starts_with(|c: char| c.is_ascii_uppercase()) && t.ends_with('{')
        })
        .count();
    assert_ne!(
        derived, naive,
        "a depth-blind count happens to agree here, so this guard proves nothing"
    );
    assert!(
        derived < 200,
        "{derived} variants means the parser ran past the enum"
    );
}
