//! Bundled fonts shared by the shaper and the renderer.
//!
//! The layout engine registers these into an empty `fontique` collection (no
//! system fonts) for deterministic, WASM-safe layout. The renderer must extract
//! glyph outlines from the *same* bytes so shaping and rasterization agree on the
//! exact face; exposing the blobs (keyed by [`FontId`]) is the bridge the font
//! resolver (`P1C-002b`, `40-FONT-MANAGEMENT-DESIGN.md`) selects over when it maps
//! a run's declared family to a concrete face. See `fonts/README.md` for
//! provenance.
//!
//! Faces are grouped into contiguous [`FontId`] blocks, one per bundled family
//! (see [`FAMILIES`]); within a block the four faces are selected by the run's
//! bold/italic flags. Every bundled family is Apache-2.0 (license-clean, matching
//! the repository and the cargo-deny allowlist):
//!
//! - `FontId(0)..=3` — **Roboto**, the default family and the ultimate fallback.
//! - `FontId(4)..=7` — **Caladea**, a metric-compatible substitute for Cambria.
//!
//! Calibri's metric-compatible partner (Carlito) is distributed only under the
//! SIL Open Font License, which is outside the license allowlist, so it is
//! deliberately **not** bundled; Calibri resolves to a documented visual fallback
//! instead (see `resolve.rs`).

use crate::text::FontId;

/// Roboto Regular (Apache-2.0).
pub const ROBOTO_REGULAR: &[u8] = include_bytes!("../fonts/Roboto-Regular.ttf");
/// Roboto Bold.
pub const ROBOTO_BOLD: &[u8] = include_bytes!("../fonts/Roboto-Bold.ttf");
/// Roboto Italic.
pub const ROBOTO_ITALIC: &[u8] = include_bytes!("../fonts/Roboto-Italic.ttf");
/// Roboto Bold Italic.
pub const ROBOTO_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/Roboto-BoldItalic.ttf");

/// Caladea Regular (Apache-2.0) — metric-compatible with Cambria.
pub const CALADEA_REGULAR: &[u8] = include_bytes!("../fonts/Caladea-Regular.ttf");
/// Caladea Bold.
pub const CALADEA_BOLD: &[u8] = include_bytes!("../fonts/Caladea-Bold.ttf");
/// Caladea Italic.
pub const CALADEA_ITALIC: &[u8] = include_bytes!("../fonts/Caladea-Italic.ttf");
/// Caladea Bold Italic.
pub const CALADEA_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/Caladea-BoldItalic.ttf");

/// A bundled fallback family: four faces (regular, bold, italic, bold-italic)
/// addressed by a contiguous [`FontId`] block starting at `base`. The face for a
/// bold/italic combination is `base + (bold | italic << 1)`.
#[derive(Clone, Copy, Debug)]
pub struct BundledFamily {
    /// The family name as it appears in the faces' `name` table (the name the
    /// shaper registers the faces under).
    pub name: &'static str,
    /// The `FontId` of this family's regular face; the block spans `base..base+4`.
    pub base: u32,
    /// The four faces, indexed by `bold | italic << 1`.
    faces: [&'static [u8]; 4],
}

impl BundledFamily {
    /// The [`FontId`] of the face for the given bold/italic combination.
    #[must_use]
    pub const fn face_id(&self, bold: bool, italic: bool) -> FontId {
        FontId(self.base + ((bold as u32) | ((italic as u32) << 1)))
    }

    /// The bytes of the face at `offset` (`0..=3`) within this family.
    #[must_use]
    pub const fn face_bytes(&self, offset: u32) -> &'static [u8] {
        self.faces[offset as usize]
    }

    /// Whether `id` addresses a face in this family's block.
    #[must_use]
    pub const fn contains(&self, id: FontId) -> bool {
        id.0 >= self.base && id.0 < self.base + 4
    }
}

/// Roboto — the default family and ultimate fallback.
pub const ROBOTO: BundledFamily = BundledFamily {
    name: "Roboto",
    base: 0,
    faces: [
        ROBOTO_REGULAR,
        ROBOTO_BOLD,
        ROBOTO_ITALIC,
        ROBOTO_BOLD_ITALIC,
    ],
};

/// Caladea — a metric-compatible substitute for Cambria.
pub const CALADEA: BundledFamily = BundledFamily {
    name: "Caladea",
    base: 4,
    faces: [
        CALADEA_REGULAR,
        CALADEA_BOLD,
        CALADEA_ITALIC,
        CALADEA_BOLD_ITALIC,
    ],
};

/// Every bundled family, in `base`-id order — the resolver's fallback chain and
/// the shaper's registration order.
pub const FAMILIES: [&BundledFamily; 2] = [&ROBOTO, &CALADEA];

/// Every bundled face, `(FontId, bytes)`, in id order — the shaper registration
/// and renderer lookup table.
pub const BUNDLED_FACES: [(FontId, &[u8]); 8] = [
    (FontId(0), ROBOTO_REGULAR),
    (FontId(1), ROBOTO_BOLD),
    (FontId(2), ROBOTO_ITALIC),
    (FontId(3), ROBOTO_BOLD_ITALIC),
    (FontId(4), CALADEA_REGULAR),
    (FontId(5), CALADEA_BOLD),
    (FontId(6), CALADEA_ITALIC),
    (FontId(7), CALADEA_BOLD_ITALIC),
];

/// The [`FontId`] of the bundled *default* (Roboto) face for the given bold/italic
/// combination. Retained as the ultimate fallback; the resolver selects other
/// families via [`BundledFamily::face_id`].
#[must_use]
pub fn face_id(bold: bool, italic: bool) -> FontId {
    ROBOTO.face_id(bold, italic)
}

/// The registered family name for a [`FontId`] (falls back to Roboto's name for
/// an unknown id).
#[must_use]
pub fn family_name(id: FontId) -> &'static str {
    FAMILIES
        .iter()
        .find(|family| family.contains(id))
        .map_or(ROBOTO.name, |family| family.name)
}

/// The font bytes for a [`FontId`] (falls back to Roboto Regular for an unknown
/// id).
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
    fn caladea_block_follows_roboto() {
        assert_eq!(CALADEA.face_id(false, false), FontId(4));
        assert_eq!(CALADEA.face_id(true, true), FontId(7));
        assert!(ROBOTO.contains(FontId(0)) && ROBOTO.contains(FontId(3)));
        assert!(!ROBOTO.contains(FontId(4)));
        assert!(CALADEA.contains(FontId(4)) && CALADEA.contains(FontId(7)));
    }

    #[test]
    fn family_name_maps_each_block() {
        assert_eq!(family_name(FontId(0)), "Roboto");
        assert_eq!(family_name(FontId(3)), "Roboto");
        assert_eq!(family_name(FontId(4)), "Caladea");
        assert_eq!(family_name(FontId(7)), "Caladea");
        // Unknown ids fall back to the default family name.
        assert_eq!(family_name(FontId(99)), "Roboto");
    }

    #[test]
    fn every_bundled_face_has_bytes() {
        for (id, bytes) in BUNDLED_FACES {
            assert!(!bytes.is_empty());
            assert_eq!(face_bytes(id), bytes);
        }
    }
}
