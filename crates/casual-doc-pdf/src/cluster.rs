//! The source characters behind a glyph, recovered from the document.
//!
//! A `ToUnicode` CMap built by inverting a face's own character map is exact for
//! text where one character shapes to one glyph, and cannot describe the two
//! cases that matter most for copy and search:
//!
//! - a **ligature** is one glyph standing for several characters, so no single
//!   code point names it (and where the face happens to map the ligature to a
//!   presentation form — Roboto maps its `fi` glyph to `U+FB01` — the inversion
//!   produces a character the document does not contain, so "fixture" copies as
//!   "ﬁxture" and a search for "fixture" misses it);
//! - a glyph reached only by **GSUB substitution** may map from no code point at
//!   all, so it copies as nothing.
//!
//! Both are answered by the same fact: [`GlyphRun::node`] names the paragraph
//! whose text the run's [`Glyph::cluster`] offsets index, so the real characters
//! are in the model the exporter already holds. This module walks the display
//! lists once, resolves each cluster to its substring, and hands the result to
//! [`crate::font`] as a per-face `glyph id -> text` map.
//!
//! [`GlyphRun::node`]: casual_doc_layout::text::GlyphRun::node
//! [`Glyph::cluster`]: casual_doc_layout::text::Glyph::cluster

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;

use casual_doc_edit::ParagraphIndex;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::flow::node_plain_text;
use casual_doc_layout::text::FontId;
use casual_doc_layout::text::GlyphRun;
use casual_doc_model::NodeId;
use casual_doc_model::v1::Document;

use crate::content::PdfPage;

/// Supplies the text a paragraph node contributes to layout, in the **same byte
/// layout** the shaper indexed — the layout that [`Glyph::cluster`] offsets
/// address.
///
/// [`Glyph::cluster`]: casual_doc_layout::text::Glyph::cluster
pub trait PdfTextSource {
    /// The text of paragraph `node`, or `None` when the source does not know it.
    ///
    /// Called at most **once per node** per export, so an implementation may
    /// materialize the string rather than cache one.
    fn node_text(&self, node: NodeId) -> Option<String>;
}

/// A [`PdfTextSource`] that knows nothing — the behaviour of an export driven
/// from display lists alone ([`crate::write_pdf`]), where no document is at hand.
/// Every glyph then falls back to the face's character map, exactly as before
/// this module existed.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTextSource;

impl PdfTextSource for NoTextSource {
    fn node_text(&self, _node: NodeId) -> Option<String> {
        None
    }
}

/// A [`PdfTextSource`] over a whole document, backed by one paragraph index.
///
/// # Complexity
///
/// `new` is **O(blocks)**: a single walk of every surface, recording a pointer
/// per paragraph — no text is copied. [`node_text`](Self::node_text) is **O(1)**
/// to find the paragraph plus O(that paragraph's text) to materialize it.
///
/// The index exists precisely so resolution is not a scan: the walking helpers
/// (`find_paragraph_any`, `paragraph_properties`) read like accessors at the call
/// site and are linear in document length, so calling one per glyph run would be
/// quadratic — the defect `documentOutline` shipped, ~1.7 × 10¹² block visits on
/// a 1.3M-paragraph document.
#[derive(Debug)]
pub struct DocumentText<'a> {
    by_id: ParagraphIndex<'a>,
}

impl<'a> DocumentText<'a> {
    /// Indexes every paragraph of `document`, on every surface.
    #[must_use]
    pub fn new(document: &'a Document) -> Self {
        Self {
            by_id: ParagraphIndex::build(document),
        }
    }
}

impl PdfTextSource for DocumentText<'_> {
    fn node_text(&self, node: NodeId) -> Option<String> {
        self.by_id
            .paragraph(node)
            .map(|paragraph| node_plain_text(&paragraph.inlines))
    }
}

/// The exact source text of every glyph the exported pages draw, keyed by face
/// and glyph id.
#[derive(Debug, Default)]
pub(crate) struct ClusterText {
    by_face: BTreeMap<u32, BTreeMap<u16, String>>,
}

impl ClusterText {
    /// Resolves the source text behind every glyph on `pages`.
    ///
    /// # Complexity
    ///
    /// **O(G log C + N + T)** for `G` glyphs on the exported pages, `C` distinct
    /// cluster offsets within one paragraph, `N` paragraphs those pages touch and
    /// `T` bytes of their text: two walks of the display lists (the first
    /// collecting cluster boundaries, which a glyph's text range needs, the second
    /// resolving), and **one** [`PdfTextSource::node_text`] call per node — never
    /// a lookup-by-id inside a loop over ids.
    pub(crate) fn build(pages: &[PdfPage<'_>], source: &dyn PdfTextSource) -> Self {
        // Pass one: every cluster offset each node's glyphs carry. A glyph's text
        // runs from its own cluster to the next DIFFERENT cluster in that node, and
        // "next" is only knowable once they have all been seen: the last cluster of
        // a run is not the last cluster of the paragraph, and a right-to-left run
        // carries its glyphs in visual order, so neither the run's glyph order nor
        // its extent can supply the boundary.
        let mut boundaries: HashMap<NodeId, BTreeSet<u32>> = HashMap::new();
        for_each_run(pages, |run| {
            if let Some(node) = run.node {
                let offsets = boundaries.entry(node).or_default();
                for glyph in &run.glyphs {
                    offsets.insert(glyph.cluster);
                }
            }
        });

        // One text materialization per node, keyed for O(1) reuse across its runs.
        let texts: HashMap<NodeId, String> = boundaries
            .keys()
            .filter_map(|node| source.node_text(*node).map(|text| (*node, text)))
            .collect();

        // Pass two: each cluster's substring, offered as a candidate for the glyph
        // that stands for it.
        let mut out = Self::default();
        for_each_run(pages, |run| {
            let Some(node) = run.node else { return };
            let (Some(text), Some(offsets)) = (texts.get(&node), boundaries.get(&node)) else {
                return;
            };
            for (cluster, glyph) in solitary_clusters(run) {
                if let Some(slice) = cluster_slice(text, offsets, cluster) {
                    out.offer(run.font, glyph, slice);
                }
            }
        });
        out
    }

    /// The `glyph id -> source text` map for one face, if any of its glyphs were
    /// resolved.
    pub(crate) fn face(&self, font: FontId) -> Option<&BTreeMap<u16, String>> {
        self.by_face.get(&font.0)
    }

    /// Records `text` as the source of `glyph` in `font`.
    ///
    /// One glyph id can be reached from different text — a face commonly maps both
    /// `U+0020` and `U+00A0` to its space glyph — and a `ToUnicode` map is keyed by
    /// glyph id, not by position, so exactly one reading has to win. The longer
    /// reading wins, because a shorter candidate for the same glyph is a reading
    /// that lost characters (a ligature's `fi` beats a bare `f`); ties go to the
    /// lexicographically smaller text, which for single characters is the lower
    /// code point — the same rule the character-map inversion already applies, so
    /// the two agree wherever both have an answer.
    fn offer(&mut self, font: FontId, glyph: u16, text: &str) {
        let face = self.by_face.entry(font.0).or_default();
        let better_already = face.get(&glyph).is_some_and(|held| {
            let (held_chars, candidate_chars) = (held.chars().count(), text.chars().count());
            held_chars > candidate_chars || (held_chars == candidate_chars && held.as_str() <= text)
        });
        if !better_already {
            face.insert(glyph, text.to_owned());
        }
    }
}

/// Calls `visit` for every non-empty glyph run on `pages`, in page order.
fn for_each_run(pages: &[PdfPage<'_>], mut visit: impl FnMut(&GlyphRun)) {
    for page in pages {
        for item in &page.list.items {
            if let PaintItem::Glyphs { run } = item
                && !run.glyphs.is_empty()
            {
                visit(run);
            }
        }
    }
}

/// The clusters of `run` that exactly **one** glyph stands for, with that glyph's
/// id: `(cluster offset, glyph id)`.
///
/// A cluster drawn by several glyphs is deliberately skipped. `ToUnicode` maps a
/// glyph id to text wherever that id appears, so giving the whole cluster's text
/// to the first of its glyphs would emit that text again for every later glyph
/// that is itself mapped — a decomposed `é` would copy as `é` followed by a second
/// combining acute. Left alone, each glyph keeps its own character-map entry and
/// the cluster copies as the base character followed by its marks, which is
/// canonically equivalent to the cluster. The cases this module exists to fix are
/// the opposite shape — several characters collapsing into ONE glyph — so nothing
/// is lost by the restriction.
///
/// A glyph whose id does not fit a CID (`u16`) is skipped: the content stream
/// clamps it, so no `ToUnicode` entry could name the glyph that was drawn.
fn solitary_clusters(run: &GlyphRun) -> impl Iterator<Item = (u32, u16)> + '_ {
    run.glyphs
        .iter()
        .enumerate()
        .filter(|(index, glyph)| {
            // A cluster's glyphs are contiguous in the run, in visual order, so
            // "alone in its cluster" is a neighbour test.
            let before = index
                .checked_sub(1)
                .and_then(|before| run.glyphs.get(before));
            let after = run.glyphs.get(index + 1);
            before.is_none_or(|other| other.cluster != glyph.cluster)
                && after.is_none_or(|other| other.cluster != glyph.cluster)
        })
        .filter_map(|(_, glyph)| u16::try_from(glyph.id).ok().map(|id| (glyph.cluster, id)))
}

/// The substring `cluster` covers: from its own offset to the next distinct
/// cluster offset in the same node, or to the end of the node's text for the last
/// one.
///
/// Returns `None` when the range is not a character range of this text — an offset
/// past its end, or one that is not a UTF-8 boundary. That is a divergence between
/// the model text and the text layout shaped (a `w:vanish` run contributes model
/// bytes but no glyphs, and `w:caps` rewrites the case), and the honest answer is
/// to leave such a glyph to the character map rather than name it with characters
/// from somewhere else in the paragraph.
fn cluster_slice<'a>(text: &'a str, offsets: &BTreeSet<u32>, cluster: u32) -> Option<&'a str> {
    let start = usize::try_from(cluster).ok()?;
    let end = offsets
        .range((cluster + 1)..)
        .next()
        .and_then(|next| usize::try_from(*next).ok())
        .unwrap_or(text.len())
        .min(text.len());
    if start >= end {
        return None;
    }
    text.get(start..end).filter(|slice| !slice.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_layout::text::Decoration;
    use casual_doc_layout::text::Glyph;
    use casual_doc_layout::units::Point;
    use casual_doc_layout::units::Twip;

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).expect("a valid node id")
    }

    fn run_of(node: Option<NodeId>, glyphs: &[(u32, u32)]) -> GlyphRun {
        GlyphRun {
            node,
            font: FontId(0),
            size: Twip(240),
            ascent: Twip::ZERO,
            descent: Twip::ZERO,
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::new(Twip::ZERO, Twip::ZERO),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            is_marker: false,
            is_leader: false,
            glyphs: glyphs
                .iter()
                .map(|(id, cluster)| Glyph {
                    id: *id,
                    advance: Twip(100),
                    cluster: *cluster,
                    is_whitespace: false,
                })
                .collect(),
        }
    }

    #[test]
    fn a_ligature_glyph_claims_every_character_of_its_cluster() {
        // "fixture": the `fi` ligature is one glyph at cluster 0, and the next
        // cluster is 2, so the glyph stands for both bytes.
        let run = run_of(Some(node(7)), &[(1834, 0), (93, 2), (89, 3)]);
        let offsets: BTreeSet<u32> = [0, 2, 3].into_iter().collect();
        let mut solitary = solitary_clusters(&run);
        assert_eq!(solitary.next(), Some((0, 1834)));
        assert_eq!(cluster_slice("fixt", &offsets, 0), Some("fi"));
        assert_eq!(cluster_slice("fixt", &offsets, 2), Some("x"));
        // The last cluster runs to the end of the node's text.
        assert_eq!(cluster_slice("fixt", &offsets, 3), Some("t"));
    }

    #[test]
    fn a_cluster_drawn_by_several_glyphs_claims_nothing() {
        // A decomposed cluster: two glyphs share cluster 0, so neither may claim
        // the whole text -- the map is keyed by glyph id, so both would emit it.
        let run = run_of(Some(node(7)), &[(40, 0), (41, 0), (42, 3)]);
        let solitary: Vec<_> = solitary_clusters(&run).collect();
        assert_eq!(
            solitary,
            vec![(3, 42)],
            "only the lone glyph claims a cluster"
        );
    }

    #[test]
    fn an_offset_outside_the_nodes_text_resolves_to_nothing() {
        let offsets: BTreeSet<u32> = [0, 9].into_iter().collect();
        assert_eq!(cluster_slice("abc", &offsets, 9), None, "past the end");
        // Not a UTF-8 boundary: the second byte of a two-byte character.
        let offsets: BTreeSet<u32> = [1].into_iter().collect();
        assert_eq!(cluster_slice("é", &offsets, 1), None);
    }

    #[test]
    fn the_longer_reading_of_one_glyph_wins_and_ties_go_to_the_lower_code_point() {
        let mut text = ClusterText::default();
        text.offer(FontId(0), 5, "f");
        text.offer(FontId(0), 5, "fi");
        text.offer(FontId(0), 5, "f");
        assert_eq!(
            text.face(FontId(0)).and_then(|face| face.get(&5)),
            Some(&"fi".to_owned()),
            "the ligature reading survives a later single-character one"
        );
        // A face maps both U+0020 and U+00A0 to one space glyph; the lower code
        // point wins, which is what inverting the character map also answers.
        text.offer(FontId(0), 6, "\u{a0}");
        text.offer(FontId(0), 6, " ");
        assert_eq!(
            text.face(FontId(0)).and_then(|face| face.get(&6)),
            Some(&" ".to_owned())
        );
    }

    #[test]
    fn a_run_with_no_anchor_contributes_nothing() {
        let run = run_of(None, &[(1834, 0)]);
        assert!(run.node.is_none());
        let list = casual_doc_layout::display::DisplayList {
            items: vec![PaintItem::Glyphs { run }],
        };
        let pages = [PdfPage {
            width: Twip(12240),
            height: Twip(15840),
            list: &list,
        }];
        let text = ClusterText::build(&pages, &NoTextSource);
        assert!(text.face(FontId(0)).is_none());
    }
}
