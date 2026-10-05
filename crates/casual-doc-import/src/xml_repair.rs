// SPDX-License-Identifier: Apache-2.0

//! Byte-level repair of a damaged XML part, so a strict reader can read it.
//!
//! # Why a repair pass at all
//!
//! The importer's streaming readers stop at the first XML error, which is the
//! right behaviour for a *reader*: it cannot invent a tree. On a damaged file that
//! costs everything after the damage, and for three damage shapes it costs
//! everything **including** the damage, because the error is at the very start of
//! the stream:
//!
//! - a document-type declaration (refused for entity-expansion reasons, and
//!   written by producers that emit a boilerplate DTD);
//! - an XML declaration naming an encoding the reader will not decode;
//! - a byte sequence that is not valid UTF-8 in the first element.
//!
//! Stopping at the damage recovers nothing from those. Repairing the bytes first
//! recovers the whole document.
//!
//! # The known pattern
//!
//! This is **error recovery with diagnostics**, as a compiler front end does it,
//! and the shape is taken from the HTML parsing specification rather than
//! invented: a damaged document has a *defined* recovery, every recovery is
//! recorded, and no input is rejected. The one rule that keeps it honest is that
//! a repair is never silent — every branch below pushes a [`Repair`], and
//! `docs/35`'s prohibited-silent-loss rule is why.
//!
//! # What it deliberately does not do
//!
//! It does not validate against the WordprocessingML schema, re-order misplaced
//! elements, or guess at a missing attribute. It makes the bytes *well-formed*
//! and nothing more; what the markup then means is the importer's business, and
//! an importer that reads a repaired part reports its own losses as usual.
//!
//! # Complexity and bounds
//!
//! One linear pass, `O(bytes)`, with output bounded by input plus the closing
//! tags for at most [`MAX_OPEN_ELEMENTS`] open elements. Nothing here runs on a
//! healthy document: the caller only reaches it after a strict parse has failed.

use crate::recovery::{Repair, RepairKind};

/// Open-element ceiling for the repair stack. Past it, nesting is dropped rather
/// than tracked: a file this deep is damaged in a way no close-the-tags repair
/// helps, and an unbounded stack on adversarial input is the bug this prevents.
const MAX_OPEN_ELEMENTS: usize = 4_096;

/// Byte ceiling on a scan for the `;` that ends an entity reference.
const MAX_ENTITY_BYTES: usize = 64;

/// Repairs `bytes` into well-formed XML, or returns `None` when there was
/// nothing to repair.
///
/// `None` is not "this part is fine": the caller reaches here only after a strict
/// parse failed, so `None` means the failure is **not** a well-formedness one —
/// a resource limit, or a construct the importer refuses on purpose — and the
/// caller must not pretend a repair happened.
pub(crate) fn repair(bytes: &[u8]) -> Option<(Vec<u8>, Vec<Repair>)> {
    let mut repairs: Vec<Repair> = Vec::new();
    let decoded = decode_lossy(bytes, &mut repairs);
    let source = decoded.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(source.len() + 64);
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut opened = false;
    let mut cursor = skip_declaration(source, &mut out, &mut repairs);

    while cursor < source.len() {
        // Past the close of the root element only whitespace is legal. Anything
        // else is appended junk — a doubled package part, a shell prompt, a
        // second copy of the document — and is discarded rather than fed to a
        // reader that will refuse the whole part because of it.
        if opened && stack.is_empty() {
            let rest = &source[cursor..];
            if rest.iter().any(|byte| !byte.is_ascii_whitespace()) {
                repairs.push(
                    Repair::new(RepairKind::TrailingBytesDiscarded)
                        .with_detail(&format!("{} bytes after the document", rest.len())),
                );
            } else {
                out.extend_from_slice(rest);
            }
            break;
        }
        match source[cursor] {
            b'<' => match classify(source, cursor) {
                Markup::Comment(end) | Markup::CData(end) | Markup::Instruction(end) => {
                    out.extend_from_slice(&source[cursor..end]);
                    cursor = end;
                }
                Markup::Doctype(end) => {
                    repairs.push(Repair::new(RepairKind::DoctypeRemoved));
                    cursor = end;
                }
                Markup::Start { end, name, empty } => {
                    scrub_into(&source[cursor..end], &mut out, &mut repairs);
                    if !empty {
                        if stack.len() < MAX_OPEN_ELEMENTS {
                            stack.push(name.to_vec());
                        } else {
                            repairs.push(
                                Repair::new(RepairKind::TrailingBytesDiscarded)
                                    .with_detail("nesting beyond the depth this engine tracks"),
                            );
                        }
                        opened = true;
                    } else if stack.is_empty() {
                        opened = true;
                    }
                    cursor = end;
                }
                Markup::End { end, name } => {
                    match stack.iter().rposition(|open| open == name) {
                        Some(position) => {
                            // Close everything the file left open inside this
                            // element before closing it, so `<a><b></a>` becomes
                            // `<a><b></b></a>` rather than losing `b`'s content.
                            while stack.len() > position + 1 {
                                let unclosed = stack.pop().unwrap_or_default();
                                repairs.push(
                                    Repair::new(RepairKind::UnclosedElementClosed).with_detail(
                                        &String::from_utf8_lossy(&unclosed),
                                    ),
                                );
                                out.extend_from_slice(b"</");
                                out.extend_from_slice(&unclosed);
                                out.push(b'>');
                            }
                            stack.pop();
                            out.extend_from_slice(&source[cursor..end]);
                        }
                        None => repairs.push(
                            Repair::new(RepairKind::StrayEndTagRemoved)
                                .with_detail(&String::from_utf8_lossy(name)),
                        ),
                    }
                    cursor = end;
                }
                Markup::Unterminated => {
                    repairs.push(
                        Repair::new(RepairKind::TrailingBytesDiscarded)
                            .with_detail("a tag cut off before its end"),
                    );
                    cursor = source.len();
                }
                Markup::NotMarkup => {
                    repairs.push(Repair::new(RepairKind::MalformedMarkupEscaped));
                    out.extend_from_slice(b"&lt;");
                    cursor += 1;
                }
            },
            b'&' => cursor = scrub_entity(source, cursor, &mut out, &mut repairs),
            byte if is_forbidden_char(byte) => {
                repairs.push(Repair::new(RepairKind::InvalidTextBytesReplaced));
                cursor += 1;
            }
            _ => {
                out.push(source[cursor]);
                cursor += 1;
            }
        }
    }

    // A truncated file ends mid-element. Closing the open elements in reverse is
    // what lets everything before the cut be read; without it a reader sees one
    // unbalanced document and keeps nothing.
    while let Some(unclosed) = stack.pop() {
        repairs.push(
            Repair::new(RepairKind::UnclosedElementClosed)
                .with_detail(&String::from_utf8_lossy(&unclosed)),
        );
        out.extend_from_slice(b"</");
        out.extend_from_slice(&unclosed);
        out.push(b'>');
    }

    if repairs.is_empty() {
        return None;
    }
    Some((out, repairs))
}

/// Decodes `bytes` as UTF-8, replacing each invalid sequence with `U+FFFD` and
/// counting the replacements as one repair each.
///
/// `String::from_utf8_lossy` does the same substitution but cannot say how often,
/// and "some characters were replaced" with no count is the kind of report a
/// reader cannot judge, so the error positions are walked explicitly.
/// What a main document's own markup says it is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MainDocumentShape {
    /// A `w:document` with a `w:body` in it: the expected shape.
    Wordprocessing,
    /// A `w:document` with no `w:body`.
    NoBody,
    /// A root element that is not `document`, named here for the report.
    NotWordprocessing(String),
    /// No element at all.
    Empty,
}

/// Classifies a main document part by its outermost markup.
///
/// # Why this is a separate scan rather than a flag on the body parser
///
/// Two shapes of damage are **invisible** to the body parser, and both used to
/// be accepted in silence:
///
/// - a main part whose root is not `w:document` — measured on a synthetic
///   package holding `<html><body>hi</body></html>`, which imported as a
///   one-block document with an **empty** compatibility report, because every
///   element in it either fell through the reporter's no-meaning filter or was
///   never dispatched;
/// - a `w:document` with no `w:body`.
///
/// Neither is a parse *error*, so neither reaches a recovery arm; and a terminal
/// check inside the parser cannot help, because the parse those two produce
/// **succeeds** — it is the strict rung of the ladder that accepts them, where
/// nothing is recovering. So the observation has to happen outside the parser, on
/// the bytes.
///
/// # Complexity
///
/// `O(bytes)` worst case, but it stops as soon as both facts are known: for a
/// healthy document that is inside the first element or two, a few hundred bytes.
/// The whole part is only walked when there is no body to find, which is the
/// damaged case this exists for.
pub(crate) fn main_document_shape(bytes: &[u8]) -> MainDocumentShape {
    let mut root: Option<String> = None;
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'<' {
            cursor += 1;
            continue;
        }
        match classify(bytes, cursor) {
            Markup::Start { end, name, .. } | Markup::End { end, name } => {
                let local = name.rsplit(|byte| *byte == b':').next().unwrap_or(name);
                match &root {
                    None => {
                        if local == b"body" {
                            // A `w:body` outside any root: treat the part as a
                            // body-bearing document rather than inventing a root.
                            return MainDocumentShape::Wordprocessing;
                        }
                        if local != b"document" {
                            return MainDocumentShape::NotWordprocessing(
                                String::from_utf8_lossy(name).into_owned(),
                            );
                        }
                        root = Some(String::from_utf8_lossy(name).into_owned());
                    }
                    Some(_) if local == b"body" => return MainDocumentShape::Wordprocessing,
                    Some(_) => {}
                }
                cursor = end;
            }
            Markup::Comment(end)
            | Markup::CData(end)
            | Markup::Instruction(end)
            | Markup::Doctype(end) => cursor = end,
            Markup::Unterminated => break,
            Markup::NotMarkup => cursor += 1,
        }
    }
    match root {
        Some(_) => MainDocumentShape::NoBody,
        None => MainDocumentShape::Empty,
    }
}

/// Decodes `bytes` as UTF-8, replacing each invalid sequence with `U+FFFD` and
/// counting the replacements as one repair each.
pub(crate) fn decode_text_lossy(bytes: &[u8]) -> (String, Vec<Repair>) {
    let mut repairs = Vec::new();
    let text = decode_lossy(bytes, &mut repairs);
    (text, repairs)
}

/// Decodes `bytes` as UTF-8, replacing each invalid sequence with `U+FFFD` and
/// counting the replacements as one repair each.
fn decode_lossy(bytes: &[u8], repairs: &mut Vec<Repair>) -> String {
    match core::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => {
            let mut out = String::with_capacity(bytes.len());
            let mut rest = bytes;
            loop {
                match core::str::from_utf8(rest) {
                    Ok(text) => {
                        out.push_str(text);
                        break;
                    }
                    Err(error) => {
                        let valid = error.valid_up_to();
                        // `valid_up_to` is by definition a valid boundary.
                        out.push_str(&String::from_utf8_lossy(&rest[..valid]));
                        out.push('\u{fffd}');
                        repairs.push(Repair::new(RepairKind::InvalidTextBytesReplaced));
                        let skip = error.error_len().unwrap_or(rest.len() - valid).max(1);
                        let advance = valid.saturating_add(skip).min(rest.len());
                        if advance == 0 {
                            break;
                        }
                        rest = &rest[advance..];
                    }
                }
            }
            out
        }
    }
}

/// Copies a leading XML declaration through, rewriting an encoding this engine
/// does not decode. Returns the cursor to continue scanning from.
///
/// Every part this engine reads is decoded as UTF-8. A declaration naming
/// anything else is either a lie (the common case: a UTF-8 file mislabelled by a
/// producer, which Word itself opens) or a genuinely different encoding, and the
/// honest repair is the same in both: read it as UTF-8 and say so, rather than
/// refuse the file. Transcoding from a declared legacy codepage is deliberately
/// not attempted — it would need a codepage table per name and would guess at
/// which of several names a producer meant.
fn skip_declaration(source: &[u8], out: &mut Vec<u8>, repairs: &mut Vec<Repair>) -> usize {
    if !source.starts_with(b"<?xml") {
        return 0;
    }
    let Some(offset) = find(source, b"?>", 0) else {
        return 0;
    };
    let end = offset + 2;
    let declaration = &source[..end];
    match declared_encoding(declaration) {
        Some(encoding) if !is_utf8_name(&encoding) => {
            repairs.push(
                Repair::new(RepairKind::XmlDeclarationNormalized)
                    .with_detail(&format!("declared {encoding}")),
            );
            out.extend_from_slice(br#"<?xml version="1.0" encoding="UTF-8"?>"#);
        }
        _ => out.extend_from_slice(declaration),
    }
    end
}

/// Extracts the `encoding="…"` value of an XML declaration.
fn declared_encoding(declaration: &[u8]) -> Option<String> {
    let at = find(declaration, b"encoding", 0)?;
    let rest = &declaration[at + b"encoding".len()..];
    let mut index = 0;
    while index < rest.len() && (rest[index].is_ascii_whitespace() || rest[index] == b'=') {
        index += 1;
    }
    let quote = *rest.get(index)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    index += 1;
    let start = index;
    while index < rest.len() && rest[index] != quote {
        index += 1;
    }
    Some(String::from_utf8_lossy(&rest[start..index]).into_owned())
}

/// Whether an encoding name denotes a UTF-8-compatible decoding of the bytes.
/// ASCII is a strict subset of UTF-8, so a correctly-labelled ASCII part needs no
/// repair.
fn is_utf8_name(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    matches!(
        name.as_str(),
        "utf-8" | "utf8" | "us-ascii" | "ascii" | "iso-8859-1" | "latin1" | ""
    )
}

/// What the markup starting at `<` is.
enum Markup<'a> {
    Comment(usize),
    CData(usize),
    Instruction(usize),
    Doctype(usize),
    Start {
        end: usize,
        name: &'a [u8],
        empty: bool,
    },
    End {
        end: usize,
        name: &'a [u8],
    },
    /// A tag that never closes: the file is cut off inside it.
    Unterminated,
    /// A `<` that cannot begin a tag, so it is text the producer failed to
    /// escape.
    NotMarkup,
}

/// Classifies the markup at `start`, which must be a `<`.
fn classify(source: &[u8], start: usize) -> Markup<'_> {
    let rest = &source[start..];
    if rest.starts_with(b"<!--") {
        return match find(source, b"-->", start + 4) {
            Some(at) => Markup::Comment(at + 3),
            None => Markup::Unterminated,
        };
    }
    if rest.starts_with(b"<![CDATA[") {
        return match find(source, b"]]>", start + 9) {
            Some(at) => Markup::CData(at + 3),
            None => Markup::Unterminated,
        };
    }
    if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
        return match doctype_end(source, start) {
            Some(end) => Markup::Doctype(end),
            None => Markup::Unterminated,
        };
    }
    if rest.starts_with(b"<?") {
        return match find(source, b"?>", start + 2) {
            Some(at) => Markup::Instruction(at + 2),
            None => Markup::Unterminated,
        };
    }
    if rest.starts_with(b"</") {
        let name_start = start + 2;
        let name_end = name_end(source, name_start);
        if name_end == name_start {
            return Markup::NotMarkup;
        }
        return match tag_end(source, name_end) {
            Some(end) => Markup::End {
                end,
                name: &source[name_start..name_end],
            },
            None => Markup::Unterminated,
        };
    }
    let name_start = start + 1;
    let name_end = name_end(source, name_start);
    if name_end == name_start {
        return Markup::NotMarkup;
    }
    match tag_end(source, name_end) {
        Some(end) => Markup::Start {
            end,
            name: &source[name_start..name_end],
            empty: end >= 2 && source[end - 2] == b'/',
        },
        None => Markup::Unterminated,
    }
}

/// Finds the end of a `<!DOCTYPE …>`, honouring an internal subset in brackets.
fn doctype_end(source: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    let mut subset = 0_u32;
    while index < source.len() {
        match source[index] {
            b'[' => subset = subset.saturating_add(1),
            b']' => subset = subset.saturating_sub(1),
            b'>' if subset == 0 => return Some(index + 1),
            _ => {}
        }
        index += 1;
    }
    None
}

/// Finds the `>` that closes a tag, skipping quoted attribute values so a `>`
/// inside one does not end the tag early.
fn tag_end(source: &[u8], from: usize) -> Option<usize> {
    let mut index = from;
    let mut quote = 0_u8;
    while index < source.len() {
        let byte = source[index];
        if quote != 0 {
            if byte == quote {
                quote = 0;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = byte;
        } else if byte == b'>' {
            return Some(index + 1);
        } else if byte == b'<' {
            // A second `<` before this tag closed: the tag is truncated, and
            // treating the rest as its attributes would swallow the document.
            return None;
        }
        index += 1;
    }
    None
}

/// The end of an XML name starting at `from`.
fn name_end(source: &[u8], from: usize) -> usize {
    let mut index = from;
    if index < source.len() && !is_name_start(source[index]) {
        return from;
    }
    while index < source.len() && is_name_char(source[index]) {
        index += 1;
    }
    index
}

const fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':' || byte >= 0x80
}

const fn is_name_char(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
}

/// Characters XML 1.0 forbids outright: the C0 controls other than tab, line
/// feed and carriage return. A reader refuses the whole part because of one, and
/// they carry no document text.
const fn is_forbidden_char(byte: u8) -> bool {
    matches!(byte, 0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F)
}

/// Copies a tag through, scrubbing entity references inside its attribute values.
fn scrub_into(tag: &[u8], out: &mut Vec<u8>, repairs: &mut Vec<Repair>) {
    let mut cursor = 0;
    while cursor < tag.len() {
        if tag[cursor] == b'&' {
            cursor = scrub_entity(tag, cursor, out, repairs);
        } else {
            out.push(tag[cursor]);
            cursor += 1;
        }
    }
}

/// Resolves what to emit for the `&` at `start`, returning the next cursor.
///
/// The five predefined entities and numeric character references are copied
/// through for the reader to unescape. A reference to an entity the part never
/// declares is **removed**, with a repair naming it: this engine refuses DTDs, so
/// there is no declaration to read and no text the reference could stand for. A
/// bare `&` is escaped, which is the single most common hand-edit damage in the
/// wild (a URL query string written into an attribute unescaped).
fn scrub_entity(source: &[u8], start: usize, out: &mut Vec<u8>, repairs: &mut Vec<Repair>) -> usize {
    let limit = source.len().min(start + MAX_ENTITY_BYTES);
    let terminator = source[start..limit].iter().position(|byte| *byte == b';');
    let Some(offset) = terminator else {
        out.extend_from_slice(b"&amp;");
        repairs.push(
            Repair::new(RepairKind::UndeclaredEntityRemoved).with_detail("a bare & in the text"),
        );
        return start + 1;
    };
    let end = start + offset + 1;
    let name = &source[start + 1..end - 1];
    if is_known_entity(name) {
        out.extend_from_slice(&source[start..end]);
    } else {
        repairs.push(
            Repair::new(RepairKind::UndeclaredEntityRemoved)
                .with_detail(&String::from_utf8_lossy(name)),
        );
    }
    end
}

/// Whether `name` is one of the five predefined entities or a numeric character
/// reference.
fn is_known_entity(name: &[u8]) -> bool {
    if matches!(name, b"amp" | b"lt" | b"gt" | b"quot" | b"apos") {
        return true;
    }
    match name {
        [b'#', b'x' | b'X', digits @ ..] if !digits.is_empty() => {
            digits.iter().all(u8::is_ascii_hexdigit)
        }
        [b'#', digits @ ..] if !digits.is_empty() => digits.iter().all(u8::is_ascii_digit),
        _ => false,
    }
}

/// `needle` in `haystack` at or after `from`.
fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}
