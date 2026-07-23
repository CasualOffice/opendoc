# Selection Foundation

**Status:** Accepted for Phase 0
**Decision date:** 2026-07-24
**Tracker:** P0-004
**Implementation:** Complete on 2026-07-24

## Outcome

Introduce validated caret and text-range state that survives all implemented
transactions without coupling the model to a UI toolkit, DOM selection, or
platform input API.

## Scope

Phase 0 supports one text selection:

```text
TextSelection {
  anchor: Position,
  focus: Position
}
```

Anchor is where the selection began. Focus is the active end. Their order is
preserved so hosts can retain selection direction.

Not included yet:

- multi-range selections;
- table-cell selections;
- node/object selections;
- keyboard or word navigation;
- pointer hit testing;
- IME composition;
- platform accessibility selection bridges;
- collaborative remote selections.

## Ownership

Selection belongs to `DocumentSession` runtime state. It is not serialized in
the normalized document and does not affect deterministic document export.

The `casual-doc-selection` crate owns:

- selection values and invariants;
- validation against a normalized document;
- transaction-map application;
- collapsed-range semantics.

The SDK owns public selection projections. Hosts own focus, pointer capture,
keyboard dispatch, and visual selection painting.

## Initial State

Every new or loaded session starts with a collapsed caret at grapheme offset zero
of the first body paragraph, with `After` affinity.

Schema v0 guarantees at least one paragraph body block, so initial selection
creation is infallible after model validation.

## Validation

Both anchor and focus must:

- resolve to an existing paragraph node;
- use an offset no greater than the paragraph's grapheme length;
- remain representable as `u32`;
- carry explicit affinity.

Anchor and focus may be in different paragraphs. Structural ordering is not
stored by the selection because direction matters; consumers that need an
ordered range use a future document-order resolver.

A selection is collapsed when anchor and focus use the same node and grapheme
offset. Affinity does not make a logical range non-collapsed.

## Session API

```rust
let selection = session.selection()?;

session.set_selection(SetSelectionRequest {
    base_revision: session.snapshot()?.revision,
    selection,
})?;
```

Selection changes:

- require an expected document revision;
- return `ODC-2001` when stale;
- return `ODC-2002` when invalid;
- do not increment revision;
- do not add or clear undo/redo history;
- will emit a selection event when the event bus exists.

## Transaction Mapping

During forward edit, undo, and redo:

1. apply operations to the working document;
2. produce the complete position map;
3. map anchor and focus independently in operation order;
4. validate the mapped selection against the committed working document;
5. publish document, revision, selection, history, and mapping atomically.

If mapped selection validation fails, the session treats it as
`ODC-2005 invariant_violation` and does not partially commit.

Existing mapping semantics produce:

- insertion at a caret: `After` affinity moves the caret after inserted text;
- deletion containing an endpoint: endpoint collapses to deletion start;
- split: endpoints after the split move to the new paragraph;
- join: endpoints in the removed paragraph move into the retained paragraph;
- undo/redo: selection follows the inverse transaction's map.

Phase 0 does not restore historical selection snapshots on undo. It maps the
current selection through undo operations. Selection-before/after history
restoration is designed with command grouping and input semantics.

## Threading

Selection is protected by the same session write lock as document, revision, and
history. A snapshot read observes one coherent session state. Public callbacks
are not invoked under this lock.

## Rejected Alternatives

**Store selection in the normalized document**

Rejected because selection is per-session/per-user runtime state and must not
change document serialization.

**Use global text offsets**

Rejected because paragraph identity and local grapheme boundaries already form
the transaction mapping contract.

**Let hosts own the canonical selection**

Rejected because commands, history, IME, collaboration anchors, and hit testing
need one engine-validated logical selection.

**Increment document revision for selection movement**

Rejected because selection-only movement does not mutate semantic document
content and would create false persistence/collaboration changes.

## Acceptance Gates

- a new and loaded session has a valid default caret;
- valid cross-paragraph anchor/focus state can be set and read;
- invalid and stale updates preserve prior selection;
- insert/delete/split/join/undo/redo map both endpoints correctly;
- selection-only changes leave revision and history unchanged;
- mapped selection validation is part of atomic commit;
- native, WASM, MSRV, docs, lint, and policy gates remain green.
