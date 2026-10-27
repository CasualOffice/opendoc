// SPDX-License-Identifier: Apache-2.0

//! JSON Schemas derived from the edit types (`docs/181` §6, G4). Nothing here
//! is hand-written: a field added to an edit type appears in every front door
//! that publishes these.

use schemars::generate::SchemaSettings;
use serde_json::Value;

use crate::edit::Edit;

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
