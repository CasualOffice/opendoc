// SPDX-License-Identifier: Apache-2.0

//! The framing adaptor's guards.
//!
//! Every one of them asserts a guarantee the layer above depends on rather than a mechanism
//! this module happens to use: that the handshake computes the value RFC 6455 publishes, that
//! one `write` is one message, that a frame bigger than the limit is refused **from its header**,
//! and that the two mandatory masking rules are refusals and not remarks.
//!
//! The two primitives are checked against **published vectors** and not against themselves,
//! because a hand-written SHA-1 that agrees with its own test is a hand-written SHA-1.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};

use super::{
    Framed, HandshakeError, MAX_HANDSHAKE_BYTES, Role, Unframed, accept, accept_key, base64,
    connect, sha1,
};

/// FIPS 180-4's own worked examples, plus the empty input.
#[test]
fn sha1_agrees_with_the_published_vectors() {
    let hex = |bytes: [u8; 20]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(hex(sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    assert_eq!(
        hex(sha1(b"abc")),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        hex(sha1(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "84983e441c3bd26ebaae4aa1f95129e5e54670f1",
        "the two-block example, which is the one a single-block implementation passes by luck"
    );
    // A million 'a's is the fourth published example and the only one that exercises the
    // length encoding past 2^32 bits of input length arithmetic on a 32-bit accumulator.
    let long = vec![b'a'; 1_000_000];
    assert_eq!(hex(sha1(&long)), "34aa973cd4c4daa4f61eeb2bdbad27316534016f");
}

/// RFC 4648 §10's vectors, including every padding case.
#[test]
fn base64_agrees_with_the_published_vectors() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foob"), "Zm9vYg==");
    assert_eq!(base64(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
}

/// **RFC 6455 §1.3's worked example, character for character.**
///
/// The handshake's one computed value, checked against the specification that defines it. A
/// browser refuses the connection outright when this is wrong, and the failure it reports says
/// nothing about which of the two primitives was at fault — so the primitives are pinned above
/// and the composition is pinned here.
#[test]
fn the_rfc_6455_worked_example_is_the_accept_value() {
    assert_eq!(
        accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    );
}

/// A loopback pair, which is a real socket and not a double of one.
fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let address = listener.local_addr().expect("an address");
    let client = std::thread::spawn(move || TcpStream::connect(address).expect("it connects"));
    let (server, _) = listener.accept().expect("it accepts");
    (server, client.join().expect("the connector finished"))
}

/// **The two ends agree on a handshake neither of them wrote both halves of.**
#[test]
fn a_client_and_a_server_complete_the_handshake() {
    let (mut server, mut client) = pair();
    let joined = std::thread::spawn(move || {
        let leftover = connect(&mut client, "127.0.0.1", "/room", [7_u8; 16]);
        (client, leftover)
    });
    let leftover = accept(&mut server).expect("the server accepts");
    assert!(
        leftover.is_empty(),
        "this client pipelined nothing, so there is nothing after the header"
    );
    let (_client, result) = joined.join().expect("the client finished");
    assert!(result.expect("the client completes").is_empty());
}

/// **A client that pipelines its first frame does not lose it.**
///
/// The failure this prevents is specific and would have looked like an authorisation bug: the
/// bytes after `\r\n\r\n` are the `Join`, and a handshake that discarded them would leave the
/// relay waiting for a message the client believes it sent.
#[test]
fn bytes_pipelined_behind_the_handshake_survive() {
    let (mut server, mut client) = pair();
    std::thread::spawn(move || {
        let request = "GET /room HTTP/1.1\r\n\
                       Host: 127.0.0.1\r\n\
                       Upgrade: websocket\r\n\
                       Connection: keep-alive, Upgrade\r\n\
                       Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                       Sec-WebSocket-Version: 13\r\n\r\nSTOWAWAY";
        client.write_all(request.as_bytes()).expect("it writes");
        client.flush().expect("it flushes");
        // Held open so the server's response has somewhere to go.
        std::thread::sleep(std::time::Duration::from_millis(200));
    });
    let leftover = accept(&mut server).expect("the server accepts");
    assert_eq!(leftover, b"STOWAWAY");
}

/// **A `Connection` header that lists more than `Upgrade` is still an upgrade.**
///
/// Browsers send `keep-alive, Upgrade`. An equality test on this header refuses real clients,
/// which is why the check is containment over the comma-separated list.
#[test]
fn a_browsers_connection_header_is_accepted() {
    assert!(super::header_contains(
        "GET / HTTP/1.1\r\nConnection: keep-alive, Upgrade",
        "connection",
        "upgrade"
    ));
    assert!(super::header_contains(
        "GET / HTTP/1.1\r\nCONNECTION: Upgrade",
        "connection",
        "upgrade"
    ));
    assert!(!super::header_contains(
        "GET / HTTP/1.1\r\nConnection: close",
        "connection",
        "upgrade"
    ));
}

/// **A handshake that never ends is refused rather than buffered.**
#[test]
fn an_endless_handshake_header_is_refused() {
    let (mut server, mut client) = pair();
    std::thread::spawn(move || {
        let filler = vec![b'x'; 4096];
        // More than the bound, never terminated. `write_all` fails once the server has gone,
        // which is the expected end of this thread rather than a fault.
        for _ in 0..16 {
            if client.write_all(&filler).is_err() {
                return;
            }
        }
    });
    match accept(&mut server) {
        Err(HandshakeError::TooLarge { read }) => {
            assert!(
                read >= MAX_HANDSHAKE_BYTES,
                "refused below the bound: {read}"
            );
        }
        other => panic!("an endless header must be refused, got {other:?}"),
    }
}

/// **One `write` is one message, and the payload arrives byte for byte.**
///
/// The equivalence the relay above depends on. Checked in both roles, because the masking rule
/// differs between them and a masking bug shows up as corrupted payload rather than as an error.
#[test]
fn one_write_is_one_message_in_both_roles() {
    for role in [Role::Server, Role::Client] {
        let mut wire = Vec::new();
        let mut writer = Framed::new(&mut wire, role);
        // Three lengths, one on each side of each length encoding's boundary.
        let payloads: Vec<Vec<u8>> = vec![
            b"ODC1-short".to_vec(),
            vec![0xa5; 125],
            vec![0x5a; 126],
            vec![0x33; 70_000],
        ];
        for payload in &payloads {
            writer.write_all(payload).expect("it frames");
        }
        let peer = match role {
            Role::Server => Role::Client,
            Role::Client => Role::Server,
        };
        let mut reader = Unframed::new(wire.as_slice(), peer, Vec::new());
        let mut out = Vec::new();
        reader.read_to_end(&mut out).expect("it unframes");
        let expected: Vec<u8> = payloads.concat();
        assert_eq!(out, expected, "{role:?} lost or corrupted a payload");
    }
}

/// **An unmasked client frame is refused, and a masked server frame is too.**
///
/// RFC 6455 §5.1 makes both mandatory in one direction each, and the defence they buy — a proxy
/// cannot be fed a crafted request that looks like a cacheable response — is worth nothing if
/// the rule is tolerated rather than enforced.
#[test]
fn the_masking_rules_are_refusals_and_not_remarks() {
    // A server reading a frame a *server* wrote: unmasked, therefore wrong.
    let mut unmasked = Vec::new();
    Framed::new(&mut unmasked, Role::Server)
        .write_all(b"hello")
        .expect("it frames");
    let mut reader = Unframed::new(unmasked.as_slice(), Role::Server, Vec::new());
    let mut out = Vec::new();
    let error = reader
        .read_to_end(&mut out)
        .expect_err("a server must refuse an unmasked client frame");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);

    // And the mirror: a client reading a frame a client wrote.
    let mut masked = Vec::new();
    Framed::new(&mut masked, Role::Client)
        .write_all(b"hello")
        .expect("it frames");
    let mut reader = Unframed::new(masked.as_slice(), Role::Client, Vec::new());
    let mut out = Vec::new();
    reader
        .read_to_end(&mut out)
        .expect_err("a client must refuse a masked server frame");
}

/// **A frame over the limit is refused from its header, before its bytes are allocated.**
///
/// The header is thirteen bytes and the declaration is of a gigabyte; the reader must answer
/// without ever having that gigabyte. Asserted by giving it nothing else: if the refusal came
/// from the arrival rather than from the declaration, this would block for ever on a stream
/// that ends immediately.
#[test]
fn a_declared_length_over_the_limit_is_refused_before_it_arrives() {
    let mut header = vec![0x82, 0xff];
    header.extend_from_slice(&1_000_000_000_u64.to_be_bytes());
    header.extend_from_slice(&[0, 0, 0, 0]);
    let mut reader = Unframed::new(header.as_slice(), Role::Server, Vec::new());
    let mut out = Vec::new();
    let error = reader
        .read_to_end(&mut out)
        .expect_err("a gigabyte declaration must be refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        out.is_empty(),
        "nothing may be delivered from a refused frame"
    );
}

/// **A fragmented message is delivered whole, and a ping between its pieces is invisible.**
///
/// Browsers do not fragment today, which is exactly why this is guarded: the layer above has no
/// way to notice a dropped continuation, and a transport that silently truncates one message in
/// a thousand is a transport that corrupts a document.
#[test]
fn fragmentation_and_control_frames_are_transparent() {
    // Hand-built, because `Framed` deliberately never fragments: the shape under test is one a
    // conforming peer may send and this writer does not.
    let frame = |fin: bool, opcode: u8, payload: &[u8]| {
        let mut out = vec![
            (if fin { 0x80 } else { 0 }) | opcode,
            0x80 | (payload.len() as u8),
        ];
        let key = [0x11, 0x22, 0x33, 0x44];
        out.extend_from_slice(&key);
        out.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ key[index % 4]),
        );
        out
    };
    let mut wire = frame(false, 2, b"ODC");
    wire.extend(frame(true, 9, b"ping"));
    wire.extend(frame(false, 0, b"1-pay"));
    wire.extend(frame(true, 0, b"load"));
    let mut reader = Unframed::new(wire.as_slice(), Role::Server, Vec::new());
    let mut out = Vec::new();
    reader.read_to_end(&mut out).expect("it unframes");
    assert_eq!(out, b"ODC1-payload");
}

/// **A close frame reads as end of stream, so a polite departure is distinguishable.**
///
/// `crate::transport::Frames` answers `Closed` between frames and `Interrupted` mid-frame, and
/// that distinction — crash or clean leave — exists only if a close arrives as a zero-length
/// read rather than as an error.
#[test]
fn a_close_frame_ends_the_stream() {
    let mut wire = Vec::new();
    let mut writer = Framed::new(&mut wire, Role::Client);
    writer.write_all(b"last").expect("it frames");
    writer.close().expect("it closes");
    let mut reader = Unframed::new(wire.as_slice(), Role::Server, Vec::new());
    let mut out = Vec::new();
    assert_eq!(reader.read_to_end(&mut out).expect("it unframes"), 4);
    assert_eq!(out, b"last");
}
