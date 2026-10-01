// SPDX-License-Identifier: Apache-2.0

//! A frame reader over any byte stream.
//!
//! # Why this is std-only, and what that costs
//!
//! An async runtime is a **dependency decision** — `tokio` pulls in a tree, and
//! `dependency-policy` (`cargo deny check bans licenses sources`) is a gate this lane cannot
//! run. So the transport here is `std::io` and the relay binary is thread-per-connection.
//!
//! That is a real limit and it is stated rather than implied: thread-per-connection costs a
//! thread and its stack per participant, so it suits a relay with tens of participants per room
//! and not thousands. It is also exactly the right shape for a *dumb* relay, whose per-message
//! work is "append, assign a number, write to N sockets" and never a document — there is no
//! computation to overlap, only i/o. Moving to an async runtime is an optimisation of a working
//! mechanism, with a dependency decision in front of it, and `107` 6.6 is satisfied without
//! taking it.
//!
//! # Why a reader type exists at all
//!
//! A stream gives no record boundaries. The frame carries its own length
//! ([`casual_doc_transaction::codec::frame_len`]), so reading one message is: read
//! until a frame is complete, decode it, keep the remainder. Getting that wrong — scanning for a
//! delimiter, or assuming one `read` is one message — is the bug every hand-rolled framing has,
//! which is why it is written once here instead of at each call site.

use std::io::Read;

use casual_doc_transaction::codec::{CodecError, MAX_FRAME_BYTES, decode_frame, frame_len};
use serde::de::DeserializeOwned;

/// Why a frame could not be read from a stream.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReadError {
    /// The stream ended between frames. Not an error for a caller that expected the peer to
    /// leave; an error for one that did not.
    Closed,
    /// The stream ended **part way through** a frame. Distinct from [`ReadError::Closed`]: a
    /// peer that stopped mid-record either crashed or is not speaking this protocol.
    Interrupted {
        /// How many bytes of the partial frame had arrived.
        partial: usize,
    },
    /// The frame was refused by the codec.
    Codec(CodecError),
    /// The stream itself failed.
    Io(std::io::Error),
}

impl core::fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Closed => write!(formatter, "the stream closed between frames"),
            Self::Interrupted { partial } => {
                write!(formatter, "the stream closed {partial} bytes into a frame")
            }
            Self::Codec(error) => write!(formatter, "{error}"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl core::error::Error for ReadError {}

impl From<std::io::Error> for ReadError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Reads whole frames from a stream, buffering across reads.
#[derive(Debug)]
pub struct Frames<R> {
    stream: R,
    buffer: Vec<u8>,
}

impl<R: Read> Frames<R> {
    /// A reader over `stream`.
    pub const fn new(stream: R) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
        }
    }

    /// Reads and decodes the next frame.
    ///
    /// # Errors
    ///
    /// [`ReadError`]. A [`CodecError::TooLarge`] is reached **before** the declared length is
    /// read, so a peer cannot make this allocate by claiming a huge frame: the buffer grows only
    /// by what actually arrives, and the header is judged as soon as twelve bytes are in.
    pub fn next_frame<T: DeserializeOwned>(&mut self) -> Result<T, ReadError> {
        loop {
            match frame_len(&self.buffer) {
                Ok(length) => {
                    let frame: T =
                        decode_frame(&self.buffer[..length]).map_err(ReadError::Codec)?;
                    self.buffer.drain(..length);
                    return Ok(frame);
                }
                Err(CodecError::Truncated { .. } | CodecError::TooShort { .. }) => {}
                Err(error) => return Err(ReadError::Codec(error)),
            }
            // Bounded: the only way the buffer grows is bytes that genuinely arrived, and a
            // frame larger than the limit has already been refused above by its header.
            let mut chunk = [0_u8; 8192];
            let read = self.stream.read(&mut chunk)?;
            if read == 0 {
                return Err(if self.buffer.is_empty() {
                    ReadError::Closed
                } else {
                    ReadError::Interrupted {
                        partial: self.buffer.len(),
                    }
                });
            }
            if self.buffer.len().saturating_add(read) > MAX_FRAME_BYTES {
                // A peer streaming past the limit without ever completing a frame. Refused
                // rather than buffered: this is the shape that turns a reader into a memory
                // sink, and it cannot be caught by the header check alone.
                return Err(ReadError::Codec(CodecError::TooLarge {
                    declared: self.buffer.len().saturating_add(read),
                    limit: MAX_FRAME_BYTES,
                }));
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
