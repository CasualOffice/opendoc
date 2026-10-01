// SPDX-License-Identifier: Apache-2.0

//! The frame reader's guards. The interesting cases are all about a stream's lack of record
//! boundaries, so each one feeds the bytes in a shape a socket really produces.

use casual_doc_transaction::codec::encode_frame;
use casual_doc_transaction::protocol::Revision;

use super::{Frames, ReadError};

/// A reader that hands out at most `chunk` bytes per `read`, the way a socket does.
struct Dribble {
    bytes: Vec<u8>,
    at: usize,
    chunk: usize,
}

impl std::io::Read for Dribble {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let left = self.bytes.len() - self.at;
        let take = left.min(self.chunk).min(out.len());
        out[..take].copy_from_slice(&self.bytes[self.at..self.at + take]);
        self.at += take;
        Ok(take)
    }
}

#[test]
fn two_frames_in_one_read_are_two_messages_and_a_frame_split_across_reads_is_one() {
    // Both halves of "a stream has no record boundaries", in one test because they are one bug:
    // an implementation that assumed one read is one message would fail the first half, and one
    // that assumed a frame arrives whole would fail the second.
    let mut bytes = encode_frame(&Revision::new(11));
    bytes.extend_from_slice(&encode_frame(&Revision::new(22)));

    // Everything at once.
    let mut frames = Frames::new(Dribble {
        bytes: bytes.clone(),
        at: 0,
        chunk: usize::MAX,
    });
    assert_eq!(
        frames.next_frame::<Revision>().expect("the first"),
        Revision::new(11)
    );
    assert_eq!(
        frames.next_frame::<Revision>().expect("the second"),
        Revision::new(22),
        "a second frame already in the buffer must be read without another `read`"
    );

    // One byte at a time, which is the shape that breaks a reader that trusts a single `read`.
    let mut frames = Frames::new(Dribble {
        bytes,
        at: 0,
        chunk: 1,
    });
    assert_eq!(
        frames.next_frame::<Revision>().expect("the first"),
        Revision::new(11)
    );
    assert_eq!(
        frames.next_frame::<Revision>().expect("the second"),
        Revision::new(22)
    );
}

#[test]
fn a_stream_that_ends_between_frames_is_different_from_one_that_ends_inside_one() {
    // Two outcomes, not one: a peer that closed cleanly has left, and a peer that stopped
    // mid-record crashed or is not speaking this protocol. Collapsing them would make a crash
    // indistinguishable from a goodbye, which is exactly the diagnosis a relay operator needs.
    let whole = encode_frame(&Revision::new(5));

    let mut clean = Frames::new(Dribble {
        bytes: whole.clone(),
        at: 0,
        chunk: usize::MAX,
    });
    assert_eq!(
        clean.next_frame::<Revision>().expect("the frame"),
        Revision::new(5)
    );
    assert!(
        matches!(clean.next_frame::<Revision>(), Err(ReadError::Closed)),
        "an end between frames is a clean close"
    );

    let mut torn = Frames::new(Dribble {
        bytes: whole[..whole.len() - 3].to_vec(),
        at: 0,
        chunk: usize::MAX,
    });
    match torn.next_frame::<Revision>() {
        Err(ReadError::Interrupted { partial }) => assert!(
            partial > 0,
            "the refusal must say how much of the frame had arrived"
        ),
        other => panic!("an end inside a frame must be `Interrupted`, got {other:?}"),
    }
}
