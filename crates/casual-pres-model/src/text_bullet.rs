// SPDX-License-Identifier: Apache-2.0

//! DrawingML bullets and auto-numbering (`a:buNone` / `a:buChar` / `a:buAutoNum`).
//!
//! # The structural divergence from a document, stated once
//!
//! A WordprocessingML list is a REFERENCE: `w:numPr` names an instance id, the
//! instance names an abstract definition, and the definition carries nine levels of
//! format. That indirection is why the document model has `Definitions::numbering`
//! and a `DanglingNumberingRef` failure at all.
//!
//! DrawingML has no such table. The bullet is stated **inline, on the paragraph's own
//! `a:pPr`**, and the only inheritance is the placeholder cascade that carries every
//! other paragraph property. So there is nothing to dangle, no instance to resolve,
//! and no numbering definitions to import — and conversely, two paragraphs that look
//! like one list are only a list because their `a:pPr` agree and their `@lvl` nest.
//!
//! This is the single most important reason a presentation is not a document with a
//! different page size: list identity does not exist in the file, so it cannot be
//! modeled as a reference without inventing it.

use serde::{Deserialize, Serialize};

use crate::Typeface;

/// The numbering scheme of an auto-numbered bullet (`a:buAutoNum@type`,
/// `ST_TextAutonumberScheme`).
///
/// All forty-one values. Enumerating families rather than successes (`SKILL` §9.3):
/// a scheme this build cannot render must still round-trip, because a deck numbered
/// in Thai or Hebrew that reopens numbered in Arabic has been silently rewritten.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub enum AutoNumberScheme {
    /// `arabicPeriod` — 1., 2., 3. The schema has no default; this is Word's and
    /// PowerPoint's, and what an unrecognized token degrades to.
    #[default]
    ArabicPeriod,
    /// `arabicParenR` — 1), 2), 3).
    ArabicParenR,
    /// `arabicParenBoth` — (1), (2), (3).
    ArabicParenBoth,
    /// `arabicPlain` — 1, 2, 3.
    ArabicPlain,
    /// `alphaLcPeriod` — a., b., c.
    AlphaLcPeriod,
    /// `alphaUcPeriod` — A., B., C.
    AlphaUcPeriod,
    /// `alphaLcParenR` — a), b), c).
    AlphaLcParenR,
    /// `alphaUcParenR` — A), B), C).
    AlphaUcParenR,
    /// `alphaLcParenBoth` — (a), (b), (c).
    AlphaLcParenBoth,
    /// `alphaUcParenBoth` — (A), (B), (C).
    AlphaUcParenBoth,
    /// `romanLcPeriod` — i., ii., iii.
    RomanLcPeriod,
    /// `romanUcPeriod` — I., II., III.
    RomanUcPeriod,
    /// `romanLcParenR` — i), ii), iii).
    RomanLcParenR,
    /// `romanUcParenR` — I), II), III).
    RomanUcParenR,
    /// `romanLcParenBoth` — (i), (ii), (iii).
    RomanLcParenBoth,
    /// `romanUcParenBoth` — (I), (II), (III).
    RomanUcParenBoth,
    /// `circleNumDbPlain` — double-byte circled numbers.
    CircleNumDbPlain,
    /// `circleNumWdBlackPlain` — Wingdings black circled numbers.
    CircleNumWdBlackPlain,
    /// `circleNumWdWhitePlain` — Wingdings white circled numbers.
    CircleNumWdWhitePlain,
    /// `arabicDbPeriod` — double-byte Arabic with a period.
    ArabicDbPeriod,
    /// `arabicDbPlain` — double-byte Arabic.
    ArabicDbPlain,
    /// `arabic1Minus` — Arabic-Indic with a trailing minus.
    Arabic1Minus,
    /// `arabic2Minus` — Arabic-Indic (Abjad) with a trailing minus.
    Arabic2Minus,
    /// `hebrew2Minus` — Hebrew with a trailing minus.
    Hebrew2Minus,
    /// `ea1ChsPeriod` — Simplified Chinese with a period.
    Ea1ChsPeriod,
    /// `ea1ChsPlain` — Simplified Chinese.
    Ea1ChsPlain,
    /// `ea1ChtPeriod` — Traditional Chinese with a period.
    Ea1ChtPeriod,
    /// `ea1ChtPlain` — Traditional Chinese.
    Ea1ChtPlain,
    /// `ea1JpnChsDbPeriod` — Japanese/Simplified Chinese double-byte with a period.
    Ea1JpnChsDbPeriod,
    /// `ea1JpnKorPlain` — Japanese/Korean.
    Ea1JpnKorPlain,
    /// `ea1JpnKorPeriod` — Japanese/Korean with a period.
    Ea1JpnKorPeriod,
    /// `thaiAlphaPeriod` — Thai alphabet with a period.
    ThaiAlphaPeriod,
    /// `thaiAlphaParenR` — Thai alphabet with a closing paren.
    ThaiAlphaParenR,
    /// `thaiAlphaParenBoth` — Thai alphabet in parens.
    ThaiAlphaParenBoth,
    /// `thaiNumPeriod` — Thai numerals with a period.
    ThaiNumPeriod,
    /// `thaiNumParenR` — Thai numerals with a closing paren.
    ThaiNumParenR,
    /// `thaiNumParenBoth` — Thai numerals in parens.
    ThaiNumParenBoth,
    /// `hindiAlphaPeriod` — Hindi alphabet with a period.
    HindiAlphaPeriod,
    /// `hindiAlpha1Period` — Hindi alphabet (alternate) with a period.
    HindiAlpha1Period,
    /// `hindiNumPeriod` — Hindi numerals with a period.
    HindiNumPeriod,
    /// `hindiNumParenR` — Hindi numerals with a closing paren.
    HindiNumParenR,
}

impl AutoNumberScheme {
    /// The `a:buAutoNum@type` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::ArabicPeriod => "arabicPeriod",
            Self::ArabicParenR => "arabicParenR",
            Self::ArabicParenBoth => "arabicParenBoth",
            Self::ArabicPlain => "arabicPlain",
            Self::AlphaLcPeriod => "alphaLcPeriod",
            Self::AlphaUcPeriod => "alphaUcPeriod",
            Self::AlphaLcParenR => "alphaLcParenR",
            Self::AlphaUcParenR => "alphaUcParenR",
            Self::AlphaLcParenBoth => "alphaLcParenBoth",
            Self::AlphaUcParenBoth => "alphaUcParenBoth",
            Self::RomanLcPeriod => "romanLcPeriod",
            Self::RomanUcPeriod => "romanUcPeriod",
            Self::RomanLcParenR => "romanLcParenR",
            Self::RomanUcParenR => "romanUcParenR",
            Self::RomanLcParenBoth => "romanLcParenBoth",
            Self::RomanUcParenBoth => "romanUcParenBoth",
            Self::CircleNumDbPlain => "circleNumDbPlain",
            Self::CircleNumWdBlackPlain => "circleNumWdBlackPlain",
            Self::CircleNumWdWhitePlain => "circleNumWdWhitePlain",
            Self::ArabicDbPeriod => "arabicDbPeriod",
            Self::ArabicDbPlain => "arabicDbPlain",
            Self::Arabic1Minus => "arabic1Minus",
            Self::Arabic2Minus => "arabic2Minus",
            Self::Hebrew2Minus => "hebrew2Minus",
            Self::Ea1ChsPeriod => "ea1ChsPeriod",
            Self::Ea1ChsPlain => "ea1ChsPlain",
            Self::Ea1ChtPeriod => "ea1ChtPeriod",
            Self::Ea1ChtPlain => "ea1ChtPlain",
            Self::Ea1JpnChsDbPeriod => "ea1JpnChsDbPeriod",
            Self::Ea1JpnKorPlain => "ea1JpnKorPlain",
            Self::Ea1JpnKorPeriod => "ea1JpnKorPeriod",
            Self::ThaiAlphaPeriod => "thaiAlphaPeriod",
            Self::ThaiAlphaParenR => "thaiAlphaParenR",
            Self::ThaiAlphaParenBoth => "thaiAlphaParenBoth",
            Self::ThaiNumPeriod => "thaiNumPeriod",
            Self::ThaiNumParenR => "thaiNumParenR",
            Self::ThaiNumParenBoth => "thaiNumParenBoth",
            Self::HindiAlphaPeriod => "hindiAlphaPeriod",
            Self::HindiAlpha1Period => "hindiAlpha1Period",
            Self::HindiNumPeriod => "hindiNumPeriod",
            Self::HindiNumParenR => "hindiNumParenR",
        }
    }

    /// Reads an `a:buAutoNum@type` token; an unrecognized one degrades to
    /// [`AutoNumberScheme::ArabicPeriod`].
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|scheme| scheme.token() == token)
            .unwrap_or(Self::ArabicPeriod)
    }

    /// Every scheme, so a guard derives the count rather than hand-maintaining it.
    pub const ALL: [Self; 41] = [
        Self::ArabicPeriod,
        Self::ArabicParenR,
        Self::ArabicParenBoth,
        Self::ArabicPlain,
        Self::AlphaLcPeriod,
        Self::AlphaUcPeriod,
        Self::AlphaLcParenR,
        Self::AlphaUcParenR,
        Self::AlphaLcParenBoth,
        Self::AlphaUcParenBoth,
        Self::RomanLcPeriod,
        Self::RomanUcPeriod,
        Self::RomanLcParenR,
        Self::RomanUcParenR,
        Self::RomanLcParenBoth,
        Self::RomanUcParenBoth,
        Self::CircleNumDbPlain,
        Self::CircleNumWdBlackPlain,
        Self::CircleNumWdWhitePlain,
        Self::ArabicDbPeriod,
        Self::ArabicDbPlain,
        Self::Arabic1Minus,
        Self::Arabic2Minus,
        Self::Hebrew2Minus,
        Self::Ea1ChsPeriod,
        Self::Ea1ChsPlain,
        Self::Ea1ChtPeriod,
        Self::Ea1ChtPlain,
        Self::Ea1JpnChsDbPeriod,
        Self::Ea1JpnKorPlain,
        Self::Ea1JpnKorPeriod,
        Self::ThaiAlphaPeriod,
        Self::ThaiAlphaParenR,
        Self::ThaiAlphaParenBoth,
        Self::ThaiNumPeriod,
        Self::ThaiNumParenR,
        Self::ThaiNumParenBoth,
        Self::HindiAlphaPeriod,
        Self::HindiAlpha1Period,
        Self::HindiNumPeriod,
        Self::HindiNumParenR,
    ];
}

/// The bullet a paragraph carries (`a:buNone` / `a:buChar` / `a:buAutoNum`).
///
/// `None` on the containing [`crate::TextParagraphProperties`] means **inherit**;
/// [`TextBullet::None`] means an explicit `a:buNone`, which SUPPRESSES an inherited
/// bullet. Collapsing the two would make a deliberately unbulleted paragraph in a
/// bulleted body placeholder grow a bullet on reopen — the distinction is the whole
/// reason this is not an `Option<Bullet>` alone.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TextBullet {
    /// `a:buNone` — explicitly no bullet, overriding the tier above.
    None,
    /// `a:buChar` — a literal character, with the font it is drawn in.
    Character {
        /// `a:buChar@char`. A `String` rather than a `char` because the attribute is
        /// a string in the schema and real files carry multi-scalar glyphs.
        character: String,
        /// `a:buFont`, which matters: the common bullets live in Wingdings and
        /// Symbol, so dropping the font renders a box.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font: Option<Typeface>,
    },
    /// `a:buAutoNum` — a generated number.
    AutoNumber {
        /// `a:buAutoNum@type`.
        scheme: AutoNumberScheme,
        /// `a:buAutoNum@startAt`, when the run of numbering restarts.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_at: Option<u32>,
    },
}

impl TextBullet {
    /// Whether this bullet draws anything, which is what a renderer branches on.
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        !matches!(self, Self::None)
    }
}
