//! Emoji shape, measure and report as whole grapheme clusters, from a face that
//! is always there.
//!
//! The bundle carried Caladea, Carlito, Roboto and Liberation and **no emoji
//! face at all**, so every pictographic scalar shaped to `.notdef` and painted
//! as a box. The status bar counted the characters, which is why a model-level
//! test would have passed for the whole time this was broken: the model held
//! them perfectly and nothing could draw them. These guards are therefore about
//! the *resolved face* and the *cluster*, never about the text.
//!
//! The five cases are the five hard shapes, one per line below: a lone scalar, a
//! ZWJ sequence, a scalar plus a skin-tone modifier, an emoji-presentation
//! sequence (base + U+FE0F), and a regional-indicator pair. Each is ONE
//! user-perceived character.

use casual_doc_layout::fonts::NOTO_EMOJI;
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{
    Decoration, FontId, Glyph, LineConstraints, LineShaper, StyledRun, TextAlignment,
};
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;

/// One emoji per hard shape, with its scalars spelled out so a reader can see
/// what the cluster is made of.
const CASES: [(&str, &str); 5] = [
    ("lone scalar", "\u{1f600}"),
    ("ZWJ sequence", "\u{1f469}\u{200d}\u{1f4bb}"),
    ("skin-tone modifier", "\u{1f44d}\u{1f3fd}"),
    ("emoji-presentation sequence", "\u{2764}\u{fe0f}"),
    ("regional-indicator pair", "\u{1f1ec}\u{1f1e7}"),
];

fn run(text: &str) -> StyledRun<'_> {
    StyledRun {
        text: text.into(),
        requested_family: None,
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

/// The bundled base draws every emoji as its own glyph — no `.notdef`, and from
/// the emoji family rather than from whichever Latin face the run asked for.
///
/// The second half of that matters as much as the first. `parley` shapes an
/// uncovered code point with a *sibling* bundled family, and the glyph ids in
/// that run index the sibling's outlines; handing the renderer the run's own
/// `FontId` made it draw whatever glyph sat at that index in the wrong font.
#[test]
fn every_emoji_shape_draws_from_the_bundled_emoji_face() {
    let shaper = ParleyShaper::without_system_fonts();
    for (name, text) in CASES {
        let glyphs = shape(&shaper, text);
        assert!(!glyphs.is_empty(), "{name}: shaped to nothing");
        for (font, glyph) in &glyphs {
            assert_ne!(
                glyph.id, 0,
                "{name} ({text:?}) shaped to .notdef — this is the tofu the owner saw",
            );
            assert!(
                NOTO_EMOJI.contains(*font),
                "{name} ({text:?}) resolved to {font:?}, not the bundled emoji family \
                 ({:?}..) — the renderer would outline that glyph id from the wrong face",
                NOTO_EMOJI.face_id(false, false),
            );
        }
    }
}

/// Every emoji is ONE cluster: its glyphs all anchor at the same model offset,
/// and the next character starts a cluster the full byte length further on.
///
/// This is the caret and backspace guarantee expressed where it is produced. A
/// cluster offset is what the caret anchors to and what a delete spans, so a ZWJ
/// sequence splitting into three clusters is a caret that stops inside a
/// person-with-laptop and a backspace that leaves half of one behind.
#[test]
fn an_emoji_is_one_cluster_however_many_scalars_it_has() {
    let shaper = ParleyShaper::without_system_fonts();
    for (name, text) in CASES {
        // `A<emoji>B`: the emoji's neighbours pin where its cluster must start
        // and end, so this cannot pass by the emoji simply being ignored.
        let padded = format!("A{text}B");
        let glyphs = shape(&shaper, &padded);
        let clusters: Vec<u32> = glyphs.iter().map(|(_, g)| g.cluster).collect();
        let emoji_start = 'A'.len_utf8() as u32;
        let emoji_end = emoji_start + text.len() as u32;

        let emoji_clusters: Vec<u32> = clusters
            .iter()
            .copied()
            .filter(|c| *c >= emoji_start && *c < emoji_end)
            .collect();
        assert!(
            !emoji_clusters.is_empty(),
            "{name}: no glyph anchored inside the emoji"
        );
        assert!(
            emoji_clusters.iter().all(|c| *c == emoji_start),
            "{name} ({text:?}) split into clusters {emoji_clusters:?}; a caret would \
             stop inside it and a backspace would leave part of it behind",
        );
        assert!(
            clusters.contains(&emoji_end),
            "{name} ({text:?}): the character after the emoji should anchor at \
             {emoji_end}, got {clusters:?}",
        );
    }
}

/// Coverage reporting names every scalar of the cluster, and reports an emoji
/// the monochrome base is standing in for.
///
/// Both halves are load-bearing and both were wrong.
///
/// *Every scalar*: `parley`'s shaping clusters do not span a ZWJ sequence or a
/// regional-indicator pair, so reporting their `text_range`s verbatim named the
/// FIRST scalar of each emoji and silently dropped the rest — a loss report that
/// understates the loss.
///
/// *Reported at all*: the bundled monochrome base draws a real glyph, so the
/// `.notdef` rule can never see it. Without a second rule, adding the base would
/// have switched the browser's colour-emoji fetch off, because the host asks for
/// faces by coverage gap and the gap would have been empty — emoji permanently
/// monochrome, and nothing to say why.
#[test]
fn coverage_reporting_names_every_scalar_of_a_degraded_emoji() {
    let shaper = ParleyShaper::without_system_fonts();
    for (_, text) in CASES {
        let _ = shape(&shaper, text);
    }
    let missing = shaper.registry().missing_coverage();
    for (name, text) in CASES {
        for ch in text.chars() {
            assert!(
                missing.contains(&ch),
                "{name}: {ch:?} (U+{:04X}) is drawn by the monochrome stand-in but was \
                 not reported, so no host would ever fetch a colour face for it",
                ch as u32,
            );
        }
    }
}

/// A cluster in a script NOTHING covers is reported whole, marks included.
///
/// This is the loss report for "a document uses a script we cannot draw", which
/// is competitive work here rather than hygiene: the whole point of reading
/// OOXML directly is that what we cannot represent is *said*, not dropped.
///
/// `parley`'s shaping clusters do not span a Devanagari syllable's vowel signs,
/// so reporting their `text_range`s verbatim named the three consonants of
/// `हिन्दी` and silently dropped all three matras — a report that understates
/// what could not be drawn, on exactly the input where the reader most needs to
/// know. Walking grapheme clusters over the run text reports all six.
#[test]
fn an_uncovered_script_is_reported_scalar_for_scalar() {
    // No bundled face covers Devanagari, and this shaper cannot reach an OS one.
    let shaper = ParleyShaper::without_system_fonts();
    let text = "\u{939}\u{93f}\u{928}\u{94d}\u{926}\u{940}"; // हिन्दी
    let glyphs = shape(&shaper, text);
    assert!(
        glyphs.iter().any(|(_, glyph)| glyph.id == 0),
        "the premise of this test is that nothing covers Devanagari here",
    );
    let missing = shaper.registry().missing_coverage();
    for ch in text.chars() {
        assert!(
            missing.contains(&ch),
            "U+{:04X} could not be drawn and was not reported — the loss report \
             understates the loss",
            ch as u32,
        );
    }
}

/// Latin text is not reported as a coverage gap — the report would be noise, and
/// a browser would fetch a multi-megabyte emoji face for every document.
#[test]
fn ordinary_text_reports_no_coverage_gap() {
    let shaper = ParleyShaper::without_system_fonts();
    let _ = shape(&shaper, "The quick brown fox jumps over the lazy dog. 123");
    assert_eq!(
        shaper.registry().missing_coverage(),
        Vec::<char>::new(),
        "plain Latin text must not ask a host to fetch anything",
    );
}
