//! Footnotes: superscript markers matched to bottom-of-page blocks
//! (`docs/182` §4.2).
//!
//! A reference is a run of superscript characters (smaller and raised, see
//! `ILine::new`) whose text is digits or a note symbol, attached to the word
//! before it. A note is a block in the lower half of the page, set smaller
//! than the body, whose lines start with a marker (superscript, or a plain
//! number or symbol followed by a space or a full stop). A note is accepted
//! only when its marker matches an unused reference *above* it on the same
//! page; its block leaves the body flow, and the reference becomes a
//! `NoteReference`. A block whose first marker matches nothing stays body
//! text, so a numbered paragraph at the bottom of a page is not mistaken for
//! a note.
//!
//! Notes that continue onto the next page, and symbol markers (`*`, `†`),
//! are renumbered by the model's footnote numbering; that is reported with
//! the finding, not hidden.
//!
//! Complexity: O(characters) to find references, O(lines) to read note
//! blocks, and O(n log n) to match (references indexed by marker text).

use std::collections::BTreeMap;

use super::ILine;

/// Note text is at most this share of the body size.
pub const NOTE_SIZE_FACTOR: f64 = 0.95;
/// Notes sit below this share of the page height.
pub const NOTE_REGION: f64 = 0.5;

/// A reference: element, line, first and past-last character, and the
/// line's bottom.
type Reference = (usize, usize, usize, usize, f64);

/// One footnote.
#[derive(Clone, Debug)]
pub struct INote {
    /// The marker as printed.
    pub marker: String,
    /// The body lines, the marker removed.
    pub lines: Vec<ILine>,
}

fn is_marker_text(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= 3
        && (text.chars().all(|c| c.is_ascii_digit())
            || text
                .chars()
                .all(|c| matches!(c, '*' | '\u{2020}' | '\u{2021}' | '\u{a7}')))
}

/// The marker a line starts with and how many characters it takes (with the
/// space after it).
fn leading_marker(line: &ILine) -> Option<(String, usize)> {
    let chars = &line.chars;
    let sup: String = chars
        .iter()
        .take_while(|c| c.sup)
        .map(|c| c.text.as_str())
        .collect();
    if is_marker_text(&sup) {
        let n = chars.iter().take_while(|c| c.sup).count();
        let spaces = chars[n..]
            .iter()
            .take_while(|c| c.text.trim().is_empty())
            .count();
        return Some((sup, n + spaces));
    }
    let mut token = String::new();
    let mut n = 0;
    for c in chars {
        if c.text.trim().is_empty() || c.text == "." {
            break;
        }
        token.push_str(&c.text);
        n += 1;
    }
    if !is_marker_text(&token) {
        return None;
    }
    let mut rest = n;
    if chars.get(rest).is_some_and(|c| c.text == ".") {
        rest += 1;
    }
    let spaces = chars[rest..]
        .iter()
        .take_while(|c| c.text.trim().is_empty())
        .count();
    (spaces > 0).then_some((token, rest + spaces))
}

/// Finds and extracts the footnotes of one page. `elements` are the page's
/// text elements (their lines); matched note lines are removed from them and
/// matched references are marked with the note's index (`first_note` + i).
/// O(characters + lines log lines).
pub fn footnotes(
    elements: &mut [Vec<ILine>],
    page_height: f64,
    body_size: f64,
    first_note: usize,
    work: &mut u64,
) -> Vec<INote> {
    // References: (element, line, char start, char end, bottom).
    let mut refs: BTreeMap<String, Vec<Reference>> = BTreeMap::new();
    for (e, lines) in elements.iter().enumerate() {
        for (l, line) in lines.iter().enumerate() {
            let mut i = 0;
            while i < line.chars.len() {
                *work += 1;
                if line.chars[i].sup
                    && i > 0
                    && !line.chars[i - 1].text.trim().is_empty()
                    && !line.chars[i - 1].sup
                {
                    let start = i;
                    let mut text = String::new();
                    while i < line.chars.len() && line.chars[i].sup {
                        text.push_str(&line.chars[i].text);
                        i += 1;
                    }
                    if is_marker_text(&text) {
                        refs.entry(text)
                            .or_default()
                            .push((e, l, start, i, line.bottom));
                    }
                } else {
                    i += 1;
                }
            }
        }
    }
    let mut notes: Vec<INote> = Vec::new();
    if refs.is_empty() {
        return notes;
    }
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    let mut taken = vec![false; elements.len()];
    for e in 0..elements.len() {
        let lines = &elements[e];
        let Some(first) = lines.first() else {
            continue;
        };
        let size = super::dominant_size(lines);
        if first.top < NOTE_REGION * page_height || size > NOTE_SIZE_FACTOR * body_size {
            continue;
        }
        // Match the first marker before taking the block.
        let Some((marker, _)) = leading_marker(first) else {
            continue;
        };
        let available = |marker: &str, used: &BTreeMap<String, usize>, taken: &[bool], top: f64| {
            let n = used.get(marker).copied().unwrap_or(0);
            refs.get(marker)
                .and_then(|list| list.get(n))
                .filter(|r| r.0 != e && !taken[r.0] && r.4 <= top + 1.0)
                .copied()
        };
        if available(&marker, &used, &taken, first.top).is_none() {
            continue;
        }
        taken[e] = true;
        let taken_lines = std::mem::take(&mut elements[e]);
        let mut current: Option<(INote, Reference)> = None;
        let mut finished: Vec<(INote, Reference)> = Vec::new();
        for mut line in taken_lines {
            *work += 1;
            let starts = leading_marker(&line)
                .and_then(|(m, n)| available(&m, &used, &taken, line.top).map(|r| (m, n, r)));
            match starts {
                Some((m, n, r)) => {
                    *used.entry(m.clone()).or_default() += 1;
                    if let Some(done) = current.take() {
                        finished.push(done);
                    }
                    line.chars.drain(..n);
                    current = Some((
                        INote {
                            marker: m,
                            lines: vec![line],
                        },
                        r,
                    ));
                }
                None => match current.as_mut() {
                    Some((note, _)) => note.lines.push(line),
                    None => unreachable!("the first line was matched"),
                },
            }
        }
        if let Some(done) = current.take() {
            finished.push(done);
        }
        for (note, (re, rl, start, end, _)) in finished {
            let index = first_note + notes.len();
            for c in &mut elements[re][rl].chars[start..end] {
                c.note = Some(index);
            }
            notes.push(note);
        }
    }
    notes
}
