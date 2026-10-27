// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §3, guards G1 and G6: every operation is classified for agents,
//! the classification agrees with what the registry's commands emit, the
//! not-exposed list only shrinks, and this crate never applies anything.

use std::collections::BTreeSet;
use std::path::Path;

use casual_doc_command::coverage::{NOT_EXPOSED_CEILING, OPERATIONS, Reach, reach};
use casual_doc_command::registry::TRACKED_LOWERING;
use casual_doc_command::{COMMANDS, Trackable};

#[test]
fn every_operation_marked_reached_is_emitted_by_a_command_and_no_command_emits_anything_else() {
    let emitted: BTreeSet<&str> = COMMANDS
        .iter()
        .flat_map(|c| c.emits.iter().copied())
        .collect();
    for (name, reach_of) in OPERATIONS {
        if *reach_of == Reach::Agent {
            assert!(
                emitted.contains(name),
                "`{name}` is marked reached by agents, but no registry command emits it"
            );
        }
    }
    for command in COMMANDS {
        for name in command.emits {
            assert_eq!(
                reach(name),
                Some(Reach::Agent),
                "`{}` declares `{name}`, which coverage does not mark as reached (or is no operation)",
                command.id
            );
        }
    }
}

#[test]
fn the_not_exposed_list_is_a_ratchet() {
    let not_exposed = OPERATIONS
        .iter()
        .filter(|(_, reach_of)| matches!(reach_of, Reach::NotExposed(_)))
        .count();
    assert_eq!(
        not_exposed, NOT_EXPOSED_CEILING,
        "the not-exposed count moved: lower NOT_EXPOSED_CEILING when a family joins the \
         registry; raising it needs an ADR line (docs/181 §3)"
    );
    for (name, reach_of) in OPERATIONS {
        if let Reach::NotExposed(reason) = reach_of {
            assert!(reason.len() > 20, "`{name}` needs a reason a user can read");
        }
    }
}

#[test]
fn commands_are_unique_and_a_trackable_one_may_emit_the_review_lowering() {
    let mut seen = BTreeSet::new();
    for command in COMMANDS {
        assert!(
            seen.insert(command.id),
            "`{}` is registered twice",
            command.id
        );
        assert!(
            command
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_'),
            "`{}` is not snake_case",
            command.id
        );
        assert!(
            command
                .example
                .contains(&format!("\"op\":\"{}\"", command.id)),
            "`{}`'s example names another op",
            command.id
        );
        if command.trackable == Trackable::Yes {
            for name in TRACKED_LOWERING {
                assert!(
                    command.emits.contains(&name),
                    "`{}` is trackable, so the review lowering may emit `{name}` for it",
                    command.id
                );
            }
        }
    }
}

#[test]
fn the_registry_applies_nothing_itself() {
    // Recursive: `src/schema/` and `src/bin/` are sources too.
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("sources") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    assert!(files.len() > 5, "the scan found the sources");
    for path in files {
        let text = std::fs::read_to_string(&path).expect("readable");
        for needle in ["Transaction::", "casual_doc_edit::apply(", "apply_group("] {
            assert!(
                !text.contains(needle),
                "{} names `{needle}`: a lowering returns operations, a caller's choke point applies them",
                path.display()
            );
        }
    }
}

#[test]
fn the_edit_types_and_the_registry_name_the_same_commands_in_the_same_order() {
    let schema = casual_doc_command::schema::edit();
    let ops = casual_doc_command::schema::ops(&schema);
    let ids: Vec<String> = COMMANDS.iter().map(|c| c.id.to_owned()).collect();
    assert_eq!(
        ops, ids,
        "an Edit variant without a registry entry, or the reverse (docs/181 §2.1)"
    );
    for command in COMMANDS {
        let edit: casual_doc_command::edit::Edit =
            serde_json::from_str(command.example).expect("each example is a valid edit");
        assert_eq!(
            edit.kind(),
            command.id,
            "{}'s example parses as itself",
            command.id
        );
        let one = casual_doc_command::schema::command(command.id).expect("its schema");
        assert_eq!(one["properties"]["op"]["const"], command.id);
    }
}
