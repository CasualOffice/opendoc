//! CSS declarations built from typed, *resolved* properties.
//!
//! Every value here is re-serialized from a model field — a number, an enum, a
//! colour the palette resolved — never copied from document text. The two
//! places document text reaches a declaration at all, a font family name and a
//! list marker's label, go through [`font_name`] and [`css_string`], which keep
//! only what cannot end the declaration, the attribute or the `<style>`
//! element.

use std::collections::BTreeMap;

use casual_doc_layout::block::{BorderPattern, ResolvedEdge};
use casual_doc_layout::cascade::requested_font_family;
use casual_doc_layout::font_substitution::{DeclaredFamilies, substitute};
use casual_doc_layout::paint_values::{PaintPalette, auto_paragraph_space_twips};
use casual_doc_model::v1::{
    Alignment, BorderEdge, FontScheme, LineRule, ParagraphProperties, RunProperties, UnderlineStyle,
};

/// The size the page paints a run that declares none, in half-points: Word's
/// default body size (`casual_doc_layout::flow`'s run metrics).
pub(super) const DEFAULT_SIZE_HALF_POINTS: u32 = 22;

/// An ordered set of CSS declarations, keyed by property.
///
/// Ordered so the output is deterministic byte for byte: the same document
/// always exports to the same file.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Declarations(BTreeMap<&'static str, String>);

impl Declarations {
    pub(super) fn set(&mut self, property: &'static str, value: impl Into<String>) {
        self.0.insert(property, value.into());
    }

    pub(super) fn get(&self, property: &str) -> Option<&str> {
        self.0.get(property).map(String::as_str)
    }

    pub(super) fn remove(&mut self, property: &str) {
        self.0.remove(property);
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn extend(&mut self, other: &Self) {
        for (property, value) in &other.0 {
            self.0.insert(property, value.clone());
        }
    }

    /// What `self` says that `base` does not already say.
    ///
    /// A property `base` sets and `self` does not is reset to `initial`, so an
    /// element can *undo* what it would otherwise inherit — a run that is not
    /// bold inside a bold heading, a paragraph with no border in a bordered
    /// style. This is the whole of the delta encoding: a class carries a
    /// style's resolved declarations once, and each element carries only where
    /// it differs.
    pub(super) fn delta(&self, base: &Self) -> Self {
        let mut out = Self::default();
        for (property, value) in &self.0 {
            if base.0.get(property) != Some(value) {
                out.0.insert(property, value.clone());
            }
        }
        for property in base.0.keys() {
            if !self.0.contains_key(property) {
                out.0.insert(property, "initial".to_owned());
            }
        }
        out
    }

    /// `property:value;…`, with no trailing separator.
    pub(super) fn to_css(&self) -> String {
        let mut out = String::new();
        for (index, (property, value)) in self.0.iter().enumerate() {
            if index > 0 {
                out.push(';');
            }
            out.push_str(property);
            out.push(':');
            out.push_str(value);
        }
        out
    }
}

/// A length in points, rounded to hundredths, with no trailing zeros.
pub(super) fn points(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    // `-0` and `0` are the same length; print one of them.
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    format!("{rounded}pt")
}

/// Twips (twentieths of a point) as points.
pub(super) fn twips(value: i64) -> String {
    points(value as f64 / 20.0)
}

/// An opaque RGBA colour as `#rrggbb`.
pub(super) fn hex(rgba: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2])
}

/// A font family name reduced to characters that cannot end a quoted CSS
/// string, a declaration, an attribute or an element: letters, digits,
/// spaces, `-`, `_` and `.`. A name that keeps nothing is no name.
pub(super) fn font_name(name: &str) -> Option<String> {
    let kept: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        .collect();
    let kept = kept.trim().to_owned();
    (!kept.is_empty()).then_some(kept)
}

/// Text as a single-quoted CSS string: the quote, the backslash and line
/// breaks are escaped, and anything that could close a `<style>` element is
/// written as an escape rather than as itself.
pub(super) fn css_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        match c {
            '\'' | '\\' | '<' | '>' | '&' | '"' => {
                out.push_str(&format!("\\{:x} ", u32::from(c)));
            }
            '\n' | '\r' => out.push_str("\\a "),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// What the fonts a document names resolve to, for a `font-family` stack.
#[derive(Debug)]
pub(super) struct Fonts<'a> {
    pub(super) scheme: Option<&'a FontScheme>,
    pub(super) declared: DeclaredFamilies,
}

impl Fonts<'_> {
    /// The `font-family` stack for a resolved run: the family the document
    /// asks for, then the face the page actually draws it with when the
    /// reader's machine lacks it (a metric-compatible partner — Carlito for
    /// Calibri — where one exists), then the generic class. The East Asian
    /// face follows, so a browser that falls back per glyph finds it.
    pub(super) fn stack(&self, run: &RunProperties) -> String {
        let requested = requested_font_family(run, self.scheme).and_then(|name| font_name(&name));
        let mut families: Vec<String> = Vec::new();
        let mut generic = "sans-serif";
        if let Some(name) = &requested {
            families.push(name.clone());
            if let Some(found) = substitute(name, self.declared.kind_of(name)) {
                families.push(found.family.name.to_owned());
                generic = generic_of(found.family.name);
            }
        } else {
            families.push(casual_doc_layout::fonts::DEFAULT_FAMILY.name.to_owned());
        }
        let east_asian = run
            .font_ref_east_asia
            .as_ref()
            .and_then(|reference| match reference {
                casual_doc_model::v1::FontRef::Named(name) => font_name(&name.name),
                casual_doc_model::v1::FontRef::Theme(_) => None,
            });
        if let Some(name) = east_asian {
            families.push(name);
        }
        let mut seen = Vec::new();
        let mut out = String::new();
        for family in families {
            if seen.contains(&family) {
                continue;
            }
            out.push_str(&css_string(&family));
            out.push(',');
            seen.push(family);
        }
        out.push_str(generic);
        out
    }
}

/// The CSS generic class of a bundled family.
fn generic_of(bundled: &str) -> &'static str {
    let lower = bundled.to_ascii_lowercase();
    if lower.contains("mono") {
        "monospace"
    } else if lower.contains("serif") && !lower.contains("sans") || lower == "caladea" {
        "serif"
    } else {
        "sans-serif"
    }
}

/// A run's declarations, split by how CSS hands them on.
#[derive(Debug, Default)]
pub(super) struct RunCss {
    /// Declarations a child inherits (font, size, colour, weight…). A run only
    /// writes where these differ from its paragraph's.
    pub(super) inherited: Declarations,
    /// Declarations that paint the run's own box and are not inherited
    /// (decoration, background, baseline shift). A run always writes these.
    pub(super) own: Declarations,
}

/// The declarations for a fully resolved run.
pub(super) fn run_css(run: &RunProperties, fonts: &Fonts<'_>, palette: &PaintPalette) -> RunCss {
    let mut css = RunCss::default();
    let inherited = &mut css.inherited;
    inherited.set("font-family", fonts.stack(run));
    let size = run.size_half_points.unwrap_or(DEFAULT_SIZE_HALF_POINTS);
    inherited.set("font-size", points(f64::from(size) / 2.0));
    inherited.set("color", hex(palette.run_color(run.color)));
    inherited.set(
        "font-weight",
        if run.bold == Some(true) { "700" } else { "400" },
    );
    inherited.set(
        "font-style",
        if run.italic == Some(true) {
            "italic"
        } else {
            "normal"
        },
    );
    if run.all_caps == Some(true) {
        inherited.set("text-transform", "uppercase");
    }
    if run.small_caps == Some(true) {
        inherited.set("font-variant", "small-caps");
    }
    if let Some(spacing) = run.character_spacing_twips.filter(|value| *value != 0) {
        inherited.set("letter-spacing", twips(i64::from(spacing)));
    }

    let own = &mut css.own;
    let mut lines = Vec::new();
    if run.underline == Some(true) {
        lines.push("underline");
    }
    if run.strike == Some(true) || run.double_strike == Some(true) {
        lines.push("line-through");
    }
    if !lines.is_empty() {
        own.set("text-decoration-line", lines.join(" "));
        let style = if run.double_strike == Some(true) {
            Some("double")
        } else if run.underline == Some(true) {
            underline_style(run.underline_style.unwrap_or_default())
        } else {
            None
        };
        if let Some(style) = style {
            own.set("text-decoration-style", style);
        }
        if run.underline == Some(true)
            && let Some(color) = run.underline_color
        {
            own.set(
                "text-decoration-color",
                hex([color.r, color.g, color.b, 255]),
            );
        }
    }
    let background = run
        .highlight
        .and_then(|highlight| palette.highlight(highlight))
        .or_else(|| palette.shading(&run.shading));
    if let Some(background) = background {
        own.set("background-color", hex(background));
    }
    if let Some(position) = run.position_half_points.filter(|value| *value != 0) {
        own.set("vertical-align", points(f64::from(position) / 2.0));
    }
    if let Some(edge) = run.border.as_ref()
        && let Some(border) = border_css(palette, edge)
    {
        own.set("border", border);
    }
    css
}

/// The CSS line style for a `w:u` value, or `None` for a single line (the
/// default `solid`).
fn underline_style(style: UnderlineStyle) -> Option<&'static str> {
    match style {
        UnderlineStyle::Double => Some("double"),
        UnderlineStyle::Dotted => Some("dotted"),
        UnderlineStyle::Dashed | UnderlineStyle::DotDash => Some("dashed"),
        UnderlineStyle::Wavy => Some("wavy"),
        UnderlineStyle::Single | UnderlineStyle::Thick | UnderlineStyle::Words => None,
    }
}

/// One border edge as `width style colour`, resolved as the page draws it, or
/// `None` when the edge is not visible.
pub(super) fn border_css(palette: &PaintPalette, edge: &BorderEdge) -> Option<String> {
    palette
        .edge(&[Some(edge)])
        .map(|resolved| edge_css(&resolved))
}

/// A resolved edge as `width style colour`.
pub(super) fn edge_css(edge: &ResolvedEdge) -> String {
    let style = match edge.pattern {
        BorderPattern::Solid => "solid",
        BorderPattern::Double => "double",
        BorderPattern::Dotted => "dotted",
        BorderPattern::Dashed | BorderPattern::DotDash | BorderPattern::DotDotDash => "dashed",
    };
    // A double line needs room for two lines and a gap; the page draws it
    // three widths wide (`ResolvedEdge::band`), and so does CSS.
    let width = if edge.pattern == BorderPattern::Double {
        edge.width.raw() * 3
    } else {
        edge.width.raw()
    };
    format!(
        "{} {style} {}",
        twips(i64::from(width.max(1))),
        hex(edge.color)
    )
}

/// The declarations of a resolved paragraph's own box: alignment, indents,
/// line spacing, borders and fill. Not its vertical spacing — that depends on
/// its neighbours and is the writer's to place.
pub(super) fn paragraph_css(
    paragraph: &ParagraphProperties,
    palette: &PaintPalette,
) -> Declarations {
    let mut css = Declarations::default();
    if let Some(alignment) = paragraph.alignment {
        css.set(
            "text-align",
            match alignment {
                Alignment::Start => "start",
                Alignment::Center => "center",
                Alignment::End => "end",
                Alignment::Justify => "justify",
            },
        );
    }
    if paragraph.bidi == Some(true) {
        css.set("direction", "rtl");
    }
    if let Some(indentation) = &paragraph.indentation {
        if let Some(start) = indentation.start_twips.filter(|value| *value != 0) {
            css.set("margin-inline-start", twips(i64::from(start)));
        }
        if let Some(end) = indentation.end_twips.filter(|value| *value != 0) {
            css.set("margin-inline-end", twips(i64::from(end)));
        }
        // Hanging wins over first-line when both are present, as in Word.
        let first = match (indentation.hanging_twips, indentation.first_line_twips) {
            (Some(hanging), _) if hanging != 0 => Some(-i64::from(hanging)),
            (_, Some(first)) if first != 0 => Some(i64::from(first)),
            _ => None,
        };
        if let Some(first) = first {
            css.set("text-indent", twips(first));
        }
    }
    if let Some(spacing) = &paragraph.spacing {
        match (spacing.line_rule, spacing.line_twips, spacing.line_percent) {
            (Some(LineRule::Exact), Some(line), _) => {
                css.set("line-height", twips(i64::from(line)));
            }
            (Some(LineRule::AtLeast), Some(line), _) => {
                css.set(
                    "line-height",
                    format!("max({},{})", twips(i64::from(line)), LINE_EM),
                );
            }
            (_, _, Some(percent)) if percent != 100 => {
                css.set("line-height", line_multiple(f64::from(percent) / 100.0));
            }
            _ => {}
        }
    }
    let borders = &paragraph.borders;
    for (property, padding, edge) in [
        ("border-top", "padding-top", borders.top.as_ref()),
        ("border-bottom", "padding-bottom", borders.bottom.as_ref()),
        (
            "border-inline-start",
            "padding-inline-start",
            borders.start.as_ref(),
        ),
        (
            "border-inline-end",
            "padding-inline-end",
            borders.end.as_ref(),
        ),
    ] {
        let Some(edge) = edge else { continue };
        let Some(border) = border_css(palette, edge) else {
            continue;
        };
        css.set(property, border);
        if let Some(space) = edge.space_points.filter(|space| *space > 0) {
            css.set(padding, points(f64::from(space)));
        }
    }
    if let Some(fill) = palette.shading(&paragraph.shading) {
        css.set("background-color", hex(fill));
    }
    if paragraph.keep_next == Some(true) {
        css.set("break-after", "avoid");
    }
    if paragraph.keep_lines == Some(true) {
        css.set("break-inside", "avoid");
    }
    css
}

/// The line box of single spacing, as a multiple of the font size. A font's
/// own "normal" line is ascent, descent and gap — about 1.2 for the faces a
/// document names — and CSS cannot multiply `normal`, so a multiple is written
/// against this.
const SINGLE_LINE: f64 = 1.2;

/// [`SINGLE_LINE`] as a length, for the `atLeast` floor.
const LINE_EM: &str = "1.2em";

/// `w:spacing@w:line` as a multiple of single spacing, as `line-height`.
fn line_multiple(multiple: f64) -> String {
    let value = (multiple * SINGLE_LINE * 1000.0).round() / 1000.0;
    format!("{value}")
}

/// The space before and after a resolved paragraph, in twips: the authored
/// value, or the font-size-derived space `w:beforeAutospacing` and
/// `w:afterAutospacing` stand for, as the page computes it.
pub(super) fn paragraph_space(paragraph: &ParagraphProperties) -> (i64, i64) {
    let auto = i64::from(auto_paragraph_space_twips(paragraph));
    let Some(spacing) = paragraph.spacing.as_ref() else {
        return (0, 0);
    };
    let before = if spacing.before_auto == Some(true) {
        auto
    } else {
        i64::from(spacing.before_twips.unwrap_or(0))
    };
    let after = if spacing.after_auto == Some(true) {
        auto
    } else {
        i64::from(spacing.after_twips.unwrap_or(0))
    };
    (before.max(0), after.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delta_writes_only_differences_and_resets_what_it_drops() {
        let mut base = Declarations::default();
        base.set("font-weight", "700");
        base.set("color", "#000000");
        base.set("text-transform", "uppercase");
        let mut run = Declarations::default();
        run.set("font-weight", "400");
        run.set("color", "#000000");
        assert_eq!(
            run.delta(&base).to_css(),
            "font-weight:400;text-transform:initial"
        );
        assert!(base.delta(&base).is_empty());
    }

    #[test]
    fn points_round_and_drop_trailing_zeros() {
        assert_eq!(twips(240), "12pt");
        assert_eq!(twips(-360), "-18pt");
        assert_eq!(twips(1), "0.05pt");
        assert_eq!(points(37.795_275_59), "37.8pt");
        assert_eq!(points(-0.001), "0pt");
    }

    #[test]
    fn a_font_name_cannot_close_its_string_or_its_element() {
        assert_eq!(font_name("Calibri Light").as_deref(), Some("Calibri Light"));
        assert_eq!(
            font_name("x';}</style><script>").as_deref(),
            Some("xstylescript")
        );
        assert_eq!(font_name("';<>"), None);
    }

    #[test]
    fn a_css_string_escapes_what_would_end_it() {
        assert_eq!(css_string("1."), "'1.'");
        assert_eq!(css_string("a'b"), "'a\\27 b'");
        assert_eq!(css_string("</style>"), "'\\3c /style\\3e '");
    }
}
