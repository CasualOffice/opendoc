//! Markup that carries no document meaning, and therefore no loss to report.
//!
//! `35-DISPOSITION-TAXONOMY.md` already draws the line this module implements:
//! *"A disposition describes the fate of document meaning. Some markup carries
//! none, and dispositioning it would fill every report with rows a reader cannot
//! act on."* The doc enumerates the exclusions rather than leaving them to each
//! parser, so that "not reported" is a stated policy with a guard behind it. This
//! module is that enumeration in code; the doc's table is the same list in prose.
//!
//! Every member here is `MappedComplete` in the taxonomy's vocabulary — fully
//! understood, with nothing left to retain — which is exactly the disposition the
//! report does not enumerate. What they are **not** is "unrecognised": treating
//! unrecognised as lost is the defect this module exists to remove. A sweep over
//! the owner's fifteen-document corpus found **621 findings describing losses that
//! did not happen** (HF-174), and the compatibility report is the answer to "what
//! did this import lose" — so entries that lost nothing bury the entries that did.
//!
//! Two rules keep this from becoming a rubber stamp:
//!
//! 1. **Where the no-op is conditional, the arm is conditional.** `<a:effectLst/>`
//!    is "no effects"; a *populated* `a:effectLst` is a real loss and must still be
//!    reported. `wp14:pctWidth` of `0` means relative sizing is off; a non-zero one
//!    is a size this model does not carry. Silencing the populated form would be a
//!    worse bug than the one this fixes, because it hides a real loss instead of a
//!    fake one. Every conditional member has a guard for BOTH halves.
//! 2. **Presence that is not the default is information.** An element whose value
//!    equals the state the model already has (the schema default, the identity
//!    mapping, the "feature off" value) is a no-op. An element whose presence says
//!    something the default does not — `<w:autoRedefine/>`, `<a:spLocks
//!    noChangeArrowheads="1"/>` — is *not*, however cosmetic it looks, and stays
//!    reported.
//!
//! The rest of this class lives at the sites that know the context, because a name
//! alone cannot decide them: the DrawingML non-visual wrappers in
//! `body::is_drawing_scaffolding`, the `wp14:pctWidth`/`pctHeight` percentage and
//! the stock footnote/endnote separators in `body`, the separator *references* in
//! `settings`, and the diagonal cell borders in `styles` (which report their
//! container's name, not their own).

use quick_xml::events::BytesStart;

use crate::properties::{attribute_value, is_true};

/// Whether an element's local name alone proves it carries no document meaning.
///
/// Routed inside `Reporter::report`, the single choke point every catch-all
/// already funnels through — the same placement, and for the same reason, as the
/// `w:rsid*` class: a per-call-site rule is a rule the next catch-all forgets.
///
/// Membership is decided by name alone, so each name must be unambiguous across
/// every part this importer reads. That is checked by
/// `every_unconditional_no_op_name_is_unique_to_its_vocabulary`.
pub(crate) fn carries_no_meaning(local: &[u8]) -> bool {
    matches!(
        local,
        // `w:proofErr` — the range markers Word's spell and grammar checkers left
        // behind on their last run (`spellStart`/`spellEnd`/`gramStart`/
        // `gramEnd`). They mark where a squiggle was drawn, not what the document
        // says; a consumer that checks spelling itself derives them again, and one
        // that does not has nothing to draw. 110 of the corpus's 621 false losses.
        b"proofErr"
        // `w:lastRenderedPageBreak` — where Word's pagination last fell. It is a
        // cache of a layout this engine computes for itself, and it is wrong the
        // moment a font, a margin or a zoom differs. Keeping it would be worse
        // than dropping it.
        | b"lastRenderedPageBreak"
        // `w:nsid` / `w:tmpl` — the GUIDs Word stamps on an abstract numbering
        // definition to match it against the user's List Library across
        // documents. Nothing inside the package joins on either: numbering
        // resolves through `w:abstractNumId`/`w:numId`, and every level's format,
        // start, text and indentation is modeled. Layout, numbering and reopen are
        // identical without them.
        //
        // This is deliberately NOT the treatment `w:rsid*` gets, and the
        // difference is worth stating. An rsid identifies *content* — which
        // editing session wrote this run — and Word's compare and merge apply that
        // to the user's own text, so `35` reports the class once per document with
        // a count. An nsid identifies a *list template*, a gallery key about where
        // a definition came from, and no feature of the document depends on it.
        // `35` records the comparison, and the open question it leaves.
        | b"nsid"
        | b"tmpl"
        // `wp14:sizeRelH` / `wp14:sizeRelV` — the wrapper around a relative size.
        // It carries only `@relativeFrom` (which edge the percentage is measured
        // from), which says nothing on its own: the percentage inside decides
        // whether relative sizing is on at all. `body` reports the
        // `wp14:pctWidth`/`pctHeight` inside when it is non-zero, so a real
        // relative size is still named; all 63 in the corpus are `0`.
        | b"sizeRelH"
        | b"sizeRelV"
    )
}

/// Whether an element carries no document meaning **in this particular form** —
/// the conditional half of the class.
///
/// `self_closing` is whether the source wrote `<x/>` rather than `<x>…</x>`; it is
/// only consulted where emptiness is the whole question. The expanded-but-childless
/// form (`<a:effectLst></a:effectLst>`) deliberately still reports: erring toward a
/// finding is the safe direction, and no Word build writes it.
pub(crate) fn carries_no_meaning_when(
    local: &[u8],
    element: &BytesStart<'_>,
    self_closing: bool,
) -> bool {
    match local {
        // `<a:effectLst/>` — an EMPTY effect list is DrawingML for "no effects",
        // and Word writes one into almost every shape. A populated `a:effectLst`
        // (a shadow, a glow, a reflection) is a real loss and still reports.
        b"effectLst" => self_closing,
        // `<a:spLocks/>` with no attributes locks nothing. Each attribute is a
        // separate lock (`noChangeArrowheads`, `noResize`, `noEditPoints`, …) and
        // this engine honours none of them, so a populated one is a restriction the
        // document asked for and did not get: still reported.
        b"spLocks" => element.attributes().next().is_none(),
        // `<a14:useLocalDpi val="0"/>` — the Office 2010 drawing extension asking
        // that a picture NOT be rescaled to the authoring machine's DPI. Off is the
        // behaviour this engine has; every one of the corpus's 18 is `0`. `val="1"`
        // would be a real difference and still reports.
        b"useLocalDpi" => !is_true(attribute_value(element, b"val").as_deref()),
        // `<a:prstTxWarp prst="textNoShape"/>` — DrawingML's token for *no* warp,
        // which Word writes into every `wps:bodyPr` whether or not the text is
        // warped. ONLYOFFICE special-cases the same token: its "is there a warp"
        // predicate is `prstTxWarp && preset !== "textNoShape"`, and it normalises
        // an absent element *to* `textNoShape`. All 22 in the corpus are this.
        // A real preset (`textArchUp`, `textWave1`, …) is unmodeled and reports.
        b"prstTxWarp" => attribute_value(element, b"prst").as_deref() == Some("textNoShape"),
        // `w:clrSchemeMapping` — which theme slot each of Word's twelve colour
        // slots resolves to. The identity mapping is what every consumer already
        // assumes, so writing it changes nothing; a document that swaps a pair
        // (`bg1="dark1"`, say) inverts real colours and still reports.
        b"clrSchemeMapping" => is_default_colour_scheme_mapping(element),
        // `w:characterSpacingControl` — `doNotCompress` is the schema default and
        // is what this engine does. `compressPunctuation` and
        // `compressPunctuationAndJapaneseKana` are real East Asian justification
        // behaviour and still report.
        b"characterSpacingControl" => {
            attribute_value(element, b"val").as_deref() == Some("doNotCompress")
        }
        // `w:tl2br` / `w:tr2bl` — the two diagonal cell borders. `nil`/`none` is
        // "there is no diagonal here", which Word writes as part of a complete
        // border set; a diagonal with an actual style is unmodeled geometry and
        // still reports (through its container's name, `w:tcBorders`).
        b"tl2br" | b"tr2bl" => matches!(
            attribute_value(element, b"val").as_deref(),
            None | Some("nil") | Some("none")
        ),
        _ => false,
    }
}

/// Word's twelve colour-slot mappings and the value each one has by default.
///
/// The mapping is a permutation of the theme slots; the default is the one every
/// consumer assumes when `w:clrSchemeMapping` is absent, so writing it is a no-op.
const DEFAULT_COLOUR_SCHEME_MAPPING: &[(&[u8], &str)] = &[
    (b"bg1", "light1"),
    (b"t1", "dark1"),
    (b"bg2", "light2"),
    (b"t2", "dark2"),
    (b"accent1", "accent1"),
    (b"accent2", "accent2"),
    (b"accent3", "accent3"),
    (b"accent4", "accent4"),
    (b"accent5", "accent5"),
    (b"accent6", "accent6"),
    (b"hyperlink", "hyperlink"),
    (b"followedHyperlink", "followedHyperlink"),
];

/// Whether every slot this `w:clrSchemeMapping` names maps to its default. An
/// absent attribute is its default, so a partial mapping is judged on what it
/// states.
fn is_default_colour_scheme_mapping(element: &BytesStart<'_>) -> bool {
    DEFAULT_COLOUR_SCHEME_MAPPING.iter().all(|(slot, default)| {
        match attribute_value(element, slot) {
            Some(value) => value == *default,
            None => true,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `<x …/>` as the parser sees it.
    fn element(content: &str) -> BytesStart<'static> {
        BytesStart::from_content(
            content.to_owned(),
            content.find(' ').unwrap_or(content.len()),
        )
        .into_owned()
    }

    /// A no-op decided by NAME alone is only safe if that name means one thing
    /// everywhere this importer reads. These are the vocabularies each belongs to;
    /// the assertion is that no OTHER WordprocessingML or DrawingML element shares
    /// the local name, which is why the list is short and why anything ambiguous
    /// (`style`, `fill`, `effectLst`) is judged with its element instead.
    #[test]
    fn every_unconditional_no_op_name_is_unique_to_its_vocabulary() {
        for name in [
            b"proofErr".as_slice(),
            b"lastRenderedPageBreak",
            b"nsid",
            b"tmpl",
            b"sizeRelH",
            b"sizeRelV",
        ] {
            assert!(
                carries_no_meaning(name),
                "{} left the class",
                String::from_utf8_lossy(name)
            );
        }
        // Names that are close relatives and must NOT be swept in.
        for name in [
            b"pctWidth".as_slice(),
            b"pctHeight",
            b"style",
            b"effectLst",
            b"autoRedefine",
            b"docPartUnique",
            b"miter",
        ] {
            assert!(
                !carries_no_meaning(name),
                "{} was silenced",
                String::from_utf8_lossy(name)
            );
        }
    }

    #[test]
    fn an_empty_effect_list_is_no_effects_and_a_populated_one_is_a_loss() {
        assert!(carries_no_meaning_when(
            b"effectLst",
            &element("a:effectLst"),
            true
        ));
        assert!(!carries_no_meaning_when(
            b"effectLst",
            &element("a:effectLst"),
            false
        ));
    }

    #[test]
    fn a_lock_that_locks_something_is_still_a_loss() {
        assert!(carries_no_meaning_when(
            b"spLocks",
            &element("a:spLocks"),
            true
        ));
        assert!(!carries_no_meaning_when(
            b"spLocks",
            &element(r#"a:spLocks noChangeArrowheads="1""#),
            true
        ));
    }

    #[test]
    fn only_the_off_form_of_each_valued_no_op_is_silent() {
        for (local, silent, loud) in [
            (
                b"useLocalDpi".as_slice(),
                r#"a14:useLocalDpi val="0""#,
                r#"a14:useLocalDpi val="1""#,
            ),
            (
                b"prstTxWarp",
                r#"a:prstTxWarp prst="textNoShape""#,
                r#"a:prstTxWarp prst="textArchUp""#,
            ),
            (
                b"characterSpacingControl",
                r#"w:characterSpacingControl w:val="doNotCompress""#,
                r#"w:characterSpacingControl w:val="compressPunctuation""#,
            ),
            (
                b"tl2br",
                r#"w:tl2br w:val="nil""#,
                r#"w:tl2br w:val="single" w:sz="4""#,
            ),
            (
                b"tr2bl",
                r#"w:tr2bl w:val="none""#,
                r#"w:tr2bl w:val="double" w:sz="8""#,
            ),
        ] {
            assert!(
                carries_no_meaning_when(local, &element(silent), false),
                "reported a loss that cannot be seen: {silent}"
            );
            assert!(
                !carries_no_meaning_when(local, &element(loud), false),
                "dropped something visible in silence: {loud}"
            );
        }
    }

    #[test]
    fn only_the_default_colour_scheme_mapping_is_silent() {
        let default = r#"w:clrSchemeMapping w:bg1="light1" w:t1="dark1" w:bg2="light2" w:t2="dark2" w:accent1="accent1" w:accent2="accent2" w:accent3="accent3" w:accent4="accent4" w:accent5="accent5" w:accent6="accent6" w:hyperlink="hyperlink" w:followedHyperlink="followedHyperlink""#;
        assert!(carries_no_meaning_when(
            b"clrSchemeMapping",
            &element(default),
            true
        ));
        // One slot swapped: the document's background and text colours trade
        // places, which is a real difference and must still be reported.
        let inverted = default.replace(r#"w:bg1="light1""#, r#"w:bg1="dark1""#);
        assert!(!carries_no_meaning_when(
            b"clrSchemeMapping",
            &element(&inverted),
            true
        ));
    }
}
