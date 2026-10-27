// SPDX-License-Identifier: Apache-2.0

//! Paragraph properties, lowered once (`docs/181` §4.2 step 2).
//!
//! A paragraph-formatting command is a [`ParagraphChange`] applied to each
//! target paragraph's properties, then offered to the caller's tracker (which
//! returns the properties unchanged outside Suggesting, and wraps them in a
//! `w:pPrChange` inside it), then written as one `SetParagraphProperties`
//! each. The editor's alignment, spacing, indent, style and list buttons and an
//! agent's `set_alignment`/`set_line_spacing`/`set_indent`/… all go through
//! [`set_properties`] with the same change, so they cannot drift.
//!
//! Which paragraphs a selection touches stays with the caller: the editor
//! resolves a caret selection (and its ruler skips table paragraphs), an agent
//! resolves an anchor.

use casual_doc_edit::Operation;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Alignment, Indentation, LineRule, NumberingRef, ParagraphProperties, StyleId,
};

/// The tracker a caller supplies: given the paragraph's current properties and
/// the changed ones, the properties to write (a tracked change when
/// Suggesting).
pub type Track<'a> = &'a mut dyn FnMut(
    &ParagraphProperties,
    ParagraphProperties,
) -> Result<ParagraphProperties, String>;

/// A tracker that tracks nothing: direct formatting.
///
/// # Errors
/// Never.
pub fn untracked(
    _current: &ParagraphProperties,
    next: ParagraphProperties,
) -> Result<ParagraphProperties, String> {
    Ok(next)
}

const NO_INDENT: Indentation = Indentation {
    start_twips: None,
    end_twips: None,
    first_line_twips: None,
    hanging_twips: None,
};

/// One paragraph-property change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParagraphChange {
    /// Horizontal alignment.
    Alignment(Alignment),
    /// Multiple line spacing (`w:lineRule="auto"`), percent of single.
    LineSpacingMultiple(u16),
    /// A fixed line height in twips: at least that (`atLeast`) or exactly it.
    LineSpacingFixed {
        /// The height, clamped at 0.
        twips: i32,
        /// `atLeast` rather than `exact`.
        at_least: bool,
    },
    /// Space before in twips; negative clears it back to the style's.
    SpaceBefore(i32),
    /// Space after in twips; negative clears it back to the style's.
    SpaceAfter(i32),
    /// Moves the start indent by this many twips, clamped at 0.
    AdjustIndent(i32),
    /// The start (left) indent in twips, clamped at 0.
    StartIndent(i32),
    /// The end (right) indent in twips, clamped at 0.
    EndIndent(i32),
    /// The first-line indent in twips: positive indents, negative hangs, 0
    /// clears both (they are mutually exclusive, as in Word).
    FirstLineIndent(i32),
    /// The paragraph style, or none.
    Style(Option<StyleId>),
    /// The list membership, or none.
    Numbering(Option<NumberingRef>),
    /// Moves a list item by this many levels, clamped to 0..=8; a paragraph
    /// that is not a list item is left alone.
    ListLevel(i32),
}

impl ParagraphChange {
    /// Applies the change to `p`. O(1).
    pub fn apply(self, p: &mut ParagraphProperties) {
        match self {
            Self::Alignment(alignment) => p.alignment = Some(alignment),
            Self::LineSpacingMultiple(percent) => {
                let mut spacing = p.spacing.unwrap_or_default();
                spacing.line_percent = Some(percent);
                // `auto` is the implicit-default rule.
                spacing.line_rule = None;
                spacing.line_twips = None;
                p.spacing = Some(spacing);
            }
            Self::LineSpacingFixed { twips, at_least } => {
                let mut spacing = p.spacing.unwrap_or_default();
                spacing.line_rule = Some(if at_least {
                    LineRule::AtLeast
                } else {
                    LineRule::Exact
                });
                spacing.line_twips = Some(twips.max(0));
                spacing.line_percent = None;
                p.spacing = Some(spacing);
            }
            Self::SpaceBefore(twips) => {
                let mut spacing = p.spacing.unwrap_or_default();
                if twips < 0 {
                    spacing.before_twips = None;
                    spacing.before_auto = None;
                } else {
                    spacing.before_twips = Some(twips);
                    spacing.before_auto = Some(false);
                }
                p.spacing = Some(spacing);
            }
            Self::SpaceAfter(twips) => {
                let mut spacing = p.spacing.unwrap_or_default();
                if twips < 0 {
                    spacing.after_twips = None;
                    spacing.after_auto = None;
                } else {
                    spacing.after_twips = Some(twips);
                    spacing.after_auto = Some(false);
                }
                p.spacing = Some(spacing);
            }
            Self::AdjustIndent(delta) => {
                let mut indent = p.indentation.unwrap_or(NO_INDENT);
                let current = indent.start_twips.unwrap_or(0);
                indent.start_twips = Some((current + delta).max(0));
                p.indentation = Some(indent);
            }
            Self::StartIndent(twips) => {
                let mut indent = p.indentation.unwrap_or(NO_INDENT);
                indent.start_twips = Some(twips.max(0));
                p.indentation = Some(indent);
            }
            Self::EndIndent(twips) => {
                let mut indent = p.indentation.unwrap_or(NO_INDENT);
                indent.end_twips = Some(twips.max(0));
                p.indentation = Some(indent);
            }
            Self::FirstLineIndent(twips) => {
                let mut indent = p.indentation.unwrap_or(NO_INDENT);
                if twips > 0 {
                    indent.first_line_twips = Some(twips);
                    indent.hanging_twips = None;
                } else if twips < 0 {
                    indent.hanging_twips = Some(-twips);
                    indent.first_line_twips = None;
                } else {
                    indent.first_line_twips = None;
                    indent.hanging_twips = None;
                }
                p.indentation = Some(indent);
            }
            Self::Style(style) => p.style_ref = style,
            Self::Numbering(numbering) => p.numbering = numbering,
            Self::ListLevel(delta) => {
                if let Some(mut numbering) = p.numbering {
                    let level = (i32::from(numbering.level) + delta).clamp(0, 8);
                    numbering.level = u8::try_from(level).unwrap_or(0);
                    p.numbering = Some(numbering);
                }
            }
        }
    }
}

/// An alignment by the editor's names: `center`, `end`/`right`, `justify`,
/// anything else start. O(1).
#[must_use]
pub fn alignment(name: &str) -> Alignment {
    match name {
        "center" => Alignment::Center,
        "end" | "right" => Alignment::End,
        "justify" => Alignment::Justify,
        _ => Alignment::Start,
    }
}

/// `leading` (operations that must come first, such as a list definition the
/// paragraphs are about to name), then one `SetParagraphProperties` per target
/// with `change` applied and offered to `track`. O(targets).
///
/// # Errors
/// When `track` refuses, or there is no target and nothing leading.
pub fn set_properties(
    leading: Vec<Operation>,
    targets: Vec<(NodeId, ParagraphProperties)>,
    change: impl Fn(&mut ParagraphProperties),
    track: Track<'_>,
) -> Result<Vec<Operation>, String> {
    let mut operations = leading;
    for (node, mut properties) in targets {
        let current = properties.clone();
        change(&mut properties);
        let properties = track(&current, properties)?;
        operations.push(Operation::SetParagraphProperties {
            node,
            properties: Box::new(properties),
        });
    }
    if operations.is_empty() {
        return Err("no paragraph in selection".to_owned());
    }
    Ok(operations)
}
