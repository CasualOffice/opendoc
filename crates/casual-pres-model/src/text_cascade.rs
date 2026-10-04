// SPDX-License-Identifier: Apache-2.0

//! The fold: a slide run's **effective** text properties, resolved through the
//! placeholder inheritance chain.
//!
//! # The problem this solves
//!
//! A run on a real slide states almost nothing. PowerPoint writes
//! `<a:rPr lang="en-US"/>` and leaves the size, the typeface, the colour, the
//! bullet and the indent to be inherited. Every tier of that inheritance is
//! modelled — the shape's `a:lstStyle`, the matching placeholder's on the layout
//! and the master, the master's `p:txStyles` and the presentation's
//! `p:defaultTextStyle` — and **this module is the only thing that reads them
//! together.** Until it existed, a slide's text had no resolvable font size and
//! nothing could shape it.
//!
//! # The pattern, named before the code
//!
//! This is inheritance flattening — resolving a *computed value* from a layered
//! cascade, the same job a browser's computed-style pass does. The repository's
//! own prior art is `casual-doc-layout::cascade`, which resolves Word's
//! `docDefaults → style chain → direct` the same way: a cheap borrowing view
//! constructed once, then queried, overlaying property by property from the lowest
//! precedence upward. This module follows that shape deliberately rather than
//! inventing a second one.
//!
//! # The chain, lowest precedence first
//!
//! ECMA-376 Part 1 §19.3.1.x. Resolution order, which is the order
//! [`TextCascade::resolve`] overlays:
//!
//! 1. `p:defaultTextStyle` on the presentation;
//! 2. the master's `p:txStyles` tier for this shape's placeholder kind;
//! 3. the matching placeholder's `a:lstStyle` on the **master**;
//! 4. the matching placeholder's `a:lstStyle` on the **layout**;
//! 5. the shape's own `a:txBody/a:lstStyle`;
//! 6. the paragraph's own `a:pPr` — [`ResolvedText::overlay_paragraph`];
//! 7. the run's own `a:rPr` — [`ResolvedText::overlay_run`].
//!
//! Steps 6 and 7 are separate calls rather than parameters because they vary per
//! paragraph and per run while steps 1-5 vary only by outline level: resolving
//! 1-5 once per level and overlaying the rest is what keeps the work off the
//! per-run path.
//!
//! # What it deliberately does not do
//!
//! **It resolves no colour and no theme font.** A `StyleColor::Scheme` and a
//! `+mj-lt` typeface name are carried through verbatim, because resolving either
//! needs the theme part and the `p:clrMap`, and neither is read yet. A caller that
//! needs a concrete colour must still treat a scheme colour as unresolved — this
//! fold makes the *inheritance* answerable, not the theme.
//!
//! **It supplies no engine default.** A chain that states no size resolves to
//! `None`, not to 18 points. The last-resort default belongs to whatever draws the
//! text, where the font registry and the DPI are known, and inventing one here
//! would make an unstated size indistinguishable from a stated one.

use serde::{Deserialize, Serialize};

use crate::text_body::ListStyle;
use crate::text_paragraph::TextParagraphProperties;
use crate::text_run::TextCharacterProperties;

/// One shape's resolved inheritance chain: cheap to build, cheap to query.
///
/// Borrows every tier rather than copying them, like
/// `casual_doc_layout::cascade::StyleCascade`. Built once per shape so the
/// placeholder-slot lookups — which scan a layout's and a master's children — are
/// paid once instead of once per paragraph.
///
/// # Complexity
///
/// Construction is `O(layouts + masters + shapes-per-tree)`, the same as
/// `Presentation::resolve_slot` and independent of the deck's slide count.
/// [`TextCascade::resolve`] is then `O(1)` per level.
#[derive(Clone, Copy, Debug)]
pub struct TextCascade<'a> {
    /// Tier 5: the shape's own `a:txBody/a:lstStyle`.
    pub(crate) shape: Option<&'a ListStyle>,
    /// Tier 4: the matching placeholder's `a:lstStyle` on the layout.
    pub(crate) layout: Option<&'a ListStyle>,
    /// Tier 3: the matching placeholder's `a:lstStyle` on the master.
    pub(crate) master_slot: Option<&'a ListStyle>,
    /// Tier 2: the master's `p:txStyles` tier for this shape's placeholder kind.
    pub(crate) master_tier: Option<&'a ListStyle>,
    /// Tier 1: the presentation's `p:defaultTextStyle`.
    pub(crate) deck: Option<&'a ListStyle>,
}

impl<'a> TextCascade<'a> {
    /// A cascade that inherits nothing, for a shape whose deck states no tier.
    ///
    /// Not a hypothetical: a `.pptx` with no `p:txStyles` and no
    /// `p:defaultTextStyle` is schema-valid, and its text inherits only from the
    /// shape itself.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            shape: None,
            layout: None,
            master_slot: None,
            master_tier: None,
            deck: None,
        }
    }

    /// The same cascade with a different tier 5 — the shape's own `a:lstStyle`
    /// replaced by some other body's.
    ///
    /// This exists for exactly one caller and the reason is worth stating: a
    /// table CELL carries its own `a:txBody/a:lstStyle`, and the four tiers under
    /// it belong to the `p:graphicFrame` the table is on. So a cell's cascade is
    /// the frame's cascade with the cell's own list style on top, and this is how
    /// a consumer builds it without a second cascade type or a second resolver.
    ///
    /// # Complexity
    ///
    /// O(1) — one field.
    #[must_use]
    pub const fn with_shape_tier(mut self, shape: Option<&'a ListStyle>) -> Self {
        self.shape = shape;
        self
    }

    /// The properties in effect for a paragraph at `level`, before the paragraph's
    /// own `a:pPr` and its runs' `a:rPr`.
    ///
    /// `level` is ZERO-based, matching `a:pPr@lvl`. `a:lvl1pPr` is level 0, and the
    /// two forms never share a variable anywhere in this crate.
    ///
    /// # Complexity
    ///
    /// O(1) in the deck: five `O(1)` level lookups and a fixed field overlay.
    #[must_use]
    pub fn resolve(&self, level: u8) -> ResolvedText {
        let mut resolved = ResolvedText::default();
        // Lowest precedence first, each overlaying the one below it. Reversing this
        // would make a master's tier beat the shape that states its own formatting,
        // which is the one way to get a cascade exactly backwards and still have
        // every level populated.
        for tier in [
            self.deck,
            self.master_tier,
            self.master_slot,
            self.layout,
            self.shape,
        ] {
            let Some(properties) = tier.and_then(|tier| tier.level(level)) else {
                continue;
            };
            resolved.overlay_paragraph(properties);
        }
        resolved
    }
}

/// The text properties in effect, with every tier folded in.
///
/// Fields stay `Option` after folding, and that is deliberate: `None` means *no
/// tier in the chain stated this*, which a renderer must answer with its own
/// default rather than with a value that looks authored. Collapsing it here would
/// make "inherited nothing" indistinguishable from "inherited 18pt".
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedText {
    /// The paragraph-level properties: alignment, margins, spacing, bullet, tabs.
    pub paragraph: TextParagraphProperties,
    /// The character-level properties every run in the paragraph starts from.
    pub character: TextCharacterProperties,
}

impl ResolvedText {
    /// Overlays one tier's `a:lvlNpPr` (or a paragraph's own `a:pPr`) on top of
    /// what is resolved so far: a stated property wins, an unstated one inherits.
    ///
    /// `a:defRPr` folds into [`ResolvedText::character`] at the same time, because
    /// a tier's default run properties belong to the same layer as its paragraph
    /// properties — reading them separately is how a tier's size and its indent
    /// end up resolved at different precedences.
    pub fn overlay_paragraph(&mut self, over: &TextParagraphProperties) {
        // DESTRUCTURED rather than field-accessed: adding a field to
        // `TextParagraphProperties` is then a compile error here (E0027) instead of
        // a property that silently stops inheriting. A cascade that forgets a field
        // is invisible — the text still renders, with the wrong value.
        let TextParagraphProperties {
            level,
            alignment,
            margin_left_emu,
            margin_right_emu,
            indent_emu,
            default_tab_emu,
            right_to_left,
            line_spacing,
            space_before,
            space_after,
            bullet,
            bullet_color,
            bullet_size_percent,
            bullet_font,
            tab_stops,
            default_character,
        } = over;

        // `@lvl` is NOT inherited: it selects which level of the tier below applies,
        // so folding it downward would make a level-2 paragraph claim to be level 2
        // of a tier that was already resolved at level 2. It is carried only so a
        // caller can read back what the paragraph said.
        if level.is_some() {
            self.paragraph.level = *level;
        }
        overlay(&mut self.paragraph.alignment, alignment);
        overlay(&mut self.paragraph.margin_left_emu, margin_left_emu);
        overlay(&mut self.paragraph.margin_right_emu, margin_right_emu);
        overlay(&mut self.paragraph.indent_emu, indent_emu);
        overlay(&mut self.paragraph.default_tab_emu, default_tab_emu);
        overlay(&mut self.paragraph.right_to_left, right_to_left);
        overlay(&mut self.paragraph.line_spacing, line_spacing);
        overlay(&mut self.paragraph.space_before, space_before);
        overlay(&mut self.paragraph.space_after, space_after);
        overlay(&mut self.paragraph.bullet, bullet);
        overlay(&mut self.paragraph.bullet_color, bullet_color);
        overlay(&mut self.paragraph.bullet_size_percent, bullet_size_percent);
        overlay(&mut self.paragraph.bullet_font, bullet_font);
        // Tab stops replace rather than merge: `a:tabLst` is a complete list in
        // ECMA-376, so a tier that states any stops states all of them. Merging two
        // lists would invent stop positions no tier authored.
        if !tab_stops.is_empty() {
            self.paragraph.tab_stops = tab_stops.clone();
        }
        if let Some(character) = default_character.as_deref() {
            self.overlay_run(character);
        }
    }

    /// Overlays a run's own `a:rPr` — the last and highest-precedence layer.
    pub fn overlay_run(&mut self, over: &TextCharacterProperties) {
        // Destructured for the same reason as above.
        let TextCharacterProperties {
            size_hundredths_point,
            bold,
            italic,
            underline,
            strike,
            caps,
            spacing_hundredths_point,
            baseline_percent,
            language,
            latin,
            east_asian,
            complex_script,
            fill,
            dirty,
        } = over;

        overlay(
            &mut self.character.size_hundredths_point,
            size_hundredths_point,
        );
        overlay(&mut self.character.bold, bold);
        overlay(&mut self.character.italic, italic);
        overlay(&mut self.character.underline, underline);
        overlay(&mut self.character.strike, strike);
        overlay(&mut self.character.caps, caps);
        overlay(
            &mut self.character.spacing_hundredths_point,
            spacing_hundredths_point,
        );
        overlay(&mut self.character.baseline_percent, baseline_percent);
        overlay(&mut self.character.language, language);
        overlay(&mut self.character.latin, latin);
        overlay(&mut self.character.east_asian, east_asian);
        overlay(&mut self.character.complex_script, complex_script);
        overlay(&mut self.character.fill, fill);
        // `@dirty` is a producer's spell-check bookkeeping flag, not formatting, so
        // it does not inherit: the highest layer that sets it wins outright and an
        // unset one means false rather than "ask the tier below".
        if *dirty {
            self.character.dirty = true;
        }
    }
}

/// Writes `over` into `target` when the higher layer states something.
///
/// The whole cascade is this one rule applied per field: a stated property wins,
/// an unstated one leaves what is already resolved alone.
fn overlay<T: Clone>(target: &mut Option<T>, over: &Option<T>) {
    if let Some(value) = over {
        *target = Some(value.clone());
    }
}
