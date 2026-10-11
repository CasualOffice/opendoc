//! Snapshot recovery preserves uncertain work instead of applying stale coordinates.
//! Text edits replay only when their complete source paragraphs still match the
//! acknowledged base. Structural/formatting edits remain in the recovery archive.
use crate::{Intent, RevisionLog, Transaction, TransactionId};
use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{Mint, Operation};
use casual_doc_model::v1::Document;
use serde::{Deserialize, Serialize};

/// One retained local transaction, with its original identities and declarations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PendingEdit {
    /// Operations in original order.
    pub operations: Vec<Operation>,
    /// Original identity reservations.
    pub mints: Vec<Mint>,
    /// Original authoring declarations.
    pub intents: Vec<Intent>,
}

/// An atomic recovery result. The caller installs it only after all checks finish.
#[derive(Debug)]
pub struct Recovery {
    /// Snapshot plus safely replayed edits.
    pub document: Document,
    /// A new local log containing the replayed work, still awaiting acknowledgement.
    pub log: RevisionLog,
    /// Work that could not be replayed safely; never discarded.
    pub retained: Vec<PendingEdit>,
    /// Number of replayed transactions.
    pub replayed: usize,
}

/// Replays safe pending text edits onto a snapshot of the same document.
/// A changed paragraph, unsupported operation, protection refusal, or apply failure
/// retains the complete transaction. Errors leave both inputs untouched.
pub fn recover(
    original: &Document,
    log: &RevisionLog,
    snapshot: &Document,
    capabilities: Capabilities,
) -> Result<Recovery, String> {
    if original.id() != snapshot.id() {
        return Err("The recovery snapshot belongs to another document.".to_owned());
    }
    let commits: Vec<_> = log
        .commits()
        .filter(|c| c.revision() > log.horizon())
        .collect();
    let mut expected = acknowledged(original, log)?;
    let mut recovered = Recovery {
        document: snapshot.clone(),
        log: RevisionLog::default(),
        retained: Vec::new(),
        replayed: 0,
    };
    recovered.log.settle(recovered.log.head());
    for commit in commits {
        let edit = PendingEdit {
            operations: commit.operations().to_vec(),
            mints: commit.mints().to_vec(),
            intents: commit.intents().to_vec(),
        };
        let safe = edit.operations.iter().all(|op| {
            let node = match op {
                Operation::InsertText { at, .. } => at.node,
                Operation::DeleteText { range } if range.start.node == range.end.node => {
                    range.start.node
                }
                _ => return false,
            };
            expected.paragraph(node).is_some()
                && expected.paragraph(node) == recovered.document.paragraph(node)
        }) && casual_doc_edit::access::refuse_if_not_permitted(
            Some(&recovered.document),
            &edit.operations,
            capabilities,
        )
        .is_ok()
            && casual_doc_edit::protection::refuse_if_protected(
                &recovered.document,
                &edit.operations,
                capabilities,
            )
            .is_ok()
            && !casual_doc_edit::protection::forms(&recovered.document);
        let transaction = Transaction::new(
            TransactionId::new(0),
            recovered.log.head(),
            "Recovered local edit",
            edit.mints.clone(),
            edit.operations.clone(),
        )
        .with_intents(edit.intents.clone());
        if safe
            && recovered
                .log
                .apply(&mut recovered.document, transaction)
                .is_ok()
        {
            recovered.replayed += 1;
        } else {
            recovered.retained.push(edit);
        }
        // Each later transaction was authored after this one, even when it was retained.
        for (mint, operation) in commit.mints().iter().zip(commit.operations()) {
            casual_doc_edit::apply(&mut expected, *mint, operation)
                .map_err(|e| format!("{e:?}"))?;
        }
    }
    Ok(recovered)
}

/// Produces the acknowledged base without changing the optimistic local document.
pub fn acknowledged(original: &Document, log: &RevisionLog) -> Result<Document, String> {
    let commits: Vec<_> = log
        .commits()
        .filter(|c| c.revision() > log.horizon())
        .collect();
    let mut base = original.clone();
    for commit in commits.into_iter().rev() {
        if commit.inverse_operations().len() != commit.operations().len()
            || commit.mints().len() != commit.operations().len()
        {
            return Err(
                "Pending work cannot be rolled back; the local document is preserved.".to_owned(),
            );
        }
        for (index, operation) in commit.inverse_operations().iter().enumerate() {
            let mint = commit.mints()[commit.mints().len() - 1 - index].inverse();
            casual_doc_edit::apply(&mut base, mint, operation).map_err(|e| format!("{e:?}"))?;
        }
    }
    Ok(base)
}
