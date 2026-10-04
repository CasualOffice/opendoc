// SPDX-License-Identifier: Apache-2.0

//! Writes the `.pptx` fixture this crate's guards import, so a human can open
//! the same bytes in PowerPoint and see what the importer is being asked to read.
//!
//! The deck is built in memory by the test suite on every run, so nothing depends
//! on the file this writes — it is for inspection, not for the guards. That is
//! deliberate: a committed binary fixture drifts from the generator that claims
//! to produce it, and `SKILL` §9.1 is the general form of that rule. Generating
//! it on demand keeps one source of truth.
//!
//! The builder is shared with the tests by path rather than copied, for the same
//! reason `casual-doc-ooxml`'s preset derivation lives in the crate rather than
//! only in its generator: two copies of a derivation pass whenever both are wrong
//! the same way.
//!
//! ```sh
//! cargo run -p casual-pres-import --example generate_pptx_fixture -- /tmp/deck.pptx
//! ```

use std::path::PathBuf;

// `allow` rather than `expect`: the perturbation helpers are used by the guards
// and not by this generator, so an `expect` would itself go unfulfilled in the
// test build. The alternative — splitting the builder so each consumer sees only
// what it calls — would put the fixture's XML and its perturbations in two files,
// which is the drift this crate's own `SKILL` §9.1 discipline is against.
#[allow(dead_code, reason = "the generator needs only the happy-path builder")]
#[path = "../src/tests/deck.rs"]
mod deck;

/// Writes the fixture, to the path given as the first argument or to
/// `fixture-deck.pptx` beside the crate.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixture-deck.pptx"),
        PathBuf::from,
    );
    std::fs::write(&destination, deck::deck())?;
    Ok(())
}
