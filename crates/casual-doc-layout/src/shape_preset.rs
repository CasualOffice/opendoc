// SPDX-License-Identifier: Apache-2.0

//! The DrawingML preset shape geometries (`a:prstGeom@prst`), resolved from the
//! committed ECMA-376 table rather than hand-written per shape.
//!
//! # What this replaces
//!
//! `ShapeGeometry` types 22 of the 187 presets, each with its own hand-coded vertex
//! list, and everything else painted as its bounding rectangle — so a Word arrow,
//! callout, banner or flowchart symbol drew as a box. `119` §6 named the way out: a
//! path is the primitive and a preset is a **recipe**, read as data and evaluated by
//! one interpreter, so the 165th preset costs no more code than the 23rd.
//!
//! # Where the data comes from
//!
//! `shape_preset_table.txt`, generated from `fixtures/spec/presetShapeDefinitions.xml`
//! by `casual-doc-ooxml`'s `generate_preset_table` example. That input is ECMA-376's
//! own normative preset data, vendored from Apache POI's Apache-2.0 redistribution;
//! `fixtures/spec/README.md` records the provenance and why ONLYOFFICE's AGPL
//! transcription of the same presets is deliberately not the source.
//!
//! # How much of it resolves today
//!
//! 124 of the 187 presets use only `moveTo`/`lnTo`/`cubicBezTo`/`quadBezTo`/`close`,
//! which the path primitive already paints. The other 63 need `a:arcTo` as well, and
//! **zero** need anything beyond that — so a preset carrying an arc is refused here and
//! keeps today's bounding-rectangle behaviour until `arcTo` lands. Refusing is the
//! point: a preset drawn with its arcs silently dropped is a different shape, and it
//! would look deliberate.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use casual_doc_model::v1::ShapeAdjustment;

use crate::display::PathCommand;
use crate::shape_guide::{GuideBox, Resolved};
use crate::units::{Point, Rect, Twip, twip_rounded};

/// The committed table, derived from the vendored specification data.
const TABLE: &str = include_str!("shape_preset_table.txt");

/// One path command of a preset definition, with coordinates still as authored
/// expression tokens.
#[derive(Clone, Copy, Debug)]
enum PresetCommand<'a> {
    Move([&'a str; 2]),
    Line([&'a str; 2]),
    Cubic([&'a str; 6]),
    Quad([&'a str; 4]),
    /// `a:arcTo`, carried so the table is complete, but not yet drawable.
    Arc,
    Close,
}

/// One `a:path` of a preset definition.
#[derive(Clone, Debug, Default)]
struct PresetPath<'a> {
    /// `a:path@w`/`@h`: the path's own coordinate space, or `None` when absent, in
    /// which case coordinates are already in the shape's box units.
    width: Option<f64>,
    height: Option<f64>,
    commands: Vec<PresetCommand<'a>>,
}

/// One preset's recipe: its adjust defaults, its computed guides, and its paths.
#[derive(Clone, Debug, Default)]
struct PresetDefinition<'a> {
    /// `a:avLst` defaults, in order. An authored `a:avLst` overrides by name.
    adjust: Vec<(&'a str, &'a str)>,
    /// `a:gdLst`, in order. Order is load-bearing: a guide may name an earlier one.
    guides: Vec<(&'a str, &'a str)>,
    paths: Vec<PresetPath<'a>>,
}

/// The parsed table, built once.
fn table() -> &'static BTreeMap<&'static str, PresetDefinition<'static>> {
    static TABLE_ONCE: OnceLock<BTreeMap<&'static str, PresetDefinition<'static>>> =
        OnceLock::new();
    TABLE_ONCE.get_or_init(parse_table)
}

/// Parses the committed table.
///
/// Lazily, once per process, and only when a document actually reaches a preset — a
/// document with no shapes never pays for it. Complexity: O(n) in the table, which is
/// a fixed 108 KB and independent of document size.
fn parse_table() -> BTreeMap<&'static str, PresetDefinition<'static>> {
    let mut presets: BTreeMap<&'static str, PresetDefinition<'static>> = BTreeMap::new();
    let mut token: Option<&'static str> = None;
    let mut current = PresetDefinition::default();
    for line in TABLE.lines() {
        let mut fields = line.split('\t');
        let Some(kind) = fields.next() else { continue };
        match kind {
            "P" => {
                if let Some(previous) = token.take() {
                    presets.insert(previous, core::mem::take(&mut current));
                }
                token = fields.next();
            }
            "A" | "G" => {
                if let (Some(name), Some(formula)) = (fields.next(), fields.next()) {
                    if kind == "A" {
                        current.adjust.push((name, formula));
                    } else {
                        current.guides.push((name, formula));
                    }
                }
            }
            "H" => {
                let width = fields.next().and_then(|value| value.parse().ok());
                let height = fields.next().and_then(|value| value.parse().ok());
                current.paths.push(PresetPath {
                    width,
                    height,
                    commands: Vec::new(),
                });
            }
            "M" | "L" | "C" | "Q" | "R" | "Z" => {
                // A command before any `H` belongs to an implicit path, which the
                // generator does not emit; guard rather than index out of bounds.
                let Some(path) = current.paths.last_mut() else {
                    continue;
                };
                let rest: Vec<&str> = fields.collect();
                let command = match (kind, rest.len()) {
                    ("M", 2) => PresetCommand::Move([rest[0], rest[1]]),
                    ("L", 2) => PresetCommand::Line([rest[0], rest[1]]),
                    ("C", 6) => {
                        PresetCommand::Cubic([rest[0], rest[1], rest[2], rest[3], rest[4], rest[5]])
                    }
                    ("Q", 4) => PresetCommand::Quad([rest[0], rest[1], rest[2], rest[3]]),
                    ("R", _) => PresetCommand::Arc,
                    ("Z", _) => PresetCommand::Close,
                    // A command whose arity does not match is not a command; dropping
                    // it would silently change the shape, so the whole preset is made
                    // undrawable by recording an arc-like refusal.
                    _ => PresetCommand::Arc,
                };
                path.commands.push(command);
            }
            _ => {}
        }
    }
    if let Some(last) = token {
        presets.insert(last, current);
    }
    presets
}

/// How many presets the table carries. For guards, so the count is derived.
#[must_use]
pub fn preset_count() -> usize {
    table().len()
}

/// Whether a preset token is in the table at all.
#[must_use]
pub fn is_known(token: &str) -> bool {
    table().contains_key(token)
}

/// The resolved outline of a preset, in page-local twips, or `None` when this build
/// cannot draw it.
///
/// `None` means the caller keeps today's behaviour — the bounding rectangle, reported
/// — and happens when the token is unknown, when the preset uses `a:arcTo`, or when a
/// coordinate does not resolve. It never means "draw something close".
///
/// `authored` is the shape's own `a:avLst`, which overrides the preset's defaults by
/// name; an authored guide may itself be a formula, which the evaluator handles.
///
/// Complexity: O(g + c) in the preset's guides and commands, both fixed per preset and
/// independent of document size.
#[must_use]
pub fn preset_outline(
    token: &str,
    authored: &[ShapeAdjustment],
    rect: Rect,
) -> Option<Vec<PathCommand>> {
    let definition = table().get(token)?;
    if definition
        .paths
        .iter()
        .flat_map(|path| path.commands.iter())
        .any(|command| matches!(command, PresetCommand::Arc))
    {
        return None;
    }

    let shape = GuideBox::new(
        f64::from(rect.size.width.raw()),
        f64::from(rect.size.height.raw()),
    );
    // The adjust list first, with the shape's own values replacing the preset defaults
    // by name, then the computed guides — which is the order the definitions assume,
    // since a `gdLst` formula routinely names an `adj`.
    let mut pairs: Vec<(&str, &str)> =
        Vec::with_capacity(definition.adjust.len() + definition.guides.len() + authored.len());
    for (name, formula) in &definition.adjust {
        let override_formula = authored
            .iter()
            .find(|guide| guide.name == *name)
            .map(|guide| guide.formula.as_str());
        pairs.push((name, override_formula.unwrap_or(formula)));
    }
    // An authored guide the preset does not declare is still in scope: Word writes
    // extra `a:gd`s for some shapes, and a path may name one.
    for guide in authored {
        if !definition
            .adjust
            .iter()
            .any(|(name, _)| *name == guide.name.as_str())
        {
            pairs.push((guide.name.as_str(), guide.formula.as_str()));
        }
    }
    pairs.extend(definition.guides.iter().copied());
    let resolved = Resolved::new(pairs, shape);

    let mut out = Vec::new();
    for path in &definition.paths {
        // `@w`/`@h` give the path its own coordinate space, scaled onto the box; absent,
        // the coordinates are already in box units because the guides produced them
        // there. Getting this backwards collapses a shape to a corner, which is why the
        // two cases are separate rather than defaulting one to the other.
        let scale = |value: f64, space: Option<f64>, extent: Twip| -> f64 {
            match space {
                Some(space) if space > 0.0 => value * f64::from(extent.raw()) / space,
                _ => value,
            }
        };
        let point = |x: &str, y: &str| -> Option<Point> {
            let x = resolved.token(x)?;
            let y = resolved.token(y)?;
            Some(Point::new(
                Twip(rect.origin.x.raw()) + twip_rounded(scale(x, path.width, rect.size.width)),
                Twip(rect.origin.y.raw()) + twip_rounded(scale(y, path.height, rect.size.height)),
            ))
        };
        for command in &path.commands {
            out.push(match *command {
                PresetCommand::Move([x, y]) => PathCommand::MoveTo {
                    point: point(x, y)?,
                },
                PresetCommand::Line([x, y]) => PathCommand::LineTo {
                    point: point(x, y)?,
                },
                PresetCommand::Cubic([x1, y1, x2, y2, x3, y3]) => PathCommand::CubicTo {
                    control1: point(x1, y1)?,
                    control2: point(x2, y2)?,
                    point: point(x3, y3)?,
                },
                PresetCommand::Quad([x1, y1, x2, y2]) => PathCommand::QuadTo {
                    control: point(x1, y1)?,
                    point: point(x2, y2)?,
                },
                // Already refused above; unreachable, and refusing again rather than
                // drawing is the safe answer if that ever changes.
                PresetCommand::Arc => return None,
                PresetCommand::Close => continue,
            });
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Whether a preset's outline closes — any `a:close` in its paths.
#[must_use]
pub fn preset_is_closed(token: &str) -> bool {
    table().get(token).is_some_and(|definition| {
        definition
            .paths
            .iter()
            .flat_map(|path| path.commands.iter())
            .any(|command| matches!(command, PresetCommand::Close))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1000 × 500 twip box at the origin, so x and y scale differently and a shape
    /// that swapped them would show it.
    fn rect() -> Rect {
        Rect::new(
            Point::new(Twip(0), Twip(0)),
            crate::units::Size::new(Twip(1_000), Twip(500)),
        )
    }

    #[test]
    fn the_table_carries_every_preset_the_specification_defines() {
        // Derived from the committed table, not typed here: 187 is the count ECMA-376
        // Part 1 §20.1.9 defines, and the generator counted 186 until a name-based
        // filter that was swallowing the preset literally called `rect` came out.
        assert_eq!(preset_count(), 187);
        assert!(is_known("rect"), "the preset named like a child element");
        assert!(is_known("roundRect"));
        assert!(is_known("bentConnector3"));
        assert!(is_known("accentBorderCallout1"));
        assert!(!is_known("notAShape"));
    }

    #[test]
    fn the_guides_per_preset_ceiling_is_what_the_table_actually_holds() {
        // `shape_guide` used to cap evaluation at 32 guides, which silently refused
        // nine arc-free presets. This pins the real shape of the data so the cap cannot
        // quietly come back in the wrong place: `gear9` is the widest at 244, and 28
        // presets exceed 32.
        let widest = table()
            .iter()
            .map(|(token, definition)| (definition.adjust.len() + definition.guides.len(), *token))
            .max()
            .expect("the table is not empty");
        assert_eq!(widest, (244, "gear9"));
        let over_32 = table()
            .values()
            .filter(|definition| definition.adjust.len() + definition.guides.len() > 32)
            .count();
        assert_eq!(over_32, 28, "presets a 32-guide cap would have broken");
    }

    #[test]
    fn a_rectangle_preset_resolves_to_its_four_corners() {
        // `rect` is the simplest recipe: move to l,t then three lines and a close. It
        // is the one preset whose expected geometry needs no arithmetic, which makes it
        // the right canary for the whole resolve path.
        let outline = preset_outline("rect", &[], rect()).expect("rect resolves");
        assert_eq!(outline.len(), 4, "a move and three lines: {outline:?}");
        assert_eq!(
            outline[0],
            PathCommand::MoveTo {
                point: Point::new(Twip(0), Twip(0))
            }
        );
        assert_eq!(
            outline[1],
            PathCommand::LineTo {
                point: Point::new(Twip(1_000), Twip(0))
            },
            "r,t is the box's top-right"
        );
        assert_eq!(
            outline[2],
            PathCommand::LineTo {
                point: Point::new(Twip(1_000), Twip(500))
            }
        );
        assert!(preset_is_closed("rect"));
    }

    #[test]
    fn a_preset_that_needs_arcs_is_refused_rather_than_flattened() {
        // `ellipse` is drawn with four arcs. Dropping them would leave a single moveTo
        // and paint nothing, or worse, a stray line — so the whole preset is refused
        // and the caller keeps its reported bounding rectangle.
        assert!(is_known("ellipse"), "it IS in the table");
        assert_eq!(
            preset_outline("ellipse", &[], rect()),
            None,
            "a preset using a:arcTo must not resolve until arcs are drawable"
        );
    }

    #[test]
    fn an_unknown_token_resolves_to_nothing() {
        assert_eq!(preset_outline("notAShape", &[], rect()), None);
    }

    #[test]
    fn an_authored_adjust_value_overrides_the_preset_default() {
        // `roundRect`'s radius comes from `adj`, default 16667. Authoring a different
        // value must move the geometry; if the override were ignored the two outlines
        // would be identical, which is exactly the silent failure this catches.
        let default = preset_outline("roundRect", &[], rect());
        let authored = preset_outline(
            "roundRect",
            &[ShapeAdjustment {
                name: "adj".to_owned(),
                formula: "val 50000".to_owned(),
            }],
            rect(),
        );
        // `roundRect` uses arcs, so both are refused today — but the override path is
        // still exercised below on a preset that does not.
        assert_eq!(default, authored, "both refused for the same reason");

        // `parallelogram` is adjustable and arc-free.
        let default = preset_outline("parallelogram", &[], rect()).expect("parallelogram resolves");
        let authored = preset_outline(
            "parallelogram",
            &[ShapeAdjustment {
                name: "adj".to_owned(),
                formula: "val 10000".to_owned(),
            }],
            rect(),
        )
        .expect("still resolves with an authored adjust");
        assert_ne!(
            default, authored,
            "an authored adj must change the outline, not be ignored"
        );
    }

    #[test]
    fn every_arc_free_preset_resolves_and_the_count_is_the_measured_one() {
        // The number that scopes this work: 124 of 187 presets draw with the commands
        // the path primitive already paints. Asserting it derived — rather than
        // trusting the measurement that produced it — is what will notice when `arcTo`
        // lands and the number moves to 187.
        let mut ok = 0_usize;
        let mut refused = 0_usize;
        for token in table().keys() {
            if preset_outline(token, &[], rect()).is_some() {
                ok += 1;
            } else {
                refused += 1;
            }
        }
        assert_eq!(ok, 124, "arc-free presets that resolve");
        assert_eq!(refused, 63, "presets still needing a:arcTo");
        assert_eq!(ok + refused, 187);
    }
}
