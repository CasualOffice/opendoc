// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §3, guards G4 and the Rust half of G3: the committed generated
//! files are exactly what the registry produces, the vocabulary version moves
//! with the command list, and the editor-ids-not-exposed list is a ratchet
//! that never overlaps what a command names. The other half of G3 (every id
//! exists in the editor's host contract, and every content-changing contract
//! id is covered) is `webapp/tests/command_registry_parity.test.mjs`, over the
//! `commands.json` this test holds current.

use std::collections::BTreeSet;
use std::path::Path;

use casual_doc_command::COMMANDS;
use casual_doc_command::registry::{UI_NOT_EXPOSED, UI_NOT_EXPOSED_CEILING};
use casual_doc_command::schema::{self, VOCABULARY_COMMANDS, VOCABULARY_VERSION, artifacts};

#[test]
fn the_committed_generated_files_are_what_the_registry_generates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let drift = artifacts::check(&root).expect("the edit schema translates");
    assert!(
        drift.is_empty(),
        "the generated files are stale; run `{}`:\n{}",
        artifacts::REGENERATE,
        drift.join("\n")
    );
}

#[test]
fn every_family_schema_holds_exactly_its_commands() {
    let mut seen = BTreeSet::new();
    for family in schema::families() {
        let ops = schema::ops(&schema::family(family).expect("a family with commands"));
        let expected: Vec<&str> = COMMANDS
            .iter()
            .filter(|command| command.family == family)
            .map(|command| command.id)
            .collect();
        assert_eq!(ops, expected, "family `{}`", family.name());
        seen.extend(ops);
    }
    assert_eq!(
        seen.len(),
        COMMANDS.len(),
        "every command is in one family schema"
    );
}

#[test]
fn adding_a_command_decides_the_vocabulary_version() {
    assert_eq!(
        COMMANDS.len(),
        VOCABULARY_COMMANDS,
        "the registry has {} commands but vocabulary {VOCABULARY_VERSION} names {VOCABULARY_COMMANDS}: \
         bump VOCABULARY_VERSION (minor to add, major to change or remove, docs/181 §4.1) and set \
         VOCABULARY_COMMANDS",
        COMMANDS.len()
    );
}

#[test]
fn the_ui_not_exposed_list_is_a_ratchet_with_reasons_and_never_names_a_twin() {
    assert_eq!(
        UI_NOT_EXPOSED.len(),
        UI_NOT_EXPOSED_CEILING,
        "the editor-ids-not-exposed count moved: lower UI_NOT_EXPOSED_CEILING when a command \
         names an id; raising it needs an ADR line (docs/181 §3)"
    );
    let named: BTreeSet<&str> = COMMANDS
        .iter()
        .flat_map(|command| command.ui_commands.iter().copied())
        .collect();
    let mut seen = BTreeSet::new();
    for (id, reason) in UI_NOT_EXPOSED {
        assert!(seen.insert(*id), "`{id}` is listed twice");
        assert!(reason.len() > 20, "`{id}` needs a reason a user can read");
        assert!(
            !named.contains(id),
            "`{id}` is not exposed and also named by a command's ui_commands; remove it from \
             UI_NOT_EXPOSED and lower the ceiling"
        );
    }
    let mut twins = BTreeSet::new();
    for command in COMMANDS {
        for id in command.ui_commands {
            assert!(
                twins.insert(*id),
                "`{id}` is named by two commands; an editor command has one agent twin"
            );
        }
    }
}
