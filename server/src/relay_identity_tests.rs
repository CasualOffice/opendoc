// SPDX-License-Identifier: Apache-2.0

//! Who is here, and who wrote what — `docs/175` §4 and §5, ADR-070, at the relay.
//!
//! The grants here are **real** JWTs, signed with Ed25519 and verified by [`JwtVerifier`]: the
//! guard that matters most — a grant for one person presented under another's identity is
//! refused — is only worth something against the verifier a deployment actually runs. The
//! mutations that drove each guard red are recorded in the commit message.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{Mint, Operation, Pos};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_transaction::codec::{decode_frame, frame_len};
use casual_doc_transaction::protocol::{
    Base, ClientId, ClientMessage, GrantToken, Identity, Join, PROTOCOL_VERSION, Profile, Refusal,
    Revision, Seq, ServerMessage, Submission,
};
use casual_doc_transaction::wire::WireOperation;
use ed25519_dalek::{Signer as _, SigningKey};

use super::Relay;
use crate::{Access, GrantRefusal, GrantVerifier, JwtVerifier, Room};

const AUDIENCE: &str = "room-7f3a";
const NOW: u64 = 1_800_000_000;

fn host_key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn now() -> u64 {
    NOW
}

fn verifier() -> JwtVerifier {
    JwtVerifier::new(&host_key().verifying_key().to_bytes(), AUDIENCE)
        .expect("a key")
        .with_clock(now)
}

/// A compact JWS over `claims` with `header`, signed by `key`.
fn jws(header: &serde_json::Value, claims: &serde_json::Value, key: &SigningKey) -> String {
    let encode = |value: &serde_json::Value| URL_SAFE_NO_PAD.encode(value.to_string());
    let signed = format!("{}.{}", encode(header), encode(claims));
    let signature = key.sign(signed.as_bytes());
    format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

fn claims(sub: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "sub": sub, "aud": AUDIENCE, "exp": NOW + 600, "role": "editor",
        "name": name, "color": "#3355c4", "picture": "https://cdn.example/a.png",
    })
}

fn grant(sub: &str, name: &str) -> GrantToken {
    let header = serde_json::json!({ "alg": "EdDSA", "typ": "JWT" });
    GrantToken::new(jws(&header, &claims(sub, name), &host_key()).into_bytes()).expect("a token")
}

fn scratch(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "opendoc-relay-identity-{name}-{}.journal",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn room(name: &str, access: Access) -> (std::path::PathBuf, Relay<Vec<u8>>) {
    let path = scratch(name);
    let relay = Relay::new(Room::create(&path).expect("a new room"), access);
    (path, relay)
}

fn join(
    relay: &mut Relay<Vec<u8>>,
    who: &str,
    grant: Option<GrantToken>,
    profile: Option<Profile>,
) -> (ServerMessage, Option<ClientId>) {
    let mut writer = Some(Vec::new());
    let handled = relay
        .handle(
            None,
            &mut writer,
            &ClientMessage::Join(Join {
                protocol: PROTOCOL_VERSION,
                identity: Identity::new(who).expect("an identity"),
                grant,
                profile,
                resume: None,
            }),
        )
        .expect("the join is handled");
    (handled.answer.expect("an answer"), handled.joined_as)
}

/// Every frame `client` has been sent, decoded.
fn received(relay: &mut Relay<Vec<u8>>, client: ClientId) -> Vec<ServerMessage> {
    let written = relay.participants_mut().take(client).expect("connected");
    let mut out = Vec::new();
    let mut at = 0;
    while at < written.len() {
        let length = frame_len(&written[at..]).expect("a frame");
        out.push(decode_frame(&written[at..at + length]).expect("it decodes"));
        at += length;
    }
    relay.participants_mut().joined(client, written);
    out
}

fn chunk(client: ClientId) -> Submission {
    let mint = Mint::reserve(&mut IdGenerator::new(0xA1), 1).expect("a mint");
    Submission {
        client,
        seq: Seq::new(1),
        base: Base::Revision(Revision::new(0)),
        operations: vec![WireOperation::of(
            Operation::InsertText {
                at: Pos::new(NodeId::new(0x91).expect("id"), 0),
                text: "q".to_owned(),
            },
            mint,
        )],
    }
}

/// **An impersonation attempt is refused.** A valid grant, signed by the host, issued to Ada,
/// presented by a join claiming to be Bob: refused with `NotAuthorised`, terminal, and nothing
/// admitted. The same grant under Ada's own identity is admitted with the grant's name.
#[test]
fn a_grant_for_one_person_presented_as_another_is_refused() {
    let (path, mut relay) = room("impersonation", Access::Granted(Box::new(verifier())));
    let (answer, joined) = join(
        &mut relay,
        "u_bob",
        Some(grant("u_ada", "Ada Lovelace")),
        None,
    );
    assert_eq!(
        answer,
        ServerMessage::Stopped {
            reason: Refusal::NotAuthorised
        },
        "Ada's grant must not admit Bob"
    );
    assert!(joined.is_none(), "and nobody joined the room");
    assert_eq!(relay.room().session().head(), Revision::new(0));

    let (answer, joined) = join(
        &mut relay,
        "u_ada",
        Some(grant("u_ada", "Ada Lovelace")),
        None,
    );
    let ServerMessage::Welcome {
        me,
        participants_present,
        capabilities,
        ..
    } = answer
    else {
        panic!("Ada's own grant admits her: {answer:?}");
    };
    assert_eq!(capabilities, Capabilities::editor());
    assert_eq!(me.as_ref().map(|p| p.name.as_str()), Some("Ada Lovelace"));
    assert!(
        me.is_some_and(|me| me.avatar_url.is_none()),
        "the stamp a joiner authors under carries no avatar; it is what gets journalled"
    );
    assert_eq!(participants_present.len(), 1);
    assert!(participants_present[0].attested);
    assert_eq!(
        participants_present[0].client,
        joined.expect("admitted"),
        "the joiner is in the list it is handed"
    );
    let _ = std::fs::remove_file(&path);
}

/// The verifier's own refusals: every way a token can be wrong is `NotAuthorised` on the wire.
#[test]
fn a_jwt_that_is_not_exactly_right_does_not_verify() {
    let header = serde_json::json!({ "alg": "EdDSA" });
    let good = jws(&header, &claims("u_ada", "Ada"), &host_key());
    let token = |text: String| GrantToken::new(text.into_bytes()).expect("a token");
    assert!(
        verifier().verify(&token(good.clone())).is_ok(),
        "the control verifies"
    );

    let mut tampered = claims("u_ada", "Ada");
    tampered["role"] = serde_json::json!("owner");
    let parts: Vec<&str> = good.split('.').collect();
    let forged_payload = format!(
        "{}.{}.{}",
        parts[0],
        URL_SAFE_NO_PAD.encode(tampered.to_string()),
        parts[2]
    );
    let other_key = SigningKey::from_bytes(&[9; 32]);
    let mut wrong_audience = claims("u_ada", "Ada");
    wrong_audience["aud"] = serde_json::json!("another-room");
    let mut expired = claims("u_ada", "Ada");
    expired["exp"] = serde_json::json!(NOW - 3600);
    let unsigned = format!(
        "{}.{}.",
        URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#),
        URL_SAFE_NO_PAD.encode(claims("u_ada", "Ada").to_string())
    );
    let mut no_role = claims("u_ada", "Ada");
    no_role.as_object_mut().expect("claims").remove("role");
    for (what, text, why) in [
        ("a tampered payload", forged_payload, GrantRefusal::Invalid),
        (
            "another key's signature",
            jws(&header, &claims("u_ada", "Ada"), &other_key),
            GrantRefusal::Invalid,
        ),
        (
            "another room's audience",
            jws(&header, &wrong_audience, &host_key()),
            GrantRefusal::Invalid,
        ),
        (
            "an expired grant",
            jws(&header, &expired, &host_key()),
            GrantRefusal::Expired,
        ),
        ("alg none", unsigned, GrantRefusal::Invalid),
        (
            "an HS256 header",
            jws(
                &serde_json::json!({ "alg": "HS256" }),
                &claims("u_ada", "Ada"),
                &host_key(),
            ),
            GrantRefusal::Invalid,
        ),
        (
            "no role",
            jws(&header, &no_role, &host_key()),
            GrantRefusal::Invalid,
        ),
        (
            "not a JWS",
            "not.a.jws.at-all".to_owned(),
            GrantRefusal::Invalid,
        ),
    ] {
        assert_eq!(verifier().verify(&token(text)).err(), Some(why), "{what}");
    }
}

/// In a granted room the profile everybody sees is the GRANT's: a client that claims another
/// name alongside a valid grant is shown under the grant's.
#[test]
fn a_granted_room_ignores_the_profile_a_client_claims() {
    let (path, mut relay) = room("claim-ignored", Access::Granted(Box::new(verifier())));
    let (_, ada) = join(
        &mut relay,
        "u_ada",
        Some(grant("u_ada", "Ada Lovelace")),
        None,
    );
    let ada = ada.expect("admitted");
    let (_, bob) = join(
        &mut relay,
        "u_bob",
        Some(grant("u_bob", "Bob Stone")),
        Some(Profile::new("u_bob", "The CEO")),
    );
    let bob = bob.expect("admitted");
    let told = received(&mut relay, ada);
    let joined = told
        .iter()
        .find_map(|m| match m {
            ServerMessage::ParticipantJoined(p) if p.client == bob => Some(p.clone()),
            _ => None,
        })
        .expect("Ada is told Bob arrived");
    assert_eq!(joined.profile.map(|p| p.name), Some("Bob Stone".to_owned()));
    assert!(joined.attested);
    let _ = std::fs::remove_file(&path);
}

/// **The open-room claim is labelled as a claim**: fanned out unattested, with the relay's
/// colour (none of its own) and no avatar, whatever the claimant sent (`docs/175` §4.2).
#[test]
fn an_open_room_fans_a_claimed_name_out_unverified_without_colour_or_avatar() {
    let (path, mut relay) = room("open-claim", Access::Open(Capabilities::editor()));
    let (_, ada) = join(&mut relay, "u_ada", None, None);
    let ada = ada.expect("admitted");
    let mut claimed = Profile::new("u_mallory", "Ada Lovelace");
    claimed.color = Some("#ff0000".to_owned());
    claimed.avatar_url = Some("https://tracker.example/pixel.png".to_owned());
    let (answer, mallory) = join(&mut relay, "u_mallory", None, Some(claimed));
    let mallory = mallory.expect("admitted");
    let joined = received(&mut relay, ada)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::ParticipantJoined(p) if p.client == mallory => Some(p),
            _ => None,
        })
        .expect("Ada is told who arrived");
    assert!(!joined.attested, "a claim is never attested");
    let profile = joined.profile.expect("the claimed name is shown");
    assert_eq!(profile.name, "Ada Lovelace");
    assert!(profile.color.is_none(), "a claimant cannot pick a colour");
    assert!(
        profile.avatar_url.is_none(),
        "a claimed avatar is a tracking pixel"
    );
    let ServerMessage::Welcome { me, .. } = answer else {
        panic!("a welcome");
    };
    assert_eq!(me.map(|p| p.name), Some("Ada Lovelace".to_owned()));
    let _ = std::fs::remove_file(&path);
}

/// **`Arrival.author` is stamped by the relay**, from the admission: the frame every other
/// participant applies names the grant's person, and the submission had nowhere to say.
#[test]
fn every_arrival_carries_the_author_the_relay_attaches() {
    let (path, mut relay) = room("stamp", Access::Granted(Box::new(verifier())));
    let (_, ada) = join(
        &mut relay,
        "u_ada",
        Some(grant("u_ada", "Ada Lovelace")),
        None,
    );
    let (_, bob) = join(&mut relay, "u_bob", Some(grant("u_bob", "Bob Stone")), None);
    let (ada, bob) = (ada.expect("admitted"), bob.expect("admitted"));
    relay
        .handle(Some(bob), &mut None, &ClientMessage::Submit(chunk(bob)))
        .expect("ordered");
    let arrival = received(&mut relay, ada)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::Apply(arrival) => Some(arrival),
            _ => None,
        })
        .expect("Ada applies Bob's chunk");
    let author = arrival.author.expect("stamped");
    assert_eq!(author.name, "Bob Stone");
    assert!(author.avatar_url.is_none());
    let _ = std::fs::remove_file(&path);
}

/// A participant who never moved their caret still left, and everybody is told.
#[test]
fn a_leave_is_announced_even_without_a_caret() {
    let (path, mut relay) = room("left", Access::Open(Capabilities::editor()));
    let (_, ada) = join(&mut relay, "u_ada", None, None);
    let (_, bob) = join(&mut relay, "u_bob", None, None);
    let (ada, bob) = (ada.expect("admitted"), bob.expect("admitted"));
    relay.disconnected(bob);
    assert!(
        received(&mut relay, ada)
            .iter()
            .any(|m| matches!(m, ServerMessage::ParticipantLeft { client } if *client == bob)),
        "Ada must be told Bob left"
    );
    let _ = std::fs::remove_file(&path);
}

/// A claimed profile out of bounds is refused, naming nothing on the wire but `Malformed`.
#[test]
fn a_claimed_profile_out_of_bounds_is_refused() {
    let (path, mut relay) = room("bounds", Access::Open(Capabilities::editor()));
    let mut claimed = Profile::new("u_ada", "Ada");
    claimed.avatar_url = Some("javascript:alert(1)".to_owned());
    let (answer, joined) = join(&mut relay, "u_ada", None, Some(claimed));
    assert_eq!(
        answer,
        ServerMessage::Stopped {
            reason: Refusal::Malformed
        }
    );
    assert!(joined.is_none());
    let _ = std::fs::remove_file(&path);
}
