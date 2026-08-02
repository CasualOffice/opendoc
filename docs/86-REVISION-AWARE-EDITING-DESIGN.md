# 86 — Revision-Aware Range Splitting and Editing

**Status:** Accepted; implementation tracked as **P1G-REVIEW-037** (closes **REVIEW-GAP-007**, doc 81).
**Scope:** `crates/casual-doc-edit` mutation helpers and their `crates/casual-doc-wasm` suggesting-mode mirrors.
**Depends on / relates to:** doc 68 (comments & suggestions), doc 82 (review identity & scoped operations), doc 83 (review projection & formatting — established the *read-side* wrapper descent this design extends to the write side).

## Problem

Every *mutation* helper in `casual-doc-edit/src/lib.rs` resolves offsets through `run_segments`
(`lib.rs:1873`), which walks a paragraph's **top-level** `inlines` and yields a segment **only for a
top-level `InlineNode::Run`**, advancing the cumulative offset past `Revision` / `Hyperlink` / `Sdt`
wrappers without ever descending into them. As a result, when a caret or selection endpoint falls
**inside** a wrapper, the editor does not behave like a mature word processor:

| # | Scenario | Path today | Result |
|---|----------|------------|--------|
| a | Type inside an existing pending insertion | `insert_text` (`lib.rs:1942`) finds no top-level run at the interior offset; the fallback never matches `cum == offset` | A stray **default-property run is appended at the paragraph end**, outside the suggestion, untracked — silent misbehavior |
| b | Delete a selection spanning pending + normal text | `ensure_run_boundary` (`lib.rs:1505`) no-ops inside the wrapper, then `remove_covered_range` (`lib.rs:2070`) hits its partial-wrapper guard | `EditError::Unsupported` — hard failure, document unchanged |
| c | Format a pending suggestion | `covered` is built from `run_segments` filtered to top-level runs (`lib.rs:505`); the wrapped run is not a top-level segment | `covered` is empty → **silent no-op** |
| d | Edit through a `Revision`/`Hyperlink`/`Sdt` boundary | `split_inlines` (`lib.rs:2205`), `remove_covered_range` (`lib.rs:2070`), `covered_top_level_indices` (`lib.rs:1919`) all reject a wrapper straddling the offset | `EditError::Unsupported` |

Doc 83 (REVIEW-GAP-030) already solved the **read** side: `flatten_run_segments` (`lib.rs:1835`) descends
into final-with-markup-contributing `Revision` / `Hyperlink` / `Sdt` wrappers so toolbar reflection sees
formatting inside a suggestion. It returns `&properties` only, so it cannot drive mutation. The seam is
documented in-code at `lib.rs:1706` and `lib.rs:1816`: *"the editing/split paths keep using `run_segments`
(top-level runs only) because revision-aware splitting is separate work (REVIEW-GAP-007)."* This document
specifies that separate work.

## Model recap

`InlineNode` (`casual-doc-model/src/v1/body.rs:1714`) is a leaf-or-wrapper tree. The editing-relevant
shape:

- **Leaf, editable:** `Run { id, properties, text }` (`body.rs:16`) — the only text-bearing leaf.
- **Transparent wrappers** with nested `inlines: Vec<InlineNode>`: `Hyperlink` (`body.rs:998`),
  `Revision` (`body.rs:1361`), `InlineSdt` (`body.rs:1670`). A revision "is a transparent range marker;
  it may wrap leaf inlines, a hyperlink/field, or a **nested** revision, and may itself appear inside a
  hyperlink/field" (`body.rs:1352`).
- **`RevisionKind`** (`body.rs:1277`): `Insertion`, `Deletion`, `MoveFrom`, `MoveTo`.
  `contributes_to(FinalWithMarkup)` is true only for `Insertion | MoveTo` (`body.rs:1307`), so a pending
  `Deletion`/`MoveFrom` is **zero active width** — it shows as struck-through markup but occupies no
  editable bytes.

A pending suggestion is a `Revision { kind: Insertion, inlines: [Run…] }`; a pending delete is a
`Revision { kind: Deletion, inlines: [Run…] }` that retains the original text. Suggesting-mode authoring
lives in the wasm layer (`casual-doc-wasm/src/lib.rs`), which rebuilds the paragraph body and emits a
plain `SetInlines` op — it does **not** go through a dedicated edit-crate suggesting API.

## Decisions

### 1. A mutation-capable locator: the run path

Introduce an internal locator that mirrors `flatten_run_segments` but is **mutation-capable**: instead of
`&properties`, it yields an **index path** into the nested `inlines` plus the byte offset within the target
run.

```rust
/// Path from a paragraph's top-level `inlines` down to a Run leaf, through
/// editing-transparent wrappers. `path[0]` indexes the paragraph's inlines;
/// each subsequent element indexes the wrapper's `inlines`. `offset_in_run`
/// is the byte offset of a boundary within the located Run's text.
struct RunPath { path: SmallVec<[usize; 4]>, offset_in_run: usize }
```

A resolver `locate_run(inlines, offset, transparency) -> Option<RunPath>` walks the tree using the same
cumulative-offset accounting as `inline_text_len` (`lib.rs:1786`) — the invariant that offsets align with
hit-testing (`lib.rs:1784`) is preserved because the *accounting* is unchanged; only the *descent* is new.
A companion `run_at_path_mut(inlines, &path) -> &mut Run` and `split_at_path(inlines, &path, ids)` (splits
the located run in place, returning the path to the right half) give the mutation helpers a wrapper-aware
equivalent of `ensure_run_boundary`.

`run_segments` is **retained unchanged** for the top-level fast paths; the wrapper-descending resolver is
used only when the top-level lookup misses. This keeps the common (no-wrapper) case identical and
low-risk.

### 2. Which wrappers are editing-transparent

`transparency` classifies a wrapper at an offset:

- **`Hyperlink`, `InlineSdt`, `Revision{Insertion}`, `Revision{MoveTo}`** → **transparent**: the resolver
  descends and edits the nested runs. (`Insertion`/`MoveTo` are the projections that contribute active
  width, so a caret can legitimately sit inside them.)
- **`Revision{Deletion}`, `Revision{MoveFrom}`** → **opaque / zero-width**: they contribute no active
  bytes, so no caret offset resolves *into* them; edits treat them as boundaries, never split them. This
  matches Word — you cannot type inside text that is already suggested-for-deletion.
- **Nested revisions** (e.g. an `Insertion` inside a `Hyperlink`, or a formatting revision inside an
  insertion): the resolver recurses; classification is applied at each level.

### 3. Typing inside a pending insertion extends the same suggestion

*(Decision confirmed with product owner.)* When the caret is inside a `Revision{Insertion}` authored in the
current review identity, inserted text is merged into that revision's nested runs (continuing the existing
`editor_group`), not wrapped in a new adjacent revision. This yields one continuous suggestion card instead
of fragmenting a single logical edit. Insertion inside a **different author's** pending insertion is treated
as an edit at that boundary and authored as the current user's own adjacent insertion (we never mutate
another author's suggestion in place). The wasm `extend_review_group_insertion` (`wasm/lib.rs:8753`), which
today only extends at a top-level revision's trailing boundary, is generalized to extend at any interior
offset resolved by the locator.

### 4. Mixed delete: remove-own-pending, suggest-the-rest

*(Decision confirmed with product owner — Word semantics.)* For a delete whose range spans a pending
insertion and normal text, each covered segment is handled by origin:

- **Text inside the current user's own `Revision{Insertion}`** → **removed outright** (un-suggesting a
  not-yet-accepted insert; deleting nothing that ever existed in the accepted document).
- **Normal (accepted) text** → wrapped in a `Revision{Deletion}` as usual.
- **Another author's pending insertion** covered by the range → left intact and, if required, marked with
  our own deletion at the boundary; we do not silently discard another author's suggestion.

After the operation, empty wrappers are removed (wrapper `inlines` must stay non-empty, `body.rs:1006`,
`body.rs:1380`, `body.rs:1675`).

### 5. Formatting descends and splits within the wrapper

`FormatText` / `ClearFormatting` resolve both endpoints with the locator, split the located runs at those
paths (decision 1), and apply the property delta to every run — top-level **and** wrapper-nested — fully
inside the range. This makes scenario (c) a real edit instead of a no-op, and lets formatting cross a
wrapper boundary (scenario d) by splitting at the boundary rather than rejecting it.

### 6. Exact inverse via snapshot → `SetInlines`

Revision-aware edits mutate nested `inlines` and have no cheap structural inverse, so they follow the
established snapshot pattern (`lib.rs:451`, used by the multi-run `DeleteText`, `FormatText`,
`ClearFormatting`, `SetHyperlink`): clone `para.inlines` up front and return
`Operation::SetInlines { node, inlines: old }` (the documented "inverse vehicle", `lib.rs:192`). A
`SetInlines` inverse restores nested wrappers verbatim, which is exactly what the mixed accepted/pending
round-trip tests assert. On `Err`, the document is left unchanged (`lib.rs:404`).

### 7. Normalization keeps the model valid

`coalesce_adjacent_runs` (`lib.rs:2044`) becomes wrapper-aware: it coalesces adjacent equal-property runs
**within each wrapper's `inlines`** as well as at top level, and it **never merges across a revision,
author, `editor_group`, hyperlink, or SDT boundary** — those boundaries are semantically load-bearing even
when the two runs' `RunProperties` happen to match. This upholds the "no adjacent equal-property runs"
invariant (`lib.rs:2039`) without collapsing distinct suggestions.

### 8. Fail-closed remains the default for the genuinely unsupported

The design widens what is *supported*, not what is *silently attempted*. Cases still outside scope — e.g.
splitting a field/SDT that must remain atomic, or an edit that would violate a wrapper's non-empty
invariant with no valid normalization — continue to return `EditError::Unsupported` with the document
unchanged, rather than best-effort corrupting the tree.

## Invariants preserved

- **Offset ↔ projection alignment:** `inline_text_len` / `nested_len` accounting is unchanged; the locator
  reuses it, so hit-testing and export offsets stay consistent (`lib.rs:1784`).
- **No adjacent equal-property runs** (decision 7).
- **Wrapper `inlines` non-empty** — empty wrappers are pruned after every mutation (decision 4).
- **Single choke point / closed op set** (extensibility invariants I1/I2, doc 45): no new `Operation`
  variant is required — revision-aware edits reuse `SetInlines` as their inverse, so OT/CRDT and
  agentic/MCP layers see no new surface.

## Verification and test matrix

Edit-crate unit tests (`mod tests`, `lib.rs:2271`), one per scenario, each asserting the forward result
**and** that applying the returned inverse restores the original tree byte-for-byte:

1. Type inside own pending insertion → extends the same `Revision{Insertion}`; inverse restores.
2. Delete across pending-insertion + normal → own-insert removed, normal becomes `Revision{Deletion}`;
   inverse restores.
3. Format a pending suggestion → nested runs split and re-propertied; inverse restores.
4. Split/format through a `Hyperlink` and through an `Sdt` boundary → nested split, no `Unsupported`.
5. Mixed **two-author** insertion: editing near author B's suggestion authors author A's own revision and
   never mutates B's runs.
6. Nested revision (formatting revision inside an insertion) resolves and edits correctly.
7. Coalescing does **not** merge across an author/group/wrapper boundary.

WASM regressions in `casual-doc-wasm` for `suggest_insert` / `suggest_delete` / `suggest_format` at
interior wrapper offsets, and Playwright e2e (`rich-paste-suggesting.spec.mjs`, `suggesting-mode-gate.spec.mjs`
and a new interior-edit spec) proving the four scenarios behave in the real editor. CI gates unchanged:
`cargo +1.96.0 fmt --check`, strict all-feature/all-target Clippy `-D warnings`, `cargo test --workspace
--all-features --locked`, webapp unit + browser-smoke.

## Phasing

- **Phase 1 (edit crate):** `RunPath` + `locate_run` / `split_at_path`, wrapper-aware variants of
  `insert_text`, `ensure_run_boundary`, `split_inlines`, `remove_covered_range`,
  `covered_top_level_indices`, `coalesce_adjacent_runs`; the seven unit tests above. Pure Rust, no UI
  surface — verifiable entirely by `cargo test`.
- **Phase 2 (wasm + e2e):** generalize `review_split_top_level_run` (`wasm/lib.rs:8697`) and
  `extend_review_group_insertion` (`wasm/lib.rs:8753`) to use the interior locator; wasm + Playwright
  coverage. Delivered as a follow-up PR so Phase 1's engine change reviews on its own.

Each phase ships on its own branch and PR.
