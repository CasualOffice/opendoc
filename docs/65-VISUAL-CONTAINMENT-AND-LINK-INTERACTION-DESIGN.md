# Visual Containment and Link Interaction Design

**Status:** Accepted for implementation  
**Owner:** Codex  
**Scope:** `demo.docx` page 5/page 7 fidelity defects, split-table
continuations, hyperlink/TOC interaction, and their regression gates

## Problem statement

The current renderer can produce geometry that paints outside the fragment that
owns it:

- later lines of a shaped multi-line paragraph can be translated twice while
  the paragraph fragment keeps the correct total height;
- paragraph continuations rebase glyph runs but not inline images, text boxes,
  or rules;
- split table-cell fragments budget content without the margins that are
  painted in every cloned continuation cell;
- page/margin-relative side-wrapped drawings are painted after pagination and
  therefore do not exclude body text in following paragraphs;
- `w:framePr` drop caps are not represented, so the large initial is clipped
  and the following paragraph does not wrap around it.

The editor also discards hyperlink identity during layout. As a result, imported
external links and TOC anchors render as ordinary text, cannot be activated, and
the editing command set cannot create, update, or remove a link.

The exact user-supplied `demo.docx` was used as local diagnostic evidence only.
It is not committed because the fixture's redistribution rights are unknown.

## Invariants

1. Every paintable child of a line uses one coordinate system: its `y` origin is
   paragraph-relative before pagination and fragment-relative after slicing.
2. A fragment's measured height contains every child it can paint, including
   cloned table-cell margins.
3. Pagination is deterministic and bounded. Float reflow may iterate only to a
   fixed limit and must have a conservative terminating fallback.
4. Hyperlink activation is a query plus a host policy decision. The document
   runtime never performs navigation or network access.
5. Hyperlink authoring is an undoable document operation; the UI must not mutate
   the model directly.
6. Unsupported framing data is preserved or reported. It is never silently
   discarded.

## Slice A — line and split-table containment

`Line` owns a single translation helper that moves runs, inline images, inline
text boxes, and rules together. The flow stacker applies the incoming paragraph
base exactly once to every already paragraph-relative line, then advances the
cursor by the sum of the line heights. Paragraph slicing uses the same helper to
rebase all child families.

A split cell retains its current cloned-margin representation. Consequently,
each head/tail decision budgets the available row slice after subtracting both
cell margins, and the emitted row-chunk height is the maximum occupied cell
height including those margins. A split that consumes no block reports no
progress and uses the existing oversized-content escape path. This is
deliberately conservative: continuation fragments repeat authored cell margins,
but no child may paint into the next row.

Acceptance:

- a multi-line chunk's children stay within their line bands;
- sliced runs, images, text boxes, and rules receive the same rebase;
- a tall, margined cell with an inline object can split across pages without
  either continuation or the next two rows overlapping.

## Slice B — hyperlink and TOC interaction

The runtime reconstructs a link span for each flattened
`InlineNode::Hyperlink` using the same node-relative UTF-8 byte accounting as
shaping, then intersects those ranges with layout selection rectangles. This
keeps the model authoritative and avoids duplicating target metadata in cached
line fragments. A public `link_at(page, point)` query returns:

- the model range;
- display text range;
- external URI plus optional tooltip, or internal bookmark name;
- the resolved bookmark position when an internal target exists.

Unresolved internal anchors remain inspectable and inert. The WASM surface
serializes the query result. The web host scrolls internal targets into view and
may open only explicitly allowed external schemes (`https`, `http`, and
`mailto`) with opener isolation. Pointer drag and non-primary clicks retain
selection behavior; a primary click without a drag activates a link.

The v1 edit operation set gains explicit create/update/remove hyperlink
operations. The first implementation accepts a non-empty selection contained in
one paragraph, rejects nested or partially intersecting link wrappers, and
allocates wrapper IDs through the session's deterministic ID generator. Updating
or removing an exact existing link is undoable through the normal transaction
history. The editor exposes this through a Link command rather than direct model
mutation.

Acceptance:

- imported external links are discoverable and clickable under host policy;
- imported TOC rows resolve their bookmark and move to the target page/position;
- create, update, remove, undo, redo, save, and reopen preserve link semantics;
- ordinary clicks and drag selection outside links are unchanged.

## Slice C — cross-paragraph side-wrap exclusion

Page/margin/column-relative `square`, `tight`, and `through` floats require
pagination-aware exclusions. The implementation uses a bounded fixed-point:

1. paginate and place floats with the current geometry;
2. derive page-local exclusion bands for affected top-level body paragraphs;
3. rebuild only affected paragraph line measures using the existing side-wrap
   shaper, then repaginate;
4. stop when float rectangles and affected line measures stabilize, or after
   three passes.

Because a page/margin-relative object can sit above its anchoring paragraph,
every top-level paragraph whose origin falls inside the resolved vertical band
is considered, including an earlier paragraph. (A paragraph that begins above
the float needs a future line-offset exclusion rather than an incorrect
whole-paragraph shift.) Multiple edge-anchored floats contribute the union (the
maximum occupied interval per edge) on each line. The supported envelope is
explicit rectangular geometry with left/right side placement. Unsupported
contours remain rectangular. On non-convergence, the widest and longest
observed exclusion on each edge is retained for one terminating pagination pass
so text cannot overpaint any observed float band. Incremental pagination
deliberately falls back to the same full fixed point whenever these
cross-paragraph exclusions are present.

Acceptance:

- the page-7 arrows exclude every intersecting body line even though the right
  arrow is anchored in a later paragraph;
- multiple exclusion rectangles combine deterministically;
- pagination terminates and repeated layout is byte-for-byte stable.

## Slice D — framed drop caps

`ParagraphProperties` gains an optional first-class frame record for the bounded
`w:framePr` surface needed by drop caps: drop mode (`drop` or `margin`), authored
line span, horizontal/vertical anchor and alignment/position, and horizontal/
vertical spacing. Import and semantic export must form a fixed point. Generic
non-drop paragraph frames outside this envelope remain preserved by retained
source and are compatibility-reported until modeled.

Layout recognizes a framed one-character drop-cap paragraph followed by a body
paragraph as a coupled flow unit. The glyph is measured at its authored size,
placed without exact-line clipping, and contributes a side exclusion spanning
the authored number of body lines. The next paragraph wraps beside that box and
returns to the full measure below it. Margin drop caps use the margin-side
reference box; invalid line counts are clamped to the validated model domain.

Acceptance:

- the page-5 initial is fully visible and the adjacent paragraph wraps without
  collision;
- import → semantic write → reopen preserves the frame;
- ordinary framed paragraphs do not silently acquire drop-cap behavior.

## Slice E — visual and collision regression gate

Add small generated, redistribution-safe DOCX fixtures for each defect family.
The machine-readable gate validates:

- line-child containment after slicing;
- monotonic, non-overlapping table row chunks;
- paint bounds against owning line/cell/page clips;
- hyperlink and bookmark target hit maps;
- deterministic rendered page hashes under the pinned bundled-font set.

LibreOffice screenshots remain an offline diagnostic oracle, not a CI
dependency. The gate records platform, renderer, font-set, page size, and scale.
The exact external `demo.docx` remains a local acceptance probe and its hash is
recorded only in the existing corpus audit.

## Slice F — host-loaded web font families

The browser build excludes the Roboto asset bytes from the WASM artifact and
keeps a deterministic metric-compatible bundled fallback for the interval
between document open and host provisioning. The web host fetches version-pinned,
CORS-enabled OpenType assets and registers them through the existing
host-populatable registry before first paint:

- Roboto variable upright and italic faces;
- Noto Sans variable upright and italic faces;
- Noto Serif variable upright and italic faces;
- the existing script-specific Noto CJK/Arabic/Devanagari/Hebrew/Thai faces,
  fetched only when `missingCoverage()` reports a relevant code point.

Font URLs and family metadata live in a testable web manifest, not in the Rust
runtime. The manifest may be replaced by a self-host deployment without changing
the SDK. Fetches are cached by immutable URL. A batch registration API admits a
bounded number/total size of host blobs and performs one repagination after the
batch, avoiding one full layout per face.

Native/headless builds keep the current pinned deterministic bundled set. The
web-only feature changes neither the normalized document nor semantic export;
it changes only the explicit font input to shaping. If the network is unavailable,
the editor remains usable with its bundled metric-compatible fallback and reports
the unavailable family instead of blocking document open.

Acceptance:

- release WASM built for the web omits the four Roboto static byte blobs;
- Roboto, Noto Sans, and Noto Serif are fetched externally and selectable;
- all six variable faces register in one bounded batch and trigger one
  repagination;
- script-specific fonts stay coverage-driven rather than eagerly downloading
  the large CJK assets;
- native deterministic layout tests are byte-for-byte unchanged.

## Delivery and rollback

Each slice lands as an individual commit and the set is reviewed in one PR.
Every behavior-changing slice includes focused tests before its tracker row is
marked Done. A slice can be reverted independently; schema additions are
additive and remain readable if a later layout slice is rolled back.
