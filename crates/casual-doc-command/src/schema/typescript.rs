// SPDX-License-Identifier: Apache-2.0

//! A JSON Schema → TypeScript type converter for the subset the edit schema
//! uses (`docs/181` §6, owner decision Q5: generated TypeScript types now, no
//! new dependency).
//!
//! Supported: `type` (one name or a list, including `null`), `object` with
//! `properties`/`required`/`additionalProperties: false`, `array` with
//! `items`, `const`, `enum`, `oneOf`/`anyOf`, and the annotation or
//! validation keywords that do not change a TypeScript type (`description`,
//! `format`, `minimum`, `default`, …). **Anything else is an error, never a
//! silent `unknown`:** a schema keyword this converter does not understand
//! fails generation, so a richer edit type cannot quietly publish a looser
//! TypeScript type than its JSON Schema.

use std::fmt::Write as _;

use serde_json::{Map, Value};

/// Keywords that constrain values but have no TypeScript spelling, or only
/// annotate. They are accepted and dropped (descriptions become doc comments).
const NO_TYPE_EFFECT: &[&str] = &[
    "description",
    "title",
    "format",
    "default",
    "examples",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
];

/// The TypeScript type of `schema`, indented for nesting depth `depth`.
///
/// # Errors
///
/// A sentence naming the keyword or shape the converter does not support.
pub fn type_of(schema: &Value, depth: usize) -> Result<String, String> {
    let object = match schema {
        Value::Bool(true) => return Ok("unknown".to_owned()),
        Value::Bool(false) => return Ok("never".to_owned()),
        Value::Object(object) => object,
        other => {
            return Err(format!(
                "a schema must be an object or a boolean, not {other}"
            ));
        }
    };
    for key in object.keys() {
        let known = NO_TYPE_EFFECT.contains(&key.as_str())
            || matches!(
                key.as_str(),
                "type"
                    | "properties"
                    | "required"
                    | "additionalProperties"
                    | "items"
                    | "const"
                    | "enum"
                    | "oneOf"
                    | "anyOf"
            );
        if !known {
            return Err(format!(
                "the JSON Schema keyword `{key}` has no TypeScript translation here; extend \
                 casual_doc_command::schema::typescript before publishing it"
            ));
        }
    }
    if let Some(value) = object.get("const") {
        return Ok(literal(value));
    }
    if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .ok_or_else(|| "`enum` must be an array".to_owned())?;
        return Ok(union(values.iter().map(literal).collect()));
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(variants) = object.get(key) {
            if object.contains_key("properties") || object.contains_key("items") {
                return Err(format!(
                    "`{key}` beside `properties` or `items` is not supported"
                ));
            }
            let variants = variants
                .as_array()
                .ok_or_else(|| format!("`{key}` must be an array"))?;
            let mut parts = Vec::with_capacity(variants.len());
            for variant in variants {
                parts.push(type_of(variant, depth)?);
            }
            return Ok(union(parts));
        }
    }
    let names: Vec<&str> = match object.get("type") {
        None => return Err("a schema without `type`, `const`, `enum`, `oneOf` or `anyOf` has no TypeScript type here".to_owned()),
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names
            .iter()
            .map(|name| name.as_str().ok_or_else(|| "`type` lists strings".to_owned()))
            .collect::<Result<_, _>>()?,
        Some(other) => return Err(format!("`type` must be a string or a list, not {other}")),
    };
    let mut parts = Vec::with_capacity(names.len());
    for name in names {
        parts.push(match name {
            "string" => "string".to_owned(),
            "number" | "integer" => "number".to_owned(),
            "boolean" => "boolean".to_owned(),
            "null" => "null".to_owned(),
            "array" => {
                let items = object
                    .get("items")
                    .ok_or_else(|| "an `array` needs `items`".to_owned())?;
                let item = type_of(items, depth)?;
                if item.contains(' ') || item.contains('\n') {
                    format!("Array<{item}>")
                } else {
                    format!("{item}[]")
                }
            }
            "object" => object_type(object, depth)?,
            other => return Err(format!("the JSON type `{other}` is not supported")),
        });
    }
    Ok(union(parts))
}

/// An object type literal, one documented member per line.
fn object_type(object: &Map<String, Value>, depth: usize) -> Result<String, String> {
    match object.get("additionalProperties") {
        None | Some(Value::Bool(false)) => {}
        Some(_) => {
            return Err("only `additionalProperties: false` (or absent) is supported".to_owned());
        }
    }
    let empty = Map::new();
    let properties = match object.get("properties") {
        None => &empty,
        Some(Value::Object(properties)) => properties,
        Some(_) => return Err("`properties` must be an object".to_owned()),
    };
    let required: Vec<&str> = object
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if properties.is_empty() {
        return Ok("Record<string, never>".to_owned());
    }
    // `op` first, then the schema's order, so a tagged variant reads naturally.
    let mut names: Vec<&String> = properties.keys().collect();
    names.sort_by_key(|name| name.as_str() != "op");
    let indent = "  ".repeat(depth + 1);
    let mut out = String::from("{\n");
    for name in names {
        let property = &properties[name];
        if let Some(text) = property.get("description").and_then(Value::as_str) {
            doc_comment(&mut out, text, &indent);
        }
        let optional = if required.contains(&name.as_str()) {
            ""
        } else {
            "?"
        };
        let _ = writeln!(
            out,
            "{indent}{}{optional}: {};",
            member_name(name),
            type_of(property, depth + 1)?
        );
    }
    out.push_str(&"  ".repeat(depth));
    out.push('}');
    Ok(out)
}

/// A `/** … */` comment at `indent`, one source line per description line.
/// A `*/` inside the text is broken so it cannot end the comment early.
pub fn doc_comment(out: &mut String, text: &str, indent: &str) {
    let text = text.replace("*/", "*\\/");
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() == 1 {
        let _ = writeln!(out, "{indent}/** {} */", lines[0]);
        return;
    }
    let _ = writeln!(out, "{indent}/**");
    for line in lines {
        if line.is_empty() {
            let _ = writeln!(out, "{indent} *");
        } else {
            let _ = writeln!(out, "{indent} * {line}");
        }
    }
    let _ = writeln!(out, "{indent} */");
}

fn member_name(name: &str) -> String {
    let plain = !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain {
        name.to_owned()
    } else {
        serde_json::to_string(name).unwrap_or_default()
    }
}

fn literal(value: &Value) -> String {
    match value {
        Value::Object(_) | Value::Array(_) => "unknown".to_owned(),
        other => other.to_string(),
    }
}

fn union(mut parts: Vec<String>) -> String {
    let mut seen = Vec::with_capacity(parts.len());
    parts.retain(|part| {
        if seen.contains(part) {
            false
        } else {
            seen.push(part.clone());
            true
        }
    });
    if parts.is_empty() {
        return "never".to_owned();
    }
    parts.join(" | ")
}

/// `insert_text` → `InsertText`.
#[must_use]
pub fn pascal(id: &str) -> String {
    id.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::type_of;
    use serde_json::json;

    #[test]
    fn the_supported_subset_translates() {
        let schema = json!({
            "type": "object",
            "properties": {
                "op": { "const": "x", "type": "string" },
                "n": { "type": ["integer", "null"], "minimum": 1 },
                "tags": { "type": "array", "items": { "type": "string" } },
                "side": { "enum": ["before", "after"] },
                "pick": { "anyOf": [{ "type": "boolean" }, { "type": "null" }] }
            },
            "required": ["op", "tags"],
            "additionalProperties": false
        });
        assert_eq!(
            type_of(&schema, 0).expect("supported"),
            "{\n  op: \"x\";\n  n?: number | null;\n  pick?: boolean | null;\n  side?: \"before\" | \"after\";\n  tags: string[];\n}"
        );
    }

    #[test]
    fn an_unknown_keyword_is_an_error_not_unknown() {
        let error = type_of(&json!({ "type": "string", "$ref": "#/x" }), 0).expect_err("refused");
        assert!(error.contains("`$ref`"), "{error}");
        let error = type_of(
            &json!({ "type": "object", "additionalProperties": { "type": "string" } }),
            0,
        )
        .expect_err("refused");
        assert!(error.contains("additionalProperties"), "{error}");
    }
}
