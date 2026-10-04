// SPDX-License-Identifier: Apache-2.0

//! The optional collaboration relay — `107` 6.6.
//!
//! # What this is, and the one sentence that constrains all of it
//!
//! ADR-047: **a dumb relay.** It assigns an order to the chunks clients offer, remembers the
//! order, and fans each chunk out untouched. It holds **no document**, runs **no transform**,
//! and understands **nothing** about what an operation means. The state machine that does the
//! ordering is `casual_doc_transaction::session::ServerSession`, which lives in the engine crate
//! and is tested there against two real replicas in one process; this member adds durability
//! (`journal`), a room, and a transport, and nothing else.
//!
//! # Why it is a workspace member under `server/` and not a crate
//!
//! **No mandatory server** is a structural property of this project, not a preference
//! (`10`, `AGENTS.md`). A relay that `crates/` could depend on would be a relay that could
//! become required by accident — one `use` at a time — and the licence-and-embeddability
//! position this project occupies does not survive that. So the direction of the dependency is
//! a build-level fact: `server/` depends on the engine; the engine cannot see `server/`.
//!
//! `nothing_under_crates_depends_on_the_relay` fails the build if that reverses. It reads the
//! manifests rather than the source, because a `path` dependency is how it would actually
//! happen.
//!
//! # One doc, one room
//!
//! The owner's rule, and it is enforced here by what the API makes possible rather than by
//! documentation: [`Room::create`] is how a room begins, and it takes a **host's** decision and
//! a path. A client cannot create one. That is deliberate and `152` records why — a document
//! reached by a link or embedded by a host joins a room from the first open, **even alone**,
//! because a disconnected replica can never learn that somebody else arrived. A standalone
//! document needs no server at all and never comes here.
//!
//! # What a lone writer pays
//!
//! Nothing beyond one round trip per chunk. `152` §10 Q3 measured the contention cost at
//! exactly `(W − 1) / 2` wasted round trips per chunk that lands, and
//! `a_single_writer_is_never_refused_by_the_ordering_rule` pins the `W = 1` case separately.
//! The relay adds one `fsync` per ordered chunk, and that is the whole of what durability costs.

pub mod access;
pub mod fanout;
pub mod journal;
pub mod relay;
pub mod room;
pub mod serve;
pub mod transport;
pub mod websocket;

pub use access::{Access, GrantRefusal, GrantVerifier};
pub use fanout::Participants;
pub use journal::{Journal, JournalError, Recovered};
pub use relay::{Handled, Relay};
pub use room::{Room, RoomError};
pub use serve::{Notice, accept_loop, participant};

#[cfg(test)]
mod policy_tests;
