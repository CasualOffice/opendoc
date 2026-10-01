// SPDX-License-Identifier: Apache-2.0

//! The codec's guards: a round trip, the pinned surface, and every refusal.
//!
//! The round trip is the weakest of the three and is here mostly to catch a derive that was
//! never added. What actually protects the format is the **golden vector** — field names are the
//! schema, so a rename is a silent break no type error catches — and the **refusals**, because a
//! codec reads bytes it did not write and every length in them is attacker-controlled.

use casual_doc_edit::{FormatDelta, Mint, Operation, Pos, Range as EditRange};
use casual_doc_model::{IdGenerator, NodeId};

use crate::codec::{
    CodecError, FRAME_MAGIC, FRAME_VERSION, GOLDEN_CHUNK, MAX_FRAME_BYTES, decode_arrival,
    decode_ordered, decode_submission, encode_arrival, encode_ordered, encode_submission,
};
use crate::protocol::{Arrival, Base, ClientId, Revision, Seq, Submission};
use crate::session::Ordered;
use crate::wire::WireOperation;
use crate::{Affinity, Intent};

/// A chunk whose values are all fixed, so the golden vector is a function of the schema alone.
///
/// Deliberately *not* built from a generator: a golden vector over generated input pins nothing,
/// because the input moves whenever the generator does.
fn golden_submission() -> Submission {
    let node = NodeId::new(0x11).expect("id");
    let mint = Mint::reserve(&mut IdGenerator::new(0x22), 1).expect("a mint");
    Submission {
        client: ClientId::new(3),
        seq: Seq::new(7),
        base: Base::Revision(Revision::new(5)),
        operations: vec![
            WireOperation::of(
                Operation::InsertText {
                    at: Pos::new(node, 4),
                    text: "hi".to_owned(),
                },
                mint,
            )
            .declaring(Intent::NONE.with_affinity(Affinity::After)),
        ],
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        use core::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[test]
fn a_chunk_round_trips_with_its_mints_and_its_declarations() {
    let submission = golden_submission();
    let decoded = decode_submission(&encode_submission(&submission)).expect("it decodes");
    assert_eq!(decoded, submission);
    // Named individually, because `Submission`'s `PartialEq` would also be satisfied by a
    // codec that dropped both and compared two empty values — and the declarations are the
    // whole reason this increment could be written at all (ADR-056).
    assert_eq!(
        decoded.operations[0].mint(),
        submission.operations[0].mint(),
        "the identity space must survive the wire, or two replicas name one node differently"
    );
    assert_eq!(
        decoded.operations[0].intent().affinity(),
        Some(Affinity::After),
        "a declaration a receiver cannot reconstruct must survive the wire"
    );
}

#[test]
fn an_arrival_and_an_ordered_log_record_round_trip() {
    let submission = golden_submission();
    let arrival = Arrival {
        revision: Revision::new(9),
        client: ClientId::new(3),
        operations: submission.operations.clone(),
    };
    assert_eq!(
        decode_arrival(&encode_arrival(&arrival)).expect("it decodes"),
        arrival
    );
    let ordered = Ordered {
        revision: Revision::new(9),
        client: ClientId::new(3),
        operations: submission.operations,
    };
    assert_eq!(
        decode_ordered(&encode_ordered(&ordered)).expect("it decodes"),
        ordered,
        "the durable log's record is the relay's ordered entry and nothing else (ADR-047)"
    );
}

#[test]
fn the_golden_chunk_is_byte_for_byte_what_this_build_writes() {
    // The compatibility surface, pinned. `serde`'s derive makes FIELD NAMES the schema, so
    // renaming one is a break that no type error catches and no round-trip test can see — both
    // sides of a round trip move together. This is the only guard that cannot.
    //
    // A diff here is not a test to fix. It means the frame version or the payload schema moved,
    // and `152` §5.1's version rule applies.
    let written = hex(&encode_submission(&golden_submission()));
    assert_eq!(
        written,
        GOLDEN_CHUNK.trim(),
        "the encoding changed. If that was intended, bump what `152` §5.1 says to bump and \
         re-record the vector in the same commit; if it was not, a field was renamed"
    );
}

#[test]
fn the_golden_chunk_starts_with_the_header_the_module_documents() {
    // The vector above would pass just as well if the header were wrong in a way that was
    // wrong consistently, so the header is read back out of it independently.
    let bytes = encode_submission(&golden_submission());
    assert_eq!(&bytes[..4], &FRAME_MAGIC, "the magic");
    assert_eq!(
        u16::from_le_bytes([bytes[4], bytes[5]]),
        FRAME_VERSION,
        "the frame version"
    );
    assert_eq!(
        u16::from_le_bytes([bytes[6], bytes[7]]),
        1,
        "the payload encoding code for JSON"
    );
    let declared = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    assert_eq!(
        declared,
        bytes.len() - 12,
        "the declared length must be the payload's, not the frame's"
    );
}

#[test]
fn a_field_a_newer_sender_added_is_skipped_rather_than_refused() {
    // The property the whole frame design is for, and the reason a non-self-describing format
    // was rejected: a reader that meets something it does not understand measures it and moves
    // on. Asserted by actually injecting an unknown field into the payload and re-framing it,
    // because a claim about forward compatibility that no test produces is just a comment.
    let submission = golden_submission();
    let framed = encode_submission(&submission);
    let payload = &framed[12..];
    let mut text = String::from_utf8(payload.to_vec()).expect("JSON is UTF-8");
    assert!(text.starts_with('{'));
    text.insert_str(1, "\"aFieldFromTheFuture\":[1,2,3],");

    let mut reframed = Vec::new();
    reframed.extend_from_slice(&framed[..8]);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a test payload of known size"
    )]
    reframed.extend_from_slice(&(text.len() as u32).to_le_bytes());
    reframed.extend_from_slice(text.as_bytes());

    assert_eq!(
        decode_submission(&reframed).expect("an unknown field is skipped"),
        submission,
        "an added field must decode to the same value, or no version of this can read another's"
    );
}

#[test]
fn every_refusal_names_what_was_expected_and_none_of_them_allocates_the_declared_length() {
    let good = encode_submission(&golden_submission());

    assert_eq!(
        decode_submission(&good[..3]),
        Err(CodecError::TooShort { got: 3, need: 12 })
    );

    let mut not_ours = good.clone();
    not_ours[0] = b'X';
    assert_eq!(decode_submission(&not_ours), Err(CodecError::NotAFrame));

    let mut future = good.clone();
    future[4..6].copy_from_slice(&9_u16.to_le_bytes());
    assert_eq!(
        decode_submission(&future),
        Err(CodecError::UnsupportedFrameVersion {
            got: 9,
            supported: FRAME_VERSION
        })
    );

    let mut other_encoding = good.clone();
    other_encoding[6..8].copy_from_slice(&2_u16.to_le_bytes());
    assert_eq!(
        decode_submission(&other_encoding),
        Err(CodecError::UnsupportedPayloadEncoding { got: 2 })
    );

    // The hostile one: a header claiming a payload far larger than the bytes present. It must
    // be refused from the HEADER, so the claim never sizes an allocation. A twelve-byte input
    // is what proves that — there is nothing else here to read.
    let mut enormous = good[..12].to_vec();
    enormous[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode_submission(&enormous),
        Err(CodecError::TooLarge {
            declared: u32::MAX as usize,
            limit: MAX_FRAME_BYTES
        }),
        "a declared length over the limit must be refused before it is used for anything"
    );

    let truncated = &good[..good.len() - 5];
    assert!(
        matches!(
            decode_submission(truncated),
            Err(CodecError::Truncated { .. })
        ),
        "a frame shorter than its own declared payload"
    );

    let mut trailing = good.clone();
    trailing.push(0);
    assert_eq!(
        decode_submission(&trailing),
        Err(CodecError::TrailingBytes { extra: 1 }),
        "a trailing byte means writer and reader disagree about the record, and guessing which \
         is right is how a log loses its tail"
    );

    let mut rubbish = good.clone();
    let payload_start = 12;
    rubbish[payload_start] = b'[';
    assert!(
        matches!(
            decode_submission(&rubbish),
            Err(CodecError::Malformed { .. })
        ),
        "a payload that is not the record this frame was read as"
    );
}

#[test]
fn an_operation_carrying_a_whole_subtree_round_trips_exactly() {
    // `InsertText` is the easy case. The operations that actually stress the schema are the ones
    // carrying model values — a range, a property delta — because those are `casual-doc-model`'s
    // own serde and not this crate's, and the layering claim in the module docs is that the two
    // schemas meet cleanly rather than being one.
    let node = NodeId::new(0x31).expect("id");
    let mint = Mint::reserve(&mut IdGenerator::new(0x44), 1).expect("a mint");
    let submission = Submission {
        client: ClientId::new(1),
        seq: Seq::new(1),
        base: Base::Chained,
        operations: vec![WireOperation::of(
            Operation::FormatText {
                range: EditRange {
                    start: Pos::new(node, 2),
                    end: Pos::new(node, 6),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    italic: Some(false),
                    ..FormatDelta::default()
                },
            },
            mint,
        )],
    };
    let decoded = decode_submission(&encode_submission(&submission)).expect("it decodes");
    assert_eq!(decoded, submission);
    let Operation::FormatText { delta, range } = decoded.operations[0].operation() else {
        panic!("expected the formatting operation back");
    };
    assert_eq!(
        (delta.bold, delta.italic),
        (Some(true), Some(false)),
        "a three-valued property must not collapse to two over the wire"
    );
    assert_eq!((range.start.offset, range.end.offset), (2, 6));
}
