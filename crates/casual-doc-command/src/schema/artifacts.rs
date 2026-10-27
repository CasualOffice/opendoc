// SPDX-License-Identifier: Apache-2.0

//! The committed generated files (`docs/181` §6, G3, G4), produced from the
//! registry and the edit types, never edited by hand.
//!
//! | File (under `webapp/generated/`) | What | Read by |
//! | --- | --- | --- |
//! | `edit-schema.json` | The JSON Schema of one edit, every command | clients that validate up front |
//! | `edit-schema/<family>.json` | One family's edits | `opendoc://schema/edits/{family}` |
//! | `commands.json` | Every command, its trackability, emits, editor twins, example; the editor ids not exposed | `webapp/tests/command_registry_parity.test.mjs` (G3) |
//! | `opendoc-edits.d.ts` | TypeScript types for the edit vocabulary | `@casualoffice/opendoc-ai`, `editor.ai` (`docs/178` M4) |
//! | `edit-vocabulary.md` | The vocabulary table, every count derived | docs (SKILL §8, §9) |
//!
//! [`check`] compares a tree against [`all`] in both directions: a committed
//! file that differs, one that is missing, and a stray file in the generated
//! directories that nothing produces.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::typescript::{doc_comment, pascal, type_of};
use super::{VOCABULARY_VERSION, command_json, edit, families, family, trackable_parts};
use crate::registry::{COMMANDS, Trackable, UI_NOT_EXPOSED, UI_NOT_EXPOSED_CEILING};

/// Where the generated files live, relative to the repository root.
pub const DIRECTORY: &str = "webapp/generated";

/// How to regenerate, quoted in every file that can carry a comment.
pub const REGENERATE: &str = "cargo run -p casual-doc-command --bin opendoc-command-gen";

/// One generated file: its path relative to the repository root, and its
/// exact contents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    /// Relative to the repository root, `/`-separated.
    pub path: String,
    /// The exact bytes to commit.
    pub contents: String,
}

fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

/// Every generated file, in a fixed order.
///
/// # Errors
///
/// When the edit schema uses a JSON Schema keyword the TypeScript converter
/// does not support (see [`super::typescript`]).
pub fn all() -> Result<Vec<Artifact>, String> {
    let mut out = vec![
        Artifact {
            path: format!("{DIRECTORY}/edit-schema.json"),
            contents: pretty(&edit()),
        },
        Artifact {
            path: format!("{DIRECTORY}/commands.json"),
            contents: pretty(&commands()),
        },
        Artifact {
            path: format!("{DIRECTORY}/opendoc-edits.d.ts"),
            contents: typescript()?,
        },
        Artifact {
            path: format!("{DIRECTORY}/edit-vocabulary.md"),
            contents: markdown(),
        },
    ];
    for family_of in families() {
        if let Some(schema) = family(family_of) {
            out.push(Artifact {
                path: format!("{DIRECTORY}/edit-schema/{}.json", family_of.name()),
                contents: pretty(&schema),
            });
        }
    }
    Ok(out)
}

/// `commands.json`: the registry as data, for the editor-side parity guard
/// (G3) and any tool that wants the vocabulary without Rust.
#[must_use]
pub fn commands() -> Value {
    json!({
        "generatedBy": REGENERATE,
        "vocabularyVersion": VOCABULARY_VERSION,
        "families": families()
            .iter()
            .map(|family_of| json!({
                "name": family_of.name(),
                "commands": COMMANDS
                    .iter()
                    .filter(|command| command.family == *family_of)
                    .map(|command| command.id)
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "commands": COMMANDS.iter().map(command_json).collect::<Vec<_>>(),
        "uiNotExposed": UI_NOT_EXPOSED
            .iter()
            .map(|(id, reason)| json!({ "id": id, "reason": reason }))
            .collect::<Vec<_>>(),
        "uiNotExposedCeiling": UI_NOT_EXPOSED_CEILING,
    })
}

/// `opendoc-edits.d.ts`: one interface per command, a union per family and
/// the `Edit` union, all from the generated schema.
///
/// # Errors
///
/// As [`all`].
pub fn typescript() -> Result<String, String> {
    let schema = edit();
    let variants = schema["oneOf"]
        .as_array()
        .ok_or_else(|| "the edit schema has no `oneOf`".to_owned())?;
    let mut out = String::new();
    let _ = writeln!(out, "// SPDX-License-Identifier: Apache-2.0");
    let _ = writeln!(
        out,
        "// GENERATED from the capability registry (crates/casual-doc-command, docs/181 §6). Do not edit."
    );
    let _ = writeln!(out, "// Regenerate: {REGENERATE}");
    out.push('\n');
    doc_comment(
        &mut out,
        "The edit vocabulary version (docs/181 §4.1): a minor bump adds commands; a major bump changes or removes one.",
        "",
    );
    let _ = writeln!(
        out,
        "export declare const VOCABULARY_VERSION: \"{VOCABULARY_VERSION}\";"
    );
    out.push('\n');
    let mut names = Vec::with_capacity(variants.len());
    for variant in variants {
        let id = variant["properties"]["op"]["const"]
            .as_str()
            .ok_or_else(|| "an edit variant has no `op` constant".to_owned())?;
        let command = crate::find(id)
            .ok_or_else(|| format!("the edit schema names `{id}`, which is not registered"))?;
        let name = format!("{}Edit", pascal(id));
        let mut doc = command.summary.to_owned();
        let (trackable, reason) = trackable_parts(command.trackable);
        let _ = write!(
            doc,
            "\n\nFamily `{}`; trackable: {trackable}.",
            command.family.name()
        );
        if let Some(reason) = reason {
            let _ = write!(doc, " {reason}");
        }
        let _ = write!(doc, "\n\n@example {}", command.example);
        doc_comment(&mut out, &doc, "");
        let _ = writeln!(out, "export interface {name} {}", type_of(variant, 0)?);
        out.push('\n');
        names.push((command.family, id, name));
    }
    doc_comment(&mut out, "Every `op` the vocabulary admits.", "");
    let _ = writeln!(
        out,
        "export type EditOp =\n{};",
        names
            .iter()
            .map(|(_, id, _)| format!("  | \"{id}\""))
            .collect::<Vec<_>>()
            .join("\n")
    );
    out.push('\n');
    doc_comment(
        &mut out,
        "One edit, as an edit plan's `edits[]` carries it.",
        "",
    );
    let _ = writeln!(
        out,
        "export type Edit =\n{};",
        names
            .iter()
            .map(|(_, _, name)| format!("  | {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    out.push('\n');
    doc_comment(&mut out, "The toolset families (docs/181 §5.2).", "");
    let _ = writeln!(
        out,
        "export type EditFamily = {};",
        families()
            .iter()
            .map(|family_of| format!("\"{}\"", family_of.name()))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    out.push('\n');
    doc_comment(&mut out, "The edits of each family.", "");
    let _ = writeln!(out, "export interface EditsByFamily {{");
    for family_of in families() {
        let members: Vec<&str> = names
            .iter()
            .filter(|(owner, _, _)| *owner == family_of)
            .map(|(_, _, name)| name.as_str())
            .collect();
        let _ = writeln!(out, "  {}: {};", family_of.name(), members.join(" | "));
    }
    let _ = writeln!(out, "}}");
    Ok(out)
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// `edit-vocabulary.md`: the table `docs/181` §6 asks for, with every count
/// derived from the registry (SKILL §8, §9).
#[must_use]
pub fn markdown() -> String {
    let total = COMMANDS.len();
    let count = |want: fn(Trackable) -> bool| {
        COMMANDS
            .iter()
            .filter(|command| want(command.trackable))
            .count()
    };
    let trackable = count(|t| matches!(t, Trackable::Yes));
    let refused = count(|t| matches!(t, Trackable::No(_)));
    let not_applicable = count(|t| matches!(t, Trackable::NotApplicable));
    let twins: usize = COMMANDS
        .iter()
        .map(|command| command.ui_commands.len())
        .sum();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<!-- GENERATED by `{REGENERATE}` from crates/casual-doc-command. Do not edit; `webapp/build.sh` and `cargo test -p casual-doc-command` fail when it is stale. -->"
    );
    out.push('\n');
    let _ = writeln!(
        out,
        "# The OpenDoc edit vocabulary, version {VOCABULARY_VERSION}"
    );
    out.push('\n');
    let _ = writeln!(
        out,
        "{total} commands in {} families. {trackable} can be tracked suggestions, {refused} {} refused in suggest mode with the published reason, and {not_applicable} {} not content edits (comments and review decisions). They are the agent-side twins of {twins} editor command ids; {} editor ids that can change a document are not exposed to agents, each with a reason (`commands.json` `uiNotExposed`).",
        families().len(),
        if refused == 1 { "is" } else { "are" },
        if not_applicable == 1 { "is" } else { "are" },
        UI_NOT_EXPOSED.len(),
    );
    out.push('\n');
    let _ = writeln!(
        out,
        "| `op` | Family | Summary | Trackable | Editor twins |"
    );
    let _ = writeln!(out, "| --- | --- | --- | --- | --- |");
    for command in COMMANDS {
        let (word, reason) = trackable_parts(command.trackable);
        let trackable =
            reason.map_or_else(|| word.to_owned(), |reason| format!("{word}: {reason}"));
        let twins = if command.ui_commands.is_empty() {
            "none (agent-only)".to_owned()
        } else {
            command
                .ui_commands
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} |",
            command.id,
            command.family.name(),
            cell(command.summary),
            cell(&trackable),
            twins
        );
    }
    out
}

/// The drift between the tree at `root` and [`all`]: one sentence per file
/// that differs, is missing, or sits in a generated directory unproduced.
/// Empty when the tree is current.
///
/// # Errors
///
/// As [`all`].
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let artifacts = all()?;
    let mut drift = Vec::new();
    for artifact in &artifacts {
        let path = root.join(&artifact.path);
        match std::fs::read_to_string(&path) {
            Ok(committed) if committed == artifact.contents => {}
            Ok(_) => drift.push(format!(
                "{} differs from what the registry generates",
                artifact.path
            )),
            Err(_) => drift.push(format!("{} is missing", artifact.path)),
        }
    }
    for directory in [DIRECTORY.to_owned(), format!("{DIRECTORY}/edit-schema")] {
        let Ok(entries) = std::fs::read_dir(root.join(&directory)) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        found.sort();
        for path in found {
            if path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let relative = format!("{directory}/{name}");
            if !artifacts.iter().any(|artifact| artifact.path == relative) {
                drift.push(format!(
                    "{relative} is not generated by anything; delete it"
                ));
            }
        }
    }
    Ok(drift)
}
