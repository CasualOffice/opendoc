//! The WordprocessingML layer over the format-neutral loss taxonomy.
//!
//! The taxonomy itself — the two axes, the nine-pair [`Disposition`], the
//! preservation ledger, the aggregating sink and
//! [`CompatibilityReport::validate`] — lives in `casual-doc-loss` and is
//! re-exported here. It was moved out of this crate because a ledger only one
//! format can reach is a preservation claim only one format can make: ODT could
//! not report into it at all, and a presentation importer would have hit the same
//! wall (`docs/156` Tier 0 row 0.10). `casual-doc-export` re-exports the
//! vocabulary from *this* module, so these re-exports are the public path that
//! keeps the import and export halves of one format from drifting into two
//! spellings of one fact (FID-R-02).
//!
//! What stays here is the part that is genuinely about WordprocessingML, and the
//! split is the point of the seam:
//!
//! - the two document-level class identifiers ([`RSID_CLASS_FEATURE`],
//!   [`WATERMARK_CLASS_FEATURE`]), which are `docx.`-prefixed by design;
//! - which local names are revision-save-ID markup;
//! - the no-op class — whether an element's *value* equals a state the model
//!   already holds, which is a fact about DrawingML and WordprocessingML and
//!   cannot be decided by a format-neutral crate (`crate::noop`);
//! - turning XML byte-slice local names into bounded locations and feature
//!   identifiers.
//!
//! [`Reporter`] is therefore a thin vocabulary layer over
//! `casual_doc_loss::LossReporter`: it decides *what to call* a finding and
//! *whether there is one*, and the shared crate does the aggregating, the
//! ceiling, the resolution into a disposition and the ledger bookkeeping. There
//! is one implementation of each of those, not one per format.

pub use casual_doc_loss::{
    CompatibilityEntry, CompatibilityReport, Disposition, DispositionViolation, FeatureLocation,
    LedgerId, LedgerRecord, ModelOutcome, PartConstructDisposition, PartDisposition,
    PreservationKind, PreservationLedger, RetentionOutcome,
};
pub(crate) use casual_doc_loss::{Finding, SourceRetention, WholePartDisposition};

use casual_doc_loss::LossReporter;

/// Stable class identifier for revision-save IDs (`w:rsid*` attributes and the
/// `w:rsids`/`w:rsid` table in `settings.xml`). The reasoning for reporting the
/// class once per document rather than per construct is on `Reporter::report_rsid`
/// and in `35-DISPOSITION-TAXONOMY.md`.
pub const RSID_CLASS_FEATURE: &str = "docx.rsid";

/// Stable feature identifier for watermark markup that was recognised but could
/// not be lifted onto its section. The shape itself still reaches the float layer,
/// so this is a `degraded` finding, not an omission: what is lost is that the
/// stamp is a *watermark* — repeated on every page of the section, behind the
/// body, unselectable — and not that the ink disappeared.
///
/// It is a `docx.`-prefixed class rather than an element name because no element
/// names it: Word writes a watermark as a `v:shape` like any other, and the only
/// thing that identifies it is the shape's `id`. Reporting `v:shape` would tell a
/// caller nothing about which shape, or why.
pub const WATERMARK_CLASS_FEATURE: &str = "docx.watermark";

/// Whether `local` names revision-save-ID markup: the `w:rsids` table in
/// `settings.xml`, its `w:rsid` entries, its `w:rsidRoot`, and the per-style
/// `w:rsid` in `styles.xml`. All are members of the one class
/// [`Reporter::report_rsid`] describes.
fn is_revision_save_id_element(local: &[u8]) -> bool {
    matches!(local, b"rsid" | b"rsids" | b"rsidRoot")
}

/// Whether `local` names a revision-save-ID attribute (`w:rsidR`, `w:rsidRPr`,
/// `w:rsidRDefault`, `w:rsidP`, `w:rsidDel`, `w:rsidTr`, `w:rsidSect`, and any
/// future sibling): the same class, carried on an element rather than as one.
pub(crate) fn is_revision_save_id_attribute(local: &[u8]) -> bool {
    local.starts_with(b"rsid")
}

/// The XML local name as a feature identifier.
fn feature_of(local: &[u8]) -> String {
    String::from_utf8_lossy(local).into_owned()
}

/// A bounded location naming only an element.
fn element_location(local: &[u8]) -> FeatureLocation {
    FeatureLocation {
        part_name: None,
        element: Some(feature_of(local)),
        attribute: None,
    }
}

/// A bounded location naming an attribute of an element.
fn attribute_location(element: &[u8], attribute: &[u8]) -> FeatureLocation {
    FeatureLocation {
        part_name: None,
        element: Some(feature_of(element)),
        attribute: Some(feature_of(attribute)),
    }
}

/// The WordprocessingML report sink shared by every parser in this crate.
///
/// Holds a `casual_doc_loss::LossReporter` and adds the vocabulary: which byte
/// slices are classes rather than constructs, which carry no meaning at all, and
/// how a local name becomes a feature identifier and a bounded location.
#[derive(Debug)]
pub(crate) struct Reporter {
    inner: LossReporter,
}

impl Reporter {
    /// A reporter for source whose unconsumed detail is retained as `retention`
    /// describes. There is no `Default`: what retains the source is a policy
    /// decision every construction site must state, and defaulting it is how it
    /// became a per-mode constant in the first place.
    pub(crate) fn new(retention: SourceRetention) -> Self {
        Self {
            inner: LossReporter::new(retention),
        }
    }

    /// Reports an element the model does not represent.
    ///
    /// Two classes are routed here rather than at each call site, because the
    /// `w:rsid` element appears in both `styles.xml` and `settings.xml` and
    /// reaches this sink through two different catch-alls: a per-site rule is a
    /// rule that the next catch-all forgets. Routing at the single choke point
    /// makes both classes total.
    ///
    /// - Revision-save-ID markup folds into its document-level class
    ///   ([`Reporter::report_rsid`]).
    /// - Markup that carries no document meaning at all raises nothing
    ///   (`noop::carries_no_meaning`, and `35-DISPOSITION-TAXONOMY.md`). An
    ///   *unrecognised* construct is not the same thing as a *lost* one, and
    ///   conflating them put 621 findings that describe no loss in front of the
    ///   owner's fifteen-document corpus (HF-174).
    pub(crate) fn report(&mut self, local: &[u8]) {
        if is_revision_save_id_element(local) {
            self.report_rsid();
            return;
        }
        if crate::noop::carries_no_meaning(local) {
            return;
        }
        self.inner
            .record(feature_of(local), element_location(local), Finding::Omitted);
    }

    /// Reports an element the model does not represent, consulting the element
    /// itself first.
    ///
    /// The difference from [`Reporter::report`] is the conditional half of the
    /// no-op class: `<a:effectLst/>` is "no effects" and `<a:effectLst>…</a:effectLst>`
    /// is a lost shadow; the name cannot tell them apart, and only the element
    /// can. `self_closing` is whether the source wrote `<x/>`.
    pub(crate) fn report_element(
        &mut self,
        local: &[u8],
        element: &quick_xml::events::BytesStart<'_>,
        self_closing: bool,
    ) {
        if crate::noop::carries_no_meaning_when(local, element, self_closing) {
            return;
        }
        self.report(local);
    }

    /// Reports an element that is structurally invalid or unusable — refused,
    /// not merely dropped.
    pub(crate) fn report_invalid(&mut self, local: &[u8]) {
        self.inner
            .record(feature_of(local), element_location(local), Finding::Invalid);
    }

    /// Reports an attribute of an otherwise-modeled element whose meaning the
    /// model does not carry: the element is `degraded`, and the location names
    /// which part of its meaning was lost (FID-R-03).
    pub(crate) fn report_attribute(&mut self, element: &[u8], attribute: &[u8]) {
        let feature = format!(
            "{}/@{}",
            String::from_utf8_lossy(element),
            String::from_utf8_lossy(attribute)
        );
        self.inner.record(
            feature,
            attribute_location(element, attribute),
            Finding::Degraded,
        );
    }

    /// Reports a construct that is not in the model but whose source is retained
    /// verbatim inside it and re-emitted on save, so the detail is recoverable
    /// even on the semantic path.
    pub(crate) fn report_retained_in_model(&mut self, local: &[u8], retained_bytes: usize) {
        self.inner.record(
            feature_of(local),
            element_location(local),
            Finding::RetainedInModel(retained_bytes),
        );
    }

    /// Reports one revision-save ID (`w:rsid*` attribute, or a `w:rsid`/`w:rsids`
    /// entry in `settings.xml`) as a member of **one document-level class**
    /// rather than as a finding of its own.
    ///
    /// Revision-save IDs are per-editing-session bookkeeping tokens: Word's
    /// compare/merge heuristics are their only consumer, and they have no effect
    /// on layout, rendering, text, or reopen fidelity. They are also the
    /// highest-volume attribute in WordprocessingML — a 14-document sample of
    /// real Word output carries up to 1,055 of them in one file, across seven
    /// names on `w:p`, `w:r`, `w:tr` and `w:sectPr`. Reporting each
    /// element-and-attribute pair would add roughly fourteen entries to every
    /// Word document's report, against the 28-30 entries such a document
    /// currently raises in total: a ~50% inflation carrying no actionable
    /// content, which is how a report becomes something callers filter out.
    ///
    /// So the class is one entry with a count. That is strictly *less* noisy than
    /// what preceded it (the `w:rsids` table in `settings.xml` used to raise two
    /// separate element findings) while covering the attributes, which could not
    /// be described at all. The loss is still named, and its disposition is still
    /// honest: `preserved` under the retention byte floor, `not-retained` on a
    /// semantic save. `35-DISPOSITION-TAXONOMY.md` records the decision.
    pub(crate) fn report_rsid(&mut self) {
        self.inner.record(
            RSID_CLASS_FEATURE.to_owned(),
            FeatureLocation::default(),
            Finding::Omitted,
        );
    }

    /// Reports watermark markup that was recognised as a watermark but could not
    /// become one — see [`WATERMARK_CLASS_FEATURE`] and
    /// [`crate::watermark::WatermarkLoss`] for the four ways that happens.
    ///
    /// `Degraded`, because the caller leaves the shape on the float layer when it
    /// calls this: the page still shows the object, it just is not a watermark.
    /// The `reason` becomes the location's "attribute", which is the only field
    /// the report carries that can say *which* way it failed while keeping one
    /// feature name and one occurrence count for the class.
    pub(crate) fn report_watermark(&mut self, reason: &str) {
        self.inner.record(
            WATERMARK_CLASS_FEATURE.to_owned(),
            FeatureLocation {
                part_name: None,
                element: Some("shape".to_owned()),
                attribute: Some(reason.to_owned()),
            },
            Finding::Degraded,
        );
    }

    /// Reports an appearance a shape's `wps:style` named in the theme's format
    /// scheme that this build cannot paint — one finding per style list, with
    /// `reason` naming which kind of entry it was.
    ///
    /// Raised per REFERENCE, not per theme entry. A theme is allowed to carry a
    /// pattern fill style or a shadow effect style that no shape ever names; the
    /// loss only exists once a shape asks for it. Reporting on the theme's contents
    /// would add a finding to most Word documents (the default Office theme's third
    /// effect style carries an `a:outerShdw`) and that is how a report becomes
    /// something callers filter out — the same argument [`Reporter::report_rsid`]
    /// records for revision-save ids.
    ///
    /// `Degraded`, matching [`Reporter::report_watermark`]: the shape is still
    /// placed and still painted, it just is not wearing the appearance the theme
    /// prescribed. The retention half of the disposition is conservative — the
    /// `a:fmtScheme` subtree IS retained verbatim and re-emitted on save, so
    /// nothing here is lost from the FILE, only from the render.
    pub(crate) fn report_theme_style_unpainted(&mut self, list: &str, reason: &str) {
        self.inner.record(
            format!("fmtScheme/{list}"),
            FeatureLocation {
                part_name: None,
                element: Some(list.to_owned()),
                attribute: Some(reason.to_owned()),
            },
            Finding::Degraded,
        );
    }

    /// Reports a shape fill or outline construct that this build imports and
    /// re-emits but cannot PAINT — one finding per construct, with `reason`
    /// naming which part of it is not drawn.
    ///
    /// `Degraded`, for the same reason [`Reporter::report_theme_style_unpainted`]
    /// is: the shape is still placed, still sized and still drawn, it just is not
    /// wearing the appearance the file asked for. A picture-filled shape paints
    /// unfilled, a `cmpd="dbl"` outline paints single, an `a:custDash` paints
    /// solid, and a `path="shape"` gradient paints concentric.
    ///
    /// The retention half of the disposition understates this case, and that is
    /// deliberate: the construct IS retained in `Definitions::shape_fill_detail`
    /// and semantic export re-emits it, so a save does not destroy it even on the
    /// regenerating path. What is lost is the render, which is why this reports at
    /// all rather than staying silent on the strength of the round trip.
    ///
    /// `reason` becomes the location's attribute, which is the only field the
    /// report carries that can say *which* part while keeping one feature name and
    /// one occurrence count — the shape [`Reporter::report_watermark`] uses.
    pub(crate) fn report_shape_appearance_unpainted(&mut self, construct: &str, reason: &str) {
        self.inner.record(
            format!("shape/{construct}"),
            FeatureLocation {
                part_name: None,
                element: Some(construct.to_owned()),
                attribute: Some(reason.to_owned()),
            },
            Finding::Degraded,
        );
    }

    /// Builds the report, resolving each finding's preservation claim against
    /// `ledger`.
    pub(crate) fn into_report(self, ledger: &mut PreservationLedger) -> CompatibilityReport {
        self.inner.into_report(ledger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The WordprocessingML vocabulary layer is what this module still owns, so
    /// it is what this module still tests: the rest moved to `casual-doc-loss`
    /// with the code, rather than being left behind as a second copy.
    ///
    /// The whole rsid class is one entry however many revision-save IDs the
    /// document carries, and its identifier is the documented class id.
    #[test]
    fn the_rsid_class_is_a_single_counted_entry() {
        let mut reporter = Reporter::new(SourceRetention::Regenerated);
        for _ in 0..1_000 {
            reporter.report_rsid();
        }
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].feature, RSID_CLASS_FEATURE);
        assert_eq!(report.entries[0].occurrences, 1_000);
    }

    /// Revision-save-ID *elements* route into the class at the choke point, not
    /// at each call site, so the two catch-alls that reach `report` cannot
    /// disagree about it.
    #[test]
    fn revision_save_id_elements_fold_into_the_class_through_report() {
        let mut reporter = Reporter::new(SourceRetention::Regenerated);
        for local in [b"rsid".as_slice(), b"rsids".as_slice(), b"rsidRoot"] {
            assert!(is_revision_save_id_element(local), "{local:?}");
            reporter.report(local);
        }
        assert!(is_revision_save_id_attribute(b"rsidRDefault"));
        assert!(!is_revision_save_id_attribute(b"paraId"));
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].feature, RSID_CLASS_FEATURE);
        assert_eq!(report.entries[0].occurrences, 3);
    }

    /// An element of the no-op class raises nothing: unrecognised is not lost.
    /// This is the half of the vocabulary that cannot move to a format-neutral
    /// crate, so it is asserted where it lives.
    #[test]
    fn a_no_op_element_raises_no_finding_at_all() {
        let mut reporter = Reporter::new(SourceRetention::Regenerated);
        reporter.report(b"proofErr");
        reporter.report(b"lastRenderedPageBreak");
        reporter.report(b"somethingRealAndUnmapped");
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        assert_eq!(
            report
                .entries
                .iter()
                .map(|entry| entry.feature.as_str())
                .collect::<Vec<_>>(),
            vec!["somethingRealAndUnmapped"]
        );
    }

    /// An attribute finding spells its feature `element/@attribute` and splits
    /// the two names across the location's two axes (FID-R-03).
    #[test]
    fn an_attribute_finding_names_both_the_element_and_the_attribute() {
        let mut reporter = Reporter::new(SourceRetention::Regenerated);
        reporter.report_attribute(b"theme", b"name");
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        let entry = &report.entries[0];
        assert_eq!(entry.feature, "theme/@name");
        assert_eq!(entry.location.element.as_deref(), Some("theme"));
        assert_eq!(entry.location.attribute.as_deref(), Some("name"));
        assert_eq!(entry.disposition, Disposition::DegradedNotRetained);
    }
}
