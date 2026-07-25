//! Bundled fonts shared by the shaper and the renderer.
//!
//! The layout engine registers these into an empty `fontique` collection (no
//! system fonts) for deterministic, WASM-safe layout. The renderer must extract
//! glyph outlines from the *same* bytes so shaping and rasterization agree on the
//! exact face; exposing the blobs (keyed by [`FontId`]) is the bridge until the
//! fuller font resolver (`P1C-002b`, `40-FONT-MANAGEMENT-DESIGN.md`) adds DOCX
//! font-name matching over a larger set. See `fonts/README.md` for provenance.
//!
//! [`FontId`]s `0..=3` are the four faces of the bundled default family (Roboto),
//! selected by the run's bold/italic flags via [`face_id`].

use crate::text::FontId;

/// Roboto Regular (Apache-2.0).
pub const ROBOTO_REGULAR: &[u8] = include_bytes!("../fonts/Roboto-Regular.ttf");
/// Roboto Bold.
pub const ROBOTO_BOLD: &[u8] = include_bytes!("../fonts/Roboto-Bold.ttf");
/// Roboto Italic.
pub const ROBOTO_ITALIC: &[u8] = include_bytes!("../fonts/Roboto-Italic.ttf");
/// Roboto Bold Italic.
pub const ROBOTO_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/Roboto-BoldItalic.ttf");

/// The four bundled faces, `(FontId, bytes)`, in id order — the registration and
/// renderer lookup table.
pub const BUNDLED_FACES: [(FontId, &[u8]); 4] = [
    (FontId(0), ROBOTO_REGULAR),
    (FontId(1), ROBOTO_BOLD),
    (FontId(2), ROBOTO_ITALIC),
    (FontId(3), ROBOTO_BOLD_ITALIC),
];

/// The [`FontId`] of the bundled face for the given bold/italic combination.
#[must_use]
pub fn face_id(bold: bool, italic: bool) -> FontId {
    FontId(u32::from(bold) | (u32::from(italic) << 1))
}

/// The font bytes for a [`FontId`] (falls back to Regular for an unknown id).
#[must_use]
pub fn face_bytes(id: FontId) -> &'static [u8] {
    BUNDLED_FACES
        .iter()
        .find(|(face, _)| *face == id)
        .map_or(ROBOTO_REGULAR, |(_, bytes)| bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_id_encodes_bold_and_italic() {
        assert_eq!(face_id(false, false), FontId(0));
        assert_eq!(face_id(true, false), FontId(1));
        assert_eq!(face_id(false, true), FontId(2));
        assert_eq!(face_id(true, true), FontId(3));
    }

    #[test]
    fn every_bundled_face_has_bytes() {
        for (id, bytes) in BUNDLED_FACES {
            assert!(!bytes.is_empty());
            assert_eq!(face_bytes(id), bytes);
        }
    }
}
