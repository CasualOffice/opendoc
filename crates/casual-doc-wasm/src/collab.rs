// SPDX-License-Identifier: Apache-2.0

//! The session facade: the protocol state machine the browser's transport drives.
//!
//! # What this is, and the line it draws
//!
//! The browser owns the **socket**. This owns the **session**. That division is the whole
//! design, and it is the one ADR-063 records: a browser's transport is the host's `WebSocket`,
//! so nothing here opens a connection, reads a clock, retries, backs off or counts attempts —
//! and nothing in `webapp/src` parses a frame, decides what a refusal means or touches a
//! document.
//!
//! So `webapp/src/collab_transport.mjs` is a reconnecting byte pipe with an outbound queue, and
//! every byte it carries was produced here and every byte it receives is handed back here.
//!
//! # The established pattern, named before any code — SKILL §8
//!
//! A **sans-I/O protocol implementation**: the state machine is a pure function of the bytes it
//! is fed, and the transport is a separate object that does nothing but move them. It is the
//! shape `h11`, `hyper`'s core and every well-factored protocol library has, and it is the same
//! factoring `casual_doc_transaction::protocol`'s own header already states for the engine. The
//! reason to say so first is that the tempting alternative — a WASM module that holds a
//! `WebSocket` through `web-sys` — would put reconnection, back-off and the browser's event loop
//! inside a crate that must also compile for a native test, and would make the session
//! untestable without a browser.
//!
//! # Why the grant arriving here cannot widen anything
//!
//! `Welcome` and `Resumed` carry the capabilities the relay decided, and this module is where
//! the URL grant stops being the source of truth. The value is applied through
//! `WasmDocument::adopt_participant_capabilities`, which **intersects**, so what arrives on the
//! wire can only ever narrow what this replica holds — exactly as a value supplied from the
//! query string can. A relay that widened a grant mid-session is therefore not obeyed by this
//! replica until it reloads, which is the safe direction and the one
//! `adopt_participant_capabilities` documents: the relay keeps its own copy and judges every
//! submission against it, so a client that is too narrow disables a control and a client that
//! is too wide is refused on the wire.
//!
//! # The choke point is not bypassed, and here is why
//!
//! `every_document_mutation_is_a_transaction` scans `lib.rs` and requires exactly one
//! `.apply(&mut self.document, transaction)` call site, inside `apply_group`. Nothing here adds
//! a second: an arrival is applied by
//! [`ClientSession::receive`](casual_doc_transaction::session::ClientSession::receive), which
//! goes down `RevisionLog::apply` **inside the engine crate** — the same envelope, one layer
//! further in. That is the point of ADR-005 rather than an exception to it, and
//! `the_collab_facade_applies_nothing_outside_the_session` holds the line for this module the
//! way the lib.rs guard holds it for that one.
//!
//! # What is deliberately still owed
//!
//! **The local caret is not mapped across an arrival** (`107` P-4). A remote insertion before
//! this reader's caret moves their text and leaves the caret where it was, by offset. The host
//! is told the document changed and re-rasters; it is not told where its caret went, because
//! this engine does not yet know. Saying so is the point — the alternative is a caret that is
//! silently wrong, which is worse than one the host can see is unmapped.
//!
//! # Complexity
//!
//! O(bytes) to decode, and whatever the arrival costs to apply — which on the uncontended path
//! is one `RevisionLog::apply`, the same call a keystroke makes. Nothing here walks the
//! document; the re-pagination after an arrival is the incremental one, through `damage_of`.

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::refusal::marked;
use casual_doc_transaction::codec::{CodecError, decode_frame, encode_frame};
use casual_doc_transaction::protocol::{
    ClientMessage, Identity, Join, PROTOCOL_VERSION, Refusal, Resume, ResumeKey, Revision,
    ServerMessage,
};
use casual_doc_transaction::session::ClientSession;
use wasm_bindgen::prelude::*;

use crate::{WasmDocument, damage_of, to_js};
use casual_doc_edit::Pos;

/// What feeding one frame to the session did, as the browser's transport reads it.
///
/// One shape for every message kind, with the fields a kind does not use left out, because the
/// alternative — a method per message — would make the transport branch on the wire vocabulary
/// and so teach it the protocol it is deliberately ignorant of.
#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    /// `welcome`, `resumed`, `ack`, `applied`, `refused`, `stopped`, `awareness`, `departed`.
    kind: &'static str,
    /// The participant number the relay assigned, on a `welcome` or a `resumed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    participant: Option<u64>,
    /// The capability names this replica holds **after** the intersection, on a `welcome` or a
    /// `resumed`. What the chrome disables a control from; never what the relay sent verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<Vec<String>>,
    /// The ordered position this message concerns.
    revision: u64,
    /// How many missed arrivals a `resumed` carried and this replica has now merged.
    #[serde(skip_serializing_if = "Option::is_none")]
    missed: Option<usize>,
    /// How many of this replica's own unordered commits were rolled back and replayed.
    /// **Zero on the uncontended path**, which is the path that must cost nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    replayed: Option<usize>,
    /// Operations dropped because the thing they addressed no longer exists, as
    /// `operation against operation`. **Never empty silently**: a tombstone is a loss, and the
    /// host is the only thing that knows who to tell (`35`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tombstones: Vec<String>,
    /// The `ODC-7xxx` code, on a `refused` or a `stopped`.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    /// Whether the client may send the same thing again. `false` on anything that is not a
    /// refusal, so a transport cannot read a missing field as permission to retry.
    retryable: bool,
    /// Whether receiving this ended the session.
    terminal: bool,
    /// Whose presence or departure this is, on an `awareness` or a `departed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<u64>,
    /// Whether the document changed, so the host re-rasters.
    document_changed: bool,
    /// The host-visible view epoch, after any re-pagination this caused.
    view_revision: u32,
    /// Page indices whose layout changed, so the host re-rasters only those.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    dirty: Vec<u32>,
    /// The page count after this message.
    page_count: u32,
}

impl Outcome {
    fn of(kind: &'static str) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }
}

/// The session's state, as the chrome shows it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct State {
    /// Whether a `Welcome` or a `Resumed` has been adopted.
    joined: bool,
    /// The participant number, when joined.
    #[serde(skip_serializing_if = "Option::is_none")]
    participant: Option<u64>,
    /// The ordered position this session has reached.
    revision: u64,
    /// Whether any of this replica's work is still unacknowledged. **Stays true while
    /// `desynced` is latched**, so the loss is nameable rather than quietly forgotten.
    unacknowledged: bool,
    /// Whether this replica has diverged from the order. One way; never cleared.
    desynced: bool,
    /// The `ODC-7xxx` code the session stopped with, if it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    stopped: Option<&'static str>,
    /// The capability names this replica holds.
    capabilities: Vec<String>,
}

/// The reader-facing sentence and code for a frame this build could not read.
///
/// `ODC-7007` and not a generic failure: "the message could not be read, do not send it again"
/// is one of the three refusals `protocol::Refusal` says must never be collapsed, and the
/// sibling engine lost a debugging session to answering an unparseable message with
/// `CannotMerge`.
fn malformed(error: &CodecError) -> String {
    marked(
        Refusal::Malformed.code(),
        &format!("A message from the shared session could not be read ({error})."),
    )
}

#[wasm_bindgen]
impl WasmDocument {
    /// The opening frame: a `Join` this replica's transport writes first and nothing before.
    ///
    /// `identity` is the host's opaque name for the person, `resume_key` the disambiguator this
    /// tab holds for its life, and `revision` the ordered position to be caught up from.
    ///
    /// **The key travels on every join, including the first**, and that is the protocol rather
    /// than a precaution: a key is recorded only by the join that presented it, so a client
    /// which withheld it on its first connection cannot be recognised on its second — it is
    /// handed a fresh `Welcome` and a new participant number, and whatever it had not had
    /// acknowledged is gone with no refusal naming the loss. So this takes the key
    /// unconditionally and the caller cannot forget it.
    ///
    /// # Errors
    ///
    /// An empty identity, or a resume key over its 128-character bound. Refused rather than
    /// trimmed: both are the host's values and a silently altered identity is a participant
    /// nobody can match.
    ///
    /// O(1).
    #[wasm_bindgen(js_name = collabJoinFrame)]
    pub fn collab_join_frame(
        &self,
        identity: &str,
        resume_key: &str,
        revision: f64,
    ) -> Result<Vec<u8>, JsValue> {
        self.collab_join_frame_inner(identity, resume_key, revision)
            .map_err(to_js)
    }

    /// Feeds one frame from the relay to the session, and applies whatever it carried.
    ///
    /// Returns an [`Outcome`] as JSON. A **refusal is not an error**: it is an outcome with a
    /// code, because the connection survives one and a transport that caught it as an exception
    /// would have to decide what to do with a `StaleBase` — which is exactly the decision this
    /// module exists to keep. The only errors are a frame that could not be read and an arrival
    /// this replica could not merge.
    ///
    /// # Errors
    ///
    /// `ODC-7007` for a frame the codec refused, and the session's own `ODC-7xxx` code for an
    /// arrival that could not be merged, both carrying a sentence a reader can act on.
    ///
    /// O(bytes), plus one `RevisionLog::apply` on the uncontended path.
    #[wasm_bindgen(js_name = collabReceiveFrame)]
    pub fn collab_receive_frame(&mut self, frame: &[u8]) -> Result<String, JsValue> {
        self.collab_receive_frame_inner(frame).map_err(to_js)
    }

    /// The next chunk of this replica's own work to send, or `None` when there is nothing to
    /// send or no room to send it.
    ///
    /// `None` is **not** a fault and the transport must not treat it as one: the session has
    /// stopped, it has desynced, it is waiting for an ordered position a refusal named, 32
    /// chunks are already in flight, or there is simply nothing new. The last of those is the
    /// common case, which is why this is a poll rather than an event.
    ///
    /// O(the chunk).
    #[wasm_bindgen(js_name = collabNextChunk)]
    pub fn collab_next_chunk(&mut self) -> Option<Vec<u8>> {
        let session = self.session.as_mut()?;
        let submission = session.flush(&self.log)?;
        Some(encode_frame(&ClientMessage::Submit(submission)))
    }

    /// A `Leave` frame: departing deliberately rather than by disconnecting.
    ///
    /// Worth sending even though a disconnect is detected anyway, because the two reach the
    /// other participants differently — a deliberate leave is a clean close the relay answers
    /// at once, and a dropped socket waits on the operating system.
    ///
    /// O(1).
    #[wasm_bindgen(js_name = collabLeaveFrame)]
    #[must_use]
    pub fn collab_leave_frame(&self) -> Vec<u8> {
        encode_frame(&ClientMessage::Leave)
    }

    /// The session's state, as JSON, for a chrome that shows it.
    ///
    /// Always answerable, including before a join and after a stop, so the chrome has one thing
    /// to ask rather than three. O(1).
    #[wasm_bindgen(getter, js_name = collabState)]
    #[must_use]
    pub fn collab_state(&self) -> String {
        let state = State {
            joined: self.session.is_some(),
            participant: self.session.as_ref().map(|s| s.client().get()),
            revision: self
                .session
                .as_ref()
                .map_or(0, |s| s.revision().get()),
            unacknowledged: self.session.as_ref().is_some_and(ClientSession::has_unacknowledged),
            desynced: self.session.as_ref().is_some_and(ClientSession::is_desynced),
            stopped: self
                .session
                .as_ref()
                .and_then(ClientSession::stopped_because)
                .map(Refusal::code),
            capabilities: self.participant_capabilities(),
        };
        serde_json::to_string(&state).unwrap_or_else(|_| "{}".to_owned())
    }
}

impl WasmDocument {
    /// See [`WasmDocument::collab_join_frame`]. Plain `Result<_, String>` so the session guards
    /// run under `cargo test` on native targets.
    fn collab_join_frame_inner(
        &self,
        identity: &str,
        resume_key: &str,
        revision: f64,
    ) -> Result<Vec<u8>, String> {
        let identity = Identity::new(identity).ok_or_else(|| {
            marked(
                "session.no-identity",
                "A shared session needs an identity for the person joining.",
            )
        })?;
        let key = ResumeKey::new(resume_key).ok_or_else(|| {
            marked(
                "session.resume-key-unusable",
                "The key this tab would be recognised by is empty or too long.",
            )
        })?;
        // A negative or fractional position is not "somewhere near": it is a value the caller
        // did not mean, and resuming from a guessed position is how a replica silently misses
        // the arrivals between the two.
        let at = if revision.is_finite() && revision >= 0.0 {
            #[expect(
                clippy::cast_sign_loss,
                clippy::cast_possible_truncation,
                reason = "guarded non-negative and finite immediately above"
            )]
            Revision::new(revision as u64)
        } else {
            return Err(marked(
                "session.resume-position-unusable",
                "The position to resume this shared session from is not a number.",
            ));
        };
        Ok(encode_frame(&ClientMessage::Join(Join {
            protocol: PROTOCOL_VERSION,
            identity,
            // No grant: this engine never verifies one and never invents one. ADR-060 —
            // the host signs, the boundary verifies, the engine enforces what came back.
            grant: None,
            resume: Some(Resume { key, revision: at }),
        })))
    }

    /// See [`WasmDocument::collab_receive_frame`].
    fn collab_receive_frame_inner(&mut self, frame: &[u8]) -> Result<String, String> {
        let message: ServerMessage = decode_frame(frame).map_err(|error| malformed(&error))?;
        let outcome = self.collab_apply_message(&message)?;
        serde_json::to_string(&outcome).map_err(|error| error.to_string())
    }

    /// Drives the session with one decoded message.
    fn collab_apply_message(&mut self, message: &ServerMessage) -> Result<Outcome, String> {
        match message {
            ServerMessage::Welcome {
                client,
                revision,
                capabilities,
                ..
            } => {
                let session = ClientSession::joined(&self.document, message, &mut self.log)
                    .map_err(|error| {
                        marked(error.refusal().code(), &format!("Joining the shared session failed: {error}."))
                    })?;
                let participant = session.client().get();
                self.session = Some(session);
                self.collab_adopt(participant, *capabilities)?;
                let mut outcome = Outcome::of("welcome");
                outcome.participant = Some(client.get());
                outcome.capabilities = Some(self.participant_capabilities());
                outcome.revision = revision.get();
                outcome.view_revision = self.revision;
                outcome.page_count = self.page_count();
                Ok(outcome)
            }
            ServerMessage::Resumed {
                client,
                revision,
                missed,
                capabilities,
                ..
            } => {
                let Some(session) = self.session.as_mut() else {
                    return Err(marked(
                        Refusal::Malformed.code(),
                        "A resume arrived for a shared session this tab never joined.",
                    ));
                };
                session.resumed(message).map_err(|error| {
                    marked(error.refusal().code(), &format!("Resuming the shared session failed: {error}."))
                })?;
                let participant = session.client().get();
                self.collab_adopt(participant, *capabilities)?;
                // The missed arrivals travel INSIDE the message, so they are merged before
                // anything can be flushed; `awaiting` enforces that as state rather than as an
                // assumption about ordering.
                let mut tombstones = Vec::new();
                let mut replayed = 0;
                let mut changed = false;
                for arrival in missed {
                    let reception = self.collab_merge(arrival)?;
                    tombstones.extend(reception.0);
                    replayed += reception.1;
                    changed = true;
                }
                let relaid = if changed {
                    Some(self.collab_relayout())
                } else {
                    None
                };
                let mut outcome = Outcome::of("resumed");
                outcome.participant = Some(client.get());
                outcome.capabilities = Some(self.participant_capabilities());
                outcome.revision = revision.get();
                outcome.missed = Some(missed.len());
                outcome.replayed = Some(replayed);
                outcome.tombstones = tombstones;
                outcome.document_changed = changed;
                if let Some((dirty, pages)) = relaid {
                    outcome.dirty = dirty;
                    outcome.page_count = pages;
                } else {
                    outcome.page_count = self.page_count();
                }
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
            ServerMessage::Ack { through, revision } => {
                let Some(session) = self.session.as_mut() else {
                    return Err(malformed_state("an acknowledgement"));
                };
                session
                    .acknowledge(*through, *revision, &mut self.log)
                    .map_err(|error| {
                        marked(error.refusal().code(), &format!("The shared session refused an acknowledgement: {error}."))
                    })?;
                let mut outcome = Outcome::of("ack");
                outcome.revision = revision.get();
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
            ServerMessage::Apply(arrival) => {
                if self.session.is_none() {
                    return Err(malformed_state("an arrival"));
                }
                let (tombstones, replayed) = self.collab_merge(arrival)?;
                let (dirty, pages) = self.collab_relayout();
                let mut outcome = Outcome::of("applied");
                outcome.revision = arrival.revision.get();
                outcome.about = Some(arrival.client.get());
                outcome.replayed = Some(replayed);
                outcome.tombstones = tombstones;
                outcome.document_changed = true;
                outcome.dirty = dirty;
                outcome.page_count = pages;
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
            ServerMessage::Refused { seq, reason } => {
                let Some(session) = self.session.as_mut() else {
                    return Err(malformed_state("a refusal"));
                };
                session.refused(*seq, *reason, &self.log).map_err(|error| {
                    marked(error.refusal().code(), &format!("The shared session has stopped: {error}."))
                })?;
                let mut outcome = Outcome::of("refused");
                outcome.code = Some(reason.code());
                outcome.retryable = reason.is_retryable();
                outcome.terminal = reason.is_terminal();
                outcome.revision = session.revision().get();
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
            ServerMessage::Stopped { reason } => {
                // Recorded even on a session that never established, because a `Stopped` is how
                // a protocol mismatch and a full room arrive — the two cases where there is no
                // session to record it on and the reader most needs the reason.
                if let Some(session) = self.session.as_mut() {
                    session.stop(*reason);
                }
                let mut outcome = Outcome::of("stopped");
                outcome.code = Some(reason.code());
                outcome.terminal = true;
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
            ServerMessage::Awareness { client, .. } => {
                let mut outcome = Outcome::of("awareness");
                outcome.about = Some(client.get());
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
            ServerMessage::Departed { client } => {
                let mut outcome = Outcome::of("departed");
                outcome.about = Some(client.get());
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
        }
    }

    /// Adopts the identity and the capabilities a `Welcome` or a `Resumed` carried.
    ///
    /// Identity first, then capabilities: the identity call partitions this replica's minting
    /// (`152` §4.2) and must land before the first edit the participant intends to share.
    ///
    /// The capabilities go through `adopt_participant_capabilities_internal`, which
    /// **intersects**, so a value arriving on the wire narrows and never widens.
    fn collab_adopt(&mut self, participant: u64, granted: Capabilities) -> Result<(), String> {
        self.adopt_participant_identity_internal(participant)?;
        self.adopt_participant_capabilities_internal(&capability_names(granted))
    }

    /// Merges one arrival, returning the tombstones as sentences and how much was replayed.
    fn collab_merge(
        &mut self,
        arrival: &casual_doc_transaction::protocol::Arrival,
    ) -> Result<(Vec<String>, usize), String> {
        // A windowed body cannot re-paginate after a mutation, which is the same refusal
        // `apply_group` makes for a local edit and for the same reason: an arrival that landed
        // against a layout nothing can bring up to date is an edit the reader cannot see.
        if self.layout.is_windowed() {
            return Err(marked(
                "session.windowed-not-available",
                "This document is too large to share while it is being read in windows.",
            ));
        }
        let Some(session) = self.session.as_mut() else {
            return Err(malformed_state("an arrival"));
        };
        let reception = session
            .receive(
                arrival,
                &mut self.document,
                &mut self.edit_ids,
                &mut self.log,
            )
            .map_err(|error| {
                marked(
                    error.refusal().code(),
                    &format!("A change from the shared session could not be applied: {error}."),
                )
            })?;
        Ok((
            reception
                .tombstones
                .iter()
                .map(|tombstone| format!("{} against {}", tombstone.operation, tombstone.against))
                .collect(),
            reception.replayed,
        ))
    }

    /// Re-paginates after an arrival and returns the pages whose layout changed.
    ///
    /// Through the **incremental** path, with the damage the arrival's operations describe, so a
    /// remote keystroke costs what a local one costs rather than `O(document)` (`107` §4 B1).
    /// The caret handed to `finish_edit_with` is the document's own anchor and is deliberately
    /// ignored: a remote arrival does not move this reader's caret, and this engine cannot yet
    /// say where it went (`107` P-4, the module header).
    fn collab_relayout(&mut self) -> (Vec<u32>, u32) {
        let operations: Vec<_> = self
            .log
            .commits()
            .last()
            .map(|commit| commit.operations().to_vec())
            .unwrap_or_default();
        let damage = damage_of(&operations);
        let anchor = Pos::new(self.document.id(), 0);
        let result = self.finish_edit_with(anchor, &damage);
        (result.dirty_pages(), result.page_count())
    }
}

/// The refusal for a message that arrived before the session it belongs to.
fn malformed_state(what: &str) -> String {
    marked(
        Refusal::Malformed.code(),
        &format!("{what} arrived for a shared session this tab has not joined."),
    )
}

/// The capability names `adopt_participant_capabilities` accepts, from a [`Capabilities`].
///
/// The inverse of the facade's own match, and in the same order the getter reports, so a host
/// can compare what it sent with what came back. A sixth capability added to the engine fails
/// to compile here rather than arriving narrowed by accident.
fn capability_names(granted: Capabilities) -> Vec<String> {
    let mut names = Vec::new();
    if granted.may_comment() {
        names.push("comment".to_owned());
    }
    if granted.may_edit() {
        names.push("edit".to_owned());
    }
    if granted.may_manage_protection() {
        names.push("manageProtection".to_owned());
    }
    if granted.may_review() {
        names.push("review".to_owned());
    }
    if granted.may_suggest() {
        names.push("suggest".to_owned());
    }
    names
}

#[cfg(test)]
#[path = "collab_tests.rs"]
mod tests;
