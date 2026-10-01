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
use casual_doc_transaction::protocol::{ClientMessage, Outcome, Revision, ServerMessage};
use opendoc_relay::transport::{Frames, ReadError};
use opendoc_relay::{Recovered, Room};

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
    let room = Arc::new(Mutex::new(room));
    let listener = TcpListener::bind(address)?;
    println!("relaying {journal} on {}", listener.local_addr()?);
    for stream in listener.incoming() {
        let stream = stream?;
        let room = Arc::clone(&room);
        std::thread::spawn(move || {
            if let Err(error) = participant(stream, &room) {
                eprintln!("participant ended: {error}");
            }
        });
    }
    Ok(())
}

fn participant(
    stream: TcpStream,
    room: &Mutex<Room>,
) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
    let mut writer = stream.try_clone()?;
    let mut frames = Frames::new(stream);
    loop {
        let message: ClientMessage = match frames.next_frame() {
            Ok(message) => message,
            Err(ReadError::Closed) => return Ok(()),
            Err(error) => return Err(Box::new(error)),
        };
        let answer = {
            // The lock is held across the decision and its journal write, which is what makes
            // "journal first, answer second" true under concurrency as well as in one thread.
            let mut room = room.lock().map_err(|_| "the room lock was poisoned")?;
            match &message {
                ClientMessage::Join(_) => Some(room.join(&message)),
                ClientMessage::Submit(submission) => match room.commit(submission)? {
                    Outcome::Ordered { revision } | Outcome::Duplicate { revision } => {
                        Some(ServerMessage::Ack {
                            through: submission.seq,
                            revision,
                        })
                    }
                    Outcome::Refused { reason } => Some(ServerMessage::Refused {
                        seq: Some(submission.seq),
                        reason,
                    }),
                },
                // Presence is not ordered, never acknowledged and never retried (`152` §2b), so
                // the relay has nothing to say about one. Fanning it out needs the roster this
                // increment does not build; dropping it is honest, and answering it would not be.
                ClientMessage::Presence(_) => None,
                ClientMessage::Leave => return Ok(()),
            }
        };
        if let Some(answer) = answer {
            writer.write_all(&encode_frame(&answer))?;
            writer.flush()?;
        }
    }
}
