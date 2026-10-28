// SPDX-License-Identifier: Apache-2.0

//! The live editor's entry point: compile a plan, commit nothing (`docs/181`
//! §4.4, the private preview mode).
//!
//! The in-page assistant (`docs/178` §9) proposes a plan to the editor the
//! person is looking at. The person sees what it would do, privately, and
//! decides. So the editor needs two things from this crate that a headless
//! session does not: the plan's **operations**, to commit through the editor's
//! own choke point on Accept, and the plan's **redline**, to show before then.
//!
//! [`compile`] is [`crate::plan::preview`]'s run with the operations kept:
//! the same anchor resolution, registry lookup, lowerings, declared-emits
//! check and engine capability check, on the same copy-on-write clone. There
//! is no second lowering; the editor never sees an edit, only operations this
//! crate's one compiler produced. The clone is thrown away; the editor applies
//! the operations to its own document, as one transaction, through its own
//! `apply_group`, which re-runs the engine's access and protection checks
//! against the editor's grant.
//!
//! **Why the operations are the ones the clone committed.** Each edit lowers
//! against the document as the edits before it left it (a later edit's anchor
//! may sit in an earlier edit's new paragraph), so the operations are
//! collected from the clone's log in commit order rather than lowered against
//! the starting document. Applying them in that order to the starting
//! document reproduces the clone's content.
//!
//! **Ids.** The session is built with clones of the editor's allocators
//! ([`Session::for_editor`]), so the ids a lowering mints are the ones the
//! editor would mint next. [`Compiled::ids`] and [`Compiled::revisions`] are
//! the allocators after the run; the editor moves its own past them before it
//! commits, and nothing the editor mints afterwards can equal a plan's id.

use casual_doc_edit::Operation;
use casual_doc_model::IdGenerator;
use casual_doc_review::body::RevisionIdAllocator;
use serde_json::Value;

use crate::error::AiResult;
use crate::plan::{self, EditPlan};
use crate::session::Session;

/// A compiled plan: what it would do, and the operations that do it.
#[derive(Debug)]
pub struct Compiled {
    /// Exactly what [`crate::plan::preview`] returns for the same plan: the
    /// per-edit results and refusals, the redline of every affected block, the
    /// mode and the author.
    pub preview: Value,
    /// Whether every edit compiled. When `false` the operations are empty and
    /// the preview's refusals say why; a plan applies all or nothing.
    pub would_apply: bool,
    /// The operations, in commit order, for the editor's choke point.
    pub operations: Vec<Operation>,
    /// The id allocator after the run. Its counter is past every id the plan
    /// minted.
    pub ids: IdGenerator,
    /// The revision-number allocator after the run.
    pub revisions: RevisionIdAllocator,
    /// Picture bytes the plan wrote (`insert_image`), by package part name.
    /// The editor adds them to its resources in the same step it commits.
    pub media: Vec<(String, Vec<u8>)>,
}

/// Compiles `plan` against `session` without committing anything: the
/// session is unchanged, and so is anything the caller built it from.
///
/// **O(document)** for the clone plus O(affected blocks), as
/// [`crate::plan::preview`].
///
/// # Errors
///
/// As [`crate::plan::preview`]: `stale-revision`, `unknown-edit`,
/// `invalid-argument`, `over-limit` for the plan as a whole. Per-edit
/// refusals are in [`Compiled::preview`] and make [`Compiled::would_apply`]
/// false.
pub fn compile(session: &Session, plan: &EditPlan) -> AiResult<Compiled> {
    let (state, outcomes) = plan::run(session, plan)?;
    let preview = plan::preview_value(session, plan, &state, &outcomes);
    let would_apply = outcomes.iter().all(|outcome| outcome.error.is_none());
    let start = session.state.log.head();
    let operations = if would_apply {
        state
            .log
            .commits()
            .filter(|commit| commit.revision() > start)
            .flat_map(|commit| commit.operations().iter().cloned())
            .collect()
    } else {
        Vec::new()
    };
    Ok(Compiled {
        preview,
        would_apply,
        operations,
        ids: state.ids,
        revisions: state.revisions,
        media: if would_apply { state.media } else { Vec::new() },
    })
}
