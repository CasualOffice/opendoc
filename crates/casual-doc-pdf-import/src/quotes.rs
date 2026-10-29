//! Typographic quotes a wrong `ToUnicode` flattened, restored from the
//! glyph's own name (`docs/182` §18.8).
//!
//! A producer that writes its own `ToUnicode` (TeX's older drivers, several
//! report generators) often maps the curly quote glyphs `quoteright` and
//! `quotedblleft` to the typewriter `'` and `"`, or the left quote to the
//! grave accent. hayro follows the `ToUnicode` (it is the specification's
//! first source), so the editor would show straight quotes where the page
//! shows curly ones. The glyph name in the embedded program is the second
//! source of truth (the Adobe Glyph List's): where the program names the
//! glyph a typographic quote and the mapping says a straight one, the name
//! wins, and the change is reported (`pdf.import.quote_from_glyph_name`).
//!
//! Only the quote family is corrected, and only toward the glyph's name, so a
//! straight quote the font really draws (a glyph named `quotesingle`) is
//! never curled. Fonts without a readable sfnt or bare-CFF program (Type1
//! programs, standard-14 faces) are left as hayro decoded them: their names
//! already came through the encoding.
//!
//! Complexity: one pass over a font's glyph names the first time one of its
//! glyphs maps to a straight quote (cached per font), then O(glyphs) per run.

use std::collections::BTreeMap;

use casual_doc_pdf_read::{DocumentFonts, FontProgramKind, PageEvidence, PaintEvidence};
use skrifa::MetadataProvider;
use skrifa::raw::ps::cff::CffFontRef;
use skrifa::raw::types::GlyphId;

/// The character a glyph name stands for, for the quote family only.
#[must_use]
pub fn quote_of_name(name: &str) -> Option<char> {
    Some(match name {
        "quoteleft" | "uni2018" => '\u{2018}',
        "quoteright" | "uni2019" => '\u{2019}',
        "quotedblleft" | "uni201C" => '\u{201C}',
        "quotedblright" | "uni201D" => '\u{201D}',
        "quotesinglbase" | "uni201A" => '\u{201A}',
        "quotedblbase" | "uni201E" => '\u{201E}',
        _ => return None,
    })
}

/// Whether `text` is a straight stand-in a curly quote may have been mapped
/// to.
fn straight(text: &str) -> bool {
    matches!(text, "'" | "\"" | "`" | "\u{00B4}" | ",")
}

/// The quote glyphs of one program: glyph id to the quote it is named.
fn quote_glyphs(kind: FontProgramKind, bytes: &[u8]) -> BTreeMap<u32, char> {
    let mut out = BTreeMap::new();
    match kind {
        FontProgramKind::TrueType | FontProgramKind::OpenTypeCff => {
            if let Ok(font) = skrifa::FontRef::new(bytes) {
                for (gid, name) in font.glyph_names().iter() {
                    if let Some(q) = quote_of_name(name.as_str()) {
                        out.insert(gid.to_u32(), q);
                    }
                }
            }
        }
        FontProgramKind::BareCff => {
            if let Ok(font) = CffFontRef::new_cff(bytes, 0, None)
                && !font.is_cid()
                && let Some(charset) = font.charset()
            {
                for gid in 0..font.num_glyphs() {
                    let gid = GlyphId::new(gid);
                    if let Ok(sid) = charset.string_id(gid)
                        && let Some(name) = font.string(sid)
                        && let Ok(name) = std::str::from_utf8(name)
                        && let Some(q) = quote_of_name(name)
                    {
                        out.insert(gid.to_u32(), q);
                    }
                }
            }
        }
        FontProgramKind::Other => {}
    }
    out
}

/// Per-font quote glyphs, read on first need.
#[derive(Debug, Default)]
pub(crate) struct QuoteNames {
    fonts: BTreeMap<usize, BTreeMap<u32, char>>,
}

impl QuoteNames {
    /// Corrects the page's straight quotes whose glyph is named a curly one.
    /// Returns how many glyphs changed. O(glyphs) plus a one-off name pass
    /// per font.
    pub(crate) fn correct(&mut self, page: &mut PageEvidence, fonts: &DocumentFonts) -> u32 {
        let mut changed = 0;
        for item in &mut page.items {
            let PaintEvidence::Glyphs(run) = item else {
                continue;
            };
            if !run
                .glyphs
                .iter()
                .any(|g| g.text.as_deref().is_some_and(straight))
            {
                continue;
            }
            let names = self.fonts.entry(run.font).or_insert_with(|| {
                fonts
                    .fonts
                    .get(run.font)
                    .and_then(|f| f.program.as_ref())
                    .map(|(kind, bytes)| quote_glyphs(*kind, bytes))
                    .unwrap_or_default()
            });
            if names.is_empty() {
                continue;
            }
            for glyph in &mut run.glyphs {
                if let (Some(text), Some(id)) = (glyph.text.as_deref(), glyph.glyph)
                    && straight(text)
                    && let Some(q) = names.get(&id)
                {
                    glyph.text = Some(q.to_string());
                    changed += 1;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_quote_family_is_named() {
        assert_eq!(quote_of_name("quoteright"), Some('\u{2019}'));
        assert_eq!(quote_of_name("quotedblleft"), Some('\u{201C}'));
        assert_eq!(quote_of_name("uni201E"), Some('\u{201E}'));
        assert_eq!(quote_of_name("quotesingle"), None, "a straight glyph");
        assert_eq!(quote_of_name("a"), None);
        assert!(straight("'") && straight("\"") && straight("`"));
        assert!(!straight("\u{2019}"));
    }
}
