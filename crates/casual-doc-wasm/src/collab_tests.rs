// SPDX-License-Identifier: Apache-2.0

//! The session facade's guards, driven natively.
//!
//! Every one of them asserts a guarantee the browser's transport depends on and cannot check
//! for itself: that a grant arriving on the wire **narrows**, that a frame this build cannot
//! read is refused with `ODC-7007` rather than ignored, that a message arriving for a session
//! that was never joined does not reach the document, and that the facade never applies an
//! operation outside the session.

use casual_doc_edit::access::Capabilities;
use casual_doc_transaction::codec::encode_frame;
use casual_doc_transaction::protocol::{
    ClientId, ClientMessage, PROTOCOL_VERSION, Refusal, Revision, ServerMessage,
};

// Its own line, for the parallel-lane import rule.
use casual_doc_transaction::protocol::{Participant, Profile};

use super::{capability_names, snapshot_bytes};

/// A one-paragraph document through the real open path, so the facade under test is the one the
/// browser gets.
fn document() -> crate::WasmDocument {
    crate::open_document(b"abcdefgh").expect("a document opens from plain text")
}

/// The body's text, so a guard can assert a refusal left the document alone.
fn text(document: &crate::WasmDocument) -> String {
    use casual_doc_model::v1::{BlockNode, InlineNode};
    let mut out = String::new();
    for block in document.document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    out.push_str(&run.text);
                }
            }
            out.push('\n');
        }
    }
    out
}

fn welcome(client: u64, capabilities: Capabilities) -> Vec<u8> {
    encode_frame(&ServerMessage::Welcome {
        me: None,
        participants_present: Vec::new(),
        base_revision: None,
        missed: Vec::new(),
        protocol: PROTOCOL_VERSION,
        client: ClientId::new(client),
        revision: Revision::new(0),
        capabilities,
        participants: Vec::new(),
    })
}

/// **A grant arriving on the wire narrows what this replica may do, and cannot widen it.**
///
/// The property a previous lane established for the URL grant — `adopt_participant_capabilities`
/// intersects, so a client-supplied value can only take access away — carried onto the wire,
/// which is where the grant becomes authoritative. Both directions are asserted, because only
/// one of them is a security property and the other is the one that silently stops working:
///
/// 1. a `Welcome` carrying `viewer` leaves a standalone replica (which holds everything) a
///    viewer;
/// 2. a *second* `Welcome` carrying `owner` does **not** restore what the first took away.
#[test]
fn a_grant_arriving_on_the_wire_narrows_and_cannot_widen() {
    let mut doc = document();
    assert!(
        doc.participant_capabilities().contains(&"edit".to_owned()),
        "a document with no room holds everything — that is the standalone mode"
    );

    doc.collab_receive_frame_inner(&welcome(0, Capabilities::commenter()))
        .expect("the welcome is adopted");
    assert_eq!(
        doc.participant_capabilities(),
        vec!["comment".to_owned()],
        "the capabilities the relay sent must be what this replica holds"
    );

    // The widening attempt. A relay that sent this — or anything on the wire pretending to be
    // one — must not be able to restore a right the first message took away.
    doc.collab_receive_frame_inner(&welcome(0, Capabilities::owner()))
        .expect("a second welcome is adopted");
    assert_eq!(
        doc.participant_capabilities(),
        vec!["comment".to_owned()],
        "a grant arriving on the wire WIDENED this replica's access, so the wire is a way to \
         escalate and the intersection has become a replacement"
    );
}

/// **The capability names sent to the engine are the names it reports back.**
///
/// Two translations of one set — `capability_names` going in and `participantCapabilities`
/// coming out — and a host that cannot compare them cannot tell a narrowed grant from a
/// misspelt one. Driven over every preset rather than one, because the presets are what a room
/// is actually configured with.
#[test]
fn every_capability_preset_survives_the_round_trip() {
    for preset in [
        Capabilities::viewer(),
        Capabilities::commenter(),
        Capabilities::suggester(),
        Capabilities::editor(),
        Capabilities::owner(),
    ] {
        let mut doc = document();
        doc.collab_receive_frame_inner(&welcome(0, preset))
            .expect("the welcome is adopted");
        assert_eq!(
            doc.participant_capabilities(),
            capability_names(preset),
            "a preset went in and something else came out, so a host comparing the two sees a \
             narrowing that did not happen"
        );
    }
}

/// **A frame this build cannot read is refused with `ODC-7007`, and the document is untouched.**
///
/// One of the three refusals `protocol::Refusal` says must never be collapsed: `Malformed`
/// means *do not send that again*, and the sibling engine lost a live debugging session to
/// answering an unparseable message with `CannotMerge` — naming the transform, the one part
/// that was working.
#[test]
fn a_frame_that_cannot_be_read_is_refused_by_name() {
    let mut doc = document();
    let before = text(&doc);
    let error = doc
        .collab_receive_frame_inner(b"not a frame at all")
        .expect_err("a frame the codec refuses must not be accepted");
    let (sentence, code) = casual_doc_edit::refusal::split(&error);
    assert_eq!(code, Some(Refusal::Malformed.code()));
    assert!(
        sentence.starts_with("refused: "),
        "the reader's half must be a sentence: {sentence:?}"
    );
    assert_eq!(text(&doc), before);
    assert!(
        !doc.collab_state().contains("\"joined\":true"),
        "an unreadable frame must not establish a session"
    );
}

/// **A message for a session this tab never joined does not reach the document.**
///
/// The engine's `OutOfOrder` rule, at the boundary: an `Ack` or an `Apply` before a `Welcome`
/// cannot be from a relay this replica is talking to, and applying one would be applying an
/// edit with no ordered position behind it.
#[test]
fn a_message_before_the_join_is_refused() {
    for frame in [
        encode_frame(&ServerMessage::Ack {
            through: casual_doc_transaction::protocol::Seq::new(1),
            revision: Revision::new(1),
        }),
        encode_frame(&ServerMessage::Apply(
            casual_doc_transaction::protocol::Arrival {
                author: None,
                revision: Revision::new(1),
                client: ClientId::new(1),
                operations: Vec::new(),
            },
        )),
    ] {
        let mut doc = document();
        let before = text(&doc);
        let error = doc
            .collab_receive_frame_inner(&frame)
            .expect_err("a message before the join must be refused");
        assert_eq!(
            casual_doc_edit::refusal::split(&error).1,
            Some(Refusal::Malformed.code())
        );
        assert_eq!(text(&doc), before);
    }
}

/// **A `Stopped` is recorded even on a session that never established.**
///
/// The two refusals that arrive this way — a protocol mismatch and a full room — are exactly
/// the cases where there is no session to record them on and the reader most needs the reason.
/// A transport that got nothing back from these would have to invent a sentence.
#[test]
fn a_stop_before_the_join_still_names_its_reason() {
    for (reason, code) in [
        (
            Refusal::ProtocolVersion {
                server: PROTOCOL_VERSION,
                client: PROTOCOL_VERSION + 1,
            },
            "ODC-7002",
        ),
        (Refusal::RoomFull { limit: 128 }, "ODC-7010"),
        (Refusal::NotAuthorised, "ODC-7003"),
    ] {
        let mut doc = document();
        let outcome = doc
            .collab_receive_frame_inner(&encode_frame(&ServerMessage::Stopped { reason }))
            .expect("a stop is an outcome, not an error");
        assert!(
            outcome.contains(&format!("\"code\":\"{code}\"")),
            "a stop must name its reason so the reader is told why: {outcome}"
        );
        assert!(outcome.contains("\"terminal\":true"));
    }
}

/// **The opening frame always carries the resume key, and refuses the inputs it cannot use.**
///
/// A key is recorded only by the join that presented it, so a client that withheld it on its
/// first connection can never be recognised on its second — it is handed a fresh `Welcome`, a
/// new participant number, and whatever it had not had acknowledged is gone with no refusal
/// naming the loss. The frame is therefore decoded back here and the key asserted present.
#[test]
fn the_opening_frame_carries_the_key_and_refuses_what_it_cannot_use() {
    let doc = document();
    let frame = doc
        .collab_join_frame_inner("ada", "tab-key", 0.0, None, None)
        .expect("a join frame");
    let message: ClientMessage =
        casual_doc_transaction::codec::decode_frame(&frame).expect("it decodes");
    let ClientMessage::Join(join) = message else {
        panic!("the opening frame must be a Join");
    };
    assert_eq!(join.protocol, PROTOCOL_VERSION);
    assert_eq!(join.identity.as_str(), "ada");
    assert!(
        join.grant.is_none(),
        "this engine never invents a grant — ADR-060, the host signs and the boundary verifies"
    );
    let resume = join.resume.expect(
        "the key must travel on the FIRST join too, or this tab can never be resumed and the \
         loss arrives with no refusal",
    );
    assert_eq!(resume.key.as_str(), "tab-key");
    assert_eq!(resume.revision, Revision::new(0));

    // And the inputs it refuses rather than guesses at.
    for (identity, key, revision) in [
        ("", "tab-key", 0.0),
        ("ada", "", 0.0),
        ("ada", "tab-key", -1.0),
        ("ada", "tab-key", f64::NAN),
    ] {
        let error = doc
            .collab_join_frame_inner(identity, key, revision, None, None)
            .expect_err("an unusable input must be refused rather than guessed at");
        assert!(
            error.starts_with("refused: "),
            "a refusal must be a sentence a reader can act on: {error:?}"
        );
    }
}

/// The host's grant and the claimed profile travel on the join as given (`docs/175` §3.3), and
/// a profile out of bounds is refused naming its field rather than trimmed.
#[test]
fn the_opening_frame_carries_the_hosts_grant_and_profile() {
    let doc = document();
    let frame = doc
        .collab_join_frame_inner(
            "u_ada",
            "tab-key",
            0.0,
            Some("header.payload.signature"),
            Some(r#"{"id":"u_ada","name":"Ada Lovelace","avatarUrl":"https://cdn.example/a.png"}"#),
        )
        .expect("a join frame");
    let ClientMessage::Join(join) =
        casual_doc_transaction::codec::decode_frame(&frame).expect("it decodes")
    else {
        panic!("a join");
    };
    assert_eq!(
        join.grant.as_ref().map(|grant| grant.as_bytes().to_vec()),
        Some(b"header.payload.signature".to_vec())
    );
    let profile = join.profile.expect("the claimed profile");
    assert_eq!(profile.name, "Ada Lovelace");
    assert_eq!(
        profile.avatar_url.as_deref(),
        Some("https://cdn.example/a.png")
    );

    for (bad, field) in [
        (r#"{"id":"u_ada","name":""}"#, "name"),
        (
            r#"{"id":"u_ada","name":"Ada","avatarUrl":"http://x/a.png"}"#,
            "avatar.url",
        ),
        (r#"{"id":"u_ada","name":"Ada","color":"blue"}"#, "color"),
        (r#"{"name":"Ada"}"#, "id"),
    ] {
        let error = doc
            .collab_join_frame_inner("u_ada", "tab-key", 0.0, None, Some(bad))
            .expect_err("a profile out of bounds is refused");
        assert!(error.contains("identity-invalid"), "{error}");
        if field != "id" {
            assert!(
                error.contains(field),
                "the refusal names `{field}`: {error}"
            );
        }
    }
}

/// **The facade applies nothing outside the session.**
///
/// `every_document_mutation_is_a_transaction` scans `lib.rs` and holds the choke point there.
/// This module is a second file that can reach the document, so the same rule is held for it
/// rather than left to review: an arrival is applied by `ClientSession::receive`, which goes
/// down `RevisionLog::apply` inside the engine crate, and a direct operation here would be an
/// edit the revision chain never sees.
///
/// The scan looks at **code**, not at prose. The first draft of this pair scanned raw source
/// and the facade's own header quoted `.apply` with its arguments while explaining why the
/// choke point is not bypassed — so the guard reddened on the paragraph that documents the
/// rule, which is a guard pinned to its circumstance rather than to its guarantee. `lib.rs`
/// avoids that by convention (its prose writes `casual_doc_edit::apply` without a
/// parenthesis), and a convention an author has to remember is one that breaks; this one is
/// mechanical instead. [`facade_code`] drops only lines that are a comment in their
/// ENTIRETY, so it can never remove a line that carries code, and the positive controls at
/// the bottom assert both halves of that.
#[test]
fn the_collab_facade_applies_nothing_outside_the_session() {
    let source = facade_code(include_str!("collab.rs"));
    let source = source.as_str();
    for forbidden in [
        "casual_doc_edit::apply(",
        "apply_edit(",
        ".apply(&mut self.document",
        "Transaction::reserve(",
        "definitions_mut(",
        "body_mut(",
    ] {
        assert!(
            !source.contains(forbidden),
            "`{forbidden}` appears in the session facade: a mutation that does not go through \
             `ClientSession::receive` is an edit with no ordered position behind it"
        );
    }
    // The one call that is allowed to change the document, present exactly once — so a second
    // arrival path cannot be added without this guard noticing.
    assert_eq!(
        source.matches(".receive(").count(),
        1,
        "an arrival must be merged in exactly one place; a second would be a second rebase"
    );
    // And the scan can see what it forbids, so it cannot pass by failing to look. Three
    // controls, because the stripper is now part of what has to be trusted: it keeps code,
    // it drops a whole-line comment, and it does not take the code beside a trailing one.
    assert!(
        facade_code("let x = self.document.definitions_mut();").contains("definitions_mut("),
        "the scan must keep a line of code that carries what it forbids"
    );
    assert!(
        !facade_code("//! `.apply(&mut self.document, transaction)` is the forbidden call")
            .contains(".apply(&mut self.document"),
        "a whole-line comment must be dropped, or prose quoting the rule reddens this guard"
    );
    assert!(
        facade_code("self.document.body_mut(); // not body_mut( really").contains("body_mut();"),
        "a trailing comment must not take the code beside it out of the scan"
    );
}

/// The facade's source with its whole-line comments removed, so the choke-point scan reads
/// code and not the paragraph that explains the rule.
///
/// Conservative by construction, and that is the entire design: a line is dropped **only**
/// when it is a comment in its entirety, so no line carrying any code can be removed and a
/// trailing `//` cannot be used to blind the scan. The three shapes this matters for — `//!`,
/// `///` and a standalone `//` — are all whole-line, which is to say they are prose.
fn facade_code(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn recovery_replays_pending_text_and_preserves_changed_paragraph_backups() {
    use casual_doc_edit::{Operation, Pos};
    let mut doc = document();
    doc.collab_receive_frame_inner(&welcome(0, Capabilities::owner()))
        .expect("join");
    let node = match &doc.document.body()[0] {
        casual_doc_model::v1::BlockNode::Paragraph(p) => p.id,
        _ => panic!("paragraph"),
    };
    let original = doc
        .export_as_inner(casual_doc_io::formats::NORMALIZED_JSON, "semantic")
        .expect("export")
        .bytes;
    doc.apply(Operation::InsertText {
        at: Pos::new(node, 2),
        text: "LOCAL".to_owned(),
    })
    .expect("edit");
    let sent = doc.collab_next_chunk().expect("a pending chunk");
    let ClientMessage::Submit(first) =
        casual_doc_transaction::codec::decode_frame(&sent).expect("submission")
    else {
        panic!("submit");
    };
    let checkpoint = doc.collab_checkpoint().expect("checkpoint");
    let acknowledged = crate::open_document(&checkpoint).expect("checkpoint opens");
    assert_eq!(text(&acknowledged), "abcdefgh\n");
    let result: serde_json::Value =
        serde_json::from_str(&doc.collab_recover_inner(&original, 1.0).expect("recover"))
            .expect("result");
    assert_eq!(result["replayed"], 1);
    assert_eq!(doc.log.unordered_commits(), 1);
    assert_eq!(text(&doc), "abLOCALcdefgh\n");
    let resent = doc.collab_next_chunk().expect("recovered pending chunk");
    let ClientMessage::Submit(second) =
        casual_doc_transaction::codec::decode_frame(&resent).expect("submission")
    else {
        panic!("submit");
    };
    assert!(
        second.seq > first.seq,
        "snapshot recovery must not restart a participant's sequence counter"
    );
    let mut changed = crate::open_document(&original).expect("same base");
    changed
        .apply(Operation::InsertText {
            at: Pos::new(node, 0),
            text: "REMOTE".to_owned(),
        })
        .expect("remote edit");
    let changed = changed
        .export_as_inner(casual_doc_io::formats::NORMALIZED_JSON, "semantic")
        .expect("export")
        .bytes;
    doc.resources
        .insert("local/picture.png".to_owned(), vec![0, 255, 128]);
    let result: serde_json::Value = serde_json::from_str(
        &doc.collab_recover_inner(&changed, 2.0)
            .expect("recover uncertain"),
    )
    .expect("result");
    assert_eq!(result["retained"], 1);
    assert_eq!(text(&doc), "REMOTEabcdefgh\n");
    assert_eq!(doc.collab_recovery_count(), 1);
    let retained = crate::open_document(&doc.collab_recovery_document(0).expect("backup"))
        .expect("backup opens");
    assert_eq!(
        retained.resources.get("local/picture.png"),
        Some([0, 255, 128].as_slice())
    );
    assert_eq!(text(&retained), "abLOCALcdefgh\n");
}

#[test]
fn recovery_refuses_wrong_document_and_invalid_revision_atomically() {
    use casual_doc_edit::{Operation, Pos};
    let mut doc = document();
    doc.collab_receive_frame_inner(&welcome(0, Capabilities::owner()))
        .expect("join");
    let node = match &doc.document.body()[0] {
        casual_doc_model::v1::BlockNode::Paragraph(p) => p.id,
        _ => panic!("paragraph"),
    };
    doc.apply(Operation::InsertText {
        at: Pos::new(node, 0),
        text: "LOCAL".to_owned(),
    })
    .expect("edit");
    let before = doc.document.clone();
    let pending = doc.log.unordered_commits();
    let mut other = crate::open_document(b"a different document").expect("open");
    other.document = casual_doc_model::v1::Document::new(
        casual_doc_model::NodeId::new(99).expect("id"),
        other.document.body().to_vec(),
        casual_doc_model::v1::Definitions::default(),
    )
    .expect("different identity");
    let bytes = other
        .export_as_inner(casual_doc_io::formats::NORMALIZED_JSON, "semantic")
        .expect("export")
        .bytes;
    assert!(doc.collab_recover_inner(&bytes, f64::NAN).is_err());
    assert!(doc.collab_recover_inner(b"not valid JSON", 1.0).is_err());
    assert!(doc.collab_recover_inner(&bytes, 1.0).is_err());
    assert_eq!(doc.document, before);
    assert_eq!(doc.log.unordered_commits(), pending);
}

#[test]
fn framed_carets_rebase_over_edits_and_departures_remove_them() {
    use casual_doc_edit::{Operation, Pos};
    use casual_doc_transaction::codec::{decode_frame, encode_frame};
    use casual_doc_transaction::protocol::{ClientMessage, ServerMessage};
    let mut doc = crate::open_document(b"abcdefgh").expect("document");
    let node = match &doc.document.body()[0] {
        casual_doc_model::v1::BlockNode::Paragraph(p) => p.id,
        _ => panic!("paragraph"),
    };
    let mut sender = crate::presence::new_roster();
    let frame = sender
        .caret_frame_inner(&doc, &node.to_string(), 1, &node.to_string(), 3)
        .expect("frame")
        .expect("changed");
    assert!(
        sender
            .caret_frame_inner(&doc, &node.to_string(), 1, &node.to_string(), 3)
            .expect("same")
            .is_none()
    );
    let ClientMessage::Presence(update) = decode_frame(&frame).expect("decode") else {
        panic!("presence");
    };
    let mut receiver = crate::presence::new_roster();
    receiver.sync(&doc);
    let client = ClientId::new(7);
    receiver
        .receive_frame_inner(&encode_frame(&ServerMessage::Awareness { client, update }))
        .expect("receive");
    let before = doc.revision;
    doc.apply(Operation::InsertText {
        at: Pos::new(node, 0),
        text: "XY".to_owned(),
    })
    .expect("edit");
    receiver.sync(&doc);
    let held = receiver.roster.participants().next().expect("caret");
    let caret = casual_doc_transaction::presence::Caret::decode(&held.payload).expect("typed");
    assert_eq!(caret.anchor.position.offset, 3);
    assert_eq!(caret.head.position.offset, 5);
    assert!(doc.revision > before);
    receiver
        .receive_frame_inner(&encode_frame(&ServerMessage::Departed { client }))
        .expect("depart");
    assert!(receiver.is_empty());
}

#[test]
fn snapshot_packages_preserve_resources_for_share_checkpoint_and_recovery_backups() {
    let mut doc = document();
    doc.resources
        .insert("word/media/picture.png".to_owned(), vec![0, 255, 7, 128]);
    let snapshot = snapshot_bytes(&doc.document, &doc.resources).expect("snapshot");
    let opened = crate::open_document(&snapshot).expect("packaged document");
    assert_eq!(opened.document, doc.document);
    assert_eq!(opened.resources, doc.resources);
    let checkpoint = doc.collab_checkpoint().expect("checkpoint");
    let opened = crate::open_document(&checkpoint).expect("checkpoint opens");
    assert_eq!(opened.resources, doc.resources);
    let mut package: serde_json::Value = serde_json::from_slice(&snapshot).expect("JSON");
    package["collabSnapshotVersion"] = serde_json::json!(2);
    // Preserve the canonical first-field marker when exercising version admission.
    let invalid = format!(
        "{{\"collabSnapshotVersion\":2,\"document\":{},\"resources\":{}}}",
        package["document"], package["resources"]
    );
    assert!(crate::open_document(invalid.as_bytes()).is_err());
    let invalid = format!(
        "{{\"collabSnapshotVersion\":1,\"document\":{},\"resources\":{{\"\":[]}}}}",
        package["document"]
    );
    assert!(crate::open_document(invalid.as_bytes()).is_err());
}

/// Two facades and an in-process relay, driven frame by frame through the codec — the
/// browser's path with the socket taken out.
struct Pair {
    server: casual_doc_transaction::session::ServerSession,
    docs: [crate::WasmDocument; 2],
    node: casual_doc_model::NodeId,
}

impl Pair {
    fn new(text: &str) -> Self {
        use casual_doc_transaction::protocol::{Identity, Join, Resume, ResumeKey};
        let mut server = casual_doc_transaction::session::ServerSession::default();
        let mut docs = [
            crate::open_document(text.as_bytes()).expect("opens"),
            crate::open_document(text.as_bytes()).expect("opens"),
        ];
        for (index, doc) in docs.iter_mut().enumerate() {
            let (welcome, _) = server.join(
                &ClientMessage::Join(Join {
                    profile: None,
                    protocol: PROTOCOL_VERSION,
                    identity: Identity::new(format!("p{index}")).expect("identity"),
                    grant: None,
                    resume: Some(Resume {
                        key: ResumeKey::new(format!("k{index}")).expect("key"),
                        revision: Revision::new(0),
                    }),
                }),
                Capabilities::owner(),
            );
            doc.collab_receive_frame_inner(&encode_frame(&welcome))
                .expect("welcome");
        }
        let node = match &docs[0].document.body()[0] {
            casual_doc_model::v1::BlockNode::Paragraph(p) => p.id,
            _ => panic!("paragraph"),
        };
        Self { server, docs, node }
    }

    /// Sends everything `from` has, acknowledges it, and delivers it to the other.
    fn exchange(&mut self, from: usize) {
        use casual_doc_transaction::protocol::{Arrival, Outcome};
        while let Some(frame) = self.docs[from].collab_next_chunk() {
            let ClientMessage::Submit(submission) =
                casual_doc_transaction::codec::decode_frame(&frame).expect("submission")
            else {
                panic!("submit");
            };
            let Outcome::Ordered { revision } = self.server.commit(&submission) else {
                panic!("ordered");
            };
            self.docs[from]
                .collab_receive_frame_inner(&encode_frame(&ServerMessage::Ack {
                    through: submission.seq,
                    revision,
                }))
                .expect("ack");
            self.docs[1 - from]
                .collab_receive_frame_inner(&encode_frame(&ServerMessage::Apply(Arrival {
                    author: None,
                    revision,
                    client: submission.client,
                    operations: submission.operations,
                })))
                .expect("apply");
        }
    }

    fn type_at(&mut self, who: usize, offset: u32, text: &str) {
        self.docs[who]
            .apply(casual_doc_edit::Operation::InsertText {
                at: casual_doc_edit::Pos::new(self.node, offset),
                text: text.to_owned(),
            })
            .expect("edit");
    }
}

/// **The facade carries the reader's caret across a remote insertion before it** — the
/// browser-side half of `107` P-4, through real frames. Driven red by returning the
/// selection unmapped: the caret stays at 5 and the next keystroke lands inside "Bravo".
#[test]
fn the_reader_s_caret_follows_a_remote_insert_earlier_in_the_paragraph() {
    let mut pair = Pair::new("Bravo paragraph two");
    let node = pair.node.to_string();
    // B's caret after "Bravo"; nothing has merged yet, so a first read maps nothing.
    let idle: serde_json::Value = serde_json::from_str(
        &pair.docs[1]
            .collab_map_selection_inner(&node, 5, &node, 5)
            .expect("maps"),
    )
    .expect("json");
    assert_eq!(idle["moved"], false);
    pair.type_at(0, 0, "pp");
    pair.exchange(0);
    let mapped: serde_json::Value = serde_json::from_str(
        &pair.docs[1]
            .collab_map_selection_inner(&node, 5, &node, 5)
            .expect("maps"),
    )
    .expect("json");
    assert_eq!(
        mapped["focus"]["offset"], 7,
        "the caret must move with the text"
    );
    assert_eq!(mapped["anchor"]["offset"], 7);
    assert_eq!(mapped["moved"], true);
    // A remote insertion exactly AT the caret stays after it, so two people typing at one
    // spot keep their words whole.
    pair.type_at(0, 7, "Q");
    pair.exchange(0);
    let tie: serde_json::Value = serde_json::from_str(
        &pair.docs[1]
            .collab_map_selection_inner(&node, 7, &node, 7)
            .expect("maps"),
    )
    .expect("json");
    assert_eq!(tie["focus"]["offset"], 7);
}

/// **Undo in a shared document reverts only the reader's own text, on both replicas** —
/// the facade end of selective undo. Driven red by `undo_inner` taking the plain inverse,
/// and by remote commits being recorded as edits: in both, B's text is what goes.
#[test]
fn undo_in_a_shared_document_reverts_only_the_reader_s_own_text() {
    let mut pair = Pair::new("Charlie paragraph");
    pair.type_at(0, 7, "111");
    pair.exchange(0);
    pair.type_at(1, 0, ">>");
    pair.type_at(1, 22, "222");
    pair.exchange(1);
    assert_eq!(text(&pair.docs[0]), ">>Charlie111 paragraph222\n");
    pair.docs[0].undo_inner().expect("A's own step is undoable");
    pair.exchange(0);
    assert_eq!(text(&pair.docs[0]), ">>Charlie paragraph222\n");
    assert_eq!(text(&pair.docs[1]), ">>Charlie paragraph222\n");
    assert_eq!(
        pair.docs[0].undo_inner().err().as_deref(),
        Some("nothing to undo"),
        "B's typing is not A's to undo"
    );
}

/// **The sync state counts what is typed-but-unsent and sent-but-unacknowledged** — what an
/// honest "your changes are reaching everyone" is derived from.
#[test]
fn the_session_state_says_what_has_not_reached_the_relay() {
    let mut pair = Pair::new("abc");
    let state = |doc: &crate::WasmDocument| -> serde_json::Value {
        serde_json::from_str(&doc.collab_state()).expect("json")
    };
    pair.type_at(0, 0, "x");
    assert_eq!(state(&pair.docs[0])["unsent"], 1);
    assert_eq!(state(&pair.docs[0])["inFlight"], 0);
    let frame = pair.docs[0].collab_next_chunk().expect("a chunk");
    assert_eq!(state(&pair.docs[0])["unsent"], 0);
    assert_eq!(state(&pair.docs[0])["inFlight"], 1);
    let ClientMessage::Submit(submission) =
        casual_doc_transaction::codec::decode_frame(&frame).expect("submission")
    else {
        panic!("submit");
    };
    let casual_doc_transaction::protocol::Outcome::Ordered { revision } =
        pair.server.commit(&submission)
    else {
        panic!("ordered");
    };
    pair.docs[0]
        .collab_receive_frame_inner(&encode_frame(&ServerMessage::Ack {
            through: submission.seq,
            revision,
        }))
        .expect("ack");
    assert_eq!(state(&pair.docs[0])["inFlight"], 0);
    assert_eq!(state(&pair.docs[0])["unacknowledged"], false);
}

#[test]
fn shared_picture_document_keeps_its_binary_image() {
    let doc = crate::open_document(include_bytes!(
        "../../../fixtures/generated/picture-hyperlink.docx"
    ))
    .expect("picture DOCX");
    assert!(!doc.resources.is_empty());
    let bytes = snapshot_bytes(&doc.document, &doc.resources).expect("snapshot");
    let opened = crate::open_document(&bytes).expect("shared snapshot");
    assert_eq!(opened.document, doc.document);
    assert_eq!(opened.resources, doc.resources);
    assert_eq!(opened.page_count(), doc.page_count());
}

/// The refusal a fixture row names, with representative payloads.
fn refusal_named(code: &str) -> Refusal {
    match code {
        "ODC-7001" => Refusal::CannotMerge,
        "ODC-7002" => Refusal::ProtocolVersion {
            server: PROTOCOL_VERSION,
            client: PROTOCOL_VERSION + 1,
        },
        "ODC-7003" => Refusal::NotAuthorised,
        "ODC-7004" => Refusal::ReadOnlyAccess,
        "ODC-7005" => Refusal::NotSaving,
        "ODC-7006" => Refusal::TooFarBehind {
            oldest: Revision::new(5),
            current: Revision::new(9),
        },
        "ODC-7007" => Refusal::Malformed,
        "ODC-7008" => Refusal::IdCollision,
        "ODC-7009" => Refusal::StaleBase {
            current: Revision::new(3),
        },
        "ODC-7010" => Refusal::RoomFull { limit: 128 },
        "ODC-7011" => Refusal::AccessChangeRefused,
        other => panic!("the fixture names {other}, which is not a collaboration refusal"),
    }
}

/// **Every collaboration refusal does to the reader's work exactly what the chrome tells them.**
///
/// RC-13 was a sentence — `ODC-7006`, "unsent changes were lost — reload to continue" — that
/// announced a loss the engine does not cause: `docs/169` keeps the optimistic document and the
/// log and asks for recovery, and a reader who believed the sentence and reloaded would have
/// thrown away the work it said was already gone. The chrome now builds every refusal's
/// sentence from what the outcome says (`session_access.mjs`'s `refusalConsequence`), and
/// `webapp/tests/collab_refusal_outcomes.json` records, per code and per way it can arrive, the
/// outcome and the three facts the sentences rest on: the local edit is still in this copy
/// (`kept`), the room has not ordered it — it is still an unordered commit in this replica's
/// log, so nobody else has it (`pending`) — and whether the session stopped.
///
/// This drives every row through the real facade, from a replica holding one flushed local
/// edit, and fails on any field that differs — so the fixture is the engine's behaviour rather
/// than a description of it, and `collab_refusal_messages.test.mjs` can hold the sentences to
/// it. Both directions are held: every row is driven, and every `ODC-7xxx` code has a row.
#[test]
fn every_collaboration_refusal_does_what_the_fixture_says() {
    use casual_doc_edit::{Operation, Pos};
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../webapp/tests/collab_refusal_outcomes.json"
    ))
    .expect("the fixture is JSON");
    let scenarios = fixture["scenarios"].as_array().expect("a scenario list");
    let mut covered = std::collections::BTreeSet::new();
    for scenario in scenarios {
        let code = scenario["code"].as_str().expect("a code");
        let arrives = scenario["arrives"].as_str().expect("a way of arriving");
        covered.insert(code.to_owned());
        let mut doc = document();
        doc.collab_receive_frame_inner(&welcome(0, Capabilities::owner()))
            .expect("join");
        let node = match &doc.document.body()[0] {
            casual_doc_model::v1::BlockNode::Paragraph(p) => p.id,
            _ => panic!("paragraph"),
        };
        doc.apply(Operation::InsertText {
            at: Pos::new(node, 2),
            text: "LOCAL".to_owned(),
        })
        .expect("edit");
        let sent = doc.collab_next_chunk().expect("a pending chunk");
        let ClientMessage::Submit(submission) =
            casual_doc_transaction::codec::decode_frame(&sent).expect("submission")
        else {
            panic!("submit");
        };
        let frame = match arrives {
            "refused" => encode_frame(&ServerMessage::Refused {
                seq: Some(submission.seq),
                reason: refusal_named(code),
            }),
            "stopped" => encode_frame(&ServerMessage::Stopped {
                reason: refusal_named(code),
            }),
            "unreadable" => b"not a frame at all".to_vec(),
            other => panic!("unknown arrival {other}"),
        };
        let observed = match doc.collab_receive_frame_inner(&frame) {
            Ok(json) => {
                let outcome: serde_json::Value = serde_json::from_str(&json).expect("outcome");
                assert_eq!(
                    outcome["code"].as_str(),
                    Some(code),
                    "{code} by {arrives}: the outcome must name its own code"
                );
                serde_json::json!({
                    "kind": outcome["kind"],
                    "terminal": outcome["terminal"],
                    "retryable": outcome["retryable"],
                })
            }
            Err(error) => {
                assert_eq!(
                    casual_doc_edit::refusal::split(&error).1,
                    Some(code),
                    "{code} by {arrives}: a thrown refusal must carry its own code"
                );
                serde_json::json!({ "kind": "error" })
            }
        };
        let state: serde_json::Value = serde_json::from_str(&doc.collab_state()).expect("state");
        let after = serde_json::json!({
            "kept": text(&doc).contains("LOCAL"),
            "pending": doc.log.unordered_commits() > 0,
            "stopped": state.get("stopped").is_some_and(|stopped| !stopped.is_null()),
        });
        assert_eq!(
            observed, scenario["outcome"],
            "{code} by {arrives}: the outcome differs from the fixture, so the sentence the \
             reader is shown for it no longer describes what happened"
        );
        assert_eq!(
            after, scenario["after"],
            "{code} by {arrives}: what became of the reader's edit differs from the fixture"
        );
    }
    for code in (1..=11).map(|n| format!("ODC-70{n:02}")) {
        assert!(
            covered.contains(&code),
            "{code} has no row in collab_refusal_outcomes.json, so nothing holds its sentence to \
             what it does"
        );
    }
}

/// The first paragraph's id, as the review calls take it.
fn first_paragraph(d: &crate::WasmDocument) -> String {
    d.document
        .body()
        .iter()
        .find_map(|block| match block {
            casual_doc_model::v1::BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
            _ => None,
        })
        .expect("a paragraph")
}

/// In a room, the relay's name for this participant wins over the Settings name: what this
/// replica authors is already what every other replica conforms it to (`docs/175` §5.2).
#[test]
fn in_a_room_the_relays_name_is_the_author() {
    let mut d = document();
    d.set_active_author("You", None, None)
        .expect("the anonymous author");
    let mut me = Profile::new("u_ada", "Ada Lovelace");
    me.initials = Some("AdL".to_owned());
    d.collab_receive_frame_inner(&encode_frame(&ServerMessage::Welcome {
        base_revision: None,
        missed: Vec::new(),
        protocol: PROTOCOL_VERSION,
        client: ClientId::new(0),
        revision: Revision::new(0),
        capabilities: Capabilities::owner(),
        participants: Vec::new(),
        participants_present: Vec::new(),
        me: Some(me),
    }))
    .expect("welcomed");
    let node = first_paragraph(&d);
    d.add_comment(
        &node,
        0,
        &node,
        6,
        "Mine.",
        Some("Somebody Else".to_owned()),
        None,
        None,
    )
    .expect("a comment");
    let comment = d
        .document
        .definitions()
        .comments
        .iter()
        .map(|(_, comment)| comment)
        .next()
        .expect("the comment");
    assert_eq!(comment.author.as_deref(), Some("Ada Lovelace"));
    assert_eq!(comment.initials.as_deref(), Some("AdL"));
}

/// The roster hands the chrome everyone present with what they look like — the input
/// `participantDisplay` was built for (`docs/175` §14) — and drops a leaver.
#[test]
fn the_roster_lists_everyone_present_with_their_profile() {
    let mut roster = crate::presence::new_roster();
    let mut ada = Profile::new("u_ada", "Ada Lovelace");
    ada.avatar_url = Some("https://cdn.example/a.png".to_owned());
    roster
        .receive_frame_inner(&encode_frame(&ServerMessage::Welcome {
            base_revision: None,
            missed: Vec::new(),
            protocol: PROTOCOL_VERSION,
            client: ClientId::new(1),
            revision: Revision::new(0),
            capabilities: Capabilities::owner(),
            participants: Vec::new(),
            participants_present: vec![
                Participant {
                    client: ClientId::new(0),
                    profile: Some(ada),
                    attested: true,
                },
                Participant {
                    client: ClientId::new(1),
                    profile: Some(Profile::new("u_bob", "Bob Stone")),
                    attested: true,
                },
            ],
            me: None,
        }))
        .expect("welcome");
    roster
        .receive_frame_inner(&encode_frame(&ServerMessage::ParticipantJoined(
            Participant {
                client: ClientId::new(2),
                profile: Some(Profile::new("u_x", "Ada Lovelace")),
                attested: false,
            },
        )))
        .expect("joined");
    let rows: serde_json::Value = serde_json::from_str(&roster.participants()).expect("json");
    let rows = rows.as_array().expect("an array");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["profile"]["name"], "Ada Lovelace");
    assert_eq!(rows[0]["profile"]["avatarUrl"], "https://cdn.example/a.png");
    assert_eq!(rows[0]["attested"], true);
    assert_eq!(rows[2]["attested"], false, "a claim stays a claim");
    let me: serde_json::Value = serde_json::from_str(&roster.me()).expect("json");
    assert_eq!(me["client"], 1);
    assert_eq!(me["profile"]["name"], "Bob Stone");

    roster
        .receive_frame_inner(&encode_frame(&ServerMessage::ParticipantLeft {
            client: ClientId::new(0),
        }))
        .expect("left");
    let rows: serde_json::Value = serde_json::from_str(&roster.participants()).expect("json");
    assert_eq!(rows.as_array().map(Vec::len), Some(2));
}
