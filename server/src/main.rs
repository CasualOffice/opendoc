// SPDX-License-Identifier: Apache-2.0

//! The relay binary.
//!
//! ```text
//! opendoc-relay create <journal>     create a room's durable log, and stop
//! opendoc-relay inspect <journal>    replay it and report what it holds, and stop
//! opendoc-relay serve <journal> <addr>
//! ```
//!
//! `create` is separate from `serve` on purpose: `152` records that **the host creates the
//! room, not the first client**, and a binary that created one on first connection would be a
//! binary that made the rule unenforceable. `inspect` exists because a durable log that cannot
//! be read without starting a server is a log nobody will check.

// A command-line tool talks to a terminal; the workspace lint that forbids this exists for the
// libraries, where a stray print is a library writing to somebody else's stdout.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::Write as _;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use casual_doc_transaction::codec::encode_frame;
use casual_doc_transaction::protocol::{
    Arrival, ClientId, ClientMessage, Outcome, Revision, ServerMessage,
};
use opendoc_relay::transport::{Frames, ReadError};
use opendoc_relay::{Participants, Recovered, Room};

/// The room and the connected participants behind **one** lock.
///
/// One lock and not two, deliberately: the decision, its journal write and the fan-out have to
/// happen without another chunk interleaving, or two participants can be told about the order in
/// two different orders. Holding one lock across all three makes "journal first, answer second,
/// then fan out" true under concurrency rather than only in one thread. The relay's per-message
/// work is an append and N writes and never a document, so the lock is held briefly by
/// construction (ADR-047, ADR-058).
struct Relay {
    room: Room,
    participants: Participants<TcpStream>,
}

fn main() -> std::process::ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let usage =
        "usage: opendoc-relay create <journal> | inspect <journal> | serve <journal> <addr>";
    let result = match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["create", journal] => create(journal),
        ["inspect", journal] => inspect(journal),
        ["serve", journal, address] => serve(journal, address),
        _ => {
            eprintln!("{usage}");
            return std::process::ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("opendoc-relay: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn create(journal: &str) -> Result<(), Box<dyn core::error::Error>> {
    Room::create(journal)?;
    println!("created a room at {journal}");
    Ok(())
}

fn inspect(journal: &str) -> Result<(), Box<dyn core::error::Error>> {
    let (room, recovered) = Room::open(journal)?;
    report(&recovered);
    let retained = room
        .history_since(Revision::new(0))
        .map_or(0, |entries| entries.len());
    println!("ordered entries retained: {retained}");
    Ok(())
}

fn report(recovered: &Recovered) {
    println!("replayed {} ordered chunks", recovered.replayed);
    if recovered.discarded_tail_bytes > 0 {
        // Reported rather than hidden: it is the only evidence the previous process did not
        // shut down cleanly.
        println!(
            "discarded {} bytes of a partial final frame (the last run did not shut down cleanly)",
            recovered.discarded_tail_bytes
        );
    }
}

/// Thread-per-connection, deliberately — see `opendoc_relay::transport` for what that costs and
/// why a dumb relay is the one shape it suits.
fn serve(journal: &str, address: &str) -> Result<(), Box<dyn core::error::Error>> {
    let (room, recovered) = Room::open(journal)?;
    report(&recovered);
    let relay = Arc::new(Mutex::new(Relay {
        room,
        participants: Participants::new(),
    }));
    let listener = TcpListener::bind(address)?;
    println!("relaying {journal} on {}", listener.local_addr()?);
    for stream in listener.incoming() {
        let stream = stream?;
        let relay = Arc::clone(&relay);
        std::thread::spawn(move || {
            if let Err(error) = participant(stream, &relay) {
                eprintln!("participant ended: {error}");
            }
        });
    }
    Ok(())
}

fn participant(
    stream: TcpStream,
    relay: &Mutex<Relay>,
) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
    let writer = stream.try_clone()?;
    let mut answers = stream.try_clone()?;
    let mut frames = Frames::new(stream);
    // Set on the Join, and the only thing that stops this connection being fanned its own chunk
    // back. Until it is known this connection is not in the participant set at all.
    let mut me: Option<ClientId> = None;
    let outcome = serve_participant(&mut frames, &mut answers, writer, relay, &mut me);
    // Whatever ended this connection — a clean leave, a broken pipe, a refused frame — the
    // participant must leave the set. A writer left behind is a socket every future chunk is
    // written to, and `Participants::fan_out` would report it as failed forever.
    if let Some(client) = me
        && let Ok(mut relay) = relay.lock()
    {
        relay.participants.left(client);
    }
    outcome
}

fn serve_participant(
    frames: &mut Frames<TcpStream>,
    answers: &mut TcpStream,
    writer: TcpStream,
    relay: &Mutex<Relay>,
    me: &mut Option<ClientId>,
) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
    let mut writer = Some(writer);
    loop {
        let message: ClientMessage = match frames.next_frame() {
            Ok(message) => message,
            Err(ReadError::Closed) => return Ok(()),
            Err(error) => return Err(Box::new(error)),
        };
        let answer = {
            let mut relay = relay.lock().map_err(|_| "the room lock was poisoned")?;
            match &message {
                ClientMessage::Join(_) => {
                    let answer = relay.room.join(&message);
                    // The participant set is keyed by the id the relay just assigned — the same
                    // id a resume hands back, which is why `joined` replaces rather than refuses.
                    if let ServerMessage::Welcome { client, .. }
                    | ServerMessage::Resumed { client, .. } = answer
                        && let Some(writer) = writer.take()
                    {
                        *me = Some(client);
                        relay.participants.joined(client, writer);
                    }
                    Some(answer)
                }
                ClientMessage::Submit(submission) => {
                    // Journal first (inside `Room::commit`), answer second, fan out third.
                    match relay.room.commit(submission)? {
                        Outcome::Ordered { revision } => {
                            let arrival = Arrival {
                                revision,
                                client: submission.client,
                                operations: submission.operations.clone(),
                            };
                            let bytes = encode_frame(&ServerMessage::Apply(arrival));
                            let behind = relay.participants.fan_out(submission.client, &bytes);
                            // Reported, not swallowed: each of these is now behind the order, and
                            // `152` §5.5's resume is how it catches up — which only happens if
                            // somebody noticed.
                            for client in behind {
                                eprintln!(
                                    "participant {} missed revision {}; it must resume",
                                    client.get(),
                                    revision.get()
                                );
                            }
                            Some(ServerMessage::Ack {
                                through: submission.seq,
                                revision,
                            })
                        }
                        // A duplicate is acknowledged and **not** fanned out again: everybody
                        // already has it, and a second copy would be applied twice.
                        Outcome::Duplicate { revision } => Some(ServerMessage::Ack {
                            through: submission.seq,
                            revision,
                        }),
                        Outcome::Refused { reason } => Some(ServerMessage::Refused {
                            seq: Some(submission.seq),
                            reason,
                        }),
                    }
                }
                // Presence is not ordered, never acknowledged and never retried (`152` §2b). The
                // relay has nothing to say about one; fanning it out needs the roster this
                // increment does not wire, and answering it would be worse than dropping it.
                ClientMessage::Presence(_) => None,
                ClientMessage::Leave => return Ok(()),
            }
        };
        if let Some(answer) = answer {
            answers.write_all(&encode_frame(&answer))?;
            answers.flush()?;
        }
    }
}
