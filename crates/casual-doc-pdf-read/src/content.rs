//! Content-stream operator spans (`docs/182` §7, P4).
//!
//! The byte-preserving save removes the text-showing operators of edited
//! frames from a page's content and leaves every other operator untouched. That
//! needs each operator's **byte range**, which hayro's iterators do not expose,
//! so this module tokenizes a decoded content stream into operator spans.
//!
//! Two independent readers must agree before any span is trusted: after
//! tokenizing, [`agrees_with_hayro`] walks the same bytes with hayro's own
//! `UntypedIter` and compares the operator sequence. A stream they disagree
//! about (an inline image whose end is ambiguous, hayro's skip of a malformed
//! token) is never instrumented or rewritten: its page is reported as not
//! preservable and saved semantically.
//!
//! Text-showing operators are numbered in stream order (`Tj`, `TJ`, `'`, `"`:
//! the *show index*). At import, [`instrument`] wraps show `k` in a
//! `/OpenDocShow<k> BMC … EMC` marked-content pair, so the evidence device
//! learns which operator painted every glyph run (marked content is legal
//! inside a text object, ISO 32000-1 §14.6, and paints nothing). At save,
//! [`remove_shows`] drops the shows of the edited frames.
//!
//! Complexity: O(stream bytes) for every function here.

use std::collections::BTreeSet;

/// The marked-content tag prefix that numbers a show operator.
pub(crate) const SHOW_TAG: &[u8] = b"OpenDocShow";

/// One operator and its operands, as byte ranges into the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpSpan {
    /// First byte of the first operand (or of the operator, with none).
    pub start: usize,
    /// One past the operator's last byte.
    pub end: usize,
    /// The operator's own range.
    pub name: (usize, usize),
    /// Each top-level operand's range.
    pub operands: Vec<(usize, usize)>,
}

/// Why a page's content cannot be rewritten safely. Each is a reason the save
/// reports when it falls back to the semantic writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewriteRefusal {
    /// The two tokenizers disagree, or the stream does not tokenize.
    Untokenizable,
    /// Removing a show would move a later, kept show on the same line: the
    /// text matrix advances by what a show draws, so a kept show that follows
    /// a removed one with no line-positioning operator between would shift.
    KeptShowWouldMove,
    /// A removed show uses a clipping text render mode (4–7), so removing it
    /// would also remove a clip other content relies on.
    ClippingText,
    /// The stream leaves a text object open or restores more states than it
    /// saves, so it cannot be bracketed by `q`/`Q`.
    Unbalanced,
}

impl RewriteRefusal {
    /// A stable id for the report.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Untokenizable => "content_untokenizable",
            Self::KeptShowWouldMove => "kept_text_would_move",
            Self::ClippingText => "clipping_text",
            Self::Unbalanced => "content_unbalanced",
        }
    }
}

fn is_white(byte: u8) -> bool {
    matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_regular(byte: u8) -> bool {
    !is_white(byte) && !is_delimiter(byte)
}

/// Whether a show operator.
pub(crate) fn is_show(name: &[u8]) -> bool {
    matches!(name, b"Tj" | b"TJ" | b"'" | b"\"")
}

/// A low-level lexer over PDF object syntax, shared by the content tokenizer
/// and the dictionary splitter.
pub(crate) struct Lexer<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) at: usize,
}

/// One token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    /// A complete string, name, number or keyword at `(start, end)`.
    Atom(usize, usize),
    /// `[` or `<<`.
    Open(usize, usize),
    /// `]` or `>>`.
    Close(usize, usize),
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(data: &'a [u8], at: usize) -> Self {
        Self { data, at }
    }

    pub(crate) fn skip_white(&mut self) {
        while let Some(&byte) = self.data.get(self.at) {
            if is_white(byte) {
                self.at += 1;
            } else if byte == b'%' {
                while let Some(&byte) = self.data.get(self.at) {
                    if byte == b'\n' || byte == b'\r' {
                        break;
                    }
                    self.at += 1;
                }
            } else {
                break;
            }
        }
    }

    /// The next token, `None` at the end, `Some(Err)` on malformed input.
    pub(crate) fn next(&mut self) -> Option<Result<Token, ()>> {
        self.skip_white();
        let start = self.at;
        let byte = *self.data.get(start)?;
        let data = self.data;
        Some(match byte {
            b'(' => {
                let mut depth = 0usize;
                let mut at = start;
                loop {
                    match data.get(at) {
                        None => return Some(Err(())),
                        Some(b'\\') => at += 2,
                        Some(b'(') => {
                            depth += 1;
                            at += 1;
                        }
                        Some(b')') => {
                            depth -= 1;
                            at += 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        Some(_) => at += 1,
                    }
                }
                self.at = at;
                Ok(Token::Atom(start, at))
            }
            b'<' if data.get(start + 1) == Some(&b'<') => {
                self.at = start + 2;
                Ok(Token::Open(start, start + 2))
            }
            b'>' if data.get(start + 1) == Some(&b'>') => {
                self.at = start + 2;
                Ok(Token::Close(start, start + 2))
            }
            b'<' => match data[start..].iter().position(|b| *b == b'>') {
                Some(offset) => {
                    let end = start + offset + 1;
                    if data[start + 1..end - 1]
                        .iter()
                        .any(|b| !b.is_ascii_hexdigit() && !is_white(*b))
                    {
                        return Some(Err(()));
                    }
                    self.at = end;
                    Ok(Token::Atom(start, end))
                }
                None => Err(()),
            },
            b'[' => {
                self.at = start + 1;
                Ok(Token::Open(start, start + 1))
            }
            b']' => {
                self.at = start + 1;
                Ok(Token::Close(start, start + 1))
            }
            b'/' => {
                let mut at = start + 1;
                while data.get(at).is_some_and(|b| is_regular(*b)) {
                    at += 1;
                }
                self.at = at;
                Ok(Token::Atom(start, at))
            }
            byte if is_regular(byte) => {
                let mut at = start;
                while data.get(at).is_some_and(|b| is_regular(*b)) {
                    at += 1;
                }
                self.at = at;
                Ok(Token::Atom(start, at))
            }
            // `)`, `>`, `{`, `}` out of place.
            _ => Err(()),
        })
    }
}

/// Whether a bare token is an operand rather than an operator.
fn is_operand_keyword(token: &[u8]) -> bool {
    match token.first() {
        Some(b'/' | b'(' | b'<') => true,
        Some(b'0'..=b'9' | b'+' | b'-' | b'.') => true,
        _ => matches!(token, b"true" | b"false" | b"null"),
    }
}

/// Splits a decoded content stream into operator spans, or `None` when it does
/// not tokenize. O(bytes).
#[must_use]
pub fn tokenize(data: &[u8]) -> Option<Vec<OpSpan>> {
    let mut lexer = Lexer::new(data, 0);
    let mut ops = Vec::new();
    let mut start: Option<usize> = None;
    let mut operands = Vec::new();
    let mut depth = 0usize;
    let mut operand_start = 0usize;
    while let Some(token) = lexer.next() {
        let token = token.ok()?;
        match token {
            Token::Open(s, _) => {
                start.get_or_insert(s);
                if depth == 0 {
                    operand_start = s;
                }
                depth += 1;
            }
            Token::Close(_, e) => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    operands.push((operand_start, e));
                }
            }
            Token::Atom(s, e) => {
                start.get_or_insert(s);
                let text = &data[s..e];
                if depth > 0 {
                    continue;
                }
                if is_operand_keyword(text) {
                    operands.push((s, e));
                    continue;
                }
                let mut end = e;
                if text == b"BI" {
                    end = inline_image_end(data, e)?;
                    lexer.at = end;
                }
                ops.push(OpSpan {
                    start: start.take().unwrap_or(s),
                    end,
                    name: (s, e),
                    operands: std::mem::take(&mut operands),
                });
            }
        }
    }
    (depth == 0).then_some(ops)
}

/// The end of an inline image whose `BI` ends at `from`: past the dictionary,
/// `ID`, one whitespace byte, the data, and an `EI` with whitespace (or the
/// end) on both sides. An ambiguous end is caught by [`agrees_with_hayro`].
fn inline_image_end(data: &[u8], from: usize) -> Option<usize> {
    let mut lexer = Lexer::new(data, from);
    loop {
        match lexer.next()?.ok()? {
            Token::Atom(s, e) if &data[s..e] == b"ID" => break,
            _ => {}
        }
    }
    let mut at = lexer.at + 1;
    while at + 2 <= data.len() {
        if &data[at..at + 2] == b"EI"
            && data.get(at - 1).is_some_and(|b| is_white(*b))
            && data.get(at + 2).is_none_or(|b| is_white(*b))
        {
            return Some(at + 2);
        }
        at += 1;
    }
    None
}

/// Whether hayro reads the same operator sequence from `data`. O(bytes).
#[must_use]
pub fn agrees_with_hayro(data: &[u8], ops: &[OpSpan]) -> bool {
    let mut iter = hayro_syntax::content::UntypedIter::new(data);
    let mut index = 0;
    while let Some(instruction) = iter.next() {
        let name: &[u8] = instruction.operator;
        match ops.get(index) {
            Some(op) if &data[op.name.0..op.name.1] == name => index += 1,
            _ => return false,
        }
    }
    index == ops.len()
}

/// Wraps every show operator in a numbered marked-content pair, or `None` when
/// the stream already uses the tag prefix (so a marker cannot be forged).
/// Returns the instrumented bytes and the number of shows. O(bytes).
#[must_use]
pub fn instrument(data: &[u8], ops: &[OpSpan]) -> Option<(Vec<u8>, u32)> {
    if data.windows(SHOW_TAG.len()).any(|w| w == SHOW_TAG) {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() + ops.len() * 24);
    let mut copied = 0;
    let mut show = 0u32;
    for op in ops {
        if !is_show(&data[op.name.0..op.name.1]) {
            continue;
        }
        out.extend_from_slice(&data[copied..op.start]);
        out.push(b'/');
        out.extend_from_slice(SHOW_TAG);
        out.extend_from_slice(format!("{show} BMC ").as_bytes());
        out.extend_from_slice(&data[op.start..op.end]);
        out.extend_from_slice(b" EMC");
        copied = op.end;
        show = show.checked_add(1)?;
    }
    out.extend_from_slice(&data[copied..]);
    Some((out, show))
}

/// The show index a marked-content tag names, if it is one of ours.
pub(crate) fn show_of_tag(tag: &[u8]) -> Option<u32> {
    let digits = tag.strip_prefix(SHOW_TAG)?;
    std::str::from_utf8(digits).ok()?.parse().ok()
}

/// `data` without the shows in `remove`, keeping every other operator
/// byte-for-byte. `'` becomes `T*` and `aw ac (s) "` becomes `aw Tw ac Tc T*`,
/// so the line and spacing state they set survives. O(bytes).
///
/// # Errors
///
/// A [`RewriteRefusal`] when the removal cannot be proven not to disturb kept
/// content.
pub fn remove_shows(data: &[u8], remove: &BTreeSet<u32>) -> Result<Vec<u8>, RewriteRefusal> {
    let ops = tokenize(data).ok_or(RewriteRefusal::Untokenizable)?;
    if !agrees_with_hayro(data, &ops) {
        return Err(RewriteRefusal::Untokenizable);
    }
    let mut out = Vec::with_capacity(data.len());
    let mut copied = 0;
    let mut show = 0u32;
    let mut in_text = false;
    // A removed show since the last operator that sets the text matrix from
    // the line matrix (BT, Td, TD, Tm, T*, ', ").
    let mut moved = false;
    let mut depth: usize = 0;
    let mut render_modes = vec![0i64];
    for op in &ops {
        let name = &data[op.name.0..op.name.1];
        let operand = |i: usize| op.operands.get(i).map(|(s, e)| &data[*s..*e]);
        match name {
            b"q" => {
                depth += 1;
                let mode = *render_modes.last().unwrap_or(&0);
                render_modes.push(mode);
            }
            b"Q" => {
                depth = depth.checked_sub(1).ok_or(RewriteRefusal::Unbalanced)?;
                render_modes.pop();
            }
            b"BT" => {
                in_text = true;
                moved = false;
            }
            b"ET" => {
                in_text = false;
                moved = false;
            }
            b"Td" | b"TD" | b"Tm" | b"T*" => moved = false,
            b"Tr" => {
                let mode = operand(0)
                    .and_then(|o| std::str::from_utf8(o).ok())
                    .and_then(|o| o.parse::<i64>().ok())
                    .unwrap_or(0);
                if let Some(last) = render_modes.last_mut() {
                    *last = mode;
                }
            }
            _ => {}
        }
        if !is_show(name) {
            continue;
        }
        let index = show;
        show += 1;
        let repositions = matches!(name, b"'" | b"\"");
        if repositions {
            moved = false;
        }
        if !remove.contains(&index) {
            if moved {
                return Err(RewriteRefusal::KeptShowWouldMove);
            }
            continue;
        }
        if *render_modes.last().unwrap_or(&0) >= 4 {
            return Err(RewriteRefusal::ClippingText);
        }
        out.extend_from_slice(&data[copied..op.start]);
        match name {
            b"'" => out.extend_from_slice(b"T*"),
            b"\"" => {
                let (Some(aw), Some(ac)) = (operand(0), operand(1)) else {
                    return Err(RewriteRefusal::Untokenizable);
                };
                out.extend_from_slice(aw);
                out.extend_from_slice(b" Tw ");
                out.extend_from_slice(ac);
                out.extend_from_slice(b" Tc T*");
            }
            _ => {}
        }
        copied = op.end;
        moved = true;
    }
    if in_text {
        return Err(RewriteRefusal::Unbalanced);
    }
    out.extend_from_slice(&data[copied..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(data: &[u8]) -> Vec<String> {
        tokenize(data)
            .unwrap()
            .iter()
            .map(|op| String::from_utf8_lossy(&data[op.name.0..op.name.1]).into_owned())
            .collect()
    }

    #[test]
    fn operators_strings_arrays_and_inline_images_tokenize_as_hayro_reads_them() {
        let data: &[u8] = b"q 1 0 0 1 0 0 cm BT /F1 12 Tf (a (nested) \\) str) Tj \
            [(x) -120 <4142>] TJ 0 0 Td 1 2 (q) \" ET % comment Tj\n\
            BI /W 2 /H 1 /BPC 8 /CS /G ID \x00\xff EI Q /P <</MCID 3>> BDC EMC";
        let ops = tokenize(data).unwrap();
        assert_eq!(
            names(data),
            [
                "q", "cm", "BT", "Tf", "Tj", "TJ", "Td", "\"", "ET", "BI", "Q", "BDC", "EMC"
            ]
        );
        assert!(agrees_with_hayro(data, &ops));
        let tj = &ops[4];
        assert_eq!(&data[tj.start..tj.end], b"(a (nested) \\) str) Tj");
    }

    #[test]
    fn instrumenting_numbers_every_show_and_refuses_a_forged_tag() {
        let data: &[u8] = b"BT (a) Tj [(b)] TJ (c) ' ET";
        let ops = tokenize(data).unwrap();
        let (out, shows) = instrument(data, &ops).unwrap();
        assert_eq!(shows, 3);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "BT /OpenDocShow0 BMC (a) Tj EMC /OpenDocShow1 BMC [(b)] TJ EMC \
             /OpenDocShow2 BMC (c) ' EMC ET"
        );
        let forged: &[u8] = b"/OpenDocShow0 BMC BT (a) Tj ET EMC";
        assert!(instrument(forged, &tokenize(forged).unwrap()).is_none());
    }

    #[test]
    fn removing_shows_keeps_every_other_operator_and_the_line_state() {
        let data: &[u8] = b"BT /F1 9 Tf 14 TL 72 700 Td (keep) Tj T* (drop) Tj \
            T* (also) ' 2 1 (quote) \" ET";
        let out = remove_shows(data, &BTreeSet::from([1, 2, 3])).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "BT /F1 9 Tf 14 TL 72 700 Td (keep) Tj T*  \
            T* T* 2 Tw 1 Tc T* ET"
        );
    }

    #[test]
    fn a_kept_show_after_a_removed_one_on_the_same_line_is_refused() {
        let data: &[u8] = b"BT 0 0 Td (drop) Tj (kept) Tj ET";
        assert_eq!(
            remove_shows(data, &BTreeSet::from([0])),
            Err(RewriteRefusal::KeptShowWouldMove)
        );
        // Repositioned between the two: safe.
        let data: &[u8] = b"BT 0 0 Td (drop) Tj 0 -12 Td (kept) Tj ET";
        assert!(remove_shows(data, &BTreeSet::from([0])).is_ok());
    }

    #[test]
    fn clipping_text_and_unbalanced_streams_are_refused() {
        let data: &[u8] = b"BT 5 Tr (clip) Tj ET";
        assert_eq!(
            remove_shows(data, &BTreeSet::from([0])),
            Err(RewriteRefusal::ClippingText)
        );
        assert_eq!(
            remove_shows(b"Q BT (a) Tj ET", &BTreeSet::from([0])),
            Err(RewriteRefusal::Unbalanced)
        );
        assert_eq!(
            remove_shows(b"BT (a) Tj", &BTreeSet::from([0])),
            Err(RewriteRefusal::Unbalanced)
        );
    }
}
