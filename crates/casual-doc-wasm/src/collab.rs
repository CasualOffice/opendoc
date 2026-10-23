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
//! `every_document_mutation_is_a_transaction` scans `lib.rs` and requires the envelope's one
//! document-mutating call to sit inside `apply_group` and nowhere else. Nothing here adds a
//! second: an arrival is applied by
//! [`ClientSession::receive`](casual_doc_transaction::session::ClientSession::receive), which
//! goes down `RevisionLog::apply` **inside the engine crate** — the same envelope, one layer
//! further in. That is the point of ADR-005 rather than an exception to it, and
//! `the_collab_facade_applies_nothing_outside_the_session` holds the line for this module the
//! way the lib.rs guard holds it for that one.
//!
//! That paragraph deliberately does not spell the forbidden call out, and `lib.rs`'s own prose
//! follows the same rule — it writes `casual_doc_edit::apply` without its parenthesis. Both
//! guards scan source text, so a sentence quoting what it forbids reddens the guard on prose
//! rather than on code. This header's first draft did exactly that, which is why the scan now
//! drops whole-line comments before it looks: a convention an author has to remember is a
//! convention that breaks, and dropping a line that is a comment in its entirety cannot hide
//! any code.
//!
//! # The reader's caret
//!
//! **Carried across every arrival** (`107` P-4, §2.2, ADR-069). The session composes, per
//! merged arrival, a map from the view the reader was looking at to the one they are looking
//! at now, and `collabMapSelection` carries the host's selection through it. The host calls
//! it synchronously in its frame handler, before it paints and before the next keystroke.
//! Until it did, a remote insertion before the caret left it at its old offset, and two
//! people typing in one paragraph produced the later typist's word reversed.
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
    ClientId, ClientMessage, Identity, Join, Member, PROTOCOL_VERSION, Refusal, Resume, ResumeKey,
    Revision, ServerMessage,
};
// Their own line, for the parallel-lane import rule.
use casual_doc_transaction::protocol::{GrantToken, Participant, Profile};
use casual_doc_transaction::session::ClientSession;
use wasm_bindgen::prelude::*;

use crate::{WasmDocument, damage_of, to_js};
use casual_doc_edit::Pos;
use casual_doc_io::DocumentResources;
use std::collections::BTreeMap;

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
    /// Whose presence, departure or access change this is.
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<u64>,
    /// The room's other participants, on a `welcome` or a `resumed`, and **empty for a
    /// participant the relay did not judge able to act on it**.
    ///
    /// The list a rights surface is built from. Empty is therefore a real answer and not a
    /// missing one — it is what a viewer gets, and what anybody gets in a room of one — which is
    /// why the surface decides what to offer from this rather than from a reader's own role
    /// alone.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    members: Vec<Membership>,
    /// The profile the relay will attach to this participant's own work, on a `welcome` or a
    /// `resumed` (`docs/175` §5.2) — what this replica now authors under.
    #[serde(skip_serializing_if = "Option::is_none")]
    me: Option<JsProfile>,
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

/// One participant of the room, as the chrome's rights surface reads them.
///
/// The names rather than the `Capabilities` value, because the chrome's whole vocabulary is the
/// name list — `session_access.mjs` holds the same six strings and `participantCapabilities`
/// reports them — and handing the chrome a second encoding of one fact is how the two drift.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Membership {
    /// Their participant number, which is what a `SetAccess` names.
    participant: u64,
    /// What they may do now.
    capabilities: Vec<String>,
    /// The most a rights change may give them: their own host-signed grant.
    ///
    /// Carried so the surface can offer exactly the roles that will be accepted. A surface that
    /// offered one the relay refuses is the dead control at a distance — it looks live, and the
    /// refusal arrives from the network.
    ceiling: Vec<String>,
}

/// A [`Profile`] as the chrome reads and writes it: the field names of `docs/175` §14
/// (`avatarUrl`, camel case), so the strip's `participantDisplay` takes it as it comes.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct JsProfile {
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    initials: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
}

impl From<&Profile> for JsProfile {
    fn from(profile: &Profile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            initials: profile.initials.clone(),
            color: profile.color.clone(),
            avatar_url: profile.avatar_url.clone(),
        }
    }
}

/// One participant present in the room, as the chrome's roster reads it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsParticipant {
    client: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<JsProfile>,
    attested: bool,
}

impl From<&Participant> for JsParticipant {
    fn from(participant: &Participant) -> Self {
        Self {
            client: participant.client.get(),
            profile: participant.profile.as_ref().map(JsProfile::from),
            attested: participant.attested,
        }
    }
}

/// A claimed profile from the host's JSON, checked against `docs/175` §4.4's bounds.
///
/// A bound is a refusal naming the field (`identity-invalid`), never a truncation: a name cut
/// short is a different name.
fn parse_profile(json: &str) -> Result<Profile, String> {
    let js: JsProfile = serde_json::from_str(json).map_err(|_| {
        marked(
            "identity-invalid",
            "The identity the host supplied is not an object with an id and a name.",
        )
    })?;
    let profile = Profile {
        id: js.id,
        name: js.name,
        initials: js.initials,
        color: js.color,
        avatar_url: js.avatar_url,
    };
    profile.check().map_err(|field| {
        marked(
            "identity-invalid",
            &format!(
                "The identity the host supplied has an unusable {}.",
                field.name()
            ),
        )
    })?;
    Ok(profile)
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
    /// Chunks handed to the transport and not yet acknowledged by the relay.
    in_flight: usize,
    /// This replica's commits not yet in any chunk: typed, and not yet sent.
    ///
    /// With `in_flight`, the two numbers a sync indicator needs (`109` HF-313): "your changes
    /// are reaching everyone" is true only when both are zero. An open socket says nothing
    /// about whether the last keystroke has been ordered.
    unsent: usize,
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
    /// `grant` is the host-signed JWT the host's provider handed out **for this join**
    /// (`docs/175` §3.3): carried and never read, stored or logged here. `profile` is a JSON
    /// `{"id","name","initials"?,"color"?,"avatarUrl"?}` the participant **claims**; a room that
    /// requires a grant ignores it and shows the grant's, and an open room shows it marked
    /// unverified with its colour and avatar dropped.
    ///
    /// # Errors
    ///
    /// An empty identity, a resume key over its 128-character bound, a grant over its 8 KiB
    /// bound, or a profile that is not JSON of that shape or breaks a bound (the field is named,
    /// `identity-invalid`). Refused rather than trimmed: these are the host's values, and a
    /// silently altered identity is a participant nobody can match.
    ///
    /// O(1).
    #[wasm_bindgen(js_name = collabJoinFrame)]
    pub fn collab_join_frame(
        &self,
        identity: &str,
        resume_key: &str,
        revision: f64,
        grant: Option<String>,
        profile: Option<String>,
    ) -> Result<Vec<u8>, JsValue> {
        self.collab_join_frame_inner(
            identity,
            resume_key,
            revision,
            grant.as_deref(),
            profile.as_deref(),
        )
        .map_err(to_js)
    }

    /// Feeds one frame from the relay to the session, and applies whatever it carried.
    ///
    /// Returns an `Outcome` as JSON — named in code text and not as a link, because the type
    /// is private and `RUSTDOCFLAGS="-D warnings"` refuses a public item linking to one.
    /// A **refusal is not an error**: it is an outcome with a
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

    /// Begins a new room whose seed is this exact document. Call only after host persistence.
    #[wasm_bindgen(js_name = collabPrepare)]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked safe integer"
    )]
    pub fn collab_prepare(&mut self, revision: f64) -> Result<(), JsValue> {
        if !revision.is_finite()
            || revision < 0.0
            || revision.fract() != 0.0
            || revision > 9_007_199_254_740_991.0
        {
            return Err(to_js("Invalid shared snapshot revision.".to_owned()));
        }
        self.session = None;
        self.bootstrap_revision = Some(revision as u64);
        self.log.begin_shared();
        Ok(())
    }

    /// Atomically reloads a checked snapshot and replays safely anchored pending text edits.
    /// Uncertain work remains downloadable through `collabRecoveryDocument`.
    #[wasm_bindgen(js_name = collabRecover)]
    pub fn collab_recover(&mut self, snapshot: &[u8], revision: f64) -> Result<String, JsValue> {
        self.collab_recover_inner(snapshot, revision).map_err(to_js)
    }

    /// Number of preserved local document snapshots, each directly reopenable.
    #[wasm_bindgen(js_name = collabRecoveryCount)]
    pub fn collab_recovery_count(&self) -> usize {
        self.recovery_backups.len()
    }

    /// A retained original document and its resources in a reopenable snapshot package.
    #[wasm_bindgen(js_name = collabRecoveryDocument)]
    pub fn collab_recovery_document(&self, index: usize) -> Result<Vec<u8>, JsValue> {
        self.recovery_backups
            .get(index)
            .cloned()
            .ok_or_else(|| to_js("No retained document at that index.".to_owned()))
    }

    /// Current normalized document with its image and font resources, bounded for sharing.
    #[wasm_bindgen(js_name = collabSnapshot)]
    pub fn collab_snapshot(&self) -> Result<Vec<u8>, JsValue> {
        snapshot_bytes(&self.document, &self.resources).map_err(to_js)
    }

    /// An acknowledged-only checkpoint: pending optimistic work is rolled back on a clone.
    #[wasm_bindgen(js_name = collabCheckpoint)]
    pub fn collab_checkpoint(&self) -> Result<Vec<u8>, JsValue> {
        if self
            .log
            .commits()
            .filter(|c| c.revision() > self.log.horizon())
            .any(|c| {
                c.operations().iter().any(|op| {
                    !matches!(
                        op,
                        casual_doc_edit::Operation::InsertText { .. }
                            | casual_doc_edit::Operation::DeleteText { .. }
                    )
                })
            })
        {
            return Err(to_js(
                "A resource-bearing checkpoint waits for structural edits to be acknowledged."
                    .to_owned(),
            ));
        }
        let document = casual_doc_transaction::recovery::acknowledged(&self.document, &self.log)
            .map_err(to_js)?;
        snapshot_bytes(&document, &self.resources).map_err(to_js)
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

    /// A `SetAccess` frame: change what **another** participant may do.
    ///
    /// `target` is the participant number a `welcome`'s or `resumed`'s membership list reported,
    /// and `granted` the capability names they should hold. There is deliberately **no actor
    /// argument**: the message has no field for one, and the relay attaches the participant
    /// number of the connection the frame arrived on — so claiming to be somebody else is
    /// unexpressible rather than merely rejected (`152` §2b).
    ///
    /// # What this does NOT do
    ///
    /// Check anything. Building a frame is not a decision, and pretending otherwise here would
    /// put a second implementation of the ceiling rule in the one place that cannot hold it: this
    /// replica knows the ceiling only because a relay told it. The decision is
    /// `ServerSession::set_access`, over `casual_doc_edit::access::refuse_access_change`, and a
    /// request outside the ceiling comes back `ODC-7011`. What the chrome's surface does with the
    /// ceiling is offer the roles that will be accepted, which is a courtesy and not a boundary.
    ///
    /// # Errors
    ///
    /// A capability name this build does not know, and a non-integral or negative participant
    /// number — both refused rather than coerced, for the reason `collab_join_frame` gives about
    /// a guessed position.
    ///
    /// O(the names).
    #[wasm_bindgen(js_name = collabSetAccessFrame)]
    pub fn collab_set_access_frame(
        &self,
        target: f64,
        granted: Vec<String>,
    ) -> Result<Vec<u8>, JsValue> {
        self.collab_set_access_frame_inner(target, &granted)
            .map_err(to_js)
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
                .map_or(self.bootstrap_revision.unwrap_or(0), |s| s.revision().get()),
            unacknowledged: self
                .session
                .as_ref()
                .is_some_and(ClientSession::has_unacknowledged),
            in_flight: self.session.as_ref().map_or(0, ClientSession::outstanding),
            unsent: self
                .session
                .as_ref()
                .map_or(0, |session| session.unsent(&self.log)),
            desynced: self
                .session
                .as_ref()
                .is_some_and(ClientSession::is_desynced),
            stopped: self
                .session
                .as_ref()
                .and_then(ClientSession::stopped_because)
                .map(Refusal::code),
            capabilities: self.participant_capabilities(),
        };
        serde_json::to_string(&state).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Carries the reader's selection across everything merged since the last call, and
    /// returns it as `{anchor: {node, offset}, focus: {node, offset}, moved}` JSON — `107` P-4.
    ///
    /// The host calls this **synchronously, once per frame that changed the document**,
    /// before it paints and before the next keystroke can read the selection. Without it a
    /// remote insertion earlier in the reader's paragraph left their caret at the old offset,
    /// so two people typing in one paragraph produced the later typist's word reversed on
    /// both screens, and a reader whose paragraph somebody typed into typed into the wrong
    /// place.
    ///
    /// # The boundary rule — where a caret goes when somebody types exactly at it
    ///
    /// A collapsed caret maps with `Affinity::Before`: text another person inserts at the
    /// reader's caret appears *after* the caret, and the reader's next keystroke goes before
    /// it. That keeps each typist's word in one piece when both type at one spot — with
    /// `After` the two words interleave a character at a time. A range maps its earlier end
    /// `After` and its later end `Before`, so text inserted at either edge stays outside the
    /// selection, which is what Word and Google Docs do with a remote insertion at the edge of
    /// your selection.
    ///
    /// A selection end whose paragraph was destroyed moves to the start of the block that now
    /// stands where the removed band began, when that was the body; otherwise it is returned
    /// unmapped with `removed: true` and the host places it (its own fallback validates every
    /// adopted position).
    ///
    /// # Complexity
    ///
    /// O(steps merged since the last call) — one step per arriving keystroke — and **never
    /// O(document)**; plus O(1) to place a caret whose paragraph was removed from the body.
    ///
    /// # Errors
    ///
    /// An end that is not a position this build can read.
    #[wasm_bindgen(js_name = collabMapSelection)]
    pub fn collab_map_selection(
        &mut self,
        anchor_node: &str,
        anchor_offset: u32,
        focus_node: &str,
        focus_offset: u32,
    ) -> Result<String, JsValue> {
        self.collab_map_selection_inner(anchor_node, anchor_offset, focus_node, focus_offset)
            .map_err(to_js)
    }
}

/// One mapped selection end, as the host reads it.
#[derive(Debug, serde::Serialize)]
struct MappedEnd {
    node: String,
    offset: u32,
}

/// What [`WasmDocument::collab_map_selection`] returns.
#[derive(Debug, serde::Serialize)]
struct MappedSelection {
    anchor: MappedEnd,
    focus: MappedEnd,
    /// Whether either end moved.
    moved: bool,
    /// Whether an end's paragraph was destroyed and could not be re-seated here.
    removed: bool,
}

impl WasmDocument {
    /// The operations Undo or Redo applies for `group`: the **selective** inverse, rebased
    /// over everybody's later edits (`casual_doc_transaction::selective`), so a shared
    /// document's Undo reverts this reader's step and nothing of their collaborators'.
    ///
    /// Alone in a document this is the recorded inverse, unchanged and at the same cost; the
    /// selective path runs only when somebody else's commit follows the step. An empty answer
    /// means somebody already removed everything the step did — the caller retires it.
    ///
    /// The document is borrowed as the scratch surface the carry needs and handed back as it
    /// was: nothing here mutates it, which is why this can sit in the session facade without
    /// being a second mutation path.
    ///
    /// # Errors
    ///
    /// `history.undo-conflict` with the reader's sentence when the step cannot be carried
    /// over a later edit — refused rather than guessed, because a guessed undo in a shared
    /// document deletes or rewrites somebody else's text.
    ///
    /// # Complexity
    ///
    /// `O(k · (k + m))` transforms for `k` operations in the step and `m` after it, bounded by
    /// the log's bound and O(1) in document size; paid on an Undo press, never a keystroke.
    pub(crate) fn selective_history_step(
        &mut self,
        group: casual_doc_transaction::GroupId,
    ) -> Result<Vec<casual_doc_edit::Operation>, String> {
        self.log
            .selective_inverse(group, &mut self.document, &mut self.edit_ids)
            .map_err(|error| marked("history.undo-conflict", error.reason()))
    }

    /// See [`WasmDocument::collab_map_selection`].
    fn collab_map_selection_inner(
        &mut self,
        anchor_node: &str,
        anchor_offset: u32,
        focus_node: &str,
        focus_offset: u32,
    ) -> Result<String, String> {
        use casual_doc_transaction::{Affinity, Position};
        let anchor = crate::parse_pos(anchor_node, anchor_offset)
            .map(|at| Pos::new(at.node, at.offset))
            .ok_or_else(|| "The selection's anchor is not a position.".to_owned())?;
        let focus = crate::parse_pos(focus_node, focus_offset)
            .map(|at| Pos::new(at.node, at.offset))
            .ok_or_else(|| "The selection's focus is not a position.".to_owned())?;
        let map = self
            .session
            .as_mut()
            .map(ClientSession::take_caret_map)
            .unwrap_or_default();
        let (anchor_affinity, focus_affinity) = if anchor == focus || anchor.node != focus.node {
            (Affinity::Before, Affinity::Before)
        } else if anchor.offset < focus.offset {
            (Affinity::After, Affinity::Before)
        } else {
            (Affinity::Before, Affinity::After)
        };
        let mut removed = false;
        let mut end = |pos: Pos, affinity| {
            let mapped = map.map_tracked(Position::new(pos.node, pos.offset, affinity));
            let mut at = Pos::new(mapped.position.node, mapped.position.offset);
            if mapped.removed {
                match mapped.site.and_then(|site| self.body_position_at(site)) {
                    Some(seated) => at = seated,
                    None => removed = true,
                }
            }
            at
        };
        let mapped_anchor = end(anchor, anchor_affinity);
        let mapped_focus = end(focus, focus_affinity);
        let moved = mapped_anchor != anchor || mapped_focus != focus;
        let out = MappedSelection {
            anchor: MappedEnd {
                node: mapped_anchor.node.to_string(),
                offset: mapped_anchor.offset,
            },
            focus: MappedEnd {
                node: mapped_focus.node.to_string(),
                offset: mapped_focus.offset,
            },
            moved,
            removed,
        };
        serde_json::to_string(&out).map_err(|error| error.to_string())
    }

    /// The first caret position in the body block that now stands where a removed band
    /// began — or in the last block, when the band ran to the end.
    ///
    /// Body-level only, which is the O(1) case; a removal inside a cell or a text box returns
    /// `None` and the host's own fallback places the caret. O(the nesting of that one block).
    fn body_position_at(&self, site: casual_doc_transaction::RemovalSite) -> Option<Pos> {
        if site.container.is_some() {
            return None;
        }
        let body = self.document.body();
        let index = usize::try_from(site.index)
            .ok()?
            .min(body.len().checked_sub(1)?);
        first_paragraph_in(&body[index]).map(|node| Pos::new(node, 0))
    }
}

/// The first paragraph inside a block, in document order. O(the nesting of the block).
fn first_paragraph_in(block: &casual_doc_model::v1::BlockNode) -> Option<casual_doc_model::NodeId> {
    use casual_doc_model::v1::BlockNode;
    match block {
        BlockNode::Paragraph(paragraph) => Some(paragraph.id),
        BlockNode::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .flat_map(|cell| cell.blocks.iter())
            .find_map(first_paragraph_in),
        BlockNode::Sdt(sdt) => sdt.blocks.iter().find_map(first_paragraph_in),
        BlockNode::AltChunk(_) => None,
    }
}

impl WasmDocument {
    /// The name, initials and id space the relay attaches to this replica's work, in a room
    /// that named this participant (`docs/175` §5.2) — `None` standalone, or in a room where
    /// nobody did. The facade's one answer, so the editor never reaches the session for it.
    pub(crate) fn room_author(&self) -> Option<(String, String, casual_doc_model::IdSpace)> {
        let session = self.session.as_ref()?;
        let me = session.me()?;
        Some((
            me.name.clone(),
            me.initials_or_derived(),
            session.id_space(),
        ))
    }

    fn collab_recover_inner(&mut self, snapshot: &[u8], revision: f64) -> Result<String, String> {
        if !revision.is_finite()
            || revision < 0.0
            || revision.fract() != 0.0
            || revision > 9_007_199_254_740_991.0
        {
            return Err("A recovery snapshot needs an exact non-negative revision.".to_owned());
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked safe integer"
        )]
        let revision = Revision::new(revision as u64);
        let session = self.session.clone();
        if revision
            < session.as_ref().map_or(
                Revision::new(self.bootstrap_revision.unwrap_or(0)),
                ClientSession::revision,
            )
        {
            return Err("The recovery snapshot is older than this replica.".to_owned());
        }
        if self.recovery_backups.len() >= 4 {
            return Err("The retained document limit was reached. Save your current document and open a fresh shared tab.".to_owned());
        }
        let mut next = crate::open_document(snapshot)?;
        let recovery = casual_doc_transaction::recovery::recover(
            &self.document,
            &self.log,
            &next.document,
            self.capabilities,
        )?;
        let retained = recovery.retained.len();
        let replayed = recovery.replayed;
        let mut backups = self.recovery_backups.clone();
        if retained > 0 {
            backups.push(snapshot_bytes(&self.document, &self.resources)?);
        }
        next.document = recovery.document;
        next.log = recovery.log;
        next.capabilities = self.capabilities;
        next.host_ceiling = self.host_ceiling;
        if let Some(mut session) = session {
            session
                .recovered(&next.document, revision, &next.log)
                .map_err(|e| e.to_string())?;
            next.collab_adopt(session.client().get(), session.capabilities())?;
            next.session = Some(session);
        } else {
            next.bootstrap_revision = Some(revision.get());
        }
        next.recovery_backups = backups;
        next.revision = self.revision;
        next.finish_edit(Pos::new(next.document.id(), 0));
        let outcome = serde_json::json!({ "kind": "recovered", "replayed": replayed,
            "retained": retained, "revision": revision.get(), "viewRevision": next.revision,
            "documentChanged": true, "pageCount": next.page_count() });
        *self = next;
        Ok(outcome.to_string())
    }

    /// See [`WasmDocument::collab_join_frame`]. Plain `Result<_, String>` so the session guards
    /// run under `cargo test` on native targets.
    fn collab_join_frame_inner(
        &self,
        identity: &str,
        resume_key: &str,
        revision: f64,
        grant: Option<&str>,
        profile: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        let grant = match grant.filter(|grant| !grant.is_empty()) {
            Some(grant) => Some(GrantToken::new(grant.as_bytes().to_vec()).ok_or_else(|| {
                marked(
                    "session.grant-unusable",
                    "The credential the host supplied for this shared session is too large.",
                )
            })?),
            None => None,
        };
        let profile = match profile.filter(|profile| !profile.is_empty()) {
            Some(profile) => Some(parse_profile(profile)?),
            None => None,
        };
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
            // Carried, never verified and never invented here. ADR-060 — the host signs, the
            // boundary verifies, the engine enforces what came back.
            grant,
            profile,
            resume: Some(Resume { key, revision: at }),
        })))
    }

    /// See [`WasmDocument::collab_set_access_frame`]. Plain `Result<_, String>` so the guards run
    /// under `cargo test` on native targets.
    fn collab_set_access_frame_inner(
        &self,
        target: f64,
        granted: &[String],
    ) -> Result<Vec<u8>, String> {
        if !target.is_finite() || target < 0.0 || target.fract() != 0.0 {
            return Err(marked(
                "session.participant-unusable",
                "That is not a participant of this shared document.",
            ));
        }
        #[expect(
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            reason = "guarded non-negative, finite and integral immediately above"
        )]
        let target = ClientId::new(target as u64);
        let capabilities = Self::capabilities_from_names(granted)?;
        Ok(encode_frame(&ClientMessage::SetAccess {
            target,
            capabilities,
        }))
    }

    /// See [`WasmDocument::collab_receive_frame`].
    pub(crate) fn collab_receive_frame_inner(&mut self, frame: &[u8]) -> Result<String, String> {
        let message: ServerMessage = decode_frame(frame).map_err(|error| malformed(&error))?;
        let outcome = match self.collab_apply_message(&message) {
            Ok(outcome) => outcome,
            Err(error)
                if casual_doc_edit::refusal::split(&error).1
                    == Some(Refusal::CannotMerge.code()) =>
            {
                if let Some(session) = self.session.as_mut() {
                    session.stop(Refusal::CannotMerge);
                }
                let mut outcome = Outcome::of("recoveryRequired");
                outcome.code = Some(Refusal::CannotMerge.code());
                outcome.revision = match &message {
                    ServerMessage::Apply(arrival) => arrival.revision.get(),
                    ServerMessage::Welcome { revision, .. }
                    | ServerMessage::Resumed { revision, .. } => revision.get(),
                    _ => self.session.as_ref().map_or(0, |s| s.revision().get()),
                };
                outcome
            }
            Err(error) => return Err(error),
        };
        serde_json::to_string(&outcome).map_err(|error| error.to_string())
    }

    /// Drives the session with one decoded message.
    fn collab_apply_message(&mut self, message: &ServerMessage) -> Result<Outcome, String> {
        match message {
            ServerMessage::Welcome {
                client,
                revision,
                missed,
                capabilities,
                participants,
                me,
                ..
            } => {
                if self.session.is_some() && self.log.unordered_commits() > 0 {
                    return Err(marked(
                        Refusal::CannotMerge.code(),
                        "Pending local work needs recovery before a fresh join.",
                    ));
                }
                let session = ClientSession::joined(&self.document, message, &mut self.log)
                    .map_err(|error| {
                        marked(
                            error.refusal().code(),
                            &format!("Joining the shared session failed: {error}."),
                        )
                    })?;
                let participant = session.client().get();
                self.session = Some(session);
                self.collab_adopt(participant, *capabilities)?;
                let mut replayed = 0;
                let mut tombstones = Vec::new();
                for arrival in missed {
                    let reception = self.collab_merge(arrival)?;
                    replayed += reception.1;
                    tombstones.extend(reception.0);
                }
                let relaid = if missed.is_empty() {
                    None
                } else {
                    Some(self.collab_relayout())
                };
                let mut outcome = Outcome::of("welcome");
                outcome.missed = Some(missed.len());
                outcome.replayed = Some(replayed);
                outcome.tombstones = tombstones;
                if let Some((dirty, pages)) = relaid {
                    outcome.document_changed = true;
                    outcome.dirty = dirty;
                    outcome.page_count = pages;
                }
                outcome.participant = Some(client.get());
                outcome.capabilities = Some(self.participant_capabilities());
                outcome.members = membership(participants);
                outcome.me = me.as_ref().map(JsProfile::from);
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
                participants,
                me,
                ..
            } => {
                let Some(session) = self.session.as_mut() else {
                    return Err(marked(
                        Refusal::Malformed.code(),
                        "A resume arrived for a shared session this tab never joined.",
                    ));
                };
                session.resumed(message).map_err(|error| {
                    marked(
                        error.refusal().code(),
                        &format!("Resuming the shared session failed: {error}."),
                    )
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
                outcome.members = membership(participants);
                outcome.me = me.as_ref().map(JsProfile::from);
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
                        marked(
                            error.refusal().code(),
                            &format!("The shared session refused an acknowledgement: {error}."),
                        )
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
                if self.session.is_none() && matches!(reason, Refusal::TooFarBehind { .. }) {
                    let mut outcome = Outcome::of("recoveryRequired");
                    outcome.code = Some(reason.code());
                    if let Refusal::TooFarBehind { current, .. } = reason {
                        outcome.revision = current.get();
                    }
                    return Ok(outcome);
                }
                let Some(session) = self.session.as_mut() else {
                    return Err(malformed_state("a refusal"));
                };
                session.refused(*seq, *reason, &self.log).map_err(|error| {
                    marked(
                        error.refusal().code(),
                        &format!("The shared session has stopped: {error}."),
                    )
                })?;
                let needs_recovery =
                    matches!(reason, Refusal::TooFarBehind { .. } | Refusal::CannotMerge);
                if needs_recovery {
                    session.stop(Refusal::CannotMerge);
                }
                let mut outcome = Outcome::of(if needs_recovery {
                    "recoveryRequired"
                } else {
                    "refused"
                });
                outcome.code = Some(reason.code());
                outcome.retryable = reason.is_retryable();
                outcome.terminal = !needs_recovery && reason.is_terminal();
                outcome.revision = match reason {
                    Refusal::TooFarBehind { current, .. } => current.get(),
                    _ => session.revision().get(),
                };
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
            // Who is present is the roster's (`WasmRoster`, which reads the same frame); the
            // document session only tells the host that the room changed, so it repaints.
            ServerMessage::ParticipantJoined(participant) => {
                let mut outcome = Outcome::of("participantJoined");
                outcome.about = Some(participant.client.get());
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
            ServerMessage::ParticipantLeft { client } => {
                let mut outcome = Outcome::of("participantLeft");
                outcome.about = Some(client.get());
                outcome.view_revision = self.revision;
                Ok(outcome)
            }
            ServerMessage::AccessChanged {
                client,
                capabilities,
            } => {
                // Applied to this replica only when it is **about** this replica, and through
                // `apply_room_access`, which is bounded by the host's ceiling and may move the
                // effective set either way inside it. The old `adopt` path could only narrow, so
                // a participant an owner had restored would have kept a disabled chrome and a
                // reason that was no longer true — see `apply_room_access`'s own note.
                //
                // A change about somebody ELSE touches nothing here. It is reported so the
                // chrome's rights surface stays live, and the engine has no opinion about what
                // another participant may do: the relay holds that and judges their submissions.
                let mine = self
                    .session
                    .as_ref()
                    .is_some_and(|session| session.client() == *client);
                if mine {
                    self.apply_room_access_internal(&capability_names(*capabilities))?;
                }
                let mut outcome = Outcome::of("accessChanged");
                outcome.about = Some(client.get());
                // The names this replica now holds when the change was its own, and the names
                // the relay announced when it was somebody else's. Both answer "what may that
                // participant do", which is the only question the field has; sending the
                // relay's word verbatim for ourselves would report a capability the intersection
                // may have just dropped.
                outcome.capabilities = Some(if mine {
                    self.participant_capabilities()
                } else {
                    capability_names(*capabilities)
                });
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
        match self.last_collab_map.as_mut() {
            Some((epoch, map)) if *epoch == u32::MAX => map.append(&reception.position_map),
            _ => self.last_collab_map = Some((u32::MAX, reception.position_map.clone())),
        }
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
    /// ignored: where the reader's caret went is `collabMapSelection`'s answer, read by the host
    /// from the composed caret map (the module header).
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
        if let Some((epoch, _)) = self.last_collab_map.as_mut() {
            *epoch = self.revision;
        }
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

/// The room's membership as the chrome reads it, from the wire's own [`Member`] values.
///
/// O(the members).
fn membership(members: &[Member]) -> Vec<Membership> {
    members
        .iter()
        .map(|member| Membership {
            participant: member.client.get(),
            capabilities: capability_names(member.capabilities),
            ceiling: capability_names(member.ceiling),
        })
        .collect()
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
    if granted.may_manage_access() {
        names.push("manageAccess".to_owned());
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

impl crate::presence::WasmRoster {
    pub(crate) fn receive_frame_inner(&mut self, frame: &[u8]) -> Result<(), String> {
        use casual_doc_transaction::protocol::ServerMessage;
        let message = casual_doc_transaction::codec::decode_frame::<ServerMessage>(frame)
            .map_err(|error| format!("presence: {error}"))?;
        match message {
            ServerMessage::Awareness { client, update } => {
                self.roster
                    .accept(client, update)
                    .map_err(|error| format!("presence: {error}"))?;
            }
            ServerMessage::Departed { client } => {
                self.roster.forget(client);
            }
            ServerMessage::Welcome {
                client,
                participants_present,
                ..
            }
            | ServerMessage::Resumed {
                client,
                participants_present,
                ..
            } => {
                self.clear();
                self.me = Some(client.get());
                self.present = participants_present
                    .iter()
                    .map(|participant| (participant.client.get(), JsParticipant::from(participant)))
                    .collect();
            }
            ServerMessage::ParticipantJoined(participant) => {
                self.present
                    .insert(participant.client.get(), JsParticipant::from(&participant));
            }
            ServerMessage::ParticipantLeft { client } => {
                self.present.remove(&client.get());
                self.roster.forget(client);
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn caret_frame_inner(
        &mut self,
        document: &crate::WasmDocument,
        anchor: &str,
        anchor_offset: u32,
        head: &str,
        head_offset: u32,
    ) -> Result<Option<Vec<u8>>, String> {
        use casual_doc_transaction::presence::{Caret, CaretEnd};
        use casual_doc_transaction::{Affinity, BlockTarget, Position};
        self.sync(document);
        let target = casual_doc_transaction::transform::BlockIndex::of(&document.document);
        let end = |node: &str, offset| {
            let pos = crate::parse_pos(node, offset)
                .ok_or_else(|| "invalid cursor position".to_owned())?;
            let container = target
                .block_position(pos.node)
                .map(|(container, _)| container)
                .unwrap_or(Some(document.document.id()));
            Ok::<_, String>(CaretEnd::new(
                Position::new(pos.node, offset, Affinity::After),
                container,
            ))
        };
        let caret = Caret {
            anchor: end(anchor, anchor_offset)?,
            head: end(head, head_offset)?,
        };
        let update = self
            .sender
            .send_caret(caret)
            .map_err(|error| format!("presence: {error}"))?;
        Ok(update.map(|update| {
            casual_doc_transaction::codec::encode_frame(
                &casual_doc_transaction::protocol::ClientMessage::Presence(update),
            )
        }))
    }
}

// Canonical marker first: plain normalized JSON remains a supported recovery input.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SnapshotPackage {
    collab_snapshot_version: u32,
    pub(crate) document: String,
    pub(crate) resources: BTreeMap<String, Vec<u8>>,
}

const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const MAX_SNAPSHOT_RESOURCES: usize = 32 * 1024 * 1024;

fn check_resources(resources: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    if resources.len() > 2048
        || resources.keys().any(|id| id.is_empty() || id.len() > 1024)
        || resources.values().map(Vec::len).sum::<usize>() > MAX_SNAPSHOT_RESOURCES
    {
        return Err("The shared snapshot resource limit was exceeded.".to_owned());
    }
    Ok(())
}

pub(crate) fn read_snapshot_package(bytes: &[u8]) -> Result<Option<SnapshotPackage>, String> {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    if !bytes[start..].starts_with(b"{\"collabSnapshotVersion\":") {
        return Ok(None);
    }
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("The shared snapshot exceeds 64 MiB.".to_owned());
    }
    let package: SnapshotPackage = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if package.collab_snapshot_version != 1 {
        return Err("Unsupported shared snapshot version.".to_owned());
    }
    check_resources(&package.resources)?;
    Ok(Some(package))
}

fn snapshot_bytes(
    document: &casual_doc_model::v1::Document,
    resources: &DocumentResources,
) -> Result<Vec<u8>, String> {
    check_resources(resources.as_map())?;
    let document = String::from_utf8(document.to_json().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let package = SnapshotPackage {
        collab_snapshot_version: 1,
        document,
        resources: resources.as_map().clone(),
    };
    let bytes = serde_json::to_vec(&package).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("The shared snapshot exceeds 64 MiB.".to_owned());
    }
    Ok(bytes)
}
