// SPDX-License-Identifier: Apache-2.0

//! `rewrite_block`'s input language (`docs/181` §4.3).
//!
//! The dialect is exactly what `read_blocks` emits for a paragraph, so an
//! agent can read a block, edit the line and send it back:
//!
//! ```text
//! {#1:2} ## The heading text
//! {#1:3} A body paragraph.
//! ```
//!
//! - an optional `{#id}` prefix, which must name the target (a line pasted
//!   from another block is refused rather than applied to this one);
//! - an optional ATX heading marker, `#` to `######` and a space;
//! - the text, on one line.
//!
//! `read_blocks` writes no inline marks (bold, italic, links), so neither does
//! this: a `*` or `_` is text. Formatting the dialect cannot express is
//! therefore never touched by a rewrite: the engine diffs the TEXT word by word
//! and lowers only the words that changed, so runs that are kept keep their
//! formatting, and a paragraph's style changes only when a heading marker asks
//! for a different level.

/// A parsed rewrite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rewrite {
    /// The heading level the line asks for, when it carries a marker.
    pub heading: Option<u8>,
    /// The paragraph's new text.
    pub text: String,
}

/// The most bytes a rewrite may carry: one paragraph, not a document.
pub const MAX_REWRITE_BYTES: usize = 64 * 1024;

/// Parses `markdown` as the new content of paragraph `target`. O(markdown).
///
/// # Errors
/// When it spans lines, names another block, or is over the bound.
pub fn parse(markdown: &str, target: &str) -> Result<Rewrite, String> {
    if markdown.len() > MAX_REWRITE_BYTES {
        return Err("A rewrite is one paragraph of at most 64 KiB.".to_owned());
    }
    let line = markdown.strip_suffix('\n').unwrap_or(markdown);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.contains('\n') || line.contains('\r') {
        return Err(
            "A rewrite is one paragraph on one line; use insert_paragraphs for new ones."
                .to_owned(),
        );
    }
    let mut rest = line;
    if let Some(after) = rest.strip_prefix("{#")
        && let Some((id, tail)) = after.split_once('}')
    {
        if id != target {
            return Err(format!(
                "The Markdown is block {id}'s, not {target}'s; rewrite one block per edit."
            ));
        }
        rest = tail.strip_prefix(' ').unwrap_or(tail);
    }
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    let heading = if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        rest = &rest[hashes + 1..];
        u8::try_from(hashes).ok()
    } else {
        None
    };
    Ok(Rewrite {
        heading,
        text: rest.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Rewrite, parse};

    #[test]
    fn the_read_blocks_line_parses_back() {
        assert_eq!(
            parse("{#1:2} ## Terms", "1:2"),
            Ok(Rewrite {
                heading: Some(2),
                text: "Terms".to_owned()
            })
        );
        assert_eq!(
            parse("Plain *not italic* text", "1:2"),
            Ok(Rewrite {
                heading: None,
                text: "Plain *not italic* text".to_owned()
            })
        );
        assert_eq!(parse("#hashtag", "1:2").map(|r| r.heading), Ok(None));
        assert!(parse("{#1:3} Other", "1:2").is_err());
        assert!(parse("one\ntwo", "1:2").is_err());
    }
}
