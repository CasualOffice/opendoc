//! The seam FID-P-03's loss-coverage guard reads the *source* package through.
//!
//! ## Why this is public, and why it is here rather than in the test
//!
//! The guard asks one question of a save: **did any element name present in the
//! source vanish from the written package without a compatibility-report entry
//! naming it?** Answering it needs the element names of the source — but not all
//! of them. `35-DISPOSITION-TAXONOMY.md`'s *no-op class* is markup whose absence
//! is not a loss (`w:proofErr`, an empty `a:effectLst`, an all-zero
//! `wp:effectExtent`), and a guard that counted those would fail on every healthy
//! document and be switched off within a week.
//!
//! The no-op class is therefore consulted, not copied. A second copy of the policy
//! in a test file would drift from the one the importer applies, and the drift
//! would be invisible in exactly the direction that matters: the test would go on
//! excusing a name the importer had started reporting, or start failing on one the
//! importer had correctly silenced. So the walk lives beside the policy, and the
//! guard calls [`meaningful_markup`].
//!
//! ## Prior art
//!
//! This is *coverage instrumentation with a justified allowlist* — the shape of a
//! code-coverage report paired with a lint suppression file that requires a reason
//! per entry (`#[allow(…, reason = "…")]`, `eslint-disable-next-line -- why`,
//! `cargo-deny`'s `[advisories] ignore`). The property that keeps such a list from
//! rotting is not the reasons but the **staleness check**: an entry that no longer
//! fires is itself an error (`--report-unused-disable-directives`). The guard
//! implements that too, so the exception list cannot quietly accumulate.
//!
//! ## Complexity
//!
//! [`meaningful_markup`] is `O(bytes)` in the part, with `O(depth)` extra work per
//! element for the ancestor bookkeeping (depth is bounded by
//! `ImportConfig::max_depth`). Nothing here runs at edit time: it is a save-time
//! and test-time audit over a whole package, never per keystroke, and it is linear
//! in the package rather than quadratic in its parts — each part is walked once
//! and merged into an accumulator, with no lookup-by-name inside the walk.

use std::collections::{BTreeMap, BTreeSet};

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::noop;

/// The element names in some XML that carry document meaning, plus the ancestry
/// needed to tell a subtree loss from an unreported one.
///
/// Built per part by [`meaningful_markup`] and merged across a package with
/// [`MeaningfulMarkup::absorb`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MeaningfulMarkup {
    elements: BTreeSet<String>,
    common_ancestors: BTreeMap<String, BTreeSet<String>>,
}

impl MeaningfulMarkup {
    /// Every element local name that carries document meaning, deduplicated.
    ///
    /// Local names only: a namespace *prefix* is whatever the producer's `xmlns:`
    /// bound it to, so comparing prefixes would compare writers rather than
    /// constructs — the same reason `FeatureLocation` records local names.
    #[must_use]
    pub const fn elements(&self) -> &BTreeSet<String> {
        &self.elements
    }

    /// The element names that enclose **every** occurrence of `element`.
    ///
    /// `35` permits a whole-subtree loss to be "reported once on its outermost
    /// element", with its descendants skipped — which is how a bézier curve inside
    /// an `a:custGeom` is accounted for by one `custGeom` finding rather than one
    /// finding per path command. The coverage guard therefore treats a vanished
    /// name as covered when one of these ancestors is reported. The *intersection*
    /// across occurrences is what makes that sound: if a name also occurs
    /// somewhere the ancestor does not enclose, that occurrence is not covered and
    /// the ancestor drops out of the set.
    ///
    /// `None` when `element` does not carry document meaning here.
    #[must_use]
    pub fn ancestors_of_every_occurrence(&self, element: &str) -> Option<&BTreeSet<String>> {
        self.common_ancestors.get(element)
    }

    /// Merges another part's markup into this one.
    ///
    /// Element names union; common ancestors intersect where a name appears in
    /// both, because an occurrence in a second part is another occurrence the
    /// ancestor has to enclose.
    pub fn absorb(&mut self, other: Self) {
        for (element, ancestors) in other.common_ancestors {
            match self.common_ancestors.get_mut(&element) {
                Some(existing) => existing.retain(|name| ancestors.contains(name)),
                None => {
                    self.common_ancestors.insert(element, ancestors);
                }
            }
        }
        self.elements.extend(other.elements);
    }
}

/// Collects the element names in one XML part that carry document meaning.
///
/// Markup in `35-DISPOSITION-TAXONOMY.md`'s no-op class is excluded, judged the
/// way the importer judges it — by name where the name settles it, and by the
/// element itself where it does not (an empty `a:effectLst` is "no effects"; a
/// populated one is a lost shadow). A name is kept if **any** occurrence carries
/// meaning, which is the direction that cannot hide a loss.
///
/// Malformed XML stops the walk and returns what was collected. Callers hand this
/// parts of an admitted package, whose well-formedness the package reader has
/// already established; returning a partial answer rather than an error keeps the
/// audit from being the thing that fails on input the importer itself accepted.
///
/// `O(bytes)`, with `O(depth)` per element for the ancestor sets.
#[must_use]
pub fn meaningful_markup(xml: &[u8]) -> MeaningfulMarkup {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut markup = MeaningfulMarkup::default();
    // The names of the elements currently open, outermost first: the ancestry of
    // whatever the walk is looking at.
    let mut open: Vec<String> = Vec::new();
    // `Err` ends the walk: a truncated part yields the names read so far.
    while let Ok(event) = reader.read_event_into(&mut buffer) {
        match &event {
            Event::Eof => break,
            Event::Start(element) | Event::Empty(element) => {
                // `Event::Empty` is `<x/>`; `Event::Start` is `<x>`, whose
                // childless form `<x></x>` deliberately reads as NOT self-closing,
                // because that is how the importer reads it.
                let self_closing = matches!(event, Event::Empty(_));
                let local_name = element.local_name();
                let local = String::from_utf8_lossy(local_name.as_ref()).into_owned();
                let carries = !noop::carries_no_meaning(local_name.as_ref())
                    && !noop::carries_no_meaning_when(local_name.as_ref(), element, self_closing);
                if carries {
                    match markup.common_ancestors.get_mut(&local) {
                        Some(existing) => existing.retain(|name| open.contains(name)),
                        None => {
                            markup
                                .common_ancestors
                                .insert(local.clone(), open.iter().cloned().collect());
                        }
                    }
                    markup.elements.insert(local.clone());
                }
                if !self_closing {
                    open.push(local);
                }
            }
            Event::End(_) => {
                open.pop();
            }
            _ => {}
        }
        buffer.clear();
    }
    markup
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The walk excludes the no-op class **as the importer judges it**, including
    /// the conditional half: an all-zero `wp:effectExtent` says the ink stops at
    /// the frame and is not markup whose absence is a loss, while a non-zero one
    /// is. If this ever diverged, the coverage guard would either excuse a real
    /// loss or fail on a healthy document.
    #[test]
    fn the_no_op_class_is_excluded_exactly_as_the_importer_judges_it() {
        let silent = meaningful_markup(
            br#"<w:document><w:body><w:p><w:proofErr w:type="spellStart"/>
                <w:drawing><wp:inline><wp:effectExtent l="0" t="0" r="0" b="0"/>
                <wp:cNvGraphicFramePr><a:graphicFrameLocks/></wp:cNvGraphicFramePr>
                </wp:inline></w:drawing></w:p></w:body></w:document>"#,
        );
        for excluded in [
            "proofErr",
            "effectExtent",
            "cNvGraphicFramePr",
            "graphicFrameLocks",
        ] {
            assert!(
                !silent.elements().contains(excluded),
                "{excluded} is in the no-op class and must not be demanded of the writer"
            );
        }
        let loud = meaningful_markup(
            br#"<w:document><w:body><w:p><w:drawing><wp:inline>
                <wp:effectExtent l="0" t="0" r="114300" b="0"/>
                <wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr>
                </wp:inline></w:drawing></w:p></w:body></w:document>"#,
        );
        assert!(
            loud.elements().contains("effectExtent"),
            "a non-zero effect extent is a real difference and must be demanded"
        );
        assert!(
            loud.elements().contains("graphicFrameLocks"),
            "a lock that locks something is a real restriction and must be demanded"
        );
    }

    /// A name enclosed by one construct everywhere it appears is covered by a
    /// finding on that construct; a name that also appears outside it is not.
    #[test]
    fn a_common_ancestor_survives_only_while_it_encloses_every_occurrence() {
        let inside = meaningful_markup(
            br#"<a:custGeom><a:pathLst><a:path><a:cubicBezTo><a:pt x="1" y="2"/></a:cubicBezTo></a:path></a:pathLst></a:custGeom>"#,
        );
        assert!(
            inside
                .ancestors_of_every_occurrence("cubicBezTo")
                .expect("the curve carries meaning")
                .contains("custGeom"),
            "a curve inside a custom geometry is covered by the geometry's finding"
        );

        let mut both = inside;
        both.absorb(meaningful_markup(
            br#"<a:prstGeom><a:cubicBezTo/></a:prstGeom>"#,
        ));
        assert!(
            !both
                .ancestors_of_every_occurrence("cubicBezTo")
                .expect("the curve carries meaning")
                .contains("custGeom"),
            "an occurrence outside the geometry is not covered by the geometry's finding"
        );
    }

    /// A self-closing element has no `End` event, so it must not be pushed onto the
    /// ancestor stack — a stuck stack would make later siblings look like
    /// descendants and excuse them.
    #[test]
    fn a_self_closing_element_is_not_an_ancestor_of_its_siblings() {
        let markup = meaningful_markup(br#"<w:body><w:tab/><w:br/></w:body>"#);
        assert_eq!(
            markup.ancestors_of_every_occurrence("br"),
            Some(&["body".to_owned()].into_iter().collect())
        );
    }

    /// Truncated XML returns what was read rather than nothing: the audit must not
    /// be the thing that fails on input the importer accepted.
    #[test]
    fn a_truncated_part_yields_the_names_read_before_the_break() {
        let markup = meaningful_markup(br#"<w:body><w:p><w:r><w:t>x"#);
        assert!(markup.elements().contains("p"));
    }
}
