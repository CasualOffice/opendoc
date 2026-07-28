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
//! reset; per-level `w:lvlRestart` is not yet modeled — see the crate deferrals).
//!
//! ## Deferrals
//!
//! - `w:lvlRestart` / non-default restart anchoring (model does not carry it).
//! - `w:numFmt` `cardinalText`/`ordinalText` and unknown tokens fall back to
//!   decimal (English word spelling is a follow-up).
//! - `lvlOverride`/`startOverride` are honored here but the importer does not yet
//!   populate them, so they are inert on imported documents.

use std::collections::HashMap;

use casual_doc_model::v1::{
    AbstractNumbering, Definitions, Indentation, LevelSuffix, NumberFormat, NumberingInstanceId,
    NumberingLevel, NumberingRef, RunProperties,
};

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
        let level = level_def(abstract_num, reference.level)?;

        // A per-instance start override (`w:startOverride`) wins over the level's
        // own `w:start`.
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

        let text = self.format_marker(reference.instance, abstract_num, level);

        Some(ResolvedMarker {
            text,
            run_properties: level.run_properties.clone(),
            suffix: level.suff.unwrap_or(LevelSuffix::Tab),
            level_indent: level
                .paragraph_properties
                .as_ref()
                .and_then(|p| p.indentation),
        })
    }

    /// Formats a level's marker text by substituting each `%n` placeholder in its
    /// `lvlText` with the current counter of level `n-1`, formatted through that
    /// level's `numFmt` (or forced to decimal when the current level is `isLgl`).
    /// A bullet level renders its `lvlText` glyph verbatim.
    fn format_marker(
        &self,
        instance: NumberingInstanceId,
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
                    out.push_str(&self.format_level_value(instance, abstract_num, level, target));
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
        instance: NumberingInstanceId,
        abstract_num: &AbstractNumbering,
        current: &NumberingLevel,
        target: u8,
    ) -> String {
        let target_level = level_def(abstract_num, target);
        let start = target_level.map_or(1, |l| l.start as u32);
        let value = self
            .counters
            .get(&(instance, target))
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
        // Word-spelled and unknown formats: decimal fallback (deferred).
        NumberFormat::CardinalText | NumberFormat::OrdinalText | NumberFormat::Other(_) => {
            value.to_string()
        }
    }
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
                // Marker overflows the hanging space: advance to the next default
                // tab stop strictly past the marker (Word's fallback behavior).
                let step = default_tab.raw().max(1);
                Twip(((after / step) + 1) * step)
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
            body_indent(LevelSuffix::Tab, Twip(-360), Twip(200), Twip(720)),
            Twip(0)
        );
        // Marker overflows the hanging space -> next default-tab multiple past it.
        assert_eq!(
            body_indent(LevelSuffix::Tab, Twip(-100), Twip(500), Twip(720)),
            Twip(720)
        );
        // Space suffix -> body immediately after the marker.
        assert_eq!(
            body_indent(LevelSuffix::Space, Twip(-360), Twip(200), Twip(720)),
            Twip(-160)
        );
    }

    // Counter behavior is exercised through a synthetic definition set: decimal
    // multi-level with resets, and a separate instance restarting.
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        AbstractNumbering, AbstractNumberingId, NumberingInstance, NumberingLevel,
        ParagraphProperties,
    };

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
        }
    }

    fn defs_with(
        levels: Vec<NumberingLevel>,
        instances: u8,
    ) -> (Definitions, Vec<NumberingInstanceId>) {
        let mut definitions = Definitions::default();
        let abs_id = AbstractNumberingId::new(NodeId::from_parts(1000, 1).unwrap());
        definitions
            .abstract_numbering
            .insert(abs_id, AbstractNumbering { levels });
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
