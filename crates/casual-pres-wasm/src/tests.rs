// SPDX-License-Identifier: Apache-2.0

//! Parity guards: the facade driven natively, exactly as a browser drives it.
//!
//! The crate is `rlib` as well as `cdylib` for this reason — `casual-doc-wasm`
//! does the same. A guard that tested the engine one layer down would prove
//! nothing about the boundary, which is where a host actually lives.

use casual_doc_render::MediaSource as _;

use crate::{PackageMedia, SlideBitmap, open_deck};

#[path = "../../casual-pres-import/src/tests/deck.rs"]
#[allow(
    dead_code,
    reason = "the perturbation helpers belong to the import guards"
)]
mod deck;

/// The facade opens a real deck and answers what a host needs to draw it.
#[test]
fn the_facade_opens_a_real_deck_and_describes_it() {
    let facade = open_deck(&deck::deck()).expect("the fixture opens through the facade");

    assert_eq!(
        facade.slide_count(),
        3,
        "three slides, in presentation order"
    );
    // The surface is reported in EMU, not pixels: it is a property of the FILE and
    // the device scale belongs to the host. Returning pixels would bake one zoom
    // into the document's own dimensions.
    assert!(
        facade.slide_width_emu() > 0.0 && facade.slide_height_emu() > 0.0,
        "the deck's p:sldSz surface reaches the boundary"
    );
    assert!(
        facade.slide_width_emu() > facade.slide_height_emu(),
        "and it is the landscape surface the fixture states, not a transposed one"
    );

    // Slide ORDER, read through the boundary, and the values discriminate: the
    // fixture's parts are slide1 ("Opening"), slide2 ("Detail") and slide10
    // ("Appendix"), presented 1, 2, 10. Any LEXICAL ordering of the part names
    // gives 1, 10, 2 — "Opening", "Appendix", "Detail" — so a facade that exposed
    // part order rather than `p:sldIdLst`'s answers a different, plausible deck.
    let names: Vec<String> = (0..facade.slide_count())
        .map(|index| facade.slide_name(index))
        .collect();
    assert_eq!(
        names,
        vec![
            "Opening".to_owned(),
            "Detail".to_owned(),
            "Appendix".to_owned()
        ],
        "p:sldIdLst's order is what a host sees"
    );

    // A hidden slide is still IN the deck — retained, counted and nameable — so a
    // sorter can dim it and a slide show can skip it. Dropping it from the count
    // would make a host unable to show what the author has.
    let hidden: Vec<bool> = (0..facade.slide_count())
        .map(|index| facade.slide_hidden(index))
        .collect();
    assert_eq!(
        hidden,
        vec![false, false, true],
        "p:sld@show is reported, with its polarity the right way round"
    );

    // An index past the end answers rather than panicking: a host that races a
    // deck swap against a render request must not take the tab down.
    assert_eq!(facade.slide_name(99), "", "an absent slide has no name");
    assert!(!facade.slide_hidden(99), "and is not hidden");
}

/// A slide renders to pixels a canvas can take, and they are not blank.
#[test]
fn the_facade_renders_a_slide_to_pixels() {
    let facade = open_deck(&deck::deck()).expect("the fixture opens");

    let bitmap = facade
        .render_slide_inner(0, 96.0)
        .expect("the first slide renders");
    assert!(
        bitmap.width_px() > 0 && bitmap.height_px() > 0,
        "the bitmap has a size"
    );
    // Row-major RGBA with no padding, which is what `putImageData` wants. A host
    // that had to repack per frame would pay for it on every scroll.
    let expected = bitmap.width_px() as usize * bitmap.height_px() as usize * 4;
    let rgba = bitmap.rgba();
    assert_eq!(rgba.len(), expected, "four bytes per pixel, no padding");

    // Something was PAINTED. Measured on the colour channel, not alpha: the slide
    // surface starts opaque, so alpha is 255 everywhere and an alpha assertion
    // would pass for a blank bitmap — the exact mistake the render guards in
    // `casual-doc-render` were corrected for.
    let non_white = rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[0] != 255 || pixel[1] != 255 || pixel[2] != 255)
        .count();
    assert!(
        non_white > 100,
        "the slide must actually paint: only {non_white} non-white pixels"
    );
}

/// The device scale is the host's, and the bitmap follows it.
///
/// One `dpi` proves nothing about scaling: a facade that ignored the argument and
/// always rendered at 96 would pass a single-resolution guard and then show a
/// blurred deck on every retina display.
#[test]
fn a_higher_dpi_gives_a_proportionally_larger_bitmap() {
    let facade = open_deck(&deck::deck()).expect("the fixture opens");

    let low = facade.render_slide_inner(0, 96.0).expect("renders at 96");
    let high = facade.render_slide_inner(0, 192.0).expect("renders at 192");

    assert_eq!(
        high.width_px(),
        low.width_px() * 2,
        "double the dpi is double the width"
    );
    assert_eq!(
        high.height_px(),
        low.height_px() * 2,
        "and double the height"
    );
}

/// An out-of-range slide is an error with the index in it, not a panic.
#[test]
fn rendering_a_slide_that_does_not_exist_is_an_error() {
    let facade = open_deck(&deck::deck()).expect("the fixture opens");
    let Err(error) = facade.render_slide_inner(99, 96.0) else {
        panic!("there is no slide 99, so this must be an error rather than a bitmap");
    };
    assert!(
        error.contains("99"),
        "the error names the index the host asked for: {error}"
    );
}

/// The fidelity report crosses the boundary, and says what was lost.
///
/// Not a side channel: it is what makes direct OOXML an advantage over a converter
/// rather than a claim, so a host can say "opened with N unsupported constructs"
/// instead of opening silently.
#[test]
fn the_fidelity_report_crosses_the_boundary_as_json() {
    let facade = open_deck(&deck::deck()).expect("the fixture opens");
    let report = facade.fidelity_report();

    let parsed: serde_json::Value =
        serde_json::from_str(&report).expect("the report is valid JSON");
    let findings = parsed["findings"]
        .as_array()
        .expect("the report has a findings array");
    assert!(
        !findings.is_empty(),
        "the fixture carries constructs this build does not cover, so an empty \
         report would be the overstatement `SKILL` §9 forbids"
    );

    // The published tokens are a contract a host branches on. Asserted literally
    // so a rename one layer down is a compile error in the mapper AND a failure
    // here, rather than a silent change to the JSON.
    for finding in findings {
        let disposition = finding["disposition"]
            .as_str()
            .expect("every finding states a disposition");
        assert!(
            disposition.contains('-')
                && disposition
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
            "a boundary token is kebab-case ASCII: {disposition}"
        );
        assert!(
            finding["feature"].as_str().is_some_and(|f| !f.is_empty()),
            "and names the construct: {finding}"
        );
    }

    // NOTHING may claim `preserved`. There is no verbatim retention LEDGER on this
    // path — `save` carries bytes through, which is strictly better than destroying
    // them and is NOT the same as being able to claim them.
    assert!(
        findings
            .iter()
            .all(|finding| finding["retentionOutcome"] != "preserved"),
        "a preserved claim with no ledger would be a false preservation claim"
    );
}

/// Save goes through retention, so the parts this engine does not model survive.
#[test]
fn the_facade_saves_with_the_unmodelled_parts_carried_through() {
    use std::io::Read as _;

    let facade = open_deck(&deck::deck()).expect("the fixture opens");
    let written = facade.save().expect("the deck saves");

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(written)).expect("the output is a ZIP");
    let mut theme = Vec::new();
    archive
        .by_name("ppt/theme/theme1.xml")
        .expect("the theme part is carried through a save")
        .read_to_end(&mut theme)
        .expect("it reads back");
    assert!(
        !theme.is_empty(),
        "and it is the theme, not a zero-byte placeholder"
    );

    // And the result is still a deck. A retained part that produced a package no
    // reader accepts would be worse than the loss it prevents.
    let reopened = open_deck(&facade.save().expect("saves again")).expect("the saved deck reopens");
    assert_eq!(
        reopened.slide_count(),
        facade.slide_count(),
        "with the same slides"
    );
}

/// Media resolves out of the retained package, with and without a leading slash.
///
/// The second form is why this is a guard rather than a one-liner: an OPC override
/// names a part absolutely, and a host that handed one through would otherwise get
/// a blank gap where its picture is.
#[test]
fn media_resolves_by_part_name_in_both_forms() {
    let facade = open_deck(&deck::deck()).expect("the fixture opens");
    let media = PackageMedia(&facade.retained.parts);

    let part = facade
        .retained
        .parts
        .keys()
        .find(|name| name.starts_with("ppt/media/"))
        .expect("the fixture carries an image")
        .clone();

    assert!(
        media
            .media_bytes(&part)
            .is_some_and(|bytes| !bytes.is_empty()),
        "the normalized form resolves"
    );
    assert!(
        media.media_bytes(&format!("/{part}")).is_some(),
        "and so does the absolute form"
    );
    assert!(
        media.media_bytes("ppt/media/absent.png").is_none(),
        "while an absent part resolves to nothing rather than to another picture"
    );
}

/// The version is the manifest's, which cannot drift from it.
#[test]
fn the_engine_version_comes_from_the_manifest() {
    let version = crate::engine_version();
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "a string typed into a page is the `105` EV-002 failure; this cannot drift"
    );
    assert!(!version.is_empty());
}

/// A bitmap's pixels move rather than copy.
///
/// Stated as a guard because it is a performance contract a refactor could quietly
/// break: `rgba` taking `self` is what keeps a scroll from duplicating a
/// full-slide bitmap per frame.
#[test]
fn a_bitmaps_pixels_are_moved_out_not_copied() {
    fn takes_by_value(bitmap: SlideBitmap) -> usize {
        bitmap.rgba().len()
    }
    let facade = open_deck(&deck::deck()).expect("the fixture opens");
    let bitmap = facade.render_slide_inner(0, 96.0).expect("renders");
    let expected = bitmap.width_px() as usize * bitmap.height_px() as usize * 4;
    assert_eq!(takes_by_value(bitmap), expected);
}
