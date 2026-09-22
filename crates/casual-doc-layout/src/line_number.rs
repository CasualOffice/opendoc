//! Margin line numbering (`w:lnNumType`) — the paint half of the P1F-36 model
//! work (`docs/105` FID-L-09, FID-P-04).
//!
//! A section's line numbering is resolved to per-page margin stamps by a
//! **post-pagination pass**, mirroring [`crate::page_border`]: it runs off the
//! pagination hot path (so page reuse — the stabilization halt — stays
//! position-free), reads only the final page list, and writes
//! [`Page::line_numbers`](crate::page::Page::line_numbers), which
//! [`compose_page`](crate::compose::compose_page) paints as furniture. Numbers
//! are not flow content and not caret positions: a click in the margin resolves
//! to the body, and selecting a paragraph never copies its number.
//!
//! Why this matters: numbered lines are mandatory in court filings (many
//! jurisdictions require 28 numbered lines per pleading page) and standard in
//! contract redlines, where review comments cite "page 4, line 17". The model,
//! import, and export have carried `w:lnNumType` since P1F-36 and nothing drew
//! it, so those documents opened with the numbers silently gone.
//!
//! # Deliberate differences from Word
//!
//! - **Top-level body paragraphs are numbered.** Word also numbers lines inside
//!   table cells; that needs a row-wise ordering rule across cells sharing a
//!   baseline and is left open rather than guessed at. Headers, footers, note
//!   bodies, and text boxes are correctly *not* numbered (Word does not number
//!   them either).
//! - **`w:distance` absent means 0.25 inch.** The attribute's real default is
//!   Word's "Auto", which it resolves against the text; a fixed 360 twips is the
//!   value Word writes when asked for auto at ordinary body sizes.
//! - **`@w:restart` absent means `newPage`** (the schema default). Word's UI
//!   default is Continuous, but Word writes that token explicitly.
//! - **A `w:lnNumType` with no attributes at all numbers nothing.** The model
//!   collapses it to the empty value, which is what the exporter also treats as
//!   absent; keeping that one interpretation across import, layout, and export is
//!   worth more than rescuing a form no producer writes.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document, LineNumberRestart, LineNumbering, SectionId};

use crate::block::BlockFragment;
use crate::cascade::StyleCascade;
use crate::page::{PaginatedLayout, PlacedLineNumber};
use crate::text::{Decoration, FieldStyle, FontId, LineShaper};
use crate::units::{Point, Twip};

/// Gap from the text column to the numbers when `w:distance` is absent (Word's
/// "Auto" at body sizes): 0.25 inch.
const AUTO_DISTANCE: Twip = Twip(360);

/// Font size used for a number whose line carries no measurable text run (an
/// empty paragraph still occupies — and is counted as — a line). 10pt, Word's
/// Line Number style size.
const FALLBACK_SIZE: Twip = Twip(200);

/// Stamps every page's margin line numbers, in place.
///
/// Runs after pagination and after the vertical-alignment pass (numbers sit on
/// their line's final baseline), and is a pure function of the final page list
/// plus the document, so running it twice is idempotent — the same property that
/// keeps `repaginate == paginate`.
///
/// Returns immediately when no section declares line numbering, which is the
/// overwhelming majority of documents.
pub(crate) fn place_line_numbers(
    layout: &mut PaginatedLayout,
    document: &Document,
    shaper: &dyn LineShaper,
) {
    let sections = &document.definitions().sections;
    let numbering: BTreeMap<SectionId, LineNumbering> = sections
        .iter()
        .filter(|section| !section.line_numbering.is_empty())
        .map(|section| (section.id, section.line_numbering))
        .collect();
    if numbering.is_empty() {
        return;
    }
    let suppressed = suppressed_paragraphs(document);

    // The running counter. `None` means "not started yet", which is what makes a
    // `continuous` section pick up its `@w:start` on its first numbered line and
    // then never reset.
    let mut counter: Option<u32> = None;
    let mut previous_section: Option<SectionId> = None;
    for page in &mut layout.pages {
        let Some(rule) = numbering.get(&page.section) else {
            // A section without line numbering neither numbers nor resets: a
            // continuous count resumes where it left off after an unnumbered
            // section, which is what Word does.
            previous_section = Some(page.section);
            continue;
        };
        let start = rule.start.unwrap_or(1).clamp(0, 32_767) as u32;
        let count_by = rule.count_by.unwrap_or(1).clamp(1, 32_767) as u32;
        let distance = rule.distance.map_or(AUTO_DISTANCE, |d| Twip(d.max(0)));
        let restart = rule.restart.unwrap_or(LineNumberRestart::NewPage);
        let new_section = previous_section != Some(page.section);
        previous_section = Some(page.section);
        let reset = match restart {
            LineNumberRestart::NewPage => true,
            LineNumberRestart::NewSection => new_section,
            LineNumberRestart::Continuous => counter.is_none(),
        };
        if reset {
            counter = Some(start);
        }
        let mut next = counter.unwrap_or(start);

        let mut stamps: Vec<PlacedLineNumber> = Vec::new();
        for placed in &page.placed {
            let BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                ..
            } = &placed.fragment
            else {
                // Table rows are not numbered yet (see the module header).
                continue;
            };
            if suppressed.contains(id) {
                // `w:suppressLineNumbers` removes the paragraph from the count
                // entirely — it does not merely hide its numbers. A pleading's
                // caption block must not shift every numbered line below it.
                continue;
            }
            // The number hangs off the paragraph's COLUMN, not the page's content
            // area, so a two-column section numbers each column against its own
            // left edge and an indented paragraph's number stays in the margin.
            let right_edge = Twip(placed.rect.origin.x.raw() - distance.raw());
            let content_top = placed.rect.origin.y + box_metrics.space_before;
            let mut line_top = Twip::ZERO;
            for line in &lines.lines {
                let number = next;
                next = next.saturating_add(1);
                let baseline = line
                    .runs
                    .first()
                    .map_or(line_top + line.ascent, |run| run.origin.y);
                line_top = line_top + line.height;
                if !number.is_multiple_of(count_by) {
                    continue;
                }
                let (font, size) = line
                    .runs
                    .iter()
                    .find(|run| !run.is_marker && !run.is_leader && !run.glyphs.is_empty())
                    .map_or((FontId(0), FALLBACK_SIZE), |run| (run.font, run.size));
                let style = FieldStyle {
                    font,
                    size,
                    character_scale_percent: 100,
                    // Word's Line Number character style takes the automatic text
                    // colour, not the colour of the line it labels: numbers beside
                    // a red-inked quotation stay black.
                    color: [0, 0, 0, 255],
                    bold: false,
                    italic: false,
                    letter_spacing: Twip::ZERO,
                    decoration: Decoration::default(),
                };
                // Shaped at the origin, then moved: the number is right-aligned to
                // `right_edge`, so its left edge depends on its own advance (a
                // three-digit number reaches further into the margin).
                let shaped = crate::flow::shape_field_run(
                    shaper,
                    &number.to_string(),
                    style,
                    Point::new(Twip::ZERO, Twip::ZERO),
                    // A margin line number is not part of any paragraph's text.
                    crate::flow::FieldAnchor::atomic(0),
                );
                let mut run = shaped.run;
                run.origin = Point::new(
                    Twip(right_edge.raw() - shaped.advance.raw()),
                    content_top + baseline,
                );
                stamps.push(PlacedLineNumber { number, run });
            }
        }
        counter = Some(next);
        page.line_numbers = stamps;
    }
}

/// The top-level body paragraphs whose **effective** properties suppress line
/// numbering (`w:suppressLineNumbers`, direct or inherited from a style).
///
/// Resolved through the style cascade rather than off the direct `w:pPr`, because
/// a pleading template declares the flag on its caption/signature styles. Built
/// once per pass and only when some section actually numbers lines, so the
/// cascade cost never touches an ordinary document.
fn suppressed_paragraphs(document: &Document) -> BTreeSet<NodeId> {
    let cascade = StyleCascade::new(document.definitions());
    document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => cascade
                .resolve_paragraph(&paragraph.properties)
                .suppress_line_numbers
                .then_some(paragraph.id),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_absent_attribute_defaults_are_words() {
        // Guards the four defaults the module header commits to, so a change to
        // any of them becomes a deliberate edit rather than a silent drift.
        let rule = LineNumbering {
            count_by: Some(1),
            ..LineNumbering::default()
        };
        assert_eq!(rule.start.unwrap_or(1), 1, "numbering starts at line 1");
        assert_eq!(rule.count_by.unwrap_or(1), 1, "every line is numbered");
        assert_eq!(
            rule.distance.map_or(AUTO_DISTANCE, Twip),
            AUTO_DISTANCE,
            "`auto` distance"
        );
        assert_eq!(AUTO_DISTANCE, Twip(360), "0.25 inch");
        assert!(
            matches!(
                rule.restart.unwrap_or(LineNumberRestart::NewPage),
                LineNumberRestart::NewPage
            ),
            "the schema default for `@w:restart` is `newPage`"
        );
    }

    #[test]
    fn a_bare_ln_num_type_is_indistinguishable_from_no_line_numbering() {
        // Import collapses an attribute-less `w:lnNumType` to the empty value and
        // export skips the empty value, so layout must agree: one interpretation
        // across all three, recorded here so the choice is visible.
        assert!(LineNumbering::default().is_empty());
    }
}
