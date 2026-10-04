// SPDX-License-Identifier: Apache-2.0

//! The PresentationML feature vocabulary layered over the shared loss taxonomy.
//!
//! # The shared taxonomy fits, and this is the whole adapter half of it
//!
//! `casual-doc-loss` is format-neutral by construction — nothing in its public
//! API names a markup vocabulary — and it asks the adapter for exactly three
//! things: a stable feature identifier, a bounded [`FeatureLocation`], and which
//! [`Finding`] applies. All three are PresentationML facts, so all three are
//! here, and the aggregation, the per-feature ceiling, the resolution into a
//! disposition and the ledger bookkeeping are the shared crate's.
//!
//! This is a thin layer by design. It exists because the *convention* for turning
//! a local name into a feature identifier has to be one convention, and because
//! `part_name` must be populated from the part actually being read rather than
//! from a conventional name the package need not use.
//!
//! # Why every finding here is `Regenerated`, not `Snapshot`
//!
//! [`SourceRetention::Snapshot`] licenses a `preserved` claim from a verbatim
//! byte floor that reproduces the import input exactly. There is no presentation
//! *writer* in this repository yet, so there is no byte floor and nothing retains
//! the source. Stamping `Snapshot` would make every entry read `preserved`
//! against a snapshot that does not exist — the report would claim the loss is
//! recoverable when it is not. `Regenerated` is the honest reading, and it is why
//! this importer's report is mostly `not-retained`: that is a true statement
//! about today's pipeline, not a pessimistic one.

use casual_doc_loss::{
    CompatibilityReport, FeatureLocation, Finding, LossReporter, PreservationLedger,
    SourceRetention,
};

use crate::ImportError;

/// Aggregating sink for one import, carrying the part each finding is charged to.
#[derive(Debug)]
pub(crate) struct Reporter {
    inner: LossReporter,
    ledger: PreservationLedger,
}

impl Reporter {
    /// A reporter for a semantic import with no verbatim byte floor.
    pub(crate) fn new() -> Self {
        Self {
            inner: LossReporter::new(SourceRetention::Regenerated),
            ledger: PreservationLedger::default(),
        }
    }

    /// Reports an element whose meaning the model does not represent at all.
    ///
    /// `part` is the normalized part name the element was read from, which is
    /// genuinely known here and so is recorded: a `a:gradFill` lost on slide 7 is
    /// a different fidelity fact to act on than one lost in the master.
    pub(crate) fn omitted(&mut self, part: &str, element: &[u8]) {
        let feature = name(element);
        self.inner.record(
            feature.clone(),
            location(part, &feature, None),
            Finding::Omitted,
        );
    }

    /// Reports an element that is structurally invalid or unusable — refused
    /// rather than merely dropped, because there is nothing to map and nothing to
    /// attach it to.
    pub(crate) fn invalid(&mut self, part: &str, element: &[u8]) {
        let feature = name(element);
        self.inner.record(
            feature.clone(),
            location(part, &feature, None),
            Finding::Invalid,
        );
    }

    /// Reports an attribute of an otherwise-modelled element whose meaning is not
    /// carried: the element is `degraded` and the location names which part of its
    /// meaning was lost.
    pub(crate) fn degraded_attribute(&mut self, part: &str, element: &[u8], attribute: &[u8]) {
        let element = name(element);
        let attribute = name(attribute);
        let feature = format!("{element}/@{attribute}");
        self.inner.record(
            feature,
            location(part, &element, Some(&attribute)),
            Finding::Degraded,
        );
    }

    /// Reports a construct that is modelled, but whose representation drops some
    /// meaning the element itself carries — distinct from an attribute loss
    /// because the lost meaning is the element's own.
    pub(crate) fn degraded(&mut self, part: &str, element: &[u8]) {
        let feature = name(element);
        self.inner.record(
            feature.clone(),
            location(part, &feature, None),
            Finding::Degraded,
        );
    }

    /// Reports a whole admitted package part the semantic model does not consume.
    ///
    /// The feature identifier is the part name, which is what
    /// `casual-doc-loss`'s own documentation prescribes for a whole-part
    /// disposition. Enumerating these is the difference between a report that
    /// lists what was lost and one that lists what was *noticed* — `SKILL` §9.3:
    /// absence from a support matrix is an overstatement by omission.
    pub(crate) fn unconsumed_part(&mut self, part: &str) {
        self.inner.record(
            part.to_owned(),
            FeatureLocation::for_part(part),
            Finding::Omitted,
        );
    }

    /// Builds the report and validates its preservation claims.
    ///
    /// A violation fails the import rather than appearing in the report, because
    /// `35-DISPOSITION-TAXONOMY.md` says a pairing it does not admit "is an
    /// internal error and must fail import, not be reported".
    pub(crate) fn finish(self) -> Result<(CompatibilityReport, PreservationLedger), ImportError> {
        let mut ledger = self.ledger;
        let report = self.inner.into_report(&mut ledger);
        report
            .validate(&ledger)
            .map_err(|violation| ImportError::IllegalDisposition {
                violation: violation.to_string(),
            })?;
        Ok((report, ledger))
    }
}

/// A local name as a feature identifier.
///
/// Unprefixed, matching `casual-doc-import`'s convention and
/// `casual-doc-loss`'s reasoning: the prefix a producer binds is arbitrary, so a
/// prefixed identifier would be a property of the writer rather than of the
/// construct. Non-UTF-8 is replaced rather than refused — an identifier is a
/// diagnostic, and failing an import over the spelling of a name we are already
/// reporting as lost would be the wrong trade.
fn name(local: &[u8]) -> String {
    String::from_utf8_lossy(local).into_owned()
}

fn location(part: &str, element: &str, attribute: Option<&str>) -> FeatureLocation {
    FeatureLocation {
        part_name: Some(part.to_owned()),
        element: Some(element.to_owned()),
        attribute: attribute.map(str::to_owned),
    }
}
