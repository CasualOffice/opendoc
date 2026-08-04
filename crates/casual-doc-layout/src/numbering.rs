//! List numbering and bullet marker engine.
//!
//! The v1 model fully carries numbering — a paragraph's `NumberingRef`
//! (`numId` + `ilvl`) and the [`AbstractNumbering`]/`NumberingInstance`/
//! [`NumberingLevel`] definitions
//! reachable through [`Definitions`] — but the flow engine produced no marker for
//! it: numbered headings (`1.`, `1.1`), bullet lists (`•`), and multi-level lists
//! (`4.1`, `a)`, `i.`) rendered with nothing where Word shows a marker. This module
//! is the missing piece: it tracks the per-list counters in document order,
//! formats each active level's value through its `numFmt` and `lvlText`, and hands
//! the flow engine a positioned glyph run to prepend to the paragraph's first line.
//!
//! The engine is deliberately split from [`crate::flow`]: [`NumberingState`] owns
//! the counter state and all number/format resolution ([`NumberingState::resolve`]),
//! while the flow engine owns font resolution (it builds the marker
//! [`StyledRun`](crate::text::StyledRun) through its existing cascade + resolver)
//! and calls back into [`PreparedMarker`] for the geometry (where the marker sits
//! relative to the hanging indent) and the injection into the shaped line.
//!
//! ## Counter model
//!
//! Counters are keyed by `(numId, ilvl)` — each numbering *instance* is an
//! independent list, so two paragraphs sharing a `numId` continue one sequence
//! (even across intervening non-list paragraphs — Word's "continued list"), while a
//! different `numId` referencing the same abstract definition restarts. Advancing a
//! level resets every deeper level of the same instance (the standard nesting
//! reset; per-level `w:lvlRestart` is not yet applied — see the deferrals).
//!
//! ## Effective level resolution
//!
//! The level a paragraph paints is resolved, not read straight off the abstract:
//! a per-instance `w:lvlOverride/w:lvl` full redefinition (its own
//! numFmt/lvlText/start/suff/justification and properties) replaces the abstract
//! level, and a `w:startOverride` still wins over that effective level's start.
//! This applies to substituted deeper levels (`%n`) too, so a multi-level marker
//! reflects each level's per-instance override.
//!
//! ## Deferrals
//!
//! - `w:lvlRestart` / non-default restart anchoring (the standard nesting reset is
//!   applied; per-level restart anchors are not).
//! - `w:numStyleLink` / `w:styleLink` List-Style level indirection.
//! - `w:numFmt` unknown/producer-specific tokens fall back to decimal
//!   (`cardinalText`/`ordinalText` are spelled out in English).

use std::collections::HashMap;

use casual_doc_model::v1::{
    AbstractNumbering, Definitions, Indentation, LevelSuffix, NumberFormat, NumberingInstanceId,
    NumberingLevel, NumberingRef, RunProperties, TabStop,
};
// Kept on a separate `use` line to minimize import-block merge conflicts.
use casual_doc_model::v1::NumberingInstance;

use crate::text::{GlyphRun, Line, LineLayout};
use crate::units::{Point, Twip};

/// The per-document numbering counter state, threaded through the flow in document
/// order so counters advance exactly once per numbered paragraph (including inside
/// tables and content controls, which recurse through the same flow path).
///
/// Intrinsic-width measurement passes flow paragraphs with a *throwaway* state, so
/// they never perturb the real counters.
#[derive(Clone, Debug, Default)]
pub struct NumberingState {
    /// `counters[(numId, ilvl)]` = the value last emitted for that level of that
    /// list instance.
    counters: HashMap<(NumberingInstanceId, u8), u32>,
}

/// The resolved marker for one numbered paragraph: the display text, the level's
/// run properties (the caller cascades and resolves the font — bullets often use
/// Symbol/Wingdings), the suffix between marker and body text, and the level's own
/// paragraph indentation (where the marker/body sit, applied below direct/style
/// indent).
#[derive(Clone, Debug)]
pub struct ResolvedMarker {
    /// The formatted marker string (e.g. `1.`, `4.1`, `a)`, or a bullet glyph).
    /// Empty when the level's format is `none` (the counter still advanced).
    pub text: String,
    /// The level's `w:rPr` (`None` when the level declares none); the flow engine
    /// resolves this through the cascade to size/color/face the marker.
    pub run_properties: Option<RunProperties>,
    /// The character between the marker and the body text (`w:suff`, default tab).
    pub suffix: LevelSuffix,
    /// The level's `w:pPr` indentation (`None` when the level declares none), merged
    /// below the paragraph's direct/style indentation by the caller.
    pub level_indent: Option<Indentation>,
    /// The level's `w:pPr/w:tabs` (empty when the level declares none). The caller
    /// unions these with the paragraph's own tab stops so the number's suffix tab
    /// advances to the list's authored tab stop, not only the default grid.
    pub level_tabs: Vec<TabStop>,
}

impl NumberingState {
    /// A fresh counter state (all lists at their start).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the counter for a paragraph referencing `(numId, ilvl)` and returns
    /// its resolved marker, or `None` when the reference does not resolve (a
    /// dangling `numId`/`ilvl` — the paragraph then flows with no marker).
    ///
    /// This mutates the counter state and must be called exactly once per numbered
    /// paragraph, in document order.
    pub fn resolve(
        &mut self,
        definitions: &Definitions,
        reference: &NumberingRef,
    ) -> Option<ResolvedMarker> {
        let instance = definitions.numbering.get(&reference.instance)?;
        let abstract_num = definitions.abstract_numbering.get(&instance.abstract_ref)?;
        // The effective level: a per-instance `w:lvlOverride/w:lvl` full
        // redefinition replaces the abstract level (format/text/start/suff/…).
        let level = effective_level(instance, abstract_num, reference.level)?;

        // A per-instance start override (`w:startOverride`) wins over the
        // effective level's own `w:start`.
        let start = instance
            .overrides
            .iter()
            .find(|o| o.level == reference.level)
            .and_then(|o| o.start)
            .unwrap_or(level.start) as u32;

        // Advance this level: first appearance starts at `start`, otherwise +1.
        let key = (reference.instance, reference.level);
        let value = self
            .counters
            .get(&key)
            .map_or(start, |v| v.saturating_add(1));
        self.counters.insert(key, value);
        // Entering (or re-touching) a level resets every deeper level of the same
        // instance, so the next time a deeper level appears it restarts at its start.
        self.counters
            .retain(|(inst, lvl), _| !(*inst == reference.instance && *lvl > reference.level));

        let text = self.format_marker(reference.instance, instance, abstract_num, level);

        Some(ResolvedMarker {
            text,
            run_properties: level.run_properties.clone(),
            suffix: level.suff.unwrap_or(LevelSuffix::Tab),
            level_indent: level
                .paragraph_properties
                .as_ref()
                .and_then(|p| p.indentation),
            level_tabs: level
                .paragraph_properties
                .as_ref()
                .map(|p| p.tabs.clone())
                .unwrap_or_default(),
        })
    }

    /// Formats a level's marker text by substituting each `%n` placeholder in its
    /// `lvlText` with the current counter of level `n-1`, formatted through that
    /// level's `numFmt` (or forced to decimal when the current level is `isLgl`).
    /// A bullet level renders its `lvlText` glyph verbatim.
    fn format_marker(
        &self,
        instance_id: NumberingInstanceId,
        instance: &NumberingInstance,
        abstract_num: &AbstractNumbering,
        level: &NumberingLevel,
    ) -> String {
        let template = level.lvl_text.as_deref().unwrap_or("");

        // A bullet (or any level whose text has no placeholder) is literal: emit the
        // glyph/text as-is. Numbered levels substitute their placeholders.
        if matches!(level.num_fmt, Some(NumberFormat::Bullet)) || !has_placeholder(template) {
            return template.to_string();
        }

        let mut out = String::with_capacity(template.len() + 2);
        let mut chars = template.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.peek().and_then(|d| d.to_digit(10)) {
                // `%1`..`%9` -> the counter of level (digit - 1).
                Some(digit) if (1..=9).contains(&digit) => {
                    chars.next();
                    let target = (digit - 1) as u8;
                    out.push_str(&self.format_level_value(
                        instance_id,
                        instance,
                        abstract_num,
                        level,
                        target,
                    ));
                }
                // A lone `%` (or `%0`) is a literal percent sign.
                _ => out.push('%'),
            }
        }
        out
    }

    /// Formats the current value of level `target` of `instance`: the tracked
    /// counter (falling back to that level's `start` when the superior level has not
    /// been seen), rendered with the target level's `numFmt` — unless the *current*
    /// level is `isLgl`, which forces every substituted value to decimal.
    fn format_level_value(
        &self,
        instance_id: NumberingInstanceId,
        instance: &NumberingInstance,
        abstract_num: &AbstractNumbering,
        current: &NumberingLevel,
        target: u8,
    ) -> String {
        let target_level = effective_level(instance, abstract_num, target);
        let start = target_level.map_or(1, |l| l.start as u32);
        let value = self
            .counters
            .get(&(instance_id, target))
            .copied()
            .unwrap_or(start);
        let fmt = if current.is_lgl {
            &NumberFormat::Decimal
        } else {
            target_level
                .and_then(|l| l.num_fmt.as_ref())
                .unwrap_or(&NumberFormat::Decimal)
        };
        format_number(value, fmt)
    }
}

/// The level a paragraph paints for `level` of `instance`: a per-instance
/// `w:lvlOverride/w:lvl` full redefinition when the instance carries one,
/// otherwise the abstract definition's level.
fn effective_level<'a>(
    instance: &'a NumberingInstance,
    abstract_num: &'a AbstractNumbering,
    level: u8,
) -> Option<&'a NumberingLevel> {
    instance
        .overrides
        .iter()
        .find(|o| o.level == level)
        .and_then(|o| o.definition.as_ref())
        .or_else(|| level_def(abstract_num, level))
}

/// Finds the definition for `level` in an abstract numbering (levels are stored in
/// document order, not necessarily dense or sorted).
fn level_def(abstract_num: &AbstractNumbering, level: u8) -> Option<&NumberingLevel> {
    abstract_num.levels.iter().find(|l| l.level == level)
}

/// Whether a `lvlText` contains a `%n` (n in 1..=9) counter placeholder.
fn has_placeholder(template: &str) -> bool {
    let bytes = template.as_bytes();
    bytes
        .windows(2)
        .any(|w| w[0] == b'%' && w[1].is_ascii_digit() && w[1] != b'0')
}

/// Formats a single counter value through a `w:numFmt`. Unsupported/word-spelling
/// formats fall back to decimal so a number always appears (see the module
/// deferrals).
fn format_number(value: u32, format: &NumberFormat) -> String {
    match format {
        // A bullet carries no counter value (its glyph is the level text and is
        // handled before formatting); nothing to render here.
        NumberFormat::None | NumberFormat::Bullet => String::new(),
        NumberFormat::Decimal => value.to_string(),
        NumberFormat::DecimalZero => {
            if value < 10 {
                format!("0{value}")
            } else {
                value.to_string()
            }
        }
        NumberFormat::LowerLetter => letters(value, false),
        NumberFormat::UpperLetter => letters(value, true),
        NumberFormat::LowerRoman => roman(value, false),
        NumberFormat::UpperRoman => roman(value, true),
        NumberFormat::Ordinal => ordinal(value),
        NumberFormat::CardinalText => cardinal_text(value),
        NumberFormat::OrdinalText => ordinal_text(value),
        // Unknown tokens: decimal fallback so the value is never lost.
        NumberFormat::Other(_) => value.to_string(),
    }
}

/// English cardinal words, title-cased as Word renders `cardinalText`
/// (`One`, `Twenty-Three`, `One Hundred Twenty-Three`). Outside `1..=999_999`
/// (Word's practical list range) it falls back to decimal so the value is kept.
fn cardinal_text(value: u32) -> String {
    if value == 0 || value > 999_999 {
        return value.to_string();
    }
    spell_cardinal(value)
}

/// English ordinal words, as Word renders `ordinalText` (`First`, `Twenty-Third`,
/// `One Hundredth`): the cardinal spelling with its final word ordinalized.
fn ordinal_text(value: u32) -> String {
    if value == 0 || value > 999_999 {
        return value.to_string();
    }
    let cardinal = spell_cardinal(value);
    // Ordinalize only the last word; a hyphenated compound (`Twenty-One`)
    // ordinalizes the part after the hyphen (`Twenty-First`).
    let (head, last) = match cardinal.rsplit_once(' ') {
        Some((head, last)) => (Some(head), last),
        None => (None, cardinal.as_str()),
    };
    let ordinal_last = match last.rsplit_once('-') {
        Some((prefix, unit)) => format!("{prefix}-{}", ordinalize_word(unit)),
        None => ordinalize_word(last),
    };
    match head {
        Some(head) => format!("{head} {ordinal_last}"),
        None => ordinal_last,
    }
}

const CARDINAL_ONES: [&str; 20] = [
    "",
    "One",
    "Two",
    "Three",
    "Four",
    "Five",
    "Six",
    "Seven",
    "Eight",
    "Nine",
    "Ten",
    "Eleven",
    "Twelve",
    "Thirteen",
    "Fourteen",
    "Fifteen",
    "Sixteen",
    "Seventeen",
    "Eighteen",
    "Nineteen",
];
const CARDINAL_TENS: [&str; 10] = [
    "", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety",
];

/// Spells `1..=999_999` in title-cased English cardinal words, American style (no
/// `and`): `One Hundred Twenty-Three`, `One Thousand Five`.
fn spell_cardinal(value: u32) -> String {
    if value >= 1000 {
        let thousands = value / 1000;
        let rest = value % 1000;
        let mut out = format!("{} Thousand", spell_below_thousand(thousands));
        if rest > 0 {
            out.push(' ');
            out.push_str(&spell_below_thousand(rest));
        }
        out
    } else {
        spell_below_thousand(value)
    }
}

/// Spells `1..=999` (`One Hundred`, `Twenty-Three`, `One Hundred Twenty-Three`).
fn spell_below_thousand(value: u32) -> String {
    let hundreds = (value / 100) as usize;
    let rest = value % 100;
    let mut parts = Vec::new();
    if hundreds > 0 {
        parts.push(format!("{} Hundred", CARDINAL_ONES[hundreds]));
    }
    if rest > 0 {
        parts.push(spell_below_hundred(rest));
    }
    parts.join(" ")
}

/// Spells `1..=99` (`Nineteen`, `Twenty`, `Twenty-Three`).
fn spell_below_hundred(value: u32) -> String {
    if value < 20 {
        CARDINAL_ONES[value as usize].to_string()
    } else {
        let tens = CARDINAL_TENS[(value / 10) as usize];
        let ones = (value % 10) as usize;
        if ones == 0 {
            tens.to_string()
        } else {
            format!("{tens}-{}", CARDINAL_ONES[ones])
        }
    }
}

/// The ordinal form of a single cardinal word (`One`→`First`, `Twenty`→
/// `Twentieth`, `Hundred`→`Hundredth`); an unrecognized word takes a `th` suffix.
fn ordinalize_word(word: &str) -> String {
    let mapped = match word {
        "One" => "First",
        "Two" => "Second",
        "Three" => "Third",
        "Four" => "Fourth",
        "Five" => "Fifth",
        "Six" => "Sixth",
        "Seven" => "Seventh",
        "Eight" => "Eighth",
        "Nine" => "Ninth",
        "Ten" => "Tenth",
        "Eleven" => "Eleventh",
        "Twelve" => "Twelfth",
        "Thirteen" => "Thirteenth",
        "Fourteen" => "Fourteenth",
        "Fifteen" => "Fifteenth",
        "Sixteen" => "Sixteenth",
        "Seventeen" => "Seventeenth",
        "Eighteen" => "Eighteenth",
        "Nineteen" => "Nineteenth",
        "Twenty" => "Twentieth",
        "Thirty" => "Thirtieth",
        "Forty" => "Fortieth",
        "Fifty" => "Fiftieth",
        "Sixty" => "Sixtieth",
        "Seventy" => "Seventieth",
        "Eighty" => "Eightieth",
        "Ninety" => "Ninetieth",
        "Hundred" => "Hundredth",
        "Thousand" => "Thousandth",
        other => return format!("{other}th"),
    };
    mapped.to_string()
}

/// Bijective base-26 letters: 1→a, 26→z, 27→aa, 28→ab, … (Word's `lowerLetter`).
/// `0` (an unusual start) renders as `0` so nothing is silently dropped.
fn letters(value: u32, upper: bool) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut n = value;
    let mut buf: Vec<u8> = Vec::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        buf.push(if upper { b'A' + rem } else { b'a' + rem });
        n = (n - 1) / 26;
    }
    buf.reverse();
    String::from_utf8(buf).expect("ascii letters are valid utf-8")
}

/// Roman numerals for 1..=3999 (Word's practical range); outside it, decimal so the
/// value is never lost.
fn roman(value: u32, upper: bool) -> String {
    if value == 0 || value > 3999 {
        return value.to_string();
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut n = value;
    let mut out = String::new();
    for (amount, glyph) in TABLE {
        while n >= amount {
            out.push_str(glyph);
            n -= amount;
        }
    }
    if upper { out.to_uppercase() } else { out }
}

/// English ordinal numeral (`1st`, `2nd`, `3rd`, `4th`, …) for `w:numFmt="ordinal"`.
fn ordinal(value: u32) -> String {
    let suffix = match (value % 10, value % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{value}{suffix}")
}

/// The x (twips, in the paragraph's block-relative coordinates where `0` is the
/// left/start indent) at which the body text begins after the marker, given the
/// marker's left edge `marker_x` and total advance `marker_width` and the level's
/// `suff`:
///
/// - **Tab** (Word's default): the body sits at the hanging stop — the left indent
///   (`0`) when the marker fits within the hanging space, otherwise the next
///   default-tab multiple past the marker so the two never overlap. (`0` is always a
///   default-tab multiple, so the hanging stop needs no explicit tab stop.)
/// - **Space/Nothing**: the body follows immediately after the marker (the space is
///   folded into the marker run by the caller), so the body starts at the marker's
///   right edge.
#[must_use]
pub fn body_indent(
    suffix: LevelSuffix,
    marker_x: Twip,
    marker_width: Twip,
    indent_start: Twip,
    tab_stops: &[TabStop],
    default_tab: Twip,
) -> Twip {
    match suffix {
        LevelSuffix::Space | LevelSuffix::Nothing => Twip(marker_x.raw() + marker_width.raw()),
        LevelSuffix::Tab => {
            let after = marker_x.raw() + marker_width.raw();
            if after <= 0 {
                // Marker fits within the hanging area: body at the left indent.
                Twip(0)
            } else {
                // Marker overflows the hanging space: the suffix tab advances to the
                // paragraph's/level's next explicit tab stop past the marker,
                // falling back to the default grid — the same resolution ordinary
                // tabs use. `marker_x`/`after` are indent-local; tab stops are in
                // margin coordinates, so translate across `indent_start`.
                let after_margin = after + indent_start.raw();
                let stop = crate::tabs::resolve_next_stop(after_margin, tab_stops, default_tab);
                Twip(stop.position - indent_start.raw())
            }
        }
    }
}

/// A marker shaped into glyph runs and positioned at its left edge, ready to be
/// prepended to a paragraph's first shaped line by [`PreparedMarker::inject`].
#[derive(Clone, Debug)]
pub struct PreparedMarker {
    /// The marker's glyph runs, already shifted to the marker's x; their baseline y
    /// is stamped onto the paragraph's first line at injection.
    runs: Vec<GlyphRun>,
    /// The marker's ascent/descent, used to synthesize a line box for an otherwise
    /// empty list paragraph (so a bullet with no text still shows).
    ascent: Twip,
    descent: Twip,
}

impl PreparedMarker {
    /// Builds a prepared marker from its shaped glyph runs (origins relative to `0`
    /// and on the shaped line's own baseline), shifting them to sit at `marker_x`.
    #[must_use]
    pub fn new(mut runs: Vec<GlyphRun>, marker_x: Twip, ascent: Twip, descent: Twip) -> Self {
        for run in &mut runs {
            run.origin = Point::new(Twip(run.origin.x.raw() + marker_x.raw()), run.origin.y);
            // Tag the marker glyphs so the host can locate the (interactive
            // checkbox) marker's rect while a caret click still lands in the body.
            run.is_marker = true;
        }
        Self {
            runs,
            ascent,
            descent,
        }
    }

    /// Prepends the marker glyphs to the first line of `layout`, aligning them to
    /// that line's baseline. When the body produced no line (an empty list item),
    /// synthesizes a line from the marker's own metrics so the marker still renders.
    pub fn inject(self, layout: &mut LineLayout, range: crate::model::ModelRange) {
        if self.runs.is_empty() {
            return;
        }
        if layout.lines.is_empty() {
            layout.lines.push(Line {
                runs: Vec::new(),
                ascent: self.ascent,
                descent: self.descent,
                height: Twip(self.ascent.raw() + self.descent.raw()),
                clip: false,
                range,
                line_break: crate::text::LineBreak::ParagraphEnd,
                page_break_after: false,
                bars: Vec::new(),
                images: Vec::new(),
                fields: Vec::new(),
                notes: Vec::new(),
                text_boxes: Vec::new(),
                rules: Vec::new(),
            });
        }
        let first = layout.lines.first_mut().expect("a line exists");
        let baseline = first.ascent;
        // Prepend so the marker paints first (position is by origin.x, so visual
        // order is unaffected; paint order is marker-then-body).
        let mut runs = self.runs;
        for run in &mut runs {
            run.origin = Point::new(run.origin.x, baseline);
        }
        runs.append(&mut first.runs);
        first.runs = runs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_and_decimal_zero() {
        assert_eq!(format_number(1, &NumberFormat::Decimal), "1");
        assert_eq!(format_number(42, &NumberFormat::Decimal), "42");
        assert_eq!(format_number(3, &NumberFormat::DecimalZero), "03");
        assert_eq!(format_number(10, &NumberFormat::DecimalZero), "10");
        assert_eq!(format_number(0, &NumberFormat::None), "");
    }

    #[test]
    fn lower_and_upper_letter_wrap_past_z() {
        assert_eq!(format_number(1, &NumberFormat::LowerLetter), "a");
        assert_eq!(format_number(26, &NumberFormat::LowerLetter), "z");
        assert_eq!(format_number(27, &NumberFormat::LowerLetter), "aa");
        assert_eq!(format_number(28, &NumberFormat::LowerLetter), "ab");
        assert_eq!(format_number(52, &NumberFormat::LowerLetter), "az");
        assert_eq!(format_number(3, &NumberFormat::UpperLetter), "C");
        assert_eq!(format_number(27, &NumberFormat::UpperLetter), "AA");
    }

    #[test]
    fn roman_numerals() {
        assert_eq!(format_number(1, &NumberFormat::LowerRoman), "i");
        assert_eq!(format_number(4, &NumberFormat::LowerRoman), "iv");
        assert_eq!(format_number(9, &NumberFormat::UpperRoman), "IX");
        assert_eq!(format_number(1994, &NumberFormat::UpperRoman), "MCMXCIV");
        assert_eq!(format_number(3, &NumberFormat::UpperRoman), "III");
    }

    #[test]
    fn ordinal_suffixes() {
        assert_eq!(format_number(1, &NumberFormat::Ordinal), "1st");
        assert_eq!(format_number(2, &NumberFormat::Ordinal), "2nd");
        assert_eq!(format_number(3, &NumberFormat::Ordinal), "3rd");
        assert_eq!(format_number(4, &NumberFormat::Ordinal), "4th");
        assert_eq!(format_number(11, &NumberFormat::Ordinal), "11th");
        assert_eq!(format_number(21, &NumberFormat::Ordinal), "21st");
    }

    #[test]
    fn cardinal_text_spells_the_number() {
        let c = |n| format_number(n, &NumberFormat::CardinalText);
        assert_eq!(c(1), "One");
        assert_eq!(c(15), "Fifteen");
        assert_eq!(c(21), "Twenty-One");
        assert_eq!(c(100), "One Hundred");
        assert_eq!(c(123), "One Hundred Twenty-Three");
        assert_eq!(c(1000), "One Thousand");
        assert_eq!(c(2025), "Two Thousand Twenty-Five");
        // Outside the spelled range: decimal, so the value is never lost.
        assert_eq!(c(0), "0");
        assert_eq!(c(1_000_000), "1000000");
    }

    #[test]
    fn ordinal_text_spells_the_ordinal() {
        let o = |n| format_number(n, &NumberFormat::OrdinalText);
        assert_eq!(o(1), "First");
        assert_eq!(o(2), "Second");
        assert_eq!(o(3), "Third");
        assert_eq!(o(5), "Fifth");
        assert_eq!(o(12), "Twelfth");
        assert_eq!(o(20), "Twentieth");
        assert_eq!(o(21), "Twenty-First");
        assert_eq!(o(23), "Twenty-Third");
        assert_eq!(o(100), "One Hundredth");
        assert_eq!(o(123), "One Hundred Twenty-Third");
        assert_eq!(o(1000), "One Thousandth");
        assert_eq!(o(0), "0");
    }

    #[test]
    fn placeholder_detection() {
        assert!(has_placeholder("%1."));
        assert!(has_placeholder("%1.%2"));
        assert!(!has_placeholder("\u{2022}")); // a bullet glyph
        assert!(!has_placeholder("Chapter")); // literal
        assert!(!has_placeholder("%0")); // not a level placeholder
    }

    #[test]
    fn body_indent_hanging_and_overflow() {
        // Marker fits within the hanging space (marker_x negative) -> body at 0.
        assert_eq!(
            body_indent(
                LevelSuffix::Tab,
                Twip(-360),
                Twip(200),
                Twip::ZERO,
                &[],
                Twip(720)
            ),
            Twip(0)
        );
        // Marker overflows the hanging space -> next default-tab multiple past it
        // (no explicit stops, zero indent: identical to the old default-grid path).
        assert_eq!(
            body_indent(
                LevelSuffix::Tab,
                Twip(-100),
                Twip(500),
                Twip::ZERO,
                &[],
                Twip(720)
            ),
            Twip(720)
        );
        // Space suffix -> body immediately after the marker.
        assert_eq!(
            body_indent(
                LevelSuffix::Space,
                Twip(-360),
                Twip(200),
                Twip::ZERO,
                &[],
                Twip(720)
            ),
            Twip(-160)
        );
    }

    #[test]
    fn body_indent_suffix_tab_honors_explicit_tab_stops() {
        use casual_doc_model::v1::TabAlignment;
        let stop = |pos| TabStop {
            position_twips: pos,
            alignment: TabAlignment::Start,
            leader: None,
        };
        // The paragraph sits at a 720-twip start indent; the marker overflows the
        // hanging space (after = 400 indent-local = 1120 in margin coords). An
        // explicit tab stop at margin 1440 wins over the default 720 grid, and the
        // result is translated back to indent-local (1440 - 720 = 720).
        assert_eq!(
            body_indent(
                LevelSuffix::Tab,
                Twip(-100),
                Twip(500),
                Twip(720),
                &[stop(1440)],
                Twip(720),
            ),
            Twip(720)
        );
        // With no explicit stop past the marker, the default grid is margin-aligned:
        // next 720 multiple past margin-1120 is 1440 -> indent-local 720.
        assert_eq!(
            body_indent(
                LevelSuffix::Tab,
                Twip(-100),
                Twip(500),
                Twip(720),
                &[],
                Twip(720)
            ),
            Twip(720)
        );
        // A stop before the marker is ignored; the next one past it is taken.
        assert_eq!(
            body_indent(
                LevelSuffix::Tab,
                Twip(-100),
                Twip(500),
                Twip(720),
                &[stop(200), stop(1600)],
                Twip(720),
            ),
            Twip(880)
        );
    }

    // Counter behavior is exercised through a synthetic definition set: decimal
    // multi-level with resets, and a separate instance restarting.
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        AbstractNumbering, AbstractNumberingId, NumberingInstance, NumberingLevel,
        ParagraphProperties,
    };
    // Separate `use` line to minimize import-block merge conflicts.
    use casual_doc_model::v1::NumberingOverride;

    fn lvl(level: u8, fmt: NumberFormat, text: &str) -> NumberingLevel {
        NumberingLevel {
            level,
            start: 1,
            num_fmt: Some(fmt),
            lvl_text: Some(text.to_string()),
            lvl_jc: None,
            suff: None,
            is_lgl: false,
            paragraph_properties: None,
            run_properties: None,
            style_ref: None,
            lvl_restart: None,
            pstyle: None,
        }
    }

    fn defs_with(
        levels: Vec<NumberingLevel>,
        instances: u8,
    ) -> (Definitions, Vec<NumberingInstanceId>) {
        let mut definitions = Definitions::default();
        let abs_id = AbstractNumberingId::new(NodeId::from_parts(1000, 1).unwrap());
        definitions.abstract_numbering.insert(
            abs_id,
            AbstractNumbering {
                levels,
                multi_level_type: None,
                num_style_link: None,
                style_link: None,
            },
        );
        let mut ids = Vec::new();
        for i in 0..instances {
            let inst = NumberingInstanceId::new(NodeId::from_parts(2000 + i as u64, 1).unwrap());
            definitions.numbering.insert(
                inst,
                NumberingInstance {
                    abstract_ref: abs_id,
                    overrides: Vec::new(),
                },
            );
            ids.push(inst);
        }
        (definitions, ids)
    }

    /// One instance whose abstract carries `levels` and that itself carries the
    /// per-instance `overrides` (`w:lvlOverride`).
    fn defs_with_overrides(
        levels: Vec<NumberingLevel>,
        overrides: Vec<NumberingOverride>,
    ) -> (Definitions, NumberingInstanceId) {
        let mut definitions = Definitions::default();
        let abs_id = AbstractNumberingId::new(NodeId::from_parts(1000, 1).unwrap());
        definitions.abstract_numbering.insert(
            abs_id,
            AbstractNumbering {
                levels,
                multi_level_type: None,
                num_style_link: None,
                style_link: None,
            },
        );
        let inst = NumberingInstanceId::new(NodeId::from_parts(2000, 1).unwrap());
        definitions.numbering.insert(
            inst,
            NumberingInstance {
                abstract_ref: abs_id,
                overrides,
            },
        );
        (definitions, inst)
    }

    fn num_ref(instance: NumberingInstanceId, level: u8) -> NumberingRef {
        NumberingRef { instance, level }
    }

    fn marker_text(state: &mut NumberingState, defs: &Definitions, r: &NumberingRef) -> String {
        state.resolve(defs, r).expect("resolves").text
    }

    #[test]
    fn multi_level_counters_reset_deeper_levels() {
        let (defs, ids) = defs_with(
            vec![
                lvl(0, NumberFormat::Decimal, "%1."),
                lvl(1, NumberFormat::LowerLetter, "%2)"),
                lvl(2, NumberFormat::LowerRoman, "%3."),
            ],
            1,
        );
        let n = ids[0];
        let mut s = NumberingState::new();
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 0)), "1.");
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 0)), "2.");
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 1)), "a)");
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 1)), "b)");
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 2)), "i.");
        // Back up to level 0: increments it and RESETS deeper levels.
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 0)), "3.");
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 1)), "a)");
    }

    #[test]
    fn separate_instances_have_independent_counters() {
        let (defs, ids) = defs_with(vec![lvl(0, NumberFormat::Decimal, "%1.")], 2);
        let mut s = NumberingState::new();
        assert_eq!(marker_text(&mut s, &defs, &num_ref(ids[0], 0)), "1.");
        assert_eq!(marker_text(&mut s, &defs, &num_ref(ids[0], 0)), "2.");
        // A different instance (numId) restarts, even sharing the abstract def.
        assert_eq!(marker_text(&mut s, &defs, &num_ref(ids[1], 0)), "1.");
        // The first instance continues where it left off.
        assert_eq!(marker_text(&mut s, &defs, &num_ref(ids[0], 0)), "3.");
    }

    #[test]
    fn full_level_override_replaces_abstract_level() {
        // Abstract level 0 is decimal `%1.`; the instance's `w:lvlOverride/w:lvl`
        // redefines it to upperRoman `%1)` starting at 3 — the paragraph must
        // paint the override's format/text/start, not the abstract default.
        let mut definition = lvl(0, NumberFormat::UpperRoman, "%1)");
        definition.start = 3;
        let (defs, inst) = defs_with_overrides(
            vec![lvl(0, NumberFormat::Decimal, "%1.")],
            vec![NumberingOverride {
                level: 0,
                start: None,
                definition: Some(definition),
            }],
        );
        let mut s = NumberingState::new();
        assert_eq!(marker_text(&mut s, &defs, &num_ref(inst, 0)), "III)");
        assert_eq!(marker_text(&mut s, &defs, &num_ref(inst, 0)), "IV)");
    }

    #[test]
    fn start_override_wins_over_override_definition_start() {
        // A `w:startOverride` (5) still wins over the override level's own start (3).
        let mut definition = lvl(0, NumberFormat::UpperRoman, "%1)");
        definition.start = 3;
        let (defs, inst) = defs_with_overrides(
            vec![lvl(0, NumberFormat::Decimal, "%1.")],
            vec![NumberingOverride {
                level: 0,
                start: Some(5),
                definition: Some(definition),
            }],
        );
        let mut s = NumberingState::new();
        assert_eq!(marker_text(&mut s, &defs, &num_ref(inst, 0)), "V)");
    }

    #[test]
    fn override_definition_applies_to_substituted_deeper_level() {
        // The instance overrides level 0's format to upperRoman; a level-1 marker
        // substituting `%1` must reflect the override's format, not the abstract's.
        let over0 = lvl(0, NumberFormat::UpperRoman, "%1.");
        let (defs, inst) = defs_with_overrides(
            vec![
                lvl(0, NumberFormat::Decimal, "%1."),
                lvl(1, NumberFormat::Decimal, "%1.%2"),
            ],
            vec![NumberingOverride {
                level: 0,
                start: None,
                definition: Some(over0),
            }],
        );
        let mut s = NumberingState::new();
        assert_eq!(marker_text(&mut s, &defs, &num_ref(inst, 0)), "I.");
        // `%1` uses level 0's overridden upperRoman; `%2` uses level 1's decimal.
        assert_eq!(marker_text(&mut s, &defs, &num_ref(inst, 1)), "I.1");
    }

    #[test]
    fn is_lgl_forces_decimal_for_all_substituted_levels() {
        let mut levels = vec![
            lvl(0, NumberFormat::UpperRoman, "%1."),
            lvl(1, NumberFormat::LowerLetter, "%1.%2"),
        ];
        levels[1].is_lgl = true;
        let (defs, ids) = defs_with(levels, 1);
        let n = ids[0];
        let mut s = NumberingState::new();
        // Level 0 renders with its own upperRoman format.
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 0)), "I.");
        // Level 1 is isLgl: BOTH %1 and %2 render as decimal.
        assert_eq!(marker_text(&mut s, &defs, &num_ref(n, 1)), "1.1");
    }

    #[test]
    fn bullet_level_emits_its_glyph_verbatim() {
        let (defs, ids) = defs_with(vec![lvl(0, NumberFormat::Bullet, "\u{2022}")], 1);
        let mut s = NumberingState::new();
        // A bullet is literal: no substitution, same glyph every item.
        assert_eq!(marker_text(&mut s, &defs, &num_ref(ids[0], 0)), "\u{2022}");
        assert_eq!(marker_text(&mut s, &defs, &num_ref(ids[0], 0)), "\u{2022}");
    }

    #[test]
    fn level_indent_is_surfaced_for_merging() {
        let mut levels = vec![lvl(0, NumberFormat::Decimal, "%1.")];
        levels[0].paragraph_properties = Some(ParagraphProperties {
            indentation: Some(Indentation {
                start_twips: Some(720),
                end_twips: None,
                first_line_twips: None,
                hanging_twips: Some(360),
            }),
            ..ParagraphProperties::default()
        });
        let (defs, ids) = defs_with(levels, 1);
        let mut s = NumberingState::new();
        let resolved = s.resolve(&defs, &num_ref(ids[0], 0)).expect("resolves");
        let indent = resolved.level_indent.expect("level indent");
        assert_eq!(indent.start_twips, Some(720));
        assert_eq!(indent.hanging_twips, Some(360));
    }
}
