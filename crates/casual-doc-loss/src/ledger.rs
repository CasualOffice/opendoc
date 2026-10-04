// SPDX-License-Identifier: Apache-2.0

//! The preservation ledger: the index of what retains the source detail a model
//! did not consume, and the only thing that licenses a `preserved` claim.

use crate::taxonomy::RetentionOutcome;

/// Identifier of a [`PreservationLedger`] record, referenced by every report
/// entry whose retention outcome is `preserved`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LedgerId(u32);

impl LedgerId {
    /// The record's stable, deterministic index within its ledger.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// A reference to the record at `index`, for a caller reconstructing a
    /// report it did not build (a deserializer, or a test exercising a dangling
    /// reference).
    ///
    /// Minting an id this way does **not** make it valid:
    /// `CompatibilityReport::validate` resolves every reference against the
    /// ledger and refuses one that does not land on a record retaining
    /// something. That is the point — a preservation claim is audited, not
    /// trusted.
    #[must_use]
    pub const fn from_index(index: u32) -> Self {
        Self(index)
    }
}

/// Where a ledger record's retained bytes live.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreservationKind {
    /// The verbatim source snapshot — a retention mode's tier-1 byte floor,
    /// which reproduces the import input exactly.
    SourceSnapshot,
    /// An admitted package part carried verbatim through the semantic writer via
    /// an opaque side-table.
    OpaquePart,
    /// A source subtree retained inside the model (not as a package part) and
    /// re-emitted verbatim on save — for example an OMML equation with no typed
    /// projection.
    ModelSubtree,
}

/// One validated record that source detail the model did not consume is retained
/// and recoverable.
///
/// `35` permits a `preserved` retention outcome **only** when the report
/// references such a record: "emitting a warning without retaining the declared
/// content is `not-retained`, never `preserved`". A record retaining zero bytes
/// retains nothing and is refused by `CompatibilityReport::validate`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerRecord {
    /// Stable, deterministic identifier.
    pub id: LedgerId,
    /// Where the retained bytes live.
    pub kind: PreservationKind,
    /// The package part the record covers, or the construct name for a
    /// [`PreservationKind::ModelSubtree`]. `None` for the whole-source snapshot.
    pub covers: Option<String>,
    /// Retained byte count.
    pub retained_bytes: usize,
}

impl LedgerRecord {
    /// Whether this record retains anything at all.
    ///
    /// For a snapshot or an in-model subtree the retained bytes *are* the
    /// artifact, so zero bytes means nothing was retained and the record cannot
    /// license a `preserved` claim — `35` calls that `not-retained`. An
    /// [`PreservationKind::OpaquePart`] record is different: the artifact is the
    /// package part, which the semantic writer re-emits with its name and
    /// declared content type, and a genuinely zero-length part is still
    /// reproduced exactly.
    #[must_use]
    pub const fn retains_something(&self) -> bool {
        self.retained_bytes > 0 || matches!(self.kind, PreservationKind::OpaquePart)
    }
}

/// The preservation ledger: every record that licenses a `preserved` retention
/// outcome in the companion `CompatibilityReport`.
///
/// Built during import and returned alongside the report, so a caller can audit
/// a `preserved` claim instead of trusting it. The ledger is *not* the retained
/// data — it is the index of it (doc-45 invariant I4 keeps opaque bytes out of
/// the semantic model); the bytes live in the adapter's own retention state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PreservationLedger {
    records: Vec<LedgerRecord>,
    snapshot: Option<LedgerId>,
}

impl PreservationLedger {
    /// A ledger whose source snapshot retains `retained_bytes` of import input
    /// verbatim.
    ///
    /// This is the record a whole-source byte floor cites, and the only one an
    /// adapter can mint without knowing anything about the format it read: if
    /// the original bytes are kept and reproduced exactly, every unconsumed
    /// remainder in them is recoverable. An adapter that retains the source and
    /// does **not** build this record is reporting `not-retained` for detail it
    /// actually kept, which understates its own fidelity.
    #[must_use]
    pub fn with_source_snapshot(retained_bytes: usize) -> Self {
        let mut ledger = Self::default();
        let id = ledger.push(PreservationKind::SourceSnapshot, None, retained_bytes);
        ledger.snapshot = Some(id);
        ledger
    }

    /// The source-snapshot record, when the import retains one.
    #[must_use]
    pub const fn source_snapshot(&self) -> Option<LedgerId> {
        self.snapshot
    }

    /// Every record, in creation order (which is also [`LedgerId`] order).
    #[must_use]
    pub fn records(&self) -> &[LedgerRecord] {
        &self.records
    }

    /// The record `id` names, if it is in this ledger.
    #[must_use]
    pub fn get(&self, id: LedgerId) -> Option<&LedgerRecord> {
        self.records
            .get(usize::try_from(id.0).unwrap_or(usize::MAX))
    }

    /// Restates the source-snapshot record's extent once the byte floor's true
    /// size is known.
    ///
    /// The record is typically created during a main-part pass, which can only
    /// account for that part. A package path then retains **every** admitted
    /// part — the main one among them — so the figure is replaced rather than
    /// added to; adding would count the main part twice and make the one number a
    /// caller uses to audit the claim wrong.
    pub fn restate_source_snapshot(&mut self, retained_bytes: usize) {
        if let Some(id) = self.snapshot
            && let Ok(index) = usize::try_from(id.0)
            && let Some(record) = self.records.get_mut(index)
        {
            record.retained_bytes = retained_bytes;
        }
    }

    /// Records an admitted part carried verbatim through the semantic writer.
    pub fn record_opaque_part(&mut self, part_name: &str, retained_bytes: usize) -> LedgerId {
        self.push(
            PreservationKind::OpaquePart,
            Some(part_name.to_owned()),
            retained_bytes,
        )
    }

    /// The opaque-part record covering `part_name`, when one was created.
    ///
    /// A lookup rather than a returned id because the two halves are often
    /// decided in different passes: the record is minted while the side-table is
    /// built, and the findings that cite it are built from a reader's outcomes. A
    /// finding with no record fails `CompatibilityReport::validate`, which is the
    /// intended outcome if these two ever disagree — the claim is audited, not
    /// trusted (`35`).
    ///
    /// Complexity: O(records). Intended to be called once per part, never in a
    /// loop over constructs.
    #[must_use]
    pub fn opaque_part_record(&self, part_name: &str) -> Option<LedgerId> {
        self.records
            .iter()
            .find(|record| {
                record.kind == PreservationKind::OpaquePart
                    && record.covers.as_deref() == Some(part_name)
            })
            .map(|record| record.id)
    }

    /// Records a source subtree retained inside the model and re-emitted on save.
    pub fn record_model_subtree(&mut self, covers: &str, retained_bytes: usize) -> LedgerId {
        self.push(
            PreservationKind::ModelSubtree,
            Some(covers.to_owned()),
            retained_bytes,
        )
    }

    /// The record that licenses `outcome` for a whole-source finding, when the
    /// snapshot is what retains it.
    ///
    /// A convenience for the adapter shape where the byte floor is the only
    /// preservation mechanism: it keeps the "cite the snapshot exactly when the
    /// disposition claims preservation" rule in one place instead of once per
    /// adapter, which is how an unaudited `preserved` gets written.
    #[must_use]
    pub const fn snapshot_license(&self, outcome: RetentionOutcome) -> Option<LedgerId> {
        match outcome {
            RetentionOutcome::Preserved => self.snapshot,
            _ => None,
        }
    }

    fn push(
        &mut self,
        kind: PreservationKind,
        covers: Option<String>,
        retained_bytes: usize,
    ) -> LedgerId {
        let id = LedgerId(u32::try_from(self.records.len()).unwrap_or(u32::MAX));
        self.records.push(LedgerRecord {
            id,
            kind,
            covers,
            retained_bytes,
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zero-byte snapshot retains nothing, and an opaque part of zero bytes
    /// still retains the part itself. The distinction is what `validate` keys on.
    #[test]
    fn only_an_opaque_part_can_retain_something_while_holding_no_bytes() {
        let ledger = PreservationLedger::with_source_snapshot(0);
        let snapshot = ledger.source_snapshot().expect("a snapshot record");
        assert!(!ledger.get(snapshot).expect("in ledger").retains_something());

        let mut ledger = PreservationLedger::default();
        let empty_part = ledger.record_opaque_part("customXml/item1.xml", 0);
        assert!(
            ledger
                .get(empty_part)
                .expect("in ledger")
                .retains_something()
        );
        let empty_subtree = ledger.record_model_subtree("oMath", 0);
        assert!(
            !ledger
                .get(empty_subtree)
                .expect("in ledger")
                .retains_something()
        );
    }

    /// Ids are the records' own indices, so a report's references stay stable
    /// and a lookup is O(1).
    #[test]
    fn ledger_ids_are_dense_indices_in_creation_order() {
        let mut ledger = PreservationLedger::with_source_snapshot(16);
        let part = ledger.record_opaque_part("word/charts/chart1.xml", 32);
        let subtree = ledger.record_model_subtree("oMath", 64);
        assert_eq!(
            [
                ledger.source_snapshot().expect("snapshot").get(),
                part.get(),
                subtree.get()
            ],
            [0, 1, 2]
        );
        assert!(ledger.get(LedgerId::from_index(3)).is_none());
        assert_eq!(
            ledger.opaque_part_record("word/charts/chart1.xml"),
            Some(part)
        );
        assert_eq!(ledger.opaque_part_record("word/charts/chart2.xml"), None);
    }

    /// Restating replaces the snapshot's extent rather than adding to it: the
    /// number a caller audits the claim with must not count a part twice.
    #[test]
    fn restating_the_snapshot_replaces_its_extent() {
        let mut ledger = PreservationLedger::with_source_snapshot(100);
        ledger.restate_source_snapshot(250);
        let snapshot = ledger.source_snapshot().expect("a snapshot record");
        assert_eq!(ledger.get(snapshot).expect("in ledger").retained_bytes, 250);
    }

    /// The snapshot licenses a `preserved` outcome and nothing else, so the rule
    /// `validate` enforces cannot be got wrong one adapter at a time.
    #[test]
    fn the_snapshot_licenses_preserved_and_only_preserved() {
        let ledger = PreservationLedger::with_source_snapshot(16);
        assert_eq!(
            ledger.snapshot_license(RetentionOutcome::Preserved),
            ledger.source_snapshot()
        );
        for outcome in [
            RetentionOutcome::NotRetained,
            RetentionOutcome::Blocked,
            RetentionOutcome::Rejected,
            RetentionOutcome::NotApplicable,
        ] {
            assert_eq!(ledger.snapshot_license(outcome), None, "{outcome:?}");
        }
    }
}
