# 86 — Typed OMML Math Model and Rendering Design

**Status:** Implemented for the bounded first slice (`P1F-MATH-TYPED-1`).  
**Date:** 2026-08-02  
**Depends on:** docs 14, 18, 43, 55, and the existing `P1F-C1` / `P1F-INLINE-FLOOR`
work.

## 1. Decision

The v1 model will gain an optional, bounded semantic projection of retained
Office Math Markup Language (OMML). The original OMML subtree remains the
round-trip authority. Import derives the projection namespace-safely; layout
uses it when supported and keeps the existing explicit text fallback otherwise.

The first supported subset is deliberately small and compositional:

- rows and math text runs;
- fractions;
- subscript, superscript, and combined sub/superscript;
- radicals with an optional degree;
- delimiters with nested content.

This is not a claim of complete OMML support. N-ary operators, matrices,
equation arrays, accents, limits, function properties, and advanced control
properties remain preserved but render through the visible fallback until they
receive typed support.

## 2. Model contract

`InlineNode::Math` keeps its stable node id, retained `omml`, and best-effort
plain `text`. It adds an optional `expression` tree. The field is additive and
serde-defaulted, so older serialized documents remain readable and callers that
construct opaque Math nodes remain valid.

The expression is a recursive enum with row, text, fraction, script, radical,
and delimiter variants. Validation is fail-closed and bounded by:

- maximum expression depth;
- maximum semantic node count;
- maximum text size, in addition to the existing retained-OMML byte bound;
- required operands for fraction/radical/script constructs.

The semantic tree is a projection, not a second source of truth. In this slice,
export writes the retained OMML verbatim. Editing or synthesizing OMML from the
projection is explicitly out of scope.

## 3. Import and preservation contract

The existing namespace guard continues to capture an `m:oMath` or
`m:oMathPara` subtree without exposing inner `m:r` / `m:t` elements to WordprocessingML
run parsing. After bounded capture, a second namespace-aware parser derives the
semantic tree.

Malformed, over-limit, or unsupported semantic structures do not erase the raw
subtree. Their projection is absent, their `m:t` fallback remains available,
and export still emits the retained OMML. There is no silent data loss.

## 4. Layout and paint contract

Math is an atomic in-flow object at the paragraph line-breaking layer. It does
not become bracket text and it does not force a standalone line merely because
it is an equation.

1. A deterministic math layout pass recursively computes a box from the typed
   expression using the document's existing text shaper for leaf glyphs.
2. The paragraph shaper receives that box at the Math node's exact logical byte
   boundary, alongside images and float exclusions.
3. Parley owns wrapping, bidi line placement, and the final inline origin.
4. The positioned math box carries ordinary `GlyphRun`s and `InlineRule`s.
5. Composition emits the existing glyph and rectangle paint primitives; the
   renderer requires no OMML-specific backend API.

Fractions stack numerator and denominator around a deterministic rule. Scripts
use reduced-size leaf shaping and baseline offsets. Radicals use a shaped radical
glyph plus an overbar. Delimiters use shaped delimiter glyphs around nested
content. Metrics are integer twips after rounding and are bounded before
arithmetic, preserving deterministic layout and resource limits.

Selection and hit testing treat the whole equation as one atomic inline object
in this slice. Caret navigation inside an equation and semantic math editing are
later work.

## 5. Fallback and compatibility behavior

If `expression` is absent or math box construction fails safely, layout uses the
existing visible `[fallback]` / `[equation]` run. This makes partial support
explicit and prevents content from disappearing.

Public fidelity reporting changes only after the vertical slice is verified:
Math remains **partial** for modeling and becomes **partial** for rendering,
with the supported subset named. It must not be described as full OMML
typesetting.

## 6. Verification gates

The implementation must include:

- model serde and validation bounds tests;
- import tests for each supported construct, namespaces, unsupported content,
  malformed content, depth, and size limits;
- unchanged raw-OMML semantic export tests;
- layout tests for inline placement, nesting, wrapping, deterministic metrics,
  and fallback;
- composition/display-list assertions for glyphs and fraction/radical rules;
- formatting, focused crate tests, workspace tests, and strict Clippy before the
  slice is reported complete.

## 7. Explicit non-goals

- complete ECMA-376 OMML coverage;
- MathML or LaTeX import/export;
- equation editing, internal caret navigation, or partial selection;
- replacing retained OMML with regenerated XML;
- platform math-font dependence or browser-DOM layout.
