//! CJK renders because a face *covers the code point*, not because a family
//! name matched — proved against the face's own `cmap`.
//!
//! `109` HF-176 measured 9,576 CJK characters across two corpus documents, 53%
//! of each document's text, rendering as boxes on the browser build. The engine
//! has the whole seam for this (`font_registry`: a host registers bytes, the
//! shaper resolves uncovered scalars into them, the renderer rasterizes them)
//! and the browser host has had the coverage-driven fetch for as long. What was
//! missing was any guard that CJK actually comes out the other side, so nothing
//! would have noticed the day it stopped. The only CJK render test in the tree
//! (`casual-doc-render`'s `cjk_renders_real_ink_with_system_fonts`) is native,
//! depends on an installed OS face, and **returns early when none is found** —
//! so on a headless runner, and on the browser path this row is about, it
//! asserts nothing at all.
//!
//! These guards run on `ParleyShaper::without_system_fonts()` — the
//! deterministic shaper that is byte-for-byte what a browser has, with no OS
//! font source compiled in — so they measure the browser path and cannot pass
//! because the developer's Mac happens to have Hiragino installed.
//!
//! ## What is asserted, and why a weaker assertion would not do
//!
//! A tofu box is a real glyph with a real advance and a real bounding box, so
//! "it has a width" and even "it drew ink" are satisfied by the defect. The
//! guarantee here is therefore stated against the resolved face's own character
//! map: the scalar must shape to a glyph id that **that face maps that scalar
//! to**. A `.notdef` cannot satisfy it (`.notdef` is glyph 0 and no `cmap` entry
//! points at it), and neither can `parley` quietly shaping the run from a
//! sibling Latin family, because that family's `cmap` has no entry for U+4E2D.
//!
//! The fixture face is a 19-codepoint, 8.6 KB subset of Noto Sans CJK SC
//! (SIL OFL 1.1 — `fixtures/fonts/OFL-1.1-NotoSansCJK.txt`), carrying Han,
//! Hiragana, Katakana and Hangul. It is a *test* asset, deliberately not a
//! bundled production face: the owner's font-provisioning decision is that the
//! browser fetches CJK over the network, and a 16 MB face in the bundle would
//! contradict it. Subsetting to the scalars these guards name keeps the fixture
//! honest about what it proves — it covers these 19 scalars and nothing else.

use casual_doc_layout::font_registry::FontRegistry;
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{
    Decoration, FontId, Glyph, LineConstraints, LineShaper, StyledRun, TextAlignment,
};
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use skrifa::MetadataProvider;

/// A 19-codepoint subset of Noto Sans CJK SC (SIL OFL 1.1). See the module docs.
const CJK_TEST_FACE: &[u8] = include_bytes!("../../../fixtures/fonts/NotoSansCJK-TestSubset.otf");

/// The scripts a host declares when registering a CJK face, matching
/// `webapp/src/web_fonts.mjs`'s `jp`/`kr`/`sc` buckets taken together.
const CJK_SCRIPTS: [&str; 4] = ["Hani", "Hira", "Kana", "Hang"];

/// One case per CJK script family the subset covers, so a failure names the
/// script rather than "CJK".
const CASES: [(&str, &str); 4] = [
    ("Han", "中文字"),
    ("Hiragana", "あいう"),
    ("Katakana", "アイウ"),
    ("Hangul", "한글"),
];

fn run(text: &str) -> StyledRun<'_> {
    StyledRun {
        text: text.into(),
        requested_family: None,
        requested_family_kind: None,
        font: FontId(0),
        size: Twip::from_points(14),
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

/// Every `(FontId, Glyph)` a paragraph of `text` shapes to, in visual order.
fn shape(shaper: &ParleyShaper, text: &str) -> Vec<(FontId, Glyph)> {
    let node = NodeId::from_parts(1, 1).unwrap();
    let layout = shaper.shape_paragraph(
        &[run(text)],
        LineConstraints {
            max_width: Twip::from_points(600),
            alignment: TextAlignment::Start,
            ..LineConstraints::default()
        },
        ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
    );
    layout
        .lines
        .iter()
        .flat_map(|line| &line.runs)
        .flat_map(|r| r.glyphs.iter().map(move |g| (r.font, *g)))
        .collect()
}

/// The glyph id `font`'s own `cmap` maps `ch` to, or `None` when that face does
/// not cover the scalar at all. Reads the bytes the registry holds for the id,
/// so this is the face the renderer will rasterize from — not a name lookup.
fn cmap_glyph_for(registry: &FontRegistry, font: FontId, ch: char) -> Option<u32> {
    let face = registry.face(font)?;
    let font_ref = skrifa::FontRef::from_index(face.bytes.as_slice(), face.index).ok()?;
    font_ref.charmap().map(ch).map(skrifa::GlyphId::to_u32)
}

/// The precondition, stated explicitly rather than assumed: with nothing
/// registered, the deterministic browser-path shaper cannot draw CJK and *says
/// so*. This is the HF-176 defect, reproduced.
///
/// Both halves matter. If this test ever goes green on the first assertion the
/// bundle has gained a CJK face and the provisioning design has changed; if it
/// goes green on the second, the coverage report has stopped naming CJK and the
/// browser would never fetch a face.
#[test]
fn cjk_is_tofu_and_reported_when_no_face_is_registered() {
    let shaper = ParleyShaper::without_system_fonts();
    for (script, text) in CASES {
        let glyphs = shape(&shaper, text);
        assert!(
            glyphs.iter().any(|(_, glyph)| glyph.id == 0),
            "{script}: the premise of HF-176 is that the bundle cannot draw {text:?}; \
             if this fails, a CJK face has entered the bundle and the provisioning \
             decision (browser fetches CJK over the network) needs re-reading",
        );
    }
    let missing = shaper.registry().missing_coverage();
    for (script, text) in CASES {
        for ch in text.chars() {
            assert!(
                missing.contains(&ch),
                "{script}: U+{:04X} could not be drawn and was not reported, so no host \
                 would ever fetch a face for it — the loss report understates the loss",
                ch as u32,
            );
        }
    }
}

/// The guarantee: a host-registered CJK face covers every CJK scalar, proved
/// against that face's own `cmap`.
///
/// This is the assertion a tofu cannot satisfy. For each scalar it checks three
/// things that only real coverage satisfies together:
///
/// 1. the glyph id is not `.notdef`;
/// 2. the run was addressed to a **dynamic** `FontId` — the host-registered
///    face, not a bundled family parley fell back to;
/// 3. the bytes the registry holds for that `FontId` map this scalar, in their
///    own character map, to **exactly the glyph id that was shaped**.
///
/// (3) is what makes "a face was chosen because it covers the code point"
/// measurable. Name-based substitution cannot produce it, and neither can
/// shaping the run from a sibling Latin family at a coincidentally valid index.
#[test]
fn a_host_registered_cjk_face_covers_every_cjk_scalar() {
    let shaper = ParleyShaper::without_system_fonts();
    let registered = shaper.register_fallback_font(CJK_TEST_FACE.to_vec(), &CJK_SCRIPTS);
    assert!(
        !registered.is_empty(),
        "the host seam accepted the face but assigned it no FontId",
    );

    for (script, text) in CASES {
        let glyphs = shape(&shaper, text);
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(
            glyphs.len(),
            chars.len(),
            "{script}: {text:?} shaped to {} glyphs, expected one per scalar",
            glyphs.len(),
        );
        for (ch, (font, glyph)) in chars.iter().zip(&glyphs) {
            assert_ne!(
                glyph.id, 0,
                "{script}: U+{:04X} still shapes to .notdef after a covering face \
                 was registered — this is the HF-176 tofu",
                *ch as u32,
            );
            assert!(
                FontRegistry::is_dynamic(*font),
                "{script}: U+{:04X} shaped to {:?}, a bundled id — parley fell back to a \
                 sibling bundled family instead of the registered CJK face, so the glyph \
                 id indexes the wrong outlines",
                *ch as u32,
                font,
            );
            let mapped = cmap_glyph_for(&shaper.registry(), *font, *ch);
            assert_eq!(
                mapped,
                Some(glyph.id),
                "{script}: U+{:04X} shaped to glyph {} but the resolved face's own cmap \
                 maps it to {mapped:?} — the face was not chosen for covering this code \
                 point, which is exactly the name-based substitution HF-176 names",
                *ch as u32,
                glyph.id,
            );
        }
    }
}

/// Registering a CJK face clears CJK from the coverage report, so a host stops
/// re-fetching it. The browser host keys its fetch on `missingCoverage()`, so a
/// report that never clears is an unbounded download loop on every edit.
#[test]
fn a_registered_cjk_face_clears_the_coverage_report_for_its_scalars() {
    let shaper = ParleyShaper::without_system_fonts();
    shaper.register_fallback_font(CJK_TEST_FACE.to_vec(), &CJK_SCRIPTS);
    for (script, text) in CASES {
        let _ = shape(&shaper, text);
        let missing = shaper.registry().missing_coverage();
        for ch in text.chars() {
            assert!(
                !missing.contains(&ch),
                "{script}: U+{:04X} is covered by the registered face but still reported \
                 as missing, so a host would fetch the same face again on every edit",
                ch as u32,
            );
        }
    }
}

/// A scalar the registered face does *not* cover stays reported. The subset
/// deliberately omits most of Unicode, so this proves the report tracks real
/// coverage rather than being switched off wholesale by the first registration.
///
/// This is the understating-is-also-false rule applied to the loss report: a
/// host must still learn about the scripts it has not provisioned.
#[test]
fn a_scalar_the_registered_face_does_not_cover_is_still_reported() {
    let shaper = ParleyShaper::without_system_fonts();
    shaper.register_fallback_font(CJK_TEST_FACE.to_vec(), &CJK_SCRIPTS);
    // Devanagari: no bundled face covers it and the CJK subset does not either.
    let text = "\u{939}\u{93f}\u{928}"; // हिन
    let _ = shape(&shaper, text);
    let missing = shaper.registry().missing_coverage();
    for ch in text.chars() {
        assert!(
            missing.contains(&ch),
            "U+{:04X} is covered by nothing and was not reported — registering a CJK \
             face must not silence the report for every other script",
            ch as u32,
        );
    }
}
