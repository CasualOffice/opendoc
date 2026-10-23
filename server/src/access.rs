// SPDX-License-Identifier: Apache-2.0

//! How a room decides who an arriving participant is and what they may do — ADR-060, ADR-070,
//! `152` §10 Q4, `docs/175` §4.
//!
//! # The split, in one sentence each
//!
//! The **host** signs a grant. This **boundary** verifies it. The **engine**
//! ([`casual_doc_edit::access`]) enforces the capabilities that came back, and stamps every chunk
//! with the profile that came back. `143` §10 wrote that down as five enforcement layers; this
//! file is the second one, and it is the only one that can hold a line against a client somebody
//! rewrote.
//!
//! # Why the engine cannot do this
//!
//! Verification needs a **key** and a **clock**, and `casual_doc_transaction` holds neither on
//! purpose — the same rule that makes a participant number assigned rather than minted
//! (`protocol`'s module docs). A signature check in the engine would also put a cryptographic
//! dependency in every embedder's bundle. So the engine takes [`Capabilities`] and a profile as
//! arguments and this crate decides where they came from.
//!
//! # The grant is a JWT, and the relay holds only a public key
//!
//! `143` §16 Q5 left the encoding open; the owner closed it on 2026-10-23 (`docs/175` §12 Q6):
//! **JWT**, signed by the host with an asymmetric key, so a relay that is compromised cannot mint
//! grants. [`JwtVerifier`] is that profile, built in: EdDSA over Ed25519 (RFC 8037) and nothing
//! else, because a verifier that accepts several algorithms is a verifier an attacker picks the
//! weakest of (and `alg: none` is the textbook case). [`GrantVerifier`] stays the seam, so a
//! deployment with its own identity system and key type implements it instead.
//!
//! # The rule this file now holds, and used to leave optional
//!
//! Under [`Access::Granted`] a join's identity **must equal the grant's subject**, or the join is
//! refused. A grant for one person presented with somebody else's identity is not a grant for
//! either. This used to be documented as something a verifier might check "if the deployment
//! cares"; it is now the boundary's, so no verifier can forget it ([`Access::admit`]).
//!
//! # There is deliberately no default
//!
//! [`Relay::new`](crate::Relay::new) takes an [`Access`] and there is no `impl Default`. A
//! permissive default is how a boundary that does not exist gets advertised.

use casual_doc_edit::access::Capabilities;
use casual_doc_transaction::protocol::{GrantToken, Identity, Join, Profile, ProfileField};

/// Why a grant was not accepted.
///
/// **Deliberately undetailed on the wire.** Every variant becomes
/// [`Refusal::NotAuthorised`](casual_doc_transaction::protocol::Refusal::NotAuthorised) and
/// `ODC-7003`; the distinction exists for an operator reading a log, because detail that helps
/// an operator diagnose also helps an attacker enumerate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum GrantRefusal {
    /// The room requires a grant and the join presented none.
    Missing,
    /// The grant did not verify — a bad signature, an unknown key, a wrong audience, a claim
    /// that is missing or malformed.
    Invalid,
    /// The grant verified and has expired, or is not valid yet. Separate from
    /// [`GrantRefusal::Invalid`] because an operator seeing a run of these is looking at a clock
    /// or a token lifetime, not an attack.
    Expired,
    /// The grant verified, and names somebody other than the identity the join presented
    /// (`docs/175` §4.1). The impersonation attempt.
    SubjectMismatch,
}

impl core::fmt::Display for GrantRefusal {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Missing => {
                formatter.write_str("the room requires a grant and none was presented")
            }
            Self::Invalid => formatter.write_str("the grant did not verify"),
            Self::Expired => formatter.write_str("the grant has expired or is not valid yet"),
            Self::SubjectMismatch => {
                formatter.write_str("the grant names a different person than the join")
            }
        }
    }
}

impl core::error::Error for GrantRefusal {}

/// What a verified grant says: what the person may do, who they are, and what they look like.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Verified {
    /// What the grant allows.
    pub capabilities: Capabilities,
    /// Whom the grant was issued to. **Required**: a verifier that cannot say whom a grant is
    /// for cannot stop one person's grant being used under another's name.
    pub subject: Identity,
    /// The display claims the host signed, if any. Its `id` must be the subject.
    pub profile: Option<Profile>,
}

impl Verified {
    /// A verified grant for `subject`, with no profile.
    #[must_use]
    pub const fn new(capabilities: Capabilities, subject: Identity) -> Self {
        Self {
            capabilities,
            subject,
            profile: None,
        }
    }

    /// The same, with the profile the host signed.
    #[must_use]
    pub fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
        self
    }
}

/// Turns a host-signed grant into what it says.
///
/// Implemented by [`JwtVerifier`], or by a deployment's own type. It is the only place a key, a
/// clock or a token format appears.
///
/// # What an implementation must check
///
/// Enough that the answer is about *this* participant joining *this* room right now: the
/// signature against a key the host holds; the audience, so a grant for one document cannot
/// open another; the expiry, against a real clock. The subject is **returned**, not checked
/// here: [`Access::admit`] compares it with the join for every verifier alike.
///
/// # What it must not do
///
/// Return anything it did not read out of a verified token. A verifier that falls back to
/// "allow" on a parse failure turns every malformed byte into an owner.
pub trait GrantVerifier: Send + Sync + core::fmt::Debug {
    /// Verifies `token` and returns what it says.
    ///
    /// # Errors
    ///
    /// [`GrantRefusal`] — all of which the relay answers as `NotAuthorised`.
    fn verify(&self, token: &GrantToken) -> Result<Verified, GrantRefusal>;
}

/// A room's access policy: how it answers "who is this, and what may they do".
#[derive(Debug)]
#[non_exhaustive]
pub enum Access {
    /// **No authorisation.** Every participant is admitted with exactly these capabilities.
    ///
    /// A real and useful policy — a read-only broadcast room, a comment-only review link, a
    /// trusted network. What it cannot do is tell two participants apart, so a grant presented
    /// to such a room is **ignored**, and a profile the participant claims is fanned out
    /// **marked unverified**, with its colour and avatar dropped (`docs/175` §4.2).
    Open(Capabilities),
    /// A grant is **required**, and this verifier decides what it carries. The profile everyone
    /// sees is the grant's; anything the client claimed is ignored.
    Granted(Box<dyn GrantVerifier>),
}

/// What [`Access::admit`] decided for one join.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Admitted {
    /// What the participant may do.
    pub capabilities: Capabilities,
    /// What they look like to everybody else, or `None` for a participant nobody named.
    pub profile: Option<Profile>,
    /// Whether the host vouched for the profile (a verified grant), or the participant merely
    /// claimed it (an open room).
    pub attested: bool,
}

/// Why a join was not admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum JoinRefusal {
    /// The grant was missing, invalid, expired or for somebody else — `ODC-7003` on the wire.
    Grant(GrantRefusal),
    /// A profile broke a bound (`docs/175` §4.4) — `ODC-7007` on the wire: the message could
    /// not be accepted as sent, and sending it again will not help.
    Profile(ProfileField),
}

impl Access {
    /// Who `join` is and what they may do under this policy — `docs/175` §4.
    ///
    /// Under [`Access::Granted`], **`join.identity` must equal the grant's subject**, and the
    /// grant's profile must be for that subject too; the profile the client sent is ignored.
    /// Under [`Access::Open`], the client's claimed profile is checked against its bounds and
    /// returned [`Profile::claimed`], unattested.
    ///
    /// # Errors
    ///
    /// [`JoinRefusal`].
    pub fn admit(&self, join: &Join) -> Result<Admitted, JoinRefusal> {
        match self {
            Self::Open(capabilities) => {
                let profile = match &join.profile {
                    Some(claimed) => {
                        claimed.check().map_err(JoinRefusal::Profile)?;
                        // The claim's id is the join's identity, whatever the client wrote: a
                        // participant cannot claim to be a second person's id either.
                        let mut claimed = claimed.clone().claimed();
                        join.identity.as_str().clone_into(&mut claimed.id);
                        claimed.check().map_err(JoinRefusal::Profile)?;
                        Some(claimed)
                    }
                    None => None,
                };
                Ok(Admitted {
                    capabilities: *capabilities,
                    profile,
                    attested: false,
                })
            }
            Self::Granted(verifier) => {
                let token = join
                    .grant
                    .as_ref()
                    .ok_or(JoinRefusal::Grant(GrantRefusal::Missing))?;
                let verified = verifier.verify(token).map_err(JoinRefusal::Grant)?;
                if verified.subject != join.identity {
                    return Err(JoinRefusal::Grant(GrantRefusal::SubjectMismatch));
                }
                if let Some(profile) = &verified.profile {
                    if profile.id != verified.subject.as_str() {
                        return Err(JoinRefusal::Grant(GrantRefusal::SubjectMismatch));
                    }
                    profile.check().map_err(JoinRefusal::Profile)?;
                }
                Ok(Admitted {
                    capabilities: verified.capabilities,
                    profile: verified.profile,
                    attested: true,
                })
            }
        }
    }
}

/// The capability preset a role name stands for: one role per preset and no way to spell a
/// combination, for the grant's `role` claim and the relay's command line alike.
#[must_use]
pub fn role_capabilities(role: &str) -> Option<Capabilities> {
    Some(match role {
        "viewer" => Capabilities::viewer(),
        "commenter" => Capabilities::commenter(),
        "suggester" => Capabilities::suggester(),
        "editor" => Capabilities::editor(),
        "owner" => Capabilities::owner(),
        _ => return None,
    })
}

/// The built-in grant profile: a compact JWS (RFC 7515) carrying JWT claims (RFC 7519), signed
/// with EdDSA over Ed25519 (RFC 8037).
///
/// # The token
///
/// - Header: `{"alg":"EdDSA"}` (a `typ` of `JWT` is allowed). **Any other `alg` is refused**,
///   `none` included, and so is a `crit` header this verifier would have to understand.
/// - Claims: `sub` (the identity, required), `aud` (a string or an array; must contain this
///   room's audience), `exp` (seconds since the epoch, required), `nbf` (optional), `role`
///   (`viewer` | `commenter` | `suggester` | `editor` | `owner`, required), and the display
///   claims `name`, `initials`, `color` and `picture` (OpenID Connect's name for an avatar URL).
///   With no `name`, the participant is attested but unnamed.
///
/// # Bounds and cost
///
/// The token is at most `MAX_GRANT_BYTES` before this sees it. One signature verification
/// (constant-time, `verify_strict`, which also refuses small-order keys and non-canonical
/// signatures) and one JSON parse per **join**, never per chunk.
#[derive(Debug)]
pub struct JwtVerifier {
    key: ed25519_dalek::VerifyingKey,
    audience: String,
    now: fn() -> u64,
    leeway: u64,
}

impl JwtVerifier {
    /// A verifier for grants signed by `public_key` (32 raw bytes) for `audience`.
    ///
    /// # Errors
    ///
    /// When the bytes are not a valid Ed25519 public key.
    pub fn new(public_key: &[u8; 32], audience: impl Into<String>) -> Result<Self, GrantRefusal> {
        let key = ed25519_dalek::VerifyingKey::from_bytes(public_key)
            .map_err(|_| GrantRefusal::Invalid)?;
        Ok(Self {
            key,
            audience: audience.into(),
            now: system_seconds,
            leeway: 60,
        })
    }

    /// The same verifier reading time from `now` instead of the system clock — for a test, or
    /// a deployment with a clock of its own.
    #[must_use]
    pub fn with_clock(mut self, now: fn() -> u64) -> Self {
        self.now = now;
        self
    }
}

/// Seconds since the Unix epoch, or zero on a clock before it (which then fails every `exp`).
fn system_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[derive(serde::Deserialize)]
struct Header {
    alg: String,
    #[serde(default)]
    crit: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

#[derive(serde::Deserialize)]
struct Claims {
    sub: String,
    aud: Audience,
    exp: u64,
    #[serde(default)]
    nbf: Option<u64>,
    role: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    initials: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    picture: Option<String>,
}

impl GrantVerifier for JwtVerifier {
    fn verify(&self, token: &GrantToken) -> Result<Verified, GrantRefusal> {
        use base64::Engine as _;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;

        let text = core::str::from_utf8(token.as_bytes()).map_err(|_| GrantRefusal::Invalid)?;
        let mut parts = text.split('.');
        let (Some(header), Some(payload), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(GrantRefusal::Invalid);
        };
        let decode = |part: &str| {
            URL_SAFE_NO_PAD
                .decode(part)
                .map_err(|_| GrantRefusal::Invalid)
        };
        let header: Header =
            serde_json::from_slice(&decode(header)?).map_err(|_| GrantRefusal::Invalid)?;
        // One algorithm. Read BEFORE the signature, and refused outright otherwise: honouring the
        // header's choice is how `alg: none` and key-confusion attacks work.
        if header.alg != "EdDSA" || header.crit.is_some() {
            return Err(GrantRefusal::Invalid);
        }
        let signature = ed25519_dalek::Signature::from_slice(&decode(signature)?)
            .map_err(|_| GrantRefusal::Invalid)?;
        let signed = &text[..text.rfind('.').ok_or(GrantRefusal::Invalid)?];
        self.key
            .verify_strict(signed.as_bytes(), &signature)
            .map_err(|_| GrantRefusal::Invalid)?;
        // Only now is anything in the payload believed.
        let claims: Claims =
            serde_json::from_slice(&decode(payload)?).map_err(|_| GrantRefusal::Invalid)?;
        let for_us = match &claims.aud {
            Audience::One(audience) => *audience == self.audience,
            Audience::Many(audiences) => audiences.contains(&self.audience),
        };
        if !for_us {
            return Err(GrantRefusal::Invalid);
        }
        let now = (self.now)();
        if claims.exp.saturating_add(self.leeway) <= now
            || claims
                .nbf
                .is_some_and(|nbf| nbf > now.saturating_add(self.leeway))
        {
            return Err(GrantRefusal::Expired);
        }
        let capabilities = role_capabilities(&claims.role).ok_or(GrantRefusal::Invalid)?;
        let subject = Identity::new(claims.sub.clone()).ok_or(GrantRefusal::Invalid)?;
        let verified = Verified::new(capabilities, subject);
        Ok(match claims.name {
            Some(name) => verified.with_profile(Profile {
                id: claims.sub,
                name,
                initials: claims.initials,
                color: claims.color,
                avatar_url: claims.picture,
            }),
            None => verified,
        })
    }
}
