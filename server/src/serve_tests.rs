// SPDX-License-Identifier: Apache-2.0

//! **The transport's end-to-end guards: two clients, one relay, one real socket.**
//!
//! # What has never been exercised before this file
//!
//! Everything in `107` Phase 6 was built and tested *in one process*: `ServerSession` against
//! two `ClientSession`s, with the test standing in for the network. So the ordering, the
//! transform, the rebase driver, the journal and the eviction policy were each guarded, and
//! **nothing carried a byte between two processes**. `offline_combine_agrees_with_the_live_session_path`
//! is the closest thing that existed, and its "through the relay" is a function call.
//!
//! These guards close that gap, and the first one is deliberately that test's sibling: the same
//! two branches, the same ancestor, the same `combine` call for the offline answer — and the live
//! answer taken from two real `ClientSession`s talking over a real WebSocket to a real
//! `opendoc-relay`. If either side ever grows a rebase of its own, the two answers part company
//! here.
//!
//! # Why the sockets are real and not doubles
//!
//! Because the three failures this lane exists to rule out are all invisible to a double: a
//! torn frame from two threads writing one socket, a handshake that loses the bytes pipelined
//! behind it, and a resume that is told `TooFarBehind` after the relay restarted. A `Vec<u8>`
//! writer has no file descriptor to tear, no handshake and no restart.

use std::io::Write as _;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{Operation, Pos};
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties,
};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_transaction::codec::encode_frame;
use casual_doc_transaction::combine::{Branch, combine};
use casual_doc_transaction::protocol::PROTOCOL_VERSION;
use casual_doc_transaction::protocol::{
    ClientId, ClientMessage, Identity, Join, Refusal, Resume, ResumeKey, Revision, ServerMessage,
};
use casual_doc_transaction::session::ClientSession;
use casual_doc_transaction::wire::{self, IdSpace};
use casual_doc_transaction::{Commit, RevisionLog, Transaction, TransactionId};

use super::{Notice, Shared, participant};
use crate::transport::Frames;
use crate::websocket::{self, Framed, Role, Unframed};
use crate::{Access, Relay, Room};

// ---------------------------------------------------------------------------------------
// The fixture: a relay on a real port, and a client that is a transport and nothing more
// ---------------------------------------------------------------------------------------

/// A scratch path unique to this binary, this name and this moment.
fn scratch(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "opendoc-transport-{}-{name}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos())
    ));
    path
}

/// A document of `paragraphs` one-run paragraphs, each holding `abcdefgh`.
///
/// **Byte for byte the shape `combine_tests::seed` makes**, including the id generator's seed,
/// because the first guard compares this file's live answer against that file's offline one and
/// a different ancestor would make the comparison meaningless.
fn seed(paragraphs: usize) -> (Document, Vec<NodeId>) {
    let mut ids = IdGenerator::new(7);
    let document_id = ids.next_id().expect("id");
    let mut blocks = Vec::new();
    let mut out = Vec::new();
    for _ in 0..paragraphs {
        let id = ids.next_id().expect("id");
        let run = ids.next_id().expect("id");
        out.push(id);
        blocks.push(BlockNode::Paragraph(Paragraph {
            id,
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Run(Run {
                id: run,
                properties: RunProperties::default().into(),
                text: "abcdefgh".to_owned(),
            })],
        }));
    }
    (
        Document::new(document_id, blocks, Definitions::default()).expect("a valid document"),
        out,
    )
}

fn plain_text(document: &Document) -> String {
    let mut text = String::new();
    for block in document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    text.push_str(&run.text);
                }
            }
            text.push('\n');
        }
    }
    text
}

fn typing(node: NodeId, at: u32, text: &str) -> Operation {
    Operation::InsertText {
        at: Pos::new(node, at),
        text: text.to_owned(),
    }
}

/// A relay listening on a real port, accepting a bounded number of connections.
///
/// **Bounded deliberately.** An endless accept loop would hold an `Arc<Shared>` for ever, and
/// the room's journal file stays open while anything holds the room — so the crash-recovery
/// guard could never reopen it. Bounding the count lets [`Running::stop`] join every thread and
/// release the last reference, which is what "the relay process went away" means.
struct Running {
    address: SocketAddr,
    relay: Arc<Shared>,
    notices: Arc<Mutex<Vec<Notice>>>,
    accepting: std::thread::JoinHandle<()>,
}

impl Running {
    /// Opens `path` as a room and serves it, accepting exactly `connections` connections.
    fn start(path: &Path, access: Access, connections: usize) -> Self {
        let (room, _) = Room::open(path).expect("the room opens");
        let relay: Arc<Shared> = Arc::new(Mutex::new(Relay::new(room, access)));
        let notices: Arc<Mutex<Vec<Notice>>> = Arc::new(Mutex::new(Vec::new()));
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
        let address = listener.local_addr().expect("an address");
        let served = Arc::clone(&relay);
        let reported = Arc::clone(&notices);
        let accepting = std::thread::spawn(move || {
            let mut workers = Vec::new();
            for _ in 0..connections {
                let Ok((stream, _)) = listener.accept() else {
                    break;
                };
                let relay = Arc::clone(&served);
                let notices = Arc::clone(&reported);
                workers.push(std::thread::spawn(move || {
                    let report = |notice: Notice| {
                        if let Ok(mut log) = notices.lock() {
                            log.push(notice);
                        }
                    };
                    // The production per-connection loop, not a copy of it.
                    if let Err(error) = participant(stream, &relay, &report) {
                        report(Notice::Ended {
                            detail: error.to_string(),
                        });
                    }
                }));
            }
            // Released before the workers are joined, so the only references left are theirs.
            drop(served);
            drop(listener);
            for worker in workers {
                let _ = worker.join();
            }
        });
        Self {
            address,
            relay,
            notices,
            accepting,
        }
    }

    /// Waits for every connection thread to finish and releases the room.
    ///
    /// Callers must have dropped their client sockets first; a worker ends when its client
    /// does.
    fn stop(self) -> Vec<Notice> {
        self.accepting.join().expect("the accept thread finished");
        assert_eq!(
            Arc::strong_count(&self.relay),
            1,
            "a connection thread outlived its client, so the room's journal is still open"
        );
        drop(self.relay);
        Arc::try_unwrap(self.notices)
            .map(|lock| lock.into_inner().unwrap_or_default())
            .unwrap_or_default()
    }
}

/// One client: a socket, a session, a document and a log — exactly what a browser's transport
/// owns, with the browser's `WebSocket` replaced by [`crate::websocket`]'s client role.
struct Peer {
    writer: Framed<TcpStream>,
    frames: Frames<Unframed<TcpStream>>,
    socket: TcpStream,
    session: ClientSession,
    document: Document,
    log: RevisionLog,
    ids: IdGenerator,
    identity: Identity,
    resume: ResumeKey,
    next: u128,
    /// Messages already read and fed to the session, kept so a guard can ask about one that
    /// arrived **before** the one it was waiting for.
    ///
    /// Without this, `pump_until(Apply)` after `pump_until(Refused)` blocks for ever whenever
    /// the fan-out overtook the refusal — a real interleaving the relay is free to produce, and
    /// a guard that only passes in one arrival order is a guard that tests the scheduler.
    seen: Vec<ServerMessage>,
}

impl Peer {
    /// Connects, handshakes, joins and adopts the answer.
    fn join(address: SocketAddr, who: &str, ancestor: &Document) -> Self {
        Self::open(address, who, ancestor, None, None)
    }

    /// The shared path: a fresh join, or a reconnect carrying `resume` and the state to keep.
    fn open(
        address: SocketAddr,
        who: &str,
        document: &Document,
        resume: Option<Resume>,
        carry: Option<(ClientSession, RevisionLog, IdGenerator, u128, ResumeKey)>,
    ) -> Self {
        let mut socket = TcpStream::connect(address).expect("it connects");
        // A message this client waits for and never gets must fail the guard, not hang it: a
        // test that blocks for ever reports nothing at all, and CI reports a timeout with no
        // name attached to it.
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .expect("a read timeout");
        // A fixed nonce: this end is a test, and `websocket::connect` takes the bytes rather
        // than inventing them precisely so a caller says where they came from.
        let nonce = {
            let mut bytes = [0_u8; 16];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = who.as_bytes()[index % who.len()] ^ (index as u8);
            }
            bytes
        };
        let leftover = websocket::connect(&mut socket, "127.0.0.1", "/room", nonce)
            .expect("the handshake completes");
        let mut writer = Framed::new(socket.try_clone().expect("a clone"), Role::Client);
        let mut frames = Frames::new(Unframed::new(
            socket.try_clone().expect("a clone"),
            Role::Client,
            leftover,
        ));
        let identity = Identity::new(who).expect("an identity");
        let key = carry.as_ref().map_or_else(
            || ResumeKey::new(format!("{who}-key")).expect("a resume key"),
            |carried| carried.4.clone(),
        );
        // **The key travels on every join, including the first**, and that is the protocol
        // rather than a precaution: `ServerSession::join` records the key only from the join
        // that presented it, so a client which withheld it on its first connection can never be
        // recognised on its second. `Join::resume`'s own doc comment said "when this is a
        // reconnect" and cost this lane a debugging cycle; it now says what the code does.
        let join = ClientMessage::Join(Join {
            protocol: PROTOCOL_VERSION,
            identity: identity.clone(),
            grant: None,
            resume: Some(Resume {
                key: key.clone(),
                revision: resume.map_or_else(Revision::default, |resume| resume.revision),
            }),
        });
        writer
            .write_all(&encode_frame(&join))
            .expect("the join is written");
        writer.flush().expect("it flushes");
        let answer: ServerMessage = frames.next_frame().expect("an answer to the join");

        let mut document = document.clone();
        let (session, log, ids, next) = match carry {
            None => {
                let mut log = RevisionLog::default();
                let session = ClientSession::joined(&document, &answer, &mut log)
                    .unwrap_or_else(|error| panic!("the join was refused: {error} ({answer:?})"));
                let space = wire::space_of(IdSpace::of_document(document.id()), session.client())
                    .expect("the participant number has a space");
                (session, log, IdGenerator::new(space.get()), 0)
            }
            Some((mut session, mut log, ids, next, _)) => {
                session
                    .resumed(&answer)
                    .unwrap_or_else(|error| panic!("the resume was refused: {error} ({answer:?})"));
                // The missed arrivals travel inside the message, so they are fed before
                // anything else can be flushed — `awaiting` enforces that as state.
                if let ServerMessage::Resumed { missed, .. } = &answer {
                    let mut ids = ids;
                    for arrival in missed {
                        session
                            .receive(arrival, &mut document, &mut ids, &mut log)
                            .expect("a missed arrival merges");
                    }
                    (session, log, ids, next)
                } else {
                    panic!("expected a resume, got {answer:?}");
                }
            }
        };
        Self {
            writer,
            frames,
            socket,
            session,
            document,
            log,
            ids,
            identity,
            resume: key,
            next,
            seen: Vec::new(),
        }
    }

    /// Applies one local edit, exactly as a keystroke does.
    fn edit(&mut self, label: &'static str, operations: Vec<Operation>) {
        self.next += 1;
        let transaction = Transaction::reserve(
            TransactionId::new(self.next),
            self.log.head(),
            label,
            &mut self.ids,
            operations,
        )
        .expect("identity spaces");
        self.log
            .apply(&mut self.document, transaction)
            .expect("the edit applies");
    }

    /// This replica's commits so far, for the offline comparison.
    fn commits(&self) -> Vec<Commit> {
        self.log.commits().cloned().collect()
    }

    /// Sends whatever the session is willing to flush, and reports whether anything went.
    fn send(&mut self) -> bool {
        match self.session.flush(&self.log) {
            Some(submission) => {
                self.writer
                    .write_all(&encode_frame(&ClientMessage::Submit(submission)))
                    .expect("the submission is written");
                self.writer.flush().expect("it flushes");
                true
            }
            None => false,
        }
    }

    /// Reads one server message and feeds it to the session.
    ///
    /// Returns what arrived, so a guard can assert on the *kind* of answer rather than only on
    /// the document that came out of it.
    fn pump(&mut self) -> ServerMessage {
        let message: ServerMessage = self.frames.next_frame().expect("a server message");
        match &message {
            ServerMessage::Ack { through, revision } => {
                self.session
                    .acknowledge(*through, *revision, &mut self.log)
                    .expect("the acknowledgement lands");
            }
            ServerMessage::Apply(arrival) => {
                self.session
                    .receive(arrival, &mut self.document, &mut self.ids, &mut self.log)
                    .expect("the arrival merges");
            }
            ServerMessage::Refused { seq, reason } => {
                self.session
                    .refused(*seq, *reason, &self.log)
                    .expect("the refusal is recorded");
            }
            ServerMessage::Stopped { reason } => self.session.stop(*reason),
            ServerMessage::Welcome { .. }
            | ServerMessage::Resumed { .. }
            | ServerMessage::Awareness { .. }
            | ServerMessage::Departed { .. }
            // A rights change is not something a `ClientSession` has state for: it governs what
            // the CHROME offers, and the authority stays the relay's own copy. Returned to the
            // caller like every other message, so a guard can assert on it.
            | ServerMessage::AccessChanged { .. } => {}
        }
        message
    }

    /// Reads until `wanted` says the message it is looking for has arrived, **counting ones
    /// that already did**.
    fn pump_until(&mut self, wanted: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
        if let Some(index) = self.seen.iter().position(&wanted) {
            return self.seen.remove(index);
        }
        for _ in 0..16 {
            let message = self.pump();
            if wanted(&message) {
                return message;
            }
            self.seen.push(message);
        }
        panic!(
            "the message this guard waited for never arrived; seen: {:?}",
            self.seen
        );
    }

    fn text(&self) -> String {
        plain_text(&self.document)
    }
}

// ---------------------------------------------------------------------------------------
// The milestone
// ---------------------------------------------------------------------------------------

/// **Two clients, one relay, one document, convergent text — through a real socket.**
///
/// The sibling of `casual_doc_transaction::combine::tests::offline_combine_agrees_with_the_live_session_path`,
/// and deliberately the same two branches: that test's "live path" is a function call and this
/// one's is a WebSocket. The merged text must equal the offline `combine` answer, so the two
/// answers part company here if either side ever grows a rebase of its own.
///
/// # What this exercises that nothing else did
///
/// The whole round trip, including the parts that only exist when two processes are involved:
/// the handshake, the frame boundaries, the fan-out to a *different* socket, Ada's chunk being
/// refused as `StaleBase` because Grace's landed first, Ada rebasing over Grace's arrival and
/// resubmitting **with the same sequence number**, and Grace receiving Ada's rebased
/// operations rather than her original ones.
#[test]
fn two_clients_through_one_relay_converge_on_one_document() {
    let path = scratch("converge");
    Room::create(&path).expect("a room");
    let (ancestor, paragraphs) = seed(2);
    let running = Running::start(&path, Access::Open(Capabilities::owner()), 2);

    let mut ada = Peer::join(running.address, "ada", &ancestor);
    let mut grace = Peer::join(running.address, "grace", &ancestor);
    assert_eq!(ada.session.client(), ClientId::new(0));
    assert_eq!(grace.session.client(), ClientId::new(1));

    // Two concurrent edits, each written against revision 0 and neither aware of the other.
    ada.edit("Typing", vec![typing(paragraphs[0], 2, "OUR")]);
    grace.edit("Typing", vec![typing(paragraphs[0], 4, "THEIR")]);
    let our_commits = ada.commits();
    let their_commits = grace.commits();

    // Grace's lands first, so the live order matches what `combine` does offline: it applies
    // `theirs` as written and rebases `ours` onto it.
    assert!(grace.send(), "grace has unacknowledged work");
    grace.pump_until(|message| matches!(message, ServerMessage::Ack { .. }));

    // Ada submits against a revision the document has moved past.
    assert!(ada.send(), "ada has unacknowledged work");
    let refusal = ada.pump_until(|message| matches!(message, ServerMessage::Refused { .. }));
    assert!(
        matches!(
            refusal,
            ServerMessage::Refused {
                reason: Refusal::StaleBase { .. },
                ..
            }
        ),
        "a chunk written against a superseded revision must be refused as stale, got {refusal:?}"
    );
    // The arrival that caused the refusal, which is what unlatches `awaiting`.
    ada.pump_until(|message| matches!(message, ServerMessage::Apply(_)));
    assert!(
        ada.send(),
        "a stale chunk must be resubmittable once the arrival it conflicted with has landed"
    );
    ada.pump_until(|message| matches!(message, ServerMessage::Ack { .. }));

    // And Grace receives Ada's chunk, rebased.
    grace.pump_until(|message| matches!(message, ServerMessage::Apply(_)));

    assert_eq!(
        ada.text(),
        grace.text(),
        "two clients of one relay hold different documents, so the transport does not converge"
    );

    // The offline answer, for the same two branches.
    let offline = combine(
        &ancestor,
        Branch {
            client: ClientId::new(0),
            commits: &our_commits,
        },
        Branch {
            client: ClientId::new(1),
            commits: &their_commits,
        },
    )
    .expect("the branches combine");
    assert_eq!(
        plain_text(&offline.document),
        ada.text(),
        "the live transport and offline combine produced different documents, so one of them \
         is not the transform the other claims to be"
    );
    assert!(
        !ada.session.has_unacknowledged() && !grace.session.has_unacknowledged(),
        "every chunk must be acknowledged once the exchange has settled"
    );
    assert!(!ada.session.is_desynced() && !grace.session.is_desynced());

    drop(ada);
    drop(grace);
    running.stop();
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------
// Resume across a restart — the bug that could not be exercised until there was a transport
// ---------------------------------------------------------------------------------------

/// **A client whose work was acknowledged is resumed after the relay restarts, not told it is
/// too far behind.**
///
/// `Record::Admitted` and the one-bounded-frame-per-entry journal landed for this, and until
/// now there was no way to drive it: a resume needs a *second connection*, and an in-process
/// test has only one. The relay here genuinely goes away — every thread joined, the last
/// reference dropped, the journal file closed — and comes back from the file alone.
///
/// The failure this rules out is the worst kind the protocol has: `TooFarBehind` is announced
/// loss, and announcing it to a client whose work the relay already journalled would be
/// throwing away acknowledged work and saying so.
#[test]
fn a_client_resumes_across_a_relay_restart_rather_than_losing_its_work() {
    let path = scratch("resume");
    Room::create(&path).expect("a room");
    let (ancestor, paragraphs) = seed(2);

    let first = Running::start(&path, Access::Open(Capabilities::owner()), 1);
    let mut ada = Peer::join(first.address, "ada", &ancestor);
    ada.edit("Typing", vec![typing(paragraphs[0], 0, "BEFORE")]);
    assert!(ada.send());
    ada.pump_until(|message| matches!(message, ServerMessage::Ack { .. }));
    let reached = ada.session.revision();
    assert_eq!(
        reached,
        Revision::new(1),
        "one chunk, ordered at revision 1"
    );
    let before = ada.text();

    // The relay goes away. The client's socket goes with it, which is what a crash looks like
    // from here; the journal is on disk and nothing else survives.
    let who = ada.identity.as_str().to_owned();
    // The document the client holds, not the ancestor: a resume preserves the replica's own
    // state, and handing the ancestor back would make this guard pass by replacing exactly
    // what it exists to prove is kept.
    let carried_document = ada.document.clone();
    drop(ada.writer);
    drop(ada.frames);
    drop(ada.socket);
    let notices = first.stop();
    assert!(
        !notices
            .iter()
            .any(|notice| matches!(notice, Notice::Evicted { .. })),
        "a client that disconnected cleanly must not be reported as evicted: {notices:?}"
    );

    // And comes back from the file.
    let second = Running::start(&path, Access::Open(Capabilities::owner()), 1);
    let mut ada = Peer::open(
        second.address,
        &who,
        &carried_document,
        Some(Resume {
            key: ResumeKey::new("ada-key").expect("a key"),
            revision: reached,
        }),
        Some((ada.session, ada.log, ada.ids, ada.next, ada.resume)),
    );
    assert_eq!(
        ada.session.revision(),
        reached,
        "a resumed client must be where it was, not at the start"
    );
    assert_eq!(
        ada.text(),
        before,
        "a resume must not replace the document; the work it exists to preserve is in it"
    );

    // And it can still write, with its chunk counter continuing rather than restarting.
    ada.edit("Typing", vec![typing(paragraphs[1], 0, "AFTER")]);
    assert!(ada.send(), "a resumed client may submit");
    let ack = ada.pump_until(|message| matches!(message, ServerMessage::Ack { .. }));
    let ServerMessage::Ack { through, revision } = ack else {
        unreachable!("filtered above")
    };
    assert_eq!(
        through.get(),
        2,
        "the chunk counter must continue across the resume, or a new chunk collides with an \
         old number and is discarded as a duplicate"
    );
    assert_eq!(revision, Revision::new(2));
    assert!(ada.text().contains("AFTER") && ada.text().contains("BEFORE"));

    drop(ada.writer);
    drop(ada.frames);
    drop(ada.socket);
    second.stop();
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------
// The grant arrives on the wire, and narrows
// ---------------------------------------------------------------------------------------

/// **A capability set that arrives on the wire narrows what a client may do and cannot widen
/// it.**
///
/// The transport is where the URL grant stops being the source of truth, so this is the
/// property that keeps that from being a widening. Two halves, both asserted:
///
/// 1. `Welcome` carries the room's ceiling, and a client that joined believing itself an owner
///    is told it is a viewer. A client has nowhere to state its own capabilities — the field is
///    server-to-client only — so a forged claim is unexpressible rather than merely rejected.
/// 2. The client is not the authority. Even a client that **ignores** what arrived and submits
///    an `InsertText` is refused at the relay with `ODC-7004`, because the relay judges every
///    submission against its own copy of the grant.
#[test]
fn a_grant_arriving_on_the_wire_narrows_and_cannot_widen() {
    let path = scratch("grant");
    Room::create(&path).expect("a room");
    let (ancestor, paragraphs) = seed(1);
    let running = Running::start(&path, Access::Open(Capabilities::viewer()), 1);

    let mut reader = Peer::join(running.address, "guest", &ancestor);
    assert_eq!(
        reader.session.capabilities(),
        Capabilities::viewer(),
        "the welcome must carry the room's ceiling, not the client's hope"
    );
    assert!(
        !reader.session.capabilities().may_edit(),
        "a viewer room must not welcome an editor"
    );

    // A client that ignores its own answer is still refused by the relay.
    reader.edit("Typing", vec![typing(paragraphs[0], 0, "NOT ALLOWED")]);
    assert!(reader.send(), "the client is free to try");
    let refusal = reader.pump_until(|message| matches!(message, ServerMessage::Refused { .. }));
    assert!(
        matches!(
            refusal,
            ServerMessage::Refused {
                reason: Refusal::ReadOnlyAccess,
                ..
            }
        ),
        "a write from a viewer must be refused at the relay, got {refusal:?}"
    );
    assert_eq!(Refusal::ReadOnlyAccess.code(), "ODC-7004");

    drop(reader.writer);
    drop(reader.frames);
    drop(reader.socket);
    running.stop();
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------
// Eviction, from the room's side
// ---------------------------------------------------------------------------------------

/// **A participant whose socket dies leaves the room, and the room survives it.**
///
/// # Two mechanisms reach this, which is why the guard asserts the outcome and not one of them
///
/// `152` §2c's back-pressure policy is **eviction on a failed write**. Over a real socket there
/// is also a second route to the same outcome, and both were measured here rather than assumed:
///
/// - the connection's own **reader** sees the socket end and `Relay::disconnected` removes the
///   participant. This is what happens in the ordinary case — with nothing mutated, no
///   `Notice::Evicted` is produced at all;
/// - the **fan-out** finds the write refused and `evict_unreachable` removes the participant.
///   Neutering `disconnected` leaves this guard **green**, which is the evidence that the
///   eviction path is genuinely reachable over a socket and not only over a failing `Vec`.
///
/// So the guarantee asserted here is the one a reader of the document cares about and the one
/// both paths owe: a participant whose socket is gone **stops being in the room**, and the
/// participant still writing is acknowledged rather than blocked behind the dead one. Pinning
/// *which* mechanism did it would be a guard on the circumstance rather than the guarantee, and
/// it would redden the day a scheduler changed which of the two won the race.
///
/// The mutation that reddens it is therefore the one line both paths share — `Participants::left`
/// returning `contains_key` instead of removing — measured at `left: 2, right: 1`.
///
/// The **client's** half — what a disconnected client does about it and how its reader is told —
/// is the transport's, and is guarded in `webapp/tests/collab_transport.test.mjs`. From here an
/// evicted client is indistinguishable from any other dropped connection, and that is the honest
/// answer rather than a gap: eviction *is* a failed write, so there is no socket left to send a
/// refusal down.
#[test]
fn a_participant_whose_socket_dies_leaves_the_room_and_the_room_survives() {
    let path = scratch("evict");
    Room::create(&path).expect("a room");
    let (ancestor, paragraphs) = seed(1);
    let running = Running::start(&path, Access::Open(Capabilities::owner()), 2);

    let mut ada = Peer::join(running.address, "ada", &ancestor);
    let grace = Peer::join(running.address, "grace", &ancestor);
    assert_eq!(
        running
            .relay
            .lock()
            .expect("the room lock")
            .participants_mut()
            .len(),
        2,
        "both connections must be in the room before one of them dies"
    );

    // Grace's socket is torn down without a close frame: the condition the policy is about.
    let _ = grace.socket.shutdown(std::net::Shutdown::Both);
    drop(grace);

    // Ada keeps writing. Bounded rather than timed: the guarantee is that the room drops her,
    // and the relay's lock is what serialises her removal against Ada's next chunk, so a bound
    // on chunks is a bound on the only thing that has to happen.
    let mut remaining = 2;
    for round in 0..12_u32 {
        ada.edit(
            "Typing",
            vec![typing(
                paragraphs[0],
                0,
                if round % 2 == 0 { "x" } else { "y" },
            )],
        );
        assert!(ada.send(), "ada may always submit");
        ada.pump_until(|message| matches!(message, ServerMessage::Ack { .. }));
        remaining = running
            .relay
            .lock()
            .expect("the room lock")
            .participants_mut()
            .len();
        if remaining == 1 {
            break;
        }
    }
    assert_eq!(
        remaining, 1,
        "a participant whose socket is gone is still in the room, so every future chunk is \
         written to a dead socket and reported as failed forever"
    );
    // The other half of the policy: the author of the chunk that met the dead socket is
    // acknowledged, not blocked behind it.
    assert!(!ada.session.has_unacknowledged());
    assert!(!ada.session.is_desynced());

    drop(ada.writer);
    drop(ada.frames);
    drop(ada.socket);
    running.stop();
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------
// The budget — `docs/107` §4 B1, with the transport attached
// ---------------------------------------------------------------------------------------

/// **One keystroke costs the same on the wire however big the document and however long the
/// session.**
///
/// `docs/107` §4 B1 in its strong form: per-interaction work is O(1) in document size. The
/// transport is new work on that path — a flush, an encode and a socket write per chunk — so the
/// budget is re-asserted *with it attached* rather than assumed to be unaffected.
///
/// Guarded as a **complexity** claim and not a millisecond one (SKILL §8): the document grows by
/// a factor of eight, the session by a factor of eight, and the bytes one keystroke puts on the
/// wire must not grow at all. A timing threshold could not tell a slow constant from a linear
/// one, and this can.
///
/// Two dimensions, because they fail for different reasons. **Document size** catches a client
/// that carries context proportional to what it is editing. **Session history** catches the
/// mistake that has an obvious shape — a flush that forgets where it got to and re-offers
/// everything it has ever sent, which is O(session) per keystroke and looks perfectly correct
/// until a document has been open for an hour.
#[test]
fn a_keystrokes_wire_cost_grows_with_neither_the_document_nor_the_session() {
    /// The frame one keystroke puts on the wire, in a document of `paragraphs` after
    /// `already` acknowledged keystrokes.
    fn one_keystroke(paragraphs: usize, already: u32) -> (usize, usize, usize) {
        let path = scratch(&format!("budget-{paragraphs}-{already}"));
        Room::create(&path).expect("a room");
        let (ancestor, nodes) = seed(paragraphs);
        let running = Running::start(&path, Access::Open(Capabilities::owner()), 1);
        let mut ada = Peer::join(running.address, "ada", &ancestor);
        for _ in 0..already {
            ada.edit("Typing", vec![typing(nodes[0], 0, "o")]);
            assert!(ada.send());
            ada.pump_until(|message| matches!(message, ServerMessage::Ack { .. }));
        }
        ada.edit("Typing", vec![typing(nodes[0], 0, "k")]);
        let submission = ada
            .session
            .flush(&ada.log)
            .expect("one keystroke is one chunk");
        let measure = (
            submission.operations.len(),
            submission
                .operations
                .iter()
                .map(|operation| operation.carried_bytes())
                .sum::<usize>(),
            // The frame a keystroke actually puts on the wire, envelope included.
            encode_frame(&ClientMessage::Submit(submission)).len(),
        );
        drop(ada.writer);
        drop(ada.frames);
        drop(ada.socket);
        running.stop();
        let _ = std::fs::remove_file(&path);
        measure
    }

    let small = one_keystroke(8, 0);
    let large = one_keystroke(64, 0);
    assert_eq!(
        small, large,
        "a keystroke's chunk grew with the document: {small:?} then {large:?} — the transport \
         is carrying something proportional to the document, which `docs/107` §4 B1 forbids"
    );
    let fresh = one_keystroke(8, 1);
    let worn = one_keystroke(8, 8);
    assert_eq!(
        fresh, worn,
        "a keystroke's chunk grew with the session's history: {fresh:?} then {worn:?} — a \
         flush is re-offering work it has already had acknowledged, which costs O(session) per \
         keystroke and converges to a document that will not load"
    );
}
