// SPDX-License-Identifier: Apache-2.0

//! The object-level constructs a drawing carries beside its geometry:
//! `a:srcRect`, `a:hlinkClick` and a connector's `a:stCxn`/`a:endCxn`.
//!
//! Every guard here was written, driven **red** by a mutation of the production
//! code, and only then restored. The mutations and their verbatim output are in
//! the commit message.
//!
//! # Why these are driven by PERTURBING the fixture rather than by extending it
//!
//! The deck in `super::deck` is shared with `casual-pres-export`,
//! `casual-pres-layout` and `casual-pres-wasm`, and adding a shape to it mints
//! node ids that shift every id after the insertion point — `SLIDE_TEN`'s own
//! note on "Invisible Rule" sets out why that matters. So each construct here is
//! injected into one part by string replacement, which also makes every guard
//! **differential**: the same deck one change apart, so an assertion cannot pass
//! because the shape never reached the reader at all.
//!
//! Each replacement asserts its own `matches(…).count()` first. A
//! `str::replace` that matched nothing would leave the deck untouched and the
//! "before" and "after" halves identical, which is a guard that passes while
//! testing nothing — the exact failure `SKILL` §4 is about.

use casual_doc_model::v1::{CROP_MAX, CROP_MIN, CropRect, GroupChild, HyperlinkTarget};
use casual_doc_package::PackageLimits;

use super::deck;
use super::features;
use crate::{ImportLimits, ImportedPresentation, import_pptx};

/// Opens a deck, failing the test with the real error if it refuses.
fn open(bytes: &[u8]) -> ImportedPresentation {
    import_pptx(bytes, PackageLimits::default(), ImportLimits::default())
        .expect("the perturbed fixture imports")
}

/// One fixture part as a `String`, for the replacements below.
fn part(name: &str) -> String {
    String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(part, _)| part == name)
            .unwrap_or_else(|| panic!("the fixture carries {name}"))
            .1,
    )
    .unwrap_or_else(|_| panic!("{name} is UTF-8"))
}

/// The fixture with `replacement` swapped in for `needle` in `name`, asserting
/// the needle was there exactly once.
fn deck_replacing(name: &str, needle: &str, replacement: &str) -> Vec<u8> {
    let body = part(name);
    assert_eq!(
        body.matches(needle).count(),
        1,
        "the perturbation must match {name} exactly once, or the two halves of the \
         guard are the same deck"
    );
    deck::deck_with(name, body.replace(needle, replacement).as_bytes())
}

/// The fixture with two parts replaced — a slide and its own `_rels`, which is
/// what a hyperlink needs: the element names an `r:id` and the relationship part
/// is where that id means anything.
fn deck_replacing_two(first: (&str, &str, &str), second: (&str, &str, &str)) -> Vec<u8> {
    let mut parts = deck::deck_parts();
    for (name, needle, replacement) in [first, second] {
        let mut hit = false;
        for entry in &mut parts {
            if entry.0 != name {
                continue;
            }
            let body = String::from_utf8(entry.1.clone()).expect("the part is UTF-8");
            assert_eq!(
                body.matches(needle).count(),
                1,
                "the perturbation must match {name} exactly once"
            );
            entry.1 = body.replace(needle, replacement).into_bytes();
            hit = true;
        }
        assert!(hit, "the fixture carries {name}");
    }
    deck::build_pptx(&parts)
}

/// Slide 2's picture, which every guard in the first half is about.
const PICTURE_BLIP_FILL: &str =
    r#"<p:blipFill><a:blip r:embed="rIdImage"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>"#;

/// Its `p:cNvPr`, self-closing in the fixture — so a hyperlink guard that forgot
/// to open it up would be replacing nothing.
const PICTURE_CNVPR: &str = r#"<p:cNvPr id="4" name="Logo" descr="The company mark"/>"#;

/// Slide 10's connector, whose `p:cNvCxnSpPr` binds neither end.
const CONNECTOR_CNVCXNSPPR: &str = r#"<p:cNvCxnSpPr/>"#;

const SLIDE_TWO: &str = "ppt/slides/slide2.xml";
const SLIDE_TWO_RELS: &str = "ppt/slides/_rels/slide2.xml.rels";
const SLIDE_TEN: &str = "ppt/slides/slide10.xml";

/// Slide 2's top-level picture.
fn picture(imported: &ImportedPresentation) -> casual_doc_model::v1::GroupPicture {
    imported
        .presentation
        .slides()
        .get(1)
        .expect("the fixture has a second slide")
        .shapes
        .children
        .iter()
        .find_map(|node| match &node.content {
            GroupChild::Picture(picture) => Some(picture.clone()),
            _ => None,
        })
        .expect("slide 2 carries a top-level p:pic")
}

/// A crop arrives edge by edge, in the authored units, on the edge it was
/// authored on.
///
/// # Why all four edges differ
///
/// `a:srcRect`'s four attributes are the one place a reader can be wrong in a way
/// that still looks right: `l`/`r` and `t`/`b` transposed gives a plausible crop
/// of the wrong part of the image. Four distinct values, none a multiple of
/// another, so no pair of them can be swapped undetected — and the fixture's
/// picture is uncropped, so the "before" half proves the value came from the
/// markup rather than from a default.
#[test]
fn a_cropped_picture_keeps_each_edge_on_the_edge_it_was_authored_on() {
    assert_eq!(
        picture(&open(&deck::deck())).crop,
        None,
        "the shared fixture's picture is uncropped, or the guard below cannot tell \
         a read crop from a pre-existing one"
    );

    let cropped = open(&deck_replacing(
        SLIDE_TWO,
        PICTURE_BLIP_FILL,
        r#"<p:blipFill><a:blip r:embed="rIdImage"/><a:srcRect l="11000" t="23000" r="7000" b="31000"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>"#,
    ));
    assert_eq!(
        picture(&cropped).crop,
        Some(CropRect {
            left: 11_000,
            top: 23_000,
            right: 7_000,
            bottom: 31_000,
        }),
        "thousandths of a percent, verbatim, per edge"
    );
    assert!(
        !features(&cropped).contains(&"srcRect"),
        "a crop that is READ must not also be reported as lost: {:?}",
        features(&cropped)
    );
}

/// A stated edge the markup omits is zero on that side, not a dropped crop.
///
/// PowerPoint writes only the edges a user actually dragged, so
/// `<a:srcRect t="20000"/>` is the common shape and a reader that required four
/// attributes would drop the overwhelming majority of real crops.
#[test]
fn a_partial_source_rect_crops_only_the_edges_it_states() {
    let cropped = open(&deck_replacing(
        SLIDE_TWO,
        PICTURE_BLIP_FILL,
        r#"<p:blipFill><a:blip r:embed="rIdImage"/><a:srcRect t="20000"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>"#,
    ));
    assert_eq!(
        picture(&cropped).crop,
        Some(CropRect {
            left: 0,
            top: 20_000,
            right: 0,
            bottom: 0,
        }),
        "one stated edge, three zeroes"
    );
}

/// An all-zero `a:srcRect` is the identity crop and must model as **no** crop.
///
/// A producer that writes the no-op explicitly must not become a document
/// carrying a field that changes nothing: an editor offering to reset a crop would
/// then offer it on a picture that has none, and a round trip would write an
/// element the source did not need. The same rule the DOCX reader applies, through
/// the same `CropRect::is_identity`.
#[test]
fn an_identity_source_rect_is_not_a_crop() {
    let identity = open(&deck_replacing(
        SLIDE_TWO,
        PICTURE_BLIP_FILL,
        r#"<p:blipFill><a:blip r:embed="rIdImage"/><a:srcRect l="0" t="0" r="0" b="0"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>"#,
    ));
    assert_eq!(
        picture(&identity).crop,
        None,
        "an all-zero rect hides nothing, so modelling it would state a crop the \
         file does not have"
    );
    assert!(
        !features(&identity).contains(&"srcRect"),
        "and it is not a LOSS either — nothing was dropped: {:?}",
        features(&identity)
    );
}

/// An out-of-range edge is clamped into the model's domain rather than carried.
///
/// Both directions, in one rect, because the clamp is two comparisons and a guard
/// that exercises one of them passes with the other deleted. `a:srcRect` admits a
/// small negative (an outset), so the floor is not zero — `CROP_MIN`/`CROP_MAX`
/// are asserted from the model's own constants so this cannot drift from them.
#[test]
fn an_out_of_range_source_rect_is_clamped_into_the_models_domain() {
    let clamped = open(&deck_replacing(
        SLIDE_TWO,
        PICTURE_BLIP_FILL,
        r#"<p:blipFill><a:blip r:embed="rIdImage"/><a:srcRect l="-999999" t="1" r="999999" b="2"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>"#,
    ));
    assert_eq!(
        picture(&clamped).crop,
        Some(CropRect {
            left: CROP_MIN,
            top: 1,
            right: CROP_MAX,
            bottom: 2,
        }),
        "clamped at both ends, with the in-range edges untouched"
    );
}

/// An external `a:hlinkClick` on a picture's `p:cNvPr` arrives as the URL the
/// relationship states, with its screen tip.
///
/// # Why the relationship part is perturbed too
///
/// `a:hlinkClick` carries an `r:id` and nothing else; the URL lives in the
/// slide's own `_rels`. A guard that injected only the element would be asserting
/// against a dangling id, which is the *other* case — and it would pass against a
/// reader that resolved every link to the same wrong thing.
#[test]
fn a_linked_picture_keeps_the_url_its_relationship_states() {
    let plain = open(&deck::deck());
    assert_eq!(
        picture(&plain).hyperlink,
        None,
        "the shared fixture's picture is unlinked, or this guard cannot tell a read \
         link from a pre-existing one"
    );

    let linked = open(&deck_replacing_two(
        (
            SLIDE_TWO,
            PICTURE_CNVPR,
            r#"<p:cNvPr id="4" name="Logo" descr="The company mark"><a:hlinkClick xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rIdLink" tooltip="Open the handbook"/></p:cNvPr>"#,
        ),
        (
            SLIDE_TWO_RELS,
            "</Relationships>",
            r#"<Relationship Id="rIdLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.invalid/handbook" TargetMode="External"/></Relationships>"#,
        ),
    ));
    let linked = &linked;
    let hyperlink = picture(linked)
        .hyperlink
        .expect("the picture states an a:hlinkClick");
    assert_eq!(
        hyperlink.target,
        HyperlinkTarget::External(casual_doc_model::v1::ExternalTarget {
            url: "https://example.invalid/handbook".to_owned(),
            // No `anchor`: `a:hlinkClick` has no `@anchor` of its own, unlike
            // `w:hyperlink`, so a fragment is part of the relationship target.
            anchor: None,
        }),
        "the relationship's @Target, verbatim"
    );
    assert_eq!(
        hyperlink.tooltip.as_deref(),
        Some("Open the handbook"),
        "@tooltip is the screen tip and is visible to a reader"
    );
    assert!(
        !features(linked).contains(&"hlinkClick"),
        "a link that is READ must not also be reported as lost: {:?}",
        features(linked)
    );
}

/// `<a:hlinkClick r:id=""/>` states NO link, and must not be reported as a lost
/// one.
///
/// This is what PowerPoint writes where a link was removed, and it is common
/// enough that treating it as an unresolvable id would put a false finding in the
/// report for a large share of real decks — the generous-direction half of the
/// loss-reporter defect `fc3ec556` fixed. Differential against the populated-id
/// case below, which MUST report.
#[test]
fn an_empty_relationship_id_is_no_link_rather_than_a_lost_one() {
    let empty = open(&deck_replacing(
        SLIDE_TWO,
        PICTURE_CNVPR,
        r#"<p:cNvPr id="4" name="Logo" descr="The company mark"><a:hlinkClick xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id=""/></p:cNvPr>"#,
    ));
    assert_eq!(
        picture(&empty).hyperlink,
        None,
        "an empty id names no relationship, so there is no link to carry"
    );
    assert!(
        !features(&empty).contains(&"hlinkClick"),
        "and nothing was lost, so nothing may be reported: {:?}",
        features(&empty)
    );

    // The same element with a POPULATED id that resolves to nothing. The deck
    // said there was a link and the model holds none, so this one IS a loss.
    let dangling = open(&deck_replacing(
        SLIDE_TWO,
        PICTURE_CNVPR,
        r#"<p:cNvPr id="4" name="Logo" descr="The company mark"><a:hlinkClick xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rIdNoSuchThing"/></p:cNvPr>"#,
    ));
    assert_eq!(picture(&dangling).hyperlink, None);
    assert!(
        features(&dangling).contains(&"hlinkClick"),
        "a stated link that resolves to nothing is loss, and the report must say so: \
         {:?}",
        features(&dangling)
    );
}

/// A slide JUMP is reported rather than modelled as a bookmark.
///
/// `action="ppaction://hlinksldjump"` with an `r:id` naming a slide part is how
/// PowerPoint spells "go to slide 1". The model's only internal target is a
/// document bookmark, so writing the slide's part name into
/// `InternalTarget::anchor` would make the model state an anchor the deck does not
/// contain — a plausible-looking wrong answer, which is worse than the reported
/// loss.
#[test]
fn a_slide_jump_is_reported_rather_than_modelled_as_a_bookmark() {
    let jump = open(&deck_replacing_two(
        (
            SLIDE_TWO,
            PICTURE_CNVPR,
            r#"<p:cNvPr id="4" name="Logo" descr="The company mark"><a:hlinkClick xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rIdJump" action="ppaction://hlinksldjump"/></p:cNvPr>"#,
        ),
        (
            SLIDE_TWO_RELS,
            "</Relationships>",
            r#"<Relationship Id="rIdJump" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slide1.xml"/></Relationships>"#,
        ),
    ));
    let jump = &jump;
    assert_eq!(
        picture(jump).hyperlink,
        None,
        "an in-package target is not a URL and not a bookmark"
    );
    assert!(
        features(jump).contains(&"hlinkClick"),
        "and the loss must be named: {:?}",
        features(jump)
    );
}

/// A connector that binds neither end loses nothing, and the report must say
/// nothing about it.
///
/// # What this replaces
///
/// Every `p:cxnSp` used to be reported `degraded` on the stated ground that its
/// start/end bindings have no field. Most connectors bind nothing — they are drawn
/// lines — so the finding fired on markup that loses nothing, which is the
/// generous-direction half of the same defect as the empty `r:id` above. The
/// fixture's "Invisible Rule" is exactly that connector.
#[test]
fn an_unbound_connector_is_not_a_loss() {
    let plain = open(&deck::deck());
    let found = features(&plain);
    for quiet in ["cxnSp", "stCxn", "endCxn"] {
        assert!(
            !found.contains(&quiet),
            "the fixture's connector binds nothing, so {quiet} would be a finding on \
             markup that lost nothing: {found:?}"
        );
    }
    // And the connector really did arrive, or the silence above is the silence of
    // a shape that never imported.
    assert!(
        plain
            .presentation
            .slides()
            .get(2)
            .expect("the fixture has a third slide")
            .shapes
            .children
            .iter()
            .any(|node| node.name.as_deref() == Some("Invisible Rule")),
        "the unbound connector must be in the model for this guard to mean anything"
    );
}

/// A connector that DOES bind its ends is reported once per end, by the name of
/// the end.
///
/// Per END and not per connector: a deck that loses one binding and a deck that
/// loses both are different fidelity facts, and `a:stCxn` and `a:endCxn` are
/// separate elements. The old blanket `degraded(cxnSp)` could say neither.
#[test]
fn a_bound_connector_reports_each_end_it_cannot_follow() {
    let bound = open(&deck_replacing(
        SLIDE_TEN,
        CONNECTOR_CNVCXNSPPR,
        r#"<p:cNvCxnSpPr><a:stCxn id="3" idx="1"/><a:endCxn id="4" idx="3"/></p:cNvCxnSpPr>"#,
    ));
    let found = features(&bound);
    for end in ["stCxn", "endCxn"] {
        assert!(
            found.contains(&end),
            "a bound end the model cannot follow must be reported as {end}: {found:?}"
        );
        assert_eq!(
            bound
                .report
                .entries
                .iter()
                .find(|entry| entry.feature == end)
                .map(|entry| entry.occurrences),
            Some(1),
            "once per binding, and the fixture states one of each"
        );
    }
    assert!(
        !found.contains(&"cxnSp"),
        "and NOT under the wrapper's name, which could not say which end: {found:?}"
    );
}
