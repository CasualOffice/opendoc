// SPDX-License-Identifier: Apache-2.0

//! The two text-style tiers a slide's text inherits from above its own shape:
//! a master's `p:txStyles` and a presentation's `p:defaultTextStyle`.
//!
//! # Why these are the tiers that were missing
//!
//! A run on a real slide usually states almost nothing. PowerPoint writes
//! `<a:rPr lang="en-US"/>` and leaves the size, the typeface, the colour, the
//! bullet and the indent to be inherited — so until these two tiers are modelled,
//! **a slide's text has no font size at all**, and nothing can shape it. That is
//! why `casual-pres-layout` draws shapes and no glyphs: not because text layout is
//! hard on a slide (it is not; nothing flows), but because the properties are not
//! resolvable yet.
//!
//! # The chain, most specific first
//!
//! ECMA-376 Part 1 §19.3.1.x gives the inheritance; this is it in resolution
//! order, which is the order [`TextStyleChain`] folds:
//!
//! 1. the run's own `a:rPr`;
//! 2. the paragraph's `a:pPr/a:defRPr`;
//! 3. the **shape's** own `a:txBody/a:lstStyle`, at the paragraph's level;
//! 4. the matching placeholder's `a:lstStyle` on the **layout**;
//! 5. the matching placeholder's `a:lstStyle` on the **master**;
//! 6. the master's `p:txStyles` tier for that placeholder kind — [`TextStyles`];
//! 7. the presentation's `p:defaultTextStyle`;
//! 8. the engine's own defaults, which are stated in layout rather than here.
//!
//! Tiers 6 and 7 are this module. Tiers 3-5 are `ListStyle`s the model already
//! carries, and `Presentation::resolve_slot` already finds the slot that tiers 4
//! and 5 need.

use serde::{Deserialize, Serialize};

use crate::PlaceholderKind;
use crate::text_body::ListStyle;

/// A master's three text-style tiers (`p:sldMaster/p:txStyles`).
///
/// Three and not nine: each tier is itself a nine-level [`ListStyle`], so the
/// master carries 27 level definitions in total. Absent tiers are empty rather
/// than `None` — a master that writes no `p:txStyles` and one that writes an empty
/// one inherit identically, so distinguishing them would be a difference the
/// format does not make.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStyles {
    /// `p:titleStyle` — the tier a title placeholder inherits from.
    #[serde(default, skip_serializing_if = "ListStyle::is_empty")]
    pub title: ListStyle,
    /// `p:bodyStyle` — the tier a body placeholder inherits from, and also the
    /// tier a shape with **no placeholder at all** inherits from.
    #[serde(default, skip_serializing_if = "ListStyle::is_empty")]
    pub body: ListStyle,
    /// `p:otherStyle` — every remaining placeholder kind.
    #[serde(default, skip_serializing_if = "ListStyle::is_empty")]
    pub other: ListStyle,
}

impl TextStyles {
    /// Whether no tier states anything, which is what a master with no
    /// `p:txStyles` carries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_empty() && self.body.is_empty() && self.other.is_empty()
    }

    /// The tier a shape in `slot` inherits from, or `p:otherStyle` for a shape that
    /// is in no slot at all.
    ///
    /// # The mapping, and the entry I first got wrong
    ///
    /// `title` and `ctrTitle` take `p:titleStyle`; `body`, `subTitle` and `obj`
    /// take `p:bodyStyle`; every other placeholder kind, **and a shape with no
    /// `p:ph` whatsoever**, take `p:otherStyle`.
    ///
    /// `obj` landing in the body tier is worth stating: a content placeholder is a
    /// body placeholder for inheritance even though it is a distinct
    /// `ST_PlaceholderType`. And because the importer reads an absent `p:ph@type`
    /// as `obj` — the schema's own default — a bare `<p:ph/>` reaches here as
    /// `Some(Object)` and correctly takes the body tier.
    ///
    /// `None` is therefore **not** "a placeholder with no type"; it is "not a
    /// placeholder". Those are different shapes with different tiers, and
    /// conflating them is the mistake this comment exists to stop: an earlier
    /// version of this function mapped `None` to `p:bodyStyle`, on the strength of
    /// a no-type placeholder resolving to body. A plain text box dropped on a slide
    /// is not a placeholder and takes `p:otherStyle`.
    #[must_use]
    pub fn tier(&self, slot: Option<PlaceholderKind>) -> &ListStyle {
        match slot {
            Some(PlaceholderKind::Title | PlaceholderKind::CtrTitle) => &self.title,
            Some(PlaceholderKind::Body | PlaceholderKind::SubTitle | PlaceholderKind::Object) => {
                &self.body
            }
            Some(_) | None => &self.other,
        }
    }
}
