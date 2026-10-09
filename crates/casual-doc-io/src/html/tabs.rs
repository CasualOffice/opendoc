//! Tab stops on a web page.
//!
//! A tab advances to a *stop*, and CSS has no stops: `tab-size` is the default
//! grid and nothing more. Two layouts cover the lines tabs are actually used
//! for, and each is built from the stop the tab goes to:
//!
//! - **A contents line** — text, a tab to a right-aligned stop, a page number —
//!   is a flex row: the text, a leader filler that takes the rest of the line
//!   (dotted, dashed or ruled as the stop's leader says), and the number, ending
//!   exactly at the stop. The same row is a "name … date" line, and a header's
//!   text with a right-aligned date, without the leader.
//! - **A header line** — left, a tab to a centre stop, a tab to a right stop —
//!   places each segment at its own stop: from its left edge for a left stop,
//!   centred on it for a centre stop, ending at it for a right or decimal stop.
//!
//! Anything else keeps the default grid and is reported (`html.tab_stop`).
//!
//! Tab `k` goes to the paragraph's `k`-th stop. Word moves a tab to the first
//! stop past the text before it, which needs the text's width; on the lines
//! above the two agree, which is why only those lines are laid out by stop.
//! Stop positions are measured from the page margin and segments are placed in
//! the paragraph's box, so each is offset by the paragraph's start indent.

use casual_doc_model::v1::{InlineNode, TabAlignment, TabLeader, TabStop};

use super::css::{Declarations, twips};

/// How a paragraph's tabs are laid out.
#[derive(Debug)]
pub(super) enum TabPlan {
    /// The default grid (`tab-size`): no tab, no custom stop, or a line the two
    /// layouts do not describe.
    Grid {
        /// Whether custom stops were set and could not be honoured.
        degraded: bool,
    },
    /// A flex row ending at a right-aligned stop, its last tab a leader.
    Leader {
        /// How the gap is filled.
        leader: Option<TabLeader>,
        /// Where the row ends, in twips from the paragraph's box.
        end: i64,
    },
    /// Each segment after a tab placed at its stop.
    Positioned(Vec<TabStop>),
}

/// The longest line, in characters, laid out by placing segments at their
/// stops. Placed segments do not wrap, so a long paragraph keeps the grid.
const POSITIONED_MAX_CHARS: usize = 120;

/// Decides how a paragraph's tabs are laid out. O(inlines + stops).
pub(super) fn plan(inlines: &[InlineNode], stops: &[TabStop], indent_start: i64) -> TabPlan {
    let tabs = count_tabs(inlines);
    let mut stops: Vec<TabStop> = stops
        .iter()
        .filter(|stop| !matches!(stop.alignment, TabAlignment::Clear | TabAlignment::Bar))
        .cloned()
        .collect();
    stops.sort_by_key(|stop| stop.position_twips);
    if tabs == 0 || stops.is_empty() {
        return TabPlan::Grid { degraded: false };
    }
    let mapped: Vec<Option<&TabStop>> = (0..tabs).map(|index| stops.get(index)).collect();
    if let Some(Some(last)) = mapped.last()
        && matches!(last.alignment, TabAlignment::End | TabAlignment::Decimal)
        && mapped[..tabs - 1]
            .iter()
            .all(|stop| stop.is_some_and(|stop| stop.alignment == TabAlignment::Start))
    {
        return TabPlan::Leader {
            leader: last.leader,
            end: i64::from(last.position_twips) - indent_start,
        };
    }
    if mapped.iter().all(Option::is_some) && visible_chars(inlines) <= POSITIONED_MAX_CHARS {
        return TabPlan::Positioned(mapped.into_iter().flatten().cloned().collect());
    }
    TabPlan::Grid { degraded: true }
}

/// The declarations that place the segment after a tab at its stop, in the
/// paragraph's box.
pub(super) fn placed(stop: &TabStop, indent_start: i64) -> Declarations {
    let at = i64::from(stop.position_twips) - indent_start;
    let mut css = Declarations::default();
    css.set("position", "absolute");
    css.set("top", "0");
    css.set("white-space", "nowrap");
    match stop.alignment {
        TabAlignment::Center => {
            css.set("left", twips(at));
            css.set("transform", "translateX(-50%)");
        }
        TabAlignment::End | TabAlignment::Decimal => {
            css.set("left", "0");
            css.set("width", twips(at.max(0)));
            css.set("text-align", "right");
        }
        _ => css.set("left", twips(at)),
    }
    css
}

/// The border a leader is drawn with, or `None` for a blank gap.
pub(super) fn leader_border(leader: Option<TabLeader>) -> Option<&'static str> {
    Some(match leader? {
        TabLeader::Dot | TabLeader::MiddleDot => "1.5px dotted currentColor",
        TabLeader::Hyphen => "1px dashed currentColor",
        TabLeader::Underscore => "1px solid currentColor",
        TabLeader::Heavy => "2px solid currentColor",
    })
}

/// The paragraph's inlines cut at every tab: `tabs + 1` segments, the tabs
/// themselves dropped. A tab inside a hyperlink, a field result, a content
/// control or a tracked change cuts that container in two, each half keeping
/// it — a contents entry's link stays a link on both sides of its leader.
/// O(inlines).
pub(super) fn split_at_tabs(inlines: &[InlineNode]) -> Vec<Vec<InlineNode>> {
    let mut segments = vec![Vec::new()];
    for inline in inlines {
        split_into(inline, &mut segments);
    }
    segments
}

fn split_into(inline: &InlineNode, segments: &mut Vec<Vec<InlineNode>>) {
    let children = match inline {
        InlineNode::Tab(_) => {
            segments.push(Vec::new());
            return;
        }
        InlineNode::Hyperlink(link) => &link.inlines,
        InlineNode::Field(field) => &field.inlines,
        InlineNode::Sdt(sdt) => &sdt.inlines,
        InlineNode::Revision(revision) => &revision.inlines,
        other => {
            if let Some(last) = segments.last_mut() {
                last.push(other.clone());
            }
            return;
        }
    };
    if count_tabs(children) == 0 {
        if let Some(last) = segments.last_mut() {
            last.push(inline.clone());
        }
        return;
    }
    for (index, part) in split_at_tabs(children).into_iter().enumerate() {
        if index > 0 {
            segments.push(Vec::new());
        }
        if part.is_empty() {
            continue;
        }
        let mut copy = inline.clone();
        match &mut copy {
            InlineNode::Hyperlink(link) => link.inlines = part,
            InlineNode::Field(field) => field.inlines = part,
            InlineNode::Sdt(sdt) => sdt.inlines = part,
            InlineNode::Revision(revision) => revision.inlines = part,
            _ => {}
        }
        if let Some(last) = segments.last_mut() {
            last.push(copy);
        }
    }
}

/// The tabs in a paragraph, through its inline containers.
pub(super) fn count_tabs(inlines: &[InlineNode]) -> usize {
    inlines
        .iter()
        .map(|inline| match inline {
            InlineNode::Tab(_) => 1,
            InlineNode::Hyperlink(link) => count_tabs(&link.inlines),
            InlineNode::Field(field) => count_tabs(&field.inlines),
            InlineNode::Sdt(sdt) => count_tabs(&sdt.inlines),
            InlineNode::Revision(revision) => count_tabs(&revision.inlines),
            _ => 0,
        })
        .sum()
}

fn visible_chars(inlines: &[InlineNode]) -> usize {
    inlines
        .iter()
        .map(|inline| match inline {
            InlineNode::Run(run) => run.text.chars().count(),
            InlineNode::Hyperlink(link) => visible_chars(&link.inlines),
            InlineNode::Field(field) => visible_chars(&field.inlines),
            InlineNode::Sdt(sdt) => visible_chars(&sdt.inlines),
            InlineNode::Revision(revision) => visible_chars(&revision.inlines),
            _ => 0,
        })
        .sum()
}
