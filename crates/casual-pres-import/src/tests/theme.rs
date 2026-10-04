// SPDX-License-Identifier: Apache-2.0

//! The theme guards: `a:clrScheme`, `p:clrMap` and its override chain, the colour
//! transforms, `a:fontScheme` and `p:style`.
//!
//! A module of its own because the end-to-end file reached the ~2,000-line ceiling
//! `SKILL` §10 sets, and because these guards share three fixture helpers nothing
//! else needs. What they drive is unchanged: the one committed `.pptx`, through the
//! one public `import_pptx`.
//!
//! Every guard here was written, then driven **red** by mutating the production
//! code to reintroduce the bug it is about, then restored and confirmed green. The
//! mutation and its verbatim failure output are recorded in the commit message
//! (`SKILL` §4).

use casual_doc_model::v1::{Fill, FillStyle, GroupChild, Rgba, SchemeColor, StyleColor};
use casual_doc_package::PackageLimits;
use casual_pres_model::{PlaceholderKind, ThemeColorSlot};

use super::{deck, features, import_fixture};
use crate::{ImportLimits, import_pptx};

/// The concrete fill of a named top-level shape in one shape tree.
fn fill_of(tree: &casual_pres_model::ShapeTree, name: &str) -> Option<Fill> {
    tree.children
        .iter()
        .find(|node| node.name.as_deref() == Some(name))
        .and_then(|node| match &node.content {
            GroupChild::Shape(shape) => shape.fill.clone(),
            _ => None,
        })
}

/// A named top-level shape's node id, for a `Definitions` side-table lookup.
fn node_id_of(tree: &casual_pres_model::ShapeTree, name: &str) -> casual_doc_model::NodeId {
    tree.children
        .iter()
        .find(|node| node.name.as_deref() == Some(name))
        .map(|node| match &node.content {
            GroupChild::Shape(shape) => shape.id,
            GroupChild::Picture(picture) => picture.id,
            GroupChild::Group(group) => group.id,
            GroupChild::TextBox(text_box) => text_box.id,
        })
        .unwrap_or_else(|| panic!("the fixture carries a shape named {name}"))
}

/// The fixture deck with one part's text substituted, for a differential guard.
///
/// Asserts the substitution actually happened, because a `replace` that matched
/// nothing yields a deck identical to the original and a differential guard whose
/// two halves are the same file passes without testing anything.
fn deck_replacing(part: &str, from: &str, to: &str) -> Vec<u8> {
    let original = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == part)
            .unwrap_or_else(|| panic!("the fixture carries {part}"))
            .1,
    )
    .expect("the part is UTF-8");
    assert_eq!(
        original.matches(from).count(),
        1,
        "the perturbation must match exactly once in {part}, or the two halves of \
         the guard are not one change apart"
    );
    deck::deck_with(part, original.replace(from, to).as_bytes())
}

/// All twelve `a:clrScheme` slots arrive, in both forms the schema admits.
///
/// Twelve distinct values and no default among them, because `ColorScheme`'s
/// fields default to black: a reader that skipped a slot leaves a hole that only a
/// distinctness assertion can see. `a:dk1`/`a:lt1` are `a:sysClr` with a `lastClr`
/// and the rest are `a:srgbClr`, so honouring one form and not the other is also
/// visible here.
#[test]
fn every_theme_colour_slot_arrives_in_both_of_the_forms_the_schema_admits() {
    let imported = import_fixture();
    let scheme = imported
        .presentation
        .definitions()
        .color_scheme
        .as_ref()
        .expect("the theme part's a:clrScheme reaches the model");
    assert_eq!(scheme.name, "Fixture", "a:clrScheme@name is modelled");

    // `a:sysClr`'s `lastClr` is the colour the deck was AUTHORED to look like.
    // Resolving the system slot against this machine instead would make the same
    // file render differently per host.
    assert_eq!(
        scheme.dark1,
        SchemeColor::System(casual_doc_model::v1::SystemColor {
            value: "windowText".to_owned(),
            last_color: Some(casual_doc_model::v1::RgbColor { r: 0, g: 0, b: 0 }),
        }),
        "a:dk1 is an a:sysClr and its lastClr snapshot is retained"
    );
    assert_eq!(
        scheme.accent1,
        SchemeColor::Srgb(casual_doc_model::v1::RgbColor {
            r: 0x44,
            g: 0x72,
            b: 0xC4
        })
    );

    let slots = [
        &scheme.dark1,
        &scheme.light1,
        &scheme.dark2,
        &scheme.light2,
        &scheme.accent1,
        &scheme.accent2,
        &scheme.accent3,
        &scheme.accent4,
        &scheme.accent5,
        &scheme.accent6,
        &scheme.hyperlink,
        &scheme.followed_hyperlink,
    ];
    for (index, slot) in slots.iter().enumerate() {
        for (other_index, other) in slots.iter().enumerate() {
            assert!(
                index == other_index || slot != other,
                "slots {index} and {other_index} hold the same value, so this \
                 fixture cannot tell a skipped slot from a read one"
            );
        }
    }
}

/// A scheme colour resolves through the master's `p:clrMap`, not through an alias
/// table.
///
/// The fixture's master states the DARK mapping (`tx1="lt1"`), so `tx1` is white.
/// Every alias table in this repository — including `casual-doc-import`'s, whose
/// own comment says it assumes the identity map — would answer black. The guard is
/// differential: the SAME markup is imported against a master whose only
/// difference is `tx1="dk1"`, and must give the other answer.
#[test]
fn a_scheme_colour_resolves_through_the_colour_map_and_not_an_alias_table() {
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    let black = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    let imported = import_fixture();
    let first = imported.presentation.slides().first().expect("a slide");
    assert_eq!(
        fill_of(&first.shapes, "Themed Band"),
        Some(Fill::Solid(white)),
        "the master maps tx1 to a:lt1, whose a:sysClr lastClr is FFFFFF"
    );

    let remapped = import_pptx(
        &deck_replacing(
            "ppt/slideMasters/slideMaster1.xml",
            r#"<p:clrMap bg1="dk1" tx1="lt1""#,
            r#"<p:clrMap bg1="dk1" tx1="dk1""#,
        ),
        PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("the remapped deck imports");
    let remapped_first = remapped.presentation.slides().first().expect("a slide");
    assert_eq!(
        fill_of(&remapped_first.shapes, "Themed Band"),
        Some(Fill::Solid(black)),
        "rebinding tx1 to a:dk1 must change the answer; if it does not, the map \
         is not being read at all"
    );
}

/// One `a:schemeClr val="accent2"` means three different colours in three parts,
/// because three tiers state three colour maps.
///
/// This is the whole override chain in one assertion. The master leaves `accent2`
/// at `a:accent2`, `slideLayout2` rebinds it to `a:accent4`, and `slide10` rebinds
/// it again to `a:accent5` — so a reader that honoured only the master, or only the
/// layout, or applied a slide override to the whole deck, gets a different set of
/// three answers than this.
#[test]
fn the_colour_map_override_chain_gives_one_token_three_different_answers() {
    let imported = import_fixture();
    let presentation = &imported.presentation;

    let master = presentation.masters().first().expect("a master");
    assert_eq!(
        fill_of(&master.shapes, "Master Accent"),
        Some(Fill::Solid(Rgba {
            r: 0xED,
            g: 0x7D,
            b: 0x31,
            a: 255
        })),
        "on the master, accent2 is a:accent2"
    );

    let second = presentation.slides().get(1).expect("a second slide");
    assert_eq!(
        fill_of(&second.shapes, "Themed Box"),
        Some(Fill::Solid(Rgba {
            r: 0xFF,
            g: 0xC0,
            b: 0x00,
            a: 255
        })),
        "slide 2 inherits slideLayout2's override, which rebinds accent2 to \
         a:accent4"
    );

    // The slide's own override wins over the layout's, and the `a:alpha` on the
    // run's colour travels with it.
    let third = presentation.slides().get(2).expect("a third slide");
    let run_fill = third
        .shapes
        .children
        .iter()
        .filter_map(|node| node.text.as_ref())
        .flat_map(|body| body.paragraphs.iter())
        .flat_map(|paragraph| paragraph.runs.iter())
        .find_map(|run| run.properties().and_then(|properties| properties.fill))
        .expect("slide 10 carries a run with a themed colour");
    assert_eq!(
        run_fill,
        StyleColor::Fixed(Rgba {
            r: 57,
            g: 157,
            b: 247,
            a: 102
        }),
        "slide 10's own a:overrideClrMapping rebinds accent2 to a:accent5, \
         a:satMod val=155000 scales its saturation by 1.55, and a:alpha \
         val=40000 is 40% of 255"
    );
    // The arithmetic, written out, because this assertion CHANGED when `a:satMod`
    // stopped being a loss and a changed expectation is only trustworthy if it is
    // derived rather than copied from the new output. The override resolves
    // accent2 to a:accent5 = #5B9BD5 = (91, 155, 213). Lightness is
    // (max + min) / 2 = (213 + 91) / 2 = 152, and an exact saturation scaling
    // about it is c' = L + (c - L) * 1.55:
    //
    //   r: 152 + (91  - 152) * 1.55 = 152 - 94.55 = 57.45  -> 57
    //   g: 152 + (155 - 152) * 1.55 = 152 +  4.65 = 156.65 -> 157
    //   b: 152 + (213 - 152) * 1.55 = 152 + 94.55 = 246.55 -> 247
    //
    // and the chroma ceiling does not bind: min(2L, 510 - 2L) = min(304, 206)
    // = 206, while the scaled chroma is (213 - 91) * 1.55 = 189.1. The previous
    // expectation was the UNMODULATED base, which is to say it asserted the loss
    // this change removes.

    // And the side table records only what each part STATES, so an inherited
    // mapping stays distinguishable from one that happens to equal it.
    let mapping = presentation.color_mapping();
    assert_eq!(mapping.masters.len(), 1, "one master, one p:clrMap");
    assert_eq!(
        mapping.layouts.len(),
        1,
        "only slideLayout2 states an a:overrideClrMapping; slideLayout1 says \
         a:masterClrMapping and must not be recorded"
    );
    assert_eq!(mapping.slides.len(), 1, "only slide10 overrides");
    assert_eq!(
        presentation.color_map_of(third).accent2,
        ThemeColorSlot::Accent5,
        "the public chain lookup agrees with what the fill resolved to"
    );
}

/// A colour transform is folded in the units the file states, and a different
/// transform gives a different colour.
///
/// `val="40000"` is 40%, not 40000%, and not 0.4 of a percent. The two shapes here
/// exercise the two folds independently — a tint on a fill and a `lumMod`/`lumOff`
/// pair on an outline — and the differential half swaps the tint for a shade of the
/// SAME magnitude, which is the one perturbation a unit error cannot survive and a
/// "a colour was resolved" assertion cannot see.
#[test]
fn a_colour_transform_is_folded_in_thousandths_of_a_percent() {
    let imported = import_fixture();
    let first = imported.presentation.slides().first().expect("a slide");

    // accent1 = 4472C4; tint 40% blends toward white: c*0.4 + 255*0.6.
    assert_eq!(
        fill_of(&first.shapes, "Tinted Band"),
        Some(Fill::Solid(Rgba {
            r: 180,
            g: 199,
            b: 231,
            a: 255
        })),
        "a:tint val=40000 is 40%"
    );

    // The same base under lumMod 75% then lumOff 25%: c*0.75 + 0.25*255.
    let outline = first
        .shapes
        .children
        .iter()
        .find(|node| node.name.as_deref() == Some("Tinted Band"))
        .and_then(|node| match &node.content {
            GroupChild::Shape(shape) => shape.stroke,
            _ => None,
        })
        .expect("the band states an a:ln with a themed colour");
    assert_eq!(
        outline.color,
        Rgba {
            r: 115,
            g: 149,
            b: 211,
            a: 255
        },
        "a:lumMod then a:lumOff, each per-100000"
    );

    let shaded = import_pptx(
        &deck_replacing(
            "ppt/slides/slide1.xml",
            r#"<a:tint val="40000"/>"#,
            r#"<a:shade val="40000"/>"#,
        ),
        PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("the shaded deck imports");
    let shaded_first = shaded.presentation.slides().first().expect("a slide");
    assert_eq!(
        fill_of(&shaded_first.shapes, "Tinted Band"),
        Some(Fill::Solid(Rgba {
            r: 27,
            g: 46,
            b: 78,
            a: 255
        })),
        "a shade of the same magnitude darkens instead of lightening; equal \
         answers here would mean the transform is being ignored"
    );
}

/// A `+mj-lt` typeface RESOLVES to a real family without being folded into the
/// run.
///
/// Both halves matter. The run must still say `+mj-lt`, because rewriting it as
/// `Calibri Light` turns a theme reference into authorship and a later theme change
/// stops following it; and the deck must be able to ANSWER what `+mj-lt` means,
/// because a name nothing can resolve is the loss this work was about.
///
/// `+mj-lt` and `+mn-lt` name different families in this fixture, so resolving the
/// wrong collection is visible. The major collection's `a:ea` carries the empty
/// `@typeface` that means "fall back to latin", which must resolve to nothing
/// rather than to the empty string.
#[test]
fn a_theme_font_reference_resolves_without_being_folded_into_the_run() {
    let imported = import_fixture();
    let presentation = &imported.presentation;

    let title_typeface = presentation
        .slides()
        .first()
        .and_then(|slide| slide.shapes.title())
        .and_then(|node| node.text.as_ref())
        .and_then(|body| body.paragraphs.first())
        .and_then(|paragraph| paragraph.runs.first())
        .and_then(|run| run.properties())
        .and_then(|properties| properties.latin.clone())
        .expect("the title run states an a:latin");
    assert_eq!(
        title_typeface.name, "+mj-lt",
        "the authored reference is kept verbatim"
    );
    assert_eq!(
        presentation.resolve_typeface(&title_typeface),
        Some("Calibri Light"),
        "and it resolves through the theme's a:majorFont"
    );

    let body_typeface = presentation
        .slides()
        .first()
        .and_then(|slide| slide.shapes.slot(PlaceholderKind::SubTitle, 1))
        .and_then(|node| node.text.as_ref())
        .and_then(|body| body.paragraphs.first())
        .and_then(|paragraph| {
            paragraph
                .runs
                .iter()
                .find_map(|run| run.properties().and_then(|p| p.latin.clone()))
        })
        .expect("the subtitle states an a:latin somewhere");
    assert_eq!(body_typeface.name, "+mn-lt");
    assert_eq!(
        presentation.resolve_typeface(&body_typeface),
        Some("Calibri"),
        "the minor collection is a DIFFERENT family, so resolving the wrong one \
         would be visible"
    );

    // A concrete family passes straight through — the one call answers both cases.
    let concrete = casual_pres_model::Typeface {
        name: "Wingdings".to_owned(),
        panose: None,
    };
    assert_eq!(presentation.resolve_typeface(&concrete), Some("Wingdings"));

    // An empty entry is the "fall back to latin" marker, and fabricating a family
    // for it would put a font nobody asked for in front of the matcher.
    let east_asian = casual_pres_model::Typeface {
        name: "+mj-ea".to_owned(),
        panose: None,
    };
    assert_eq!(presentation.resolve_typeface(&east_asian), None);

    let scheme = presentation
        .definitions()
        .font_scheme
        .as_ref()
        .expect("the a:fontScheme reaches the model");
    assert_eq!(
        scheme.major.script_overrides.len(),
        1,
        "the a:font script override is read, not dropped"
    );
    assert_eq!(scheme.major.script_overrides[0].script, "Hans");
    assert_eq!(
        scheme.minor.latin.panose.as_deref(),
        Some("020F0502020204030204"),
        "the panose hint rides along with the family"
    );
}

/// A shape's `p:style` keeps its references and resolves its `a:phClr` argument,
/// and the style matrix keeps a `None` where an entry is not modelled.
///
/// The index rules are what make this dangerous. `a:lnRef@idx="2"` names the
/// fixture's gradient-filled outline, which this build cannot hold; if the reader
/// had DROPPED that entry instead of keeping a `None` in its place, `idx="2"` would
/// resolve to the solid entry at index 1 and the shape would paint a confidently
/// wrong outline with nothing reporting it. So the guard asserts both the hole and
/// the length.
#[test]
fn a_shape_style_keeps_its_references_and_resolves_its_placeholder_argument() {
    let imported = import_fixture();
    let presentation = &imported.presentation;
    let first = presentation.slides().first().expect("a slide");
    let bar = node_id_of(&first.shapes, "Accent Bar");
    let style = presentation
        .definitions()
        .shape_styles
        .get(&bar)
        .copied()
        .expect("the shape's p:style reaches Definitions::shape_styles");

    assert_eq!(style.fill_idx, Some(1), "a:fillRef@idx is one-based");
    assert_eq!(
        style.fill_color,
        Some(Rgba {
            r: 0x44,
            g: 0x72,
            b: 0xC4,
            a: 255
        }),
        "the a:phClr argument the SHAPE supplies is resolved: accent1, under this \
         slide's colour map"
    );
    assert_eq!(style.line_idx, Some(2));
    assert_eq!(
        style.line_color,
        Some(Rgba {
            r: 34,
            g: 57,
            b: 98,
            a: 255
        }),
        "a:lnRef's argument is accent1 with a:shade val=50000, folded"
    );
    assert_eq!(
        style.effect_idx,
        Some(0),
        "idx 0 is \"no effect\", which is honoured exactly and is not a loss"
    );

    // The matrix the reference resolves against.
    let scheme = presentation
        .definitions()
        .format_scheme
        .as_ref()
        .expect("the a:fmtScheme reaches the model");
    assert_eq!(
        scheme.fill_styles.len(),
        3,
        "three a:fillStyleLst entries, including the one that is not modelled"
    );
    assert!(matches!(
        scheme.fill_style(1),
        Some(FillStyle::Solid { .. })
    ));
    assert!(matches!(scheme.fill_style(3), Some(FillStyle::Pattern(_))));
    assert_eq!(
        scheme.line_styles.len(),
        2,
        "both a:lnStyleLst entries occupy an index"
    );
    assert_eq!(
        scheme.line_style(1).map(|entry| entry.width_emu),
        Some(6_350),
        "a:ln@w is EMU"
    );
    assert_eq!(
        scheme.line_style(2),
        None,
        "the gradient-filled outline is not modelled, and it must resolve to \
         NOTHING rather than to its neighbour"
    );
    assert_eq!(
        scheme.effect_style(1).map(|entry| entry.carries_effects),
        Some(false),
        "an empty a:effectLst carries no effect, so a reference to it loses nothing"
    );
    assert_eq!(
        scheme.effect_style(2).map(|entry| entry.carries_effects),
        Some(true),
        "and the a:outerShdw entry does, which is what makes the report honest \
         rather than noisy"
    );

    // Every stop of the theme's gradient entry is the `a:phClr` PLACEHOLDER, and
    // the three differ only in their transforms — so a build that dropped the
    // transforms would resolve a three-stop gradient to three copies of one colour.
    let Some(FillStyle::Gradient(gradient)) = scheme.fill_style(2) else {
        panic!("entry 2 is the gradient");
    };
    assert_eq!(gradient.stops.len(), 3);
    let tints: Vec<Option<i32>> = gradient
        .stops
        .iter()
        .map(|stop| match stop.color {
            StyleColor::Placeholder(transform) => transform.tint,
            StyleColor::Fixed(_) => panic!("a phClr stop must not be resolved here"),
        })
        .collect();
    assert_eq!(
        tints,
        vec![Some(67_000), Some(73_000), Some(81_000)],
        "the three stops differ only in their a:tint, in per-100000 units"
    );
}

/// A shape whose `a:fillRef` argument is itself `a:phClr` supplies no colour, and
/// the model says so rather than inventing one.
///
/// `StyleColor::Placeholder` resolved against no argument is `None`, and that is
/// the honest answer: the reference says "use the colour the shape names" and the
/// shape named the parameter again.
#[test]
fn a_style_reference_whose_argument_is_the_placeholder_resolves_to_nothing() {
    let imported = import_pptx(
        &deck_replacing(
            "ppt/slides/slide1.xml",
            r#"<a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef>"#,
            r#"<a:fillRef idx="1"><a:schemeClr val="phClr"/></a:fillRef>"#,
        ),
        PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("the deck imports");
    let first = imported.presentation.slides().first().expect("a slide");
    let bar = node_id_of(&first.shapes, "Accent Bar");
    let style = imported
        .presentation
        .definitions()
        .shape_styles
        .get(&bar)
        .copied()
        .expect("the p:style is still read");
    assert_eq!(style.fill_idx, Some(1), "the reference itself is unchanged");
    assert_eq!(
        style.fill_color, None,
        "a:phClr is a formal parameter; resolving it to a colour here would give \
         every styled shape in the deck the same fill"
    );
}

/// A deck with no theme part still opens, and every scheme colour in it is
/// REPORTED rather than painted in substituted Office colours.
///
/// This is the half of the behaviour that must survive the new resolution: a
/// branded deck whose theme we could not read must not reopen in a palette we
/// invented, because an invented palette looks deliberate.
#[test]
fn a_deck_with_no_theme_part_reports_every_scheme_colour_rather_than_guessing() {
    let imported = import_pptx(
        &deck::deck_without("ppt/theme/theme1.xml"),
        PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("a deck with no theme part still opens");
    let features = features(&imported);
    assert!(
        features.contains(&"schemeClr"),
        "with no palette, a theme slot is a reported gap again: {features:?}"
    );

    let first = imported.presentation.slides().first().expect("a slide");
    assert_eq!(
        fill_of(&first.shapes, "Themed Band"),
        None,
        "and the shape keeps NO fill rather than one we chose for it"
    );
    assert!(
        imported.presentation.definitions().color_scheme.is_none(),
        "nothing may fabricate a colour scheme"
    );
    assert_eq!(
        imported
            .presentation
            .resolve_typeface(&casual_pres_model::Typeface {
                name: "+mj-lt".to_owned(),
                panose: None,
            }),
        None,
        "and a theme font reference answers nothing rather than a default family"
    );
}

/// An `a:clrScheme` that filled NO slot gives no palette, and one that filled some
/// but not all of them gives a palette plus a finding.
///
/// `ColorScheme`'s twelve fields are values and default to black, so admitting an
/// empty scheme would resolve every role in the deck to black — a confidently wrong
/// answer wearing the shape of a palette, which is exactly what reporting exists to
/// prevent. A PARTIAL scheme is admitted because the slots that arrived are real,
/// and reported because the model cannot say "this slot was not stated".
#[test]
fn an_empty_colour_scheme_gives_no_palette_and_a_partial_one_is_reported() {
    let theme = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/theme/theme1.xml")
            .expect("the fixture carries the theme part")
            .1,
    )
    .expect("the theme part is UTF-8");
    let scheme_start = theme
        .find(r#"<a:clrScheme name="Fixture">"#)
        .expect("the fixture's a:clrScheme is spelled this way");
    let scheme_end = theme.find("</a:clrScheme>").expect("and it closes") + "</a:clrScheme>".len();

    let with_scheme = |replacement: &str| {
        let mut rewritten = theme.clone();
        rewritten.replace_range(scheme_start..scheme_end, replacement);
        import_pptx(
            &deck::deck_with("ppt/theme/theme1.xml", rewritten.as_bytes()),
            PackageLimits::default(),
            ImportLimits::default(),
        )
        .expect("the deck still imports")
    };

    let emptied = with_scheme(r#"<a:clrScheme name="Fixture"/>"#);
    assert!(
        emptied.presentation.definitions().color_scheme.is_none(),
        "a scheme with no slots must not become twelve blacks"
    );
    assert!(
        features(&emptied).contains(&"clrScheme"),
        "and the fact is reported: {:?}",
        features(&emptied)
    );
    assert!(
        features(&emptied).contains(&"schemeClr"),
        "so every scheme colour is an unresolved gap again"
    );

    let partial = with_scheme(
        r#"<a:clrScheme name="Fixture"><a:dk1><a:srgbClr val="010203"/></a:dk1></a:clrScheme>"#,
    );
    let read = partial
        .presentation
        .definitions()
        .color_scheme
        .as_ref()
        .expect("one real slot is still a palette");
    assert_eq!(
        read.dark1,
        SchemeColor::Srgb(casual_doc_model::v1::RgbColor {
            r: 0x01,
            g: 0x02,
            b: 0x03
        }),
        "the slot that WAS stated is kept"
    );
    assert!(
        features(&partial).contains(&"clrScheme"),
        "and the eleven that were not are reported: {:?}",
        features(&partial)
    );

    // The committed fixture states all twelve, so it must NOT carry the finding —
    // otherwise the two assertions above pass for a reason unrelated to the count.
    assert!(
        !features(&import_fixture()).contains(&"clrScheme"),
        "a complete scheme is not a loss"
    );
}

/// A deck whose theme is related ONLY from the master — which is the one
/// relationship ECMA-376 requires — still resolves its colours.
///
/// `ppt/_rels/presentation.xml.rels` carrying a `theme` relationship is a
/// PowerPoint convention, not a requirement: §13.3.8 puts the required theme
/// relationship on the slide master. A reader that looked only at the presentation
/// part would import a conformant package with no theme at all and report every
/// scheme colour, which is the defect this whole change exists to remove — and it
/// would do it on exactly the files that follow the spec rather than the
/// convention.
///
/// Differential, because "it resolved" is satisfiable by the fixture's own
/// presentation-level relationship: the perturbation REMOVES that relationship, so
/// the master's is the only one left.
#[test]
fn a_theme_related_only_from_the_master_is_still_found() {
    let rels = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/_rels/presentation.xml.rels")
            .expect("the fixture carries the presentation relationships")
            .1,
    )
    .expect("the part is UTF-8");
    let theme_line = rels
        .lines()
        .find(|line| line.contains(r#"Id="rIdTheme""#))
        .expect("the fixture relates the theme from the presentation part too");

    let master_only = import_pptx(
        &deck::deck_with(
            "ppt/_rels/presentation.xml.rels",
            rels.replace(theme_line, "").as_bytes(),
        ),
        PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("a deck whose theme is related only from the master still imports");

    assert!(
        master_only
            .presentation
            .definitions()
            .color_scheme
            .is_some(),
        "the master's own theme relationship is authoritative and must be followed"
    );
    let first = master_only.presentation.slides().first().expect("a slide");
    assert_eq!(
        fill_of(&first.shapes, "Themed Band"),
        Some(Fill::Solid(Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        })),
        "and the colour resolves to the same answer as with both relationships"
    );
    let features = features(&master_only);
    assert!(
        !features.contains(&"schemeClr"),
        "so nothing is reported as unresolvable: {features:?}"
    );
    assert!(
        !features.contains(&"ppt/theme/theme1.xml"),
        "and the theme part is still consumed: {features:?}"
    );
}
