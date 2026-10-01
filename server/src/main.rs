// SPDX-License-Identifier: Apache-2.0

//! The relay binary.
//!
//! ```text
//! opendoc-relay create <journal>     create a room's durable log, and stop
//! opendoc-relay inspect <journal>    replay it and report what it holds, and stop
//! opendoc-relay serve <journal> <addr> [viewer|commenter|suggester|editor|owner]
//! ```
//!
//! # The role argument is required, and that is the point
//!
//! `serve` will not start without it. It is the room's
//! [`Access::Open`] ceiling — every participant gets exactly that
//! and the relay refuses anything outside it at the operation (ADR-060) — and an operator has to
//! name it, because a default would be a permission nobody chose. `viewer` is a read-only
//! broadcast room; `commenter` is a review link; `owner` is what an unauthenticated relay used to
//! be, now said out loud.
//!
//! **This binary verifies no grants**, so it cannot tell two participants apart. A deployment
//! that needs per-participant access implements
//! [`GrantVerifier`](opendoc_relay::GrantVerifier) and passes
//! [`Access::Granted`]; `143` §16 Q5 is why no signature profile
//! is built in.
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

use casual_doc_edit::access::Capabilities;
use casual_doc_transaction::codec::encode_frame;
use casual_doc_transaction::protocol::{ClientId, ClientMessage, Revision};
use opendoc_relay::transport::{Frames, ReadError};
use opendoc_relay::{Access, Recovered, Relay, Room};

fn main() -> std::process::ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: opendoc-relay create <journal> | inspect <journal> | \
                 serve <journal> <addr> <viewer|commenter|suggester|editor|owner>";
    let result = match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["create", journal] => create(journal),
        ["inspect", journal] => inspect(journal),
        ["serve", journal, address, role] => match open_room_role(role) {
            Some(access) => serve(journal, address, access),
            None => {
                eprintln!("{usage}");
                return std::process::ExitCode::from(2);
            }
        },
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
///
/// Everything this function decides is sockets and threads. The decisions that are hard to get
/// right are in `opendoc_relay::relay`, where a test can reach them.
/// The room's ceiling, by name. One role per capability preset and no way to spell a
/// combination: an operator choosing a room's policy from a command line should be choosing
/// between understood roles, and a host that needs an unusual set has
/// [`Access`] and the library.
fn open_room_role(role: &str) -> Option<Access> {
    let capabilities = match role {
        "viewer" => Capabilities::viewer(),
        "commenter" => Capabilities::commenter(),
        "suggester" => Capabilities::suggester(),
        "editor" => Capabilities::editor(),
        "owner" => Capabilities::owner(),
        _ => return None,
    };
    Some(Access::Open(capabilities))
}

fn serve(journal: &str, address: &str, access: Access) -> Result<(), Box<dyn core::error::Error>> {
    let (room, recovered) = Room::open(journal)?;
    report(&recovered);
    let relay = Arc::new(Mutex::new(Relay::new(room, access)));
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
    relay: &Mutex<Relay<TcpStream>>,
) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
    let fanned = stream.try_clone()?;
    let mut answers = stream.try_clone()?;
    let mut frames = Frames::new(stream);
    let mut writer = Some(fanned);
    let mut me: Option<ClientId> = None;
    let outcome = loop {
        let message: ClientMessage = match frames.next_frame() {
            Ok(message) => message,
            Err(ReadError::Closed) => break Ok(()),
            Err(error) => break Err(Box::new(error) as Box<dyn core::error::Error + Send + Sync>),
        };
        let handled = {
            // One lock across decide, journal, answer and fan out: two participants must not be
            // told about the order in two different orders.
            let mut relay = relay.lock().map_err(|_| "the room lock was poisoned")?;
            relay.handle(me, &mut writer, &message)?
        };
        if let Some(client) = handled.joined_as {
            me = Some(client);
        }
        for client in handled.behind {
            // Reported, not swallowed: each of these is behind the order, and `152` §5.5's resume
            // is how it catches up — which only happens if somebody noticed.
            eprintln!(
                "participant {} missed a chunk; it must resume",
                client.get()
            );
        }
        if let Some(answer) = handled.answer {
            answers.write_all(&encode_frame(&answer))?;
            answers.flush()?;
        }
        if handled.leaving {
            break Ok(());
        }
    };
    // Every exit path: a clean leave, a broken pipe, a refused frame. A writer left behind is a
    // socket every future chunk is written to and reported as failed forever.
    if let Some(client) = me
        && let Ok(mut relay) = relay.lock()
    {
        for missed in relay.disconnected(client) {
            eprintln!(
                "participant {} was not told that {} left",
                missed.get(),
                client.get()
            );
        }
    }
    outcome
}
