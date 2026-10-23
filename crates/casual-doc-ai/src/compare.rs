// SPDX-License-Identifier: Apache-2.0

//! Structured comparison of two open documents, run in its budgeted,
//! cancellable form and refused past the wall-clock budget rather than left
//! running (`docs/178` §5.7).

use std::time::Instant;

use casual_doc_diff::{DiffJob, DiffSides, Progress};
use serde_json::{Value, json};

use crate::error::{AiError, AiResult};
use crate::read::{cursor, parse_cursor};
use crate::session::Session;

/// Blocks projected per step between budget checks.
const STEP: usize = 2_000;

/// Compares `left` (older) with `right` (newer). Changes are paged.
/// **O(both documents)**, bounded by the session's compare budget.
///
/// # Errors
///
/// `over-budget` past the wall-clock budget; `invalid-argument` for a bad
/// cursor.
pub fn compare(
    left: &Session,
    right: &Session,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    let limits = &left.config().limits;
    let started = Instant::now();
    let mut job = DiffJob::new();
    let sides = DiffSides {
        left: left.document(),
        right: right.document(),
        left_digests: None,
        right_digests: None,
    };
    loop {
        match job.step(sides, STEP) {
            Progress::Complete => break,
            Progress::Cancelled => {
                return Err(AiError::new("cancelled", "The comparison was cancelled."));
            }
            Progress::Working { .. } => {
                if started.elapsed().as_millis() > u128::from(limits.compare_budget_ms) {
                    job.cancel();
                    return Err(AiError::new(
                        "over-budget",
                        "The comparison ran past this server's time budget and was stopped.",
                    ));
                }
            }
        }
    }
    let diff = job
        .take()
        .ok_or_else(|| AiError::new("compare-failed", "The comparison produced no result."))?;
    // Bound the paged list to both revisions, so an edit to either invalidates it.
    let revision = left
        .revision()
        .wrapping_mul(1_000_003)
        .wrapping_add(right.revision());
    let start = parse_cursor("diff", revision, cursor_in)?;
    let limit = limits.page(limit, 50);
    let total = diff.changes.len();
    let changes: Vec<Value> = diff
        .changes
        .iter()
        .skip(start)
        .take(limit)
        .map(|change| serde_json::to_value(change).unwrap_or(Value::Null))
        .collect();
    let end = start + changes.len();
    Ok(json!({
        "complete": diff.complete,
        "total": total,
        "kindCounts": serde_json::to_value(&diff.kind_counts).unwrap_or(Value::Null),
        "findings": serde_json::to_value(&diff.findings).unwrap_or(Value::Null),
        "changes": changes,
        "nextCursor": (end < total).then(|| cursor("diff", revision, end)),
    }))
}
