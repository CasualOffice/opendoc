// SPDX-License-Identifier: Apache-2.0

//! The artifact guard for `docs/153`.
//!
//! Three things are asserted, and each of them has been driven red by mutating
//! the code it protects (the mutations are recorded in the commit that added
//! this file, per `SKILL` §4):
//!
//! 1. **Every citation still resolves.** A row grading a capability against an
//!    anchor that no longer exists — on either side — fails, rather than
//!    silently downgrading the row and publishing a wrong verdict.
//! 2. **The document is not stale.** Regenerating from the two inventories must
//!    reproduce the committed file byte for byte.
//! 3. **The matrix cannot understate us.** A row that claims we lack a
//!    capability names the command ids it asserts are absent, and the guard
//!    fails the moment `COMMAND_CONTRACT` declares one of them. `docs/105`
//!    CQ-005 and CQ-010 are both stale in that direction today; this is the
//!    mechanism that stops `docs/153` joining them.
//!
//! What it does NOT check, stated because a guard cited as evidence for more
//! than it checks is this repository's recurring burn: that a row's two anchors
//! describe the SAME capability. Nothing mechanical reads that. It is a human
//! claim, and the matrix says so.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use opendoc_parity::matrix::{Map, Ours, grade, regions};
use opendoc_parity::ours::read_inventory;
use opendoc_parity::surface::Surface;
use opendoc_parity::{DOC, MAP_JSON, SURFACE_JSON, splice};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels under the repository root")
        .to_path_buf()
}

fn load() -> (PathBuf, Map, Surface) {
    let repo = repo_root();
    let map: Map =
        serde_json::from_str(&fs::read_to_string(repo.join(MAP_JSON)).expect("read the map"))
            .expect("the capability map is valid JSON of the declared shape");
    let surface: Surface = serde_json::from_str(
        &fs::read_to_string(repo.join(SURFACE_JSON)).expect("read the surface snapshot"),
    )
    .expect("the ONLYOFFICE snapshot is valid JSON of the declared shape");
    (repo, map, surface)
}

#[test]
fn every_citation_in_the_matrix_still_resolves_on_both_sides() {
    let (repo, map, surface) = load();
    let inventory = read_inventory(&repo).expect("read COMMAND_CONTRACT");
    let (rows, problems) = grade(&map, &inventory, &surface, &repo);

    assert!(
        problems.is_empty(),
        "{} rows of docs/153 are graded against evidence that no longer holds:\n{}",
        problems.len(),
        problems
            .iter()
            .map(|p| format!("  {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(!rows.is_empty(), "the capability map graded nothing");
}

#[test]
fn the_committed_document_is_what_the_inventories_produce() {
    let (repo, map, surface) = load();
    let inventory = read_inventory(&repo).expect("read COMMAND_CONTRACT");
    let (rows, problems) = grade(&map, &inventory, &surface, &repo);
    assert!(problems.is_empty(), "grade first: {problems:?}");

    // Line endings normalised before comparing. A Windows checkout can present
    // the committed file with CRLF while the generator always writes LF, so a
    // raw string comparison fails on every line for a reason that has nothing to
    // do with the document being stale — which is precisely what it reported on
    // `platform (Windows-x64)` while every POSIX runner was green.
    let committed = fs::read_to_string(repo.join(DOC))
        .expect("read docs/153")
        .replace("\r\n", "\n");
    let fresh = splice(&committed, &regions(&rows, &map, &surface, &inventory))
        .expect("splice the generated regions");
    // Not `assert_eq!`: the document is over a quarter of a megabyte, and a
    // guard whose failure buries the one changed line under both copies of it
    // is a guard people learn to skim. Report the first divergence instead.
    if fresh != committed {
        let at = fresh
            .lines()
            .zip(committed.lines())
            .position(|(a, b)| a != b);
        let detail = at.map_or_else(
            || {
                format!(
                    "the committed file is {} lines and a fresh run is {}",
                    committed.lines().count(),
                    fresh.lines().count()
                )
            },
            |line| {
                format!(
                    "first difference at line {}:\n  committed: {}\n  fresh:     {}",
                    line + 1,
                    committed.lines().nth(line).unwrap_or_default(),
                    fresh.lines().nth(line).unwrap_or_default()
                )
            },
        );
        panic!(
            "docs/153 is stale. Run `cargo run -p opendoc-parity -- write` and commit the \
             result.\n{detail}"
        );
    }
}

#[test]
fn a_row_claiming_we_lack_something_is_re_checked_against_the_contract() {
    // The understating half of `SKILL` §9 rule 6, as a mechanism rather than a
    // habit. `grade` already fails such a row; this test states the rule
    // separately so that removing it from `grade` cannot pass unnoticed, and so
    // the failure names the row a reader has to re-grade.
    let (repo, map, _) = load();
    let inventory = read_inventory(&repo).expect("read COMMAND_CONTRACT");

    let mut shipped: Vec<String> = Vec::new();
    for capability in &map.capabilities {
        for id in absent_ids(&capability.ours) {
            if inventory.has_command(&id) {
                shipped.push(format!("{} asserts `{id}` is absent", capability.id));
            }
        }
    }
    assert!(
        shipped.is_empty(),
        "docs/153 is understating this project: COMMAND_CONTRACT declares commands the matrix \
         grades as gaps. Re-grade these rows:\n{}",
        shipped.join("\n")
    );
}

#[test]
fn the_snapshot_of_their_surface_is_internally_consistent() {
    // A snapshot that lost its sort order would make every `has_*` binary search
    // unreliable, and an unreliable lookup fails rows in the direction that
    // OVERSTATES the gap list. Cheap to assert, and it cannot be satisfied by
    // accident.
    let (_, _, surface) = load();
    let controls: Vec<&str> = surface.controls.iter().map(|c| c.id.as_str()).collect();
    assert!(
        controls.windows(2).all(|w| w[0] <= w[1]),
        "controls are not sorted"
    );
    assert!(
        surface.locale_keys.windows(2).all(|w| w[0] <= w[1]),
        "locale keys are not sorted"
    );
    assert!(
        surface.api_methods.windows(2).all(|w| w[0] <= w[1]),
        "API methods are not sorted"
    );
    assert!(
        !surface.taken_at.is_empty(),
        "the snapshot must record the day it was taken, so a reader can tell how old it is"
    );
}

#[test]
fn every_capability_id_is_unique_and_filed_under_a_declared_area() {
    let (_, map, _) = load();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let areas: BTreeSet<&str> = map.areas.iter().map(|a| a.id.as_str()).collect();
    for capability in &map.capabilities {
        assert!(
            seen.insert(&capability.id),
            "duplicate capability id `{}`",
            capability.id
        );
        assert!(
            areas.contains(capability.area.as_str()),
            "`{}` is filed under undeclared area `{}`",
            capability.id,
            capability.area
        );
    }
    for area in &map.areas {
        assert!(
            map.capabilities.iter().any(|c| c.area == area.id),
            "area `{}` has no capabilities, so it would print as an empty heading",
            area.id
        );
    }
}

/// The command ids an `ours` clause asserts are NOT in the contract, including
/// the ones nested inside a `partial`.
fn absent_ids(ours: &Ours) -> Vec<String> {
    match ours {
        Ours::Absent(ids) => ids.clone(),
        Ours::Partial { evidence, .. } => absent_ids(evidence),
        _ => Vec::new(),
    }
}
