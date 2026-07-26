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
//! bold/italic flags. Every bundled family is license-clean for redistribution
//! with the source:
//!
//! - `FontId(0)..=3` — **Roboto** (Apache-2.0), the default family and the
//!   ultimate fallback.
//! - `FontId(4)..=7` — **Caladea** (Apache-2.0), a metric-compatible substitute
//!   for Cambria.
//! - `FontId(8)..=11` — **Carlito** (SIL OFL-1.1), a metric-compatible substitute
//!   for Calibri.
//! - `FontId(12)..=15` — **Liberation Sans** (SIL OFL-1.1), a metric-compatible
//!   substitute for Arial/Helvetica.
//! - `FontId(16)..=19` — **Liberation Serif** (SIL OFL-1.1), a metric-compatible
//!   substitute for Times New Roman.
//! - `FontId(20)..=23` — **Liberation Mono** (SIL OFL-1.1), a metric-compatible
//!   substitute for Courier New.
//!
//! Carlito and the Liberation families ship under the SIL Open Font License 1.1:
//! a permissive license that governs only the font file (not this Apache-2.0
//! code) and carries no copyleft effect. Because the faces are embedded as
//! `include_bytes!` asset bytes rather than as a crate dependency, `cargo-deny`
//! does not scan them; the OFL text is shipped alongside the fonts in
//! `fonts/LICENSES/` (see `fonts/README.md`). The Liberation set is
//! LibreOffice's own metric-compatible substitute family, so substituting a
//! missing Arial/Times/Courier request to it reproduces LibreOffice's line
//! breaking and pagination (`font_substitution`).

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

/// Carlito Regular (SIL OFL-1.1) — metric-compatible with Calibri.
pub const CARLITO_REGULAR: &[u8] = include_bytes!("../fonts/Carlito-Regular.ttf");
/// Carlito Bold.
pub const CARLITO_BOLD: &[u8] = include_bytes!("../fonts/Carlito-Bold.ttf");
/// Carlito Italic.
pub const CARLITO_ITALIC: &[u8] = include_bytes!("../fonts/Carlito-Italic.ttf");
/// Carlito Bold Italic.
pub const CARLITO_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/Carlito-BoldItalic.ttf");

/// Liberation Sans Regular (SIL OFL-1.1) — metric-compatible with Arial/Helvetica.
pub const LIBERATION_SANS_REGULAR: &[u8] =
    include_bytes!("../fonts/liberation/LiberationSans-Regular.ttf");
/// Liberation Sans Bold.
pub const LIBERATION_SANS_BOLD: &[u8] =
    include_bytes!("../fonts/liberation/LiberationSans-Bold.ttf");
/// Liberation Sans Italic.
pub const LIBERATION_SANS_ITALIC: &[u8] =
    include_bytes!("../fonts/liberation/LiberationSans-Italic.ttf");
/// Liberation Sans Bold Italic.
pub const LIBERATION_SANS_BOLD_ITALIC: &[u8] =
    include_bytes!("../fonts/liberation/LiberationSans-BoldItalic.ttf");

/// Liberation Serif Regular (SIL OFL-1.1) — metric-compatible with Times New Roman.
pub const LIBERATION_SERIF_REGULAR: &[u8] =
    include_bytes!("../fonts/liberation/LiberationSerif-Regular.ttf");
/// Liberation Serif Bold.
pub const LIBERATION_SERIF_BOLD: &[u8] =
    include_bytes!("../fonts/liberation/LiberationSerif-Bold.ttf");
/// Liberation Serif Italic.
pub const LIBERATION_SERIF_ITALIC: &[u8] =
    include_bytes!("../fonts/liberation/LiberationSerif-Italic.ttf");
/// Liberation Serif Bold Italic.
pub const LIBERATION_SERIF_BOLD_ITALIC: &[u8] =
    include_bytes!("../fonts/liberation/LiberationSerif-BoldItalic.ttf");

/// Liberation Mono Regular (SIL OFL-1.1) — metric-compatible with Courier New.
pub const LIBERATION_MONO_REGULAR: &[u8] =
    include_bytes!("../fonts/liberation/LiberationMono-Regular.ttf");
/// Liberation Mono Bold.
pub const LIBERATION_MONO_BOLD: &[u8] =
    include_bytes!("../fonts/liberation/LiberationMono-Bold.ttf");
/// Liberation Mono Italic.
pub const LIBERATION_MONO_ITALIC: &[u8] =
    include_bytes!("../fonts/liberation/LiberationMono-Italic.ttf");
/// Liberation Mono Bold Italic.
pub const LIBERATION_MONO_BOLD_ITALIC: &[u8] =
    include_bytes!("../fonts/liberation/LiberationMono-BoldItalic.ttf");

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

/// Carlito — a metric-compatible substitute for Calibri.
pub const CARLITO: BundledFamily = BundledFamily {
    name: "Carlito",
    base: 8,
    faces: [
        CARLITO_REGULAR,
        CARLITO_BOLD,
        CARLITO_ITALIC,
        CARLITO_BOLD_ITALIC,
    ],
};

/// Liberation Sans — a metric-compatible substitute for Arial/Helvetica
/// (LibreOffice's own Arial substitute, so line breaking matches it).
pub const LIBERATION_SANS: BundledFamily = BundledFamily {
    name: "Liberation Sans",
    base: 12,
    faces: [
        LIBERATION_SANS_REGULAR,
        LIBERATION_SANS_BOLD,
        LIBERATION_SANS_ITALIC,
        LIBERATION_SANS_BOLD_ITALIC,
    ],
};

/// Liberation Serif — a metric-compatible substitute for Times New Roman
/// (LibreOffice's own Times substitute).
pub const LIBERATION_SERIF: BundledFamily = BundledFamily {
    name: "Liberation Serif",
    base: 16,
    faces: [
        LIBERATION_SERIF_REGULAR,
        LIBERATION_SERIF_BOLD,
        LIBERATION_SERIF_ITALIC,
        LIBERATION_SERIF_BOLD_ITALIC,
    ],
};

/// Liberation Mono — a metric-compatible substitute for Courier New
/// (LibreOffice's own Courier substitute).
pub const LIBERATION_MONO: BundledFamily = BundledFamily {
    name: "Liberation Mono",
    base: 20,
    faces: [
        LIBERATION_MONO_REGULAR,
        LIBERATION_MONO_BOLD,
        LIBERATION_MONO_ITALIC,
        LIBERATION_MONO_BOLD_ITALIC,
    ],
};

/// Every bundled family, in `base`-id order — the resolver's fallback chain and
/// the shaper's registration order.
pub const FAMILIES: [&BundledFamily; 6] = [
    &ROBOTO,
    &CALADEA,
    &CARLITO,
    &LIBERATION_SANS,
    &LIBERATION_SERIF,
    &LIBERATION_MONO,
];

/// Every bundled face, `(FontId, bytes)`, in id order — the shaper registration
/// and renderer lookup table.
pub const BUNDLED_FACES: [(FontId, &[u8]); 24] = [
    (FontId(0), ROBOTO_REGULAR),
    (FontId(1), ROBOTO_BOLD),
    (FontId(2), ROBOTO_ITALIC),
    (FontId(3), ROBOTO_BOLD_ITALIC),
    (FontId(4), CALADEA_REGULAR),
    (FontId(5), CALADEA_BOLD),
    (FontId(6), CALADEA_ITALIC),
    (FontId(7), CALADEA_BOLD_ITALIC),
    (FontId(8), CARLITO_REGULAR),
    (FontId(9), CARLITO_BOLD),
    (FontId(10), CARLITO_ITALIC),
    (FontId(11), CARLITO_BOLD_ITALIC),
    (FontId(12), LIBERATION_SANS_REGULAR),
    (FontId(13), LIBERATION_SANS_BOLD),
    (FontId(14), LIBERATION_SANS_ITALIC),
    (FontId(15), LIBERATION_SANS_BOLD_ITALIC),
    (FontId(16), LIBERATION_SERIF_REGULAR),
    (FontId(17), LIBERATION_SERIF_BOLD),
    (FontId(18), LIBERATION_SERIF_ITALIC),
    (FontId(19), LIBERATION_SERIF_BOLD_ITALIC),
    (FontId(20), LIBERATION_MONO_REGULAR),
    (FontId(21), LIBERATION_MONO_BOLD),
    (FontId(22), LIBERATION_MONO_ITALIC),
    (FontId(23), LIBERATION_MONO_BOLD_ITALIC),
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
    fn carlito_block_follows_caladea() {
        assert_eq!(CARLITO.face_id(false, false), FontId(8));
        assert_eq!(CARLITO.face_id(true, true), FontId(11));
        assert!(!CALADEA.contains(FontId(8)));
        assert!(CARLITO.contains(FontId(8)) && CARLITO.contains(FontId(11)));
    }

    #[test]
    fn liberation_blocks_follow_carlito() {
        assert_eq!(LIBERATION_SANS.face_id(false, false), FontId(12));
        assert_eq!(LIBERATION_SANS.face_id(true, true), FontId(15));
        assert_eq!(LIBERATION_SERIF.face_id(false, false), FontId(16));
        assert_eq!(LIBERATION_SERIF.face_id(true, true), FontId(19));
        assert_eq!(LIBERATION_MONO.face_id(false, false), FontId(20));
        assert_eq!(LIBERATION_MONO.face_id(true, true), FontId(23));
        assert!(!CARLITO.contains(FontId(12)));
        assert!(LIBERATION_SANS.contains(FontId(12)) && LIBERATION_SANS.contains(FontId(15)));
        assert!(LIBERATION_SERIF.contains(FontId(16)) && LIBERATION_SERIF.contains(FontId(19)));
        assert!(LIBERATION_MONO.contains(FontId(20)) && LIBERATION_MONO.contains(FontId(23)));
    }

    #[test]
    fn family_name_maps_each_block() {
        assert_eq!(family_name(FontId(0)), "Roboto");
        assert_eq!(family_name(FontId(3)), "Roboto");
        assert_eq!(family_name(FontId(4)), "Caladea");
        assert_eq!(family_name(FontId(7)), "Caladea");
        assert_eq!(family_name(FontId(8)), "Carlito");
        assert_eq!(family_name(FontId(11)), "Carlito");
        assert_eq!(family_name(FontId(12)), "Liberation Sans");
        assert_eq!(family_name(FontId(15)), "Liberation Sans");
        assert_eq!(family_name(FontId(16)), "Liberation Serif");
        assert_eq!(family_name(FontId(19)), "Liberation Serif");
        assert_eq!(family_name(FontId(20)), "Liberation Mono");
        assert_eq!(family_name(FontId(23)), "Liberation Mono");
        // Unknown ids fall back to the default family name.
        assert_eq!(family_name(FontId(99)), "Roboto");
    }

    /// Every bundled Liberation face is valid TrueType the shaper can register
    /// (guards against a truncated or placeholder asset landing in the tree).
    #[test]
    fn liberation_faces_are_valid_truetype() {
        for family in [&LIBERATION_SANS, &LIBERATION_SERIF, &LIBERATION_MONO] {
            for offset in 0..4u32 {
                let bytes = family.face_bytes(offset);
                assert!(bytes.len() > 10_000, "{} face looks truncated", family.name);
                // TrueType sfnt version 0x00010000.
                assert_eq!(&bytes[0..4], &[0x00, 0x01, 0x00, 0x00]);
            }
        }
    }

    #[test]
    fn every_bundled_face_has_bytes() {
        for (id, bytes) in BUNDLED_FACES {
            assert!(!bytes.is_empty());
            assert_eq!(face_bytes(id), bytes);
        }
    }
}
