//! Document-embedded (`.odttf`) faces at the layout seam.
//!
//! `40-FONT-MANAGEMENT-DESIGN.md` §3.2 puts the document's own faces at the
//! **top** of the three-tier chain (embedded → bundled → host-enumerated), and
//! §3.3 owns the de-obfuscation. These tests drive the whole seam against the
//! real shaper: a document that embeds a face must shape and rasterize with
//! *that* face, not with the metric substitute the family name would otherwise
//! pick; and every way an embedded face can fail must degrade to substitution
//! and be reported, never panic and never paint tofu.
//!
//! The embedded payload is a bundled Liberation face re-obfuscated with a real
//! Word `w:fontKey`, so the fixture is a genuine `.odttf` stream without adding
//! a redistributable font blob to `fixtures/`. Liberation **Mono** is chosen
//! deliberately: the substitute for an unrecognized family name is Liberation
//! *Sans* (proportional), so "did the embedded face win?" is observable in the
//! glyph advances themselves, not only in an id.

use casual_doc_layout::font_registry::{
    EmbeddedFaceError, FontRegistry, register_embedded_fonts, register_embedded_fonts_bounded,
};
use casual_doc_layout::fonts::{
    LIBERATION_MONO_BOLD, LIBERATION_MONO_BOLD_ITALIC, LIBERATION_MONO_ITALIC,
    LIBERATION_MONO_REGULAR, LIBERATION_SANS_REGULAR,
};
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{Decoration, FontId, LineConstraints, LineShaper, StyledRun};
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, EmbeddedFace, EmbeddedFontSet, FontDescriptor, FontSig,
    InlineNode, Paragraph, ParagraphProperties, Run, RunProperties,
};

/// A family name no entry in `font_substitution`'s table knows, and with no
/// `mono`/`serif` substring — so without an embedded face it classifies generic
/// sans and substitutes Liberation Sans.
const FAMILY: &str = "Opendoc Embedded Probe";
const REGULAR_PART: &str = "word/fonts/font1.odttf";
const BOLD_PART: &str = "word/fonts/font2.odttf";
const ITALIC_PART: &str = "word/fonts/font3.odttf";
const BOLD_ITALIC_PART: &str = "word/fonts/font4.odttf";
/// A real `w:fontKey` from a Word-produced package.
const KEY: &str = "{3EEE3167-E5B8-4798-AE48-EA6B71E31D4D}";

/// Applies the ECMA-376 Part 1 §17.8.1 obfuscation: the first 32 bytes XORed
/// with the `w:fontKey`'s hex bytes in reverse order. Written here rather than
/// reusing the engine's function so the fixture is produced independently of
/// the code under test.
fn obfuscate(font_key: &str, plain: &[u8]) -> Vec<u8> {
    let digits: Vec<u32> = font_key.chars().filter_map(|c| c.to_digit(16)).collect();
    assert_eq!(digits.len(), 32, "a fontKey is 32 hex digits");
    let key: Vec<u8> = digits
        .chunks(2)
        .map(|pair| (pair[0] * 16 + pair[1]) as u8)
        .rev()
        .collect();
    let mut out = plain.to_vec();
    for (index, byte) in out.iter_mut().take(32).enumerate() {
        *byte ^= key[index % 16];
    }
    out
}

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn face(part_name: &str) -> Option<EmbeddedFace> {
    Some(EmbeddedFace {
        font_key: KEY.to_owned(),
        subsetted: false,
        relationship_id: "rId1".to_owned(),
        part_name: part_name.to_owned(),
    })
}

/// A document whose `fontTable.xml` declares `FAMILY` with the given embedded
/// faces, and whose body names that family.
fn document(embedded: EmbeddedFontSet) -> Document {
    Document::new(
        node(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default(),
            inlines: vec![InlineNode::Run(Run {
                id: node(11),
                properties: RunProperties::default(),
                text: "iiiWWW".to_owned(),
            })],
        })],
        Definitions {
            font_table: vec![FontDescriptor {
                name: FAMILY.to_owned(),
                alt_name: None,
                panose1: None,
                charset: None,
                family: None,
                pitch: None,
                sig: FontSig::default(),
                not_true_type: false,
                embedded,
            }],
            ..Definitions::default()
        },
    )
    .expect("the probe document is valid")
}

fn regular_only() -> EmbeddedFontSet {
    EmbeddedFontSet {
        regular: face(REGULAR_PART),
        ..EmbeddedFontSet::default()
    }
}

fn styled(bold: bool, italic: bool) -> StyledRun<'static> {
    StyledRun {
        text: "iiiWWW".into(),
        requested_family: Some(FAMILY.into()),
        font: FontId(0),
        size: Twip::from_points(11),
        character_scale_percent: 100,
        bold,
        italic,
        letter_spacing: Twip::ZERO,
        color: [0, 0, 0, 255],
        decoration: Decoration::default(),
        highlight: None,
        shading: None,
        baseline_shift: Twip::ZERO,
    }
}

/// Shapes one run requesting `FAMILY` and returns the face it resolved to plus
/// the glyph advances it produced.
fn shape(shaper: &ParleyShaper, bold: bool, italic: bool) -> (FontId, Vec<i32>) {
    let anchor = node(10);
    let layout = shaper.shape_paragraph(
        &[styled(bold, italic)],
        LineConstraints {
            max_width: Twip::from_points(400),
            ..LineConstraints::default()
        },
        ModelRange::new(ModelPos::new(anchor, 0), ModelPos::new(anchor, 6)),
    );
    let run = layout.lines[0].runs.first().expect("the run shaped");
    (
        run.font,
        run.glyphs.iter().map(|glyph| glyph.advance.raw()).collect(),
    )
}

/// The headline guarantee of FID-L-01: a run naming an embedded family shapes
/// with the document's own face, and the renderer can fetch that face's exact
/// bytes. Without registration the same run falls to the bundled substitute.
#[test]
fn an_embedded_face_outranks_the_metric_substitute() {
    let doc = document(regular_only());
    let odttf = obfuscate(KEY, LIBERATION_MONO_REGULAR);

    // Baseline: no embedded face registered. The unknown family classifies
    // generic sans and shapes with bundled Liberation Sans.
    let bare = ParleyShaper::new();
    let (substitute_face, substitute_advances) = shape(&bare, false, false);
    assert!(
        !FontRegistry::is_dynamic(substitute_face),
        "without an embedded face the run keeps a bundled substitute id",
    );
    assert!(
        substitute_advances
            .windows(2)
            .any(|pair| pair[0] != pair[1]),
        "the substitute (Liberation Sans) is proportional: 'i' and 'W' differ",
    );

    // With the document's own face registered.
    let shaper = ParleyShaper::new();
    let outcome = register_embedded_fonts(&shaper, &doc, |part| {
        (part == REGULAR_PART).then_some(odttf.as_slice())
    });
    assert_eq!(outcome.faces, 1, "one embedded face registered");
    assert_eq!(outcome.families, vec![FAMILY.to_owned()]);
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);

    let (embedded_face, embedded_advances) = shape(&shaper, false, false);
    assert!(
        FontRegistry::is_dynamic(embedded_face),
        "the run resolved to the dynamically registered embedded face, not a \
         bundled id — this is the assertion FID-L-01 was missing",
    );
    assert_eq!(
        shaper
            .registry()
            .face(embedded_face)
            .expect("the embedded face is served from the registry")
            .bytes
            .as_slice(),
        LIBERATION_MONO_REGULAR,
        "the registry serves the de-obfuscated bytes for rasterization, so the \
         shaped face and the painted face are the same file",
    );
    assert!(
        embedded_advances.windows(2).all(|pair| pair[0] == pair[1]),
        "the embedded face is monospaced: every advance is equal — the run is \
         genuinely shaped with the document's face, not merely tagged with its id",
    );
    assert_ne!(
        embedded_advances, substitute_advances,
        "embedding the face changes line breaking, which is the point",
    );
}

/// Each `w:embed*` slot registers with the weight and style the slot declares,
/// so a bold run picks the embedded bold face rather than synthesizing one.
#[test]
fn each_embed_slot_registers_under_its_own_weight_and_style() {
    let doc = document(EmbeddedFontSet {
        regular: face(REGULAR_PART),
        bold: face(BOLD_PART),
        italic: face(ITALIC_PART),
        bold_italic: face(BOLD_ITALIC_PART),
    });
    let parts: Vec<(&str, Vec<u8>)> = vec![
        (REGULAR_PART, obfuscate(KEY, LIBERATION_MONO_REGULAR)),
        (BOLD_PART, obfuscate(KEY, LIBERATION_MONO_BOLD)),
        (ITALIC_PART, obfuscate(KEY, LIBERATION_MONO_ITALIC)),
        (
            BOLD_ITALIC_PART,
            obfuscate(KEY, LIBERATION_MONO_BOLD_ITALIC),
        ),
    ];

    let shaper = ParleyShaper::new();
    let outcome = register_embedded_fonts(&shaper, &doc, |name| {
        parts
            .iter()
            .find(|(part, _)| *part == name)
            .map(|(_, bytes)| bytes.as_slice())
    });
    assert_eq!(outcome.faces, 4, "all four slots registered");
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);

    for (bold, italic, expected) in [
        (false, false, LIBERATION_MONO_REGULAR),
        (true, false, LIBERATION_MONO_BOLD),
        (false, true, LIBERATION_MONO_ITALIC),
        (true, true, LIBERATION_MONO_BOLD_ITALIC),
    ] {
        let (id, _) = shape(&shaper, bold, italic);
        assert!(
            FontRegistry::is_dynamic(id),
            "bold={bold} italic={italic} resolved to an embedded face",
        );
        assert_eq!(
            shaper.registry().face(id).unwrap().bytes.as_slice(),
            expected,
            "bold={bold} italic={italic} selected the face its w:embed* slot \
             declared, not another slot's",
        );
    }
}

/// A face that cannot be de-obfuscated degrades to substitution *and* is
/// reported. Silent tofu — or a silent swap with no finding — is the failure
/// mode this guards.
#[test]
fn a_corrupt_embedded_face_falls_back_to_substitution_and_is_reported() {
    let doc = document(regular_only());
    // Obfuscated with a different key than the one the document declares.
    let wrong = obfuscate(
        "{00000000-1111-2222-3333-444444444444}",
        LIBERATION_MONO_REGULAR,
    );

    let shaper = ParleyShaper::new();
    let outcome = register_embedded_fonts(&shaper, &doc, |part| {
        (part == REGULAR_PART).then_some(wrong.as_slice())
    });
    assert_eq!(outcome.faces, 0, "nothing was registered");
    assert!(outcome.families.is_empty());
    assert_eq!(outcome.failures.len(), 1);
    let failure = &outcome.failures[0];
    assert_eq!(failure.error, EmbeddedFaceError::NotSfnt);
    assert_eq!(failure.family, FAMILY);
    assert_eq!(failure.slot, "w:embedRegular");
    assert_eq!(failure.part_name, REGULAR_PART);

    let (id, advances) = shape(&shaper, false, false);
    assert!(
        !FontRegistry::is_dynamic(id),
        "the run fell back to the bundled substitute",
    );
    assert!(
        advances.iter().all(|&advance| advance > 0),
        "the fallback shapes real glyphs, not .notdef boxes",
    );
}

/// The `.odttf` part may be unreadable or absent from the resource map (a
/// truncated package, a relationship pointing nowhere). That is reported, not
/// panicked and not silently ignored.
#[test]
fn a_missing_odttf_part_is_reported() {
    let doc = document(regular_only());
    let shaper = ParleyShaper::new();
    let outcome = register_embedded_fonts(&shaper, &doc, |_| None);
    assert_eq!(outcome.faces, 0);
    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].error, EmbeddedFaceError::MissingPart);
    assert_eq!(
        outcome.failures[0].error.as_str(),
        "missing-part",
        "the reason reaches the host as a stable report id",
    );
}

/// A malformed `w:fontKey` cannot decode anything; it is rejected by reason
/// rather than by guessing.
#[test]
fn a_malformed_font_key_is_reported() {
    let mut embedded = regular_only();
    embedded.regular.as_mut().unwrap().font_key = "{not-a-guid}".to_owned();
    let doc = document(embedded);
    let odttf = obfuscate(KEY, LIBERATION_MONO_REGULAR);

    let shaper = ParleyShaper::new();
    let outcome = register_embedded_fonts(&shaper, &doc, |_| Some(odttf.as_slice()));
    assert_eq!(outcome.faces, 0);
    assert_eq!(
        outcome.failures[0].error,
        EmbeddedFaceError::MalformedFontKey,
    );
}

/// The aggregate resource bound holds: once the document's embedded fonts have
/// spent the budget, further faces are refused (and reported) instead of being
/// copied into memory. Driven with an explicit small budget so the guard is
/// reachable without allocating the production ceiling.
#[test]
fn the_aggregate_font_budget_refuses_further_faces() {
    let doc = document(EmbeddedFontSet {
        regular: face(REGULAR_PART),
        bold: face(BOLD_PART),
        ..EmbeddedFontSet::default()
    });
    let first = obfuscate(KEY, LIBERATION_MONO_REGULAR);
    let second = obfuscate(KEY, LIBERATION_SANS_REGULAR);
    // Enough for the first face and nothing like enough for the second.
    let budget = first.len() as u64 + 16;

    let shaper = ParleyShaper::new();
    let outcome = register_embedded_fonts_bounded(
        &shaper,
        &doc,
        |part| {
            if part == REGULAR_PART {
                Some(first.as_slice())
            } else {
                Some(second.as_slice())
            }
        },
        budget,
    );
    assert_eq!(outcome.faces, 1, "only the first face fit the budget");
    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].error, EmbeddedFaceError::BudgetExceeded,);
    assert_eq!(outcome.failures[0].slot, "w:embedBold");
}

/// A document with no embedded fonts is untouched — no registrations, no
/// findings, and the bundled substitution path is exactly as before.
#[test]
fn a_document_without_embedded_fonts_is_unaffected() {
    let doc = document(EmbeddedFontSet::default());
    let shaper = ParleyShaper::new();
    let outcome = register_embedded_fonts(&shaper, &doc, |_| {
        panic!("no part should be requested when nothing is embedded")
    });
    assert_eq!(outcome, Default::default());
    assert!(shaper.registry().snapshot().is_empty());
}
