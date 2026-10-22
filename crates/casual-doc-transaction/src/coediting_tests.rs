// SPDX-License-Identifier: Apache-2.0

//! Two people editing one paragraph: the caret follows concurrent edits, and Undo reverts
//! only the reader's own steps.
//!
//! # What these reproduce
//!
//! Four defects found by running two real browsers through the relay (`109` HF-310…HF-312),
//! re-expressed here as deterministic interleavings so they are reachable on every
//! `cargo test` rather than only with two browsers and a stopwatch:
//!
//! - **Concurrent typing reversed the later typist's text.** A typed `abcdef` at the start of
//!   a paragraph while B typed `UVWXYZ` at its end; both screens showed `…ZYXWVU`. Each
//!   arrival moved B's text one byte right and left B's caret where it was, so every next
//!   keystroke went in *before* the previous one.
//! - **A remote insert earlier in the paragraph left the caret stale**, so the next keystrokes
//!   landed that many bytes early (`ppBrawwvo`).
//! - **⌘Z undid the other person's text**, because a remote commit was recorded as an
//!   ordinary edit and was therefore the newest undo step — and the undo travelled to them.
//!
//! # How the harness models the browsers
//!
//! A [`Peer`] holds what one tab holds: a document, a log, a session, and a **caret**. Typing
//! inserts at the caret and advances it, as the editor does. Receiving an arrival maps the
//! caret through [`Reception::local_map`](super::Reception::local_map) with
//! [`Affinity::Before`], which is what the wasm facade does for a collapsed caret. Every other
//! piece — the relay's order, the refusals, the rebase driver, the log — is the production
//! code, so a guard here goes red when *that* code regresses, not when the harness does.

use std::collections::VecDeque;

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{Operation, Pos, Range as EditRange};
use casual_doc_model::v1::{
    BlockNode, Break, BreakKind, Definitions, Document, InlineNode, Paragraph, ParagraphProperties,
    Run, RunProperties,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::protocol::{
    Arrival, ClientMessage, Identity, Join, Outcome, PROTOCOL_VERSION, Resume, ResumeKey, Revision,
};
use crate::selective::SelectiveUndoError;
use crate::{Affinity, Coalesce, Origin, Position, RevisionLog, Transaction, TransactionId};

use super::{ClientSession, ServerSession};

/// The investigation's fixture, verbatim: three paragraphs, the second one shared.
const PARAGRAPHS: [&str; 3] = [
    "Alpha paragraph one for co-editing tests.",
    "Bravo paragraph two shared by both users.",
    "Charlie paragraph three at the end.",
];

/// The three paragraphs; the third is split into a plain and a bold run, so a deletion
/// across the boundary takes the edit crate's general path (a whole-paragraph inverse).
fn seed() -> (Document, Vec<NodeId>) {
    let mut ids = IdGenerator::new(7);
    let document_id = ids.next_id().expect("id");
    let mut blocks = Vec::new();
    let mut nodes = Vec::new();
    for (index, text) in PARAGRAPHS.iter().enumerate() {
        let id = ids.next_id().expect("id");
        nodes.push(id);
        let mut inlines = Vec::new();
        if index == 2 {
            let bold = RunProperties {
                bold: Some(true),
                ..RunProperties::default()
            };
            inlines.push(InlineNode::Run(Run {
                id: ids.next_id().expect("id"),
                properties: RunProperties::default().into(),
                text: "Charlie ".to_owned(),
            }));
            inlines.push(InlineNode::Run(Run {
                id: ids.next_id().expect("id"),
                properties: bold.into(),
                text: "paragraph three at the end.".to_owned(),
            }));
        } else {
            inlines.push(InlineNode::Run(Run {
                id: ids.next_id().expect("id"),
                properties: RunProperties::default().into(),
                text: (*text).to_owned(),
            }));
        }
        blocks.push(BlockNode::Paragraph(Paragraph {
            id,
            properties: ParagraphProperties::default().into(),
            inlines,
        }));
    }
    let document =
        Document::new(document_id, blocks, Definitions::default()).expect("a valid document");
    (document, nodes)
}

/// One browser tab.
struct Peer {
    document: Document,
    ids: IdGenerator,
    log: RevisionLog,
    session: ClientSession,
    caret: Position,
    inbox: VecDeque<Arrival>,
    next_transaction: u128,
    /// Whether the next keystroke continues the current typing gesture.
    typing: bool,
}

impl Peer {
    fn join(document: &Document, server: &mut ServerSession, who: &str) -> Self {
        let (message, _) = server.join(
            &ClientMessage::Join(Join {
                protocol: PROTOCOL_VERSION,
                identity: Identity::new(who).expect("an identity"),
                grant: None,
                resume: Some(Resume {
                    key: ResumeKey::new(who).expect("a key"),
                    revision: Revision::new(0),
                }),
            }),
            Capabilities::owner(),
        );
        let mut log = RevisionLog::default();
        let session =
            ClientSession::joined(document, &message, &mut log).expect("the join is accepted");
        Self {
            document: document.clone(),
            ids: IdGenerator::new(session.id_space().get()),
            log,
            session,
            caret: Position::new(NodeId::new(1).expect("id"), 0, Affinity::Before),
            inbox: VecDeque::new(),
            next_transaction: 0,
            typing: false,
        }
    }

    /// A click: the caret moves and the next keystroke opens a new undo step.
    fn place(&mut self, node: NodeId, offset: u32) {
        self.caret = Position::new(node, offset, Affinity::Before);
        self.typing = false;
    }

    fn apply(&mut self, operations: Vec<Operation>, coalesce: Coalesce, origin: Origin) {
        self.next_transaction += 1;
        let transaction = Transaction::reserve(
            TransactionId::new(self.next_transaction),
            self.log.head(),
            "Typing",
            &mut self.ids,
            operations,
        )
        .expect("identity spaces")
        .coalescing(coalesce)
        .with_origin(origin);
        self.log
            .apply(&mut self.document, transaction)
            .expect("the edit applies");
    }

    /// One keystroke at the caret, coalescing with the gesture it continues.
    fn type_text(&mut self, text: &str) {
        let coalesce = if self.typing {
            Coalesce::Continue
        } else {
            Coalesce::New
        };
        self.apply(
            vec![Operation::InsertText {
                at: Pos::new(self.caret.node, self.caret.offset),
                text: text.to_owned(),
            }],
            coalesce,
            Origin::Edit,
        );
        self.caret.offset += u32::try_from(text.len()).expect("short");
        self.typing = true;
    }

    /// Shift+Enter: a hard line break at the caret, the caret after it.
    fn insert_line_break(&mut self) {
        let id = self.ids.next_id().expect("id");
        let node = InlineNode::Break(Break {
            id,
            kind: BreakKind::Line,
        });
        let width = casual_doc_edit::inlines_text_len(std::slice::from_ref(&node));
        self.apply(
            vec![Operation::InsertInlineObject {
                at: Pos::new(self.caret.node, self.caret.offset),
                node: Box::new(node),
            }],
            Coalesce::New,
            Origin::Edit,
        );
        self.caret.offset += width;
        self.typing = false;
    }

    /// Takes the next arrival off the wire, and carries the caret with it.
    fn deliver_one(&mut self) -> bool {
        let Some(arrival) = self.inbox.pop_front() else {
            return false;
        };
        let reception = self
            .session
            .receive(&arrival, &mut self.document, &mut self.ids, &mut self.log)
            .expect("the arrival merges");
        let mapped = reception.local_map.map_tracked(self.caret);
        assert!(!mapped.removed, "nothing in these runs deletes a paragraph");
        self.caret = mapped.position;
        true
    }

    /// The text of paragraph `node`, with a hard break shown as `|`.
    fn text(&self, node: NodeId) -> String {
        let mut out = String::new();
        for block in self.document.body() {
            if let BlockNode::Paragraph(paragraph) = block
                && paragraph.id == node
            {
                for inline in &paragraph.inlines {
                    match inline {
                        InlineNode::Run(run) => out.push_str(&run.text),
                        InlineNode::Break(_) => out.push('|'),
                        _ => {}
                    }
                }
            }
        }
        out
    }

    /// Undo, as the editor runs it: the selective inverse of the newest own step, retiring
    /// steps somebody else already removed.
    fn undo(&mut self) -> Result<(), SelectiveUndoError> {
        loop {
            let group = self.log.undo_target().ok_or(SelectiveUndoError::Unknown)?;
            let operations =
                self.log
                    .selective_inverse(group, &mut self.document, &mut self.ids)?;
            if operations.is_empty() {
                self.log.retire(group);
                continue;
            }
            self.apply(operations, Coalesce::New, Origin::Undo { group });
            self.typing = false;
            return Ok(());
        }
    }
}

/// The relay and the tabs.
struct Room {
    server: ServerSession,
    peers: Vec<Peer>,
}

impl Room {
    fn new(people: usize) -> (Self, Vec<NodeId>) {
        let (document, nodes) = seed();
        let mut server = ServerSession::default();
        let peers = (0..people)
            .map(|index| Peer::join(&document, &mut server, &format!("peer-{index}")))
            .collect();
        (Self { server, peers }, nodes)
    }

    /// Everything `who` has to send, through the relay, fanned out to the others.
    fn flush(&mut self, who: usize) {
        loop {
            let peer = &mut self.peers[who];
            let Some(submission) = peer.session.flush(&peer.log) else {
                return;
            };
            match self.server.commit(&submission) {
                Outcome::Ordered { revision } => {
                    peer.session
                        .acknowledge(submission.seq, revision, &mut peer.log)
                        .expect("the acknowledgement lands");
                    for (index, other) in self.peers.iter_mut().enumerate() {
                        if index != who {
                            other.inbox.push_back(Arrival {
                                revision,
                                client: submission.client,
                                operations: submission.operations.clone(),
                            });
                        }
                    }
                }
                Outcome::Duplicate { revision } => {
                    peer.session
                        .acknowledge(submission.seq, revision, &mut peer.log)
                        .expect("the acknowledgement lands");
                }
                Outcome::Refused { reason } => {
                    peer.session
                        .refused(Some(submission.seq), reason, &peer.log)
                        .expect("a refusal of one chunk is not terminal");
                    return;
                }
            }
        }
    }

    fn deliver_all(&mut self, who: usize) {
        while self.peers[who].deliver_one() {}
    }

    /// Runs the network until nothing is in flight, the way a quiet room settles.
    fn settle(&mut self) {
        for _ in 0..64 {
            for who in 0..self.peers.len() {
                self.flush(who);
            }
            for who in 0..self.peers.len() {
                self.deliver_all(who);
            }
            let quiet = self.peers.iter().all(|peer| {
                peer.inbox.is_empty()
                    && !peer.session.has_unacknowledged()
                    && peer.log.unordered_commits() == 0
            });
            if quiet {
                return;
            }
        }
        panic!("the room never settled");
    }

    /// Every replica holds the same text, paragraph by paragraph.
    fn converged(&self, nodes: &[NodeId]) -> Vec<String> {
        let first: Vec<String> = nodes.iter().map(|node| self.peers[0].text(*node)).collect();
        for peer in &self.peers[1..] {
            let other: Vec<String> = nodes.iter().map(|node| peer.text(*node)).collect();
            assert_eq!(first, other, "the replicas disagree");
        }
        first
    }
}

/// Two typists in one paragraph, one at each end, with the given schedule of who acts next.
///
/// `schedule` is a list of steps: `a`/`b` type the next character of that typist's word,
/// `A`/`B` flush that tab to the relay, `1`/`2` deliver everything waiting for tab one/two.
fn race(schedule: &str, a_at_start: bool) -> String {
    let (mut room, nodes) = Room::new(2);
    let paragraph = nodes[1];
    let length = u32::try_from(PARAGRAPHS[1].len()).expect("short");
    let (a_offset, b_offset) = if a_at_start { (0, length) } else { (length, 0) };
    room.peers[0].place(paragraph, a_offset);
    room.peers[1].place(paragraph, b_offset);
    let mut a_word = "abcdef".chars();
    let mut b_word = "UVWXYZ".chars();
    for step in schedule.chars() {
        match step {
            'a' => {
                if let Some(ch) = a_word.next() {
                    room.peers[0].type_text(&ch.to_string());
                }
            }
            'b' => {
                if let Some(ch) = b_word.next() {
                    room.peers[1].type_text(&ch.to_string());
                }
            }
            'A' => room.flush(0),
            'B' => room.flush(1),
            '1' => room.deliver_all(0),
            '2' => room.deliver_all(1),
            other => panic!("unknown step {other}"),
        }
    }
    room.settle();
    room.converged(&nodes)[1].clone()
}

/// The schedule a browser produces when every keystroke is sent at once and the relay
/// answers before the next key: type, send, and hear from the other tab in between.
const KEY_BY_KEY: &str = "aAbB12aAbB12aAbB12aAbB12aAbB12aAbB12";

#[test]
fn the_later_typist_s_text_is_not_reversed_when_two_people_type_in_one_paragraph() {
    // RC-01 as the investigation ran it: A at the start, B at the end, one key at a time.
    assert_eq!(
        race(KEY_BY_KEY, true),
        format!("abcdef{}UVWXYZ", PARAGRAPHS[1]),
        "concurrent typing in one paragraph must keep each typist's word in order"
    );
    // And with the roles swapped, which the investigation also reproduced (`ghij` → `jihg`).
    assert_eq!(
        race(KEY_BY_KEY, false),
        format!("UVWXYZ{}abcdef", PARAGRAPHS[1])
    );
}

#[test]
fn typing_ahead_of_the_relay_keeps_both_words_whole_in_every_interleaving() {
    // The contended path: keystrokes pile up unacknowledged on both sides, so arrivals are
    // rebased over pending work and the caret is carried through the arrival's image. A
    // seeded walk over schedules rather than a handful of hand-picked ones, because the bug
    // this guards lived in the order of events, not in any one operation.
    let mut state = 0x9E37_79B9_u32;
    for seed in 0..200 {
        let mut schedule = String::new();
        for _ in 0..40 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            schedule.push(['a', 'b', 'A', 'B', '1', '2'][(state >> 24) as usize % 6]);
        }
        let start = seed % 2 == 0;
        let text = race(&schedule, start);
        let typed_a = schedule.matches('a').count().min(6);
        let typed_b = schedule.matches('b').count().min(6);
        let a_word = &"abcdef"[..typed_a];
        let b_word = &"UVWXYZ"[..typed_b];
        let expected = if start {
            format!("{a_word}{}{b_word}", PARAGRAPHS[1])
        } else {
            format!("{b_word}{}{a_word}", PARAGRAPHS[1])
        };
        assert_eq!(text, expected, "schedule {schedule}");
    }
}

#[test]
fn a_remote_insert_before_the_caret_carries_the_caret_with_the_text() {
    // RC-02 verbatim: B's caret after "Bravo" (offset 5); A inserts "pp" at the start; B
    // types "ww". Without the mapping B's keys landed two bytes early: "ppBrawwvo".
    let (mut room, nodes) = Room::new(2);
    let paragraph = nodes[1];
    room.peers[1].place(paragraph, 5);
    room.peers[0].place(paragraph, 0);
    room.peers[0].type_text("p");
    room.peers[0].type_text("p");
    room.flush(0);
    room.deliver_all(1);
    assert_eq!(
        room.peers[1].caret.offset, 7,
        "the caret must move with the two bytes inserted before it"
    );
    room.peers[1].type_text("w");
    room.peers[1].type_text("w");
    room.settle();
    let text = room.converged(&nodes)[1].clone();
    assert!(
        text.starts_with("ppBravoww paragraph"),
        "B's keystrokes must land after \"Bravo\", got {text:?}"
    );
}

#[test]
fn a_line_break_inserted_concurrently_keeps_both_typists_on_their_own_side_of_it() {
    // The position model gives a hard break its own width (`casual_doc_edit::
    // inline_text_len`). A breaks the line after "Bravo" and types on the new line while B
    // types further along; both words must stay whole and on the side of the break each
    // typist put them, on both replicas.
    let (mut room, nodes) = Room::new(2);
    let paragraph = nodes[1];
    room.peers[0].place(paragraph, 5);
    room.peers[1].place(paragraph, 15);
    room.peers[0].insert_line_break();
    room.peers[1].type_text("y");
    room.peers[0].type_text("X");
    room.flush(1);
    room.peers[1].type_text("z");
    room.flush(0);
    room.deliver_all(1);
    room.deliver_all(0);
    room.peers[0].type_text("Y");
    room.peers[1].type_text("!");
    room.settle();
    let text = room.converged(&nodes)[1].clone();
    assert!(
        text.contains("paragraphyz! two"),
        "B's word must stay whole where B typed it, got {text:?}"
    );
    assert!(
        text.contains("XY"),
        "A's word must stay whole, got {text:?}"
    );
    let width = casual_doc_edit::inlines_text_len(&[InlineNode::Break(Break {
        id: NodeId::new(9).expect("id"),
        kind: BreakKind::Line,
    })]);
    // Where a break occupies a position (the model `docs/109` TA-01 moves to), text typed
    // after it lands after it. Under the older zero-width model the caret before and after
    // the break were one offset, which is TA-01 itself and not this lane's to assert.
    if width > 0 {
        assert_eq!(text, "Bravo|XY paragraphyz! two shared by both users.");
    }
}

// ── Undo ───────────────────────────────────────────────────────────────────────────────

#[test]
fn undo_reverts_only_the_reader_s_own_text() {
    // RC-03 verbatim: A types "111" at offset 7 of the third paragraph; afterwards B types
    // "222" at its end; A presses Undo. Before, the newest step was B's remote commit, so
    // A's Undo turned B's "222" into "2" — on B's screen too — and left "111".
    let (mut room, nodes) = Room::new(2);
    let paragraph = nodes[2];
    room.peers[0].place(paragraph, 7);
    for _ in 0..3 {
        room.peers[0].type_text("1");
    }
    room.settle();
    let end = u32::try_from(room.peers[1].text(paragraph).len()).expect("short");
    room.peers[1].place(paragraph, end);
    for _ in 0..3 {
        room.peers[1].type_text("2");
    }
    room.settle();
    assert_eq!(
        room.converged(&nodes)[2],
        "Charlie111 paragraph three at the end.222"
    );

    room.peers[0].undo().expect("A's own step is undoable");
    room.settle();
    assert_eq!(
        room.converged(&nodes)[2],
        "Charlie paragraph three at the end.222",
        "Undo must remove A's word and nothing of B's, on both replicas"
    );
    assert_eq!(
        room.peers[0].undo(),
        Err(SelectiveUndoError::Unknown),
        "A has nothing else to undo: B's typing is not A's step"
    );
}

#[test]
fn undo_carries_the_inverse_over_text_somebody_typed_in_front_of_it() {
    // The coordinates case. B types in FRONT of A's word after A typed it, so the plain
    // inverse — "delete bytes 7..10" — would delete B's text and some of the original.
    let (mut room, nodes) = Room::new(2);
    let paragraph = nodes[2];
    room.peers[0].place(paragraph, 7);
    for ch in ["x", "y", "z"] {
        room.peers[0].type_text(ch);
    }
    room.settle();
    room.peers[1].place(paragraph, 0);
    room.peers[1].type_text(">>");
    room.settle();
    room.peers[0].undo().expect("undoable");
    room.settle();
    assert_eq!(
        room.converged(&nodes)[2],
        ">>Charlie paragraph three at the end."
    );
}

#[test]
fn one_undo_takes_back_a_whole_word_typed_while_somebody_else_typed_elsewhere() {
    // A's word is one step even though B's keystrokes landed between A's: a local `Continue`
    // joins the newest OWN group across arrivals.
    let (mut room, nodes) = Room::new(2);
    room.peers[0].place(nodes[0], 5);
    room.peers[1].place(nodes[2], 0);
    for (a, b) in ["k", "l", "m"].into_iter().zip(["7", "8", "9"]) {
        room.peers[0].type_text(a);
        room.peers[1].type_text(b);
        room.settle();
    }
    room.peers[0].undo().expect("undoable");
    room.settle();
    let texts = room.converged(&nodes);
    assert_eq!(texts[0], PARAGRAPHS[0], "the whole word goes in one Undo");
    assert!(texts[2].starts_with("789Charlie"), "B's typing stays");
}

#[test]
fn a_step_somebody_else_already_removed_is_skipped_and_the_older_one_undone() {
    let (mut room, nodes) = Room::new(2);
    room.peers[0].place(nodes[0], 0);
    room.peers[0].type_text("Q");
    room.peers[0].place(nodes[1], 0);
    room.peers[0].type_text("R");
    room.settle();
    // B deletes A's "R".
    room.peers[1].next_transaction += 1;
    let peer = &mut room.peers[1];
    let transaction = Transaction::reserve(
        TransactionId::new(peer.next_transaction),
        peer.log.head(),
        "Delete",
        &mut peer.ids,
        vec![Operation::DeleteText {
            range: EditRange {
                start: Pos::new(nodes[1], 0),
                end: Pos::new(nodes[1], 1),
            },
        }],
    )
    .expect("ids");
    peer.log
        .apply(&mut peer.document, transaction)
        .expect("applies");
    room.settle();
    room.peers[0].undo().expect("the older step is undone");
    room.settle();
    let texts = room.converged(&nodes);
    assert_eq!(
        texts[0], PARAGRAPHS[0],
        "the next older own step was undone"
    );
    assert_eq!(texts[1], PARAGRAPHS[1]);
}

#[test]
fn an_undo_that_cannot_be_carried_cleanly_is_refused_and_changes_nothing() {
    // A deletes across the plain/bold boundary, which records a whole-paragraph inverse.
    // Once B types in that paragraph the inverse would overwrite B's text, so the undo is
    // refused — with the document, the log and the step all unchanged.
    let (mut room, nodes) = Room::new(2);
    let paragraph = nodes[2];
    {
        let peer = &mut room.peers[0];
        peer.next_transaction += 1;
        let transaction = Transaction::reserve(
            TransactionId::new(peer.next_transaction),
            peer.log.head(),
            "Delete",
            &mut peer.ids,
            vec![Operation::DeleteText {
                range: EditRange {
                    start: Pos::new(paragraph, 5),
                    end: Pos::new(paragraph, 12),
                },
            }],
        )
        .expect("ids");
        peer.log
            .apply(&mut peer.document, transaction)
            .expect("applies");
    }
    room.settle();
    room.peers[1].place(paragraph, 0);
    room.peers[1].type_text("B:");
    room.settle();
    let before = room.peers[0].document.clone();
    let target = room.peers[0].log.undo_target();
    let refused = room.peers[0].undo();
    assert!(
        matches!(refused, Err(SelectiveUndoError::Conflict { .. })),
        "got {refused:?}"
    );
    assert_eq!(
        room.peers[0].document, before,
        "a refused undo changes nothing"
    );
    assert_eq!(
        room.peers[0].log.undo_target(),
        target,
        "and keeps the step"
    );
    assert!(room.converged(&nodes)[2].starts_with("B:Charl"));
}

#[test]
fn redo_after_somebody_typed_in_front_puts_the_word_back_where_it_was() {
    let (mut room, nodes) = Room::new(2);
    let paragraph = nodes[2];
    room.peers[0].place(paragraph, 7);
    room.peers[0].type_text("!");
    room.settle();
    room.peers[0].undo().expect("undo");
    room.settle();
    room.peers[1].place(paragraph, 0);
    room.peers[1].type_text("<<");
    room.settle();
    // Redo is the undo of the undo step, through the same selective path.
    let peer = &mut room.peers[0];
    let group = peer
        .log
        .redo_target()
        .expect("a redo step survives B's typing");
    let operations = peer
        .log
        .selective_inverse(group, &mut peer.document, &mut peer.ids)
        .expect("redo carries");
    peer.apply(operations, Coalesce::New, Origin::Redo { group });
    room.settle();
    assert_eq!(
        room.converged(&nodes)[2],
        "<<Charlie! paragraph three at the end."
    );
}

#[test]
fn a_reader_alone_undoes_with_the_plain_inverse_and_touches_nothing_else() {
    // The single-user path is unchanged: with no arrival after the step, the selective
    // inverse IS the recorded inverse and the document is not used as a scratch surface.
    let (mut room, nodes) = Room::new(1);
    room.peers[0].place(nodes[0], 0);
    room.peers[0].type_text("a");
    room.peers[0].type_text("b");
    let peer = &mut room.peers[0];
    let group = peer.log.undo_target().expect("a step");
    let before = peer.document.clone();
    let mut ids = IdGenerator::new(0x5555_0000);
    let selective = peer
        .log
        .selective_inverse(group, &mut peer.document, &mut ids)
        .expect("plain");
    assert_eq!(selective, peer.log.inverse_of(group));
    assert_eq!(peer.document, before);
    assert_eq!(
        ids.next_id(),
        IdGenerator::new(0x5555_0000).next_id(),
        "no scratch id was reserved"
    );
}

#[test]
fn a_caret_mapped_once_after_several_arrivals_lands_where_mapping_each_would() {
    // A `Welcome` or a `Resumed` carries several arrivals in one frame, and a host that maps
    // once per frame must get the composition of all of them — the session holds it.
    let (mut room, nodes) = Room::new(2);
    let paragraph = nodes[1];
    room.peers[0].place(paragraph, 0);
    for ch in ["1", "2", "3"] {
        room.peers[0].type_text(ch);
        room.flush(0);
    }
    room.peers[1].place(paragraph, 5);
    let _ = room.peers[1].session.take_caret_map();
    let start = room.peers[1].caret;
    room.deliver_all(1);
    let composed = room.peers[1].session.take_caret_map().map(start);
    assert_eq!(composed, room.peers[1].caret);
    assert_eq!(composed.offset, 8);
    assert!(
        room.peers[1].session.take_caret_map().steps().is_empty(),
        "taking the map leaves it empty"
    );
}
