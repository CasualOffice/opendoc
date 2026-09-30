// SPDX-License-Identifier: Apache-2.0

//! The OpenDoc side of the matrix, read from this repository at run time.
//!
//! There is no committed snapshot of our own surface and there must not be one:
//! a snapshot would be a second copy of the inventory, and this repository's
//! recurring defect is exactly that — two hand-kept tables that drift until one
//! of them starts lying (`SKILL` §8). Our side is read from the files that
//! implement it, every time the matrix is generated or checked.
//!
//! The authority is `webapp/src/host_contract.mjs`. Its `COMMAND_CONTRACT` is
//! not a documentation table: `webapp/tests/e2e/host-contract.spec.mjs` drives
//! the editor in five states and asserts BOTH directions — every command the
//! editor offers resolves in the contract, and every contract row is offered by
//! the editor. So a command id present here is a capability a user can reach,
//! and a capability we ship is a command id present here. That guard is what
//! makes this file's reading of a `.mjs` module load-bearing rather than
//! decorative, and it is the only reason the matrix may rest on it.
//!
//! Complexity: one linear pass per file read, and the files are read once per
//! run.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Our declared command surface, as read from the contract module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    /// Every exact command id in `COMMAND_CONTRACT`.
    pub commands: BTreeSet<String>,
    /// Every prefix in `COMMAND_FAMILIES`. A family's members are generated at
    /// run time from the document, so they are not enumerable here — which is
    /// stated in the matrix rather than papered over.
    pub families: BTreeSet<String>,
}

impl Inventory {
    /// True when `id` is an exact contract row.
    #[must_use]
    pub fn has_command(&self, id: &str) -> bool {
        self.commands.contains(id)
    }

    /// True when `prefix` is a declared family.
    #[must_use]
    pub fn has_family(&self, prefix: &str) -> bool {
        self.families.contains(prefix)
    }
}

/// What went wrong while reading our side.
#[derive(Debug)]
pub enum OursError {
    /// The filesystem refused.
    Io(String),
    /// The contract module did not have the shape this reader pins.
    Shape(String),
}

impl std::fmt::Display for OursError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "reading this repository: {e}"),
            Self::Shape(e) => write!(f, "webapp/src/host_contract.mjs: {e}"),
        }
    }
}

impl std::error::Error for OursError {}

/// The contract module, relative to the repository root.
pub const CONTRACT_MODULE: &str = "webapp/src/host_contract.mjs";

/// Reads `webapp/src/host_contract.mjs` at `repo_root` and returns what it
/// declares.
///
/// The parse is deliberately strict. Inside each `Object.freeze([...])` block
/// every non-blank, non-comment line must be a recognised `exact(` or `family(`
/// call or a closing bracket; anything else is an error rather than a skip.
/// A lenient parser here would silently under-report our surface the first time
/// the module grew a new declaration shape, and under-reporting is the failure
/// this whole lane exists to end — `docs/105` CQ-005 claims "no i18n seam at
/// all" while nineteen locales ship.
///
/// # Errors
///
/// Returns [`OursError`] when the module cannot be read or does not have the
/// shape described above.
pub fn read_inventory(repo_root: &Path) -> Result<Inventory, OursError> {
    let path = repo_root.join(CONTRACT_MODULE);
    let text =
        fs::read_to_string(&path).map_err(|e| OursError::Io(format!("{}: {e}", path.display())))?;

    let commands = parse_block(
        &text,
        "export const COMMAND_CONTRACT = Object.freeze([",
        "exact(",
    )?;
    let families = parse_block(
        &text,
        "export const COMMAND_FAMILIES = Object.freeze([",
        "family(",
    )?;
    if commands.is_empty() {
        return Err(OursError::Shape(
            "COMMAND_CONTRACT parsed to nothing".to_string(),
        ));
    }
    if families.is_empty() {
        return Err(OursError::Shape(
            "COMMAND_FAMILIES parsed to nothing".to_string(),
        ));
    }
    Ok(Inventory { commands, families })
}

/// Pulls the first string argument of every `call(` inside the block that opens
/// with `header` and closes with a line that is exactly `]);`.
///
/// Entries may span several lines — `exact("view.zoom", null, [ … ])` declares
/// its argument list over three — so the scan tracks parenthesis depth and
/// treats a line as a continuation exactly while the depth is above zero. At
/// depth zero every non-comment line must open with `call`, which is what makes
/// a new declaration shape an error instead of a silently dropped command.
fn parse_block(text: &str, header: &str, call: &str) -> Result<BTreeSet<String>, OursError> {
    let start = text
        .find(header)
        .ok_or_else(|| OursError::Shape(format!("no `{header}`")))?;
    let body = &text[start + header.len()..];
    let mut out = BTreeSet::new();
    let mut closed = false;
    let mut depth: i32 = 0;
    for line in body.lines() {
        let trimmed = line.trim();
        if depth == 0 {
            if trimmed == "]);" {
                closed = true;
                break;
            }
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with('*')
                || trimmed.starts_with("/*")
            {
                continue;
            }
            let Some(rest) = trimmed.strip_prefix(call) else {
                return Err(OursError::Shape(format!(
                    "unrecognised line inside the `{call}` block, so the parse would \
                     under-report our surface: {trimmed}"
                )));
            };
            let value = first_string(rest)
                .ok_or_else(|| OursError::Shape(format!("no quoted id in: {trimmed}")))?;
            out.insert(value);
        }
        depth += paren_balance(trimmed);
        if depth < 0 {
            return Err(OursError::Shape(format!(
                "parentheses close below the block's own level at: {trimmed}"
            )));
        }
    }
    if !closed {
        return Err(OursError::Shape(format!(
            "`{header}` is never closed by `]);`"
        )));
    }
    Ok(out)
}

/// Open parentheses minus closing ones, ignoring anything inside a double-quoted
/// string and anything after a `//` comment.
fn paren_balance(line: &str) -> i32 {
    let mut depth = 0;
    let mut in_string = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if in_string => {
                chars.next();
            }
            '"' => in_string = !in_string,
            '/' if !in_string && chars.peek() == Some(&'/') => break,
            '(' if !in_string => depth += 1,
            ')' if !in_string => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// The contents of the first `"…"` in `text`.
fn first_string(text: &str) -> Option<String> {
    let open = text.find('"')?;
    let rest = &text[open + 1..];
    let close = rest.find('"')?;
    Some(rest[..close].to_string())
}

/// Resolves a grep anchor: `literal` must occur in `file`, relative to the
/// repository root.
///
/// Anchors are how the matrix cites a capability that is NOT a host command —
/// an importer behaviour, an export format, a dialog field. `docs/130` made the
/// same choice for the same reason and wrote down why: it first cited
/// `webapp/src/main.js` by line in about thirty places, and every one of those
/// citations was stale within a day because the file grew by 666 lines.
///
/// # Errors
///
/// Returns [`OursError`] when the file cannot be read.
pub fn anchor_resolves(repo_root: &Path, file: &str, literal: &str) -> Result<bool, OursError> {
    let path: PathBuf = repo_root.join(file);
    let text =
        fs::read_to_string(&path).map_err(|e| OursError::Io(format!("{}: {e}", path.display())))?;
    Ok(text.contains(literal))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
export const COMMAND_CONTRACT = Object.freeze([
  // a comment
  exact("file.new", "new"),
  exact("file.open", "open"),
]);

export const COMMAND_FAMILIES = Object.freeze([
  family("table.", "mutate", "table"),
]);
"#;

    #[test]
    fn the_contract_block_yields_its_ids() {
        let commands = parse_block(
            SAMPLE,
            "export const COMMAND_CONTRACT = Object.freeze([",
            "exact(",
        )
        .expect("parse");
        assert_eq!(
            commands.iter().map(String::as_str).collect::<Vec<_>>(),
            ["file.new", "file.open"]
        );
    }

    #[test]
    fn an_unrecognised_declaration_shape_is_an_error_not_a_skip() {
        let text = SAMPLE.replace(
            r#"exact("file.open", "open"),"#,
            r#"whatever("file.open"),"#,
        );
        let err = parse_block(
            &text,
            "export const COMMAND_CONTRACT = Object.freeze([",
            "exact(",
        )
        .expect_err("an unknown shape must fail the parse");
        assert!(format!("{err}").contains("under-report"), "{err}");
    }

    #[test]
    fn an_entry_spanning_several_lines_is_one_command_not_a_parse_error() {
        // The real module declares `exact("view.zoom", null, [ … ])` over three
        // lines, and a line-at-a-time parse rejected it.
        let text = "export const COMMAND_CONTRACT = Object.freeze([\n\
                    \x20 exact(\"view.zoom\", null, [\n\
                    \x20   Object.freeze({ name: \"percent\", type: \"number\" }),\n\
                    \x20 ]),\n\
                    \x20 exact(\"help.about\", null),\n\
                    ]);\n";
        let commands = parse_block(
            text,
            "export const COMMAND_CONTRACT = Object.freeze([",
            "exact(",
        )
        .expect("parse");
        assert_eq!(
            commands.iter().map(String::as_str).collect::<Vec<_>>(),
            ["help.about", "view.zoom"]
        );
    }

    #[test]
    fn a_parenthesis_inside_a_string_does_not_open_a_continuation() {
        assert_eq!(paren_balance(r#"exact("a(b", null),"#), 0);
        assert_eq!(paren_balance("exact(\"a\", null, [ // a ) in a comment"), 1);
    }

    #[test]
    fn an_unterminated_block_is_an_error() {
        let text = "export const COMMAND_CONTRACT = Object.freeze([\n  exact(\"a\", null),\n";
        let err = parse_block(
            text,
            "export const COMMAND_CONTRACT = Object.freeze([",
            "exact(",
        )
        .expect_err("an unterminated block must fail the parse");
        assert!(format!("{err}").contains("never closed"), "{err}");
    }
}
