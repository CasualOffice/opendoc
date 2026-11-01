// SPDX-License-Identifier: Apache-2.0

//! The roster, across the JS boundary — who else is in the room and where they
//! are looking (`docs/152` §2b, `107` 6.6).
//!
//! `casual_doc_transaction::presence::Roster` has been built, bounded and guarded
//! since 2026-10-01 and **had no reader at all**: doc 152 §2b says so in as many
//! words, and `SKILL` §9.4 names "built and unreachable" as the most expensive
//! recurring pattern in this repository. This module is the reader. It adds no
//! rule and no second implementation — every acceptance decision, every bound and
//! the staleness rule are the engine's, taken verbatim.
//!
//! # Why a handle of its own, and not a field on `WasmDocument`
//!
//! Presence is **not** document state, and the separation is structural rather
//! than tidy:
//!
//! - it is never persisted and never replayed, which
//!   `presence_is_never_written_to_the_revision_log` holds in the engine. A field
//!   on the document handle would put it one careless line away from a commit;
//! - it dies with the connection (ONLYOFFICE's `Remove_ForeignCursor` fires on a
//!   connection-state change, and for good reason: a caret that outlives its
//!   owner tells the reader somebody is there who is not). Dropping a handle is
//!   how that is expressed;
//! - a disconnected document constructs no roster. Connected editors map only
//!   the bounded participant roster through each edit's position map.
//!
//! A standalone document in **mode 1** — opened from a file, no link, no host
//! embedding it — never constructs one of these and needs nothing from this
//! module. That is the local-first guarantee, unchanged.
//!
//! # The identity rule, and where it is kept
//!
//! A participant number is **assigned by the relay and attached by the
//! receiver**, never read out of the message: `PresenceUpdate` carries a clock
//! and an opaque payload and has no identity field for a client to write a claim
//! into. This boundary preserves that shape exactly — [`WasmRoster::presence_update`]
//! takes the participant number as a *separate argument*, which the host fills
//! from the session the frame arrived on, not from the frame's contents.
//!
//! # Typed caret payloads and geometry
//!
//! The raw opaque-payload API remains lossless. Typed carets use stable node IDs,
//! offsets and affinity; the session facade encodes frames and the engine maps
//! anchors through each edit. Geometry comes from the document layout. Presence
//! never mutates a document or enters its transaction log.
//!
//! Updates cost O(log participants); mapping and geometry visit a bounded roster.
//! Payload size and participant count use the engine's admission limits.

use casual_doc_transaction::presence::{
    Accepted, MAX_PARTICIPANTS, MAX_PRESENCE_BYTES, PresenceClock, PresenceUpdate, Roster,
};
// `ClientId` is a newtype over the participant number the relay assigns — data,
// with no behaviour and no connection to the ordering machinery. Its own `use`
// line, where the pinned rustfmt sorts it, so a parallel lane adding an import
// does not conflict here.
use casual_doc_transaction::protocol::ClientId;
use wasm_bindgen::prelude::*;

use crate::to_js;

/// The most bytes one participant's presence payload may carry, for a host that
/// would otherwise hardcode it.
///
/// Published rather than written into the chrome because the engine owns the
/// bound: a number typed into a JS file is a number with no committed artifact
/// behind it (`105` EV-002). A host that composes a payload checks it against
/// this before sending, so an oversized caret is caught where it can be fixed
/// instead of refused at the far end.
#[wasm_bindgen(js_name = maxPresenceBytes)]
#[must_use]
pub fn max_presence_bytes() -> u32 {
    u32::try_from(MAX_PRESENCE_BYTES).unwrap_or(u32::MAX)
}

/// The most participants one room's roster will hold, for the same reason.
///
/// Past this a participant is refused rather than the roster growing without
/// limit: the roster is fanned out to everyone, so the cost of the *n*-th
/// participant is paid *n* times.
#[wasm_bindgen(js_name = maxParticipants)]
#[must_use]
pub fn max_participants() -> u32 {
    u32::try_from(MAX_PARTICIPANTS).unwrap_or(u32::MAX)
}

/// Everyone in the room, and where they are looking.
///
/// Created on joining a room and **dropped when the connection ends** — presence
/// does not outlive its transport. Holds no document, issues no `Operation`, and
/// reaches no revision log.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct WasmRoster {
    pub(crate) roster: Roster,
    pub(crate) sender: casual_doc_transaction::presence::PresenceSender,
    mapped_head: Option<casual_doc_transaction::RevisionId>,
    mapped_epoch: Option<u32>,
    /// Who is present and what they look like (`docs/175` §4.3): the `Welcome` list, then the
    /// joins and leaves the relay announces. Dies with the handle, like the carets.
    /// Already in the chrome's shape: the facade (`collab.rs`) converts the wire's rows, so
    /// this module never names the protocol beyond `ClientId`.
    pub(crate) present: std::collections::BTreeMap<u64, crate::collab::JsParticipant>,
    /// This replica's own participant number, from its `Welcome` or `Resumed`.
    pub(crate) me: Option<u64>,
}

/// An empty roster, for a host that has just joined a room.
///
/// A document in mode 1 — a file opened with no link and no embedding host — does
/// not call this and needs nothing from it.
#[wasm_bindgen(js_name = newRoster)]
#[must_use]
pub fn new_roster() -> WasmRoster {
    WasmRoster::default()
}

#[wasm_bindgen]
impl WasmRoster {
    /// Clears transient membership and resends the current position after reconnect.
    pub fn clear(&mut self) {
        self.roster = Roster::new();
        self.present.clear();
        self.sender.rejoined();
    }

    /// Reads awareness and departures through the engine codec. No browser wire parsing.
    #[wasm_bindgen(js_name = receiveFrame)]
    pub fn receive_frame(&mut self, frame: &[u8]) -> Result<(), JsValue> {
        self.receive_frame_inner(frame).map_err(to_js)
    }

    /// Builds a typed, bounded caret frame. Unchanged positions produce no frame.
    #[wasm_bindgen(js_name = caretFrame)]
    pub fn caret_frame(
        &mut self,
        document: &crate::WasmDocument,
        anchor: &str,
        anchor_offset: u32,
        head: &str,
        head_offset: u32,
    ) -> Result<Option<Vec<u8>>, JsValue> {
        self.caret_frame_inner(document, anchor, anchor_offset, head, head_offset)
            .map_err(to_js)
    }

    /// `caretFrame` while this participant's assistant is writing at `agent_node` /
    /// `agent_offset` (`docs/178` §10): every other replica draws a "Casual AI" caret
    /// there. An empty `agent_node` is no assistant, exactly `caretFrame`.
    #[wasm_bindgen(js_name = caretFrameWithAgent)]
    #[allow(clippy::too_many_arguments)]
    pub fn caret_frame_with_agent(
        &mut self,
        document: &crate::WasmDocument,
        anchor: &str,
        anchor_offset: u32,
        head: &str,
        head_offset: u32,
        agent_node: &str,
        agent_offset: u32,
    ) -> Result<Option<Vec<u8>>, JsValue> {
        let agent = (!agent_node.is_empty()).then_some((agent_node, agent_offset));
        self.caret_frame_with_agent_inner(document, anchor, anchor_offset, head, head_offset, agent)
            .map_err(to_js)
    }

    /// Maps the roster across edits and returns geometry from the document engine.
    /// A participant whose assistant is writing also carries `agent: {caret}`, the AI
    /// caret's rectangle (`docs/178` §10).
    #[wasm_bindgen(js_name = remoteSelections)]
    pub fn remote_selections(&mut self, document: &crate::WasmDocument) -> String {
        self.sync(document);
        let rows: Vec<_> = self
            .roster
            .participants()
            .filter_map(|held| {
                let caret = casual_doc_transaction::presence::Caret::decode(&held.payload).ok()?;
                let anchor = caret.anchor.position;
                let head = caret.head.position;
                let rect = document.caret_rect(&head.node.to_string(), head.offset);
                if rect.is_empty() {
                    return None;
                }
                let mut row = serde_json::json!({
                    "client": held.client.get(), "caret": rect,
                    "selection": document.selection_rects(&anchor.node.to_string(), anchor.offset,
                        &head.node.to_string(), head.offset)
                });
                if let Some(agent) = caret.agent {
                    let at = agent.position;
                    let rect = document.caret_rect(&at.node.to_string(), at.offset);
                    if !rect.is_empty() {
                        row["agent"] = serde_json::json!({ "caret": rect });
                    }
                }
                Some(row)
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_owned())
    }

    /// Records `client`'s presence, replacing whatever that client had.
    ///
    /// `client` is the participant number **the relay assigned**, which the host
    /// takes from the session the frame arrived on and never from the frame's
    /// contents — there is no identity field in a presence frame for a sender to
    /// put a claim in, and this argument is where the receiver supplies the
    /// answer instead.
    ///
    /// `clock` is the sender's own monotonic count of the updates it has sent. It
    /// is not a document revision and is not ordered against edits: presence
    /// merges with nothing, so it needs no position in the total order. An update
    /// whose clock is not newer than the one already held is **ignored** and
    /// reported as `"stale"` — the defect ONLYOFFICE guards with
    /// `participantsTimestamp`, where a roster arriving out of order moves a caret
    /// backwards and leaves it there.
    ///
    /// Returns what happened, so a host repaints only when something moved:
    /// `"joined"`, `"moved"` or `"stale"`.
    ///
    /// Complexity: O(log participants). Nothing about the document is read.
    ///
    /// # Errors
    ///
    /// Throws when the payload is above [`max_presence_bytes`], and when a **new**
    /// participant would take the room past [`max_participants`]. Neither changes
    /// the roster, and a participant already in a full room may still move — the
    /// cap is on membership, not on movement, or every caret in a busy room would
    /// freeze.
    #[wasm_bindgen(js_name = presenceUpdate)]
    pub fn presence_update(
        &mut self,
        client: u64,
        clock: u64,
        payload: Vec<u8>,
    ) -> Result<String, JsValue> {
        self.presence_update_inner(client, clock, payload)
            .map_err(to_js)
    }

    /// Forgets `client` — it left, or its connection dropped. Returns whether
    /// anything was held.
    ///
    /// A host calls this on a leave frame **and** on a connection-state change,
    /// which is what ONLYOFFICE's `Remove_ForeignCursor` does: a caret that
    /// outlives its owner is worse than no caret, because the reader believes
    /// somebody is there.
    pub fn forget(&mut self, client: u64) -> bool {
        self.roster.forget(ClientId::new(client))
    }

    /// Everyone held, in participant-number order, as JSON:
    /// `[{"client":n,"clock":n,"payloadBytes":n}]`.
    ///
    /// Participant-number order, not map-iteration order, so two replicas list
    /// the room identically and a reader's sidebar does not reshuffle itself on
    /// every update.
    ///
    /// The payload itself is **not** in this JSON, and that is deliberate: it is
    /// opaque bytes, and the only lossless way to put arbitrary bytes in JSON is
    /// to invent an encoding at the boundary. `payloadBytes` is its length, so a
    /// host can render a participant chip without touching the payload at all,
    /// and [`payload_of`](Self::payload_of) hands back the bytes unchanged for the
    /// ones it wants to draw.
    ///
    /// Complexity: O(participants), capped at [`max_participants`].
    ///
    /// Since `docs/175` §4.3 the list is **everyone present**, not only everyone who has sent a
    /// caret: the relay's participant list joins the caret roster, and each row carries
    /// `profile` (`{id, name, initials?, color?, avatarUrl?}`, when anybody named the person)
    /// and `attested` (whether the host vouched for it). A participant present but caretless has
    /// no `clock` or `payloadBytes`. This replica is included; [`me`](Self::me) says which row
    /// it is.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn participants(&self) -> String {
        let mut rows: std::collections::BTreeMap<u64, serde_json::Value> = self
            .present
            .iter()
            .map(|(client, participant)| {
                (
                    *client,
                    serde_json::to_value(participant).unwrap_or(serde_json::Value::Null),
                )
            })
            .collect();
        for held in self.roster.participants() {
            let row = rows.entry(held.client.get()).or_insert_with(
                || serde_json::json!({ "client": held.client.get(), "attested": false }),
            );
            if let Some(row) = row.as_object_mut() {
                row.insert("clock".to_owned(), held.clock.get().into());
                row.insert("payloadBytes".to_owned(), held.payload.len().into());
            }
        }
        let rows: Vec<_> = rows.into_values().collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_owned())
    }

    /// This replica's own row of [`participants`](Self::participants) as JSON
    /// (`{client, profile?, attested}`), or `null` before the room has welcomed it.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn me(&self) -> String {
        self.me
            .and_then(|client| self.present.get(&client))
            .and_then(|participant| serde_json::to_string(participant).ok())
            .unwrap_or_else(|| "null".to_owned())
    }

    /// One participant's payload, exactly as it arrived, or `undefined` if that
    /// participant is not held.
    ///
    /// Bytes, not text: the engine never interprets the payload and this boundary
    /// does not either. A host that put UTF-8 JSON in decodes it itself and knows
    /// what it put there.
    #[wasm_bindgen(js_name = payloadOf)]
    #[must_use]
    pub fn payload_of(&self, client: u64) -> Option<Vec<u8>> {
        self.roster
            .get(ClientId::new(client))
            .map(|held| held.payload.clone())
    }

    /// How many participants are held.
    #[wasm_bindgen(getter, js_name = participantCount)]
    #[must_use]
    pub fn participant_count(&self) -> u32 {
        u32::try_from(self.roster.len()).unwrap_or(u32::MAX)
    }

    /// Whether nobody is held — a room this replica is alone in, which under
    /// "one doc, one room" is the ordinary state of a shared document on its
    /// first open rather than an error.
    #[wasm_bindgen(getter, js_name = isEmpty)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.roster.is_empty()
    }
}

impl WasmRoster {
    pub(crate) fn sync(&mut self, document: &crate::WasmDocument) {
        if self.mapped_epoch == Some(document.revision) {
            return;
        }
        if let Some(previous) = self.mapped_head {
            let target = casual_doc_transaction::transform::BlockIndex::of(&document.document);
            if let Some((epoch, map)) = &document.last_collab_map {
                if *epoch == document.revision {
                    self.roster.rebase(map, &target);
                } else {
                    for commit in document.log.commits().filter(|c| c.revision() > previous) {
                        self.roster.rebase(commit.position_map(), &target);
                    }
                }
            } else {
                for commit in document.log.commits().filter(|c| c.revision() > previous) {
                    self.roster.rebase(commit.position_map(), &target);
                }
            }
        }
        self.mapped_head = Some(document.log.head());
        self.mapped_epoch = Some(document.revision);
    }

    /// See [`WasmRoster::presence_update`]. Plain `Result<_, String>` so the
    /// guards run under `cargo test` on native targets, where constructing a
    /// `JsValue` panics.
    pub(crate) fn presence_update_inner(
        &mut self,
        client: u64,
        clock: u64,
        payload: Vec<u8>,
    ) -> Result<String, String> {
        let accepted = self
            .roster
            .accept(
                ClientId::new(client),
                PresenceUpdate {
                    clock: PresenceClock::new(clock),
                    payload,
                },
            )
            .map_err(|error| format!("presence: {error}"))?;
        Ok(match accepted {
            Accepted::Joined => "joined",
            Accepted::Moved => "moved",
            Accepted::Stale => "stale",
        }
        .to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> WasmRoster {
        new_roster()
    }

    /// The reader exists and reports the three outcomes a host repaints on. Until
    /// this module landed, `Roster` had no caller outside its own crate at all.
    #[test]
    fn the_roster_reports_joins_moves_and_staleness() {
        let mut room = roster();
        assert!(room.is_empty());
        assert_eq!(
            room.presence_update_inner(7, 1, b"caret-a".to_vec()),
            Ok("joined".to_owned())
        );
        assert_eq!(
            room.presence_update_inner(7, 2, b"caret-b".to_vec()),
            Ok("moved".to_owned())
        );
        assert_eq!(room.participant_count(), 1);
        assert_eq!(room.payload_of(7), Some(b"caret-b".to_vec()));
        assert!(room.forget(7));
        assert!(!room.forget(7));
        assert!(room.is_empty());
        assert_eq!(room.payload_of(7), None);
    }

    /// The out-of-order defect ONLYOFFICE guards with `participantsTimestamp`: a
    /// roster arriving late must not move a caret backwards. The clock decides,
    /// and the stale frame leaves the held payload alone.
    ///
    /// Mutation that drove it red: `presence_update_inner` ignoring the clock and
    /// bumping it to `held.clock + 1` before handing it to the engine, so every
    /// arrival looked newer.
    #[test]
    fn a_late_presence_frame_does_not_move_a_caret_backwards() {
        let mut room = roster();
        room.presence_update_inner(1, 5, b"newest".to_vec())
            .expect("the first frame joins");
        assert_eq!(
            room.presence_update_inner(1, 4, b"older".to_vec()),
            Ok("stale".to_owned()),
            "a frame behind the one held is ignored"
        );
        assert_eq!(
            room.presence_update_inner(1, 5, b"same-clock".to_vec()),
            Ok("stale".to_owned()),
            "a frame AT the clock held says nothing new"
        );
        assert_eq!(
            room.payload_of(1),
            Some(b"newest".to_vec()),
            "the held payload survives both"
        );
    }

    /// Both bounds are refusals a host can show, and the membership cap does not
    /// freeze someone already in the room.
    #[test]
    fn the_two_bounds_are_refusals_and_movement_still_works_in_a_full_room() {
        let mut room = roster();
        let oversized = vec![0u8; MAX_PRESENCE_BYTES + 1];
        let refused = room
            .presence_update_inner(1, 1, oversized)
            .expect_err("an oversized payload is refused");
        assert!(refused.starts_with("presence: "), "{refused}");
        assert!(refused.contains("above"), "{refused}");
        assert!(room.is_empty(), "a refusal changes nothing");

        for number in 0..u64::try_from(MAX_PARTICIPANTS).unwrap() {
            room.presence_update_inner(number, 1, b"x".to_vec())
                .expect("the room fills to its cap");
        }
        assert_eq!(room.participant_count(), max_participants());
        let full = room
            .presence_update_inner(9_999, 1, b"x".to_vec())
            .expect_err("a new participant past the cap is refused");
        assert!(full.contains("participants"), "{full}");
        assert_eq!(
            room.presence_update_inner(0, 2, b"y".to_vec()),
            Ok("moved".to_owned()),
            "the cap is on membership, not on movement"
        );
    }

    /// The roster is listed in participant-number order so two replicas render the
    /// room identically, and the JSON carries the length of the opaque payload
    /// rather than an encoding invented at the boundary.
    #[test]
    fn the_roster_json_is_ordered_and_carries_no_invented_encoding() {
        let mut room = roster();
        for number in [9u64, 1, 5, 3] {
            room.presence_update_inner(number, 1, b"ab".to_vec())
                .expect("joins");
        }
        let rows: serde_json::Value =
            serde_json::from_str(&room.participants()).expect("the roster is JSON");
        let rows = rows.as_array().expect("an array").clone();
        let order: Vec<i64> = rows
            .iter()
            .map(|row| row["client"].as_i64().expect("a client number"))
            .collect();
        assert_eq!(order, vec![1, 3, 5, 9]);
        for row in &rows {
            let mut keys: Vec<&str> = row
                .as_object()
                .expect("an object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            // `attested` joined the row with the participant list (`docs/175` §4.3): a
            // participant known only by their caret was named by nobody, so it is `false`, and
            // there is no `profile` to invent.
            assert_eq!(keys, ["attested", "client", "clock", "payloadBytes"]);
            assert_eq!(row["attested"], serde_json::Value::Bool(false));
            assert_eq!(row["payloadBytes"].as_i64(), Some(2));
        }
    }

    /// **Presence is not document state**, and the facade must keep it that way.
    ///
    /// The engine holds its half structurally:
    /// `presence_is_never_written_to_the_revision_log` fails the build if the
    /// revision log or the session envelope so much as names this module. That scan
    /// cannot see the facade, and the facade is where the mistake is easy — a
    /// `Roster` field on the document handle would sit one careless line away from
    /// a commit, would be walked by anything on the editing path, and would outlive
    /// the connection it belongs to.
    ///
    /// So this scans `WasmDocument`'s own declaration. A source scan is the only way
    /// to assert the ABSENCE of a field: a constructor listing every field still
    /// compiles when another one is added.
    #[test]
    fn the_document_handle_holds_no_roster() {
        let source = include_str!("lib.rs").replace("\r\n", "\n");
        let declaration = source
            .split_once("pub struct WasmDocument {")
            .and_then(|(_, rest)| rest.split_once("\n}\n"))
            .map(|(body, _)| body.to_owned())
            .expect("`WasmDocument` is declared in lib.rs");
        for forbidden in ["Roster", "Presence", "presence::"] {
            assert!(
                !declaration.contains(forbidden),
                "`WasmDocument` names `{forbidden}`: presence is never persisted, never \
                 replayed and dies with its connection, so it must not be document state"
            );
        }
        // The guard is only worth having if it can see the thing it forbids.
        let planted = "    roster: Roster,\n";
        assert!(
            ["Roster", "Presence", "presence::"]
                .iter()
                .any(|forbidden| planted.contains(forbidden)),
            "the scan cannot see a field it is supposed to forbid"
        );
    }

    /// A payload crosses the boundary **unchanged**, including bytes that are not
    /// valid UTF-8 — the boundary must not quietly turn an opaque payload into
    /// text it can print.
    #[test]
    fn an_opaque_payload_crosses_the_boundary_byte_for_byte() {
        let mut room = roster();
        let payload = vec![0x00, 0xff, 0xfe, 0x80, b'{', 0x7f];
        room.presence_update_inner(2, 1, payload.clone())
            .expect("joins");
        assert_eq!(room.payload_of(2), Some(payload));
    }
}
