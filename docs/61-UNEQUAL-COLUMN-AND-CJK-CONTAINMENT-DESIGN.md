# 61 — Unequal-Column and CJK Containment Follow-up

**Status:** Implemented as the post-PR-170 corpus increment.
**Date:** 2026-07-27
**Baseline:** `main@ec7a052` (PR #170 merged with all CI gates green).
**Scope:** Core DOCX import-consumer behavior, layout, pagination, and native
rendering. The browser demo is explicitly out of scope.

## Problem statement

The Chinese SDS remains visibly wrong after the first containment pass:

1. selected paragraphs use more vertical space than the authored sub-single
   `auto` line spacing;
2. text can escape the right edge of a narrow unequal column;
3. the user's Word reference shows a header VML text box on two visible lines,
   while OpenDoc and the available LibreOffice PDF both produce three;
4. pagination remains 18 pages versus the 16-page LibreOffice reference.

These are geometry and shaping defects, not reasons to tune global font sizes,
page margins, or paragraph spacing.

## Corpus evidence

The SDS uses repeated continuous sections with explicit unequal columns such as:

```xml
<w:cols w:num="2" w:equalWidth="0">
  <w:col w:w="3163" w:space="40"/>
  <w:col w:w="6447"/>
</w:cols>
```

The content contains authored `w:br w:type="column"` boundaries between the
narrow label and wide value streams. The current driver shapes the complete
section once at `ColumnLayout::flow_width()`, which selects the widest column.
The paginator then places some of those wide-shaped fragments in the 3163-twip
column. Placement width and line-break width therefore disagree by construction.

The header's first VML text box has an authored width of 157.1 points, explicit
zero insets, and two paragraphs. The user's Word reference fits it into two
visible lines. LibreOffice is not an oracle for this detail: its PDF also wraps
the second paragraph and produces three visible lines.

Five tabbed SDS form rows were measured extending about 1759–4311 twips beyond
their intended line measure. They contain more than one label/value group in a
single paragraph, so the previous trailing-value-only tab wrapper never bounded
the overflowing intermediate segment.

PR #170 also changed CJK fallback normalization so a sub-single `auto` line
multiple cannot produce a line box below one em. This prevented overpaint, but
it overrides the source spacing on affected paragraphs.

## Design

### 1. Preserve forced-break identity

Line layout must retain whether a forced break is a page break or a column break.
The single-column paginator may continue treating either as a page transition.
The column paginator must advance `column -> next column -> next page` for a
column break and must start a new page for a page break.

The break kind is line metadata, not inferred later from document text.

### 2. Shape unequal columns at their physical widths

For an unequal-column section, the driver builds a width-specific galley for
each physical column. The column paginator selects the fragment shaped for the
active column. At an explicit break inside a paragraph it resumes from the
matching model offset in the new column's version of that paragraph.

Equal-width sections retain one shared galley. If width-specific fragment
topology ever differs, the implementation must fall back deterministically
rather than indexing mismatched fragments.

This increment guarantees correct-width shaping at block boundaries and
explicit forced-break boundaries. A paragraph split by ordinary column overflow
is moved whole when its alternate-width layout fits the next column. An
oversized paragraph may retain its starting-column line layout until a future
resumable shaping cursor is introduced; that residual is explicit rather than
presented as general unequal-column containment.

### 3. Honor authored dense `auto` line spacing

CJK fallback normalization continues to replace host-face vertical metrics with
run-font-relative metrics. The blanket one-em floor is removed for authored
sub-single `auto` spacing. `lineRule="exact"` retains its explicit clip and
baseline containment. No global line-height or font-size adjustment is allowed.

### 4. Normalize fallback advances before accepting a wrap

Post-shape one-em CJK advance normalization must not disagree with the line
break decision. When fallback metrics leave exactly one CJK glyph on a second
line, the two lines may be compacted only when the combined advance exceeds the
authored measure by no more than 3%. The correction scales positioned advances
and paint-time glyph outlines together, and applies only to plain start-aligned
LTR fallback text without inline objects or float exclusions. The unmodified
layout is the deterministic fallback outside that envelope.

The SDS header regression is the acceptance case: explicit zero insets and the
157.1-point box remain unchanged, while its two paragraphs occupy two lines.

### 5. Bound every tab segment

Every overflowing tab segment is shaped with wrapping rather than only the final
segment. A final value retains its hanging tab column on continuation lines. An
intermediate segment uses its tab position for the first line, resumes at the
paragraph start on continuation lines, and allows later tabs to establish the
next logical label/value row. This matches the SDS's serialized multi-row form
paragraphs without document-specific text or coordinates.

## Invariants

- Width-specific fragments selected at block and forced-break boundaries match
  the physical active-column measure.
- Page breaks and column breaks are not conflated.
- Exact lines remain clipped and baseline-contained.
- Authored sub-single `auto` spacing is not raised to one em.
- A wrap retry is accepted only if normalized glyph advances stay within the
  original line measure.
- Every ordinary tab segment remains within the paragraph measure; a
  margin-relative positional tab remains bounded by its explicit margin box.
- Restricted corpus files and derived images remain outside the repository.
- No browser-demo changes are part of this slice.

## Verification

Synthetic coverage includes:

- unequal `3163/6447` columns with a forced column break;
- page-vs-column break behavior;
- a paragraph resumed at the same model offset in the new column width;
- sub-single CJK `auto` spacing;
- bounded fallback-advance wrap retry;
- generalized intermediate and trailing tab-segment wrapping;
- explicit-zero-inset VML header text boxes.

Then render the exact local SDS probe and inspect the header plus every page that
previously escaped its right-side box. Page count is evidence, not the sole
acceptance condition. The standard formatting, workspace test, strict Clippy,
Rustdoc, wasm32, MSRV, and benchmark gates remain required before a PR.

## Implementation result

- All five severe measured tab-row escapes are contained; the affected lines no
  longer cross the right canvas edge.
- The SDS renders 17 pages, down from 18; LibreOffice remains at 16.
- The authored dense `auto` spacing is restored and the user's header reference
  occupies two visible lines without changing the VML box geometry.
- Sample remains 26 pages with intact TOC leaders, separated page numbers, and
  footer; demo remains 8 pages, Class Notes 1, and Medical 3.
- Formatting, strict all-target/all-feature Clippy, all-feature workspace tests
  and doc tests, strict Rustdoc, wasm32, Rust 1.88 MSRV, release benchmark smoke,
  and diff validation pass.
