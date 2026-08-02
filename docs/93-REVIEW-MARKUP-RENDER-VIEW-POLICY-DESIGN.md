# 93 — Review-Markup Render View Policy

**Status:** Proposed; implementation tracked as **P1F-REVIEW-MARKUP-VIEW** (closes the docs/55 §11 / docs/60 §11 "review semantics have no view policy" residual and the read-only markup view P1G-REVIEW-036 deferred).
**Scope:** `crates/casual-doc-layout` (the galley) and a one-line viewer wiring; the native `casual-doc-render` paint primitives it needs already exist. No model / import / export / edit-op change.
**Relates to:** doc 55 §11, doc 60 §11, doc 83 (review projection & formatting), P1G-REVIEW-035/036/048.

## Problem

The native layout galley hard-codes `ReviewProjection::FinalWithMarkup` and applies **no** review presentation:

- **Insertions / MoveTo** (`flow.rs` revision arm, ~2425) recurse into their runs with **zero markup styling** — no author color, no underline — so a tracked insertion renders as ordinary accepted body text.
- **Deletions / MoveFrom** hit the catch-all arm and are dropped to **zero active width** — the struck text is not shown at all (contrary to the leak's original framing; it is *invisible*, not "normal text").
- **Comment ranges** (`CommentRangeStart`/`End`) and `CommentReference` fall through `_ => {}` — no highlight, no marker.

So the `casual-doc-render` CPU/PNG path — and anything that rasterizes the shared galley — shows no tracked-change or comment markup. The paint primitives are already there: `GlyphRun` carries `color`, `decoration { underline, strikethrough }`, `highlight`, and `casual-doc-render` already paints all three (P1F-15). The missing piece is the layout flattening step that never sets them from `RevisionKind` — deliberately, because there was no view policy to key it on (P1G-REVIEW-036: the read-only markup view was "explicitly deferred until it has a view-position/editing policy").

## The consumer-topology constraint (why this can't be a paint-only drop-in)

The webapp **live editor renders its text through the same galley**: `doc.renderPage(i, dpi)` rasterizes the native galley to a bitmap that is blitted onto a canvas (`webapp/src/main.js`), and P1G-REVIEW-048 paints per-author insertion/deletion markup and comment highlight as a **DOM overlay on top of that canvas** (JS/CSS only). Therefore, stamping author color + underline + strikethrough into the shared galley's `GlyphRun`s would make the editor render markup **twice** — once baked into the canvas, once in the overlay — and would also change the editor's byte space (showing struck deletions), which ~15 `casual-doc-wasm` caret/selection call sites depend on staying zero-width.

So the markup **cannot** be unconditionally baked into the galley. It must be a **distinct, opt-in, read-only view** that the editor does not request.

## Decisions

### 1. An explicit render view policy at the galley entry

Add a `ReviewView` parameter to the galley builders (`build_galley`, `build_galley_with_report`, `build_galley_for_blocks`) and thread it into `FlowCtx`:

```rust
/// How the galley presents tracked changes and comments. The editor uses
/// `Editing` (unchanged: final-with-markup byte space, no baked styling — the
/// webapp draws markup in its DOM overlay). Read-only viewers request `Markup`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReviewView {
    #[default]
    Editing,
    /// Read-only: show struck deletions, author-colored/underlined insertions,
    /// and highlighted comment ranges. Never fed to caret/selection/hit-test.
    Markup,
}
```

`Editing` reproduces today's behavior exactly (default, so every existing call site is unchanged). `Markup` is the new read-only view.

### 2. `Markup` keeps and strikes deleted content; the editor galley never does

Under `ReviewView::Markup` the deletion/moveFrom arm **recurses instead of dropping**, emitting the struck text; under `Editing` it stays zero-width as today. This is the one place the **byte space differs**, which is exactly why `Markup` is a *separate read-only galley reached only through the read-only entry* — it is never handed to the caret/`selectionRects`/hit-test path. The editor's default galley (deletions zero-width) is untouched, so the wasm caret math and the incremental-halt byte-space contract are preserved.

### 3. What `Markup` paints

| Construct | Presentation |
|---|---|
| `Insertion` | author color + underline |
| `MoveTo` | author color + underline (move destination) |
| `Deletion` | author color + strikethrough, **shown** |
| `MoveFrom` | author color + strikethrough (move origin) |
| Comment range | author/comment highlight fill over the runs between `CommentRangeStart`/`End` |

Styling is set as an override on `FlowCtx` before recursing into a revision's inlines (so nested runs inherit it), and a comment-range stack stamps `highlight` on runs inside an open range. All of it uses the existing `GlyphRun` fields — no render-crate change.

### 4. Author → color is presentation-only and shared with the overlay

Map `Revision.author` to one of a fixed 10-hue palette via the **same FNV-1a hash** the webapp overlay (P1G-REVIEW-048) uses, so the PNG/fidelity view and the live editor overlay assign the same author the same hue. The color is never persisted (render-only), consistent with the extensibility seams (docs/45) — no new model field, no export change.

### 5. The galley cache key must fold in the view

`build_galley_cached` keys on `paragraph_hash` alone because there is exactly one policy today. The `ReviewView` **must** be folded into that key, or a `Markup` galley could be served from an `Editing` cache entry (and vice-versa). Hard requirement.

### 6. Only the native read-only viewer requests `Markup`

The `casual-doc-render` fidelity/PNG viewer (and any future read-only "show changes" preview) passes `ReviewView::Markup`; the webapp live editor passes `Editing` and keeps its DOM overlay. This is what avoids double-markup. `Original`/`Final` read-only views (accept-all / reject-all previews) are a natural later extension of the same enum but are **out of scope** here.

## Compatibility and safety

- **Default-unchanged:** `ReviewView::Editing` is the default; every existing galley call, test, and the wasm caret path behaves exactly as today.
- **Byte-space isolation:** `Markup` is read-only and never reaches caret/selection/hit-test; only `Editing`'s zero-width-deletion byte space does.
- **No model/import/export/edit-op change; no new persisted state.** Extensibility invariants I1–I4 (docs/45) intact.
- **No double-markup:** the editor canvas stays on `Editing`; markup shows only in its DOM overlay (live editor) or the `Markup` galley (read-only PNG/fidelity), never both.

## Verification

- Layout unit tests: under `Markup`, an insertion run emits an author-colored underlined `GlyphRun`; a deletion emits a **non-empty** struck run; a comment-bracketed run carries `highlight: Some(_)`; under `Editing` all of the above are unchanged (deletion zero-width, no decoration) — guarding the byte-space contract.
- A render test: a struck+colored `GlyphRun` paints a mid-line decoration row in the author color (reuse the `render_decorated` harness).
- Cache-key regression: a `Markup` request never returns an `Editing` cache entry.
- Fixture/PNG: a small tracked-changes DOCX (ins + del + comment range) → CPU-backend snapshot showing struck colored deletion, underlined insertion, highlighted comment span (the fidelity corpus can extend this).

## Delivery

- **PR1 (this design):** the doc + a tracker row.
- **PR2 (M):** the `ReviewView` plumbing (default `Editing`), the `Markup` styling for insertion/deletion/moveFrom/moveTo + comment highlight, the cache-key fold, the read-only galley entry, the native fidelity/PNG viewer wiring, and the tests above.
- **PR3 (optional):** `Original`/`Final` read-only accept/reject previews on the same enum.
