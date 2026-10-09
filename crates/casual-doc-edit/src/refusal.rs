//! One mechanism for a refusal the engine has already written for the reader.
//!
//! # The loss this closes
//!
//! `webapp/src/edit_errors.mjs` passes a refusal through VERBATIM when it is
//! marked, and replaces every other one with a single generic sentence — "that
//! edit isn't supported for this selection yet". Most engine errors deserve
//! that: `OffsetOutOfRange` is internal vocabulary. But a refusal the engine
//! computed a specific reason for — *which* constraint, *which* row, what to do
//! instead — loses that reason at the facade, and the reader is sent to inspect
//! a selection that is fine.
//!
//! # The shape
//!
//! A marked refusal is one string carrying two things:
//!
//! ```text
//! refused: <sentence for the reader>\u{1f}<stable routing code>
//! ```
//!
//! * `refused: ` is the marker the host already recognises, unchanged. It stays
//!   at the front so a host that knows only the old contract keeps working.
//! * The sentence is English the engine wrote. It is the FALLBACK, not the
//!   product: a host that cannot route the code still shows a specific reason
//!   rather than a generic one.
//! * The code is a stable, dotted, machine key (`table.unmerge-not-merged`) that
//!   a host routes through its own catalogue — `t(code)` — so a non-English
//!   reader gets the specific reason in their own language. A code never changes
//!   once published; the sentence may be reworded freely.
//!
//! # Why the code travels inside the message and not beside it
//!
//! The one boundary that turns an engine refusal into a thrown JS value is
//! `casual-doc-wasm`'s `to_js` (ADR-030 I1: one choke point), and it splits the
//! string back into a JS `Error` whose `message` is `refused: <sentence>` and
//! whose `code` is the key. Every internal error channel between here and there
//! is a plain `String` — 114 `Result<_, String>` signatures — so a second field
//! would mean either rewriting all of them or inventing a side channel, and a
//! side channel is the second mechanism this module exists to avoid. The
//! separator is `U+001F INFORMATION SEPARATOR ONE`, whose entire purpose is
//! marking a machine field inside a text record.
//!
//! This is also the placeholder `to_js` names: when `SdkError` finally crosses
//! the boundary with `code`/`severity` (doc 57 §5.5), this key is what its
//! reader-facing code field carries. `casual_doc_sdk::ErrorCode`'s twelve values
//! class a failure for a PROGRAM; these key a sentence for a PERSON, and a
//! taxonomy that answers both answers neither well.
//!
//! Complexity: every function here is O(message length) and allocates at most
//! once. Nothing walks a document.

/// The host's marker for "already explained, pass it through verbatim"
/// (`webapp/src/edit_errors.mjs`).
pub const MARKER: &str = "refused: ";

/// Separates the reader's sentence from the stable routing code.
///
/// `U+001F`, and never valid inside either half. A refusal that somehow reached
/// a reader unsplit would render it as nothing at all, which is why the
/// separator is a control character rather than a bracket or a colon that a
/// sentence could legitimately contain.
pub const CODE_SEPARATOR: char = '\u{1f}';

/// Builds a marked refusal from a literal code and a literal sentence, at
/// compile time.
///
/// For the `const fn reason()` families ([`crate::breaks::BreakRefusal`],
/// [`crate::FieldRefusal`]) that must return `&'static str`; [`marked`] is the
/// same contract for a sentence assembled at runtime.
///
/// ```
/// assert_eq!(
///     casual_doc_edit::refused!("table.unmerge-not-merged", "This cell isn't merged."),
///     "refused: This cell isn't merged.\u{1f}table.unmerge-not-merged"
/// );
/// ```
#[macro_export]
macro_rules! refused {
    ($code:literal, $sentence:literal) => {
        concat!("refused: ", $sentence, "\u{1f}", $code)
    };
}

/// [`refused!`] for a sentence that is only known at runtime.
///
/// Complexity: O(code + sentence), one allocation.
#[must_use]
pub fn marked(code: &str, sentence: &str) -> String {
    let mut out = String::with_capacity(MARKER.len() + sentence.len() + 1 + code.len());
    out.push_str(MARKER);
    out.push_str(sentence);
    out.push(CODE_SEPARATOR);
    out.push_str(code);
    out
}

/// Splits a refusal into the text a reader sees and the code a host routes.
///
/// The text keeps its `refused: ` marker, because that marker is what tells the
/// host the text is already a sentence. An unmarked or uncoded message comes
/// back unchanged with `None`, so this is safe to call on every error at the
/// boundary — which is exactly how `to_js` uses it.
///
/// Complexity: O(message length); no allocation.
#[must_use]
pub fn split(message: &str) -> (&str, Option<&str>) {
    match message.split_once(CODE_SEPARATOR) {
        Some((text, code)) if message.starts_with(MARKER) && !code.is_empty() => (text, Some(code)),
        _ => (message, None),
    }
}

#[cfg(test)]
mod tests {
    use super::{CODE_SEPARATOR, MARKER, marked, split};

    #[test]
    fn a_marked_refusal_splits_into_the_sentence_and_the_code() {
        let message = marked(
            "table.sort-merged",
            "Sorting needs a table with no merged cells.",
        );
        let (text, code) = split(&message);
        assert_eq!(text, "refused: Sorting needs a table with no merged cells.");
        assert_eq!(code, Some("table.sort-merged"));
        assert!(
            !text.contains(CODE_SEPARATOR),
            "the reader's half must carry no machine field: {text:?}"
        );
        assert!(
            text.starts_with(MARKER),
            "and must keep the marker the host recognises: {text:?}"
        );
    }

    #[test]
    fn an_unmarked_or_uncoded_message_is_returned_whole() {
        for message in [
            "OffsetOutOfRange",
            "refused: an older refusal that carries no code yet",
            "not a refusal at all\u{1f}but it has a separator",
        ] {
            assert_eq!(
                split(message),
                (message, None),
                "only a MARKED message may be split, or the boundary would invent \
                 a code out of an internal error: {message:?}"
            );
        }
    }

    /// The marker may only be produced by this module, so every refusal the
    /// engine explains carries a code a host can translate.
    ///
    /// A bare `"refused: …"` literal passes the host's marker test and reaches
    /// the reader in English with no way to route it — which is how a second,
    /// codeless family grows back beside the first.
    /// Everything in `raw` above its test module.
    ///
    /// `include_str!` hands back the bytes as they sit on disk and this repository's
    /// `.gitattributes` pins **only the preset shape table** to LF, so a Windows checkout of a
    /// source file is CRLF and any pattern carrying `\n` matches nothing there. The fallback then
    /// hands the scan the **whole file**, test bodies included, and the guard charges a module for
    /// its own fixtures — on one platform and not the others.
    /// It reddened `platform (Windows-x64)` in the sibling object-refusal guard.
    ///
    /// The cut is therefore the attribute **alone**, which carries no line break and so cannot
    /// have this problem by construction. That is the technique `main` chose for this guard while
    /// this branch was open; it is kept rather than replaced with a normalise-then-split, because
    /// two techniques for one rule diverge and the newline-free one needs no normalisation to
    /// reason about.
    ///
    /// What this branch adds is the **arming** the technique had no way to prove: the caller
    /// asserts the cut took effect, and the planted case below drives both line endings on every
    /// platform. Shared with that planted case on purpose — a cut the real scan performs and the
    /// planted case re-performs separately is a cut the planted case cannot test.
    fn production_half(raw: &str) -> &str {
        raw.find("#[cfg(test)]").map_or(raw, |at| &raw[..at])
    }

    #[test]
    fn no_module_of_this_crate_writes_the_marker_by_hand() {
        for (name, source) in [
            ("lib.rs", include_str!("lib.rs")),
            ("breaks.rs", include_str!("breaks.rs")),
            ("clone.rs", include_str!("clone.rs")),
            ("containers.rs", include_str!("containers.rs")),
            ("references.rs", include_str!("references.rs")),
        ] {
            let production = production_half(source);
            assert!(
                !production.contains("\"refused: "),
                "{name} writes the refusal marker as a literal; use \
                 `refused!(\"code\", \"Sentence.\")` so a host can translate it"
            );
            // And the cut has to be TAKING EFFECT, or the scan is reading the test code it was
            // written to exclude and nobody finds out until a fixture happens to trip it.
            assert!(
                production.len() < source.len(),
                "{name}'s test module was not found, so this scan read the file whole"
            );
        }

        // The CRLF case itself, driven on EVERY platform rather than only on Windows. Both
        // halves: the literal in the production half must be visible, and the one in the test
        // module must not be — which is the pair that fails when the cut silently misses.
        let planted = "fn a() {\n    let _ = \"refused: by hand\";\n}\n\
                       #[cfg(test)]\nmod tests {\n    let _ = \"refused: in a fixture\";\n}\n";
        for (what, text) in [
            ("LF", planted.to_owned()),
            ("CRLF", planted.replace('\n', "\r\n")),
        ] {
            let production = production_half(text.as_str());
            assert!(
                production.contains("\"refused: by hand\""),
                "with {what} line endings the scan cannot see the literal it forbids"
            );
            assert!(
                !production.contains("\"refused: in a fixture\""),
                "with {what} line endings the cut did not take effect, so the scan is charging \
                 a module for its own test fixtures"
            );
        }
    }

    #[test]
    fn the_macro_and_the_function_agree() {
        assert_eq!(
            crate::refused!(
                "list.continue-already",
                "This item already continues a list."
            ),
            marked(
                "list.continue-already",
                "This item already continues a list."
            )
        );
    }
}
