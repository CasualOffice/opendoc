// SPDX-License-Identifier: Apache-2.0

//! The ONLYOFFICE side of the matrix: a snapshot of THEIR surface, extracted
//! from their source rather than typed by hand.
//!
//! Their checkouts live outside this repository and their code is AGPL-3.0, so
//! nothing of theirs is vendored here. What is committed is a snapshot of
//! **identifiers** — control names, locale keys, API method names, boot-option
//! flags — each with the file and line it was read from. Identifiers are facts
//! about a competitor's surface, not their expression: no English label, no
//! source line and no translated string is copied.
//!
//! The snapshot is produced by `opendoc-parity extract --reference <dir>` and
//! re-verified by `opendoc-parity extract --reference <dir> --check`, which
//! re-runs the extraction and fails when the committed snapshot is not what
//! their current tree yields. So the matrix cannot silently drift from their
//! side either — but only on a machine that has their tree, which is stated
//! plainly in `docs/153` rather than implied to be a CI gate.
//!
//! Complexity: every extractor below is a single linear pass over each file it
//! reads, so the whole extraction is O(bytes of their tree).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A file the snapshot was read from, with enough shape to notice it moved.
///
/// `bytes` and `lines` are not a cryptographic digest and are not claimed to
/// be one: the real drift check is `--check`, which re-extracts everything and
/// compares the whole snapshot, so a changed file shows up as changed rows
/// rather than as a changed hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFile {
    /// Path relative to the reference root (`web-apps/...` or `sdkjs/...`).
    pub path: String,
    /// Size in bytes.
    pub bytes: u64,
    /// Line count.
    pub lines: u32,
}

/// One declared UI control on their side (`this.btnBold = new Common.UI.Button`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Control {
    /// The member name, e.g. `btnBold`. Unique within a file, not globally.
    pub id: String,
    /// Path relative to the reference root.
    pub file: String,
    /// 1-based line of the declaration.
    pub line: u32,
}

/// One `appOptions` flag from their boot controller, with the gates its
/// right-hand side mentions.
///
/// This is how the fair-comparison exclusions stop being hand-typed: a control
/// whose flag mentions `canLicense` is not a web-parity gap, it is a paywall,
/// and `docs/130` had to write that table out by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppOption {
    /// The flag name without the `appOptions.` prefix, e.g. `canUseHistory`.
    pub name: String,
    /// Path relative to the reference root.
    pub file: String,
    /// 1-based line of the assignment.
    pub line: u32,
    /// The assignment mentions `canLicense` — a paid tier decides it.
    pub licence: bool,
    /// The assignment mentions `isDesktopApp` — the browser never sees it.
    pub desktop: bool,
}

/// Everything extracted from their tree, in one committed artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Surface {
    /// The day the snapshot was taken, `YYYY-MM-DD`.
    pub taken_at: String,
    /// The files every row below was read from.
    pub files: Vec<SourceFile>,
    /// Declared `Common.UI.*` controls across their document editor and shell.
    pub controls: Vec<Control>,
    /// Locale KEY names (never values) under the view classes that are surfaces.
    pub locale_keys: Vec<String>,
    /// `Type.prototype.Method` names from the engine's public APIs.
    pub api_methods: Vec<String>,
    /// Boot flags, with the gates their expressions mention.
    pub app_options: Vec<AppOption>,
}

impl Surface {
    /// True when `id` is a control they declare somewhere.
    ///
    /// Cost: O(log n) — `controls` is sorted by the extractor and this is a
    /// binary search over it.
    #[must_use]
    pub fn has_control(&self, id: &str) -> bool {
        self.controls
            .binary_search_by(|c| c.id.as_str().cmp(id))
            .is_ok()
    }

    /// True when `key` is a locale key they declare.
    #[must_use]
    pub fn has_locale_key(&self, key: &str) -> bool {
        self.locale_keys
            .binary_search_by(|k| k.as_str().cmp(key))
            .is_ok()
    }

    /// True when `name` is a public engine API method they declare.
    #[must_use]
    pub fn has_api_method(&self, name: &str) -> bool {
        self.api_methods
            .binary_search_by(|m| m.as_str().cmp(name))
            .is_ok()
    }

    /// The boot flag called `name`, if they declare one.
    #[must_use]
    pub fn app_option(&self, name: &str) -> Option<&AppOption> {
        self.app_options.iter().find(|o| o.name == name)
    }
}

/// The view and controller trees the control extractor walks, relative to the
/// reference root. Listed rather than globbed so that the snapshot's coverage
/// is a stated decision a reader can disagree with.
const CONTROL_ROOTS: &[&str] = &[
    "web-apps/apps/documenteditor/main/app/view",
    "web-apps/apps/documenteditor/main/app/controller",
    "web-apps/apps/common/main/lib/view",
    "web-apps/apps/common/main/lib/controller",
];

/// Their document editor's English catalogue — the key names are the roster of
/// every context-menu row and dialog field, which are built dynamically and so
/// are invisible to the control extractor.
const LOCALE_FILE: &str = "web-apps/apps/documenteditor/main/locale/en.json";

/// The boot controller that decides what a session may see.
const MAIN_CONTROLLER: &str = "web-apps/apps/documenteditor/main/app/controller/Main.js";

/// Their engine's two public APIs: the editor command surface and the document
/// builder. Together they are the best mechanical evidence of what the ENGINE
/// can do, as opposed to what the chrome exposes.
const API_FILES: &[&str] = &["sdkjs/word/api.js", "sdkjs/word/apiBuilder.js"];

/// Locale-key prefixes kept in the snapshot. Everything else in their catalogue
/// is error text, unit names and format samples — not a surface.
const LOCALE_PREFIXES: &[&str] = &[
    "DE.Views.",
    "Common.Views.",
    "Common.Controllers.Shortcuts.",
    "DE.Controllers.Toolbar.",
];

/// What went wrong while reading their tree.
#[derive(Debug)]
pub enum ExtractError {
    /// A path the extractor requires is not under the reference root.
    Missing(String),
    /// The filesystem refused.
    Io(String),
    /// Their locale catalogue did not parse as a JSON object.
    Locale(String),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(p) => write!(f, "not found under the reference root: {p}"),
            Self::Io(e) => write!(f, "reading the reference tree: {e}"),
            Self::Locale(e) => write!(f, "parsing their locale catalogue: {e}"),
        }
    }
}

impl std::error::Error for ExtractError {}

/// Reads their tree at `root` and returns the snapshot it yields.
///
/// `root` is the directory that holds `web-apps/` and `sdkjs/` side by side.
/// `taken_at` is carried through verbatim so a re-extraction for `--check` can
/// reproduce the committed artifact byte for byte.
///
/// # Errors
///
/// Returns [`ExtractError`] when a required path is absent, the filesystem
/// refuses, or their locale catalogue does not parse.
pub fn extract(root: &Path, taken_at: &str) -> Result<Surface, ExtractError> {
    let mut files = Vec::new();
    let mut controls = Vec::new();

    for dir in CONTROL_ROOTS {
        let base = root.join(dir);
        if !base.is_dir() {
            return Err(ExtractError::Missing((*dir).to_string()));
        }
        for path in js_files(&base)? {
            let rel = relative(root, &path);
            let text = read(&path)?;
            files.push(SourceFile {
                path: rel.clone(),
                bytes: text.len() as u64,
                lines: text.lines().count() as u32,
            });
            controls.extend(controls_in(&text, &rel));
        }
    }

    let locale_path = root.join(LOCALE_FILE);
    let locale_text = read(&locale_path)?;
    files.push(SourceFile {
        path: LOCALE_FILE.to_string(),
        bytes: locale_text.len() as u64,
        lines: locale_text.lines().count() as u32,
    });
    let locale_keys = locale_keys_in(&locale_text)?;

    let main_path = root.join(MAIN_CONTROLLER);
    let main_text = read(&main_path)?;
    let app_options = app_options_in(&main_text, MAIN_CONTROLLER);

    let mut api_methods = BTreeSet::new();
    for rel in API_FILES {
        let path = root.join(rel);
        let text = read(&path)?;
        files.push(SourceFile {
            path: (*rel).to_string(),
            bytes: text.len() as u64,
            lines: text.lines().count() as u32,
        });
        api_methods.extend(api_methods_in(&text));
    }

    controls.sort_by(|a, b| (&a.id, &a.file, a.line).cmp(&(&b.id, &b.file, b.line)));
    controls.dedup();
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(Surface {
        taken_at: taken_at.to_string(),
        files,
        controls,
        locale_keys,
        api_methods: api_methods.into_iter().collect(),
        app_options,
    })
}

fn read(path: &Path) -> Result<String, ExtractError> {
    fs::read_to_string(path).map_err(|e| ExtractError::Io(format!("{}: {e}", path.display())))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn js_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, ExtractError> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries =
            fs::read_dir(&d).map_err(|e| ExtractError::Io(format!("{}: {e}", d.display())))?;
        for entry in entries {
            let entry = entry.map_err(|e| ExtractError::Io(e.to_string()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "js") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `this.btnBold = new Common.UI.Button({` and the `me.` spelling of the same.
///
/// Their views declare every interactive control this way, which is why the
/// pattern is worth pinning: a control that is not declared like this is not in
/// the snapshot, and `docs/153` says so rather than implying the roster is
/// complete.
fn controls_in(text: &str, rel: &str) -> Vec<Control> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some(rest) = after_any(line, &["this.", "me."]) else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.len() < 4 {
            continue;
        }
        let prefix_ok = ["btn", "mni", "chk", "cmb"]
            .iter()
            .any(|p| name.starts_with(p));
        if !prefix_ok {
            continue;
        }
        let tail = &rest[name.len()..];
        let tail = tail.trim_start();
        if !tail.starts_with('=') {
            continue;
        }
        if !tail.contains("new Common.UI.") {
            continue;
        }
        out.push(Control {
            id: name,
            file: rel.to_string(),
            line: index as u32 + 1,
        });
    }
    out
}

/// The first position after any of `needles`, searched left to right.
fn after_any<'a>(line: &'a str, needles: &[&str]) -> Option<&'a str> {
    let mut best: Option<usize> = None;
    for needle in needles {
        if let Some(at) = line.find(needle) {
            let end = at + needle.len();
            best = Some(best.map_or(end, |b: usize| b.min(end)));
        }
    }
    best.map(|at| &line[at..])
}

fn locale_keys_in(text: &str) -> Result<Vec<String>, ExtractError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| ExtractError::Locale(e.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| ExtractError::Locale("top level is not an object".to_string()))?;
    let mut keys: Vec<String> = object
        .keys()
        .filter(|k| LOCALE_PREFIXES.iter().any(|p| k.starts_with(p)))
        .cloned()
        .collect();
    keys.sort();
    Ok(keys)
}

/// `this.appOptions.canUseHistory = this.appOptions.canLicense && …`
fn app_options_in(text: &str, rel: &str) -> Vec<AppOption> {
    let mut out: Vec<AppOption> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some(rest) = line.split_once("appOptions.") else {
            continue;
        };
        let rest = rest.1;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let tail = rest[name.len()..].trim_start();
        if !tail.starts_with('=') || tail.starts_with("==") {
            continue;
        }
        if out.iter().any(|o| o.name == name) {
            continue;
        }
        out.push(AppOption {
            name,
            file: rel.to_string(),
            line: index as u32 + 1,
            licence: tail.contains("canLicense"),
            desktop: tail.contains("isDesktopApp"),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `asc_docs_api.prototype.X` and `ApiParagraph.prototype.X`, normalised to
/// `Type.Method` so a capability row can name one without an underscore soup.
fn api_methods_in(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some((head, tail)) = trimmed.split_once(".prototype.") else {
            continue;
        };
        if head.is_empty() || !head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        if !(head.starts_with("Api") || head == "asc_docs_api") {
            continue;
        }
        let method: String = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if method.is_empty() {
            continue;
        }
        out.insert(format!("{head}.{method}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_control_declaration_is_recognised_in_both_spellings() {
        let text = "                    this.btnBold = new Common.UI.Button({\n\
                    \x20                   me.cmbFontName = new Common.UI.ComboBox({\n";
        let found = controls_in(text, "x.js");
        assert_eq!(
            found.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["btnBold", "cmbFontName"]
        );
        assert_eq!(found[0].line, 1);
        assert_eq!(found[1].line, 2);
    }

    #[test]
    fn a_read_of_a_control_is_not_a_declaration() {
        // `this.btnBold.setDisabled(...)` must not be counted: only assignments
        // of a fresh `Common.UI.*` are declarations.
        let text = "this.btnBold.setDisabled(true);\nif (this.btnBold == null) {}\n";
        assert!(controls_in(text, "x.js").is_empty());
    }

    #[test]
    fn an_app_option_records_the_gates_its_expression_mentions() {
        let text = "this.appOptions.canUseHistory = this.appOptions.canLicense && x;\n\
                    this.appOptions.canQuickPrint = this.appOptions.isDesktopApp;\n\
                    if (this.appOptions.canUseHistory == true) {}\n";
        let found = app_options_in(text, "Main.js");
        assert_eq!(found.len(), 2);
        let history = found
            .iter()
            .find(|o| o.name == "canUseHistory")
            .expect("history flag");
        assert!(history.licence);
        assert!(!history.desktop);
        let quick = found
            .iter()
            .find(|o| o.name == "canQuickPrint")
            .expect("quick print flag");
        assert!(quick.desktop);
    }

    #[test]
    fn api_methods_keep_only_the_two_public_shapes() {
        let text = "asc_docs_api.prototype.asc_AddMath = function() {};\n\
                    ApiDocument.prototype.InsertWatermark = function() {};\n\
                    CDocument.prototype.Internal_Thing = function() {};\n";
        let found = api_methods_in(text);
        assert!(found.contains("asc_docs_api.asc_AddMath"));
        assert!(found.contains("ApiDocument.InsertWatermark"));
        assert!(!found.iter().any(|m| m.starts_with("CDocument")));
    }
}
