//! Per-codepoint script classification for OOXML multi-script font resolution.
//!
//! A single `w:r` run can mix scripts, and OOXML resolves the face for each code
//! point from a *different* `w:rFonts` slot depending on the code point's script
//! (ECMA-376 §17.3.2.26, "Run Fonts"):
//!
//! - East-Asian code points (CJK/Kana/Hangul) resolve against `w:eastAsia`;
//! - complex-script code points (Arabic, Hebrew, Thai, the Indic scripts, …)
//!   resolve against `w:cs` — and also pick up the complex-script bold/italic/size
//!   (`w:bCs`/`w:iCs`/`w:szCs`);
//! - everything else (Latin, digits, common punctuation) resolves against
//!   `w:ascii` / `w:hAnsi`.
//!
//! This module is the one place those Unicode ranges live. [`crate::shape`]'s
//! CJK-metric normalization already needed the East-Asian test, so its
//! `is_cjk_scalar` delegates to [`is_east_asian`] here rather than duplicating the
//! range table.
//!
//! Scope: this classifies each code point's *slot*. It is not a full Unicode
//! script-itemization pass, and it does not implement per-run bidi reordering —
//! that stays with the shaper's Unicode-bidi analysis (see `docs/55` §7).

use casual_doc_model::v1::RunFontHint;

/// Which `w:rFonts` slot a code point resolves its face (and, for complex script,
/// its bold/italic/size) against.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptSlot {
    /// `w:ascii` / `w:hAnsi` — Latin and everything not East-Asian or complex.
    Default,
    /// `w:eastAsia` — CJK, Kana, Hangul.
    EastAsia,
    /// `w:cs` — Arabic, Hebrew, Thai, Indic, and the other complex scripts.
    ComplexScript,
}

/// Whether a scalar belongs to an East-Asian (CJK/Japanese/Korean) block. These
/// blocks are the ones the bundled Latin faces never cover and that Word resolves
/// against the `w:eastAsia` font slot.
#[must_use]
pub fn is_east_asian(ch: char) -> bool {
    matches!(u32::from(ch),
        0x1100..=0x11FF   // Hangul Jamo
        | 0x2E80..=0x2EFF // CJK Radicals Supplement
        | 0x3000..=0x303F // CJK Symbols and Punctuation
        | 0x3040..=0x30FF // Hiragana + Katakana
        | 0x3100..=0x312F // Bopomofo
        | 0x3130..=0x318F // Hangul Compatibility Jamo
        | 0x3190..=0x319F // Kanbun
        | 0x31A0..=0x31BF // Bopomofo Extended
        | 0x31F0..=0x31FF // Katakana Phonetic Extensions
        | 0x3200..=0x33FF // Enclosed CJK Letters/Months + CJK Compatibility
        | 0x3400..=0x4DBF // CJK Unified Ideographs Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xA960..=0xA97F // Hangul Jamo Extended-A
        | 0xAC00..=0xD7FF // Hangul Syllables + Jamo Extended-B
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0xFE30..=0xFE4F // CJK Compatibility Forms
        | 0xFF00..=0xFFEF // Halfwidth and Fullwidth Forms
        | 0x20000..=0x3FFFF // CJK Unified Ideographs Extensions B–G + Supplement
    )
}

/// Whether a scalar belongs to a complex script — the scripts Word resolves
/// against the `w:cs` (complex-script) font slot and shapes with the
/// complex-script bold/italic/size. Covers the RTL scripts (Hebrew, Arabic and
/// its supplements/presentation forms, Syriac, Thaana, NKo, Samaritan) and the
/// South/South-East Asian complex scripts (the Indic block, Thai, Lao, Tibetan,
/// Myanmar, Khmer). These ranges are disjoint from [`is_east_asian`]'s.
#[must_use]
pub fn is_complex_script(ch: char) -> bool {
    matches!(u32::from(ch),
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0700..=0x074F // Syriac
        | 0x0750..=0x077F // Arabic Supplement
        | 0x0780..=0x07BF // Thaana
        | 0x07C0..=0x07FF // NKo
        | 0x0800..=0x083F // Samaritan
        | 0x0840..=0x085F // Mandaic
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0x0900..=0x0DFF // Devanagari … Sinhala (Indic)
        | 0x0E00..=0x0E7F // Thai
        | 0x0E80..=0x0EFF // Lao
        | 0x0F00..=0x0FFF // Tibetan
        | 0x1000..=0x109F // Myanmar
        | 0x1780..=0x17FF // Khmer
        | 0xFB1D..=0xFB4F // Hebrew Presentation Forms
        | 0xFB50..=0xFDFF // Arabic Presentation Forms-A
        | 0xFE70..=0xFEFF // Arabic Presentation Forms-B
    )
}

/// The slot a single code point resolves against, given the run's `w:rFonts@hint`
/// (which disambiguates the *neutral* code points — spaces, digits, common
/// punctuation — that carry no script of their own).
///
/// Strongly-scripted code points ignore the hint: an ideograph is always
/// East-Asian, an Arabic letter always complex, a Latin letter always default. A
/// neutral code point follows the hint (`eastAsia`→East-Asian, `cs`→complex),
/// defaulting to the ascii slot when the run declares no hint — which keeps a
/// CJK run's interior punctuation in its East-Asian face when Word tagged the run
/// `w:hint="eastAsia"`, and keeps ordinary Latin punctuation on the ascii face
/// otherwise.
#[must_use]
pub fn slot_for(ch: char, hint: Option<RunFontHint>) -> ScriptSlot {
    if is_east_asian(ch) {
        ScriptSlot::EastAsia
    } else if is_complex_script(ch) {
        ScriptSlot::ComplexScript
    } else if ch.is_alphabetic() {
        // A strong Latin/Greek/Cyrillic/… letter: always the default slot.
        ScriptSlot::Default
    } else {
        // A neutral (whitespace/digit/punctuation/symbol): follow the hint.
        hint_slot(hint)
    }
}

/// The slot a `w:rFonts@hint` selects for neutral code points.
fn hint_slot(hint: Option<RunFontHint>) -> ScriptSlot {
    match hint {
        Some(RunFontHint::EastAsia) => ScriptSlot::EastAsia,
        Some(RunFontHint::Cs) => ScriptSlot::ComplexScript,
        Some(RunFontHint::Default) | None => ScriptSlot::Default,
    }
}

/// Partitions `text` into maximal contiguous spans that each resolve against a
/// single [`ScriptSlot`], in document order. Each returned span is a byte-slice of
/// `text` paired with its slot; concatenating the spans reproduces `text` exactly.
/// A run with no East-Asian or complex code points (and no hint that moves its
/// neutrals) yields exactly one `Default` span — the common Latin case, so callers
/// keep their existing single-run fast path unchanged.
#[must_use]
pub fn partition_by_slot(text: &str, hint: Option<RunFontHint>) -> Vec<(&str, ScriptSlot)> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut current: Option<ScriptSlot> = None;
    for (i, ch) in text.char_indices() {
        let slot = slot_for(ch, hint);
        match current {
            Some(c) if c == slot => {}
            Some(c) => {
                spans.push((&text[start..i], c));
                start = i;
                current = Some(slot);
            }
            None => current = Some(slot),
        }
    }
    if let Some(c) = current {
        spans.push((&text[start..], c));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn east_asian_and_complex_ranges_are_disjoint_and_classified() {
        assert!(is_east_asian('中'));
        assert!(is_east_asian('あ'));
        assert!(is_east_asian('한'));
        assert!(!is_east_asian('A'));
        assert!(!is_east_asian('א'));

        assert!(is_complex_script('א')); // Hebrew alef
        assert!(is_complex_script('ا')); // Arabic alef
        assert!(is_complex_script('ก')); // Thai ko kai
        assert!(is_complex_script('अ')); // Devanagari a
        assert!(!is_complex_script('A'));
        assert!(!is_complex_script('中'));

        // No scalar is both.
        for cp in 0x0000u32..=0x2FFFF {
            if let Some(ch) = char::from_u32(cp) {
                assert!(
                    !(is_east_asian(ch) && is_complex_script(ch)),
                    "{cp:#x} classified as both East-Asian and complex"
                );
            }
        }
    }

    #[test]
    fn strong_letters_ignore_the_hint() {
        assert_eq!(slot_for('中', None), ScriptSlot::EastAsia);
        assert_eq!(
            slot_for('中', Some(RunFontHint::Cs)),
            ScriptSlot::EastAsia,
            "an ideograph is East-Asian regardless of the hint"
        );
        assert_eq!(slot_for('א', None), ScriptSlot::ComplexScript);
        assert_eq!(
            slot_for('A', Some(RunFontHint::EastAsia)),
            ScriptSlot::Default
        );
    }

    #[test]
    fn neutrals_follow_the_hint() {
        // A space/digit carries no script; the hint decides.
        assert_eq!(slot_for(' ', None), ScriptSlot::Default);
        assert_eq!(
            slot_for(' ', Some(RunFontHint::EastAsia)),
            ScriptSlot::EastAsia
        );
        assert_eq!(
            slot_for('1', Some(RunFontHint::Cs)),
            ScriptSlot::ComplexScript
        );
    }

    #[test]
    fn latin_only_text_is_a_single_default_span() {
        let spans = partition_by_slot("Hello, world 123!", None);
        assert_eq!(spans, vec![("Hello, world 123!", ScriptSlot::Default)]);
    }

    #[test]
    fn mixed_latin_and_cjk_splits_at_the_script_boundary() {
        let spans = partition_by_slot("A中B", None);
        assert_eq!(
            spans,
            vec![
                ("A", ScriptSlot::Default),
                ("中", ScriptSlot::EastAsia),
                ("B", ScriptSlot::Default),
            ]
        );
    }

    #[test]
    fn concatenating_spans_reproduces_the_text() {
        let text = "Latin العربية 中文 ไทย end";
        let joined: String = partition_by_slot(text, None)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        assert_eq!(joined, text);
    }
}
