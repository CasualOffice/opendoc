//! User-facing measurement units — the one parse/format layer over the twip.
//!
//! The engine's internal unit is and stays the [`Twip`] ([`crate::units`]): every
//! measurement in the model, in layout and in the display list is an integer
//! 1/1440 inch, and nothing here changes that. What this module adds is the
//! **boundary** between that integer grid and the number a person types or reads
//! — in centimetres, millimetres, inches, points or picas.
//!
//! # Prior art
//!
//! This is the **Quantity** pattern (Fowler, *Analysis Patterns* / *Patterns of
//! Enterprise Application Architecture*): the same value object that `Money`
//! is, an amount paired with its unit, with conversion and rounding stated on
//! the type rather than spread across call sites. The dimensional-analysis
//! crates (`uom`, `dimensioned`) are the Rust expression of it; they are not
//! used here because exactly one dimension (length) and five units are in play,
//! and because the conversion has to be **exact integer arithmetic** rather than
//! `f64` scaling — see below.
//!
//! The alternative that was rejected is the one ONLYOFFICE uses, read from
//! their tree rather than guessed at: no quantity type at all, a global mutable
//! current-unit int, and a `fnRecalcToMM`/`fnRecalcFromMM` pair of bare
//! number-to-number helpers that pivot through **millimetres as an `f64`** and
//! read that global instead of taking the unit as an argument. Measured in
//! their `web-apps` checkout: 536 `fnRecalcFromMM` call sites, 329
//! `fnRecalcToMM`, 188 `getCurrentMetricName`, and 62 separate hand-written
//! `updateMetricUnit()` methods each iterating a hand-maintained spinner array.
//!
//! That shape is not a style difference, it is where their defects are, and
//! each one is a reason a rule for the whole product lives in one place:
//!
//! - **One rounding rule per call site drifts.** Their display rounding is a
//!   hard-coded `toFixed(2)` repeated at ~170 sites, so centimetres, points and
//!   inches are all shown to two places whether or not the grid supports it,
//!   and their two copies of the same conversion disagree (`toFixed(4)` in the
//!   shared helper, `toFixed(6)` in the spinner's own `_recalcUnits`).
//! - **A global current-unit means a widget can convert with the wrong
//!   factor.** `fnRecalcToMM` reads the app-wide setting rather than the
//!   field's, and their own `RightMenu.updateMetricUnit` has two panels
//!   commented out, so those panels keep a stale unit. Here the unit is an
//!   argument, always: there is no ambient state to get out of sync with.
//! - **A value that cannot be represented is silently rewritten.** Theirs
//!   truncates to two places, re-rounds through mm, then clamps to the
//!   spinner's range and merely fires an `inputerror` most dialogs do not
//!   listen to. Here every refusal is a typed [`QuantityError`] the chrome can
//!   show, and nothing is silently mutated.
//!
//! Their unit *set* is also smaller than Word's: `cmbUnit` offers three
//! (`cm`, `pt`, `inch`) and millimetres and picas exist only as suffixes their
//! spinner will accept while typing. All five are first class here, because
//! Word offers five and a document authored in millimetres is common.
//!
//! # Exact integer conversion, and why
//!
//! Every unit is an exact rational multiple of the twip, so conversion is done
//! in `i128` as `amount x num / den` with round-half-away-from-zero, never as
//! `f64` multiplication:
//!
//! | Unit | twips per unit | as a rational |
//! | --- | --- | --- |
//! | inch | 1440 | 1440 / 1 |
//! | point | 20 | 20 / 1 |
//! | pica (12 pt) | 240 | 240 / 1 |
//! | centimetre | 566.929133858... | 72000 / 127 |
//! | millimetre | 56.6929133858... | 7200 / 127 |
//!
//! `2.54 cm` is therefore *exactly* 1440 twips, not 1439.9999999999998 rounded
//! by luck. The whole point of the engine computing in twips is determinism
//! (`00-README.md`); a float boundary would have re-introduced
//! platform-dependent rounding at the one place a user can see it.
//!
//! # The round trip is honest, and bounded
//!
//! `unit -> twips` is **lossy in general**: the twip grid is 1/1440 inch, so
//! 0.001 cm has no representation. What is guaranteed, and guarded in
//! this module's `tests`, is the round trip *at display precision*:
//!
//! > For every value this module will ever **show**, parsing it and formatting
//! > it again returns the identical string.
//!
//! That holds because each unit's display precision is chosen so one display
//! step is at least one twip (see [`MeasurementUnit::decimal_places`]), which
//! makes the value -> twip map injective over displayable values. `2.54 cm ->
//! 1440 twips -> "2.54 cm"` exactly; `2.543 cm -> 1442 twips -> "2.54 cm"`,
//! which is a *stated* loss of the third decimal, not a claim of exactness.
//! The reverse trip (twips -> unit -> twips) is **not** exact and is not
//! claimed: 1441 twips is 2.5418 cm, shows as `2.54`, and comes back as 1440.
//! A host that needs the authored twip must keep the twip, which is why nothing
//! here is a setter.
//!
//! # Locale
//!
//! Parsing accepts **both** `.` and `,` as the decimal separator, always. That
//! is safe rather than sloppy because a group separator is *rejected* — `1,234`
//! is one thousand two hundred thirty four nowhere in this module, it is
//! `1.234` — so there is no ambiguity left to resolve with a locale. A German
//! user typing `2,54` and an American typing `2.54` both get 1440 twips, with
//! no locale plumbed into the engine at all.
//!
//! Formatting cannot be guessed that way, because the output is read, not
//! parsed, so it takes an explicit [`DecimalSeparator`]. The engine has no
//! locale of its own and must not infer one from the host process; the chrome
//! already resolves a locale for `docs/124` and passes the separator it implies.
//! Unit *names* are likewise not localised here — the abbreviations returned by
//! [`MeasurementUnit::suffix`] are the parse vocabulary, not display text; a
//! localised `cm` belongs in the chrome's own string catalogue.

use crate::units::Twip;

/// A unit a person can choose to work in (ONLYOFFICE's `cmbUnit`, Word's
/// *Measurement units*).
///
/// The twip is not a member: it is the engine's unit, never a user's.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum MeasurementUnit {
    /// Centimetres. The default outside the United States, and the reason this
    /// module exists.
    Centimetre,
    /// Millimetres.
    Millimetre,
    /// Inches — the engine's historical assumption, and still the US default.
    #[default]
    Inch,
    /// Typographic points (1/72 inch, 20 twips).
    Point,
    /// Picas (12 points, 240 twips).
    Pica,
}

impl MeasurementUnit {
    /// Every unit, in the order a chooser should offer them: metric coarse to
    /// fine, then imperial, then typographic.
    ///
    /// This is not ONLYOFFICE's order (their `cmbUnit` is `cm`, `pt`, `inch` and
    /// has no millimetre or pica entry at all) and it is not Word's either
    /// (Word leads with inches). It is ours, and the reason to state it here is
    /// that a chooser built from an *ordinal* would break the day the list
    /// changes — which is exactly what ONLYOFFICE's two disagreeing unit enums
    /// cost them, a remap ternary copy-pasted into five files. Persist
    /// [`MeasurementUnit::id`], never an index.
    ///
    /// O(1).
    pub const ALL: [Self; 5] = [
        Self::Centimetre,
        Self::Millimetre,
        Self::Inch,
        Self::Point,
        Self::Pica,
    ];

    /// The unit to default to for a host whose only signal is a region — the
    /// first-run default before a person has chosen.
    ///
    /// Inches for the United States and Canada, centimetres everywhere else,
    /// which is the rule ONLYOFFICE applies (their `Main.js` tests the region
    /// against `/^(ca|us)$/i` before falling back to centimetres) and matches
    /// the two countries whose paper sizes are imperial. Accepts a bare region
    /// code or a full BCP-47 tag, so `"us"`, `"en-US"` and `"en_us"` all give
    /// inches.
    ///
    /// O(1).
    #[must_use]
    pub fn for_region(region: &str) -> Self {
        let last = region.split(['-', '_']).next_back().unwrap_or(region);
        if last.eq_ignore_ascii_case("us") || last.eq_ignore_ascii_case("ca") {
            Self::Inch
        } else {
            Self::Centimetre
        }
    }

    /// The canonical abbreviation, and the token [`parse`](Quantity::parse)
    /// recognises as a suffix.
    ///
    /// O(1).
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Centimetre => "cm",
            Self::Millimetre => "mm",
            Self::Inch => "in",
            Self::Point => "pt",
            Self::Pica => "pi",
        }
    }

    /// A stable identifier for the unit, for a host that must persist the
    /// preference. Parsed back by [`MeasurementUnit::from_id`].
    ///
    /// O(1).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Centimetre => "cm",
            Self::Millimetre => "mm",
            Self::Inch => "inch",
            Self::Point => "point",
            Self::Pica => "pica",
        }
    }

    /// The unit with this [`id`](Self::id), or `None`. Case-insensitive over
    /// ASCII, and the abbreviations from [`suffix`](Self::suffix) are accepted
    /// too, so a persisted `"in"` and a persisted `"inch"` both resolve.
    ///
    /// O(1).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|unit| id.eq_ignore_ascii_case(unit.id()) || unit_token(id) == Some(*unit))
    }

    /// Twips per one of this unit, as the exact rational `(numerator,
    /// denominator)`. See the module header's table.
    ///
    /// O(1).
    #[must_use]
    const fn per_unit(self) -> (i128, i128) {
        match self {
            // 1 in = 2.54 cm, so 1 cm = 1440 / 2.54 = 72000 / 127 twips.
            Self::Centimetre => (72_000, 127),
            Self::Millimetre => (7_200, 127),
            Self::Inch => (1_440, 1),
            Self::Point => (20, 1),
            Self::Pica => (240, 1),
        }
    }

    /// How many decimal places this unit is **displayed** to.
    ///
    /// Not a taste choice: each value is the largest precision for which one
    /// display step is still at least one whole twip, which is what makes
    /// `parse -> format` a fixed point (module header, and the exhaustive guard
    /// in this module's `tests`). One step, in twips:
    ///
    /// | Unit | places | one step |
    /// | --- | --- | --- |
    /// | inch | 2 | 14.4 twips |
    /// | centimetre | 2 | 5.67 twips |
    /// | millimetre | 1 | 5.67 twips |
    /// | point | 1 | 2 twips |
    /// | pica | 2 | 2.4 twips |
    ///
    /// Millimetres get one place rather than two precisely because two would be
    /// 0.57 twips — a finer grid than the engine has — and the second decimal
    /// would be a digit the product cannot honour. Showing a digit that does
    /// not survive a round trip is the dishonest option, so it is not offered.
    ///
    /// O(1).
    #[must_use]
    pub const fn decimal_places(self) -> u8 {
        match self {
            Self::Centimetre | Self::Inch | Self::Pica => 2,
            Self::Millimetre | Self::Point => 1,
        }
    }

    /// One display step of this unit, in twips, rounded to the nearest whole twip
    /// and never below one — the natural `step` for a spinner in this unit.
    ///
    /// O(1).
    #[must_use]
    pub fn step(self) -> Twip {
        let (num, den) = self.per_unit();
        let scale = 10_i128.pow(u32::from(self.decimal_places()));
        let twips = div_round_half_away(num, den * scale);
        Twip(i32::try_from(twips.max(1)).unwrap_or(1))
    }
}

/// The character used to separate the whole and fractional parts when a value is
/// **formatted**. Parsing accepts both regardless (module header).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DecimalSeparator {
    /// `2.54` — English and most of Asia.
    #[default]
    Dot,
    /// `2,54` — most of continental Europe, and most of the world by population
    /// once India's dot is set aside.
    Comma,
}

impl DecimalSeparator {
    /// The separator character.
    ///
    /// O(1).
    #[must_use]
    pub const fn character(self) -> char {
        match self {
            Self::Dot => '.',
            Self::Comma => ',',
        }
    }

    /// The separator implied by a BCP-47 language tag, for a host that has a
    /// locale but no separator preference.
    ///
    /// This is a **coarse** mapping and is documented as such: it keys off the
    /// language subtag alone, so `es-ES` (comma) and `es-MX` (dot) are both
    /// reported as the language's majority convention. A host with CLDR data
    /// should pass the separator it already knows rather than call this.
    ///
    /// O(1).
    #[must_use]
    pub fn for_language_tag(tag: &str) -> Self {
        let language = tag
            .split(['-', '_'])
            .next()
            .unwrap_or(tag)
            .to_ascii_lowercase();
        // The dot languages, listed rather than the comma ones, because the
        // comma set is the larger and a language nobody listed should not be
        // silently given the US convention.
        const DOT: [&str; 12] = [
            "en", "ja", "zh", "ko", "th", "he", "hi", "bn", "ta", "ms", "ga", "mt",
        ];
        if DOT.contains(&language.as_str()) {
            Self::Dot
        } else {
            Self::Comma
        }
    }
}

/// Why a typed measurement could not be read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuantityError {
    /// The input held no digits (it was empty, or only a sign or a unit).
    Empty,
    /// A character that is not a digit, a sign, a decimal separator or a
    /// recognised unit suffix. A group separator lands here too: `1,234.5` is
    /// refused rather than guessed at.
    NotANumber,
    /// A unit suffix that is not one of [`MeasurementUnit::ALL`].
    UnknownUnit,
    /// More significant digits than the exact-integer conversion accepts (see
    /// [`MAX_DIGITS`]). A page measurement never needs them; refusing is better
    /// than overflowing.
    TooPrecise,
    /// The value converts to more twips than the engine's `i32` grid holds
    /// (about 1.49 million inches).
    OutOfRange,
}

impl core::fmt::Display for QuantityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::Empty => "no number",
            Self::NotANumber => "not a number",
            Self::UnknownUnit => "unknown unit",
            Self::TooPrecise => "too many digits",
            Self::OutOfRange => "out of range",
        };
        f.write_str(message)
    }
}

impl core::error::Error for QuantityError {}

/// The most significant digits [`Quantity::parse`] accepts, so the `i128`
/// conversion cannot overflow whatever is typed. 1.49 million inches is the
/// widest measurement the twip grid holds at all, which is seven digits before
/// the point; fifteen leaves eight for the fraction and still cannot overflow
/// `mantissa x 72000 x 10^places`.
pub const MAX_DIGITS: usize = 15;

/// A length a person typed or will read: an amount in a [`MeasurementUnit`].
///
/// A value object — [`Copy`], comparable only within a unit, and convertible to
/// the engine's [`Twip`] by exact integer arithmetic. The amount is kept as a
/// scaled integer (`mantissa / 10^places`) rather than an `f64` so that a typed
/// value survives to the twip conversion without a binary-floating-point step
/// (module header).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Quantity {
    /// The amount's digits, sign included, scaled by `10^places`.
    mantissa: i128,
    /// The decimal exponent of [`Self::mantissa`].
    places: u32,
    /// The unit the amount is in.
    unit: MeasurementUnit,
}

impl Quantity {
    /// The quantity `twips` measures, expressed in `unit` and **rounded to that
    /// unit's display precision** ([`MeasurementUnit::decimal_places`]).
    ///
    /// This is the display direction, so it rounds; it is not an inverse of
    /// [`Quantity::to_twips`] and the module header says so.
    ///
    /// O(1).
    #[must_use]
    pub fn from_twips(twips: Twip, unit: MeasurementUnit) -> Self {
        let places = u32::from(unit.decimal_places());
        let (num, den) = unit.per_unit();
        let scale = 10_i128.pow(places);
        // amount = twips * den / num, scaled up by 10^places before rounding so
        // the rounding happens at the displayed digit, not before it.
        let mantissa = div_round_half_away(i128::from(twips.raw()) * den * scale, num);
        Self {
            mantissa,
            places,
            unit,
        }
    }

    /// Reads a typed measurement.
    ///
    /// `default_unit` is the unit the value is in when the text carries no
    /// suffix — the host's current preference. A suffix **overrides** it, which
    /// is Word's behaviour and worth keeping: a user working in centimetres can
    /// type `0.5"` in a margin field and get half an inch.
    ///
    /// Accepted: optional leading/trailing space, an optional `+`/`-`, ASCII
    /// digits with at most one `.` or `,` separator, and an optional unit suffix
    /// (`cm`, `mm`, `in`, `"`, `pt`, `pi`, `pc`, and the long forms). Rejected:
    /// group separators, exponents, non-ASCII digits, and anything else — every
    /// one as a [`QuantityError`] the chrome can show, never as a silent zero.
    ///
    /// Complexity: O(n) in the input's length, with no allocation.
    pub fn parse(input: &str, default_unit: MeasurementUnit) -> Result<Self, QuantityError> {
        let text = input.trim();
        let mut unit = default_unit;
        let mut number = text;
        // Split a trailing unit off the end: the longest non-numeric tail.
        let digits_end = text
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == ',')
            .map_or(0, |(i, c)| i + c.len_utf8());
        if digits_end < text.len() {
            let tail = text[digits_end..].trim();
            if !tail.is_empty() {
                unit = unit_token(tail).ok_or(QuantityError::UnknownUnit)?;
            }
            number = text[..digits_end].trim_end();
        }

        let (negative, body) = match number.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, number.strip_prefix('+').unwrap_or(number)),
        };
        let body = body.trim_start();
        let mut mantissa: i128 = 0;
        let mut places: u32 = 0;
        let mut digits = 0usize;
        let mut seen_separator = false;
        let mut seen_digit = false;
        for character in body.chars() {
            match character {
                '0'..='9' => {
                    digits += 1;
                    if digits > MAX_DIGITS {
                        return Err(QuantityError::TooPrecise);
                    }
                    seen_digit = true;
                    mantissa = mantissa * 10 + i128::from(character as u8 - b'0');
                    if seen_separator {
                        places += 1;
                    }
                }
                '.' | ',' if !seen_separator => seen_separator = true,
                _ => return Err(QuantityError::NotANumber),
            }
        }
        if !seen_digit {
            return Err(QuantityError::Empty);
        }
        Ok(Self {
            mantissa: if negative { -mantissa } else { mantissa },
            places,
            unit,
        })
    }

    /// The unit this quantity is in.
    ///
    /// O(1).
    #[must_use]
    pub const fn unit(self) -> MeasurementUnit {
        self.unit
    }

    /// The amount, as the `f64` a host needs for a slider or a chart.
    ///
    /// Deliberately **not** the path to [`Twip`]s: [`Quantity::to_twips`] goes
    /// through integers instead, so nothing the user typed is rounded twice.
    ///
    /// O(1).
    #[must_use]
    pub fn amount(self) -> f64 {
        self.mantissa as f64 / 10_f64.powi(self.places as i32)
    }

    /// The engine measurement this quantity is, rounded half-away-from-zero to
    /// the nearest whole twip.
    ///
    /// `Err(QuantityError::OutOfRange)` when the value does not fit the engine's
    /// `i32` twip grid, so a paste of `999999999 cm` is refused rather than
    /// wrapped into a negative page width.
    ///
    /// O(1).
    pub fn to_twips(self) -> Result<Twip, QuantityError> {
        let (num, den) = self.unit.per_unit();
        let scale = 10_i128.pow(self.places);
        let twips = div_round_half_away(self.mantissa * num, den * scale);
        i32::try_from(twips)
            .map(Twip)
            .map_err(|_| QuantityError::OutOfRange)
    }

    /// The amount as text at the unit's display precision, using `separator`.
    ///
    /// No unit suffix and no grouping: the chrome owns the label beside the
    /// field, and grouping would produce a string this module refuses to read
    /// back.
    ///
    /// Complexity: O(d) in the number of digits; one allocation.
    #[must_use]
    pub fn format(self, separator: DecimalSeparator) -> String {
        let places = u32::from(self.unit.decimal_places());
        let scale = 10_i128.pow(places);
        // Re-round to the DISPLAY precision: a parsed `2.543 cm` shows as `2.54`.
        let scaled = if self.places == places {
            self.mantissa
        } else if self.places < places {
            self.mantissa * 10_i128.pow(places - self.places)
        } else {
            div_round_half_away(self.mantissa, 10_i128.pow(self.places - places))
        };
        let negative = scaled < 0;
        let magnitude = scaled.unsigned_abs();
        let whole = magnitude / scale.unsigned_abs();
        let mut out = String::new();
        if negative {
            out.push('-');
        }
        out.push_str(&whole.to_string());
        if places > 0 {
            let fraction = magnitude % scale.unsigned_abs();
            out.push(separator.character());
            let digits = fraction.to_string();
            for _ in 0..(places as usize).saturating_sub(digits.len()) {
                out.push('0');
            }
            out.push_str(&digits);
        }
        out
    }
}

/// Reads a typed measurement straight to twips — the call a dialog field makes.
///
/// `Quantity::parse(input, unit)?.to_twips()`, spelled once so no dialog has to
/// remember the order.
///
/// Complexity: O(n) in the input's length.
pub fn parse_twips(input: &str, unit: MeasurementUnit) -> Result<Twip, QuantityError> {
    Quantity::parse(input, unit)?.to_twips()
}

/// Writes an engine measurement as text in `unit` — the call a dialog field and
/// the ruler readout make.
///
/// Complexity: O(d) in the digit count; one allocation.
#[must_use]
pub fn format_twips(twips: Twip, unit: MeasurementUnit, separator: DecimalSeparator) -> String {
    Quantity::from_twips(twips, unit).format(separator)
}

/// The unit a suffix token names, or `None`.
///
/// Accepts the abbreviations, the `"` inch mark, the `pc` pica abbreviation
/// typography uses, and the spelled-out names with or without a plural `s`.
///
/// O(1) — a fixed table.
fn unit_token(token: &str) -> Option<MeasurementUnit> {
    let lower = token.trim().trim_end_matches('.').to_ascii_lowercase();
    // `inches` -> `inch`, `picas` -> `pica`; the `es` form is tried first so the
    // `ch` does not survive as `inche`.
    let singular = lower
        .strip_suffix("es")
        .or_else(|| lower.strip_suffix('s'))
        .unwrap_or(lower.as_str());
    match singular {
        "cm" | "centimeter" | "centimetre" => Some(MeasurementUnit::Centimetre),
        "mm" | "millimeter" | "millimetre" => Some(MeasurementUnit::Millimetre),
        "in" | "\"" | "inch" => Some(MeasurementUnit::Inch),
        "pt" | "point" => Some(MeasurementUnit::Point),
        "pi" | "pc" | "pica" => Some(MeasurementUnit::Pica),
        _ => None,
    }
}

/// `numerator / denominator`, rounded half away from zero, in `i128`.
///
/// Half away from zero (not Rust's truncation, and not banker's rounding) is
/// what a person expects of a measurement field: `0.5 pt` is 10 twips, and
/// `-0.5 pt` is -10, symmetrically. `denominator` is always positive here — it
/// is a unit denominator times a power of ten.
///
/// O(1).
const fn div_round_half_away(numerator: i128, denominator: i128) -> i128 {
    let negative = numerator < 0;
    let magnitude = numerator.unsigned_abs();
    let denominator = denominator.unsigned_abs();
    let rounded = (magnitude * 2 + denominator) / (denominator * 2);
    if negative {
        -(rounded as i128)
    } else {
        rounded as i128
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canonical_conversions_are_exact() {
        // 2.54 cm is one inch, EXACTLY: the claim the module header makes.
        assert_eq!(
            parse_twips("2.54", MeasurementUnit::Centimetre),
            Ok(Twip(1_440))
        );
        assert_eq!(
            parse_twips("25.4", MeasurementUnit::Millimetre),
            Ok(Twip(1_440))
        );
        assert_eq!(parse_twips("1", MeasurementUnit::Inch), Ok(Twip(1_440)));
        assert_eq!(parse_twips("72", MeasurementUnit::Point), Ok(Twip(1_440)));
        assert_eq!(parse_twips("6", MeasurementUnit::Pica), Ok(Twip(1_440)));
        // And a US-Letter page in centimetres is 21.59 x 27.94.
        assert_eq!(
            format_twips(
                Twip(12_240),
                MeasurementUnit::Centimetre,
                DecimalSeparator::Dot
            ),
            "21.59"
        );
        assert_eq!(
            format_twips(
                Twip(15_840),
                MeasurementUnit::Centimetre,
                DecimalSeparator::Dot
            ),
            "27.94"
        );
        // A4 in centimetres is 21.0 x 29.7 — the value a European user types.
        assert_eq!(
            parse_twips("21", MeasurementUnit::Centimetre),
            Ok(Twip(11_906))
        );
        assert_eq!(
            parse_twips("29.7", MeasurementUnit::Centimetre),
            Ok(Twip(16_838))
        );
    }

    #[test]
    fn the_round_trip_is_a_fixed_point_over_every_displayable_value() {
        // THE claim: for every value the product will ever show, in every unit,
        // format(parse(shown)) == shown. This is what makes the displayed
        // precision honest rather than decorative, and it is exhaustive over the
        // page-sized range rather than sampled.
        for unit in MeasurementUnit::ALL {
            let places = u32::from(unit.decimal_places());
            let scale = 10_i128.pow(places);
            // 0 to 100 units covers every page, margin, indent and tab stop a
            // document has, in every unit (100 in = 8.3 ft; 100 pt = 1.4 in).
            for step in 0..=(100 * scale) {
                let shown = Quantity {
                    mantissa: step,
                    places,
                    unit,
                }
                .format(DecimalSeparator::Dot);
                let twips = parse_twips(&shown, unit).expect("a displayed value parses");
                let again = format_twips(twips, unit, DecimalSeparator::Dot);
                assert_eq!(
                    again,
                    shown,
                    "{unit:?}: {shown} -> {} twips -> {again}",
                    twips.raw()
                );
            }
        }
    }

    #[test]
    fn a_finer_precision_than_the_twip_grid_would_break_that_fixed_point() {
        // WHY `decimal_places` is what it is, asserted rather than asserted-by-
        // comment: millimetres at TWO places (0.57 twips per step) is not a fixed
        // point, which is the reason the unit shows one place.
        let mut collisions = 0;
        for step in 0..1_000 {
            let a = Quantity {
                mantissa: step,
                places: 2,
                unit: MeasurementUnit::Millimetre,
            };
            let b = Quantity {
                mantissa: step + 1,
                places: 2,
                unit: MeasurementUnit::Millimetre,
            };
            if a.to_twips() == b.to_twips() {
                collisions += 1;
            }
        }
        assert!(
            collisions > 0,
            "two-decimal millimetres must collide on the twip grid; \
             if they no longer do, the grid changed and decimal_places should be revisited"
        );
        // Whereas at the shipped precision, adjacent displayable values never do.
        for unit in MeasurementUnit::ALL {
            let places = u32::from(unit.decimal_places());
            for step in 0..1_000 {
                let a = Quantity {
                    mantissa: step,
                    places,
                    unit,
                };
                let b = Quantity {
                    mantissa: step + 1,
                    places,
                    unit,
                };
                assert_ne!(
                    a.to_twips(),
                    b.to_twips(),
                    "{unit:?} step {step} collides at the shipped precision"
                );
            }
        }
    }

    #[test]
    fn the_reverse_trip_is_lossy_and_says_so() {
        // The header claims twips -> unit -> twips is NOT exact. Hold it to that,
        // so nobody later reads the fixed-point guard above as a claim of
        // exactness in both directions.
        let authored = Twip(1_441);
        let shown = format_twips(authored, MeasurementUnit::Centimetre, DecimalSeparator::Dot);
        assert_eq!(shown, "2.54");
        assert_eq!(
            parse_twips(&shown, MeasurementUnit::Centimetre),
            Ok(Twip(1_440))
        );
        assert_ne!(
            parse_twips(&shown, MeasurementUnit::Centimetre),
            Ok(authored)
        );
    }

    #[test]
    fn both_decimal_separators_parse_and_grouping_is_refused() {
        assert_eq!(
            parse_twips("2,54", MeasurementUnit::Centimetre),
            Ok(Twip(1_440))
        );
        assert_eq!(
            parse_twips("2.54", MeasurementUnit::Centimetre),
            Ok(Twip(1_440))
        );
        // A group separator is not silently taken as 1234: two separators is an
        // error, which is the only honest answer without a locale.
        assert_eq!(
            Quantity::parse("1,234.5", MeasurementUnit::Centimetre),
            Err(QuantityError::NotANumber)
        );
    }

    #[test]
    fn formatting_uses_the_separator_it_is_given() {
        assert_eq!(
            format_twips(
                Twip(1_440),
                MeasurementUnit::Centimetre,
                DecimalSeparator::Comma
            ),
            "2,54"
        );
        assert_eq!(
            format_twips(Twip(1_440), MeasurementUnit::Inch, DecimalSeparator::Dot),
            "1.00"
        );
        assert_eq!(
            format_twips(Twip(1_440), MeasurementUnit::Point, DecimalSeparator::Dot),
            "72.0"
        );
        assert_eq!(
            format_twips(Twip(-1_440), MeasurementUnit::Inch, DecimalSeparator::Dot),
            "-1.00"
        );
        // A negative that rounds to zero does not print "-0".
        assert_eq!(
            format_twips(Twip(0), MeasurementUnit::Inch, DecimalSeparator::Dot),
            "0.00"
        );
    }

    #[test]
    fn a_language_tag_picks_a_separator_coarsely_and_admits_it() {
        assert_eq!(
            DecimalSeparator::for_language_tag("en-US"),
            DecimalSeparator::Dot
        );
        assert_eq!(
            DecimalSeparator::for_language_tag("de-DE"),
            DecimalSeparator::Comma
        );
        assert_eq!(
            DecimalSeparator::for_language_tag("fr"),
            DecimalSeparator::Comma
        );
        assert_eq!(
            DecimalSeparator::for_language_tag("ja-JP"),
            DecimalSeparator::Dot
        );
        // The documented coarseness: es-MX really uses a dot, and this reports
        // the language's majority convention instead. Recorded, not hidden.
        assert_eq!(
            DecimalSeparator::for_language_tag("es-MX"),
            DecimalSeparator::Comma
        );
    }

    #[test]
    fn a_suffix_overrides_the_current_unit_the_way_word_does() {
        // Working in centimetres, typing an inch value.
        assert_eq!(
            parse_twips("0.5\"", MeasurementUnit::Centimetre),
            Ok(Twip(720))
        );
        assert_eq!(
            parse_twips("0.5 in", MeasurementUnit::Centimetre),
            Ok(Twip(720))
        );
        assert_eq!(
            parse_twips("12 pt", MeasurementUnit::Centimetre),
            Ok(Twip(240))
        );
        assert_eq!(parse_twips("1 pica", MeasurementUnit::Inch), Ok(Twip(240)));
        assert_eq!(parse_twips("10 MM", MeasurementUnit::Inch), Ok(Twip(567)));
        assert_eq!(
            parse_twips("2 inches", MeasurementUnit::Centimetre),
            Ok(Twip(2_880))
        );
        assert_eq!(
            Quantity::parse("3 furlongs", MeasurementUnit::Inch),
            Err(QuantityError::UnknownUnit)
        );
    }

    #[test]
    fn bad_input_is_an_error_not_a_zero() {
        // A dialog must be able to SAY something (SKILL §10); every one of these
        // used to be the silent 0 a bare `parseFloat` returns.
        for bad in ["", "   ", "-", "+", "abc", "1e3", "١٢", "1..2", "--1"] {
            assert!(
                Quantity::parse(bad, MeasurementUnit::Inch).is_err(),
                "{bad:?} must not parse"
            );
        }
        assert_eq!(
            Quantity::parse("1234567890123456", MeasurementUnit::Inch),
            Err(QuantityError::TooPrecise)
        );
        assert_eq!(
            parse_twips("99999999 in", MeasurementUnit::Inch),
            Err(QuantityError::OutOfRange)
        );
    }

    #[test]
    fn rounding_is_half_away_from_zero_and_symmetric() {
        // 0.5 pt = 10 twips exactly; 0.025 in = 36 twips exactly. Probe the tie.
        assert_eq!(parse_twips("0.5", MeasurementUnit::Point), Ok(Twip(10)));
        assert_eq!(parse_twips("-0.5", MeasurementUnit::Point), Ok(Twip(-10)));
        // 1 mm = 56.6929... twips -> 57; -1 mm -> -57.
        assert_eq!(parse_twips("1", MeasurementUnit::Millimetre), Ok(Twip(57)));
        assert_eq!(
            parse_twips("-1", MeasurementUnit::Millimetre),
            Ok(Twip(-57))
        );
        // 0.05 pt = 1 twip exactly; 0.024 pt = 0.48 twips -> 0.
        assert_eq!(parse_twips("0.05", MeasurementUnit::Point), Ok(Twip(1)));
        assert_eq!(parse_twips("0.024", MeasurementUnit::Point), Ok(Twip(0)));
    }

    #[test]
    fn a_unit_identifier_round_trips_for_a_host_that_persists_it() {
        for unit in MeasurementUnit::ALL {
            assert_eq!(MeasurementUnit::from_id(unit.id()), Some(unit));
            assert_eq!(MeasurementUnit::from_id(unit.suffix()), Some(unit));
        }
        assert_eq!(
            MeasurementUnit::from_id("INCH"),
            Some(MeasurementUnit::Inch)
        );
        assert_eq!(MeasurementUnit::from_id("cubit"), None);
    }

    #[test]
    fn a_region_picks_the_first_run_default() {
        assert_eq!(MeasurementUnit::for_region("us"), MeasurementUnit::Inch);
        assert_eq!(MeasurementUnit::for_region("en-US"), MeasurementUnit::Inch);
        assert_eq!(MeasurementUnit::for_region("en_ca"), MeasurementUnit::Inch);
        assert_eq!(
            MeasurementUnit::for_region("de-DE"),
            MeasurementUnit::Centimetre
        );
        assert_eq!(MeasurementUnit::for_region(""), MeasurementUnit::Centimetre);
        // `Default` stays Inch because that is what the engine assumed before
        // this module existed; a host that wants the world's default asks for it.
        assert_eq!(MeasurementUnit::default(), MeasurementUnit::Inch);
    }

    #[test]
    fn a_spinner_step_is_at_least_one_twip_in_every_unit() {
        for unit in MeasurementUnit::ALL {
            assert!(
                unit.step().raw() >= 1,
                "{unit:?} step must be a whole twip or more"
            );
        }
        assert_eq!(MeasurementUnit::Inch.step(), Twip(14));
        assert_eq!(MeasurementUnit::Centimetre.step(), Twip(6));
        assert_eq!(MeasurementUnit::Millimetre.step(), Twip(6));
        assert_eq!(MeasurementUnit::Point.step(), Twip(2));
        assert_eq!(MeasurementUnit::Pica.step(), Twip(2));
    }
}
