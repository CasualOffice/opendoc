// SPDX-License-Identifier: Apache-2.0

//! Derivation of the committed DrawingML preset-geometry table.
//!
//! Lives in the crate rather than only in the generator example so that the generator
//! and the guard that checks its output run the SAME code. A guard that re-implemented
//! the derivation would pass whenever both copies were wrong the same way, which is
//! the failure mode `105` EV-001 exists to prevent.
//!
//! The format, the provenance of the input, and why ONLYOFFICE's transcription of the
//! same presets is deliberately not the source are documented on
//! `casual-doc-layout`'s `shape_preset` module and in `fixtures/spec/README.md`.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// The `a:pt` children an element carries, read in order.
fn attribute(element: &BytesStart<'_>, name: &[u8]) -> String {
    element
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == name)
        .and_then(|attribute| String::from_utf8(attribute.value.to_vec()).ok())
        .unwrap_or_default()
}

/// Emits the pending point-bearing command, if one is open.
fn flush(
    out: &mut String,
    pending: &mut Option<&'static str>,
    points: &mut Vec<(String, String)>,
    commands: &mut u32,
) {
    if let Some(code) = pending.take() {
        out.push_str(code);
        for (x, y) in points.iter() {
            out.push('\t');
            out.push_str(x);
            out.push('\t');
            out.push_str(y);
        }
        out.push('\n');
        *commands += 1;
    }
    points.clear();
}

/// Derives the compact table from `presetShapeDefinitions.xml`.
///
/// Complexity: O(n) in the input, which is a fixed specification artifact.
pub fn derive_preset_table(xml: &str) -> Result<String, quick_xml::Error> {
    let mut reader = Reader::from_str(xml);

    let mut out = String::new();
    // Depth 1 is a preset element; the root is depth 0.
    let mut depth = 0_u32;
    // Which guide list is open, so an `a:gd` is filed as an adjust default or a
    // computed guide. They are the same element in two places.
    let mut in_av = false;
    let mut in_gd = false;
    // The command awaiting its `a:pt` children, and the points gathered so far.
    let mut pending: Option<&'static str> = None;
    let mut points: Vec<(String, String)> = Vec::new();
    let mut presets = 0_u32;
    let mut commands = 0_u32;

    loop {
        // `Start` and `Empty` are handled through one `open` path and differ only in
        // depth and in whether a close follows immediately. Treating them alike was the
        // first version's bug: an empty element has no `End`, so depth drifted on every
        // one of them — and this file is mostly empty elements.
        let event = reader.read_event()?;
        let (element, empty) = match &event {
            Event::Eof => break,
            Event::Start(element) => (element.clone(), false),
            Event::Empty(element) => (element.clone(), true),
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                match element.local_name().as_ref() {
                    b"avLst" => in_av = false,
                    b"gdLst" => in_gd = false,
                    b"moveTo" | b"lnTo" | b"cubicBezTo" | b"quadBezTo" => {
                        flush(&mut out, &mut pending, &mut points, &mut commands);
                    }
                    _ => {}
                }
                continue;
            }
            _ => continue,
        };
        let local = element.local_name();
        let name = local.as_ref().to_vec();
        // A preset token is any element at DEPTH 1. The depth alone discriminates,
        // because a preset's own children sit at depth 2 — and an extra name-based
        // filter here is not merely redundant, it is wrong: ECMA-376 has a preset
        // literally named `rect`, which a list of known child names swallows. The
        // generator counted 186 of 187 until that came out.
        match name.as_slice() {
            b"avLst" => in_av = true,
            b"gdLst" => in_gd = true,
            b"gd" => {
                // The same element in two lists; which one decides its record kind.
                let kind = if in_av {
                    "A"
                } else if in_gd {
                    "G"
                } else {
                    // An `a:gd` outside both lists belongs to an adjust handle, which is
                    // an authoring affordance, not geometry.
                    ""
                };
                if !kind.is_empty() {
                    out.push_str(kind);
                    out.push('\t');
                    out.push_str(&attribute(&element, b"name"));
                    out.push('\t');
                    out.push_str(&attribute(&element, b"fmla"));
                    out.push('\n');
                }
            }
            b"path" => {
                out.push_str("H\t");
                out.push_str(&attribute(&element, b"w"));
                out.push('\t');
                out.push_str(&attribute(&element, b"h"));
                out.push('\t');
                out.push_str(&attribute(&element, b"fill"));
                out.push('\t');
                out.push_str(&attribute(&element, b"stroke"));
                out.push('\n');
            }
            b"moveTo" => {
                pending = Some("M");
                points.clear();
            }
            b"lnTo" => {
                pending = Some("L");
                points.clear();
            }
            b"cubicBezTo" => {
                pending = Some("C");
                points.clear();
            }
            b"quadBezTo" => {
                pending = Some("Q");
                points.clear();
            }
            b"arcTo" => {
                out.push_str("R\t");
                out.push_str(&attribute(&element, b"wR"));
                out.push('\t');
                out.push_str(&attribute(&element, b"hR"));
                out.push('\t');
                out.push_str(&attribute(&element, b"stAng"));
                out.push('\t');
                out.push_str(&attribute(&element, b"swAng"));
                out.push('\n');
                commands += 1;
            }
            b"close" => {
                out.push_str("Z\n");
                commands += 1;
            }
            b"pt" => {
                points.push((attribute(&element, b"x"), attribute(&element, b"y")));
            }
            other => {
                if depth == 1 {
                    out.push_str("P\t");
                    out.push_str(&String::from_utf8_lossy(other));
                    out.push('\n');
                    presets += 1;
                }
            }
        }
        if empty {
            // An empty command element closes at once; an empty anything else needs no
            // close handling.
            if matches!(
                name.as_slice(),
                b"moveTo" | b"lnTo" | b"cubicBezTo" | b"quadBezTo"
            ) {
                flush(&mut out, &mut pending, &mut points, &mut commands);
            }
            match name.as_slice() {
                b"avLst" => in_av = false,
                b"gdLst" => in_gd = false,
                _ => {}
            }
        } else {
            depth += 1;
        }
    }

    let _ = (presets, commands);
    Ok(out)
}
