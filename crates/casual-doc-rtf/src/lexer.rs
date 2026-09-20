//! One linear, non-recursive pass over an RTF byte stream.
//!
//! The lexer owns every *lexical* bound: control-word length, parameter digit
//! count, and the length of a `\binN` payload. It owns no semantics — it does
//! not know what a destination is — so the semantic layer can be read without
//! also holding the byte grammar in mind.

use crate::limits::enforce;
use crate::{RtfError, RtfLimits};

/// One lexical unit of an RTF stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Token<'a> {
    /// `{`
    GroupStart,
    /// `}`
    GroupEnd,
    /// `\word` with an optional signed parameter.
    ControlWord {
        /// ASCII letters only, at most [`RtfLimits::MAX_CONTROL_WORD_BYTES`].
        name: &'a str,
        /// The decimal parameter, when the control word carried one.
        parameter: Option<i32>,
    },
    /// `\<non-letter>`, e.g. `\\`, `\{`, `\*`, `\~`.
    ControlSymbol(u8),
    /// `\'hh` — one raw byte in the code page currently in force.
    HexByte(u8),
    /// A run of literal bytes containing no `{`, `}`, `\`, CR, or LF.
    Text(&'a [u8]),
}

/// Cursor over the admitted RTF bytes.
#[derive(Debug)]
pub(crate) struct Lexer<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Lexer<'a> {
    /// Creates a lexer over already length-checked bytes.
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    /// Returns the next token, or `None` at end of input.
    ///
    /// A bare CR or LF is RTF's cosmetic line wrapping and is skipped here, so
    /// the semantic layer never has to remember that a newline is not text. A
    /// backslash *immediately* followed by CR or LF is a paragraph mark, and is
    /// returned as the `par` control word, which is what the specification says
    /// it means.
    pub(crate) fn next_token(&mut self) -> Result<Option<Token<'a>>, RtfError> {
        loop {
            let Some(&byte) = self.bytes.get(self.position) else {
                return Ok(None);
            };
            match byte {
                b'{' => {
                    self.position += 1;
                    return Ok(Some(Token::GroupStart));
                }
                b'}' => {
                    self.position += 1;
                    return Ok(Some(Token::GroupEnd));
                }
                b'\\' => return self.lex_escape().map(Some),
                b'\r' | b'\n' => {
                    self.position += 1;
                }
                // A NUL inside the stream is not text in any producer's output
                // and is the classic way to smuggle a terminator past a
                // downstream C consumer; drop it rather than carry it into a
                // Rust `String` that would happily hold it.
                0 => {
                    self.position += 1;
                }
                _ => {
                    let start = self.position;
                    while let Some(&next) = self.bytes.get(self.position) {
                        if matches!(next, b'{' | b'}' | b'\\' | b'\r' | b'\n' | 0) {
                            break;
                        }
                        self.position += 1;
                    }
                    return Ok(Some(Token::Text(&self.bytes[start..self.position])));
                }
            }
        }
    }

    /// Consumes `length` raw bytes of a `\binN` payload.
    ///
    /// Returns `Malformed` when the declaration runs past the end of the
    /// stream. The caller must have already charged `length` against its own
    /// byte budget: the declaration is checked *before* anything is copied, so
    /// a `\bin999999999` in a two-kilobyte file cannot allocate.
    pub(crate) fn take_binary(&mut self, length: usize) -> Result<&'a [u8], RtfError> {
        let end = self
            .position
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(RtfError::Malformed {
                reason: "a \\bin payload declares more bytes than the stream holds",
            })?;
        let payload = &self.bytes[self.position..end];
        self.position = end;
        Ok(payload)
    }

    fn lex_escape(&mut self) -> Result<Token<'a>, RtfError> {
        debug_assert_eq!(self.bytes.get(self.position), Some(&b'\\'));
        self.position += 1;
        let Some(&byte) = self.bytes.get(self.position) else {
            return Err(RtfError::Malformed {
                reason: "the stream ends in a backslash",
            });
        };
        if byte.is_ascii_alphabetic() {
            return self.lex_control_word();
        }
        self.position += 1;
        match byte {
            b'\'' => self.lex_hex_byte(),
            // `\<CR>` and `\<LF>` are paragraph marks per the specification.
            b'\r' | b'\n' => {
                // A CRLF pair after a backslash is one paragraph, not two.
                if byte == b'\r' && self.bytes.get(self.position) == Some(&b'\n') {
                    self.position += 1;
                }
                Ok(Token::ControlWord {
                    name: "par",
                    parameter: None,
                })
            }
            _ => Ok(Token::ControlSymbol(byte)),
        }
    }

    fn lex_control_word(&mut self) -> Result<Token<'a>, RtfError> {
        let start = self.position;
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            self.position += 1;
            enforce(
                "rtf_control_word_bytes",
                self.position - start,
                RtfLimits::MAX_CONTROL_WORD_BYTES,
            )?;
        }
        // Every byte in the span was checked `is_ascii_alphabetic`, so this is
        // ASCII by construction; the conversion cannot fail.
        let name = std::str::from_utf8(&self.bytes[start..self.position]).map_err(|_| {
            RtfError::Malformed {
                reason: "a control word contains a non-ASCII byte",
            }
        })?;

        let negative = self.bytes.get(self.position) == Some(&b'-');
        if negative {
            self.position += 1;
        }
        let digits_start = self.position;
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
            enforce(
                "rtf_parameter_digits",
                self.position - digits_start,
                RtfLimits::MAX_PARAMETER_DIGITS,
            )?;
        }
        let parameter = if self.position > digits_start {
            let text =
                std::str::from_utf8(&self.bytes[digits_start..self.position]).map_err(|_| {
                    RtfError::Malformed {
                        reason: "a control-word parameter contains a non-ASCII byte",
                    }
                })?;
            // Ten digits can still overflow `i32`; saturate rather than wrap,
            // because every consumer of a parameter range-checks it anyway and
            // a wrapped length is the dangerous case.
            let magnitude: i64 = text.parse().unwrap_or(i64::from(i32::MAX));
            let signed = if negative { -magnitude } else { magnitude };
            Some(signed.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
        } else if negative {
            // `\-` is the optional-hyphen control symbol, not a control word;
            // a bare minus with no digits after letters is malformed input we
            // treat as "no parameter" rather than refusing the document.
            None
        } else {
            None
        };

        // Exactly one delimiting space is part of the control word.
        if self.bytes.get(self.position) == Some(&b' ') {
            self.position += 1;
        }
        Ok(Token::ControlWord { name, parameter })
    }

    fn lex_hex_byte(&mut self) -> Result<Token<'a>, RtfError> {
        let high = self.hex_digit()?;
        let low = self.hex_digit()?;
        Ok(Token::HexByte((high << 4) | low))
    }

    fn hex_digit(&mut self) -> Result<u8, RtfError> {
        let byte = self
            .bytes
            .get(self.position)
            .copied()
            .ok_or(RtfError::Malformed {
                reason: "the stream ends inside a \\'hh escape",
            })?;
        let value = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => {
                return Err(RtfError::Malformed {
                    reason: "a \\'hh escape is not followed by two hex digits",
                });
            }
        };
        self.position += 1;
        Ok(value)
    }
}

/// Returns whether the bytes carry an RTF signature.
///
/// Skips a UTF-8 BOM and leading ASCII whitespace, then requires `{`,
/// optional whitespace, and `\rtf`. Deliberately strict: a file named `.rtf`
/// whose bytes are not RTF must be refused, not opened as prose, because
/// opening a control-word stream as text is the "wrong characters" failure
/// this adapter exists to avoid.
#[must_use]
pub fn is_rtf(bytes: &[u8]) -> bool {
    let mut rest = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    // A brace-less prefix of arbitrary length is not RTF; bound the scan so a
    // probe over a 256 MiB file of spaces is still O(small).
    let mut skipped = 0_usize;
    while let Some((first, tail)) = rest.split_first() {
        if !first.is_ascii_whitespace() || skipped >= 16 {
            break;
        }
        rest = tail;
        skipped += 1;
    }
    let Some(mut rest) = rest.strip_prefix(b"{") else {
        return false;
    };
    skipped = 0;
    while let Some((first, tail)) = rest.split_first() {
        if !first.is_ascii_whitespace() || skipped >= 16 {
            break;
        }
        rest = tail;
        skipped += 1;
    }
    rest.starts_with(b"\\rtf")
}
