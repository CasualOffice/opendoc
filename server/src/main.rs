// SPDX-License-Identifier: Apache-2.0

//! The relay binary.
//!
//! ```text
//! opendoc-relay create <journal>     create a room's durable log, and stop
//! opendoc-relay inspect <journal>    replay it and report what it holds, and stop
//! opendoc-relay serve <journal> <addr> [viewer|commenter|suggester|editor|owner]
//! ```
//!
//! # `serve` speaks WebSocket and nothing else
//!
//! The client is a browser, and a browser cannot open a TCP socket. So `serve` performs the
//! RFC 6455 handshake on every connection and carries `ODC1` frames as binary messages —
//! `opendoc_relay::websocket` records why that is a framing adaptor rather than a dependency,
//! and ADR-063 records the decision it replaces.
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

use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use casual_doc_edit::access::Capabilities;
use casual_doc_transaction::protocol::Revision;
use opendoc_relay::{Access, Notice, Recovered, Relay, Room, accept_loop};

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
    println!(
        "relaying {journal} on ws://{} (WebSocket, protocol {})",
        listener.local_addr()?,
        casual_doc_transaction::protocol::PROTOCOL_VERSION
    );
    // The loop, the threads and the per-connection decisions are all in the library now, where
    // a test can drive two real clients through one real socket. What is left here is printing,
    // which is the one thing a library must not do.
    let notice: Arc<dyn Fn(Notice) + Send + Sync> = Arc::new(|notice| match notice {
        Notice::Evicted { client } => eprintln!(
            "participant {} could not be written to and was removed from the room; it \
             resumes to catch up",
            client.get()
        ),
        Notice::DepartureUndelivered { missed, departed } => eprintln!(
            "participant {} was not told that {} left",
            missed.get(),
            departed.get()
        ),
        Notice::Ended { detail } => eprintln!("participant ended: {detail}"),
        // `Notice` is `#[non_exhaustive]`, so a variant added later compiles here and is
        // reported rather than silently dropped. The `{notice:?}` is the evidence an operator
        // needs until somebody writes a sentence for it.
        other => eprintln!("relay: {other:?}"),
    });
    accept_loop(&listener, &relay, &notice)?;
    Ok(())
}
