// SPDX-License-Identifier: Apache-2.0

//! Authorship attached at the order and conformed on apply — `docs/175` §5, ADR-070.
//!
//! Every guard here drives the production order ([`ServerSession`]) and the production replica
//! ([`ClientSession::receive`]): a participant is admitted with a profile, commits, and the
//! arrival everybody else applies is the one the order built. The mutations that drove each guard
//! red are recorded in the commit message.

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{Operation, ReviewParagraphState};
use casual_doc_model::v1::{
    BlockNode, Comment, CommentId, CommentRangeEnd, CommentRangeStart, DefinitionMap, Definitions,
    Document, InlineNode, Paragraph, ParagraphProperties, Run, RunProperties,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::protocol::{
    Arrival, ClientMessage, Identity, Join, PROTOCOL_VERSION, Profile, Resume, ResumeKey, Revision,
    ServerMessage,
};
use crate::{RevisionLog, Transaction, TransactionId};

use super::{ClientSession, ServerSession};

fn seed() -> (Document, NodeId, NodeId) {
    let mut ids = IdGenerator::new(7);
    let document_id = ids.next_id().expect("id");
    let paragraph = ids.next_id().expect("id");
    let run = ids.next_id().expect("id");
    let document = Document::new(
        document_id,
        vec![BlockNode::Paragraph(Paragraph {
            id: paragraph,
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Run(Run {
                id: run,
                properties: RunProperties::default().into(),
                text: "Shared text.".to_owned(),
            })],
        })],
        Definitions::default(),
    )
    .expect("a valid document");
    (document, paragraph, run)
}

fn profile(id: &str, name: &str) -> Profile {
    Profile::new(id, name)
}

/// One replica: what a browser tab holds.
struct Replica {
    document: Document,
    log: RevisionLog,
    ids: IdGenerator,
    session: ClientSession,
}

fn admit(
    server: &mut ServerSession,
    document: &Document,
    who: &str,
    author: Option<Profile>,
) -> Replica {
    let (message, _) = server.join_as(
        &ClientMessage::Join(Join {
            protocol: PROTOCOL_VERSION,
            identity: Identity::new(who).expect("an identity"),
            grant: None,
            profile: None,
            resume: Some(Resume {
                key: ResumeKey::new(who).expect("a key"),
                revision: Revision::new(0),
            }),
        }),
        Capabilities::owner(),
        author,
    );
    let mut log = RevisionLog::default();
    let session = ClientSession::joined(document, &message, &mut log).expect("joined");
    Replica {
        document: document.clone(),
        ids: IdGenerator::new(session.id_space().get()),
        log,
        session,
    }
}

impl Replica {
    /// Comments the paragraph under `author`, exactly as the facade's review lowering would,
    /// and returns the new comment's id.
    fn comment(&mut self, paragraph: NodeId, run: NodeId, author: &str) -> CommentId {
        let comment = CommentId::new(self.ids.next_id().expect("id"));
        let start = self.ids.next_id().expect("id");
        let end = self.ids.next_id().expect("id");
        let mut comments: DefinitionMap<CommentId, Comment> =
            self.document.definitions().comments.clone();
        comments.insert(
            comment,
            Comment {
                author: Some(author.to_owned()),
                initials: Some(casual_doc_edit::authorship::derive_initials(author)),
                date: Some("2026-10-24T09:00:00Z".to_owned()),
                ..Comment::default()
            },
        );
        let text = self
            .document
            .paragraph(paragraph)
            .and_then(|p| p.inlines.first().cloned())
            .expect("the run");
        assert!(matches!(&text, InlineNode::Run(r) if r.id == run));
        let operation = Operation::UpdateReviewState {
            paragraphs: vec![ReviewParagraphState {
                node: paragraph,
                inlines: vec![
                    InlineNode::CommentRangeStart(CommentRangeStart { id: start, comment }),
                    text,
                    InlineNode::CommentRangeEnd(CommentRangeEnd { id: end, comment }),
                ],
            }],
            comments: Some(comments),
        };
        let transaction = Transaction::reserve(
            TransactionId::new(1),
            self.log.head(),
            "Comment",
            &mut self.ids,
            vec![operation],
        )
        .expect("ids");
        self.log
            .apply(&mut self.document, transaction)
            .expect("the comment applies");
        comment
    }

    fn receive(&mut self, arrival: &Arrival) {
        self.session
            .receive(arrival, &mut self.document, &mut self.ids, &mut self.log)
            .expect("the arrival merges");
    }

    fn author_of(&self, comment: CommentId) -> Option<&str> {
        self.document
            .definitions()
            .comments
            .get(&comment)
            .and_then(|c| c.author.as_deref())
    }
}

/// The arrivals the order holds since `from`, as a late joiner's `Welcome` would carry them.
fn arrivals(server: &ServerSession, from: u64) -> Vec<Arrival> {
    server
        .history_since(Revision::new(from))
        .expect("retained")
        .map(|entry| Arrival {
            revision: entry.revision,
            client: entry.client,
            operations: entry.operations.clone(),
            author: entry.author.clone(),
        })
        .collect()
}

/// **A forged author is corrected on apply.** Bob's client is rewritten to put Ada's name on a
/// new comment. The order attaches the profile Bob was admitted with, and every other replica —
/// two of them, to show they agree — stores Bob's name.
#[test]
fn a_forged_author_is_corrected_on_apply_and_every_replica_agrees() {
    let (document, paragraph, run) = seed();
    let mut server = ServerSession::default();
    let mut ada = admit(
        &mut server,
        &document,
        "u_ada",
        Some(profile("u_ada", "Ada Lovelace")),
    );
    let mut bob = admit(
        &mut server,
        &document,
        "u_bob",
        Some(profile("u_bob", "Bob Stone")),
    );
    let mut cleo = admit(
        &mut server,
        &document,
        "u_cleo",
        Some(profile("u_cleo", "Cleo Ng")),
    );

    let forged = bob.comment(paragraph, run, "Ada Lovelace");
    let submission = bob.session.flush(&bob.log).expect("a chunk");
    server.commit(&submission);

    let arrived = arrivals(&server, 0);
    assert_eq!(
        arrived[0].author.as_ref().map(|p| p.name.as_str()),
        Some("Bob Stone"),
        "the order attaches the admitted profile, not anything the sender said"
    );
    ada.receive(&arrived[0]);
    cleo.receive(&arrived[0]);
    assert_eq!(ada.author_of(forged), Some("Bob Stone"));
    assert_eq!(cleo.author_of(forged), Some("Bob Stone"));
    let initials = |r: &Replica| {
        r.document
            .definitions()
            .comments
            .get(&forged)
            .and_then(|c| c.initials.clone())
    };
    assert_eq!(initials(&ada).as_deref(), Some("BS"));
    assert_eq!(
        ada.document, cleo.document,
        "two replicas conform one arrival identically"
    );
}

/// **An honest commit is a no-op on arrival.** Bob comments under the name the order will
/// attach, so the replica that receives it ends with exactly Bob's own optimistic state: nothing
/// is rewritten, and nothing would need to roll back.
#[test]
fn an_honest_comment_arrives_exactly_as_its_author_wrote_it() {
    let (document, paragraph, run) = seed();
    let mut server = ServerSession::default();
    let mut ada = admit(
        &mut server,
        &document,
        "u_ada",
        Some(profile("u_ada", "Ada Lovelace")),
    );
    let mut bob = admit(
        &mut server,
        &document,
        "u_bob",
        Some(profile("u_bob", "Bob Stone")),
    );
    let me = bob
        .session
        .me()
        .expect("the stamp Bob authors under")
        .name
        .clone();
    bob.comment(paragraph, run, &me);
    let submission = bob.session.flush(&bob.log).expect("a chunk");
    server.commit(&submission);
    ada.receive(&arrivals(&server, 0)[0]);
    assert_eq!(
        ada.document.definitions().comments,
        bob.document.definitions().comments
    );
}

/// An arrival nobody named keeps what it carries: an open room with no claimed profile is
/// "Participant N" and its comment is stored as its client wrote it (`docs/175` §10).
#[test]
fn an_unnamed_participants_comment_is_not_restamped() {
    let (document, paragraph, run) = seed();
    let mut server = ServerSession::default();
    let mut ada = admit(&mut server, &document, "u_ada", None);
    let mut bob = admit(&mut server, &document, "u_bob", None);
    let comment = bob.comment(paragraph, run, "You");
    let submission = bob.session.flush(&bob.log).expect("a chunk");
    server.commit(&submission);
    let arrived = arrivals(&server, 0);
    assert!(arrived[0].author.is_none());
    ada.receive(&arrived[0]);
    assert_eq!(ada.author_of(comment), Some("You"));
}

/// The stamp is the admission's, and an admission is durable: a session restored from its
/// checkpoint stamps the next chunk with the same name, and a reconnect that names somebody
/// else is the one that changes it.
#[test]
fn the_author_stamp_survives_a_checkpoint_and_follows_the_latest_admission() {
    let (document, paragraph, run) = seed();
    let mut server = ServerSession::default();
    let _ada = admit(
        &mut server,
        &document,
        "u_ada",
        Some(profile("u_ada", "Ada Lovelace")),
    );
    let mut bob = admit(
        &mut server,
        &document,
        "u_bob",
        Some(profile("u_bob", "Bob Stone")),
    );
    let bob_client = bob.session.client();
    let (state, retained) = server.checkpoint();
    let retained: Vec<_> = retained.cloned().collect();
    let encoded = serde_json::to_string(&state).expect("encode");
    let mut restored =
        ServerSession::restored(serde_json::from_str(&encoded).expect("decode"), retained);
    assert_eq!(
        restored.author_for(bob_client).map(|p| p.name.as_str()),
        Some("Bob Stone")
    );
    bob.comment(paragraph, run, "Bob Stone");
    let submission = bob.session.flush(&bob.log).expect("a chunk");
    restored.commit(&submission);
    assert_eq!(
        arrivals(&restored, 0)[0]
            .author
            .as_ref()
            .map(|p| p.name.as_str()),
        Some("Bob Stone")
    );
}

/// A checkpoint written before identity existed still opens: the field is defaulted and the
/// participants are simply unnamed until their next join.
#[test]
fn a_checkpoint_from_before_identity_still_decodes() {
    let mut server = ServerSession::default();
    let (document, _, _) = seed();
    let _ada = admit(&mut server, &document, "u_ada", None);
    let (state, _) = server.checkpoint();
    let mut value = serde_json::to_value(&state).expect("encode");
    value.as_object_mut().expect("an object").remove("authors");
    let state = serde_json::from_value(value).expect("an old checkpoint decodes");
    let restored = ServerSession::restored(state, Vec::new());
    assert!(
        restored
            .author_for(crate::protocol::ClientId::new(0))
            .is_none()
    );
}

/// The avatar never enters the order's durable state: the stamp drops it (`docs/175` §4.5).
#[test]
fn the_journalled_stamp_carries_no_avatar() {
    let mut server = ServerSession::default();
    let (document, _, _) = seed();
    let mut ada_profile = profile("u_ada", "Ada Lovelace");
    ada_profile.avatar_url = Some("https://cdn.example/ada.png".to_owned());
    let ada = admit(&mut server, &document, "u_ada", Some(ada_profile));
    let stamp = server.author_for(ada.session.client()).expect("a stamp");
    assert!(stamp.avatar_url.is_none());
    assert!(ada.session.me().is_some_and(|me| me.avatar_url.is_none()));
}

/// **A submission has no author field** — the `152` §2b method, applied to edits: a claim with
/// nowhere to be written is unexpressible rather than merely rejected. If somebody adds one, the
/// relay would have to start *checking* it, which `docs/175` §5.1 explains it cannot do.
#[test]
fn a_submission_has_nowhere_to_claim_an_author() {
    let source = include_str!("protocol.rs");
    let start = source
        .find("pub struct Submission {")
        .expect("the declaration");
    let body = &source[start..start + source[start..].find("\n}").expect("its end")];
    for claim in ["author", "profile", "identity", "name"] {
        let fields: Vec<_> = body
            .lines()
            .filter(|line| line.trim_start().starts_with("pub "))
            .filter(|line| line.contains(claim))
            .collect();
        assert!(
            fields.is_empty(),
            "`Submission` declares {fields:?}: a sender must not be able to state who wrote a \
             chunk. The relay attaches it (`Arrival::author`)."
        );
    }
}

/// `Welcome` carries the stamp the joiner will author under, which is what lets an honest
/// replica's optimistic state equal everybody else's.
#[test]
fn a_welcome_tells_the_joiner_its_own_stamp() {
    let mut server = ServerSession::default();
    let (message, _) = server.join_as(
        &ClientMessage::Join(Join {
            protocol: PROTOCOL_VERSION,
            identity: Identity::new("u_ada").expect("an identity"),
            grant: None,
            profile: None,
            resume: None,
        }),
        Capabilities::owner(),
        Some(profile("u_ada", "Ada Lovelace")),
    );
    let ServerMessage::Welcome { me, .. } = message else {
        panic!("a welcome");
    };
    assert_eq!(me.map(|p| p.name), Some("Ada Lovelace".to_owned()));
}

#[test]
fn profile_bounds_are_refusals_naming_the_field() {
    use crate::protocol::ProfileField;
    assert_eq!(profile("", "Ada").check(), Err(ProfileField::Id));
    assert_eq!(
        profile(&"x".repeat(129), "Ada").check(),
        Err(ProfileField::Id)
    );
    assert_eq!(profile("u", " ").check(), Err(ProfileField::Name));
    assert_eq!(
        profile("u", &"n".repeat(256)).check(),
        Err(ProfileField::Name)
    );
    let mut p = profile("u", "Ada");
    p.initials = Some("ABCDEFGHI".to_owned());
    assert_eq!(p.check(), Err(ProfileField::Initials));
    p.initials = Some("AL".to_owned());
    p.color = Some("red".to_owned());
    assert_eq!(p.check(), Err(ProfileField::Color));
    p.color = Some("#3355c4".to_owned());
    for refused in [
        "http://cdn.example/a.png",
        "javascript:alert(1)",
        "https://user:pw@cdn.example/a.png",
        "data:text/html,hi",
    ] {
        p.avatar_url = Some(refused.to_owned());
        assert_eq!(p.check(), Err(ProfileField::AvatarUrl), "{refused}");
    }
    p.avatar_url = Some(format!("https://cdn.example/{}", "a".repeat(1100)));
    assert_eq!(p.check(), Err(ProfileField::AvatarUrl));
    p.avatar_url = Some("https://cdn.example/ada.png".to_owned());
    assert_eq!(p.check(), Ok(()));
    p.avatar_url = Some("data:image/png;base64,AAAA".to_owned());
    assert_eq!(p.check(), Ok(()));
}

#[test]
fn a_claimed_profile_loses_its_colour_and_avatar() {
    let mut p = profile("u", "Ada");
    p.color = Some("#123456".to_owned());
    p.avatar_url = Some("https://cdn.example/a.png".to_owned());
    let claimed = p.claimed();
    assert_eq!(claimed.name, "Ada");
    assert!(claimed.color.is_none() && claimed.avatar_url.is_none());
}
