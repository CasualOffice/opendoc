//! Folding a second `w:style` definition of the same `w:styleId` into the first.
//!
//! ECMA-376 §17.7.4.17 makes `w:styleId` unique, but real producers break that:
//! a Google Docs export writes `Normal`, `Heading1`…`Heading6`, `Title`,
//! `Subtitle` and `TableNormal` up to four times each, the copies differing only
//! in what each one bothered to say (a thin copy with spacing and fonts, a full
//! copy that adds `w:qFormat`, `w:uiPriority`, `w:outlineLvl` or the table
//! style's `w:tblCellMar`). Word opens such a file and applies the formatting.
//!
//! The importer used to keep the first definition and report every later one as
//! a lost `style`, which put "style ×32" in front of the owner for a document
//! that lost nothing visible, and dropped the only copy of `TableNormal` that
//! carried cell margins (HF-401).
//!
//! **Rule.** A later definition of the same kind *continues* the first one, as
//! a repeated element inside one definition would. Scalars (name, `basedOn`,
//! priority…) fill what the first leaves unset; flags assert if either copy
//! asserts them; the formatting (`w:pPr`, `w:rPr`, `w:tblPr`, `w:trPr`,
//! `w:tcPr`, `w:tblStylePr`) is the union of the copies. Only a genuine
//! disagreement is a loss — the copies setting one property to different
//! values — and only then is the later copy reported (once per disagreeing
//! copy), with the first copy standing as it always did. Style names compare
//! without regard to ASCII case, as Word's style names do (`normal` and
//! `Normal` name one style).
//!
//! **Mechanism** (`styles.rs` `absorb_duplicate`): the copies' source markup is
//! read as ONE definition by the ordinary reader, in both orders. The two
//! readings agree exactly when no property is set differently by the copies,
//! and then that reading is the union. Reading the markup keeps the rule total
//! as the model grows — a property added to `ParagraphProperties` tomorrow is
//! folded and compared without anyone listing it here — and it costs nothing
//! in the wasm module, where a generic comparison of the model values would
//! (serde: +68 KB to compare, +298 KB to merge, measured against
//! `wasm_size.test.mjs`'s budget). It also judges by meaning, not spelling: a
//! `w:lineRule` with no `w:line`, which the model does not carry, is not a
//! disagreement. This is the known **normalization** pattern (one canonical
//! record per key). Cost: `O(size of the copies)` per duplicate, bounded by
//! `MAX_FOLDED_COPIES`.

/// Folds a scalar: unset takes the later value, equal agrees, different
/// disagrees (the first stands).
pub(super) fn fold_scalar<T: PartialEq>(first: &mut Option<T>, later: Option<T>) -> bool {
    match (first.as_ref(), later) {
        (_, None) => false,
        (None, later) => {
            *first = later;
            false
        }
        (Some(current), Some(later)) => *current != later,
    }
}

/// A style name, compared as Word compares style names: without ASCII case.
pub(super) fn fold_name(first: &mut Option<String>, later: Option<String>) -> bool {
    match (first.as_ref(), later) {
        (_, None) => false,
        (None, later) => {
            *first = later;
            false
        }
        (Some(current), Some(later)) => !current.eq_ignore_ascii_case(&later),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scalar_fills_a_gap_agrees_or_disagrees() {
        let mut priority = None;
        assert!(!fold_scalar(&mut priority, Some(9)));
        assert_eq!(priority, Some(9));
        assert!(!fold_scalar(&mut priority, Some(9)));
        assert!(fold_scalar(&mut priority, Some(10)));
        assert_eq!(priority, Some(9), "the first stands");
    }

    #[test]
    fn style_names_fold_without_ascii_case() {
        let mut name = Some("normal".to_owned());
        assert!(!fold_name(&mut name, Some("Normal".to_owned())));
        assert!(fold_name(&mut name, Some("Normal Table".to_owned())));
    }
}
