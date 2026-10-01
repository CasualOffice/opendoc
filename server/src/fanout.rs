// SPDX-License-Identifier: Apache-2.0

//! Who is connected, and writing one ordered chunk to all of them but its author.
//!
//! # Why this is a type and not three lines in the binary
//!
//! Fan-out has exactly two rules and both are easy to get wrong in a way no compiler catches:
//!
//! 1. **The author is excluded.** A client that received its own chunk back would apply its own
//!    edit twice. It already has it; what it is waiting for is an acknowledgement, which is a
//!    different message on a different path.
//! 2. **A write that fails must be reported, never swallowed.** A participant whose socket has
//!    gone is a participant who is now **behind the order**, and `152` §5.5's resume path exists
//!    precisely so that it can be caught up — but only if somebody noticed. A `let _ =` on a
//!    failed write turns a reconnectable client into a silently diverged one.
//!
//! Generic over the writer so both of those are testable against a `Vec<u8>` rather than against
//! a socket. A rule that can only be exercised by opening a port is a rule that gets exercised
//! by hand, once.
//!
//! # What this deliberately is not
//!
//! It is not a queue. A slow participant blocks the fan-out for the duration of its own write,
//! because the relay holds one lock across the decision, the journal and the fan-out — which is
//! what makes "journal first, answer second" true under concurrency rather than only in one
//! thread. Buffering per participant would need back-pressure and a policy for a reader that
//! never drains, and both of those are real designs rather than details. `ADR-058` records the
//! thread-per-connection choice this sits inside, and the same sentence applies: it is an
//! optimisation of a working mechanism.

use std::collections::BTreeMap;
use std::io::Write;

use casual_doc_transaction::protocol::ClientId;

/// Everyone connected, and where to write to them.
#[derive(Debug, Default)]
pub struct Participants<W> {
    writers: BTreeMap<ClientId, W>,
}

impl<W: Write> Participants<W> {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            writers: BTreeMap::new(),
        }
    }

    /// Records that `client` is connected, replacing any writer already held for it.
    ///
    /// Replacing rather than refusing, because `152` §5.5's resume hands a reconnecting client
    /// **the same** [`ClientId`] — that is what keeps `(client, seq)` suppressing duplicates — so
    /// a second writer for one id is a reconnection and not a mistake.
    pub fn joined(&mut self, client: ClientId, writer: W) {
        self.writers.insert(client, writer);
    }

    /// Forgets `client`. `true` if it was there.
    pub fn left(&mut self, client: ClientId) -> bool {
        self.writers.remove(&client).is_some()
    }

    /// Forgets `client` and hands back its writer.
    ///
    /// What a caller needs in order to shut a connection down deliberately rather than by
    /// dropping it, and the only way to look at what a participant received — there is no getter
    /// for that, because one would exist for the tests alone.
    pub fn take(&mut self, client: ClientId) -> Option<W> {
        self.writers.remove(&client)
    }

    /// How many participants are connected.
    #[must_use]
    pub fn len(&self) -> usize {
        self.writers.len()
    }

    /// Whether nobody is connected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.writers.is_empty()
    }

    /// Writes `bytes` to every participant except `author`.
    ///
    /// Returns the clients whose write failed, **in order**. A caller must do something with
    /// them: they are behind the order now, and the resume path is how they get caught up.
    /// Returning them rather than logging them here keeps the policy — drop, retry, or stop the
    /// room — with the caller that knows which it wants.
    pub fn fan_out(&mut self, author: ClientId, bytes: &[u8]) -> Vec<ClientId> {
        let mut failed = Vec::new();
        for (client, writer) in &mut self.writers {
            if *client == author {
                continue;
            }
            if writer
                .write_all(bytes)
                .and_then(|()| writer.flush())
                .is_err()
            {
                failed.push(*client);
            }
        }
        failed
    }
}

#[cfg(test)]
#[path = "fanout_tests.rs"]
mod tests;
