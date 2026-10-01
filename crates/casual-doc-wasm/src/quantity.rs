// SPDX-License-Identifier: Apache-2.0

//! Measurement units across the JS boundary — ONLYOFFICE's `cmbUnit`, Word's
//! *Measurement units* (`docs/153`, "Choose measurement units").
//!
//! The engine already holds the hard half: `casual_doc_layout::quantity` parses
//! and formats `cm`, `mm`, `in`, `pt` and `pica` against the twip **by exact
//! integer arithmetic**, so `2.54 cm` is 1440 twips rather than a
//! floating-point approximation of it. This module is the four calls a dialog
//! needs, and nothing else: a chooser's rows, the first-run default, a parse and
//! a format.
//!
//! # Why none of this hangs off `WasmDocument`
//!
//! A unit preference belongs to the *person*, not to a document — the same choice
//! governs the ruler, the Page Setup dialog, the paragraph-indent spinners and the
//! table-properties pane, including while no document is open at all. Hanging it
//! off a document handle would have made the host either re-apply it per handle
//! or keep a second copy of the truth. These are therefore free functions, and
//! the preference lives where the host keeps its preferences.
//!
//! # The one rule a host must not get wrong: persist the id, never an ordinal
//!
//! Every function here speaks [`MeasurementUnit::id`] strings — `"cm"`, `"mm"`,
//! `"inch"`, `"point"`, `"pica"`. No ordinal, no index, no array position is
//! exposed, deliberately: ONLYOFFICE carries **two disagreeing unit enums** and
//! paid for it with a remap ternary copy-pasted into five files. The engine's own
//! doc comment on `MeasurementUnit::ALL` states the same rule; this module is the
//! boundary that makes it impossible to break from JS, because there is no
//! ordinal here to persist.
//!
//! # Refusals are returned, never rounded to zero
//!
//! `parse_measurement` throws for every [`QuantityError`], with the engine's own
//! message. A dialog that silently read `1,234.5` as `0` would set a margin the
//! user did not type, and `1,234.5` is refused by the engine precisely because
//! guessing whether the comma groups or separates is not something a measurement
//! field may do (`AGENTS.md`: no silent data loss).
//!
//! # Complexity
//!
//! Every function here is O(1) in document size and O(input) in the text typed.
//! Nothing touches a document; nothing allocates per page.

use casual_doc_layout::quantity::{DecimalSeparator, MeasurementUnit, format_twips, parse_twips};
use casual_doc_layout::units::Twip;
use wasm_bindgen::prelude::*;

use crate::to_js;

/// Every unit a chooser should offer, in order, as JSON.
///
/// `[{"id","suffix","decimalPlaces","stepTwip"}]` — the stable id to persist and
/// pass back, the abbreviation to show, how many decimals a field in this unit
/// displays, and the twip value of one of those decimal steps (a spinner's
/// `step`).
///
/// The order is the engine's — metric coarse to fine, then imperial, then
/// typographic — and is **not** ONLYOFFICE's (`cm`, `pt`, `inch`, with no
/// millimetre or pica at all) nor Word's (inches first). It carries no index,
/// because an index is the thing a host must never persist.
///
/// `decimalPlaces` is not a taste choice: it is the largest precision for which
/// one display step is still at least one whole twip, so a field built from it
/// never shows a digit that cannot survive a round trip. Millimetres get one
/// decimal for that reason, not out of neglect.
#[wasm_bindgen(js_name = measurementUnits)]
#[must_use]
pub fn measurement_units() -> String {
    let rows: Vec<String> = MeasurementUnit::ALL
        .into_iter()
        .map(|unit| {
            format!(
                "{{\"id\":\"{}\",\"suffix\":\"{}\",\"decimalPlaces\":{},\"stepTwip\":{}}}",
                unit.id(),
                unit.suffix(),
                unit.decimal_places(),
                unit.step().raw(),
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}

/// The unit to default to before anyone has chosen, from a region code or a full
/// BCP-47 tag. Returns an id from [`measurement_units`].
///
/// Inches for the United States and Canada, centimetres everywhere else — the
/// rule ONLYOFFICE applies and the two countries whose paper sizes are imperial.
/// `"us"`, `"en-US"` and `"en_us"` all give `"inch"`.
///
/// This is a **default**, not a lock: a host stores what the person chose and
/// only falls back here on a first run.
#[wasm_bindgen(js_name = defaultMeasurementUnit)]
#[must_use]
pub fn default_measurement_unit(region: &str) -> String {
    MeasurementUnit::for_region(region).id().to_owned()
}

/// The decimal separator a BCP-47 language tag implies, as the literal character
/// `"."` or `","`, for a host that has a locale but no separator preference.
///
/// Coarse by construction, and documented as such in the engine: it keys off the
/// language subtag alone, so `es-ES` (comma) and `es-MX` (dot) both report the
/// language's majority convention. A host with CLDR data already knows better
/// and should pass its own answer to [`format_measurement`] instead of calling
/// this.
///
/// Parsing accepts both separators regardless of this; only formatting needs to
/// choose one, because its output is read rather than parsed.
#[wasm_bindgen(js_name = decimalSeparatorForLanguage)]
#[must_use]
pub fn decimal_separator_for_language(language_tag: &str) -> String {
    DecimalSeparator::for_language_tag(language_tag)
        .character()
        .to_string()
}

/// Reads what a person typed into a measurement field, in twips.
///
/// `unit_id` is the id of the unit the field is **labelled** in, and is used only
/// when the text carries no suffix of its own: `"2.5"` in a centimetre field is
/// 2.5 cm, while `"2.5 in"` is 2.5 inches whatever the field is labelled. Both
/// `.` and `,` are accepted as the decimal separator, so a German user typing
/// `2,54` into a field a host formatted as `2.54` is understood.
///
/// Exact: the amount is kept as a scaled integer and converted by exact rational
/// arithmetic, so `"2.54"` cm is 1440 twips and not 1439.9997 rounded.
///
/// # Errors
///
/// Throws, with the engine's message, for an empty field, a value that is not a
/// number (a group separator — `1,234.5` — lands here rather than being guessed
/// at), an unknown unit suffix, more significant digits than the exact conversion
/// accepts, and a value outside the twip grid. **A refusal, never a zero**: a
/// field that quietly reads an unparseable entry as 0 applies a margin nobody
/// asked for.
#[wasm_bindgen(js_name = parseMeasurement)]
pub fn parse_measurement(text: &str, unit_id: &str) -> Result<i32, JsValue> {
    parse_measurement_inner(text, unit_id).map_err(to_js)
}

/// Writes a twip value for a person to read, in `unit_id`, to that unit's
/// display precision.
///
/// `separator` is `"."` or `","` (the words `"dot"` and `"comma"` are accepted
/// too, for a host that would rather not pass punctuation). It is explicit
/// because the engine holds no locale of its own and must not infer one from the
/// host process — `docs/124` is where the chrome resolves one.
///
/// `format` then `parse` is a fixed point for every displayable value, which the
/// engine holds with an exhaustive guard: what a host shows is what it reads
/// back.
///
/// # Errors
///
/// Throws for an unknown `unit_id` or an unknown `separator`, rather than quietly
/// falling back to inches and a dot — a silently substituted unit is a wrong
/// number presented as a right one.
#[wasm_bindgen(js_name = formatMeasurement)]
pub fn format_measurement(twips: i32, unit_id: &str, separator: &str) -> Result<String, JsValue> {
    format_measurement_inner(twips, unit_id, separator).map_err(to_js)
}

/// [`parse_measurement`] without the `JsValue`, so native tests exercise the
/// same path the boundary does.
pub(crate) fn parse_measurement_inner(text: &str, unit_id: &str) -> Result<i32, String> {
    let unit = unit_of(unit_id)?;
    parse_twips(text, unit)
        .map(Twip::raw)
        .map_err(|error| format!("measurement: {error}"))
}

/// [`format_measurement`] without the `JsValue`, for the same reason.
pub(crate) fn format_measurement_inner(
    twips: i32,
    unit_id: &str,
    separator: &str,
) -> Result<String, String> {
    let unit = unit_of(unit_id)?;
    let separator = separator_of(separator)?;
    Ok(format_twips(Twip(twips), unit, separator))
}

/// Resolves a persisted unit id. Both the id (`"inch"`) and the abbreviation
/// (`"in"`) resolve, so a host that stored either keeps working.
fn unit_of(unit_id: &str) -> Result<MeasurementUnit, String> {
    MeasurementUnit::from_id(unit_id).ok_or_else(|| {
        let known: Vec<&str> = MeasurementUnit::ALL.into_iter().map(|u| u.id()).collect();
        format!(
            "measurement: unknown unit {unit_id:?}; the units are {}",
            known.join(", ")
        )
    })
}

/// Resolves a separator argument. The character or the word, nothing else.
fn separator_of(separator: &str) -> Result<DecimalSeparator, String> {
    match separator {
        "." | "dot" => Ok(DecimalSeparator::Dot),
        "," | "comma" => Ok(DecimalSeparator::Comma),
        other => Err(format!(
            "measurement: unknown decimal separator {other:?}; pass \".\" or \",\""
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact boundary the whole module exists for: 2.54 cm is 1440 twips on
    /// the nose, and an inch formatted back is `"1"` and not `"0.99"`.
    ///
    /// Mutation that drove it red: `per_unit`'s centimetre arm is the engine's
    /// and out of this lane's reach, so the mutation taken instead was in
    /// `parse_measurement_inner` — routing the value through `f64`
    /// (`Twip((text.parse::<f64>().unwrap() * 1440.0 / 2.54) as i32)`).
    #[test]
    fn a_centimetre_is_exact_across_the_boundary() {
        assert_eq!(parse_measurement_inner("2.54", "cm"), Ok(1440));
        assert_eq!(parse_measurement_inner("1", "inch"), Ok(1440));
        assert_eq!(parse_measurement_inner("72", "point"), Ok(1440));
        assert_eq!(parse_measurement_inner("6", "pica"), Ok(1440));
        assert_eq!(parse_measurement_inner("25.4", "mm"), Ok(1440));
        assert_eq!(format_measurement_inner(1440, "cm", "."), Ok("2.54".into()));
        // The display precision is always written out, so a field does not change
        // width as the user types: an inch is `1.00`, not `1`.
        assert_eq!(
            format_measurement_inner(1440, "inch", "."),
            Ok("1.00".into())
        );
        assert_eq!(format_measurement_inner(1440, "mm", ","), Ok("25,4".into()));
    }

    /// A suffix in the text wins over the field's label, and both separators are
    /// read — the two things a user does that a unit-blind field gets wrong.
    #[test]
    fn a_typed_suffix_wins_and_both_separators_are_read() {
        assert_eq!(parse_measurement_inner("1 in", "cm"), Ok(1440));
        assert_eq!(parse_measurement_inner("2,54", "cm"), Ok(1440));
        assert_eq!(parse_measurement_inner("2.54", "cm"), Ok(1440));
    }

    /// A refusal a caller can show, never a silent zero. This is the guard the
    /// lane rule names: `1,234.5` is ambiguous, so the engine refuses it, and the
    /// boundary must pass that refusal on rather than reading a margin of 0.
    #[test]
    fn an_unreadable_measurement_is_refused_rather_than_read_as_zero() {
        for bad in ["", "   ", "abc", "1,234.5", "5 furlongs", "1e9999"] {
            let refused = parse_measurement_inner(bad, "cm");
            assert!(
                refused.is_err(),
                "{bad:?} must be refused, not read as {refused:?}"
            );
            let message = refused.unwrap_err();
            assert!(
                message.starts_with("measurement: "),
                "the refusal must carry a message a dialog can show, got {message:?}"
            );
        }
        // And the value that IS zero still parses, so the refusal is not just
        // "anything falsy fails".
        assert_eq!(parse_measurement_inner("0", "cm"), Ok(0));
    }

    /// An unknown unit or separator is a refusal, not a silent fallback to the
    /// engine's historical inch-and-dot assumption.
    #[test]
    fn an_unknown_unit_or_separator_is_refused() {
        let unit = parse_measurement_inner("1", "furlong").unwrap_err();
        assert!(unit.contains("unknown unit"), "{unit}");
        assert!(unit.contains("inch"), "the refusal lists the units: {unit}");
        let separator = format_measurement_inner(1440, "cm", ";").unwrap_err();
        assert!(
            separator.contains("unknown decimal separator"),
            "{separator}"
        );
        // A persisted ordinal is the mistake this module exists to prevent, so
        // it must not resolve to the unit at that index.
        for ordinal in ["0", "1", "2", "3", "4"] {
            assert!(
                parse_measurement_inner("1", ordinal).is_err(),
                "ordinal {ordinal:?} must not resolve to a unit"
            );
        }
    }

    /// The chooser's rows carry the stable id and no index, and every id they
    /// publish resolves back. A host built from this payload cannot persist an
    /// ordinal, because there is none in it.
    #[test]
    fn the_unit_chooser_publishes_ids_and_no_ordinal() {
        let json = measurement_units();
        let rows: serde_json::Value = serde_json::from_str(&json).expect("units are JSON");
        let rows = rows.as_array().expect("an array").clone();
        assert_eq!(rows.len(), MeasurementUnit::ALL.len());
        for row in &rows {
            let object = row.as_object().expect("an object");
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["decimalPlaces", "id", "stepTwip", "suffix"]);
            let id = object["id"].as_str().expect("an id");
            assert!(
                MeasurementUnit::from_id(id).is_some(),
                "the published id {id:?} must resolve"
            );
            assert!(object["stepTwip"].as_i64().unwrap_or_default() >= 1);
        }
        assert_eq!(
            rows[0]["id"], "cm",
            "the order is the engine's, metric first"
        );
    }

    /// **What the boundary shows, it reads back.** For every twip value and every
    /// unit: format it, parse what was shown, format that again — and the two
    /// strings are identical. A dialog that shows a value and reads the field back
    /// unchanged therefore cannot drift the number it was given, however many
    /// times the user opens it.
    ///
    /// Stated as a fixed point of `format`, and deliberately **not** as
    /// `parse(format(t)) == t`: the engine's display precision is the finest for
    /// which one step is still a whole twip, so a twip between two display steps
    /// is not representable and rounds to the nearer one. That rounding is honest —
    /// `decimal_places` exists so a shown digit always survives a round trip — and
    /// a guard demanding exact recovery of an unrepresentable value would be
    /// demanding a digit the product cannot honour. The second half bounds the
    /// rounding instead: it never moves further than one display step.
    ///
    /// The comma separator is a display choice only, so it must read back to the
    /// same twip as the dot.
    #[test]
    fn what_the_boundary_shows_it_reads_back() {
        for unit in MeasurementUnit::ALL {
            let id = unit.id();
            let step = unit.step().raw();
            // Every whole twip across a span wider than a page, so the values
            // BETWEEN display steps are covered rather than avoided.
            for twips in 0..2_000 {
                let shown = format_measurement_inner(twips, id, ".").expect("formats");
                let read = parse_measurement_inner(&shown, id).expect("parses");
                let again = format_measurement_inner(read, id, ".").expect("formats");
                assert_eq!(
                    again, shown,
                    "{id}: {twips} twips shown as {shown:?}, read back as {read}, \
                     shown again as {again:?}"
                );
                assert!(
                    (read - twips).abs() <= step,
                    "{id}: {twips} twips read back as {read}, further than one \
                     display step ({step}) away"
                );
                let comma = format_measurement_inner(twips, id, "comma").expect("formats");
                assert_eq!(
                    parse_measurement_inner(&comma, id),
                    Ok(read),
                    "{id}: the separator is a display choice, not a value change"
                );
            }
        }
    }

    /// The first-run default, and the fact that it is a region rule rather than a
    /// language rule.
    #[test]
    fn the_first_run_default_follows_the_region() {
        assert_eq!(default_measurement_unit("en-US"), "inch");
        assert_eq!(default_measurement_unit("us"), "inch");
        assert_eq!(default_measurement_unit("en_ca"), "inch");
        assert_eq!(default_measurement_unit("en-GB"), "cm");
        assert_eq!(default_measurement_unit("de-DE"), "cm");
        assert_eq!(decimal_separator_for_language("en-US"), ".");
        assert_eq!(decimal_separator_for_language("de-DE"), ",");
    }
}
