// SPDX-License-Identifier: Apache-2.0

//! The presentation colour map (`p:clrMap`), and the two resolvers that turn a
//! theme reference into a concrete answer.
//!
//! # Why a presentation needs a colour MAP and a document does not
//!
//! WordprocessingML has twelve theme colour slots and names them directly:
//! `w:themeColor="accent1"` means `a:accent1`, always. PresentationML inserts an
//! indirection. A deck's shapes are authored against *roles* — `bg1` is "the
//! background", `tx1` is "the text" — and `p:sldMaster/p:clrMap` binds each role
//! to one of the theme's twelve slots. A light design writes `bg1="lt1"
//! tx1="dk1"`; the same theme with `bg1="dk1" tx1="lt1"` is the dark variant, and
//! **no other part of the file differs**.
//!
//! That is why an `a:schemeClr val="tx1"` is not resolvable from the theme part
//! alone: `tx1` could be `a:dk1` or `a:lt1`, and picking the first silently
//! inverts every dark-themed deck. It is also why the four slot-named tokens
//! (`dk1`, `lt1`, `dk2`, `lt2`) bypass the map — they name the theme entry itself,
//! not a role — which [`SchemeColorToken`] makes explicit rather than folding into
//! an alias table.
//!
//! # What is reused rather than redeclared
//!
//! Everything below the map. [`ColorScheme`] holds the twelve slots,
//! [`ColorTransform`] holds `a:tint`/`a:shade`/`a:alpha`/`a:lumMod`/`a:lumOff`
//! unapplied in the units the file states, and
//! [`ColorTransform::apply`] folds them — the one arithmetic both document classes
//! use, so a themed shape on a slide and a themed shape in a document cannot paint
//! different colours from the same inputs.

use std::collections::BTreeMap;

use casual_doc_model::v1::{ColorScheme, ColorTransform, FontScheme, Rgba, SchemeColor};
use serde::{Deserialize, Serialize};

use crate::{PresentationError, SlideId, SlideLayoutId, SlideMasterId};

/// How many slots an `a:clrScheme` has, which is also the length of a
/// [`ThemePalette`].
pub const THEME_COLOR_SLOTS: usize = 12;

/// One of the twelve `a:clrScheme` children (ECMA-376 §20.1.6.2).
///
/// The discriminant order is the OOXML child order, which is also
/// [`ColorScheme`]'s field order, so [`ThemeColorSlot::index`] indexes a
/// [`ThemePalette`] without a lookup table.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeColorSlot {
    /// `a:dk1`.
    Dark1,
    /// `a:lt1`.
    Light1,
    /// `a:dk2`.
    Dark2,
    /// `a:lt2`.
    Light2,
    /// `a:accent1`.
    Accent1,
    /// `a:accent2`.
    Accent2,
    /// `a:accent3`.
    Accent3,
    /// `a:accent4`.
    Accent4,
    /// `a:accent5`.
    Accent5,
    /// `a:accent6`.
    Accent6,
    /// `a:hlink`.
    Hyperlink,
    /// `a:folHlink`.
    FollowedHyperlink,
}

impl ThemeColorSlot {
    /// Every slot, in `a:clrScheme` child order.
    pub const ALL: [Self; THEME_COLOR_SLOTS] = [
        Self::Dark1,
        Self::Light1,
        Self::Dark2,
        Self::Light2,
        Self::Accent1,
        Self::Accent2,
        Self::Accent3,
        Self::Accent4,
        Self::Accent5,
        Self::Accent6,
        Self::Hyperlink,
        Self::FollowedHyperlink,
    ];

    /// The slot a `p:clrMap` attribute value names.
    ///
    /// `None` for anything outside `ST_ColorSchemeIndex`, which the caller reports
    /// rather than substituting a slot: a wrong binding repaints the deck and looks
    /// deliberate.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "dk1" => Self::Dark1,
            "lt1" => Self::Light1,
            "dk2" => Self::Dark2,
            "lt2" => Self::Light2,
            "accent1" => Self::Accent1,
            "accent2" => Self::Accent2,
            "accent3" => Self::Accent3,
            "accent4" => Self::Accent4,
            "accent5" => Self::Accent5,
            "accent6" => Self::Accent6,
            "hlink" => Self::Hyperlink,
            "folHlink" => Self::FollowedHyperlink,
            _ => return None,
        })
    }

    /// The token this slot is written as.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Dark1 => "dk1",
            Self::Light1 => "lt1",
            Self::Dark2 => "dk2",
            Self::Light2 => "lt2",
            Self::Accent1 => "accent1",
            Self::Accent2 => "accent2",
            Self::Accent3 => "accent3",
            Self::Accent4 => "accent4",
            Self::Accent5 => "accent5",
            Self::Accent6 => "accent6",
            Self::Hyperlink => "hlink",
            Self::FollowedHyperlink => "folHlink",
        }
    }

    /// This slot's index into a [`ThemePalette`], in `a:clrScheme` child order.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A presentation-level colour ROLE: the twelve `p:clrMap` attributes, which are
/// also the twelve mapped `a:schemeClr@val` tokens.
///
/// Distinct from [`ThemeColorSlot`] on purpose. The two enumerations share ten
/// spellings and differ in the two that matter — a role is `bg1`/`tx1`/`bg2`/`tx2`
/// where a slot is `dk1`/`lt1`/`dk2`/`lt2` — and collapsing them into one type is
/// how a reader ends up resolving `tx1` to `a:dk1` because that is what the
/// identity map happens to say.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorRole {
    /// `bg1`.
    Background1,
    /// `tx1`.
    Text1,
    /// `bg2`.
    Background2,
    /// `tx2`.
    Text2,
    /// `accent1`.
    Accent1,
    /// `accent2`.
    Accent2,
    /// `accent3`.
    Accent3,
    /// `accent4`.
    Accent4,
    /// `accent5`.
    Accent5,
    /// `accent6`.
    Accent6,
    /// `hlink`.
    Hyperlink,
    /// `folHlink`.
    FollowedHyperlink,
}

impl ColorRole {
    /// Every role, in `p:clrMap` attribute order.
    pub const ALL: [Self; THEME_COLOR_SLOTS] = [
        Self::Background1,
        Self::Text1,
        Self::Background2,
        Self::Text2,
        Self::Accent1,
        Self::Accent2,
        Self::Accent3,
        Self::Accent4,
        Self::Accent5,
        Self::Accent6,
        Self::Hyperlink,
        Self::FollowedHyperlink,
    ];

    /// The `p:clrMap` attribute name this role is bound by.
    #[must_use]
    pub const fn attribute(self) -> &'static str {
        match self {
            Self::Background1 => "bg1",
            Self::Text1 => "tx1",
            Self::Background2 => "bg2",
            Self::Text2 => "tx2",
            Self::Accent1 => "accent1",
            Self::Accent2 => "accent2",
            Self::Accent3 => "accent3",
            Self::Accent4 => "accent4",
            Self::Accent5 => "accent5",
            Self::Accent6 => "accent6",
            Self::Hyperlink => "hlink",
            Self::FollowedHyperlink => "folHlink",
        }
    }

    /// The default slot for this role: the identity binding PowerPoint writes for
    /// a light design (`bg1="lt1"`, `tx1="dk1"`).
    ///
    /// Used only where the package states no map at all — a deck whose master
    /// carries no `p:clrMap` is malformed, and this is the repair that keeps it
    /// openable. It is NOT a substitute for reading the map: the whole point of
    /// [`ColorMap`] is that the identity is one choice of twelve-way binding and
    /// a dark design states a different one.
    #[must_use]
    pub const fn default_slot(self) -> ThemeColorSlot {
        match self {
            Self::Background1 => ThemeColorSlot::Light1,
            Self::Text1 => ThemeColorSlot::Dark1,
            Self::Background2 => ThemeColorSlot::Light2,
            Self::Text2 => ThemeColorSlot::Dark2,
            Self::Accent1 => ThemeColorSlot::Accent1,
            Self::Accent2 => ThemeColorSlot::Accent2,
            Self::Accent3 => ThemeColorSlot::Accent3,
            Self::Accent4 => ThemeColorSlot::Accent4,
            Self::Accent5 => ThemeColorSlot::Accent5,
            Self::Accent6 => ThemeColorSlot::Accent6,
            Self::Hyperlink => ThemeColorSlot::Hyperlink,
            Self::FollowedHyperlink => ThemeColorSlot::FollowedHyperlink,
        }
    }
}

/// What an `a:schemeClr@val` token names (`ST_SchemeColorVal`, §20.1.10.47).
///
/// Three cases, and the distinction between the first two is the whole reason this
/// type exists — see the module documentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemeColorToken {
    /// A role (`bg1`, `tx1`, `accent1`, …): resolved THROUGH the colour map.
    Role(ColorRole),
    /// A slot (`dk1`, `lt1`, `dk2`, `lt2`): names the `a:clrScheme` entry directly
    /// and does not consult the map.
    Slot(ThemeColorSlot),
    /// `phClr` — a formal parameter standing for the colour the referencing shape
    /// supplies, not a colour. Resolving it to anything here would give every
    /// styled shape in the deck the same fill.
    Placeholder,
}

impl SchemeColorToken {
    /// The token an `a:schemeClr@val` spells, or `None` for a value outside the
    /// enumeration.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "phClr" => Self::Placeholder,
            "bg1" => Self::Role(ColorRole::Background1),
            "tx1" => Self::Role(ColorRole::Text1),
            "bg2" => Self::Role(ColorRole::Background2),
            "tx2" => Self::Role(ColorRole::Text2),
            "dk1" => Self::Slot(ThemeColorSlot::Dark1),
            "lt1" => Self::Slot(ThemeColorSlot::Light1),
            "dk2" => Self::Slot(ThemeColorSlot::Dark2),
            "lt2" => Self::Slot(ThemeColorSlot::Light2),
            other => Self::Role(match other {
                "accent1" => ColorRole::Accent1,
                "accent2" => ColorRole::Accent2,
                "accent3" => ColorRole::Accent3,
                "accent4" => ColorRole::Accent4,
                "accent5" => ColorRole::Accent5,
                "accent6" => ColorRole::Accent6,
                "hlink" => ColorRole::Hyperlink,
                "folHlink" => ColorRole::FollowedHyperlink,
                _ => return None,
            }),
        })
    }
}

/// A `p:clrMap` (or a `p:clrMapOvr/a:overrideClrMapping`): the binding from the
/// twelve presentation roles to the theme's twelve slots.
///
/// All twelve attributes are REQUIRED by `CT_ColorMapping`, so a well-formed map
/// is total. A missing attribute falls back to [`ColorRole::default_slot`] and the
/// reader reports it, rather than the map being half-applied.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorMap {
    /// `@bg1`.
    pub background1: ThemeColorSlot,
    /// `@tx1`.
    pub text1: ThemeColorSlot,
    /// `@bg2`.
    pub background2: ThemeColorSlot,
    /// `@tx2`.
    pub text2: ThemeColorSlot,
    /// `@accent1`.
    pub accent1: ThemeColorSlot,
    /// `@accent2`.
    pub accent2: ThemeColorSlot,
    /// `@accent3`.
    pub accent3: ThemeColorSlot,
    /// `@accent4`.
    pub accent4: ThemeColorSlot,
    /// `@accent5`.
    pub accent5: ThemeColorSlot,
    /// `@accent6`.
    pub accent6: ThemeColorSlot,
    /// `@hlink`.
    pub hyperlink: ThemeColorSlot,
    /// `@folHlink`.
    pub followed_hyperlink: ThemeColorSlot,
}

impl Default for ColorMap {
    /// The identity map — the light-design binding, and the only defensible
    /// fallback for a package that states none.
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl ColorMap {
    /// The identity map (`bg1="lt1"`, `tx1="dk1"`, `bg2="lt2"`, `tx2="dk2"`, each
    /// accent and link to itself).
    pub const IDENTITY: Self = Self {
        background1: ThemeColorSlot::Light1,
        text1: ThemeColorSlot::Dark1,
        background2: ThemeColorSlot::Light2,
        text2: ThemeColorSlot::Dark2,
        accent1: ThemeColorSlot::Accent1,
        accent2: ThemeColorSlot::Accent2,
        accent3: ThemeColorSlot::Accent3,
        accent4: ThemeColorSlot::Accent4,
        accent5: ThemeColorSlot::Accent5,
        accent6: ThemeColorSlot::Accent6,
        hyperlink: ThemeColorSlot::Hyperlink,
        followed_hyperlink: ThemeColorSlot::FollowedHyperlink,
    };

    /// The slot a role is bound to.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub const fn slot(&self, role: ColorRole) -> ThemeColorSlot {
        match role {
            ColorRole::Background1 => self.background1,
            ColorRole::Text1 => self.text1,
            ColorRole::Background2 => self.background2,
            ColorRole::Text2 => self.text2,
            ColorRole::Accent1 => self.accent1,
            ColorRole::Accent2 => self.accent2,
            ColorRole::Accent3 => self.accent3,
            ColorRole::Accent4 => self.accent4,
            ColorRole::Accent5 => self.accent5,
            ColorRole::Accent6 => self.accent6,
            ColorRole::Hyperlink => self.hyperlink,
            ColorRole::FollowedHyperlink => self.followed_hyperlink,
        }
    }

    /// The slot `role`'s binding is written into, for a reader filling the map
    /// attribute by attribute.
    pub fn slot_mut(&mut self, role: ColorRole) -> &mut ThemeColorSlot {
        match role {
            ColorRole::Background1 => &mut self.background1,
            ColorRole::Text1 => &mut self.text1,
            ColorRole::Background2 => &mut self.background2,
            ColorRole::Text2 => &mut self.text2,
            ColorRole::Accent1 => &mut self.accent1,
            ColorRole::Accent2 => &mut self.accent2,
            ColorRole::Accent3 => &mut self.accent3,
            ColorRole::Accent4 => &mut self.accent4,
            ColorRole::Accent5 => &mut self.accent5,
            ColorRole::Accent6 => &mut self.accent6,
            ColorRole::Hyperlink => &mut self.hyperlink,
            ColorRole::FollowedHyperlink => &mut self.followed_hyperlink,
        }
    }

    /// The slot an `a:schemeClr@val` token selects, or `None` for `phClr`.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub const fn resolve(&self, token: SchemeColorToken) -> Option<ThemeColorSlot> {
        match token {
            SchemeColorToken::Role(role) => Some(self.slot(role)),
            SchemeColorToken::Slot(slot) => Some(slot),
            SchemeColorToken::Placeholder => None,
        }
    }
}

/// Which colour map is in force where.
///
/// A side table on [`Presentation`](crate::Presentation) rather than fields on
/// [`SlideMaster`](crate::SlideMaster)/[`SlideLayout`](crate::SlideLayout)/
/// [`Slide`](crate::Slide). The reason is the one `v1::Definitions::shape_styles`
/// and `v1::Definitions::charts` record: those three structs have literal
/// construction sites in three crates, and adding a field to one is a breaking
/// change to every literal with nothing for a merge to conflict on
/// (`SKILL` §5a shape 1). Keeping it here also keeps the override CHAIN in one
/// place — see [`ColorMapping::in_force`], which is the only correct way to ask
/// the question.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorMapping {
    /// `p:sldMaster/p:clrMap`, by master. A master always states one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub masters: BTreeMap<SlideMasterId, ColorMap>,
    /// `p:sldLayout/p:clrMapOvr/a:overrideClrMapping`, by layout — absent where
    /// the layout states `a:masterClrMapping`, which is the common case and MUST
    /// stay distinguishable from an override that happens to equal the master's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layouts: BTreeMap<SlideLayoutId, ColorMap>,
    /// `p:sld/p:clrMapOvr/a:overrideClrMapping`, by slide.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slides: BTreeMap<SlideId, ColorMap>,
}

impl ColorMapping {
    /// Whether nothing is mapped, so the table is omitted from a snapshot.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.masters.is_empty() && self.layouts.is_empty() && self.slides.is_empty()
    }

    /// The map in force for a slide: its own override, else its layout's, else its
    /// master's, else the identity.
    ///
    /// The chain is the one ECMA-376 §19.3.1.39 states, and it is a CHAIN rather
    /// than a merge: an `a:overrideClrMapping` replaces all twelve bindings at
    /// once, so a slide that overrides `tx1` has also restated `bg1`.
    ///
    /// Complexity: O(log parts) — three map lookups, no scan.
    #[must_use]
    pub fn in_force(
        &self,
        slide: SlideId,
        layout: Option<SlideLayoutId>,
        master: Option<SlideMasterId>,
    ) -> ColorMap {
        if let Some(map) = self.slides.get(&slide) {
            return *map;
        }
        if let Some(map) = layout.and_then(|layout| self.layouts.get(&layout)) {
            return *map;
        }
        master
            .and_then(|master| self.masters.get(&master))
            .copied()
            .unwrap_or_default()
    }
}

/// The twelve theme slots resolved to concrete colours, plus the colour map that
/// says which role reaches which slot.
///
/// # Why resolution lives in the MODEL
///
/// Because two callers need the same answer. Import resolves a shape's own
/// `a:schemeClr` fill so the shape arrives with a colour; layout resolves a
/// `p:style` reference's `a:phClr` argument so a themed shape paints. The document
/// side has the same pair and implemented it twice — `casual-doc-import`'s
/// `resolve_palette` carries a doc comment saying it "mirrors the layout resolver",
/// which is the admission that two copies of one rule exist. One type here means
/// they cannot drift (`SKILL` §8: prefer one mechanism over two).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemePalette {
    slots: [Rgba; THEME_COLOR_SLOTS],
    map: ColorMap,
}

impl ThemePalette {
    /// Resolves an `a:clrScheme` into concrete channels under `map`.
    ///
    /// Complexity: O(1) — twelve slots, fixed.
    #[must_use]
    pub fn new(scheme: &ColorScheme, map: ColorMap) -> Self {
        Self {
            slots: [
                resolve_slot(&scheme.dark1),
                resolve_slot(&scheme.light1),
                resolve_slot(&scheme.dark2),
                resolve_slot(&scheme.light2),
                resolve_slot(&scheme.accent1),
                resolve_slot(&scheme.accent2),
                resolve_slot(&scheme.accent3),
                resolve_slot(&scheme.accent4),
                resolve_slot(&scheme.accent5),
                resolve_slot(&scheme.accent6),
                resolve_slot(&scheme.hyperlink),
                resolve_slot(&scheme.followed_hyperlink),
            ],
            map,
        }
    }

    /// The same palette under a different colour map, for a part that states a
    /// `p:clrMapOvr`.
    #[must_use]
    pub const fn with_map(mut self, map: ColorMap) -> Self {
        self.map = map;
        self
    }

    /// The map this palette resolves roles through.
    #[must_use]
    pub const fn map(&self) -> ColorMap {
        self.map
    }

    /// One slot's concrete colour.
    #[must_use]
    pub const fn slot(&self, slot: ThemeColorSlot) -> Rgba {
        self.slots[slot.index()]
    }

    /// An `a:schemeClr` with its colour transforms, as a concrete colour.
    ///
    /// `None` for `phClr` (a formal parameter, not a colour) and for a `val` outside
    /// `ST_SchemeColorVal`. The caller reports both rather than substituting: a
    /// scheme colour guessed wrong paints a branded deck in the wrong brand.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn resolve(&self, value: &str, transform: ColorTransform) -> Option<Rgba> {
        let slot = self.map.resolve(SchemeColorToken::from_token(value)?)?;
        Some(transform.apply(self.slot(slot)))
    }
}

/// One `a:clrScheme` slot as opaque channels: an `a:srgbClr` is its RGB; an
/// `a:sysClr` uses the `lastClr` the producer recorded.
///
/// Honouring `lastClr` is the lossless reading. It is the colour the deck was
/// authored to look like, and resolving the system slot against THIS machine's
/// palette would make the same file render differently per host — which is the
/// determinism rule (`SKILL` §12) as much as a fidelity one. Only when there is no
/// `lastClr` does the token decide, and then only between white and black.
fn resolve_slot(color: &SchemeColor) -> Rgba {
    match color {
        SchemeColor::Srgb(rgb) => Rgba {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
            a: 255,
        },
        SchemeColor::System(system) => match system.last_color {
            Some(rgb) => Rgba {
                r: rgb.r,
                g: rgb.g,
                b: rgb.b,
                a: 255,
            },
            None => match system.value.as_str() {
                "window" | "background" | "btnFace" | "menu" | "3dLight" | "highlightText"
                | "infoBk" | "scrollBar" | "windowFrame" => Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                _ => Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            },
        },
    }
}

/// Which theme font collection and script axis a `+mj-lt`-style typeface
/// reference names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeFontReference {
    /// `+mj-*` (the heading collection) rather than `+mn-*` (the body one).
    pub major: bool,
    /// The script axis the reference's suffix selects.
    pub script: ThemeFontScript,
}

/// The script axis of a theme font reference: the `lt`/`ea`/`cs` suffix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeFontScript {
    /// `-lt` — `a:latin`.
    Latin,
    /// `-ea` — `a:ea`.
    EastAsian,
    /// `-cs` — `a:cs`.
    ComplexScript,
}

impl ThemeFontReference {
    /// Parses an `a:latin@typeface` theme reference (`+mj-lt`, `+mn-ea`, …).
    ///
    /// `None` for a concrete family name, which is the overwhelmingly common case
    /// and needs no resolution at all.
    #[must_use]
    pub fn parse(typeface: &str) -> Option<Self> {
        let (collection, script) = typeface.split_once('-')?;
        let major = match collection {
            "+mj" => true,
            "+mn" => false,
            _ => return None,
        };
        Some(Self {
            major,
            script: match script {
                "lt" => ThemeFontScript::Latin,
                "ea" => ThemeFontScript::EastAsian,
                "cs" => ThemeFontScript::ComplexScript,
                _ => return None,
            },
        })
    }

    /// The concrete family this reference names in `scheme`.
    ///
    /// `None` when the entry's `@typeface` is empty, which `a:ea`/`a:cs` routinely
    /// are: the empty string means "fall back to the latin entry", and that is a
    /// decision for the font matcher rather than something to fabricate here.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn resolve(self, scheme: &FontScheme) -> Option<&str> {
        let collection = if self.major {
            &scheme.major
        } else {
            &scheme.minor
        };
        let entry = match self.script {
            ThemeFontScript::Latin => &collection.latin,
            ThemeFontScript::EastAsian => &collection.ea,
            ThemeFontScript::ComplexScript => &collection.cs,
        };
        (!entry.typeface.is_empty()).then_some(entry.typeface.as_str())
    }
}

/// Refuses a colour mapping whose keys do not name parts of this deck.
///
/// A dangling key is a silent wrong answer rather than a missing one:
/// [`ColorMapping::in_force`] would fall through to the tier above and resolve a
/// role to the wrong slot, which repaints the slide without anything failing.
pub(crate) fn validate_mapping(
    mapping: &ColorMapping,
    masters: &[crate::SlideMaster],
    layouts: &[crate::SlideLayout],
    slides: &[crate::Slide],
) -> Result<(), PresentationError> {
    for master in mapping.masters.keys() {
        if !masters.iter().any(|part| part.id == *master) {
            return Err(PresentationError::DanglingColorMapRef(master.node_id()));
        }
    }
    for layout in mapping.layouts.keys() {
        if !layouts.iter().any(|part| part.id == *layout) {
            return Err(PresentationError::DanglingColorMapRef(layout.node_id()));
        }
    }
    for slide in mapping.slides.keys() {
        if !slides.iter().any(|part| part.id == *slide) {
            return Err(PresentationError::DanglingColorMapRef(slide.node_id()));
        }
    }
    Ok(())
}
