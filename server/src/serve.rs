// SPDX-License-Identifier: Apache-2.0

//! Sockets and threads — **in the library, where a test can reach them.**
//!
//! # Why this moved out of the binary
//!
//! The same reason [`crate::relay`]'s own header gives for moving the message handling out of
//! it: "in a `main.rs` behind a `TcpStream` none of them can be tested at all". The loop here
//! is small but it holds three decisions a `Vec<u8>` double cannot exercise —
//!
//! 1. the WebSocket handshake happens before the first frame is read, and the bytes a client
//!    pipelined behind it are carried into the frame reader rather than lost;
//! 2. the answer to the sender is written **under the room lock**, with every other write to
//!    every other socket (see [`participant`] for why that is a correctness rule and not
//!    tidiness);
//! 3. **every** exit path forgets the connection, including the one where the journal failed.
//!
//! — and two of the three were wrong in the binary. A relay whose per-connection loop is
//! unreachable from a test is a relay whose convergence has never been observed, which is
//! precisely the state this lane found: `grep -rl 'WebSocket' webapp/src` returned nothing and
//! no test anywhere drove two clients through one socket.
//!
//! # The shape, named before any code
//!
//! **Thread-per-connection, with one lock per room**, unchanged from what
//! [`crate::transport`] documents and costs. This module adds no concurrency model; it only
//! makes the existing one addressable.
//!
//! # Complexity
//!
//! O(participants) per ordered chunk, which is the fan-out, and O(bytes) per frame. Nothing
//! here is O(document): the relay holds no document (ADR-047).

use std::io::Write as _;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use casual_doc_transaction::codec::encode_frame;
use casual_doc_transaction::protocol::{ClientId, ClientMessage};

use crate::relay::{Handled, Relay};
use crate::transport::{Frames, ReadError};
use crate::websocket::{self, Framed, Role, Unframed};

/// How long a connection may take to complete its handshake.
///
/// A connection that never finishes one holds a thread, and before the handshake there is
/// nobody to attribute it to — the same argument
/// [`MAX_HANDSHAKE_BYTES`](crate::websocket::MAX_HANDSHAKE_BYTES) makes about size, applied to
/// time. Cleared once the handshake is done, because an **established** session may legitimately
/// sit idle for as long as its reader is reading.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// The writer one participant's socket presents to the relay: a WebSocket message per frame.
pub type Socket = Framed<TcpStream>;

/// One room, shared by its connection threads.
pub type Shared = Mutex<Relay<Socket>>;

/// Something an operator should be told, carried out rather than printed.
///
/// A library that printed would be a library writing to somebody else's stdout — the workspace
/// lint that forbids it exists for exactly this — and a test that had to parse stdout to learn
/// that an eviction happened would be a test asserting a log format.
#[derive(Debug)]
#[non_exhaustive]
pub enum Notice {
    /// A participant could not be written to and was removed from the room.
    ///
    /// The relay has **already** removed it (`152` §2c); this is the evidence, not the policy.
    Evicted {
        /// Who was removed.
        client: ClientId,
    },
    /// Somebody was not told that somebody else left.
    DepartureUndelivered {
        /// Who did not hear it.
        missed: ClientId,
        /// Who left.
        departed: ClientId,
    },
    /// A connection ended with an error rather than by leaving.
    Ended {
        /// What happened, already rendered: the error types differ by path and an operator
        /// wants the sentence.
        detail: String,
    },
}

/// Accepts connections for ever, one thread each.
///
/// # Errors
///
/// The listener's, which is the only failure that ends the loop. A single connection's failure
/// is reported through `notice` and costs that connection only.
pub fn accept_loop(
    listener: &TcpListener,
    relay: &Arc<Shared>,
    notice: &Arc<dyn Fn(Notice) + Send + Sync>,
) -> std::io::Result<()> {
    for stream in listener.incoming() {
        let stream = stream?;
        let relay = Arc::clone(relay);
        let notice = Arc::clone(notice);
        std::thread::spawn(move || {
            let report: &dyn Fn(Notice) = notice.as_ref();
            if let Err(error) = participant(stream, &relay, report) {
                report(Notice::Ended {
                    detail: error.to_string(),
                });
            }
        });
    }
    Ok(())
}

/// Serves one connection: handshake, then frames, until it ends.
///
/// # Why the answer is written under the room lock
///
/// Because two threads writing to one socket can tear a frame, and a torn frame is not a
/// dropped message — it is a stream the peer can never resynchronise, since a self-describing
/// length framing has no delimiter to scan for. This loop's thread writes the **answer** to its
/// own participant; every other participant's thread writes the **fan-out** to it. Those are
/// different file descriptors onto one socket, so nothing but ordering stops them interleaving.
///
/// [`crate::relay`]'s header already states the contract — "decide → journal → answer → fan
/// out, all under one lock held by the caller" — and the binary this replaces wrote the answer
/// *outside* the lock, so the contract was documented and not kept. Holding the lock across the
/// answer costs what the fan-out already costs: a participant whose socket blocks blocks the
/// room. That exposure is unchanged and is what the eviction policy exists for; the tearing is
/// gone.
///
/// # Why every exit breaks rather than propagating
///
/// The binary used `?` on the journal failure, which returned from the function and skipped the
/// cleanup its own comment called mandatory — "a writer left behind is a socket every future
/// chunk is written to and reported as failed forever". A failure to journal is exactly when a
/// relay must not also leak a participant, so every path here reaches the bottom.
///
/// # Errors
///
/// A handshake refusal, a frame the codec refused, a poisoned lock, or a journal failure. The
/// connection has been forgotten by the time any of them is returned.
pub fn participant(
    stream: TcpStream,
    relay: &Shared,
    notice: &dyn Fn(Notice),
) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
    let mut opening = stream.try_clone()?;
    opening.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    let leftover = websocket::accept(&mut opening)?;
    // Cleared, not left: an established participant may sit idle, and a timeout that outlived
    // the handshake would disconnect a reader for reading.
    opening.set_read_timeout(None)?;

    let fanned = Framed::new(stream.try_clone()?, Role::Server);
    let mut answers = Framed::new(stream.try_clone()?, Role::Server);
    let mut frames = Frames::new(Unframed::new(stream, Role::Server, leftover));
    let mut writer = Some(fanned);
    let mut me: Option<ClientId> = None;

    let outcome = loop {
        let message: ClientMessage = match frames.next_frame() {
            Ok(message) => message,
            Err(ReadError::Closed) => break Ok(()),
            Err(error) => break Err(Box::new(error) as Box<dyn core::error::Error + Send + Sync>),
        };
        let handled = match answer_under_the_lock(relay, me, &mut writer, &message, &mut answers) {
            Ok(handled) => handled,
            Err(error) => break Err(error),
        };
        if let Some(client) = handled.joined_as {
            me = Some(client);
        }
        for client in handled.evicted {
            notice(Notice::Evicted { client });
        }
        if handled.leaving {
            break Ok(());
        }
    };

    if let Some(client) = me
        && let Ok(mut relay) = relay.lock()
    {
        for missed in relay.disconnected(client) {
            notice(Notice::DepartureUndelivered {
                missed,
                departed: client,
            });
        }
    }
    // A courtesy, and best-effort by construction: on the paths that end here the socket is
    // usually already gone. A peer that gets it sees a departure rather than a reset, which is
    // the difference between `ReadError::Closed` and `ReadError::Interrupted` on its side.
    let _ = answers.close();
    outcome
}

/// Handles one message and answers its sender, with the room lock held across both.
fn answer_under_the_lock(
    relay: &Shared,
    me: Option<ClientId>,
    writer: &mut Option<Socket>,
    message: &ClientMessage,
    answers: &mut Socket,
) -> Result<Handled, Box<dyn core::error::Error + Send + Sync>> {
    let mut room = relay.lock().map_err(|_| "the room lock was poisoned")?;
    let handled = room.handle(me, writer, message)?;
    if let Some(answer) = handled.answer.as_ref() {
        answers.write_all(&encode_frame(answer))?;
        answers.flush()?;
    }
    Ok(handled)
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
