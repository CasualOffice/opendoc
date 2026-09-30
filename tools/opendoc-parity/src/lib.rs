// SPDX-License-Identifier: Apache-2.0

//! Derives the ONLYOFFICE capability-parity matrix published as `docs/153`.
//!
//! # Why this exists
//!
//! Parity with ONLYOFFICE Docs has reached this project as scattered one-off
//! observations: a control missing here, a dialog behaving differently there.
//! There was no matrix, and — more to the point — no matrix that was DERIVED
//! rather than hand-kept. `SKILL` §8 is explicit that hand-maintained counts
//! here have drifted into false public claims twice, and §9 rule 1 says a
//! published number is generated from a committed artifact or it is not
//! published. So the matrix is generated, from two inventories:
//!
//! * **Ours** — [`ours`], read live from `webapp/src/host_contract.mjs` on every
//!   run. Nothing about our side is snapshotted, because a snapshot would be the
//!   second hand-kept table that drifts.
//! * **Theirs** — [`surface`], a committed snapshot of identifiers extracted
//!   from their source tree, which lives outside this repository and is not
//!   vendored into it.
//!
//! The human half is [`matrix::Map`]: the claim that one of their controls and
//! one of our commands are the same capability. Everything else — whether each
//! anchor still resolves, whether every row is classified, and the verdict
//! itself — is computed.
//!
//! # What cannot be extracted mechanically
//!
//! Stated here rather than left to be inferred, because understating the
//! method's limits is the same defect as overstating its results:
//!
//! * **Semantic equivalence.** That their `btnDropCap` and our `insert.dropCap`
//!   are the same capability is a human reading. The tool checks both anchors
//!   exist; it cannot check they mean the same thing.
//! * **Depth.** Two products can both have a drop-cap dialog and offer different
//!   fields in it. A `Parity` verdict is about the capability, not about every
//!   option inside it; where depth differs the row is `Partial` and says so.
//! * **Our family members.** `COMMAND_FAMILIES` prefixes such as `table.` and
//!   `style.` are populated at run time from the document, so they cannot be
//!   enumerated statically. A row resting on a family cites the family.
//! * **Their absence.** An absence cannot be cited to a line, so a row claiming
//!   they lack something records the search that establishes it, and a reader
//!   with their tree can re-run it.
//!
//! # Usage
//!
//! ```text
//! cargo run -p opendoc-parity -- check
//! cargo run -p opendoc-parity -- write
//! cargo run -p opendoc-parity -- extract --reference /path/holding/web-apps-and-sdkjs
//! cargo run -p opendoc-parity -- extract --reference … --check
//! ```

pub mod matrix;
pub mod ours;
pub mod surface;

use std::collections::BTreeMap;

/// The document whose generated regions this tool owns.
pub const DOC: &str = "docs/153-ONLYOFFICE-CAPABILITY-PARITY-MATRIX.md";

/// The committed snapshot of their surface.
pub const SURFACE_JSON: &str = "tools/opendoc-parity/data/onlyoffice-surface.json";

/// The human-authored capability map.
pub const MAP_JSON: &str = "tools/opendoc-parity/data/capabilities.json";

/// Splices each region of `regions` into `doc` between its markers.
///
/// A region named `parity-tally` is delimited by `<!-- @generated parity-tally -->`
/// and `<!-- @end parity-tally -->`, the convention `docs/149` already uses. Every
/// region in `regions` must have both markers, and the document must have no
/// marker without a region — an unfilled generated block is a page that looks
/// generated and is not.
///
/// # Errors
///
/// Returns the sentence to print when a marker is missing or unmatched.
pub fn splice(doc: &str, regions: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = doc.to_string();
    for (name, body) in regions {
        let open = format!("<!-- @generated {name} -->");
        let close = format!("<!-- @end {name} -->");
        let start = out
            .find(&open)
            .ok_or_else(|| format!("{DOC} has no `{open}`"))?;
        let end = out
            .find(&close)
            .ok_or_else(|| format!("{DOC} has no `{close}`"))?;
        if end < start {
            return Err(format!("{DOC} closes `{name}` before it opens it"));
        }
        let head = start + open.len();
        out.replace_range(head..end, &format!("\n{body}"));
    }
    for line in out.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("<!-- @generated ") else { continue };
        let name = rest.trim_end_matches("-->").trim();
        if !regions.contains_key(name) {
            return Err(format!(
                "{DOC} declares a generated region `{name}` that this tool does not produce"
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_is_replaced_between_its_markers() {
        let doc = "before\n<!-- @generated a -->\nstale\n<!-- @end a -->\nafter\n";
        let regions: BTreeMap<String, String> =
            [("a".to_string(), "fresh\n".to_string())].into_iter().collect();
        let out = splice(doc, &regions).expect("splice");
        assert_eq!(out, "before\n<!-- @generated a -->\nfresh\n<!-- @end a -->\nafter\n");
    }

    #[test]
    fn a_generated_region_this_tool_does_not_fill_is_refused() {
        let doc = "<!-- @generated a -->\n<!-- @end a -->\n<!-- @generated b -->\n<!-- @end b -->\n";
        let regions: BTreeMap<String, String> =
            [("a".to_string(), "x\n".to_string())].into_iter().collect();
        let err = splice(doc, &regions).expect_err("an unfilled region must fail");
        assert!(err.contains('b'), "{err}");
    }

    #[test]
    fn a_missing_marker_is_refused() {
        let regions: BTreeMap<String, String> =
            [("a".to_string(), "x\n".to_string())].into_iter().collect();
        let err = splice("nothing here\n", &regions).expect_err("a missing marker must fail");
        assert!(err.contains("@generated a"), "{err}");
    }
}
