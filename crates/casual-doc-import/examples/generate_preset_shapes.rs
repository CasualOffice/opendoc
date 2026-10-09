// SPDX-License-Identifier: Apache-2.0

//! Generates the DrawingML preset-geometry table the model's geometry engine
//! evaluates (`crates/casual-doc-model/data/preset_shapes.txt`) from the
//! ECMA-376 Part 1 machine-readable annex `presetShapeDefinitions.xml`.
//!
//! ```sh
//! cargo run -p casual-doc-import --example generate_preset_shapes -- \
//!     presetShapeDefinitions.xml > crates/casual-doc-model/data/preset_shapes.txt
//! ```
//!
//! # Why a generator, and why this source
//!
//! The 187 preset shapes are DATA in the standard: guide formulas, adjust
//! defaults and path commands, published by Ecma as an XML annex beside Part 1.
//! Transcribing them by hand is the expensive and error-prone route ONLYOFFICE
//! took (`docs/156` §4.6); generating them from the standard's own file is the
//! route Apache OpenOffice and LibreOffice took. The provenance of the input —
//! where the file was obtained, its SHA-256, and the terms it is used under — is
//! recorded in `crates/casual-doc-model/data/README.md` and in `NOTICE`.
//!
//! # What is kept
//!
//! Everything the engine draws and everything an adjust handle needs:
//! `avLst`, `gdLst`, `ahLst`, `rect` and `pathLst`. `cxnLst` (connection sites,
//! which only a connector-routing feature consumes) is dropped to keep the table
//! small; it is a one-line change here to add it back.
//!
//! The output is sorted by shape name so the engine can binary-search it, and its
//! body carries an FNV-1a 64 checksum in the header so a hand edit is caught by
//! the model's own unit test rather than shipping.
//!
//! It is a manual tool over a file the operator supplies, so it is an example:
//! CI compiles it, and the model's tests verify what it produced.
#![allow(clippy::print_stdout, clippy::print_stderr)] // a manual generator

use std::collections::BTreeMap;
use std::fmt::Write as _;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// The table format this generator writes; the model's loader refuses any other.
const FORMAT: u32 = 1;

fn attr(element: &BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == name.as_bytes())
        .and_then(|attribute| {
            std::str::from_utf8(attribute.value.as_ref())
                .ok()
                .map(str::to_owned)
        })
}

/// An optional attribute as a table token: the value, or `-` when absent.
fn opt(element: &BytesStart<'_>, name: &str) -> String {
    attr(element, name).unwrap_or_else(|| "-".to_owned())
}

fn required(element: &BytesStart<'_>, name: &str, shape: &str) -> String {
    attr(element, name).unwrap_or_else(|| {
        panic!(
            "shape {shape}: <{}> has no {name}",
            String::from_utf8_lossy(element.local_name().as_ref())
        )
    })
}

/// A formula, normalized to single spaces. Formulas are whitespace-separated
/// tokens in the standard, so this loses nothing.
fn formula(element: &BytesStart<'_>, shape: &str) -> String {
    required(element, "fmla", shape)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The path's `@fill`, `@stroke` and `@extrusionOk`, with the ECMA-376 defaults
/// (`norm`, true, true) written explicitly so the loader never has to know them.
fn path_header(element: &BytesStart<'_>) -> String {
    let w = attr(element, "w").unwrap_or_else(|| "0".to_owned());
    let h = attr(element, "h").unwrap_or_else(|| "0".to_owned());
    let fill = attr(element, "fill").unwrap_or_else(|| "norm".to_owned());
    let flag = |name: &str| match attr(element, name).as_deref() {
        None | Some("1" | "true") => "1",
        Some(_) => "0",
    };
    format!(
        "path {w} {h} {fill} {} {}",
        flag("stroke"),
        flag("extrusionOk")
    )
}

/// Reads the annex into `name -> table lines`.
#[allow(clippy::too_many_lines)] // one flat event loop reads more plainly than five helpers
fn read(xml: &str) -> BTreeMap<String, Vec<String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut shapes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut depth = 0_usize;
    let mut shape: Option<String> = None;
    let mut lines: Vec<String> = Vec::new();
    // Which list a `gd` belongs to, and the open command collecting `pt`s.
    let mut list: Option<&'static str> = None;
    let mut command: Option<(&'static str, Vec<String>)> = None;
    let mut handle: Option<String> = None;
    loop {
        let event = reader.read_event().expect("well-formed XML");
        let (element, is_empty) = match &event {
            Event::Start(element) => (Some(element.clone()), false),
            Event::Empty(element) => (Some(element.clone()), true),
            Event::End(end) => {
                depth -= 1;
                let local = end.local_name();
                let local = std::str::from_utf8(local.as_ref()).expect("UTF-8 names");
                match local {
                    "avLst" | "gdLst" => list = None,
                    "moveTo" | "lnTo" | "quadBezTo" | "cubicBezTo" => {
                        let (code, points) = command.take().expect("an open command");
                        lines.push(format!("{code} {}", points.join(" ")));
                    }
                    "ahXY" | "ahPolar" => {
                        let mut line = handle.take().expect("an open handle");
                        assert!(line.ends_with(" ?"), "handle without a pos: {line}");
                        line.truncate(line.len() - 2);
                        lines.push(line);
                    }
                    _ => {}
                }
                if depth == 1 {
                    let name = shape.take().expect("a shape element");
                    assert!(
                        shapes
                            .insert(name.clone(), core::mem::take(&mut lines))
                            .is_none(),
                        "duplicate shape {name}"
                    );
                }
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        let element = element.expect("a start element");
        if !is_empty {
            depth += 1;
        }
        let local = element.local_name();
        let local = std::str::from_utf8(local.as_ref())
            .expect("UTF-8 names")
            .to_owned();
        // depth 1 is the root (`presetShapeDefinitons`, spelt so in the annex);
        // depth 2 is one shape.
        if depth == 1 {
            continue;
        }
        if depth == 2 && !is_empty {
            shape = Some(local);
            continue;
        }
        let name = shape.clone().unwrap_or_default();
        match local.as_str() {
            "avLst" => list = Some("av"),
            "gdLst" => list = Some("gd"),
            "gd" => {
                let code = list.expect("a gd inside avLst or gdLst");
                lines.push(format!(
                    "{code} {} {}",
                    required(&element, "name", &name),
                    formula(&element, &name)
                ));
            }
            "ahXY" => {
                handle = Some(format!(
                    "ahxy {} {} {} {} {} {} ?",
                    opt(&element, "gdRefX"),
                    opt(&element, "minX"),
                    opt(&element, "maxX"),
                    opt(&element, "gdRefY"),
                    opt(&element, "minY"),
                    opt(&element, "maxY"),
                ));
            }
            "ahPolar" => {
                handle = Some(format!(
                    "ahpolar {} {} {} {} {} {} ?",
                    opt(&element, "gdRefR"),
                    opt(&element, "minR"),
                    opt(&element, "maxR"),
                    opt(&element, "gdRefAng"),
                    opt(&element, "minAng"),
                    opt(&element, "maxAng"),
                ));
            }
            "pos" => {
                // Inside a connection site this is dropped with the site.
                if let Some(line) = handle.as_mut() {
                    let at = line.len() - 1;
                    line.replace_range(
                        at..,
                        &format!(
                            "{} {} ?",
                            required(&element, "x", &name),
                            required(&element, "y", &name)
                        ),
                    );
                }
            }
            "rect" => lines.push(format!(
                "rect {} {} {} {}",
                required(&element, "l", &name),
                required(&element, "t", &name),
                required(&element, "r", &name),
                required(&element, "b", &name)
            )),
            "path" => lines.push(path_header(&element)),
            "moveTo" => command = Some(("M", Vec::new())),
            "lnTo" => command = Some(("L", Vec::new())),
            "quadBezTo" => command = Some(("Q", Vec::new())),
            "cubicBezTo" => command = Some(("C", Vec::new())),
            "pt" => {
                let (_, points) = command.as_mut().expect("a pt inside a command");
                points.push(required(&element, "x", &name));
                points.push(required(&element, "y", &name));
            }
            "arcTo" => lines.push(format!(
                "A {} {} {} {}",
                required(&element, "wR", &name),
                required(&element, "hR", &name),
                required(&element, "stAng", &name),
                required(&element, "swAng", &name)
            )),
            "close" => lines.push("Z".to_owned()),
            // Connection sites and their `pos`/`cxn` children are not kept.
            "ahLst" | "cxnLst" | "cxn" | "pathLst" => {}
            other => panic!("shape {name}: unexpected element <{other}>"),
        }
    }
    shapes
}

/// FNV-1a 64 over the table body, the same hash `fixtures/manifest.json`'s
/// visual baselines use; the model's loader recomputes it.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: generate_preset_shapes <presetShapeDefinitions.xml>");
    let xml = std::fs::read_to_string(&path).expect("read the annex");
    let shapes = read(&xml);
    assert_eq!(shapes.len(), 187, "ECMA-376 defines 187 preset geometries");

    let mut body = String::new();
    for (name, lines) in &shapes {
        writeln!(body, "shape {name}").expect("write to a String");
        for line in lines {
            writeln!(body, "{line}").expect("write to a String");
        }
    }
    print!(
        "# opendoc preset shape geometry table, format {FORMAT}.\n\
         # GENERATED from ECMA-376 Part 1 `presetShapeDefinitions.xml` by\n\
         # `cargo run -p casual-doc-import --example generate_preset_shapes`.\n\
         # Provenance and terms: crates/casual-doc-model/data/README.md.\n\
         # Do not edit by hand; the loader verifies this checksum of the body.\n\
         # body-fnv1a64: {:016x}\n{body}",
        fnv1a64(body.as_bytes())
    );
}
