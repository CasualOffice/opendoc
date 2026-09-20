//! The model consumer ledger (backlog row FID-P-04).
//!
//! `docs/105` calls **"modeled is not shipped, and built is not reachable"** the
//! most expensive recurring pattern in this repository. A construct gets typed in
//! `casual-doc-model`, wired through `casual-doc-import` and `casual-doc-export`,
//! round-trips cleanly, and is marked done — while nothing in layout, rendering,
//! the wasm facade or the webapp ever reads it, so no user can tell it exists. A
//! DOCX round-trip proves the bytes survive. It proves nothing about reachability.
//!
//! So every row of the model inventory in `docs/95-MODEL-COMPLETENESS-LAYER1-TRACKER.md`
//! must declare, in a **Consumer** column, one of three things:
//!
//! | form | meaning |
//! |---|---|
//! | `` `field` → `path`#`symbol` `` | code outside the round-trip reads this field |
//! | ``unconsumed `field` (ROW-ID)`` | typed, round-tripped, and unreachable |
//! | `preservation-only: …` | deliberately never consumed |
//!
//! and this test fails the build when a row declares nothing, when a claimed
//! consumer does not exist, or when the number of `unconsumed` rows grows.
//!
//! # Why a claimed consumer is verified rather than believed
//!
//! A cell reading "yes" would pass any guard that only checks the cell is
//! non-empty, and this repository has shipped exactly that kind of green-but-wrong
//! assertion before (skill §4). So a consumer claim names a file, a field and a
//! symbol, and all three are checked against the source on disk: the file must
//! exist, and must contain **both** the model field name and the symbol. That
//! cannot be satisfied by a plausible-looking sentence.
//!
//! # Why `unconsumed` exists, and why it is a ratchet
//!
//! The alternative to an explicit `unconsumed` state is marking the gaps
//! `preservation-only` — which would be a lie for most of them (they are intended
//! to be rendered, just not yet) — or failing the build on day one, which would
//! only get the guard deleted. The honest form is to name each gap, count them,
//! and cap the count: [`UNCONSUMED_CEILING`] is the number that existed when the
//! ledger was introduced. Adding another unconsumed row fails the build; closing
//! one should lower the ceiling. **Never raise it to get green.**
//!
//! # What this guard does not catch, stated so nobody over-trusts it
//!
//! The consumer check is textual: the file exists, contains the symbol, and
//! contains the field name. That catches a consumer deleted or renamed, and a
//! claim that was never true. It does **not** catch a consumer that reads a field
//! and then throws the value away — the percentage table-width row in `docs/95`
//! is exactly that shape, and is recorded as `unconsumed` with the reason spelled
//! out rather than cited as a consumer. Judging that case needs a human reading
//! the code; this guard's job is to make sure someone did, and to keep the answer
//! written down where a reviewer can disagree with it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The inventory this ledger governs.
///
/// `include_str!` so a doc edit forces a recompile of the guard. It returns CRLF
/// on a Windows checkout, which is why every read below goes through
/// [`tracker_text`] — a previous source-scanning guard in this repository passed
/// on every platform and failed only on Windows for exactly that reason.
const TRACKER_RAW: &str = include_str!("../../../docs/95-MODEL-COMPLETENESS-LAYER1-TRACKER.md");

/// Line endings normalised, so the parsing below is platform-independent.
fn tracker_text() -> String {
    TRACKER_RAW.replace('\r', "")
}

/// How many rows may declare themselves `unconsumed`.
///
/// 17 of 40 when the ledger was introduced (FID-P-04): five Tier-1 rows and twelve
/// Tier-2 rows are typed, round-tripped and unreachable from the product. This is
/// a **debt ceiling**. Lower it when a gap closes. Raising it means shipping
/// another construct a user cannot reach, and needs a decision, not an edit.
const UNCONSUMED_CEILING: usize = 17;

/// The inventory is two tables of Tier-1 and Tier-2 rows; far fewer than this
/// means the parser stopped matching the document and the guard has gone vacuous,
/// which is the failure mode that makes a green test worthless.
const MINIMUM_ROWS: usize = 40;

/// What one inventory row declares about who consumes it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Consumer {
    /// `field` is read by `symbol` in `path`.
    Code {
        field: String,
        path: String,
        symbol: String,
    },
    /// `field` is typed and round-tripped, and nothing reads it. Cites a tracker row.
    Unconsumed { field: String },
    /// Retention is the whole intent; a consumer is not expected.
    PreservationOnly,
}

/// One parsed row of the inventory: its item name and its consumer declaration.
#[derive(Clone, Debug)]
struct Row {
    item: String,
    consumer: Consumer,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The text between the first pair of backticks at or after `from`, and the index
/// just past the closing backtick.
fn backticked(cell: &str, from: usize) -> Option<(String, usize)> {
    let open = cell[from..].find('`')? + from + 1;
    let close = cell[open..].find('`')? + open;
    Some((cell[open..close].to_owned(), close + 1))
}

/// Parses one Consumer cell, or `None` if it declares nothing recognisable.
///
/// Trailing prose after the declaration is allowed and ignored — a row is
/// expected to explain itself.
fn parse_consumer(cell: &str) -> Option<Consumer> {
    let cell = cell.trim();
    if cell.is_empty() {
        return None;
    }
    if let Some(rest) = cell.strip_prefix("preservation-only") {
        // A bare "preservation-only" with no reason is not a declaration; the
        // reason is the part a reviewer can disagree with.
        return rest
            .trim_start_matches(':')
            .trim()
            .is_empty()
            .eq(&false)
            .then_some(Consumer::PreservationOnly);
    }
    if let Some(rest) = cell.strip_prefix("unconsumed") {
        let (field, after) = backticked(rest, 0)?;
        // A tracker row must own closing it, so the gap is queued rather than
        // merely confessed.
        let cites_a_row = rest[after..]
            .split(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'))
            .any(|token| {
                token.len() >= 5
                    && token.contains('-')
                    && token.starts_with(|c: char| c.is_ascii_uppercase())
            });
        return cites_a_row.then_some(Consumer::Unconsumed { field });
    }
    // `field` → `path`#`symbol`
    let (field, after_field) = backticked(cell, 0)?;
    let arrow = cell[after_field..].find('→')? + after_field;
    let (path, after_path) = backticked(cell, arrow)?;
    let hash = cell[after_path..].find('#')? + after_path;
    let (symbol, _) = backticked(cell, hash)?;
    Some(Consumer::Code {
        field,
        path,
        symbol,
    })
}

/// Every `✅` row of the inventory tables, with its Consumer cell parsed.
///
/// Returns the malformed rows separately rather than panicking, so the failure
/// message can list all of them at once.
fn parse_rows(tracker: &str) -> (Vec<Row>, Vec<String>) {
    let mut rows = Vec::new();
    let mut malformed = Vec::new();
    for line in tracker.lines() {
        let line = line.trim();
        if !line.starts_with("| ✅ |") {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        // Done | Item | OOXML | [Prev] | Scope | Consumer
        let (Some(item), Some(cell)) = (cells.get(1), cells.last()) else {
            malformed.push(format!("row has too few columns: {line}"));
            continue;
        };
        match parse_consumer(cell) {
            Some(consumer) => rows.push(Row {
                item: (*item).to_owned(),
                consumer,
            }),
            None => malformed.push(format!(
                "{item}: Consumer cell {cell:?} declares nothing the ledger recognises — use \
                 `field` → `path`#`symbol`, or unconsumed `field` (ROW-ID), or \
                 preservation-only: <reason>"
            )),
        }
    }
    (rows, malformed)
}

/// Every `.rs` file under `crates/casual-doc-model/src`, concatenated.
///
/// Used to prove an `unconsumed` row names a field the model actually has, so the
/// ledger cannot be padded with constructs that do not exist.
fn model_sources() -> String {
    fn walk(dir: &Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
    let mut out = String::new();
    walk(&repo_root().join("crates/casual-doc-model/src"), &mut out);
    out
}

#[test]
fn every_model_row_declares_who_consumes_it() {
    let tracker = tracker_text();
    let (rows, malformed) = parse_rows(&tracker);
    assert!(
        malformed.is_empty(),
        "docs/95 rows without a usable Consumer declaration:\n  {}",
        malformed.join("\n  ")
    );
    assert!(
        rows.len() >= MINIMUM_ROWS,
        "only {} inventory rows parsed out of docs/95 (expected at least {MINIMUM_ROWS}) — the \
         table shape changed and this guard has stopped seeing the inventory",
        rows.len()
    );
}

#[test]
fn every_claimed_consumer_exists_and_reads_the_field() {
    let tracker = tracker_text();
    let (rows, _) = parse_rows(&tracker);
    let root = repo_root();
    let mut failures = Vec::new();
    let mut checked = 0_usize;

    for row in &rows {
        let Consumer::Code {
            field,
            path,
            symbol,
        } = &row.consumer
        else {
            continue;
        };
        let full = root.join(path);
        let Ok(source) = std::fs::read_to_string(&full) else {
            failures.push(format!("{}: no such file as {path}", row.item));
            continue;
        };
        let source = source.replace('\r', "");
        if !source.contains(symbol) {
            failures.push(format!(
                "{}: {path} does not contain {symbol:?} — the named consumer is gone or renamed",
                row.item
            ));
        }
        if !source.contains(field) {
            failures.push(format!(
                "{}: {path} does not mention {field:?} — the cited consumer does not read the \
                 field, so the row claims reachability it does not have",
                row.item
            ));
        }
        checked += 1;
    }

    assert!(
        failures.is_empty(),
        "docs/95 consumer claims that do not hold:\n  {}",
        failures.join("\n  ")
    );
    // A ledger where nothing claims a consumer would pass the loop above trivially.
    assert!(
        checked >= 20,
        "only {checked} rows claim a code consumer; docs/95 had 23 when the ledger was written, \
         so either the tables changed shape or reachability regressed"
    );
}

#[test]
fn unconsumed_rows_name_a_real_model_field_and_do_not_multiply() {
    let tracker = tracker_text();
    let (rows, _) = parse_rows(&tracker);
    let model = model_sources();
    assert!(
        model.len() > 10_000,
        "the model sources did not load; this guard would pass vacuously"
    );

    let mut unconsumed = BTreeSet::new();
    let mut unknown = Vec::new();
    for row in &rows {
        if let Consumer::Unconsumed { field } = &row.consumer {
            unconsumed.insert(row.item.clone());
            if !model.contains(field) {
                unknown.push(format!(
                    "{}: `{field}` does not appear anywhere in casual-doc-model",
                    row.item
                ));
            }
        }
    }

    assert!(
        unknown.is_empty(),
        "unconsumed rows naming a field the model does not have:\n  {}",
        unknown.join("\n  ")
    );
    assert!(
        unconsumed.len() <= UNCONSUMED_CEILING,
        "{} model rows are modelled with no consumer, above the ceiling of {UNCONSUMED_CEILING}. \
         A construct that nothing reads is invisible to a user however cleanly it round-trips \
         (FID-P-04). Give it a consumer, or mark it preservation-only with a reason — do not \
         raise the ceiling.\n  {}",
        unconsumed.len(),
        unconsumed.iter().cloned().collect::<Vec<_>>().join("\n  ")
    );
    // The ceiling is a ratchet: if gaps closed, it must come down with them, or it
    // stops being a ratchet and becomes slack.
    assert_eq!(
        unconsumed.len(),
        UNCONSUMED_CEILING,
        "{} unconsumed rows remain but UNCONSUMED_CEILING is still {UNCONSUMED_CEILING}; lower \
         the ceiling in the same change that closes the gap",
        unconsumed.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_consumer_claim_parses_into_its_three_parts() {
        let parsed = parse_consumer(
            "`lvl_restart` → `crates/casual-doc-layout/src/numbering.rs`#`fn resolve`",
        );
        assert_eq!(
            parsed,
            Some(Consumer::Code {
                field: "lvl_restart".to_owned(),
                path: "crates/casual-doc-layout/src/numbering.rs".to_owned(),
                symbol: "fn resolve".to_owned(),
            })
        );
    }

    #[test]
    fn an_unconsumed_row_must_cite_a_tracker_row() {
        assert_eq!(
            parse_consumer("unconsumed `no_proof` (FID-P-04) — no spell checker exists"),
            Some(Consumer::Unconsumed {
                field: "no_proof".to_owned()
            })
        );
        // No row id: a confession with nobody owning it is not a declaration.
        assert_eq!(parse_consumer("unconsumed `no_proof`"), None);
        // No field: nothing to check against the model.
        assert_eq!(parse_consumer("unconsumed (FID-P-04)"), None);
    }

    #[test]
    fn preservation_only_must_say_why() {
        assert_eq!(
            parse_consumer("preservation-only: legacy CJK ruby, retained verbatim"),
            Some(Consumer::PreservationOnly)
        );
        assert_eq!(parse_consumer("preservation-only"), None);
        assert_eq!(parse_consumer("preservation-only:"), None);
    }

    #[test]
    fn hand_waving_is_not_a_declaration() {
        // The whole point: a cell has to be checkable, so none of these pass.
        for cell in [
            "",
            "yes",
            "consumed by layout",
            "`lvl_restart`",
            "`lvl_restart` → layout",
            "`lvl_restart` → `crates/casual-doc-layout/src/numbering.rs`",
        ] {
            assert_eq!(parse_consumer(cell), None, "{cell:?} should not parse");
        }
    }

    #[test]
    fn the_tracker_tables_are_still_the_shape_this_guard_parses() {
        let tracker = tracker_text();
        let (rows, malformed) = parse_rows(&tracker);
        assert!(malformed.is_empty(), "{malformed:?}");
        // Tier 1 (21) + Tier 2 (19).
        assert_eq!(rows.len(), 40, "docs/95 inventory size changed");
    }
}
