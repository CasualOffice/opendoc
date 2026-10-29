//! The text a glyph means, made fit for editing (`docs/182` §5.1, §18.8).
//!
//! hayro answers a glyph's Unicode through the font's `ToUnicode`, then its
//! glyph names through the Adobe Glyph List, then `uniXXXX` names. What comes
//! back is what the producer wrote, and producers write things a reader
//! cannot edit:
//!
//! - **Ligatures** (U+FB00–FB06): one glyph for `fi`, `ffl`, `st`. Searching
//!   or spell checking "find" fails on `ﬁnd`, so each is decomposed into its
//!   letters (the compatibility decomposition, NFKC's, for this block only).
//! - **Symbol fonts** (Symbol, Wingdings, ZapfDingbats): the glyph's code is
//!   its meaning, so `ToUnicode` maps it to the private-use mirror of the
//!   font's code page (U+F020–F0FF, Word's bullets are U+F0B7 and U+F0A7) or,
//!   with no `ToUnicode`, to the Latin letter of its code (`l` for a
//!   Wingdings bullet). Each is mapped through the font's own encoding to the
//!   real character (U+2022 BULLET, U+2192 RIGHTWARDS ARROW, U+2611 BALLOT BOX
//!   WITH CHECK). A code the table does not know has no Unicode: it takes the
//!   unmapped path (`pdf.import.native_text_invalid`, then OCR), never the
//!   private-use character, which no other face draws.
//! - **Combining marks**: a base letter and a mark drawn as two glyphs, or a
//!   spacing accent (`´`, `¨`, `ˇ`) placed over a letter by a negative kern
//!   (TeX does this), are composed into the precomposed letter where Unicode
//!   has one (canonical composition, NFC's pairs, the generated `compose_table.rs`); a mark with
//!   no precomposed letter stays a combining mark after its base.
//! - **Not text at all**: an empty mapping, U+0000, U+FFFD, C0 and C1
//!   controls, a byte-order mark. Such a glyph has no Unicode, and is
//!   reported and recovered as one (never U+FFFD in the document).
//!
//! What is left alone, deliberately: U+00A0 (a no-break space is meaningful
//! between "10" and "kg"), U+00AD (the importer decides: a soft hyphen the PDF
//! drew is a visible hyphen in fixed layout and a join in reflow), dashes and
//! quotes (a wrong `ToUnicode` quote is corrected by the importer from the
//! glyph's name, which needs the font program, `casual-doc-pdf-import`).
//!
//! Complexity: O(glyphs) per run; the composition lookup is a binary search
//! in a 900-entry table.

use crate::compose_table::COMPOSE;
use crate::evidence::GlyphEvidence;

/// Which code page a font's codes mean, decided from its name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontCodes {
    /// A text font: its Unicode is taken as given.
    Text,
    /// Adobe Symbol (Symbol, SymbolMT, StandardSymL).
    Symbol,
    /// Microsoft Wingdings.
    Wingdings,
    /// ITC Zapf Dingbats (ZapfDingbats, Dingbats).
    Dingbats,
    /// Another symbol font (Wingdings 2 and 3, Webdings, Marlett, MT Extra):
    /// its codes are known to be symbols, but not which.
    OtherSymbol,
}

impl FontCodes {
    /// The code page of the font named `name` (a `BaseFont` without its
    /// subset prefix, or a PostScript name). O(name).
    #[must_use]
    pub fn of(name: &str) -> Self {
        let lower: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if lower.starts_with("wingdings2")
            || lower.starts_with("wingdings3")
            || lower.starts_with("webdings")
            || lower.starts_with("marlett")
            || lower.starts_with("mtextra")
        {
            Self::OtherSymbol
        } else if lower.starts_with("wingdings") {
            Self::Wingdings
        } else if lower.starts_with("zapfdingbats") || lower.starts_with("dingbats") {
            Self::Dingbats
        } else if lower.starts_with("symbol") || lower.starts_with("standardsym") {
            Self::Symbol
        } else {
            Self::Text
        }
    }

    /// Whether this is a symbol font.
    #[must_use]
    pub fn symbolic(self) -> bool {
        self != Self::Text
    }
}

/// What normalising one glyph's text did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Normalized {
    /// The text, or `None` when the glyph has no usable Unicode.
    pub text: Option<String>,
    /// A ligature was decomposed into its letters.
    pub ligature: bool,
    /// A symbol font's code was mapped to its real character.
    pub symbol: bool,
}

/// The letters of a ligature in U+FB00–FB06.
#[must_use]
pub fn ligature_letters(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{FB00}' => "ff",
        '\u{FB01}' => "fi",
        '\u{FB02}' => "fl",
        '\u{FB03}' => "ffi",
        '\u{FB04}' => "ffl",
        '\u{FB05}' | '\u{FB06}' => "st",
        _ => return None,
    })
}

/// Adobe Symbol encoding, codes 0x20–0xFE (`None`: no glyph). The standard
/// encoding of the Symbol font (PDF 32000-1 Annex D.5), with Windows' euro
/// at 0xA0.
const SYMBOL: [u16; 0xDF] = {
    let mut t = [0u16; 0xDF];
    let ascii = b" !\0#\0%&\0()\0+,\0./0123456789:;<=>?";
    let mut i = 0;
    while i < ascii.len() {
        t[i] = ascii[i] as u16;
        i += 1;
    }
    // 0x20.. with the Symbol-specific code points.
    t[0x22 - 0x20] = 0x2200;
    t[0x24 - 0x20] = 0x2203;
    t[0x27 - 0x20] = 0x220B;
    t[0x2A - 0x20] = 0x2217;
    t[0x2D - 0x20] = 0x2212;
    let upper: [u16; 26] = [
        0x0391, 0x0392, 0x03A7, 0x0394, 0x0395, 0x03A6, 0x0393, 0x0397, 0x0399, 0x03D1, 0x039A,
        0x039B, 0x039C, 0x039D, 0x039F, 0x03A0, 0x0398, 0x03A1, 0x03A3, 0x03A4, 0x03A5, 0x03C2,
        0x03A9, 0x039E, 0x03A8, 0x0396,
    ];
    let lower: [u16; 26] = [
        0x03B1, 0x03B2, 0x03C7, 0x03B4, 0x03B5, 0x03C6, 0x03B3, 0x03B7, 0x03B9, 0x03D5, 0x03BA,
        0x03BB, 0x03BC, 0x03BD, 0x03BF, 0x03C0, 0x03B8, 0x03C1, 0x03C3, 0x03C4, 0x03C5, 0x03D6,
        0x03C9, 0x03BE, 0x03C8, 0x03B6,
    ];
    t[0x40 - 0x20] = 0x2245;
    let mut k = 0;
    while k < 26 {
        t[0x41 - 0x20 + k] = upper[k];
        t[0x61 - 0x20 + k] = lower[k];
        k += 1;
    }
    t[0x5B - 0x20] = 0x005B;
    t[0x5C - 0x20] = 0x2234;
    t[0x5D - 0x20] = 0x005D;
    t[0x5E - 0x20] = 0x22A5;
    t[0x5F - 0x20] = 0x005F;
    t[0x60 - 0x20] = 0x203E;
    t[0x7B - 0x20] = 0x007B;
    t[0x7C - 0x20] = 0x007C;
    t[0x7D - 0x20] = 0x007D;
    t[0x7E - 0x20] = 0x223C;
    let high: [u16; 0x5F] = [
        // 0xA0
        0x20AC, 0x03D2, 0x2032, 0x2264, 0x2044, 0x221E, 0x0192, 0x2663, 0x2666, 0x2665, 0x2660,
        0x2194, 0x2190, 0x2191, 0x2192, 0x2193, // 0xB0
        0x00B0, 0x00B1, 0x2033, 0x2265, 0x00D7, 0x221D, 0x2202, 0x2022, 0x00F7, 0x2260, 0x2261,
        0x2248, 0x2026, 0x23D0, 0x23AF, 0x21B5, // 0xC0
        0x2135, 0x2111, 0x211C, 0x2118, 0x2297, 0x2295, 0x2205, 0x2229, 0x222A, 0x2283, 0x2287,
        0x2284, 0x2282, 0x2286, 0x2208, 0x2209, // 0xD0
        0x2220, 0x2207, 0x00AE, 0x00A9, 0x2122, 0x220F, 0x221A, 0x22C5, 0x00AC, 0x2227, 0x2228,
        0x21D4, 0x21D0, 0x21D1, 0x21D2, 0x21D3, // 0xE0
        0x25CA, 0x2329, 0x00AE, 0x00A9, 0x2122, 0x2211, 0x239B, 0x239C, 0x239D, 0x23A1, 0x23A2,
        0x23A3, 0x23A7, 0x23A8, 0x23A9, 0x23AA, // 0xF0
        0x0000, 0x232A, 0x222B, 0x2320, 0x23AE, 0x2321, 0x239E, 0x239F, 0x23A0, 0x23A4, 0x23A5,
        0x23A6, 0x23AB, 0x23AC, 0x23AD,
    ];
    let mut h = 0;
    while h < high.len() {
        t[0xA0 - 0x20 + h] = high[h];
        h += 1;
    }
    t
};

/// The Wingdings codes producers use for bullets, arrows and boxes (Word's
/// bullet library and its Insert Symbol defaults). The rest of the code page
/// is pictographs with no settled Unicode mapping and takes the unmapped
/// path.
const WINGDINGS: &[(u8, char)] = &[
    (0x20, ' '),
    (0x6C, '\u{25CF}'), // l: black circle
    (0x6D, '\u{274D}'), // m: shadowed white circle
    (0x6E, '\u{25A0}'), // n: black square
    (0x6F, '\u{25A1}'), // o: white square
    (0x70, '\u{25FB}'), // p: white medium square
    (0x71, '\u{2751}'), // q: shadowed white square
    (0x72, '\u{2752}'), // r: shadowed white square, upper right
    (0x73, '\u{2B27}'), // s: black medium lozenge
    (0x74, '\u{29EB}'), // t: black lozenge
    (0x75, '\u{25C6}'), // u: black diamond
    (0x76, '\u{2756}'), // v: black diamond minus white X
    (0x77, '\u{2B25}'), // w: black medium diamond
    (0x9F, '\u{2022}'), // small bullet
    (0xA1, '\u{25CB}'), // white circle
    (0xA7, '\u{25AA}'), // §: black small square
    (0xA8, '\u{25FB}'), // white medium square
    (0xAB, '\u{2605}'), // black star
    (0xD8, '\u{27A2}'), // Ø: arrowhead bullet
    (0xDF, '\u{2190}'), // arrows
    (0xE0, '\u{2192}'),
    (0xE1, '\u{2191}'),
    (0xE2, '\u{2193}'),
    (0xE8, '\u{2794}'), // heavy wide-headed rightwards arrow
    (0xF0, '\u{21E8}'), // rightwards white arrow
    (0xFB, '\u{2717}'), // ballot X
    (0xFC, '\u{2713}'), // ü: check mark
    (0xFD, '\u{2612}'), // ballot box with X
    (0xFE, '\u{2611}'), // ballot box with check
];

/// The Zapf Dingbats codes for checks, crosses, bullets and boxes (its
/// standard encoding, PDF 32000-1 Annex D.6).
const DINGBATS: &[(u8, char)] = &[
    (0x20, ' '),
    (0x33, '\u{2713}'),
    (0x34, '\u{2714}'),
    (0x35, '\u{2715}'),
    (0x36, '\u{2716}'),
    (0x37, '\u{2717}'),
    (0x38, '\u{2718}'),
    (0x48, '\u{2605}'),
    (0x6C, '\u{25CF}'),
    (0x6D, '\u{274D}'),
    (0x6E, '\u{25A0}'),
    (0x6F, '\u{274F}'),
    (0x70, '\u{2750}'),
    (0x71, '\u{2751}'),
    (0x72, '\u{2752}'),
    (0x73, '\u{25B2}'),
    (0x74, '\u{25BC}'),
    (0x75, '\u{25C6}'),
    (0x76, '\u{2756}'),
    (0xD5, '\u{2192}'),
    (0xD6, '\u{2194}'),
    (0xD7, '\u{2195}'),
];

/// The character code `code` (one byte) means in a font of `codes`.
#[must_use]
pub fn symbol_char(codes: FontCodes, code: u8) -> Option<char> {
    let table = |list: &[(u8, char)]| {
        list.binary_search_by_key(&code, |(k, _)| *k)
            .ok()
            .map(|i| list[i].1)
    };
    match codes {
        FontCodes::Symbol => code
            .checked_sub(0x20)
            .and_then(|i| SYMBOL.get(usize::from(i)))
            .filter(|u| **u != 0)
            .and_then(|u| char::from_u32(u32::from(*u))),
        FontCodes::Wingdings => table(WINGDINGS),
        FontCodes::Dingbats => table(DINGBATS),
        FontCodes::OtherSymbol | FontCodes::Text => None,
    }
}

/// Whether `c` cannot be text: a control character, a byte-order mark, the
/// replacement character.
fn not_text(c: char) -> bool {
    c.is_control() || matches!(c, '\u{FEFF}' | '\u{FFFD}' | '\u{FFFE}' | '\u{FFFF}')
}

/// Normalises the text hayro gave one glyph of a font of `codes`.
/// O(text).
#[must_use]
pub fn normalize(text: Option<&str>, codes: FontCodes) -> Normalized {
    let mut out = Normalized::default();
    let Some(text) = text else {
        return out;
    };
    // A replacement character means the producer had no mapping either.
    if text.contains('\u{FFFD}') {
        return out;
    }
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        let code = u32::from(c);
        // The private-use mirror of a one-byte code page (U+F020–F0FF), or
        // the code itself where the font cannot mean a Latin character:
        // Wingdings and the dingbat fonts have no letters at all, and Symbol
        // has no Latin letters (its Latin-1 range overlaps real characters
        // hayro decodes from glyph names, `±` `×` `÷`, so only letters are
        // taken as codes there). A space is a space in every font.
        let byte = if (0xF020..=0xF0FF).contains(&code) {
            Some((code - 0xF000) as u8)
        } else if code == 0x20 {
            None
        } else if matches!(
            codes,
            FontCodes::Wingdings | FontCodes::Dingbats | FontCodes::OtherSymbol
        ) && code <= 0xFF
        {
            Some(code as u8)
        } else if codes == FontCodes::Symbol && c.is_ascii_alphabetic() {
            Some(code as u8)
        } else {
            None
        };
        if let Some(byte) = byte {
            let table = if codes == FontCodes::Text && code >= 0xF000 {
                // A private-use code from a font whose name says nothing:
                // Word's bullets name the font, so this is a guess not made.
                None
            } else {
                symbol_char(codes, byte)
            };
            match table {
                Some(mapped) => {
                    if mapped != c {
                        out.symbol = true;
                    }
                    result.push(mapped);
                }
                None => return Normalized::default(),
            }
            continue;
        }
        if let Some(letters) = ligature_letters(c) {
            out.ligature = true;
            result.push_str(letters);
            continue;
        }
        if not_text(c) {
            continue;
        }
        result.push(c);
    }
    if result.is_empty() {
        return Normalized::default();
    }
    out.text = Some(compose(&result));
    out
}

/// The precomposed character of `base` and the combining `mark`, if Unicode
/// has one (canonical composition). O(log table).
#[must_use]
pub fn compose_pair(base: char, mark: char) -> Option<char> {
    let (b, m) = (u32::from(base), u32::from(mark));
    let (b, m) = (u16::try_from(b).ok()?, u16::try_from(m).ok()?);
    COMPOSE
        .binary_search_by(|(x, y, _)| (*x, *y).cmp(&(b, m)))
        .ok()
        .and_then(|i| char::from_u32(u32::from(COMPOSE[i].2)))
}

/// Whether `c` is a combining mark this module composes (Combining
/// Diacritical Marks, U+0300–036F).
#[must_use]
pub fn combining(c: char) -> bool {
    ('\u{0300}'..='\u{036F}').contains(&c)
}

/// `text` with every base-plus-mark pair that has a precomposed character
/// composed, left to right (so `e` + U+0302 + U+0301 is `ế`). O(text).
#[must_use]
pub fn compose(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if combining(c)
            && let Some(last) = out.chars().next_back()
            && let Some(composed) = compose_pair(last, c)
        {
            out.pop();
            out.push(composed);
            continue;
        }
        out.push(c);
    }
    out
}

/// The combining mark a spacing accent stands for when it is drawn over a
/// letter.
#[must_use]
pub fn spacing_accent(c: char) -> Option<char> {
    Some(match c {
        '`' => '\u{0300}',
        '\u{00B4}' => '\u{0301}',
        '^' | '\u{02C6}' => '\u{0302}',
        '~' | '\u{02DC}' => '\u{0303}',
        '\u{00AF}' | '\u{02C9}' => '\u{0304}',
        '\u{02D8}' => '\u{0306}',
        '\u{02D9}' => '\u{0307}',
        '\u{00A8}' => '\u{0308}',
        '\u{02DA}' => '\u{030A}',
        '\u{02DD}' => '\u{030B}',
        '\u{02C7}' => '\u{030C}',
        '\u{00B8}' => '\u{0327}',
        '\u{02DB}' => '\u{0328}',
        _ => return None,
    })
}

/// How much of `accent`'s horizontal extent `letter`'s covers, as a share of
/// the accent's (zero-width accents count as covered when inside).
fn covered(accent: &GlyphEvidence, letter: &GlyphEvidence) -> f64 {
    let (a0, a1) = (accent.origin.x, accent.origin.x + accent.advance.max(0.0));
    let (l0, l1) = (letter.origin.x, letter.origin.x + letter.advance.max(0.0));
    if a1 - a0 < 0.01 {
        return if a0 >= l0 - 0.01 && a0 <= l1 + 0.01 {
            1.0
        } else {
            0.0
        };
    }
    (a1.min(l1) - a0.max(l0)).max(0.0) / (a1 - a0)
}

/// Composes the marks of one run in place: a glyph that is only combining
/// marks joins the glyph before it, and a spacing accent drawn at least half
/// over a neighbouring letter joins that letter, each composed where Unicode
/// has a precomposed character. Returns how many glyphs were joined.
/// O(glyphs).
pub fn compose_run(glyphs: &mut Vec<GlyphEvidence>) -> u32 {
    let mut joined = 0;
    let mut i = 0;
    while i < glyphs.len() {
        let Some(text) = glyphs[i].text.clone() else {
            i += 1;
            continue;
        };
        let mut chars = text.chars();
        let first = chars.next();
        let single = chars.next().is_none();
        // A glyph of combining marks only: onto the previous glyph.
        if i > 0
            && text.chars().all(combining)
            && let Some(previous) = glyphs[i - 1].text.as_mut()
        {
            previous.push_str(&text);
            *previous = compose(previous);
            glyphs.remove(i);
            joined += 1;
            continue;
        }
        // A spacing accent over a letter: onto that letter.
        if single && let Some(mark) = first.and_then(spacing_accent) {
            let letter = |j: usize, glyphs: &[GlyphEvidence]| -> Option<(usize, f64)> {
                let t = glyphs[j].text.as_deref()?;
                let mut cs = t.chars();
                let c = cs.next()?;
                if cs.next().is_some() || !c.is_alphabetic() {
                    return None;
                }
                let share = covered(&glyphs[i], &glyphs[j]);
                (share >= 0.5).then_some((j, share))
            };
            let before = if i > 0 { letter(i - 1, glyphs) } else { None };
            let after = if i + 1 < glyphs.len() {
                letter(i + 1, glyphs)
            } else {
                None
            };
            let target = match (before, after) {
                (Some(b), Some(a)) => Some(if a.1 > b.1 { a.0 } else { b.0 }),
                (b, a) => b.or(a).map(|t| t.0),
            };
            if let Some(j) = target {
                let base = glyphs[j]
                    .text
                    .as_deref()
                    .and_then(|t| t.chars().next())
                    .expect("a letter");
                if let Some(composed) = compose_pair(base, mark) {
                    glyphs[j].text = Some(composed.to_string());
                    glyphs.remove(i);
                    joined += 1;
                    if j > i {
                        // The letter moved down into the accent's slot.
                        i += 1;
                    }
                    continue;
                }
            }
        }
        i += 1;
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::Point;

    fn glyph(text: &str, x: f64, advance: f64) -> GlyphEvidence {
        GlyphEvidence {
            text: Some(text.to_owned()),
            origin: Point { x, y: 100.0 },
            advance,
            glyph: None,
        }
    }

    fn texts(glyphs: &[GlyphEvidence]) -> Vec<&str> {
        glyphs.iter().filter_map(|g| g.text.as_deref()).collect()
    }

    #[test]
    fn ligatures_are_decomposed_into_their_letters() {
        for (lig, letters) in [
            ("\u{FB00}", "ff"),
            ("\u{FB01}", "fi"),
            ("\u{FB02}", "fl"),
            ("\u{FB03}", "ffi"),
            ("\u{FB04}", "ffl"),
            ("\u{FB05}", "st"),
            ("\u{FB06}", "st"),
        ] {
            let n = normalize(Some(lig), FontCodes::Text);
            assert_eq!(n.text.as_deref(), Some(letters), "{lig:?}");
            assert!(n.ligature);
        }
        let plain = normalize(Some("f"), FontCodes::Text);
        assert!(!plain.ligature);
    }

    #[test]
    fn symbol_fonts_map_their_codes_to_real_characters() {
        // Word's Symbol bullet, through its private-use ToUnicode.
        let bullet = normalize(Some("\u{F0B7}"), FontCodes::of("SymbolMT"));
        assert_eq!(bullet.text.as_deref(), Some("\u{2022}"));
        assert!(bullet.symbol);
        // No ToUnicode: the Latin letter of the code.
        assert_eq!(
            normalize(Some("a"), FontCodes::Symbol).text.as_deref(),
            Some("\u{03B1}")
        );
        assert_eq!(
            normalize(Some("\u{F0AE}"), FontCodes::Symbol)
                .text
                .as_deref(),
            Some("\u{2192}")
        );
        // Symbol's Latin-1 overlap is taken as decoded: `×` is not a code.
        assert_eq!(
            normalize(Some("\u{00D7}"), FontCodes::Symbol)
                .text
                .as_deref(),
            Some("\u{00D7}")
        );
        // Wingdings: the bullet library and the boxes.
        let wingdings = FontCodes::of("ABCDEF+Wingdings-Regular");
        assert_eq!(wingdings, FontCodes::Wingdings);
        for (code, want) in [
            ("l", "\u{25CF}"),
            ("\u{F0A7}", "\u{25AA}"),
            ("\u{F0D8}", "\u{27A2}"),
            ("\u{F0FC}", "\u{2713}"),
            ("\u{F0FE}", "\u{2611}"),
            ("\u{F0E0}", "\u{2192}"),
        ] {
            assert_eq!(
                normalize(Some(code), wingdings).text.as_deref(),
                Some(want),
                "{code:?}"
            );
        }
        assert_eq!(
            normalize(Some("4"), FontCodes::of("ZapfDingbats"))
                .text
                .as_deref(),
            Some("\u{2714}")
        );
        // A code the table does not know has no Unicode: never the
        // private-use character.
        assert_eq!(normalize(Some("\u{F021}"), wingdings).text, None);
        assert_eq!(
            normalize(Some("\u{F0B7}"), FontCodes::of("Webdings")).text,
            None
        );
        // A text font keeps its text, Latin-1 included.
        assert_eq!(
            normalize(Some("\u{00B7}a"), FontCodes::of("Arial-BoldMT"))
                .text
                .as_deref(),
            Some("\u{00B7}a")
        );
    }

    #[test]
    fn what_is_not_text_has_no_unicode() {
        for bad in ["", "\u{0}", "\u{FFFD}", "a\u{FFFD}", "\u{FEFF}", "\u{7}"] {
            assert_eq!(normalize(Some(bad), FontCodes::Text).text, None, "{bad:?}");
        }
        assert_eq!(normalize(None, FontCodes::Text).text, None);
        // A byte-order mark inside a mapping is dropped, the text kept.
        assert_eq!(
            normalize(Some("\u{FEFF}A"), FontCodes::Text)
                .text
                .as_deref(),
            Some("A")
        );
        // A no-break space and a soft hyphen are the importer's to decide.
        assert_eq!(
            normalize(Some("\u{00A0}"), FontCodes::Text).text.as_deref(),
            Some("\u{00A0}")
        );
        assert_eq!(
            normalize(Some("\u{00AD}"), FontCodes::Text).text.as_deref(),
            Some("\u{00AD}")
        );
    }

    #[test]
    fn combining_marks_compose_to_nfc() {
        assert_eq!(compose("e\u{0301}"), "\u{00E9}");
        assert_eq!(compose("e\u{0302}\u{0301}"), "\u{1EBF}");
        // No precomposed letter: the mark stays after its base.
        assert_eq!(compose("q\u{0301}"), "q\u{0301}");
        // A mark in its own glyph joins the letter before it.
        let mut run = vec![glyph("n", 0.0, 5.0), glyph("\u{0303}", 5.0, 0.0)];
        assert_eq!(compose_run(&mut run), 1);
        assert_eq!(texts(&run), vec!["\u{00F1}"]);
        // TeX: the acute drawn over the e by a negative kern.
        let mut run = vec![
            glyph("\u{00B4}", 0.6, 4.0),
            glyph("e", 0.0, 5.0),
            glyph("t", 5.0, 3.0),
        ];
        assert_eq!(compose_run(&mut run), 1);
        assert_eq!(texts(&run), vec!["\u{00E9}", "t"]);
        // A spacing accent standing alone is kept.
        let mut run = vec![glyph("a", 0.0, 5.0), glyph("\u{00B4}", 8.0, 4.0)];
        assert_eq!(compose_run(&mut run), 0);
        assert_eq!(texts(&run), vec!["a", "\u{00B4}"]);
    }
}
