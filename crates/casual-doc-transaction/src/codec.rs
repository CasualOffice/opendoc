// SPDX-License-Identifier: Apache-2.0

//! The byte codec — `107` 6.4, and the compatibility surface `152` §9 would not freeze until
//! the operation shapes stopped moving.
//!
//! # Why this could not be written before, and what changed
//!
//! `152` §9 refused to freeze bytes while `150` §9.1 and §9.2 were still about to move the
//! operation shapes: *"the one thing a compatibility surface must not do"*. ADR-056 answered
//! both on the **envelope** instead, so **no `Operation` variant changed and `Pos` is
//! untouched**. The shapes are final, and this is the first increment that was waiting only on
//! that.
//!
//! # The established pattern, named before any code
//!
//! A versioned, length-delimited **frame** around a **self-describing payload**. That is
//! protobuf's and CBOR's shared property and the only one that matters here: *a reader that
//! meets something it does not understand can measure it and move on*. A non-self-describing
//! format — `bincode`, `postcard` — is smaller and cannot do that at all: its schema is the
//! reader's own struct definition, so an added field is a silent misparse rather than a skipped
//! one. For a format that two different versions of this software will read, that trade is the
//! wrong way round.
//!
//! So: a fixed 12-byte header carrying a magic, a frame version and a payload encoding, then a
//! length, then the payload. Three consequences worth stating:
//!
//! 1. **The payload encoding is a field, not an assumption.** `08-ADR-REGISTER`'s pending list
//!    reserves *"canonical CBOR encoding profile and golden vectors"* as an undecided ADR, and
//!    that decision is about the **document** snapshot. Nothing here pre-empts it: swapping
//!    this payload encoding later is a [`PayloadEncoding`] value, not a format break, and a
//!    decoder that meets an encoding it does not hold refuses by name.
//! 2. **The schema has one definition.** The op set derives `serde` (ADR-057) rather than
//!    being hand-encoded in 58 arms. A hand-written encoder and the `apply` it must agree with
//!    are two definitions of one schema, and two definitions of one rule diverge — which is
//!    the same argument `150` §3.2 makes for its exhaustive matches, pointing the other way.
//! 3. **Field names are the surface.** `serde`'s derive is externally tagged and
//!    field-named, so adding a field is additive, removing or renaming one is a break, and
//!    [`GOLDEN_CHUNK`] is what makes an accidental break fail the build rather than a reader.
//!
//! # What is bounded, and why every bound is a refusal
//!
//! A codec is the one place in this crate that reads bytes it did not write, so every length
//! it reads is attacker-controlled. [`MAX_FRAME_BYTES`] is checked **before** any allocation,
//! from the header alone — a length field that says four gigabytes must not become a four
//! gigabyte `Vec` on the way to being rejected. `21`'s parser-limit discipline, applied here.
//!
//! # Complexity
//!
//! O(bytes). Nothing here walks a document, and nothing here is on the keystroke path: a
//! single-user edit encodes nothing at all.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::protocol::{Arrival, Submission};
use crate::session::Ordered;

/// The first four bytes of every frame: `ODC1`, for OpenDoc collaboration.
///
/// A magic is not decoration. The durable log of `107` 6.1 is a file somebody will one day
/// point the wrong tool at, and "this is not one of ours" is a better answer than a parse error
/// forty bytes in.
pub const FRAME_MAGIC: [u8; 4] = *b"ODC1";

/// The frame layout's own version.
///
/// Bumped when the **header** changes — not when the payload schema grows a field, which is
/// what the payload encoding's own additive rule covers. Nothing has superseded version 1.
pub const FRAME_VERSION: u16 = 1;

/// The largest frame this codec will decode, in bytes.
///
/// Sized from the protocol's own chunk budget rather than guessed: a submission is already
/// capped at [`CHUNK_BUDGET_BYTES`](crate::protocol::CHUNK_BUDGET_BYTES) of carried payload,
/// and this allows an order of magnitude over it for the framing, the declarations and the
/// encoding's own overhead. A frame above it is refused from the header, before anything is
/// allocated.
pub const MAX_FRAME_BYTES: usize = 10 * crate::protocol::CHUNK_BUDGET_BYTES;

/// The fixed header: magic, frame version, payload encoding, payload length.
const HEADER_BYTES: usize = 4 + 2 + 2 + 4;

/// How a frame's payload is encoded.
///
/// A field rather than an assumption, so the encoding can be replaced without the frame
/// changing — see the module documentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PayloadEncoding {
    /// Field-named JSON, as `casual-doc-model`'s own snapshot (`25`) already uses.
    ///
    /// Chosen for the first increment because it is the encoding the model's values are
    /// *already* round-tripped in, so the operation payloads and the document snapshot have one
    /// value schema between them rather than two. Its known cost is recorded in `152` §10 Q6: a
    /// `Vec<u8>` becomes an array of decimal numbers, about four times the size. That cost does
    /// **not** land here — no operation in the set carries raw bytes, because media travels as
    /// a *reference* and the stream stays in the host's resource map — and it is exactly why
    /// the snapshot half of `107` 6.1 must not reuse this encoding without measuring.
    Json,
}

impl PayloadEncoding {
    /// The value this encoding has on the wire.
    const fn code(self) -> u16 {
        match self {
            Self::Json => 1,
        }
    }

    /// The encoding `code` names, or `None` for one this build does not hold.
    const fn from_code(code: u16) -> Option<Self> {
        match code {
            1 => Some(Self::Json),
            _ => None,
        }
    }
}

/// Why a frame could not be decoded.
///
/// Every variant names what was expected, because a codec refusal is read by somebody holding
/// a file and no context at all.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CodecError {
    /// Fewer bytes than a header.
    TooShort {
        /// How many bytes were offered.
        got: usize,
        /// How many a header needs.
        need: usize,
    },
    /// The first four bytes are not [`FRAME_MAGIC`]. Not one of ours.
    NotAFrame,
    /// A frame version this build does not implement.
    UnsupportedFrameVersion {
        /// The version the frame declared.
        got: u16,
        /// The version this build writes.
        supported: u16,
    },
    /// A payload encoding this build does not hold.
    UnsupportedPayloadEncoding {
        /// The encoding code the frame declared.
        got: u16,
    },
    /// The declared payload length exceeds [`MAX_FRAME_BYTES`]. Refused from the header,
    /// before any allocation.
    TooLarge {
        /// The length the header declared.
        declared: usize,
        /// The limit.
        limit: usize,
    },
    /// The frame is shorter than its own declared payload length.
    Truncated {
        /// The length the header declared.
        declared: usize,
        /// How many payload bytes were actually there.
        available: usize,
    },
    /// Bytes after the payload the header accounted for. Refused rather than ignored: a
    /// trailing byte means the writer and the reader disagree about the record, and guessing
    /// which of them is right is how a log quietly loses its tail.
    TrailingBytes {
        /// How many bytes were left over.
        extra: usize,
    },
    /// The payload did not decode as the record this frame was read as.
    Malformed {
        /// The decoder's own message.
        detail: String,
    },
}

impl core::fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort { got, need } => {
                write!(formatter, "a frame needs {need} bytes of header, got {got}")
            }
            Self::NotAFrame => write!(formatter, "not an OpenDoc collaboration frame"),
            Self::UnsupportedFrameVersion { got, supported } => write!(
                formatter,
                "frame version {got} is not supported; this build writes {supported}"
            ),
            Self::UnsupportedPayloadEncoding { got } => {
                write!(formatter, "payload encoding {got} is not supported")
            }
            Self::TooLarge { declared, limit } => write!(
                formatter,
                "the frame declares {declared} payload bytes, over the {limit}-byte limit"
            ),
            Self::Truncated {
                declared,
                available,
            } => write!(
                formatter,
                "the frame declares {declared} payload bytes and carries {available}"
            ),
            Self::TrailingBytes { extra } => {
                write!(formatter, "{extra} bytes after the frame's own payload")
            }
            Self::Malformed { detail } => write!(formatter, "the payload is malformed: {detail}"),
        }
    }
}

impl core::error::Error for CodecError {}

/// Encodes `record` as one frame.
///
/// # Panics
///
/// Never, for the records this module exposes: every one of them serialises infallibly, and a
/// payload over [`MAX_FRAME_BYTES`] is a *decode* refusal rather than an encode one, so a
/// writer that genuinely produced one still writes a frame a reader will name.
fn encode<T: Serialize>(record: &T) -> Vec<u8> {
    let payload = serde_json::to_vec(record).unwrap_or_else(|error| {
        // A record of ours that cannot serialise is a programming error, not a runtime
        // condition, and the only shapes `serde_json` refuses are ones none of these records
        // has (a non-string map key, a non-finite float). Encoding an error *message* as the
        // payload would be worse than an empty one: a reader would parse it as a record.
        debug_assert!(false, "a codec record failed to serialise: {error}");
        Vec::new()
    });
    let mut out = Vec::with_capacity(HEADER_BYTES + payload.len());
    out.extend_from_slice(&FRAME_MAGIC);
    out.extend_from_slice(&FRAME_VERSION.to_le_bytes());
    out.extend_from_slice(&PayloadEncoding::Json.code().to_le_bytes());
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a payload over u32::MAX is refused on decode by MAX_FRAME_BYTES, which is \
                  five orders of magnitude smaller"
    )]
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

/// Decodes one frame, which must be the whole of `bytes`.
///
/// # Errors
///
/// [`CodecError`], naming what was expected. Checks are in the order that keeps an
/// attacker-controlled length from being allocated: magic, version, encoding, declared length
/// against [`MAX_FRAME_BYTES`], then the bytes actually present.
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
    if bytes.len() < HEADER_BYTES {
        return Err(CodecError::TooShort {
            got: bytes.len(),
            need: HEADER_BYTES,
        });
    }
    let (header, rest) = bytes.split_at(HEADER_BYTES);
    if header[..4] != FRAME_MAGIC {
        return Err(CodecError::NotAFrame);
    }
    let version = u16::from_le_bytes([header[4], header[5]]);
    if version != FRAME_VERSION {
        return Err(CodecError::UnsupportedFrameVersion {
            got: version,
            supported: FRAME_VERSION,
        });
    }
    let encoding = u16::from_le_bytes([header[6], header[7]]);
    let Some(PayloadEncoding::Json) = PayloadEncoding::from_code(encoding) else {
        return Err(CodecError::UnsupportedPayloadEncoding { got: encoding });
    };
    let declared = u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;
    // Before the length is used for anything at all, including a comparison with `rest`.
    if declared > MAX_FRAME_BYTES {
        return Err(CodecError::TooLarge {
            declared,
            limit: MAX_FRAME_BYTES,
        });
    }
    if rest.len() < declared {
        return Err(CodecError::Truncated {
            declared,
            available: rest.len(),
        });
    }
    if rest.len() > declared {
        return Err(CodecError::TrailingBytes {
            extra: rest.len() - declared,
        });
    }
    serde_json::from_slice(&rest[..declared]).map_err(|error| CodecError::Malformed {
        detail: error.to_string(),
    })
}

/// Encodes a client's chunk for the wire.
#[must_use]
pub fn encode_submission(submission: &Submission) -> Vec<u8> {
    encode(submission)
}

/// Decodes a client's chunk.
///
/// The result is **untrusted**: it says what a sender claimed, not what is true. Every
/// identity it declares is still checked by [`WireOperation::localise`](crate::wire::WireOperation::localise)
/// and every position by `casual_doc_edit::apply`. A codec that validated semantics would be a
/// second enforcement point for rules that already have one.
///
/// # Errors
///
/// [`CodecError`].
pub fn decode_submission(bytes: &[u8]) -> Result<Submission, CodecError> {
    decode(bytes)
}

/// Encodes a relay's fan-out for the wire.
#[must_use]
pub fn encode_arrival(arrival: &Arrival) -> Vec<u8> {
    encode(arrival)
}

/// Decodes a relay's fan-out. Untrusted, as [`decode_submission`] is.
///
/// # Errors
///
/// [`CodecError`].
pub fn decode_arrival(bytes: &[u8]) -> Result<Arrival, CodecError> {
    decode(bytes)
}

/// Encodes one ordered entry of a relay's history — the durable log's record type.
///
/// `107` 6.1's log is a sequence of these and nothing else, which follows from ADR-047: the
/// relay holds no document, so the only durable state it has *is* the order it imposed.
#[must_use]
pub fn encode_ordered(ordered: &Ordered) -> Vec<u8> {
    encode(ordered)
}

/// Decodes one ordered entry. Untrusted, as [`decode_submission`] is.
///
/// # Errors
///
/// [`CodecError`].
pub fn decode_ordered(bytes: &[u8]) -> Result<Ordered, CodecError> {
    decode(bytes)
}

/// A pinned encoding of one minimal chunk, as a hex string.
///
/// **This is the compatibility surface, written down.** `serde`'s derive makes field names the
/// schema, which is what makes an added field additive — and also what makes a renamed field a
/// silent break that no type error catches. A golden vector is the only thing that turns such a
/// rename into a build failure, and `the_golden_chunk_is_byte_for_byte_what_this_build_writes`
/// is where it does.
///
/// Changing this constant is therefore a **protocol decision**: it means either the frame
/// version or the payload schema moved, and `152` §5.1's version rule applies.
pub const GOLDEN_CHUNK: &str = include_str!("codec_golden_chunk.hex");

#[cfg(test)]
#[path = "codec_tests.rs"]
mod tests;
