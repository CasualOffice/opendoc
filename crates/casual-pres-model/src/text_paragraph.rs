// SPDX-License-Identifier: Apache-2.0

//! DrawingML paragraphs and runs (`a:p`, `a:r`, `a:br`, `a:fld`).

use casual_doc_model::NodeId;
use casual_doc_model::v1::StyleColor;
use serde::{Deserialize, Serialize};

use crate::{
    MAX_SPACING_HUNDREDTHS, MAX_TEXT_LEVEL, PresentationError, TextBullet, TextCharacterProperties,
    Typeface,
};

/// The `ST_TextMargin` bound on `a:pPr@marL`/`@marR`, in EMU (0 to 51 inches).
pub const MAX_TEXT_MARGIN_EMU: i64 = 51_206_400;

/// Paragraph alignment (`a:pPr@algn`, `ST_TextAlignType`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    /// `l`, the default.
    #[default]
    Left,
    /// `ctr`.
    Center,
    /// `r`.
    Right,
    /// `just` — justified, last line left.
    Justify,
    /// `justLow` — Kashida-justified for Arabic.
    JustifyLow,
    /// `dist` — distributed, including the last line.
    Distribute,
    /// `thaiDist` — Thai distributed.
    ThaiDistribute,
}

impl TextAlign {
    /// The `a:pPr@algn` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Center => "ctr",
            Self::Right => "r",
            Self::Justify => "just",
            Self::JustifyLow => "justLow",
            Self::Distribute => "dist",
            Self::ThaiDistribute => "thaiDist",
        }
    }

    /// Reads an `a:pPr@algn` token; anything else is the default.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|align| align.token() == token)
            .unwrap_or(Self::Left)
    }

    /// Every alignment, so a guard derives the count.
    pub const ALL: [Self; 7] = [
        Self::Left,
        Self::Center,
        Self::Right,
        Self::Justify,
        Self::JustifyLow,
        Self::Distribute,
        Self::ThaiDistribute,
    ];
}

/// A spacing value (`a:lnSpc` / `a:spcBef` / `a:spcAft`), which DrawingML expresses
/// as **either** a percentage **or** an absolute measure — never both.
///
/// An enum rather than two optional fields because the two are mutually exclusive in
/// the schema, and a type that can hold both can hold a state no file can express.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "unit", rename_all = "camelCase")]
pub enum TextSpacing {
    /// `a:spcPct@val`, in thousandths of a percent — `100000` is single spacing.
    Percent {
        /// The value, in thousandths of a percent.
        thousandths: u32,
    },
    /// `a:spcPts@val`, in hundredths of a point.
    Points {
        /// The value, in hundredths of a point.
        hundredths: u32,
    },
}

/// A tab stop inside DrawingML text (`a:tabLst/a:tab`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextTabStop {
    /// `@pos`, in EMU from the text body's left inset.
    pub position_emu: i64,
    /// `@algn`.
    pub alignment: TextAlign,
}

/// Paragraph-level properties (`a:pPr`, and each `a:lvlNpPr` of an `a:lstStyle`).
///
/// Every field is optional and omitted when absent, for the same reason the character
/// properties are: absence inherits through the placeholder cascade, so writing a
/// default in place of "unset" freezes the value at the wrong tier.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextParagraphProperties {
    /// `a:pPr@lvl` — the ZERO-based outline level, `0..=8`. This is what selects the
    /// `a:lstStyle` level and the master's `p:txStyles` tier, so it is the single
    /// most load-bearing attribute on a slide paragraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// `a:pPr@algn`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<TextAlign>,
    /// `a:pPr@marL`, EMU — the indent of the whole paragraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_left_emu: Option<i64>,
    /// `a:pPr@marR`, EMU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_right_emu: Option<i64>,
    /// `a:pPr@indent`, EMU — the FIRST-LINE offset relative to `marL`, and negative
    /// for the hanging indent every bulleted paragraph uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent_emu: Option<i64>,
    /// `a:pPr@defTabSz`, EMU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tab_emu: Option<i64>,
    /// `a:pPr@rtl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_to_left: Option<bool>,
    /// `a:lnSpc`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<TextSpacing>,
    /// `a:spcBef`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_before: Option<TextSpacing>,
    /// `a:spcAft`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_after: Option<TextSpacing>,
    /// The bullet. `None` inherits; `Some(TextBullet::None)` suppresses an inherited
    /// one — see [`TextBullet`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet: Option<TextBullet>,
    /// `a:buClr` — the bullet's own colour, independent of the text's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet_color: Option<StyleColor>,
    /// `a:buSzPct@val`, in thousandths of a percent of the text size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet_size_percent: Option<u32>,
    /// `a:buFont` when it is declared separately from `a:buChar`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet_font: Option<Typeface>,
    /// `a:tabLst`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tab_stops: Vec<TextTabStop>,
    /// `a:defRPr` — the character properties runs in this paragraph inherit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_character: Option<Box<TextCharacterProperties>>,
}

impl TextParagraphProperties {
    /// Whether every field is unset, so `a:pPr` is omitted on write.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Checks the level bound, the margins and the nested default run properties.
    ///
    /// # Complexity
    ///
    /// O(1).
    pub fn validate(&self) -> Result<(), PresentationError> {
        if let Some(level) = self.level
            && level > MAX_TEXT_LEVEL
        {
            return Err(PresentationError::TextLevelOutOfRange(level));
        }
        for margin in [self.margin_left_emu, self.margin_right_emu]
            .into_iter()
            .flatten()
        {
            if !(0..=MAX_TEXT_MARGIN_EMU).contains(&margin) {
                return Err(PresentationError::TextMarginOutOfDomain(margin));
            }
        }
        if let Some(indent) = self.indent_emu
            && !(-MAX_TEXT_MARGIN_EMU..=MAX_TEXT_MARGIN_EMU).contains(&indent)
        {
            return Err(PresentationError::TextMarginOutOfDomain(indent));
        }
        if let Some(character) = self.default_character.as_deref() {
            check_character(character)?;
        }
        Ok(())
    }
}

/// Shared character-property domain check.
fn check_character(character: &TextCharacterProperties) -> Result<(), PresentationError> {
    if !character.size_in_domain() {
        return Err(PresentationError::FontSizeOutOfDomain(
            character.size_hundredths_point.unwrap_or_default(),
        ));
    }
    if let Some(spacing) = character.spacing_hundredths_point
        && !(-MAX_SPACING_HUNDREDTHS..=MAX_SPACING_HUNDREDTHS).contains(&spacing)
    {
        return Err(PresentationError::TextSpacingOutOfDomain(spacing));
    }
    Ok(())
}

/// A text run (`a:r`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRunText {
    /// Stable identity.
    pub id: NodeId,
    /// `a:rPr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Box<TextCharacterProperties>>,
    /// `a:t`. Non-empty: an empty run carries no text and no position, so it is
    /// refused rather than retained, matching the document model's `EmptyTextRun`.
    pub text: String,
}

/// A soft line break (`a:br`), which carries its own `a:rPr`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextLineBreak {
    /// Stable identity.
    pub id: NodeId,
    /// `a:rPr` — a break's own properties, which set the height of the blank line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Box<TextCharacterProperties>>,
}

/// A generated text field (`a:fld`) — a slide number or a date.
///
/// Carries both the field's kind and its **cached text**, and both are necessary: the
/// cache is what a reader that cannot evaluate the field displays, and discarding it
/// would blank every slide number in a deck opened by anything but PowerPoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextField {
    /// Stable identity.
    pub id: NodeId,
    /// `a:fld@id` — a GUID string the producer wrote, retained verbatim because
    /// PowerPoint matches fields by it.
    pub field_id: String,
    /// `a:fld@type`, e.g. `slidenum` or `datetime1`. Retained as the authored token
    /// rather than an enum: the datetime family alone has thirteen members whose
    /// formats are locale-dependent, and an unknown token must still round-trip.
    pub kind: String,
    /// `a:rPr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Box<TextCharacterProperties>>,
    /// `a:t` — the cached result.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// One child of a paragraph, in document order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TextRun {
    /// `a:r`.
    Run(TextRunText),
    /// `a:br`.
    LineBreak(TextLineBreak),
    /// `a:fld`.
    Field(TextField),
}

impl TextRun {
    /// This child's stable id.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        match self {
            Self::Run(run) => run.id,
            Self::LineBreak(brk) => brk.id,
            Self::Field(field) => field.id,
        }
    }

    /// The text this child contributes. A break contributes `\n`; a field
    /// contributes its cached result.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Run(run) => &run.text,
            Self::LineBreak(_) => "\n",
            Self::Field(field) => &field.text,
        }
    }

    /// This child's character properties, if it declares any.
    #[must_use]
    pub fn properties(&self) -> Option<&TextCharacterProperties> {
        match self {
            Self::Run(run) => run.properties.as_deref(),
            Self::LineBreak(brk) => brk.properties.as_deref(),
            Self::Field(field) => field.properties.as_deref(),
        }
    }
}

/// A DrawingML paragraph (`a:p`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextParagraph {
    /// Stable identity.
    pub id: NodeId,
    /// `a:pPr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Box<TextParagraphProperties>>,
    /// The runs, breaks and fields, in order. May be EMPTY: an empty `a:p` is how
    /// PowerPoint writes a blank line, so refusing it would refuse real files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<TextRun>,
    /// `a:endParaRPr` — the properties of the paragraph MARK, which is what the
    /// caret inherits when it sits at the end of the paragraph and what the next
    /// typed character takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_properties: Option<Box<TextCharacterProperties>>,
}

impl TextParagraph {
    /// An empty paragraph, which is what a blank line and an empty placeholder are.
    #[must_use]
    pub const fn empty(id: NodeId) -> Self {
        Self {
            id,
            properties: None,
            runs: Vec::new(),
            end_properties: None,
        }
    }

    /// The paragraph's zero-based outline level, defaulting to `0` when unstated —
    /// which is what the cascade does.
    #[must_use]
    pub fn level(&self) -> u8 {
        self.properties
            .as_deref()
            .and_then(|properties| properties.level)
            .unwrap_or(0)
    }

    /// The paragraph's plain text.
    ///
    /// # Complexity
    ///
    /// O(runs).
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(TextRun::text).collect()
    }

    /// Visits this paragraph's id and each child's id.
    ///
    /// # Complexity
    ///
    /// O(runs).
    pub fn visit_node_ids(&self, visit: &mut dyn FnMut(NodeId)) {
        visit(self.id);
        for run in &self.runs {
            visit(run.id());
        }
    }

    /// Checks the paragraph properties, every run's domains, and that no run is an
    /// empty `a:t`.
    ///
    /// # Complexity
    ///
    /// O(runs).
    pub fn validate(&self) -> Result<(), PresentationError> {
        if let Some(properties) = self.properties.as_deref() {
            properties.validate()?;
        }
        for run in &self.runs {
            if let TextRun::Run(text) = run
                && text.text.is_empty()
            {
                return Err(PresentationError::EmptyTextRun(text.id));
            }
            if let Some(character) = run.properties() {
                check_character(character)?;
            }
        }
        if let Some(end) = self.end_properties.as_deref() {
            check_character(end)?;
        }
        Ok(())
    }
}
