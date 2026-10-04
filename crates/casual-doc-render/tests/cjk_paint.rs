//! CJK puts real glyph ink on the page on the browser path — ink that is *not*
//! the notdef box's.
//!
//! `109` HF-176: more than half the text of two corpus documents rendered as
//! boxes. The coverage half of that is guarded at the shaping level
//! (`casual-doc-layout`'s `tests/cjk_coverage.rs`, which proves the resolved
//! face's own `cmap` maps each scalar). This file guards the half that the user
//! actually sees: that the rasterizer draws those glyphs.
//!
//! ## Why "it drew ink" is not the assertion
//!
//! **A tofu box has ink.** It is a real glyph with a real outline — a hollow
//! rectangle — so `dark_pixel_count(surface) > 0`, and even `> 100`, is
//! satisfied by the exact defect this row reports. A guard written that way
//! would have passed for the whole time CJK was broken.
//!
//! So the assertion here is a *comparison against the defect*: the same text,
//! at the same size, at the same origin, rendered twice — once on the bundled
//! deterministic path (which produces the notdef boxes, and is what the browser
//! has before it fetches anything) and once after a covering face is registered
//! through the host seam. The two rasters must differ, and the covered one must
//! carry ink. Nothing but real coverage satisfies that pair: a tofu render is
//! byte-identical to a tofu render.
//!
//! The per-glyph test below goes one step further and compares a single CJK
//! glyph's raster against *that same face's* `.notdef` glyph raster, so the
//! comparison does not depend on the two runs having different advances.
//!
//! The test face is the 19-codepoint, 8.6 KB subset of Noto Sans CJK SC
//! (SIL OFL 1.1) in `fixtures/fonts/` — see `tests/cjk_coverage.rs` for why it
//! is a subset and what that means it does and does not prove.

use casual_doc_layout::compose::compose_paragraph;
use casual_doc_layout::display::{DisplayList, PaintItem};
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{
    Decoration, FontId, LineConstraints, LineShaper, StyledRun, TextAlignment,
};
use casual_doc_layout::units::{Point, Twip};
use casual_doc_model::NodeId;
use casual_doc_render::{NoMediaSource, RegistryFontSource, Surface, render};

const CJK_TEST_FACE: &[u8] = include_bytes!("../../../fixtures/fonts/NotoSansCJK-TestSubset.otf");
const CJK_SCRIPTS: [&str; 4] = ["Hani", "Hira", "Kana", "Hang"];

/// One case per CJK script family, so a failure names the script.
const CASES: [(&str, &str); 4] = [
    ("Han", "中文字"),
    ("Hiragana", "あいう"),
    ("Katakana", "アイウ"),
    ("Hangul", "한글"),
];

const SIZE_PT: i32 = 32;
const DPI: f32 = 96.0;

fn styled(text: &str) -> StyledRun<'_> {
    StyledRun {
        text: text.into(),
        requested_family: None,
        requested_family_kind: None,
        font: FontId(0),
        size: Twip::from_points(SIZE_PT),
        character_scale_percent: 100,
        bold: false,
        italic: false,
        letter_spacing: Twip::ZERO,
        color: [0, 0, 0, 255],
        decoration: Decoration::default(),
        highlight: None,
        shading: None,
        baseline_shift: Twip::ZERO,
    }
}

/// Shapes `text`, composes it at a fixed origin, and rasterizes it through the
/// shaper's own registry — so the bytes rasterized are the bytes shaped from.
fn raster(shaper: &ParleyShaper, text: &str) -> Surface {
    let node = NodeId::from_parts(1, 1).unwrap();
    let layout = shaper.shape_paragraph(
        &[styled(text)],
        LineConstraints {
            max_width: Twip::from_points(600),
            alignment: TextAlignment::Start,
            ..LineConstraints::default()
        },
        ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
    );
    let list = compose_paragraph(
        &layout,
        Point::new(Twip::from_points(4), Twip::from_points(SIZE_PT)),
    );
    let fonts = RegistryFontSource::new(&shaper.registry());
    let mut surface = Surface::new(260, 80).unwrap();
    render(&list, &mut surface, DPI, &fonts, &NoMediaSource);
    surface
}

fn ink(surface: &Surface) -> usize {
    surface
        .data()
        .chunks_exact(4)
        .filter(|px| px[0] < 200 || px[1] < 200 || px[2] < 200)
        .count()
}

/// A compact, order-sensitive digest of every pixel byte (FNV-1a, 64-bit).
///
/// The guards below compare *whole rasters*, and `assert_ne!` on two 80 KB byte
/// slices prints both of them: one failure produced 811 KB of `255, 255, 255,`
/// and told the reader nothing. Digesting first keeps the comparison exactly as
/// strong — any single differing byte changes the digest — while making the
/// failure legible, so the ink counts printed beside it are what a reader
/// actually diagnoses from.
fn pixel_digest(surface: &Surface) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in surface.data() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The guarantee a user can see: registering a covering face through the host
/// seam changes what is painted, from the notdef boxes to real glyphs.
///
/// The tofu raster is asserted to carry ink *first*, deliberately: it records in
/// the test itself that "there are dark pixels" cannot be the guarantee, because
/// the broken state has them too.
#[test]
fn registering_a_cjk_face_changes_what_is_painted_from_tofu_to_glyphs() {
    for (script, text) in CASES {
        let tofu = raster(&ParleyShaper::without_system_fonts(), text);
        let tofu_ink = ink(&tofu);
        assert!(
            tofu_ink > 0,
            "{script}: the notdef boxes must themselves have ink — if they do not, \
             an ink-count assertion would look like a real guard while proving nothing",
        );

        let covered_shaper = ParleyShaper::without_system_fonts();
        covered_shaper.register_fallback_font(CJK_TEST_FACE.to_vec(), &CJK_SCRIPTS);
        let covered = raster(&covered_shaper, text);

        assert!(
            ink(&covered) > 0,
            "{script}: nothing was painted for {text:?} after a covering face was registered",
        );
        assert_ne!(
            pixel_digest(&covered),
            pixel_digest(&tofu),
            "{script}: {text:?} rasterized identically with and without a covering face \
             ({} ink pixels either way), so the host seam changed nothing the reader can \
             see — this is HF-176 tofu",
            tofu_ink,
        );
    }
}

/// A single CJK glyph's raster differs from *the same face's* `.notdef` raster.
///
/// This removes the one loophole in the test above: two runs of different total
/// advance would differ in their rasters for reasons that have nothing to do
/// with coverage. Here both rasters come from one face, at one size, at one
/// origin, and differ only in which glyph id is drawn — glyph 0 (`.notdef`,
/// the box) against the glyph the face maps U+4E2D to. If the renderer were
/// drawing the box for covered scalars, these would be equal.
#[test]
fn a_cjk_glyph_does_not_rasterize_as_the_faces_notdef_box() {
    let shaper = ParleyShaper::without_system_fonts();
    shaper.register_fallback_font(CJK_TEST_FACE.to_vec(), &CJK_SCRIPTS);

    let node = NodeId::from_parts(1, 1).unwrap();
    let layout = shaper.shape_paragraph(
        &[styled("中")],
        LineConstraints {
            max_width: Twip::from_points(600),
            alignment: TextAlignment::Start,
            ..LineConstraints::default()
        },
        ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
    );
    let mut glyph_run = layout.lines[0].runs[0].clone();
    glyph_run.origin = Point::new(Twip::from_points(4), Twip::from_points(SIZE_PT));
    assert_eq!(glyph_run.glyphs.len(), 1, "one scalar, one glyph");
    assert_ne!(glyph_run.glyphs[0].id, 0, "U+4E2D resolved to .notdef");

    let fonts = RegistryFontSource::new(&shaper.registry());

    let paint = |run: &casual_doc_layout::text::GlyphRun| {
        let mut list = DisplayList::new();
        list.push(PaintItem::Glyphs { run: run.clone() });
        let mut surface = Surface::new(120, 80).unwrap();
        render(&list, &mut surface, DPI, &fonts, &NoMediaSource);
        surface
    };

    let real = paint(&glyph_run);

    // The same run, the same face, the same origin — glyph 0 instead.
    let mut notdef_run = glyph_run.clone();
    notdef_run.glyphs[0].id = 0;
    let notdef = paint(&notdef_run);

    assert!(ink(&real) > 0, "U+4E2D painted nothing at all");
    assert!(
        ink(&notdef) > 0,
        "this face's .notdef paints nothing, so it is not the tofu box this \
         comparison assumes and the test proves less than it claims",
    );
    assert_ne!(
        pixel_digest(&real),
        pixel_digest(&notdef),
        "U+4E2D rasterized byte-for-byte as its own face's .notdef box ({} ink pixels \
         against the box's {}) — the glyph is covered in the cmap but the renderer is \
         still drawing tofu",
        ink(&real),
        ink(&notdef),
    );
}
