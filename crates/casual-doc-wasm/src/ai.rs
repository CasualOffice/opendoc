// SPDX-License-Identifier: Apache-2.0

//! `editor.ai`: the in-page assistant's door into the live editor (`docs/178`
//! §9, `docs/181` §4.4).
//!
//! A host's own assistant reads the open document by anchor and proposes an
//! edit plan. The person who asked sees what the plan would do, **privately**,
//! and decides; nothing reaches the document, the undo stack or a shared room
//! until they accept. That is the Gemini-in-Docs and Tiptap "private until
//! accepted" model, beside the durable tracked suggestions an MCP agent makes.
//!
//! # One compiler, one mutation path
//!
//! Nothing here lowers an edit. A plan is compiled by `casual-doc-ai`
//! ([`casual_doc_ai::editor::compile`]): the same anchors, registry,
//! lowerings and engine capability check the MCP server runs, on a
//! copy-on-write clone of this document. What comes back is the redline and
//! the operations. The operations wait here, keyed by a preview id, and the
//! person's Accept commits **exactly those** through the editor's one choke
//! point, `apply_group`, as one transaction and one undo step, where the
//! engine's access and protection checks run again against this replica's
//! grant. Discard drops them. JavaScript only ever holds the id, so a page
//! cannot hand the editor operations of its own through this door.
//!
//! # Who may do what (`docs/178` §9's table)
//!
//! The host's grant is resolved to an access level in `webapp/src/ai_assistant.mjs`
//! (from capabilities, never from a role name) and arrives here as one of
//! `none`, `read`, `comment`, `suggest` or `review`, plus whether the host
//! granted direct edits. It becomes the `casual-doc-ai` profile the compile
//! runs under, so the ENGINE refuses what the profile does not admit:
//!
//! | access | profile | direct |
//! | --- | --- | --- |
//! | `none` | nothing, reads included | — |
//! | `read` | `read` | never |
//! | `comment` | `comment` | never |
//! | `suggest` | `suggest` | `edit` with the host's grant |
//! | `review` | `review` for a plan that decides revisions, else `suggest` | `review` with the grant |
//!
//! # Ids
//!
//! The compile runs on clones of this editor's own allocators, so a plan mints
//! the ids the editor would have minted next; Accept moves the allocators past
//! them before it commits. A plan's ids therefore never equal an id the editor
//! mints afterwards (`every_id_an_accepted_plan_minted_stays_unique` below).
//!
//! # The author
//!
//! Suggestions and comments are authored by the host identity with an AI
//! label, "Ada Lovelace (AI)" (`docs/178` §9, Q3 pending the owner; recorded
//! in `docs/181` §12, Lane preview). A plan cannot name its author. In a
//! shared room the relay's attested name wins at commit, as for every edit
//! (`docs/175` §5.2), so the label does not survive there until `178` §10's
//! agent principal exists.

use std::cell::{Cell, RefCell};

use casual_doc_ai::plan::{EditPlan, Mode};
use casual_doc_ai::{AiError, Clock, Limits, Profile, Session, SessionConfig, read};
use casual_doc_edit::Operation;
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_review::body::RevisionIdAllocator;
use casual_doc_transaction::{Coalesce, Origin};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, damage_of, to_js};

/// The label suggestions carry when the host has said who the person is.
const AI_LABEL: &str = " (AI)";
/// The author when the host has not: the `casual-doc-ai` default.
const ANONYMOUS_AUTHOR: &str = "AI assistant";

/// A compiled plan waiting for the person's decision.
struct Pending {
    id: u32,
    /// Which editor it was compiled for: the handle's address, stable for its
    /// life (wasm-bindgen boxes it). Another editor on the page cannot accept it.
    owner: usize,
    document: NodeId,
    /// The document revision it was compiled against; Accept refuses anything
    /// else, because the operations address that document.
    head: u64,
    operations: Vec<Operation>,
    ids: IdGenerator,
    revisions: RevisionIdAllocator,
    media: Vec<(String, Vec<u8>)>,
}

thread_local! {
    /// At most one private preview at a time, as Docs and Word show one
    /// proposal card: a new preview replaces the one before it.
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
    static NEXT_PREVIEW: Cell<u32> = const { Cell::new(0) };
}

/// The JSON a binding returns: `{ok: true, value}` or `{ok: false, error}`.
/// Values rather than thrown errors, so a refusal is something a host
/// branches on and a native test can read.
fn answer(result: Result<Value, AiError>) -> String {
    match result {
        Ok(value) => json!({ "ok": true, "value": value }),
        Err(error) => json!({ "ok": false, "error": error }),
    }
    .to_string()
}

fn withheld() -> AiError {
    AiError::new(
        "capability-withheld",
        "This editor was opened without access to its content, so the assistant can neither \
         read nor change it.",
    )
    .with_details(json!({ "enforcedBy": "editor" }))
}

/// The profile a plan compiles under, from the host's access level. See the
/// module's table. O(edits).
fn profile_for(access: &str, direct: bool, plan: Option<&EditPlan>) -> Result<Profile, AiError> {
    let decides = plan.is_some_and(|plan| {
        plan.edits
            .iter()
            .any(|edit| edit.get("op").and_then(Value::as_str) == Some("review"))
    });
    let wants_direct = plan.is_some_and(|plan| plan.mode == Mode::Direct);
    Ok(match access {
        "read" => Profile::Read,
        "comment" => Profile::Comment,
        "suggest" if direct => Profile::Edit,
        "suggest" => Profile::Suggest,
        // `review` without the direct grant: a direct plan compiles as a
        // suggester's, so the compile refuses it with the profile's reason.
        "review" if direct || (decides && !wants_direct) => Profile::Review,
        "review" => Profile::Suggest,
        "none" => return Err(withheld()),
        other => {
            return Err(AiError::invalid(format!(
                "Unknown assistant access `{}`.",
                casual_doc_ai::text::clip(other, 40)
            )));
        }
    })
}

impl WasmDocument {
    /// The author an assistant's suggestions carry: the host identity with the
    /// AI label, or the anonymous assistant. Never from the plan.
    fn ai_author(&self) -> String {
        self.resolve_author(None).map_or_else(
            || ANONYMOUS_AUTHOR.to_owned(),
            |name| format!("{name}{AI_LABEL}"),
        )
    }

    /// A `casual-doc-ai` session over this editor's document and allocators.
    /// **O(document)**: one clone, which a compile needs anyway.
    fn ai_session(&self, profile: Profile, date: &str) -> Session {
        let clock = if date.is_empty() {
            // `SystemTime` has no clock on wasm32; the host always passes one,
            // and a native caller that does not gets a fixed, honest epoch.
            Clock::Fixed("1970-01-01T00:00:00Z".to_owned())
        } else {
            Clock::Fixed(date.to_owned())
        };
        Session::for_editor(
            self.document.clone(),
            self.edit_ids.clone(),
            self.revision_ids.clone(),
            self.log.head().get(),
            SessionConfig {
                author: self.ai_author(),
                profile,
                clock,
                limits: Limits::default(),
            },
        )
    }

    fn ai_read_value(&self, request: &str, access: &str) -> Result<Value, AiError> {
        profile_for(access, false, None)?;
        let request: Value = serde_json::from_str(request)
            .map_err(|error| AiError::invalid(format!("The request is not JSON: {error}.")))?;
        let session = self.ai_session(Profile::Read, "");
        let text = |key: &str| request.get(key).and_then(Value::as_str);
        let number = |key: &str| request.get(key).and_then(Value::as_u64);
        let limit = number("limit").and_then(|value| usize::try_from(value).ok());
        match text("tool").unwrap_or("") {
            "outline" => read::outline(
                &session,
                number("depth").and_then(|value| u8::try_from(value).ok()),
                text("cursor"),
                limit,
            ),
            "read" => read::read_blocks(
                &session,
                text("start"),
                text("cursor"),
                number("maxTokens").and_then(|value| u32::try_from(value).ok()),
                if text("format") == Some("markdown") {
                    read::ReadFormat::Markdown
                } else {
                    read::ReadFormat::Json
                },
            ),
            "search" => read::search(
                &session,
                text("query").unwrap_or(""),
                match text("mode") {
                    Some("case-insensitive") => read::SearchMode::Insensitive,
                    Some("whole-word") => read::SearchMode::Words,
                    _ => read::SearchMode::Exact,
                },
                text("cursor"),
                limit,
            ),
            "trackedChanges" => read::list_tracked_changes(&session, text("cursor"), limit),
            "comments" => read::list_comments(&session, text("cursor"), limit),
            other => Err(AiError::invalid(format!(
                "Unknown read `{}`; the reads are outline, read, search, trackedChanges and \
                 comments.",
                casual_doc_ai::text::clip(other, 40)
            ))),
        }
    }

    /// Compiles `plan` and keeps its operations under a fresh preview id.
    /// Changes nothing the document, the undo stack or a room can see.
    fn ai_preview_value(
        &self,
        plan: &str,
        access: &str,
        direct: bool,
        date: &str,
    ) -> Result<Value, AiError> {
        let plan: EditPlan = serde_json::from_str(plan)
            .map_err(|error| AiError::invalid(format!("The plan is malformed: {error}.")))?;
        let profile = profile_for(access, direct, Some(&plan))?;
        let session = self.ai_session(profile, date);
        let compiled = casual_doc_ai::editor::compile(&session, &plan)?;
        let mut preview = compiled.preview;
        // The compile judged the plan against the assistant's profile. This
        // replica has its own grant (a room's word, a host ceiling) and the
        // document its own protection; ask the same question the choke point
        // will, now, so the person is not offered an Accept that will fail.
        let editor_refusal = if !compiled.would_apply {
            None
        } else if self.layout.is_windowed() {
            Some(AiError::new(
                "engine-refused",
                "This document is too large to edit here, so the assistant's changes cannot be \
                 applied.",
            ))
        } else {
            self.refuse_if_protected(&compiled.operations)
                .err()
                .map(AiError::engine)
        };
        let previewable = compiled.would_apply && editor_refusal.is_none();
        let id = if previewable {
            let id = NEXT_PREVIEW.with(|next| {
                let id = next.get().wrapping_add(1).max(1);
                next.set(id);
                id
            });
            PENDING.with(|pending| {
                *pending.borrow_mut() = Some(Pending {
                    id,
                    owner: core::ptr::from_ref(self) as usize,
                    document: self.document.id(),
                    head: self.log.head().get(),
                    operations: compiled.operations,
                    ids: compiled.ids,
                    revisions: compiled.revisions,
                    media: compiled.media,
                });
            });
            Some(id)
        } else {
            None
        };
        if let Some(object) = preview.as_object_mut() {
            object.insert("previewId".to_owned(), json!(id));
            object.insert("wouldApply".to_owned(), json!(previewable));
            object.insert("editorRefusal".to_owned(), json!(editor_refusal));
        }
        Ok(preview)
    }

    /// Commits the operations previewed under `id` through `apply_group`.
    fn ai_accept_inner(&mut self, id: u32) -> Result<EditResult, String> {
        let owner = core::ptr::from_ref(self) as usize;
        let pending = PENDING.with(|pending| {
            let mut slot = pending.borrow_mut();
            if slot
                .as_ref()
                .is_some_and(|held| held.id == id && held.owner == owner)
            {
                slot.take()
            } else {
                None
            }
        });
        let Some(pending) = pending else {
            return Err(
                "refused: That assistant suggestion is no longer open.\u{1f}ai.preview-gone"
                    .to_owned(),
            );
        };
        if pending.document != self.document.id()
            || pending.head != self.log.head().get()
            || pending.ids.namespace() != self.edit_ids.namespace()
        {
            return Err(
                "refused: The document changed after the assistant's suggestion was prepared. \
                 Ask for it again.\u{1f}ai.preview-stale"
                    .to_owned(),
            );
        }
        // Past every id the plan minted, so nothing the editor mints next can
        // equal one of them. Monotonic: neither allocator ever moves back.
        self.edit_ids
            .reserve_through(pending.ids.next_counter().saturating_sub(1));
        self.revision_ids.used.extend(pending.revisions.used);
        self.revision_ids.next = self.revision_ids.next.max(pending.revisions.next);
        let caret = self.apply_group(
            &pending.operations,
            HistoryKind::Edit.label(),
            Coalesce::New,
            Origin::Edit,
        )?;
        // The pictures the plan wrote join the package only once the model
        // holds their references.
        for (part, bytes) in pending.media {
            self.resources.insert(part, bytes);
        }
        self.typing_history = None;
        Ok(self.finish_edit_with(caret, &damage_of(&pending.operations)))
    }

    /// Drops the preview `id` if it is this editor's. Whether one was dropped.
    fn ai_discard_inner(&self, id: u32) -> bool {
        let owner = core::ptr::from_ref(self) as usize;
        PENDING.with(|pending| {
            let mut slot = pending.borrow_mut();
            let mine = slot
                .as_ref()
                .is_some_and(|held| held.id == id && held.owner == owner);
            if mine {
                *slot = None;
            }
            mine
        })
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// The assistant's reads (`docs/178` §4.1), over the live document:
    /// `request` is `{"tool": "outline" | "read" | "search" | "trackedChanges"
    /// | "comments", ...}` with that read's arguments. Returns `{ok, value}` or
    /// `{ok: false, error}` as JSON. **O(document)**: a clone plus the read.
    #[wasm_bindgen(js_name = aiRead)]
    #[must_use]
    pub fn ai_read(&self, request: &str, access: &str) -> String {
        answer(self.ai_read_value(request, access))
    }

    /// Compiles an edit plan privately (`docs/181` §4.4): returns the redline
    /// of every affected block, each edit's refusal, the mode and the author,
    /// and a `previewId` when it could be accepted. Changes nothing. `date` is
    /// the suggestion date (ISO 8601). **O(document)** for the clone plus
    /// O(affected blocks).
    #[wasm_bindgen(js_name = aiPreview)]
    #[must_use]
    pub fn ai_preview(&self, plan: &str, access: &str, direct: bool, date: &str) -> String {
        answer(self.ai_preview_value(plan, access, direct, date))
    }

    /// Accepts the preview `id`: commits exactly its operations as one undo
    /// step through the editor's choke point. Refused when it is gone, stale,
    /// or the engine refuses it. **O(operations)** plus the re-layout.
    #[wasm_bindgen(js_name = aiAccept)]
    pub fn ai_accept(&mut self, id: u32) -> Result<EditResult, JsValue> {
        self.ai_accept_inner(id).map_err(to_js)
    }

    /// Discards the preview `id`. Nothing it compiled is kept. O(1).
    #[wasm_bindgen(js_name = aiDiscard)]
    #[must_use]
    pub fn ai_discard(&self, id: u32) -> bool {
        self.ai_discard_inner(id)
    }
}

#[cfg(test)]
mod tests {
    use super::{PENDING, WasmDocument};
    use crate::open_document;
    use casual_doc_model::v1::BlockNode;
    use serde_json::{Value, json};

    const TEXT: &[u8] =
        b"The notice period is thirty (30) days.\nTermination for convenience applies.\n";
    const DATE: &str = "2026-10-28T09:00:00Z";

    fn editor() -> WasmDocument {
        let mut d = open_document(TEXT).expect("a text file opens");
        d.set_active_author("Ada Lovelace", None, None)
            .expect("an identity");
        d
    }

    fn value(answer: &str) -> Value {
        let parsed: Value = serde_json::from_str(answer).expect("JSON");
        assert_eq!(parsed["ok"], true, "{parsed}");
        parsed["value"].clone()
    }

    fn node_of(d: &WasmDocument, needle: &str) -> String {
        let hits = value(&d.ai_read(
            &json!({ "tool": "search", "query": needle }).to_string(),
            "read",
        ));
        hits["hits"][0]["node"].as_str().expect("a hit").to_owned()
    }

    fn revision(d: &WasmDocument) -> u64 {
        value(&d.ai_read(&json!({ "tool": "outline" }).to_string(), "read"))["revision"]
            .as_u64()
            .expect("a revision")
    }

    fn suggestion(d: &WasmDocument) -> String {
        let node = node_of(d, "thirty");
        json!({
            "base_revision": revision(d),
            "edits": [
                { "op": "replace_text", "target": { "node": node, "exact": "thirty (30)" }, "text": "sixty (60)" },
                { "op": "insert_paragraphs", "after": { "node": node }, "paragraphs": [{ "text": "Notice is given in writing." }] },
            ],
        })
        .to_string()
    }

    fn texts(d: &WasmDocument) -> Vec<String> {
        d.document
            .body()
            .iter()
            .filter_map(|block| match block {
                BlockNode::Paragraph(paragraph) => {
                    Some(casual_doc_ai::text::paragraph_text(paragraph))
                }
                _ => None,
            })
            .collect()
    }

    fn content(d: &WasmDocument) -> String {
        casual_doc_diff::content_digest_hex(&d.document)
    }

    #[test]
    fn a_preview_shows_the_redline_and_changes_nothing() {
        let d = editor();
        let before = content(&d);
        let head = d.log.head();
        let undo = d.log.undo_depth();
        let preview = value(&d.ai_preview(&suggestion(&d), "suggest", false, DATE));

        assert_eq!(preview["wouldApply"], true, "{preview}");
        assert!(preview["previewId"].as_u64().is_some());
        assert_eq!(preview["author"], "Ada Lovelace (AI)");
        assert_eq!(preview["mode"], "suggest");
        let runs = preview["redline"][0]["afterRuns"].to_string();
        assert!(
            runs.contains("\"deleted\"") && runs.contains("\"inserted\""),
            "{runs}"
        );

        assert_eq!(content(&d), before, "the document is untouched");
        assert_eq!(d.log.head(), head, "no revision");
        assert_eq!(d.log.undo_depth(), undo, "nothing to undo");
    }

    #[test]
    fn accepting_commits_exactly_the_previewed_operations_as_one_undo_step() {
        let mut d = editor();
        let before = content(&d);
        let preview = value(&d.ai_preview(&suggestion(&d), "suggest", false, DATE));
        let id = u32::try_from(preview["previewId"].as_u64().expect("an id")).expect("u32");
        let previewed = PENDING.with(|pending| {
            format!(
                "{:?}",
                pending
                    .borrow()
                    .as_ref()
                    .expect("a pending plan")
                    .operations
            )
        });
        let head = d.log.head();

        d.ai_accept_inner(id).expect("accepted");

        let commit = d.log.commits().last().expect("a commit");
        assert_eq!(
            format!("{:?}", commit.operations()),
            previewed,
            "the commit is the preview's operations, unchanged"
        );
        assert_eq!(d.log.head().get(), head.get() + 1, "one transaction");
        assert!(
            texts(&d)
                .iter()
                .any(|text| text == "Notice is given in writing.")
        );
        let changes = value(&d.ai_read(&json!({ "tool": "trackedChanges" }).to_string(), "read"));
        let authors: Vec<&str> = changes["trackedChanges"]
            .as_array()
            .expect("changes")
            .iter()
            .filter_map(|change| change["author"].as_str())
            .collect();
        assert!(!authors.is_empty(), "{changes}");
        assert!(
            authors.iter().all(|author| *author == "Ada Lovelace (AI)"),
            "{authors:?}"
        );

        d.undo_inner().expect("one undo");
        assert_eq!(content(&d), before, "one undo removes the whole plan");
        assert!(
            d.ai_accept_inner(id).is_err(),
            "an accepted preview cannot be accepted twice"
        );
    }

    #[test]
    fn discarding_leaves_no_trace() {
        let mut d = editor();
        let before = content(&d);
        let undo = d.log.undo_depth();
        let preview = value(&d.ai_preview(&suggestion(&d), "suggest", false, DATE));
        let id = u32::try_from(preview["previewId"].as_u64().expect("an id")).expect("u32");
        assert!(d.ai_discard_inner(id));
        assert!(PENDING.with(|pending| pending.borrow().is_none()));
        let refused = d.ai_accept_inner(id).expect_err("discarded");
        assert!(refused.contains("no longer open"), "{refused}");
        assert_eq!(content(&d), before);
        assert_eq!(d.log.undo_depth(), undo);
    }

    #[test]
    fn a_preview_the_document_moved_past_is_refused_on_accept() {
        let mut d = editor();
        let preview = value(&d.ai_preview(&suggestion(&d), "suggest", false, DATE));
        let id = u32::try_from(preview["previewId"].as_u64().expect("an id")).expect("u32");
        let node = node_of(&d, "Termination");
        d.apply(casual_doc_edit::Operation::InsertText {
            at: casual_doc_edit::Pos::new(node.parse().expect("an id"), 0),
            text: "Early ".to_owned(),
        })
        .expect("the person types");
        let before = content(&d);
        let refused = d.ai_accept_inner(id).expect_err("stale");
        assert!(refused.contains("changed"), "{refused}");
        assert_eq!(content(&d), before);
    }

    #[test]
    fn readonly_access_is_refused_by_the_engine_and_none_reads_nothing() {
        let d = editor();
        let preview = value(&d.ai_preview(&suggestion(&d), "read", false, DATE));
        assert_eq!(preview["wouldApply"], false);
        assert!(preview["previewId"].is_null());
        assert_eq!(
            preview["edits"][0]["refusal"]["code"],
            "capability-withheld"
        );

        let none: Value =
            serde_json::from_str(&d.ai_read(&json!({ "tool": "outline" }).to_string(), "none"))
                .expect("JSON");
        assert_eq!(none["ok"], false);
        assert_eq!(none["error"]["code"], "capability-withheld");
    }

    /// The engine, not the assistant's access level, holds the line: a host
    /// that resolved `suggest` for a replica whose own grant is read-only
    /// (a room's word) is refused by the choke point's check at preview time,
    /// and an Accept is never offered.
    #[test]
    fn the_replicas_own_grant_refuses_what_the_access_level_would_allow() {
        let mut d = editor();
        d.adopt_participant_capabilities_internal(&[])
            .expect("narrowed to a viewer");
        let preview = value(&d.ai_preview(&suggestion(&d), "suggest", false, DATE));
        assert_eq!(preview["wouldApply"], false, "{preview}");
        assert!(preview["previewId"].is_null());
        assert_eq!(preview["editorRefusal"]["code"], "engine-refused");
    }

    #[test]
    fn a_direct_plan_needs_the_hosts_grant() {
        let d = editor();
        let node = node_of(&d, "thirty");
        let direct = json!({
            "base_revision": revision(&d),
            "mode": "direct",
            "edits": [{ "op": "insert_text", "at": { "node": node, "exact": "days" }, "text": " net" }],
        })
        .to_string();
        for access in ["suggest", "review"] {
            let refused = value(&d.ai_preview(&direct, access, false, DATE));
            assert_eq!(refused["wouldApply"], false, "{access}: {refused}");
        }
        let granted = value(&d.ai_preview(&direct, "suggest", true, DATE));
        assert_eq!(granted["wouldApply"], true, "{granted}");
    }

    #[test]
    fn every_id_an_accepted_plan_minted_stays_unique() {
        let mut d = editor();
        let preview = value(&d.ai_preview(&suggestion(&d), "suggest", false, DATE));
        let id = u32::try_from(preview["previewId"].as_u64().expect("an id")).expect("u32");
        d.ai_accept_inner(id).expect("accepted");
        // The editor's next mint (an Enter, a paste) must name nothing that
        // exists already.
        let space = casual_doc_model::IdSpace::new(d.edit_ids.namespace());
        let next = d.edit_ids.next_counter();
        assert!(
            d.document.highest_counter_in(space) < next,
            "the editor would mint counter {next}, which the plan already put in the document"
        );
    }

    /// `docs/147`, `docs/181` G6 for this file: the assistant's door reaches
    /// the document only through `apply_group`. The editor's own guard
    /// (`every_document_mutation_is_a_transaction`) scans `lib.rs` alone, so
    /// this file holds itself to the same rule.
    #[test]
    fn the_assistant_mutates_only_through_the_choke_point() {
        let source = include_str!("ai.rs");
        let code = &source[..source.find("#[cfg(test)]").expect("a test module")];
        for forbidden in [
            "Transaction::reserve(",
            "casual_doc_edit::apply(",
            ".apply(&mut self.document",
            "definitions_mut(",
            "body_mut(",
            "self.document =",
        ] {
            assert!(
                !code.contains(forbidden),
                "ai.rs contains `{forbidden}`: a mutation outside `apply_group`"
            );
        }
        assert_eq!(
            code.matches("self.apply_group(").count(),
            1,
            "Accept commits through the choke point exactly once"
        );
    }
}
