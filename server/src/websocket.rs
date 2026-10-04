// SPDX-License-Identifier: Apache-2.0

//! RFC 6455 as a **byte-stream adaptor**, so the relay above it does not know it is there.
//!
//! # The established pattern, named before any code — SKILL §8
//!
//! This is a **framing adaptor** (a decorator over a stream), and it is the same shape
//! `std::io::BufReader` has: wrap a `Read`, wrap a `Write`, change nothing about what passes
//! through. It is deliberately *not* a WebSocket library's usual shape — an enumerated
//! `Message` type a caller matches on — because the layer above already has a record boundary
//! of its own ([`casual_doc_transaction::codec`]'s `ODC1` frames) and a reader for it
//! ([`crate::transport::Frames`]). Two record layers, each insisting the caller handle its
//! boundaries, is two mechanisms for one rule; SKILL §8 says prefer one.
//!
//! So: [`Unframed`] is a `Read` that yields the concatenated payloads of the data messages it
//! receives, and [`Framed`] is a `Write` that turns each `write` call into exactly one binary
//! message. `Relay`, `Frames` and `Participants` compile over them unchanged, and every test
//! those types already have still exercises the real thing.
//!
//! # Why the relay speaks WebSocket at all, and why it speaks nothing else
//!
//! **The client is a browser.** A browser can open a `WebSocket`, an HTTP request or a WebRTC
//! peer connection, and nothing else; it cannot open a TCP socket. The relay's whole purpose
//! is to carry bytes between browsers, so the transport is not a choice between WebSocket and
//! raw TCP — raw TCP was never reachable from the only client that exists.
//!
//! It speaks *nothing else* for the same reason, stated as a decision rather than left implied:
//! there was no raw-framing client in the tree when this landed, so a sniffing dual path would
//! have been a second mechanism serving nobody. A deployment that wants raw frames has
//! [`crate::transport::Frames`] over whatever stream it likes; what it does not get is this
//! binary doing two things.
//!
//! # Why this is hand-written and adds no dependency
//!
//! Because the alternative decides something much larger than framing. The obvious library is
//! `tokio-tungstenite`, and `tokio` fails `cargo check --workspace --target
//! wasm32-unknown-unknown` — `mio` refuses the target outright — so adopting it means either
//! dropping this member from the workspace wasm gate or moving it out of the workspace. ADR-063
//! records that decision and what each option costs. The short version: **WebSocket is a framing
//! layer, not a concurrency model**, and the relay is already thread-per-connection `std` (see
//! [`crate::transport`] for what that costs and why a dumb relay suits it). Framing it by hand
//! is ~200 lines of a frozen 2011 specification with published test vectors; the dependency
//! decision it avoids is permanent.
//!
//! What that costs is written down rather than glossed: the two primitives the handshake needs
//! — SHA-1 and base64 — are implemented here. SHA-1 is **not** used as a hash function in the
//! security sense; RFC 6455 §1.3 uses it as a fixed, publicly-known transformation of a
//! publicly-known nonce whose only job is to prove the peer parsed the request, and the RFC's
//! own worked example is the guard.
//!
//! # Complexity
//!
//! O(bytes) for everything here. Nothing walks a document, nothing retains a message after it
//! has been delivered, and the one unbounded thing a peer controls — the length it declares —
//! is judged against [`MAX_FRAME_BYTES`] before a byte of it is allocated.

use std::io::{self, Read, Write};

use casual_doc_transaction::codec::MAX_FRAME_BYTES;

/// The fixed string RFC 6455 §1.3 appends to a client's key before hashing.
const ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// The largest handshake header block this reads before refusing.
///
/// A handshake arrives before anything has been authorised and therefore from nobody in
/// particular — the same reasoning [`casual_doc_transaction::protocol::MAX_GRANT_BYTES`] gives.
/// Generous for a browser's request, which is a few hundred bytes with cookies.
pub const MAX_HANDSHAKE_BYTES: usize = 16 * 1024;

/// Which end of the connection this is.
///
/// It decides exactly one thing in each direction, and RFC 6455 §5.1 makes both mandatory: a
/// client masks every frame it sends and a server masks none, so each end **refuses** the
/// other's absent or present mask rather than tolerating it. Tolerating it is how a proxy
/// cache-poisoning defence becomes decoration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// Accepts connections: sends unmasked, requires inbound frames to be masked.
    Server,
    /// Opens connections: sends masked, requires inbound frames to be unmasked.
    Client,
}

/// Why a handshake did not complete.
#[derive(Debug)]
#[non_exhaustive]
pub enum HandshakeError {
    /// The peer closed before the header block ended.
    Closed,
    /// The header block exceeded [`MAX_HANDSHAKE_BYTES`] without ending.
    TooLarge {
        /// How many bytes had arrived.
        read: usize,
    },
    /// A required header was missing or carried the wrong value. Names the header, because a
    /// handshake failure is read by somebody holding a browser and no other evidence.
    Header {
        /// Which header.
        name: &'static str,
    },
    /// The response was not `101 Switching Protocols`.
    NotUpgraded {
        /// The status line, truncated to something a log can hold.
        status: String,
    },
    /// The stream itself failed.
    Io(io::Error),
}

impl core::fmt::Display for HandshakeError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Closed => write!(formatter, "the peer closed during the WebSocket handshake"),
            Self::TooLarge { read } => write!(
                formatter,
                "the WebSocket handshake header reached {read} bytes without ending"
            ),
            Self::Header { name } => {
                write!(formatter, "the WebSocket handshake header {name} was wrong")
            }
            Self::NotUpgraded { status } => {
                write!(formatter, "the peer answered {status} and not 101")
            }
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl core::error::Error for HandshakeError {}

impl From<io::Error> for HandshakeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Performs the server half of the handshake on `stream`.
///
/// Returns the bytes that arrived **after** the header block, which a client is allowed to
/// pipeline and a browser occasionally does. Dropping them loses the first frame of the
/// session, which is the `Join` — a bug that looks like an authorisation failure, so the
/// leftover is a return value rather than something a caller may forget to ask for.
///
/// # Errors
///
/// [`HandshakeError`]. On a refusal the caller's stream has had **nothing** written to it: an
/// HTTP error response would be a second protocol in a relay that speaks one, and a peer that
/// got the handshake wrong learns it from the closed connection.
pub fn accept<S: Read + Write>(stream: &mut S) -> Result<Vec<u8>, HandshakeError> {
    let (header, leftover) = read_header_block(stream)?;
    let header = String::from_utf8_lossy(&header);
    let mut lines = header.lines();
    let request = lines.next().unwrap_or_default();
    if !request.starts_with("GET ") {
        return Err(HandshakeError::Header { name: "request" });
    }
    if !header_contains(&header, "upgrade", "websocket") {
        return Err(HandshakeError::Header { name: "Upgrade" });
    }
    if !header_contains(&header, "connection", "upgrade") {
        return Err(HandshakeError::Header { name: "Connection" });
    }
    if header_value(&header, "sec-websocket-version").as_deref() != Some("13") {
        return Err(HandshakeError::Header {
            name: "Sec-WebSocket-Version",
        });
    }
    let Some(key) = header_value(&header, "sec-websocket-key") else {
        return Err(HandshakeError::Header {
            name: "Sec-WebSocket-Key",
        });
    };
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(&key)
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(leftover)
}

/// Performs the client half of the handshake on `stream`.
///
/// `nonce` is the 16 bytes that become `Sec-WebSocket-Key`. **Taken as an argument rather than
/// generated here**, for the reason the engine crate gives about clocks and identities: this
/// module has no business holding an entropy source, and the one caller that is not a test is
/// a browser, which never reaches this function at all — `WebSocket` does its own handshake.
/// So the only callers are tests and a native client, both of which know where their bytes
/// come from.
///
/// # Errors
///
/// [`HandshakeError`], including [`HandshakeError::Header`] naming `Sec-WebSocket-Accept` when
/// the server's answer does not match the nonce — the one check that proves the peer is a
/// WebSocket server and not an HTTP endpoint that answers 101 to everything.
pub fn connect<S: Read + Write>(
    stream: &mut S,
    host: &str,
    path: &str,
    nonce: [u8; 16],
) -> Result<Vec<u8>, HandshakeError> {
    let key = base64(&nonce);
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;
    stream.flush()?;
    let (header, leftover) = read_header_block(stream)?;
    let header = String::from_utf8_lossy(&header);
    let status = header.lines().next().unwrap_or_default();
    if !status.starts_with("HTTP/1.1 101") {
        return Err(HandshakeError::NotUpgraded {
            status: status.chars().take(120).collect(),
        });
    }
    if header_value(&header, "sec-websocket-accept").as_deref() != Some(&accept_key(&key)) {
        return Err(HandshakeError::Header {
            name: "Sec-WebSocket-Accept",
        });
    }
    Ok(leftover)
}

/// RFC 6455 §1.3: base64 of the SHA-1 of the key concatenated with the protocol GUID.
///
/// Exposed because it is the handshake's one computed value and the RFC publishes a worked
/// example for it, so a guard can check this function against the specification rather than
/// against itself.
#[must_use]
pub fn accept_key(key: &str) -> String {
    let mut data = key.as_bytes().to_vec();
    data.extend_from_slice(ACCEPT_GUID.as_bytes());
    base64(&sha1(&data))
}

/// Reads up to the `\r\n\r\n` that ends an HTTP header block.
///
/// Returns the block (without the terminator) and whatever arrived after it.
fn read_header_block<S: Read>(stream: &mut S) -> Result<(Vec<u8>, Vec<u8>), HandshakeError> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        if let Some(end) = find(&buffer, b"\r\n\r\n") {
            let leftover = buffer.split_off(end + 4);
            buffer.truncate(end);
            return Ok((buffer, leftover));
        }
        if buffer.len() >= MAX_HANDSHAKE_BYTES {
            return Err(HandshakeError::TooLarge { read: buffer.len() });
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(HandshakeError::Closed);
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// The first index at which `needle` occurs in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// One header's value, trimmed. Header names are case-insensitive (RFC 9110 §5.1), and a
/// browser's casing is not something to depend on.
fn header_value(header: &str, name: &str) -> Option<String> {
    header.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_owned())
    })
}

/// Whether a header's value contains `wanted`, case-insensitively.
///
/// `Connection` is a comma-separated list and a browser sends `keep-alive, Upgrade`, so an
/// equality test on it rejects real clients.
fn header_contains(header: &str, name: &str, wanted: &str) -> bool {
    header_value(header, name).is_some_and(|value| {
        value
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case(wanted))
    })
}

/// Standard base64, no line breaks.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let (b0, b1, b2) = (
            u32::from(group[0]),
            group.get(1).map_or(0, |b| u32::from(*b)),
            group.get(2).map_or(0, |b| u32::from(*b)),
        );
        let packed = (b0 << 16) | (b1 << 8) | b2;
        for shift in [18_u32, 12, 6, 0] {
            // Masked to six bits, so it indexes the 64-entry table and widens losslessly.
            let index = ((packed >> shift) & 0x3f) as usize;
            out.push(char::from(ALPHABET[index]));
        }
    }
    // Pad over the characters the missing input bytes produced, rather than skipping them
    // above: the arithmetic stays one expression and the padding stays one rule.
    let pad = (3 - bytes.len() % 3) % 3;
    out.truncate(out.len() - pad);
    for _ in 0..pad {
        out.push('=');
    }
    out
}

/// SHA-1 (FIPS 180-4), for the handshake's accept value and for nothing else.
///
/// **Not a security claim.** RFC 6455 §1.3 specifies this exact transformation of a nonce the
/// client publishes in the clear; it proves the peer parsed the request and defends a cache
/// from a crafted HTTP request, and it is not asked to be collision-resistant. Written here
/// rather than depended on for the reason the module header gives.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [
        0x6745_2301,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut words = [0_u32; 80];
        for (index, word) in block.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999_u32),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0_u8; 20];
    for (chunk, word) in out.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// A WebSocket opcode, as far as this adaptor cares about them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Opcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    const fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(Self::Continuation),
            1 => Some(Self::Text),
            2 => Some(Self::Binary),
            8 => Some(Self::Close),
            9 => Some(Self::Ping),
            10 => Some(Self::Pong),
            _ => None,
        }
    }

    const fn is_control(self) -> bool {
        matches!(self, Self::Close | Self::Ping | Self::Pong)
    }
}

/// A `Read` over a WebSocket connection, yielding the payload bytes of its data messages.
///
/// # What it hides, and the one thing it does not
///
/// It hides fragmentation, masking and control frames: a `Binary` message split into four
/// continuations is four payloads concatenated, and a `Ping` between them is invisible. It does
/// **not** hide the close: a `Close` frame, like a closed socket, reads as end of stream, so
/// [`crate::transport::Frames`] answers `ReadError::Closed` for a peer that left politely and
/// `ReadError::Interrupted` for one that left mid-record — the distinction that module's doc
/// comment says it exists to keep.
///
/// # A ping is not answered, and that is deliberate
///
/// Answering one needs the write half, which lives in [`Framed`] on a cloned handle, and the
/// only client this relay serves — a browser's `WebSocket` — cannot send a ping at all (the API
/// exposes no such method; the browser answers the *server's* pings in its own stack). So a
/// pong here would be a reply to a message no reachable peer sends, over a seam built to carry
/// it. If a non-browser client ever needs keep-alives, the honest shape is a ping written from
/// the relay's side on a timer, not a pong smuggled through a reader.
#[derive(Debug)]
pub struct Unframed<R> {
    stream: R,
    role: Role,
    /// Bytes read from the socket and not yet parsed into frames.
    buffer: Vec<u8>,
    /// Payload delivered to the caller from `taken` onwards.
    ready: Vec<u8>,
    taken: usize,
    closed: bool,
}

impl<R: Read> Unframed<R> {
    /// A reader over `stream`, continuing from `prefix` — the bytes a handshake read past its
    /// own header block.
    #[must_use]
    pub fn new(stream: R, role: Role, prefix: Vec<u8>) -> Self {
        Self {
            stream,
            role,
            buffer: prefix,
            ready: Vec::new(),
            taken: 0,
            closed: false,
        }
    }

    /// Parses one frame out of `buffer`, or `None` when more bytes are needed.
    ///
    /// `Err` is a protocol violation, which is terminal: there is no resynchronisation point in
    /// a self-describing length framing, so carrying on would be reading payload as headers.
    fn take_frame(&mut self) -> Option<io::Result<(bool, Opcode, Vec<u8>)>> {
        if self.buffer.len() < 2 {
            return None;
        }
        let first = self.buffer[0];
        let second = self.buffer[1];
        let fin = first & 0x80 != 0;
        if first & 0x70 != 0 {
            return Some(Err(violation("a reserved bit was set")));
        }
        let Some(opcode) = Opcode::from_bits(first & 0x0f) else {
            return Some(Err(violation("an unknown opcode arrived")));
        };
        let masked = second & 0x80 != 0;
        if masked != matches!(self.role, Role::Server) {
            return Some(Err(violation(match self.role {
                Role::Server => "a client frame arrived unmasked (RFC 6455 §5.1)",
                Role::Client => "a server frame arrived masked (RFC 6455 §5.1)",
            })));
        }
        let short = usize::from(second & 0x7f);
        let (length, mut offset) = match short {
            126 => {
                if self.buffer.len() < 4 {
                    return None;
                }
                (
                    usize::from(u16::from_be_bytes([self.buffer[2], self.buffer[3]])),
                    4,
                )
            }
            127 => {
                if self.buffer.len() < 10 {
                    return None;
                }
                let mut bytes = [0_u8; 8];
                bytes.copy_from_slice(&self.buffer[2..10]);
                let declared = u64::from_be_bytes(bytes);
                let Ok(length) = usize::try_from(declared) else {
                    return Some(Err(violation("a frame declared an unaddressable length")));
                };
                (length, 10)
            }
            _ => (short, 2),
        };
        if opcode.is_control() && (length > 125 || !fin) {
            return Some(Err(violation(
                "a control frame was fragmented or over 125 bytes (RFC 6455 §5.5)",
            )));
        }
        // Judged before a byte of it is allocated, which is the whole point of checking the
        // header rather than the arrival: a peer must not be able to make this allocate by
        // claiming a length it never sends.
        if length > MAX_FRAME_BYTES {
            return Some(Err(violation("a frame declared more than the frame limit")));
        }
        if masked {
            offset += 4;
        }
        if self.buffer.len() < offset + length {
            return None;
        }
        let mut payload = self.buffer[offset..offset + length].to_vec();
        if masked {
            let key = &self.buffer[offset - 4..offset];
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= key[index % 4];
            }
        }
        self.buffer.drain(..offset + length);
        Some(Ok((fin, opcode, payload)))
    }
}

/// A protocol violation, reported as an `io::Error` because that is what `Read` may return and
/// the layer above reads it as `ReadError::Io`.
fn violation(detail: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail)
}

impl<R: Read> Read for Unframed<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.taken < self.ready.len() {
                let take = (self.ready.len() - self.taken).min(out.len());
                out[..take].copy_from_slice(&self.ready[self.taken..self.taken + take]);
                self.taken += take;
                if self.taken == self.ready.len() {
                    self.ready.clear();
                    self.taken = 0;
                }
                return Ok(take);
            }
            if self.closed {
                return Ok(0);
            }
            match self.take_frame() {
                Some(Err(error)) => {
                    self.closed = true;
                    return Err(error);
                }
                Some(Ok((_, Opcode::Close, _))) => {
                    self.closed = true;
                    return Ok(0);
                }
                // A ping or a pong says nothing to the layer above; see the type's docs for why
                // nothing is sent back.
                Some(Ok((_, Opcode::Ping | Opcode::Pong, _))) => continue,
                Some(Ok((_, _, payload))) => {
                    // Text and binary are both delivered: a text message carrying an `ODC1`
                    // frame is malformed, and refusing it here would name the transport for a
                    // fault the codec names precisely (`CodecError::NotAFrame`).
                    if payload.is_empty() {
                        continue;
                    }
                    self.ready = payload;
                    self.taken = 0;
                    continue;
                }
                None => {
                    let mut chunk = [0_u8; 8192];
                    let read = self.stream.read(&mut chunk)?;
                    if read == 0 {
                        self.closed = true;
                        // Mid-frame bytes are handed on rather than hidden, so the layer above
                        // can tell a polite departure from a crash.
                        return Ok(0);
                    }
                    if self.buffer.len().saturating_add(read) > MAX_FRAME_BYTES * 2 {
                        self.closed = true;
                        return Err(violation(
                            "a peer streamed past the frame limit without completing a frame",
                        ));
                    }
                    self.buffer.extend_from_slice(&chunk[..read]);
                }
            }
        }
    }
}

/// A `Write` over a WebSocket connection: **one `write` call is one binary message.**
///
/// That equivalence is the contract the relay above depends on, and it holds because every
/// caller already writes a whole `ODC1` frame in one call — `Relay` encodes a frame and hands
/// it over, `Participants::fan_out` writes one, the binary's answer path writes one. `write`
/// never reports a short write, so a `write_all` over it cannot split a frame across two
/// messages either.
#[derive(Debug)]
pub struct Framed<W> {
    stream: W,
    role: Role,
    /// Counts messages, and seeds the mask when this end is a client.
    sent: u64,
}

impl<W: Write> Framed<W> {
    /// A writer over `stream`.
    #[must_use]
    pub const fn new(stream: W, role: Role) -> Self {
        Self {
            stream,
            role,
            sent: 0,
        }
    }

    /// Sends a close frame, so the peer's reader sees a departure rather than a reset.
    ///
    /// # Errors
    ///
    /// The underlying stream's, which a caller on a broken socket may ignore: a close frame is
    /// a courtesy and the peer's socket already ends the session.
    pub fn close(&mut self) -> io::Result<()> {
        // 1000, "normal closure", as a big-endian status with no reason text.
        self.send(Opcode::Close, &1000_u16.to_be_bytes())?;
        self.stream.flush()
    }

    /// Writes one frame with `opcode` and `payload`.
    fn send(&mut self, opcode: Opcode, payload: &[u8]) -> io::Result<()> {
        let mask = matches!(self.role, Role::Client);
        let mut header = Vec::with_capacity(14);
        header.push(
            0x80 | match opcode {
                Opcode::Continuation => 0,
                Opcode::Text => 1,
                Opcode::Binary => 2,
                Opcode::Close => 8,
                Opcode::Ping => 9,
                Opcode::Pong => 10,
            },
        );
        let flag = if mask { 0x80 } else { 0 };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "each arm casts a value the branch has already bounded"
        )]
        match payload.len() {
            length if length < 126 => header.push(flag | length as u8),
            length if length <= usize::from(u16::MAX) => {
                header.push(flag | 126);
                header.extend_from_slice(&(length as u16).to_be_bytes());
            }
            length => {
                header.push(flag | 127);
                header.extend_from_slice(&(length as u64).to_be_bytes());
            }
        }
        let body = if mask {
            // RFC 6455 §5.3 wants an unpredictable key, and what it buys is a defence against
            // a proxy being tricked into caching a crafted request. The only callers that mask
            // are tests and a native client — a browser does its own framing — so the key is
            // derived from the message counter and the payload rather than from an entropy
            // source this module has no business holding. **Stated rather than implied**: a
            // native client that faces a hostile proxy must supply its own, which is why
            // `Framed::new` takes a role and not a boolean.
            let seed = self
                .sent
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                .wrapping_add(payload.len() as u64)
                .wrapping_add(u64::from(payload.first().copied().unwrap_or(0)) << 32);
            let key = seed.to_le_bytes();
            let key = [key[0], key[1], key[2], key[3]];
            header.extend_from_slice(&key);
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ key[index % 4])
                .collect()
        } else {
            payload.to_vec()
        };
        self.sent = self.sent.wrapping_add(1);
        // One `write_all` for header and body together: two would let a reader on the far side
        // see a header with no payload behind it, which is correct but needlessly chatty, and
        // on a `Vec<u8>` test writer would interleave with another participant's frame.
        header.extend_from_slice(&body);
        self.stream.write_all(&header)
    }
}

impl<W: Write> Write for Framed<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.send(Opcode::Binary, buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

#[cfg(test)]
#[path = "websocket_tests.rs"]
mod tests;
