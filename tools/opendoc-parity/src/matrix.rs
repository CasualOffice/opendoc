// SPDX-License-Identifier: Apache-2.0

//! The matrix itself: capabilities, the evidence each side is cited by, and the
//! verdict that falls out of the two.
//!
//! **The verdict is never written down.** Every row states what evidence to look
//! for on each side; the verdict is computed from whether that evidence
//! resolves. That is the whole point of the design. A hand-written verdict
//! column is a number in a document, and numbers in documents here have drifted
//! into false public claims in both directions — overstating (`webapp`'s
//! fidelity page, twice) and understating (`docs/105` CQ-005 says "no i18n seam
//! at all" while nineteen locales ship; CQ-010 says "no embed surface" while
//! `packages/opendoc-embed` exists). A row that says "we do not have this" is
//! therefore expressed as an ASSERTION — `ours.absent` names the command ids
//! that must NOT be in the contract — so the day somebody ships it, the guard
//! goes red and the document cannot keep understating us.
//!
//! What is human here, stated plainly because `SKILL` §9 requires it: the claim
//! that their `btnDropCap` and our `insert.dropCap` are the same capability. No
//! tool can read that. What the tool checks is that both anchors still exist,
//! that every capability is classified, and that no row's verdict contradicts
//! either inventory.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ours::{Inventory, anchor_resolves};
use crate::surface::Surface;

/// A grep anchor into this repository: `literal` must occur in `file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// Path relative to the repository root.
    pub file: String,
    /// A literal that must occur in it.
    pub literal: String,
}

/// How a capability is evidenced on our side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Ours {
    /// An exact row of `COMMAND_CONTRACT`.
    Command(String),
    /// A prefix of `COMMAND_FAMILIES`. Members are generated at run time, so
    /// the family is the strongest static evidence there is.
    Family(String),
    /// A literal in a named file — for capabilities that are not host commands:
    /// importer behaviour, export formats, dialog fields.
    Anchor(Anchor),
    /// We have part of it. The evidence must still resolve, and `missing` says
    /// what is not there in the same sentence a reader would want.
    Partial {
        evidence: Box<Ours>,
        missing: String,
    },
    /// We do not have it, asserted rather than stated: none of these command
    /// ids may appear in `COMMAND_CONTRACT`. The day one does, the guard fails
    /// and this row must be re-graded.
    Absent(Vec<String>),
}

/// How a capability is evidenced on their side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Theirs {
    /// A declared `Common.UI.*` control.
    Control(String),
    /// A locale key — the roster of their context menu and dialog fields, which
    /// are built dynamically and so have no control declaration to cite.
    LocaleKey(String),
    /// A public engine API method: evidence about the ENGINE rather than the
    /// chrome.
    Api(String),
    /// A boot flag from their `Main.js` — the evidence for a capability their
    /// editor does not draw itself but takes from the integrator or from the
    /// handshake, such as the template list or the interface language.
    AppOption(String),
    /// They have it, and a server-issued licence result or the desktop shell
    /// decides whether a browser session sees it. `option` names the
    /// `appOptions` flag, which the extractor records with the gates its
    /// expression mentions.
    Gated {
        evidence: Box<Theirs>,
        option: String,
    },
    /// They do not have it. `docs/153` names the search that establishes that,
    /// because an absence cannot be cited to a line.
    Absent { searched: String },
}

/// One row of the matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capability {
    /// Stable id, `area.slug`. Cited from commits and trackers.
    pub id: String,
    /// The area it is filed under.
    pub area: String,
    /// What a person would call it.
    pub title: String,
    /// Evidence on our side.
    pub ours: Ours,
    /// Evidence on theirs.
    pub theirs: Theirs,
    /// How hard a document user hits it, 1 (constantly) to 5 (specialist).
    /// Required on a gap, permitted elsewhere so a closed gap keeps its history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u8>,
    /// Anything a reader needs that the verdict does not carry.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// A named group of capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Area {
    /// Stable id, used as each capability's `area`.
    pub id: String,
    /// The heading it prints under.
    pub title: String,
}

/// The human-authored half of the matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Map {
    /// Areas, in the order they print.
    pub areas: Vec<Area>,
    /// Capabilities, in any order; they print grouped by area and sorted by id.
    pub capabilities: Vec<Capability>,
}

/// What the two inventories say about one capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// Both ship it.
    Parity,
    /// We ship it; a server licence result or the desktop shell decides whether
    /// their browser session may.
    Ungated,
    /// We ship it; they do not.
    Ahead,
    /// We ship part of it.
    Partial,
    /// They ship it; we do not.
    Gap,
    /// Something outside their editor decides whether a browser session sees
    /// it, and we do not ship it either.
    TheirsGated,
    /// Neither product ships it. Recorded rather than dropped, because the
    /// search that established it is worth keeping: a shared absence is the
    /// answer to "why is this not on the gap list", and deleting the row would
    /// mean somebody repeats the search next quarter.
    Neither,
}

impl Verdict {
    /// The word the matrix prints.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Parity => "Parity",
            Self::Ungated => "Ours, ungated",
            Self::Ahead => "Ours only",
            Self::Partial => "Partial",
            Self::Gap => "Gap",
            Self::TheirsGated => "Theirs, gated",
            Self::Neither => "Neither",
        }
    }

    /// What the word means, printed once in the tally.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Parity => "both products ship it, and both anchors resolve",
            Self::Ungated => {
                "we ship it in the editor itself; theirs is behind a licence result the \
                 document server issues, the desktop shell, or the integrator's own config"
            }
            Self::Ahead => "we ship it and their tree has no such surface",
            Self::Partial => "we ship part of it; the row says what is missing",
            Self::Gap => "their standalone browser session ships it and we do not",
            Self::TheirsGated => {
                "theirs needs a licence result, the desktop shell or the integrator, \
                 and we lack it too"
            }
            Self::Neither => {
                "neither product ships it, and the row records the search that established that"
            }
        }
    }
}

/// A row with its computed verdict.
#[derive(Debug, Clone)]
pub struct Row {
    /// The capability this row grades.
    pub capability: Capability,
    /// What the two inventories say about it.
    pub verdict: Verdict,
}

/// A contradiction between the map and one of the inventories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The capability id, or `""` for a whole-map problem.
    pub capability: String,
    /// What is wrong, in the sentence a reader needs.
    pub detail: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.capability.is_empty() {
            write!(f, "{}", self.detail)
        } else {
            write!(f, "{}: {}", self.capability, self.detail)
        }
    }
}

/// Grades every capability against both inventories.
///
/// Returns the rows and every contradiction found. A contradiction is not a
/// warning: `--check` fails on any, because a matrix that grades a row against
/// evidence that no longer exists is the stale-in-both-directions failure this
/// lane was opened to end.
///
/// Complexity: O(capabilities × log surface) — each anchor is a binary search
/// or one file read, and file reads are cached by the caller through
/// `repo_root` being the same tree.
#[must_use]
pub fn grade(
    map: &Map,
    ours: &Inventory,
    theirs: &Surface,
    repo_root: &Path,
) -> (Vec<Row>, Vec<Problem>) {
    let mut problems = Vec::new();
    let mut rows = Vec::new();

    let areas: BTreeSet<&str> = map.areas.iter().map(|a| a.id.as_str()).collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();

    for capability in &map.capabilities {
        if !seen.insert(capability.id.as_str()) {
            problems.push(Problem {
                capability: capability.id.clone(),
                detail: "declared twice".to_string(),
            });
        }
        if !areas.contains(capability.area.as_str()) {
            problems.push(Problem {
                capability: capability.id.clone(),
                detail: format!("area `{}` is not declared", capability.area),
            });
        }
        if !capability.id.starts_with(&format!("{}.", capability.area)) {
            problems.push(Problem {
                capability: capability.id.clone(),
                detail: format!("id must start with `{}.`", capability.area),
            });
        }

        let have = check_ours(
            &capability.ours,
            ours,
            repo_root,
            &capability.id,
            &mut problems,
        );
        let they = check_theirs(&capability.theirs, theirs, &capability.id, &mut problems);

        let verdict = match (have, they) {
            (Have::Yes, They::Yes) => Verdict::Parity,
            (Have::Yes, They::Gated) => Verdict::Ungated,
            (Have::Yes, They::No) => Verdict::Ahead,
            (Have::Part, _) => Verdict::Partial,
            (Have::No, They::Yes) => Verdict::Gap,
            (Have::No, They::Gated) => Verdict::TheirsGated,
            (Have::No, They::No) => Verdict::Neither,
        };

        if verdict == Verdict::Gap && capability.rank.is_none() {
            problems.push(Problem {
                capability: capability.id.clone(),
                detail: "a gap must be ranked 1-5 by how hard a document user hits it".to_string(),
            });
        }
        if matches!(capability.rank, Some(r) if !(1..=5).contains(&r)) {
            problems.push(Problem {
                capability: capability.id.clone(),
                detail: "rank must be 1-5".to_string(),
            });
        }
        if matches!(capability.theirs, Theirs::Absent { .. }) && capability.note.is_empty() {
            problems.push(Problem {
                capability: capability.id.clone(),
                detail: "an absence on their side needs a note saying what was searched for"
                    .to_string(),
            });
        }

        rows.push(Row {
            capability: capability.clone(),
            verdict,
        });
    }

    rows.sort_by(|a, b| a.capability.id.cmp(&b.capability.id));
    (rows, problems)
}

/// Whether our side has the capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Have {
    Yes,
    Part,
    No,
}

/// Whether their side has it, and whether a free browser session sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum They {
    Yes,
    Gated,
    No,
}

fn check_ours(
    ours: &Ours,
    inventory: &Inventory,
    repo_root: &Path,
    id: &str,
    problems: &mut Vec<Problem>,
) -> Have {
    match ours {
        Ours::Command(command) => {
            if inventory.has_command(command) {
                Have::Yes
            } else {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: format!(
                        "cites command `{command}`, which COMMAND_CONTRACT does not declare"
                    ),
                });
                Have::No
            }
        }
        Ours::Family(prefix) => {
            if inventory.has_family(prefix) {
                Have::Yes
            } else {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: format!(
                        "cites family `{prefix}`, which COMMAND_FAMILIES does not declare"
                    ),
                });
                Have::No
            }
        }
        Ours::Anchor(anchor) => match anchor_resolves(repo_root, &anchor.file, &anchor.literal) {
            Ok(true) => Have::Yes,
            Ok(false) => {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: format!("`{}` no longer contains `{}`", anchor.file, anchor.literal),
                });
                Have::No
            }
            Err(e) => {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: e.to_string(),
                });
                Have::No
            }
        },
        Ours::Partial { evidence, missing } => {
            if missing.is_empty() {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: "a partial row must say what is missing".to_string(),
                });
            }
            match check_ours(evidence, inventory, repo_root, id, problems) {
                Have::No => Have::No,
                _ => Have::Part,
            }
        }
        Ours::Absent(would_be) => {
            if would_be.is_empty() {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: "an absence must name the command ids it asserts are missing"
                        .to_string(),
                });
            }
            let mut shipped = Vec::new();
            for candidate in would_be {
                if inventory.has_command(candidate) {
                    shipped.push(candidate.clone());
                }
            }
            if shipped.is_empty() {
                Have::No
            } else {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: format!(
                        "graded as a gap, but COMMAND_CONTRACT now declares {} — the matrix is \
                         understating us and this row must be re-graded",
                        shipped.join(", ")
                    ),
                });
                Have::Yes
            }
        }
    }
}

fn check_theirs(theirs: &Theirs, surface: &Surface, id: &str, problems: &mut Vec<Problem>) -> They {
    match theirs {
        Theirs::Control(control) => {
            if surface.has_control(control) {
                They::Yes
            } else {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: format!("cites their control `{control}`, which the snapshot has not"),
                });
                They::No
            }
        }
        Theirs::LocaleKey(key) => {
            if surface.has_locale_key(key) {
                They::Yes
            } else {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: format!("cites their locale key `{key}`, which the snapshot has not"),
                });
                They::No
            }
        }
        Theirs::Api(method) => {
            if surface.has_api_method(method) {
                They::Yes
            } else {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: format!("cites their API `{method}`, which the snapshot has not"),
                });
                They::No
            }
        }
        Theirs::AppOption(name) => {
            if surface.app_option(name).is_some() {
                They::Yes
            } else {
                problems.push(Problem {
                    capability: id.to_string(),
                    detail: format!("cites their boot flag `{name}`, which the snapshot has not"),
                });
                They::No
            }
        }
        Theirs::Gated { evidence, option } => {
            let present = check_theirs(evidence, surface, id, problems);
            match surface.app_option(option) {
                None => {
                    problems.push(Problem {
                        capability: id.to_string(),
                        detail: format!(
                            "cites their boot flag `{option}`, which the snapshot has not"
                        ),
                    });
                    present
                }
                Some(flag)
                    if flag.licence
                        || flag.desktop
                        || flag.host
                        || flag.name == "canLicense"
                        || flag.name == "isDesktopApp" =>
                {
                    They::Gated
                }
                Some(_) => {
                    problems.push(Problem {
                        capability: id.to_string(),
                        detail: format!(
                            "grades `{option}` as gated, but its expression mentions none of \
                             `canLicense`, `isDesktopApp`, `editorConfig` or `permissions`, so \
                             nothing outside the editor decides it"
                        ),
                    });
                    present
                }
            }
        }
        Theirs::Absent { .. } => They::No,
    }
}

/// Renders the generated regions of `docs/153`, keyed by region name.
///
/// The caller splices each one between its `<!-- @generated NAME -->` and
/// `<!-- @end NAME -->` markers, the same convention `docs/149` already uses.
#[must_use]
pub fn regions(
    rows: &[Row],
    map: &Map,
    theirs: &Surface,
    ours: &Inventory,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    out.insert("parity-tally".to_string(), tally(rows));
    out.insert("parity-gaps".to_string(), gaps(rows));
    out.insert("parity-matrix".to_string(), full(rows, map));
    out.insert(
        "parity-provenance".to_string(),
        provenance(theirs, ours, rows),
    );
    out
}

fn tally(rows: &[Row]) -> String {
    let order = [
        Verdict::Parity,
        Verdict::Ungated,
        Verdict::Ahead,
        Verdict::Partial,
        Verdict::Gap,
        Verdict::TheirsGated,
        Verdict::Neither,
    ];
    let mut text = String::from("| Verdict | Rows | What it means |\n| --- | --- | --- |\n");
    for verdict in order {
        let count = rows.iter().filter(|r| r.verdict == verdict).count();
        let _ = writeln!(
            text,
            "| {} | {} | {} |",
            verdict.label(),
            count,
            verdict.meaning()
        );
    }
    let _ = writeln!(
        text,
        "| **Total graded** | **{}** | every row below |",
        rows.len()
    );
    text
}

fn gaps(rows: &[Row]) -> String {
    let mut open: Vec<&Row> = rows.iter().filter(|r| r.verdict == Verdict::Gap).collect();
    open.sort_by(|a, b| {
        (a.capability.rank, &a.capability.id).cmp(&(b.capability.rank, &b.capability.id))
    });
    if open.is_empty() {
        return "_No open gaps._\n".to_string();
    }
    let mut text = String::from(
        "| Rank | Capability | Id | Theirs | Note |\n| --- | --- | --- | --- | --- |\n",
    );
    for row in open {
        let _ = writeln!(
            text,
            "| {} | {} | `{}` | {} | {} |",
            row.capability
                .rank
                .map_or_else(|| "—".to_string(), |r| r.to_string()),
            row.capability.title,
            row.capability.id,
            cell(&cite_theirs(&row.capability.theirs)),
            dash(&row.capability.note),
        );
    }
    text
}

fn full(rows: &[Row], map: &Map) -> String {
    let mut text = String::new();
    for area in &map.areas {
        let in_area: Vec<&Row> = rows
            .iter()
            .filter(|r| r.capability.area == area.id)
            .collect();
        if in_area.is_empty() {
            continue;
        }
        let _ = writeln!(text, "### {} ({} rows)\n", area.title, in_area.len());
        text.push_str(
            "| Capability | Verdict | Ours | Theirs | Note |\n| --- | --- | --- | --- | --- |\n",
        );
        for row in in_area {
            let _ = writeln!(
                text,
                "| {} | {} | {} | {} | {} |",
                cell(&row.capability.title),
                row.verdict.label(),
                cell(&cite_ours(&row.capability.ours)),
                cell(&cite_theirs(&row.capability.theirs)),
                dash(&row.capability.note),
            );
        }
        text.push('\n');
    }
    text
}

fn provenance(theirs: &Surface, ours: &Inventory, rows: &[Row]) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "| Side | Source | Inventory |");
    let _ = writeln!(text, "| --- | --- | --- |");
    let _ = writeln!(
        text,
        "| OpenDoc | `webapp/src/host_contract.mjs`, read on every run | {} exact commands, {} families |",
        ours.commands.len(),
        ours.families.len()
    );
    let _ = writeln!(
        text,
        "| ONLYOFFICE | snapshot of {} files, taken {} | {} controls, {} locale keys, {} engine API methods, {} boot flags ({} licence-gated, {} desktop-only) |",
        theirs.files.len(),
        theirs.taken_at,
        theirs.controls.len(),
        theirs.locale_keys.len(),
        theirs.api_methods.len(),
        theirs.app_options.len(),
        theirs.app_options.iter().filter(|o| o.licence).count(),
        theirs.app_options.iter().filter(|o| o.desktop).count(),
    );
    let _ = writeln!(
        text,
        "| Matrix | `tools/opendoc-parity/data/capabilities.json` | {} graded rows |",
        rows.len()
    );
    text
}

fn cite_ours(ours: &Ours) -> String {
    match ours {
        Ours::Command(c) => format!("`{c}`"),
        Ours::Family(f) => format!("family `{f}`"),
        Ours::Anchor(a) => format!("`{}` · `{}`", a.file, a.literal),
        Ours::Partial { evidence, missing } => {
            format!("{} — missing: {missing}", cite_ours(evidence))
        }
        Ours::Absent(ids) => {
            let list = ids
                .iter()
                .map(|i| format!("`{i}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("none ({list} undeclared)")
        }
    }
}

fn cite_theirs(theirs: &Theirs) -> String {
    match theirs {
        Theirs::Control(c) => format!("`{c}`"),
        Theirs::LocaleKey(k) => format!("`{k}`"),
        Theirs::Api(m) => format!("`{m}`"),
        Theirs::AppOption(n) => format!("boot flag `{n}`"),
        Theirs::Gated { evidence, option } => {
            format!("{} behind `{option}`", cite_theirs(evidence))
        }
        // Inside a code span, always. A `searched` clause is a grep line: paths,
        // flags and identifiers, none of it English. Left bare it would feed
        // `documenteditor`, `put_TextPrUnderline` and `asc_docs_api` into
        // `webapp/tools/build-glossary.mjs`, whose whole rule is that code spans
        // are excluded so the spell checker's vocabulary cannot fill up with
        // identifiers — and a document that stales another lane's generated
        // artifact is how `main` goes red (`SKILL` §5a).
        Theirs::Absent { searched } => {
            format!("none (searched `{}`)", searched.replace('`', "'"))
        }
    }
}

/// Any value, made safe to print inside a Markdown table cell.
fn cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A note, made safe to print inside a Markdown table cell.
///
/// A `|` in a note would end the cell and silently shift every column after it,
/// which is a generated document quietly rendering wrong — the failure mode this
/// whole tool exists to prevent. Newlines collapse for the same reason.
fn dash(note: &str) -> String {
    if note.is_empty() {
        return "—".to_string();
    }
    cell(note)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::{AppOption, Control};

    fn surface() -> Surface {
        Surface {
            taken_at: "2026-10-01".to_string(),
            files: Vec::new(),
            controls: vec![Control {
                id: "btnBold".to_string(),
                file: "x.js".to_string(),
                line: 1,
            }],
            locale_keys: Vec::new(),
            api_methods: Vec::new(),
            app_options: vec![AppOption {
                name: "canUseHistory".to_string(),
                file: "Main.js".to_string(),
                line: 1,
                licence: true,
                desktop: false,
                host: false,
            }],
        }
    }

    fn inventory() -> Inventory {
        Inventory {
            commands: ["format.bold".to_string()].into_iter().collect(),
            families: ["table.".to_string()].into_iter().collect(),
        }
    }

    fn map_with(capability: Capability) -> Map {
        Map {
            areas: vec![Area {
                id: "text".to_string(),
                title: "Text".to_string(),
            }],
            capabilities: vec![capability],
        }
    }

    fn row(ours: Ours, theirs: Theirs, rank: Option<u8>) -> Capability {
        Capability {
            id: "text.x".to_string(),
            area: "text".to_string(),
            title: "X".to_string(),
            ours,
            theirs,
            rank,
            note: "searched".to_string(),
        }
    }

    #[test]
    fn both_sides_present_is_parity() {
        let map = map_with(row(
            Ours::Command("format.bold".to_string()),
            Theirs::Control("btnBold".to_string()),
            None,
        ));
        let (rows, problems) = grade(&map, &inventory(), &surface(), Path::new("."));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(rows[0].verdict, Verdict::Parity);
    }

    #[test]
    fn a_licence_gated_control_we_ship_ungated_is_our_advantage() {
        let map = map_with(row(
            Ours::Command("format.bold".to_string()),
            Theirs::Gated {
                evidence: Box::new(Theirs::Control("btnBold".to_string())),
                option: "canUseHistory".to_string(),
            },
            None,
        ));
        let (rows, problems) = grade(&map, &inventory(), &surface(), Path::new("."));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(rows[0].verdict, Verdict::Ungated);
    }

    #[test]
    fn an_absence_we_have_since_shipped_is_reported_rather_than_published() {
        // The understating half of `SKILL` §9 rule 6, as a mechanism: the row
        // claims we lack `format.bold`, the contract declares it, and the matrix
        // refuses to print the claim.
        let map = map_with(row(
            Ours::Absent(vec!["format.bold".to_string()]),
            Theirs::Control("btnBold".to_string()),
            Some(2),
        ));
        let (rows, problems) = grade(&map, &inventory(), &surface(), Path::new("."));
        assert_eq!(rows[0].verdict, Verdict::Parity);
        assert!(
            problems
                .iter()
                .any(|p| p.detail.contains("understating us")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_gap_without_a_rank_is_refused() {
        let map = map_with(row(
            Ours::Absent(vec!["insert.nothing".to_string()]),
            Theirs::Control("btnBold".to_string()),
            None,
        ));
        let (_, problems) = grade(&map, &inventory(), &surface(), Path::new("."));
        assert!(
            problems.iter().any(|p| p.detail.contains("ranked")),
            "{problems:?}"
        );
    }

    #[test]
    fn evidence_that_no_longer_resolves_is_a_problem_not_a_silent_downgrade() {
        let map = map_with(row(
            Ours::Command("format.bold".to_string()),
            Theirs::Control("btnGone".to_string()),
            None,
        ));
        let (_, problems) = grade(&map, &inventory(), &surface(), Path::new("."));
        assert!(
            problems.iter().any(|p| p.detail.contains("btnGone")),
            "{problems:?}"
        );
    }
}
