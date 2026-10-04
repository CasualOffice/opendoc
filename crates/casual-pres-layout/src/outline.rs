// SPDX-License-Identifier: Apache-2.0

//! A slide's text as STRUCTURE, for assistive technology.
//!
//! # Why this exists
//!
//! A rendered slide is pixels. A `<canvas>` exposes no text, no headings and no
//! list structure, so a screen reader presented with a painted deck gets nothing at
//! all — and the text has been in the model and shaped into glyph runs the whole
//! time. That is exactly the "modelled but unreachable" failure `SKILL` §9.4 names,
//! and it is the sharpest gap `docs/156` §8 records against the viewer. The
//! document side answers the same problem the same way: an `accessibilityTree()`
//! projection the page mirrors into real elements off-screen.
//!
//! # Why it lives in THIS crate and not in the facade
//!
//! Because the rule about which text a reader should hear is the rule about which
//! text PAINTS, and that rule is already written here — once, in
//! `CascadeTier::paints_text`. A placeholder's `a:txBody` on a layout or a master
//! is PROMPT text ("Click to edit Master title style"); a *non*-placeholder shape on
//! either tier is a logo caption or a running footer label and is ordinary content.
//! Both of those paint exactly as stated, and a projection built on its own copy of
//! that distinction would drift from the painter by the second edit — so this walks
//! `cascade_tiers` itself, with the same two filters, and the mirror and the
//! canvas cannot disagree about what the slide says.
//!
//! No shaper, no geometry and no theme resolution: this answers "what does the
//! slide SAY", which needs none of them, and that is what keeps it callable before
//! a font has loaded.

use casual_doc_model::NodeId;
use casual_pres_model::{PlaceholderKind, Presentation, SlideNode, TextRun};

use crate::cascade_tiers;

/// Which tier a text-bearing shape came from.
///
/// Carried rather than flattened because the two are not the same kind of thing to
/// a reader: the slide's shapes are its content, while text inherited from the
/// layout or the master is furniture that repeats on every slide in the deck — a
/// footer, a logo's caption, a running rule's label. A mirror that interleaved them
/// would make a reader hear the same footer between every pair of slides with no
/// way to tell it apart from the slide's own words.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SlideTextTier {
    /// The slide's own `p:spTree`.
    Slide,
    /// The slide layout's, for a shape in no placeholder slot.
    Layout,
    /// The slide master's, likewise.
    Master,
}

impl SlideTextTier {
    /// A stable token, for a host that has to name the tier.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Slide => "slide",
            Self::Layout => "layout",
            Self::Master => "master",
        }
    }
}

/// The accessibility ROLE a text-bearing shape projects to.
///
/// Four roles rather than the sixteen `ST_PlaceholderType` tokens, because a mirror
/// needs to know two things — "is this the slide's heading" and "is this its prose"
/// — and would only flatten the rest. The title fold is
/// [`PlaceholderKind::is_title`]'s, since `title` and `ctrTitle` are one slot
/// everywhere else in this engine too.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SlideTextRole {
    /// `title` or `ctrTitle`: the slide's heading.
    Title,
    /// `body`, `subTitle` or the schema-default `obj`: the slide's prose.
    Body,
    /// Any other slot — a footer, a date, a slide number.
    Other,
    /// No placeholder at all: a text box an author dropped on the slide. Its text is
    /// slide content and is read, but it is not the slide's heading.
    #[default]
    Shape,
}

impl SlideTextRole {
    /// The role a placeholder slot projects to.
    #[must_use]
    pub const fn of(kind: Option<PlaceholderKind>) -> Self {
        match kind {
            Some(kind) if kind.is_title() => Self::Title,
            Some(PlaceholderKind::Body | PlaceholderKind::SubTitle | PlaceholderKind::Object) => {
                Self::Body
            }
            Some(_) => Self::Other,
            None => Self::Shape,
        }
    }

    /// A stable token, for a host that has to name the role.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Body => "body",
            Self::Other => "other",
            Self::Shape => "shape",
        }
    }
}

/// One paragraph of a slide's text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlineParagraph {
    /// `a:pPr@lvl`, zero-based, as the file states it — so a mirror can nest a list
    /// rather than flattening every bullet to one depth.
    pub level: u8,
    /// The paragraph's text, runs and fields joined.
    pub text: String,
}

/// One text-bearing shape of a slide.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlineShape {
    /// The shape's own id, so a host can tie a mirrored element back to the drawing
    /// — which is what a later hit-test or caret will need.
    pub id: NodeId,
    /// Which tier the shape came from.
    pub tier: SlideTextTier,
    /// What the shape is to a reader.
    pub role: SlideTextRole,
    /// The shape's name (`p:cNvPr@name`), when the file carries one. A reader with
    /// no visual context has nothing else to distinguish two text boxes by.
    pub name: Option<String>,
    /// The paragraphs that carry text, in order.
    pub paragraphs: Vec<OutlineParagraph>,
}

/// A slide's text, in reading order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SlideTextOutline {
    /// The text-bearing shapes: the slide's own first, then the layout's inherited
    /// furniture, then the master's.
    pub shapes: Vec<OutlineShape>,
}

/// Projects the text of the slide at `index` into reading order.
///
/// Returns `None` only when `index` is past the end, matching
/// [`crate::lay_out_slide`].
///
/// # Reading order, and why it is not paint order
///
/// Paint order is master, then layout, then slide — the right order for pixels,
/// because a slide's shape must cover the layout's. It is the WRONG order for a
/// reader: it would announce the deck's footer and slide number before the slide's
/// own title, on every slide. So this emits the slide's own shapes first, in tree
/// order, and the inherited furniture after, nearest tier first. Within one tier the
/// order is the tree's, which is the order the selection pane shows and the order
/// PowerPoint itself reads — never a sort by position, which would be a second
/// layout with its own opinions about reading order and a wrong one is worse than
/// the file's own.
///
/// # Complexity
///
/// O(shapes on this slide + its layout + its master) and O(text) in what they carry.
/// Independent of the deck's length: nothing here walks another slide.
#[must_use]
pub fn slide_text_outline(presentation: &Presentation, index: usize) -> Option<SlideTextOutline> {
    let slide = presentation.slides().get(index)?;
    // `cascade_tiers` yields paint order, master first. Reversed here for reading
    // order, which puts the slide's own content first — and reversed rather than
    // rebuilt so there is still exactly one place that decides which trees are in
    // the cascade at all.
    let mut shapes = Vec::new();
    for (position, tier) in cascade_tiers(presentation, slide)
        .into_iter()
        .rev()
        .enumerate()
    {
        // Distance from the slide, which identifies the tier without a second
        // source of truth about the chain's shape: the master is reached THROUGH the
        // layout, so a present master implies a present layout and the reversed
        // order is always slide, layout, master — never slide, master.
        let which = match position {
            0 => SlideTextTier::Slide,
            1 => SlideTextTier::Layout,
            _ => SlideTextTier::Master,
        };
        for child in &tier.tree.children {
            // The same two filters the painter applies, for the same reasons: a
            // hidden shape is not read because the author hid it, and a
            // placeholder's text on a layout or a master is the prompt.
            if child.hidden || !tier.paints_text(child) {
                continue;
            }
            if let Some(shape) = outline_shape(child, which) {
                shapes.push(shape);
            }
        }
    }
    Some(SlideTextOutline { shapes })
}

/// One shape's text, or `None` when it carries none a reader would hear.
fn outline_shape(node: &SlideNode, tier: SlideTextTier) -> Option<OutlineShape> {
    let body = node.text.as_ref()?;
    let paragraphs: Vec<OutlineParagraph> = body
        .paragraphs
        .iter()
        .filter_map(|paragraph| {
            let text = paragraph_text(paragraph);
            // An empty paragraph contributes nothing to a reader. It stays in the
            // model — it carries its own mark properties and a blank line's height,
            // and it round-trips — but announcing a blank line is noise.
            (!text.trim().is_empty()).then_some(OutlineParagraph {
                level: paragraph.level(),
                text,
            })
        })
        .collect();
    if paragraphs.is_empty() {
        return None;
    }
    Some(OutlineShape {
        id: node.id(),
        tier,
        role: SlideTextRole::of(node.placeholder.map(|slot| slot.kind)),
        name: node.name.clone(),
        paragraphs,
    })
}

/// One paragraph's runs, joined.
///
/// A field contributes its CACHED text, which is what every consumer but PowerPoint
/// displays: a slide number renders from that cache, so a reader hearing nothing
/// where the slide plainly says "7" would be the field's loss read aloud.
///
/// A soft break becomes a SPACE rather than a newline. The mirror puts each
/// paragraph in its own element, so a newline inside one is whitespace the
/// accessibility tree collapses anyway — and `TextRun::text` returns `"\n"` for a
/// break because its caller is plain-text extraction, where the newline is the
/// point. Two different answers to two different questions, which is why this does
/// not reuse it.
fn paragraph_text(paragraph: &casual_pres_model::TextParagraph) -> String {
    let mut text = String::new();
    for run in &paragraph.runs {
        match run {
            TextRun::Run(run) => text.push_str(&run.text),
            TextRun::Field(field) => text.push_str(&field.text),
            TextRun::LineBreak(_) => text.push(' '),
        }
    }
    text
}
