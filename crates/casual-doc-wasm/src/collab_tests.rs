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

use super::capability_names;

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
        .collab_join_frame_inner("ada", "tab-key", 0.0)
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
            .collab_join_frame_inner(identity, key, revision)
            .expect_err("an unusable input must be refused rather than guessed at");
        assert!(
            error.starts_with("refused: "),
            "a refusal must be a sentence a reader can act on: {error:?}"
        );
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
