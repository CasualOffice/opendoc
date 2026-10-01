// SPDX-License-Identifier: Apache-2.0

//! The two fan-out rules, each asserted as the guarantee.

use casual_doc_transaction::protocol::ClientId;

use super::Participants;

/// A writer that records what it was given, or fails every write.
#[derive(Debug, Default)]
struct Sink {
    written: Vec<u8>,
    broken: bool,
}

impl std::io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.broken {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the peer went away",
            ));
        }
        self.written.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.broken {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the peer went away",
            ));
        }
        Ok(())
    }
}

#[test]
fn the_author_receives_nothing_and_the_others_receive_the_bytes() {
    // The author's exclusion, asserted by reading what each sink RECEIVED. Counting failures
    // cannot do it: a fan-out that wrote to the author as well would report no failure at all,
    // and a fan-out that wrote to nobody would report no failure either. Three participants, so
    // "everyone but the author" cannot be satisfied by accident.
    let mut participants: Participants<Sink> = Participants::new();
    participants.joined(ClientId::new(0), Sink::default());
    participants.joined(ClientId::new(1), Sink::default());
    participants.joined(ClientId::new(2), Sink::default());

    assert!(participants.fan_out(ClientId::new(1), b"abc").is_empty());

    // Drain by removing each writer, which is the only accessor the type offers — deliberately,
    // because a getter for a participant's buffer would exist only for this test.
    let mut lengths = Vec::new();
    for client in 0..3_u64 {
        let sink = participants.take(ClientId::new(client)).expect("present");
        lengths.push((client, sink.written.len()));
    }
    assert_eq!(
        lengths,
        vec![(0, 3), (1, 0), (2, 3)],
        "the author must receive NOTHING — a client that got its own chunk back would apply its \
         own edit twice"
    );
}

#[test]
fn a_participant_whose_write_failed_is_reported_rather_than_swallowed() {
    // The second rule. A failed write means that client is now BEHIND THE ORDER, and the resume
    // path exists to catch it up — but only if somebody noticed. A `let _ =` here turns a
    // reconnectable client into a silently diverged one.
    let mut participants: Participants<Sink> = Participants::new();
    participants.joined(ClientId::new(0), Sink::default());
    participants.joined(
        ClientId::new(1),
        Sink {
            written: Vec::new(),
            broken: true,
        },
    );
    participants.joined(ClientId::new(2), Sink::default());

    let failed = participants.fan_out(ClientId::new(0), b"abc");
    assert_eq!(
        failed,
        vec![ClientId::new(1)],
        "the broken participant must be named, and only it"
    );
}

#[test]
fn a_reconnecting_client_replaces_its_own_writer_rather_than_appearing_twice() {
    // `152` §5.5 hands a resumed client THE SAME id, because that is what keeps `(client, seq)`
    // suppressing duplicates. So a second writer for one id is a reconnection, and treating it as
    // a new participant would fan every chunk out twice down a socket that is already closed.
    let mut participants: Participants<Sink> = Participants::new();
    participants.joined(ClientId::new(7), Sink::default());
    participants.joined(ClientId::new(7), Sink::default());
    assert_eq!(
        participants.len(),
        1,
        "one id is one participant however many times it reconnects"
    );
    assert!(participants.left(ClientId::new(7)));
    assert!(participants.is_empty());
    assert!(
        !participants.left(ClientId::new(7)),
        "forgetting a participant twice must say so rather than pretending"
    );
}
