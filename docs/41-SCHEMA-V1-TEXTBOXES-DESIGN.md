# Normalized Schema v1: Text Boxes and Alternate Content Design

**Status:** Accepted — 2026-07-25 (repository owner directive: complete the model,
skip nothing)
**Tracker:** P1A-019 (schema v1 semantic extension), text-box slice
**Decision basis:** ADR-027, schema v1 (`38-…`), tables (`39-…`), fields (`40-…`),
importer no-skip audit (multi-agent, 16 confirmed findings)

## Why

A multi-agent audit of the importer confirmed that text boxes are a **blocker**:
a `w:txbxContent` (in a DrawingML `wps:txbx` or a legacy VML `v:textbox`) contains
block content (`w:p`), but the flat body parser cannot open the inner paragraph
(the enclosing run is still open), so the box's inner `</w:p>` instead fires
`finish_paragraph()` on the **enclosing** body paragraph. The result: the real
paragraph is truncated early, the boxed text is mis-captured into the wrong
paragraph, and `drawing_depth` is reset to 0 so the enclosing drawing's **image is
silently dropped**. This corrupts even ordinary main-body documents that contain a
text box. The audit also found `mc:AlternateContent` is walked in **both** its
`mc:Choice` and `mc:Fallback` branches, duplicating content.

This slice models text boxes as first-class content and selects a single
alternate-content branch, so nothing is dropped, duplicated, or mis-attributed.

## Model

A text box is inline-anchored (or floating) but contains **block** content. Model
it as an additive inline node holding nested blocks, exactly like a table cell:

```text
InlineNode {
  Run | Tab | Break | Drawing | Hyperlink | Field   // unchanged
  TextBox(TextBox)                                    // new
}

TextBox {
  id: NodeId,
  blocks: Vec<BlockNode>,   // non-empty; paragraphs and nested tables
}
```

`TextBox.blocks` reuses the recursive block model already validated for table
cells (`validate_block` / `record_block_ids` / `accumulate_block_limits`), so
paragraphs, runs, tables, and their bounds all recurse for free. A text box may
contain a table, and a table cell may contain a text box; both are bounded by the
existing table-depth bound plus a new text-box nesting bound.

### Nesting and wrapper interaction

- `MAX_TEXTBOX_DEPTH = 8` bounds text-box-in-text-box nesting (validation rejects
  deeper; import caps identically).
- A `TextBox` is a block-container, not an inline wrapper, so the `in_wrapper`
  rule (hyperlink/field) does not forbid it. A text box may appear inside a
  hyperlink or field's inline run stream; its own blocks are validated
  independently. A hyperlink/field inside a text box's paragraph is a normal
  inline in that nested paragraph (the wrapper rule applies per inline sequence).

## Strict validation (additive)

`validate_inlines` gains a `TextBox` arm: reject empty `blocks`
(`EmptyTextBox(id)`); validate each block with `validate_block`, incrementing a
text-box-depth counter and rejecting past `MAX_TEXTBOX_DEPTH`
(`TextBoxNestingTooDeep(id)`); a text box is a hard run-merge boundary (resets
adjacent-run tracking). Id-uniqueness and snapshot block/text limits recurse into
`TextBox.blocks`. New `ModelError`: `EmptyTextBox(NodeId)`,
`TextBoxNestingTooDeep(NodeId)`.

## Import

The body parser gains a **block-sink stack** so a text box's inner paragraphs are
collected into the box instead of corrupting the enclosing paragraph:

- Entering `w:txbxContent` (DrawingML or VML) suspends the current paragraph/run
  context and pushes a text-box block sink. Inner `w:p`/`w:r`/`w:t`/tables build
  normally into that sink (the inner `</w:p>` finishes the *inner* paragraph, not
  the outer one). Leaving `w:txbxContent` pops the sink, builds a `TextBox` inline
  (bounded; empty box → reported and dropped), and routes it into the suspended
  inline stream via the existing segment router.
- Because the inner context is suspended and restored, the enclosing drawing's
  `drawing_depth`/`blipFill`/`pending_embed` are preserved, so the drawing's own
  image still commits — fixing the silent image drop.
- `mc:AlternateContent`: descend only into the **first** `mc:Choice` (the
  producer's preferred representation); `mc:Fallback` (and any later `mc:Choice`)
  is skipped and reported, so content is neither duplicated nor lost.

Everything a text box carries that is not modeled (shape geometry, wrapping, fill)
continues to be reported, and in Retention mode preserved.

## Round-trip and fidelity

Retention is unchanged (byte-exact). The fidelity harness recurses `TextBox.blocks`
when extracting text, so boxed text counts toward the text proxy — and, critically,
the enclosing paragraph is no longer truncated, so surrounding text is correct too.

## Out of scope (still reported + Retention-preserved)

Shape geometry/anchoring/wrapping/fill; linked text boxes (`wps:linkedTxbx`);
converting VML pictures to `Drawing` (separate slice); extra-part parsing
(header/footer/footnote — separate slice); `w:ruby` ordering (separate slice).
