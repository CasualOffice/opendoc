// SPDX-License-Identifier: Apache-2.0

//! JSON Schemas derived from the edit types (`docs/181` §6, G4). Nothing here
//! is hand-written: a field added to an edit type appears in every front door
//! that publishes these.
//!
//! [`artifacts`] turns the same schemas, and the registry, into the committed
//! generated files (`webapp/generated/`): the edit schema, one schema per
//! family, `commands.json`, a TypeScript declaration file and a Markdown
//! vocabulary table. The `opendoc-command-gen` binary writes them, and its
//! `--check` mode (run by `webapp/build.sh`) and this crate's
//! `tests/generated.rs` (run by CI's `test` job) fail when a committed copy is
//! not what the registry produces.

pub mod artifacts;
pub mod typescript;

use schemars::generate::SchemaSettings;
use serde_json::{Value, json};

use crate::edit::Edit;
use crate::registry::{COMMANDS, Command, Family, Trackable};

/// The edit vocabulary's version (`docs/181` §4.1), `major.minor`. Adding a
/// command is a minor bump; changing or removing one is a major bump and
/// needs an ADR. Published in `commands.json`, the generated TypeScript and
/// `list_edit_kinds`.
pub const VOCABULARY_VERSION: &str = "1.1";

/// How many commands [`VOCABULARY_VERSION`] names. A test holds it equal to
/// the registry, so adding a command cannot ship without deciding the version
/// bump that goes with it.
pub const VOCABULARY_COMMANDS: usize = 29;

/// The schema of one edit: a `oneOf` over every command, each carrying its
/// `op` constant, with shared definitions inlined so the result is
/// self-contained (MCP `inputSchema` has no `$ref` resolution guarantee).
/// O(vocabulary).
#[must_use]
pub fn edit() -> Value {
    let settings = SchemaSettings::draft2020_12().with(|settings| {
        settings.inline_subschemas = true;
    });
    let schema = settings.into_generator().into_root_schema_for::<Edit>();
    let mut value = schema.to_value();
    if let Some(object) = value.as_object_mut() {
        object.remove("$schema");
        object.remove("title");
    }
    value
}

/// The `op` names the schema of [`edit`] admits, in schema order.
/// O(vocabulary).
#[must_use]
pub fn ops(schema: &Value) -> Vec<String> {
    schema["oneOf"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|variant| variant["properties"]["op"]["const"].as_str())
        .map(str::to_owned)
        .collect()
}

/// The schema of the single command `id`, if registered. O(vocabulary).
#[must_use]
pub fn command(id: &str) -> Option<Value> {
    edit()["oneOf"]
        .as_array()?
        .iter()
        .find(|variant| variant["properties"]["op"]["const"] == id)
        .cloned()
}

/// The families that have at least one command, in the order their first
/// command is registered. O(commands × families).
#[must_use]
pub fn families() -> Vec<Family> {
    let mut out: Vec<Family> = Vec::new();
    for command in COMMANDS {
        if !out.contains(&command.family) {
            out.push(command.family);
        }
    }
    out
}

/// The family named `name`, if any command belongs to it. O(commands).
#[must_use]
pub fn family_named(name: &str) -> Option<Family> {
    families().into_iter().find(|family| family.name() == name)
}

/// The schema of an edit restricted to the commands `admit` accepts: the same
/// `oneOf` as [`edit`] with the other variants removed. Used for one family's
/// schema and for a session whose toolsets admit only some families.
/// O(vocabulary).
#[must_use]
pub fn edit_where(admit: impl Fn(&Command) -> bool) -> Value {
    let mut schema = edit();
    if let Some(variants) = schema["oneOf"].as_array_mut() {
        variants.retain(|variant| {
            variant["properties"]["op"]["const"]
                .as_str()
                .and_then(crate::find)
                .is_some_and(&admit)
        });
    }
    schema
}

/// The schema of one family's edits, with a description naming it.
/// `None` when no command belongs to `family`. O(vocabulary).
#[must_use]
pub fn family(family: Family) -> Option<Value> {
    if !families().contains(&family) {
        return None;
    }
    let mut schema = edit_where(|command| command.family == family);
    schema["description"] = json!(format!(
        "One edit of the `{}` family of the OpenDoc edit vocabulary {VOCABULARY_VERSION}.",
        family.name()
    ));
    Some(schema)
}

/// Its trackability as a stable word plus the published reason, if any.
#[must_use]
pub const fn trackable_parts(trackable: Trackable) -> (&'static str, Option<&'static str>) {
    match trackable {
        Trackable::Yes => ("yes", None),
        Trackable::No(reason) => ("no", Some(reason)),
        Trackable::NotApplicable => ("not-applicable", None),
    }
}

/// One command as published to every front door (`commands.json`,
/// `list_edit_kinds`, `describe_edit_kind`): everything the registry declares,
/// with the example parsed. O(command).
#[must_use]
pub fn command_json(command: &Command) -> Value {
    let (trackable, reason) = trackable_parts(command.trackable);
    json!({
        "id": command.id,
        "family": command.family.name(),
        "summary": command.summary,
        "trackable": trackable,
        "trackableReason": reason,
        "emits": command.emits,
        "uiCommands": command.ui_commands,
        "example": serde_json::from_str::<Value>(command.example).unwrap_or(Value::Null),
    })
}
