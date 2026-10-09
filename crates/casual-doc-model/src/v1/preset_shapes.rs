// SPDX-License-Identifier: Apache-2.0

//! The 187 ECMA-376 preset shape geometries (`a:prstGeom@prst`), as data.
//!
//! The definitions are the standard's own: generated from ECMA-376 Part 1's
//! `presetShapeDefinitions.xml` annex by
//! `crates/casual-doc-import/examples/generate_preset_shapes.rs` into
//! `crates/casual-doc-model/data/preset_shapes.txt`, whose provenance and terms
//! are recorded in `crates/casual-doc-model/data/README.md` and `NOTICE`. Nothing
//! here is transcribed by hand, and nothing is taken from another implementation.
//!
//! Each preset is read into the SAME [`CustomGeometry`] an authored `a:custGeom`
//! is, and compiled by the same [`GeometryProgram`], so there is one engine for
//! both and a preset cannot be drawn by rules a freeform is not (`docs/119` §6).
//!
//! # Cost
//!
//! The table is parsed, checksummed and compiled once per process, on first use:
//! 187 programs from a 128 KB text, measured in the tests. A lookup after that is
//! a binary search over the sorted names — O(log 187) string comparisons.

use std::sync::OnceLock;

use super::{
    AdjustHandle, CustomGeometry, GeometryPoint, GeometryProgram, GeometryRect, GeometryValue,
    PathFill, ShapeAdjustment, ShapePath, ShapePathCommand,
};

/// The generated table. Not hand-edited: the loader verifies its checksum.
const TABLE: &str = include_str!("../../data/preset_shapes.txt");

/// How many preset geometries ECMA-376 defines, and therefore how many the table
/// carries.
pub const PRESET_SHAPE_COUNT: usize = 187;

/// One preset geometry: its adjust defaults, its definition, and the definition
/// compiled.
#[derive(Debug)]
pub struct PresetShape {
    name: String,
    adjustments: Vec<ShapeAdjustment>,
    geometry: CustomGeometry,
    program: GeometryProgram,
}

impl PresetShape {
    /// The `ST_ShapeType` token (`star5`, `wedgeRectCallout`, …).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The preset's adjust values and their defaults (`a:avLst`). A shape's
    /// authored `a:avLst` overrides these by name.
    #[must_use]
    pub fn adjustments(&self) -> &[ShapeAdjustment] {
        &self.adjustments
    }

    /// The definition, in the same form an authored `a:custGeom` takes.
    #[must_use]
    pub fn geometry(&self) -> &CustomGeometry {
        &self.geometry
    }

    /// The definition, compiled.
    #[must_use]
    pub fn program(&self) -> &GeometryProgram {
        &self.program
    }
}

/// The preset geometry `name` names, or `None` when it is not one of the
/// standard's 187.
///
/// Complexity: O(log 187) after the first call, which loads the table (O(table)).
#[must_use]
pub fn preset_shape(name: &str) -> Option<&'static PresetShape> {
    let table = presets();
    table
        .binary_search_by(|preset| preset.name.as_str().cmp(name))
        .ok()
        .map(|index| &table[index])
}

/// Every preset name, sorted.
pub fn preset_shape_names() -> impl Iterator<Item = &'static str> {
    presets().iter().map(|preset| preset.name.as_str())
}

fn presets() -> &'static [PresetShape] {
    static PRESETS: OnceLock<Vec<PresetShape>> = OnceLock::new();
    PRESETS.get_or_init(|| load(TABLE).unwrap_or_default())
}

/// FNV-1a 64, the hash the generator stamps the table body with.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Why the table did not load. Only ever seen by the tests: a table that fails
/// here is a corrupt build artifact, and the engine then knows no presets, which
/// paints them as the bounding rectangles they were before the table existed.
#[derive(Debug, PartialEq, Eq)]
enum LoadError {
    /// The header carries no `body-fnv1a64` line.
    MissingChecksum,
    /// The body does not hash to the header's value: it was edited by hand or
    /// truncated.
    Checksum,
    /// A line is not in the format, at this 1-based line of the body.
    Line(usize),
    /// A shape does not compile.
    Compile(String),
    /// The names are not strictly sorted, so the binary search would miss some.
    Order,
}

/// Parses, verifies and compiles the table.
fn load(table: &str) -> Result<Vec<PresetShape>, LoadError> {
    const MARK: &str = "# body-fnv1a64: ";
    let header_at = table.find(MARK).ok_or(LoadError::MissingChecksum)?;
    let digits_at = header_at + MARK.len();
    let line_end = table[digits_at..]
        .find('\n')
        .map_or(table.len(), |offset| digits_at + offset);
    let expected =
        u64::from_str_radix(&table[digits_at..line_end], 16).map_err(|_| LoadError::Checksum)?;
    let body = table.get(line_end + 1..).unwrap_or_default();
    if fnv1a64(body.as_bytes()) != expected {
        return Err(LoadError::Checksum);
    }

    let mut out: Vec<PresetShape> = Vec::with_capacity(PRESET_SHAPE_COUNT);
    let mut current: Option<(String, Vec<ShapeAdjustment>, CustomGeometry)> = None;
    let finish = |current: Option<(String, Vec<ShapeAdjustment>, CustomGeometry)>,
                  out: &mut Vec<PresetShape>|
     -> Result<(), LoadError> {
        if let Some((name, adjustments, geometry)) = current {
            let program = GeometryProgram::compile(&adjustments, &geometry)
                .map_err(|error| LoadError::Compile(format!("{name}: {error}")))?;
            out.push(PresetShape {
                name,
                adjustments,
                geometry,
                program,
            });
        }
        Ok(())
    };
    for (index, line) in body.lines().enumerate() {
        let bad = || LoadError::Line(index + 1);
        let mut tokens = line.split_whitespace();
        let Some(code) = tokens.next() else {
            continue;
        };
        let rest: Vec<&str> = tokens.collect();
        if code == "shape" {
            finish(current.take(), &mut out)?;
            let [name] = rest[..] else { return Err(bad()) };
            current = Some((name.to_owned(), Vec::new(), CustomGeometry::default()));
            continue;
        }
        let (_, adjustments, geometry) = current.as_mut().ok_or_else(bad)?;
        let value = |token: &str| {
            if token == "-" {
                Ok(None)
            } else {
                GeometryValue::parse(token).map(Some).ok_or_else(bad)
            }
        };
        let required = |token: &str| value(token)?.ok_or_else(bad);
        let point = |x: &str, y: &str| -> Result<GeometryPoint, LoadError> {
            Ok(GeometryPoint {
                x: required(x)?,
                y: required(y)?,
            })
        };
        let name = |token: &str| (token != "-").then(|| token.to_owned());
        match (code, &rest[..]) {
            ("av" | "gd", [guide, formula @ ..]) if !formula.is_empty() => {
                let guide = ShapeAdjustment {
                    name: (*guide).to_owned(),
                    formula: formula.join(" "),
                };
                if code == "av" {
                    adjustments.push(guide);
                } else {
                    geometry.guides.push(guide);
                }
            }
            ("ahxy", [gx, min_x, max_x, gy, min_y, max_y, px, py]) => {
                geometry.handles.push(AdjustHandle::Xy {
                    guide_x: name(gx),
                    min_x: value(min_x)?,
                    max_x: value(max_x)?,
                    guide_y: name(gy),
                    min_y: value(min_y)?,
                    max_y: value(max_y)?,
                    position: point(px, py)?,
                });
            }
            ("ahpolar", [gr, min_r, max_r, ga, min_a, max_a, px, py]) => {
                geometry.handles.push(AdjustHandle::Polar {
                    guide_radius: name(gr),
                    min_radius: value(min_r)?,
                    max_radius: value(max_r)?,
                    guide_angle: name(ga),
                    min_angle: value(min_a)?,
                    max_angle: value(max_a)?,
                    position: point(px, py)?,
                });
            }
            ("rect", [l, t, r, b]) => {
                geometry.text_rect = Some(GeometryRect {
                    left: required(l)?,
                    top: required(t)?,
                    right: required(r)?,
                    bottom: required(b)?,
                });
            }
            ("path", [w, h, fill, stroke, extrusion]) => {
                geometry.paths.push(ShapePath {
                    width_emu: w.parse().map_err(|_| bad())?,
                    height_emu: h.parse().map_err(|_| bad())?,
                    fill: PathFill::from_token(fill).ok_or_else(bad)?,
                    stroke: *stroke == "1",
                    extrusion_ok: *extrusion == "1",
                    commands: Vec::new(),
                });
            }
            (command, operands) => {
                let path = geometry.paths.last_mut().ok_or_else(bad)?;
                path.commands.push(match (command, operands) {
                    ("M", [x, y]) => ShapePathCommand::MoveTo {
                        point: point(x, y)?,
                    },
                    ("L", [x, y]) => ShapePathCommand::LineTo {
                        point: point(x, y)?,
                    },
                    ("A", [wr, hr, st, sw]) => ShapePathCommand::ArcTo {
                        width_radius: required(wr)?,
                        height_radius: required(hr)?,
                        start_angle: required(st)?,
                        swing_angle: required(sw)?,
                    },
                    ("Q", [cx, cy, x, y]) => ShapePathCommand::QuadBezTo {
                        control: point(cx, cy)?,
                        point: point(x, y)?,
                    },
                    ("C", [ax, ay, bx, by, x, y]) => ShapePathCommand::CubicBezTo {
                        control1: point(ax, ay)?,
                        control2: point(bx, by)?,
                        point: point(x, y)?,
                    },
                    ("Z", []) => ShapePathCommand::Close,
                    _ => return Err(bad()),
                });
            }
        }
    }
    finish(current, &mut out)?;
    if !out.windows(2).all(|pair| pair[0].name < pair[1].name) {
        return Err(LoadError::Order);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_loads_every_preset_the_standard_defines() {
        let loaded = load(TABLE).expect("the committed table loads");
        assert_eq!(loaded.len(), PRESET_SHAPE_COUNT);
        assert_eq!(preset_shape_names().count(), PRESET_SHAPE_COUNT);
        for name in ["rect", "star5", "wedgeRectCallout", "cloud", "gear9"] {
            assert!(preset_shape(name).is_some(), "{name} is a preset");
        }
        assert!(preset_shape("notAShape").is_none());
    }

    /// The table is a generated artifact; a hand edit that does not re-run the
    /// generator must not load, because nothing else would notice it.
    #[test]
    fn a_hand_edited_table_is_refused() {
        let edited = TABLE.replacen("av adj val 19098", "av adj val 19099", 1);
        assert_ne!(edited, TABLE, "the probe edit must land");
        assert_eq!(load(&edited).err(), Some(LoadError::Checksum));
    }

    /// Every preset must evaluate at an ordinary box with its default adjust
    /// values — a definition that cannot is a preset this engine silently paints
    /// as a rectangle.
    #[test]
    fn every_preset_evaluates_at_an_ordinary_box() {
        let mut failures = Vec::new();
        for (width, height) in [(1_828_800.0, 914_400.0), (914_400.0, 1_828_800.0)] {
            for name in preset_shape_names() {
                let preset = preset_shape(name).expect("listed");
                match preset.program().evaluate(width, height, &[]) {
                    Some(resolved) if !resolved.paths.is_empty() => {}
                    _ => failures.push(format!("{name} @ {width}x{height}")),
                }
            }
        }
        assert!(
            failures.is_empty(),
            "presets that do not evaluate: {failures:?}"
        );
    }

    /// The degenerate boxes and the edges of every adjust range are where a
    /// guide divides by zero. Each preset must still evaluate there — as the flat
    /// or collapsed shape it is — rather than refuse and paint a rectangle. With
    /// zero divisors refused, this listed 153 flat-box cases and 21 adjust
    /// settings, among them `parallelogram` at `adj = 0`.
    #[test]
    fn every_preset_evaluates_at_flat_boxes_and_at_the_ends_of_its_adjust_range() {
        let mut failures = Vec::new();
        for name in preset_shape_names() {
            let preset = preset_shape(name).expect("listed");
            for (width, height) in [(1_828_800.0, 0.0), (0.0, 914_400.0), (0.0, 0.0)] {
                if preset.program().evaluate(width, height, &[]).is_none() {
                    failures.push(format!("{name} @ {width}x{height}"));
                }
            }
            for value in ["val 0", "val 50000", "val 100000", "val -100000"] {
                let overrides: Vec<ShapeAdjustment> = preset
                    .adjustments()
                    .iter()
                    .map(|adjustment| ShapeAdjustment {
                        name: adjustment.name.clone(),
                        formula: value.to_owned(),
                    })
                    .collect();
                if preset
                    .program()
                    .evaluate(1_828_800.0, 914_400.0, &overrides)
                    .is_none()
                {
                    failures.push(format!("{name} with every adjust at {value}"));
                }
            }
        }
        assert!(failures.is_empty(), "presets that refuse: {failures:?}");
    }

    /// Adjust handles evaluate to points on the shape, driving the values the
    /// definition names — what an editor needs to draw Word's yellow handles.
    #[test]
    fn a_presets_adjust_handles_resolve() {
        let star = preset_shape("star5").expect("a preset");
        let handles = star
            .program()
            .handles(1_000.0, 1_000.0, &[])
            .expect("evaluates");
        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].guides, [None, Some("adj")]);
        assert_eq!(handles[0].bounds[1], (Some(0.0), Some(50_000.0)));
        assert!(
            (handles[0].position.x - 500.0).abs() < 1e-9,
            "the star5 handle sits on the vertical centre line: {:?}",
            handles[0].position
        );
    }
}
