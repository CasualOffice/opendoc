// SPDX-License-Identifier: Apache-2.0

//! The plain text a paragraph's inlines contribute — the one definition of it.
//!
//! It lived in `casual-doc-layout` (`flow.rs`), which still re-exports it. It moved
//! here so a crate with no layout dependency (the capability registry,
//! `casual-doc-command`, `docs/181`) reads exactly the text the shaper indexes,
//! rather than a second implementation that could drift from it.
//! `casual_doc_edit::inline_text_len` measures the same text and is held equal to
//! it by a test there.

use super::{FormFieldKind, InlineNode, ReviewProjection};

/// The plain text a paragraph's inline nodes contribute, in the **same byte
/// layout** the shaper indexes — so a UTF-8 byte offset from hit-testing
/// (`casual_doc_layout::hittest`, `ModelPos::offset`) slices this string at the caret the
/// user clicked. This is the authoritative source for copy-in-view: it mirrors
/// the text-bearing flow contributions exactly (`Run` text, `Tab` → `\t`,
/// `Symbol` → its code point, and recursion through visible
/// `Hyperlink`/`Revision`/`Sdt` wrappers); every other inline kind contributes
/// no shaped text bytes.
#[must_use]
pub fn node_plain_text(inlines: &[InlineNode]) -> String {
    node_plain_text_with_projection(inlines, ReviewProjection::FinalWithMarkup)
}

/// The plain text contributed by `inlines` under an explicit review projection.
///
/// Runtime editing uses [`ReviewProjection::FinalWithMarkup`]; the parameterized
/// form keeps the Original/Final contract testable without enabling an unsafe
/// view switcher before position mapping and editing policy are implemented.
#[must_use]
pub fn node_plain_text_with_projection(
    inlines: &[InlineNode],
    projection: ReviewProjection,
) -> String {
    let mut out = String::new();
    append_node_plain_text(inlines, projection, &mut out);
    out
}

/// [`node_plain_text_with_projection`] into a caller-owned buffer.
///
/// Public so a caller that asks the same question about every paragraph of a
/// document can reuse one `String` instead of allocating one per paragraph and
/// dropping it immediately — on a 1.3M-paragraph document that allocation was
/// the bulk of the cost of ordering two endpoints (`docs/116` §4.3). It appends;
/// clear the buffer between paragraphs.
pub fn append_node_plain_text(
    inlines: &[InlineNode],
    projection: ReviewProjection,
    out: &mut String,
) {
    // container-set: one paragraph's own text, so it descends `Hyperlink`, `Revision`, `Sdt` and `Field` and deliberately not `TextBox` or `Group`, whose paragraphs have their own offset space (`casual_doc_edit::inline_text_len` counts them as zero too).
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Tab(_) => out.push('\t'),
            // One position, as `casual-doc-edit`'s `inline_text_len` counts it
            // (`docs/109` HF-296): a line break is a character a reader sees.
            InlineNode::Break(_) => out.push('\n'),
            InlineNode::Symbol(symbol) => {
                if let Some(ch) = char::from_u32(symbol.char) {
                    out.push(ch);
                }
            }
            InlineNode::Hyperlink(hyperlink) => {
                append_node_plain_text(&hyperlink.inlines, projection, out);
            }
            InlineNode::Revision(revision) if revision.kind.contributes_to(projection) => {
                append_node_plain_text(&revision.inlines, projection, out);
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => append_node_plain_text(&sdt.inlines, projection, out),
            // A legacy field (`w:fldChar` + `w:instrText`) contributes the
            // text it SHOWS. Dropping it made every legacy form field
            // invisible to everything downstream of plain text at once - the
            // accessibility mirror, table-cell text, and the offsets the
            // checkbox gesture resolves against - while `field_anchor_len`
            // counted that same text, so the two disagreed about how long the
            // paragraph was.
            //
            // A `FORMCHECKBOX` has no content to contribute: layout
            // synthesises its box from the field's state, so the same
            // character is synthesised here rather than reporting an empty
            // field where a reader can plainly see a box.
            InlineNode::Field(field) => match field.form.as_ref().map(|form| &form.kind) {
                Some(FormFieldKind::CheckBox(checkbox)) => {
                    out.push(checkbox.glyph());
                }
                _ => append_node_plain_text(&field.inlines, projection, out),
            },
            _ => {}
        }
    }
}
