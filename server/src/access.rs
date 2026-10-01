// SPDX-License-Identifier: Apache-2.0

//! How a room decides what an arriving participant may do — ADR-060, `152` §10 Q4.
//!
//! # The split, in one sentence each
//!
//! The **host** signs a grant. This **boundary** verifies it. The **engine**
//! ([`casual_doc_edit::access`]) enforces the capabilities that came back. `143` §10 wrote that
//! down as five enforcement layers; this file is the second one, and it is the only one that can
//! hold a line against a client somebody rewrote.
//!
//! # Why the engine cannot do this
//!
//! Verification needs a **key** and a **clock**, and `casual_doc_transaction` holds neither on
//! purpose — the same rule that makes a participant number assigned rather than minted
//! (`protocol`'s module docs). A signature check in the engine would also put a cryptographic
//! dependency in every embedder's bundle, including the ones that already have an identity
//! system and a signing key and want to use them. So the engine takes
//! [`Capabilities`] as an argument and this crate decides
//! where they came from.
//!
//! # Why there is no built-in signature profile, and what that does and does not mean
//!
//! `143` §16 Q5 is explicitly open: JWT, PASETO, or an opaque provider token, with the contract
//! specifying claims and behaviour rather than forcing an encoding. Picking one here means adding
//! a cryptographic dependency to this workspace, which is a decision with a supply-chain and a
//! `cargo deny` consequence and not a coding one. So what ships is the **seam**
//! ([`GrantVerifier`]) and two policies that are both explicit.
//!
//! What that does **not** mean is that nothing is enforced. [`Access::Open`] is a real,
//! reachable capability: a room opened with `Capabilities::commenter()` refuses every
//! participant's `InsertText` at the relay, with `ODC-7004`, whatever their client offered. What
//! it does mean is that an *open* room cannot tell two participants apart, which is why the
//! policy is a named argument rather than a default — see [`Access::Open`]'s own warning.
//!
//! # There is deliberately no default
//!
//! [`Relay::new`](crate::Relay::new) takes an [`Access`] and there is no `impl Default`. A
//! permissive default is how a boundary that does not exist gets advertised, and the working
//! contract's "never a dead control" has an exact analogue here: a permission check nobody
//! configured must fail loudly at the call site, as a missing argument, rather than quietly at
//! runtime as a room where everyone is an owner.

use casual_doc_edit::access::Capabilities;
use casual_doc_transaction::protocol::GrantToken;

/// Why a grant was not accepted.
///
/// **Deliberately undetailed on the wire.** Every variant becomes
/// [`Refusal::NotAuthorised`](casual_doc_transaction::protocol::Refusal::NotAuthorised) and
/// `ODC-7003`; the distinction exists for an operator reading a log, because detail that helps
/// an operator diagnose also helps an attacker enumerate. That is the rule `Refusal`'s own doc
/// comment already states, honoured rather than restated.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum GrantRefusal {
    /// The room requires a grant and the join presented none.
    Missing,
    /// The grant did not verify — a bad signature, an unknown key, a wrong audience.
    Invalid,
    /// The grant verified and has expired. Separate from [`GrantRefusal::Invalid`] because an
    /// operator seeing a run of these is looking at a clock or a token lifetime, not an attack.
    Expired,
}

impl core::fmt::Display for GrantRefusal {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Missing => {
                formatter.write_str("the room requires a grant and none was presented")
            }
            Self::Invalid => formatter.write_str("the grant did not verify"),
            Self::Expired => formatter.write_str("the grant has expired"),
        }
    }
}

impl core::error::Error for GrantRefusal {}

/// Turns a host-signed grant into the capabilities it carries.
///
/// Implemented by the host, or by a deployment's own binary. It is the only place a key, a clock
/// or a token format appears, and that containment is the point: everything above it deals in
/// [`Capabilities`], so the engine, the relay and the chrome are all unaware of which profile a
/// deployment chose.
///
/// # What an implementation must check
///
/// Enough that the answer is about *this* participant joining *this* room right now. At minimum:
/// the signature against a key the host holds; the audience, so a grant for one document cannot
/// open another; the expiry, against a real clock; and — if the deployment cares — the subject
/// against the [`Identity`](casual_doc_transaction::protocol::Identity) on the join, because a
/// grant for one person presented alongside somebody else's identity is not a grant for either.
///
/// # What it must not do
///
/// Return capabilities it did not read out of a verified token. A verifier that falls back to
/// "allow" on a parse failure is a verifier that turns every malformed byte into an owner, and it
/// is the single most likely way this seam gets implemented wrongly. The signature is
/// `Result<_, GrantRefusal>` rather than `Option` so that "I could not tell" has somewhere to go
/// other than a capability set.
pub trait GrantVerifier: Send + Sync + core::fmt::Debug {
    /// Verifies `token` and returns what it grants.
    ///
    /// # Errors
    ///
    /// [`GrantRefusal`] — all of which the relay answers as `NotAuthorised`.
    fn verify(&self, token: &GrantToken) -> Result<Capabilities, GrantRefusal>;
}

/// A room's access policy: how it answers "what may this participant do".
#[derive(Debug)]
#[non_exhaustive]
pub enum Access {
    /// **No authorisation.** Every participant is admitted with exactly these capabilities.
    ///
    /// A real and useful policy — a read-only broadcast room, a comment-only review link, a
    /// trusted network — and a real enforcement point: the relay refuses an operation outside the
    /// set, at the operation, with `ODC-7004`.
    ///
    /// What it cannot do is tell two participants apart. There is nothing to verify, so a grant
    /// presented to such a room is **ignored** rather than honoured, which is the conservative
    /// direction: an open room's ceiling is the value written here and a token cannot raise it.
    Open(Capabilities),
    /// A grant is **required**, and this verifier decides what it carries.
    ///
    /// A join with no grant is [`GrantRefusal::Missing`]; a join whose grant does not verify is
    /// whatever the verifier says. Either way the answer on the wire is `NotAuthorised` and the
    /// participant is not admitted, so no capability is ever inferred from a failure.
    Granted(Box<dyn GrantVerifier>),
}

impl Access {
    /// What `token` grants under this policy.
    ///
    /// # Errors
    ///
    /// [`GrantRefusal`] when a grant was required and did not satisfy the verifier.
    pub fn capabilities(&self, token: Option<&GrantToken>) -> Result<Capabilities, GrantRefusal> {
        match self {
            Self::Open(capabilities) => Ok(*capabilities),
            Self::Granted(verifier) => match token {
                Some(token) => verifier.verify(token),
                None => Err(GrantRefusal::Missing),
            },
        }
    }
}
