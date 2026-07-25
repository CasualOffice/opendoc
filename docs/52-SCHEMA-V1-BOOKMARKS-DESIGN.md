# Normalized Schema v1: Bookmarks Design

**Status:** Implemented — 2026-07-25 (multi-agent coverage workflow; adversarially reviewed, verdict sound-with-fixes; all review fixes folded in at implementation).
**Tracker:** P1A-036

> Produced by the parallel model-coverage design workflow. The adversarial review flagged concrete implementation fixes (see the tracker entry); fold them in at implementation time.



**Status:** Draft for review — 2026-07-25
**Tracker:** P1A-0xx — extends P1A-019 (schema v1 semantic extension); first of the "range markup" family (bookmarks, then comment ranges, then move ranges)
**Decision basis:** ADR-027, schema v1 (`38-…`), comments (`47-…`), tracked changes (`48-…`, the closest analog for id-paired markers), importer no-skip audit
**Files touched:** `casual-doc-model/src/v1/{ids.rs, definitions.rs, body.rs, document.rs}`, `casual-doc-model/src/error.rs`, `casual-doc-import/src/{body.rs, lib.rs}`

## Why

`w:bookmarkStart{w:id,w:name}` / `w:bookmarkEnd{w:id}` are the range-anchor construct that internal navigation depends on. Today both fall through to the final `_ if self.in_document => self.reporter.report(local)` arm in `on_start`: the bookmark name is lost, its position is lost, and an internal hyperlink (`HyperlinkTarget::Internal{anchor}`) resolves a `w:anchor` string that points at nothing modeled. This slice models a bookmark as an id-keyed **definition** (its name) plus a **paired marker range** (`BookmarkStart`/`BookmarkEnd` leaf inline nodes) delimiting its extent in document flow.

## Model — definition + paired range markers (not a wrapper, not name-on-marker)

Bookmarks are modeled as **two leaf `InlineNode` marker variants** referencing a **`Bookmark` definition** keyed by a new `BookmarkId`. Three properties of the source drive every part of this choice:

1. **Bookmarks overlap and are not well-nested.** `<bookmarkStart id=1><bookmarkStart id=2><bookmarkEnd id=1><bookmarkEnd id=2>` is legal. A *wrapper* node (`Hyperlink`, `Field`, `Revision`) requires strict XML nesting and therefore **cannot** represent bookmarks — this is the decisive reason bookmarks are paired independent markers, not a range wrapper. Each marker is a zero-width point; the "range" is the ordered span between the two markers that share a `BookmarkId`.

2. **Bookmarks span block boundaries.** A bookmark routinely starts in one paragraph and ends in another (TOC / section anchors). Because the two markers are *independent* leaf nodes each carrying the `BookmarkId`, a start in paragraph 1 and an end in paragraph 3 pair by shared `BookmarkId` with **no requirement that they sit in the same paragraph** — exactly what a wrapper cannot express and a marker pair can.

3. **The name is a property of the whole bookmark, declared once (on the start).** The end carries only `w:id`. Putting the name on a keyed *definition* (rather than duplicating it onto the start marker and inventing a name-less asymmetry on the end) gives a single authoritative storage + domain-check site, deduplicates it, follows the established id-cross-referenced pattern (`comments`, `notes`, `media`), and leaves a hook for a future name→bookmark index. This is why I choose **definition + range** over **name-on-marker**.

```rust
// ids.rs — new newtype via the existing id_newtype! macro
id_newtype!(
    /// Stable identity of a bookmark definition (shared by its start/end markers).
    BookmarkId
);

// definitions.rs
/// A bookmark definition (its id is the map key). A bookmark is a named range;
/// its extent is delimited by a `BookmarkStart`/`BookmarkEnd` marker pair in body
/// flow, and only its name is a definition-level property.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bookmark {
    /// The bookmark name as written (non-empty, at most 255 bytes).
    pub name: String,
}

// definitions.rs — new field on Definitions (append; additive)
/// Bookmark definitions by id. Additive: omitted when empty so existing
/// snapshots serialize byte-identically.
#[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
pub bookmarks: DefinitionMap<BookmarkId, Bookmark>,

// body.rs — two new leaf inline nodes
/// The start marker of a bookmark range (`w:bookmarkStart`). A zero-width point;
/// the range is the span to the `BookmarkEnd` sharing its `bookmark`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkStart {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The bookmark this opens (resolves in `Definitions::bookmarks`).
    pub bookmark: BookmarkId,
}

/// The end marker of a bookmark range (`w:bookmarkEnd`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkEnd {
    /// Stable identity.
    pub id: NodeId,
    /// The bookmark this closes (resolves in `Definitions::bookmarks`).
    pub bookmark: BookmarkId,
}

// body.rs — two new internally-tagged InlineNode variants (append)
InlineNode {
    …,
    /// The start marker of a bookmark range.
    BookmarkStart(BookmarkStart),
    /// The end marker of a bookmark range.
    BookmarkEnd(BookmarkEnd),
}
```

`InlineNode::id()` gains the two obvious arms (`Self::BookmarkStart(node) => node.id`, `Self::BookmarkEnd(node) => node.id`).

**Ids used per bookmark:** one `NodeId` for the `BookmarkId` (the definition key), plus one `NodeId` per marker node = 3 ids for a normal start+end pair, all distinct. The `BookmarkId`'s underlying `NodeId` is the definition key and is **allocated when the start tag is first seen** (it must exist before the end marker, possibly in a later paragraph, can reference it) — the same "allocate id at `on_start`" precedent as `enter_textbox` (which allocates `textbox_id` on the opening tag). Each marker's own `NodeId` is allocated later in document order in `segment_to_inline`, exactly like every other inline.

## Internal-hyperlink anchor resolution — keep it LAX; add `DanglingBookmarkRef` only for marker→definition

The task's key question. Two distinct references exist; they get opposite treatment:

- **Marker → definition (`BookmarkStart/End.bookmark` → `Definitions::bookmarks`): STRICT.** This is an importer-controlled structural invariant (the importer always inserts the definition when it allocates the `BookmarkId`), directly mirroring `DanglingCommentRef` / `DanglingNoteRef`. A marker whose `bookmark` does not resolve is a corrupt model → new **`DanglingBookmarkRef(NodeId)`** error. This is the "`DanglingBookmarkRef`-style check" the brief anticipated — but it guards marker integrity, **not** hyperlink anchors.

- **Internal hyperlink `anchor` (a bookmark *name*) → set of bookmark names: LAX. No new fatal error, and (this slice) not even a soft report.** Justification:
  1. **Forward references.** A hyperlink commonly precedes its target bookmark in document order; at hyperlink-commit time the target may be unseen, so any single-pass check yields false "dangling" results.
  2. **Cross-part targets.** An anchor in the body may target a bookmark defined in a header/footer/note — parts the importer parses into *separate* passes. A global name check would need cross-part merge before it is even meaningful.
  3. **Well-known anchors.** `_top`, `_GoBack`, and TOC/heading auto-bookmarks are valid targets that need no matching `bookmarkStart` in the same part.
  4. **Engineering priority order** (`AGENTS.md`): correctness/no-reject of valid documents outranks strictness. Failing `validate()` on a legitimate document because an anchor is unresolved is unacceptable; the existing `HyperlinkTarget::Internal` domain check (non-empty, ≤255) already bounds the value.

  A name→bookmark reverse index (enabling an *optional, deferred, non-fatal* dangling-anchor report after whole-document name collection) is recorded as an explicit future enhancement, not built here.

## Strict validation (additive)

- **`validate_bookmarks`** (new, called from `validate()` alongside `validate_comments`): for each `(_, bookmark)` in `definitions.bookmarks`, `check_domain(!name.is_empty() && name.len() <= 255, "bookmark.name")`.
- **`validate_unique_ids`**: insert each `definitions.bookmarks` key `node_id()` into the id set (like every other `DefinitionMap`), so a def id colliding with any node is `DuplicateNodeId`.
- **`validate_inlines`**: two new arms —
  ```rust
  InlineNode::BookmarkStart(marker) | InlineNode::BookmarkEnd(marker) => {
      if !self.definitions.bookmarks.contains_key(&marker.bookmark) {
          return Err(ModelError::DanglingBookmarkRef(marker.bookmark.node_id()));
      }
      previous_run_properties = None;   // hard merge boundary: preserves position
  }
  ```
  Resetting `previous_run_properties` is load-bearing — two equal-property runs separated by a bookmark marker must **not** be merged, or the marker position is destroyed. (Because `BookmarkStart`/`End` fields differ, the two-`marker` pattern needs either two arms or a small helper; both markers share `id`/`bookmark` field names so a helper `fn bookmark_ref(&self, m) -> BookmarkId` keeps it to one arm.)
- **`record_inline_ids`**: leaf nodes — the function's leading `insert_id(ids, inline.id())?` plus its `_ => {}` tail already covers them; no change beyond the two variants existing.
- **`accumulate_inline_limits`**: add `BookmarkStart`/`BookmarkEnd` to the existing no-text no-op arm (`Tab | Break | Drawing | NoteReference | CommentReference | …`). They carry no text and no children, so they cost nothing against text/block limits (their def ids are already covered by unique-id accounting).
- **New `ModelError` variant** (appended): `DanglingBookmarkRef(NodeId)` with `Display`: `write!(f, "bookmark reference {id} does not resolve")`.

Bookmark markers are **transparent to `in_wrapper`, `textbox_depth`, and `revision_depth`** (they are inert leaves like `Tab`), so they are permitted inside hyperlinks, fields, revisions, and text boxes with no new nesting rule.

## Import — self-closing markers, source-`w:id` pairing, no XML depth counter

Bookmarks live in the main body and in every part that reuses `BodyParser` (notes, headers, footers, comments), so **no new part and no `ParseInputs` field**. New `BodyParser` state:

```rust
/// Source `w:id` string -> allocated BookmarkId, for start/end pairing across
/// paragraphs. Part-scoped (NOT swapped in ContentFrame): a bookmark opened in
/// body flow and closed inside a text box still pairs.
bookmark_ids: BTreeMap<String, BookmarkId>,
```

Collected `Bookmark` definitions are accumulated through a **`&mut DefinitionMap<BookmarkId, Bookmark>` threaded into every part parser** — the exact `media::build_into(&mut media, …)` pattern — so bookmarks from body + notes + headers + footers + comments land in one document-global `Definitions::bookmarks`. `lib.rs` gains `let mut bookmarks = DefinitionMap::default();`, passes `&mut bookmarks` into each part entry point (`parse`, `parse_notes`, `parse_header_footer`, `parse_comments`), and sets `bookmarks` in the `Definitions { … }` literal (replacing `..Definitions::default()` for that field).

New `Segment` variants and their `segment_to_inline` arms (allocate the marker's node id in document order, mirroring every leaf):

```rust
enum Segment { …, BookmarkStart { bookmark: BookmarkId }, BookmarkEnd { bookmark: BookmarkId } }
```

### `on_start` — the Empty-event / id-balancing handling

`w:bookmarkStart` / `w:bookmarkEnd` are **self-closing** → quick-xml emits `Event::Empty`, which the reader dispatches as `on_start` **then** `on_end`. Both are handled **entirely in `on_start`**; because their local names are unique, their `on_end` falls to `_ => {}`. Unlike `w:ins`/`w:del`, **no depth or suppression counter is needed**: there is no XML content between a marker's open and close, so there is nothing to balance at the XML level. The "id balancing" is purely the source-`w:id` → `BookmarkId` map, and the report-vs-model rules below keep it self-consistent so **no dangling reference can ever be produced**.

```rust
b"bookmarkStart" if self.paragraph_open && !self.run_open => {
    // name required, bounded; else report+drop the whole bookmark (id NOT
    // registered → its end becomes an orphan and is also reported → balanced).
    match attribute_value(element, b"name")
        .filter(|n| !n.is_empty() && n.len() <= 255)
    {
        Some(name) => {
            let source = attribute_value(element, b"id").unwrap_or_default();
            if self.bookmark_ids.contains_key(&source) {
                self.reporter.report(b"bookmarkStart");      // duplicate id: keep first
            } else {
                let bookmark = BookmarkId::new(self.next_id()?);   // def id, doc order
                self.bookmarks.insert(bookmark, Bookmark { name });
                self.bookmark_ids.insert(source, bookmark);
                self.push_segment(Segment::BookmarkStart { bookmark });
            }
        }
        None => self.reporter.report(b"bookmarkStart"),
    }
}
b"bookmarkEnd" if self.paragraph_open && !self.run_open => {
    match attribute_value(element, b"id")
        .and_then(|source| self.bookmark_ids.get(&source).copied())
    {
        Some(bookmark) => self.push_segment(Segment::BookmarkEnd { bookmark }),
        None => self.reporter.report(b"bookmarkEnd"),   // orphan end: report+drop
    }
}
```

`push_segment` already routes a marker into the innermost open wrapper (hyperlink/field/revision) or the paragraph, so a marker inside a link/revision is preserved in the correct container.

### Balancing guarantee (why no reference ever dangles)

| Source shape | Start | End | Result |
|---|---|---|---|
| both inline, paragraph open | modeled (def created) | modeled (same `BookmarkId`) | full pair, resolves |
| inline start, **block-level** end | modeled | reported+dropped (guard fails) | start with no end marker — **allowed** (lax pairing); no dangling ref |
| **block-level** start, inline end | reported (guard fails; **id never registered**) | `bookmark_ids` miss → reported+dropped | both reported; no partial model |
| missing/oversized name | reported (id not registered) | `bookmark_ids` miss → reported | both reported |
| duplicate `w:id` | second reported | pairs with first | consistent |
| orphan end (no start) | — | `bookmark_ids` miss → reported | dropped, reported |

The invariant: **a `BookmarkId` enters `Definitions::bookmarks` and the id-map only when a start is fully modeled inline; an end is only modeled when its id already resolves.** Therefore every modeled marker's `bookmark` resolves — `DanglingBookmarkRef` can never fire on importer output, matching how `DanglingCommentRef` never fires on real imports. Block-level markers are the one deferred case (below), and deferring them is *safe* precisely because it drops both the marker and, when it's a start, its id-registration.

`bookmark_ids` (and the `&mut bookmarks` accumulator) are **not** part of `ContentFrame` save/restore — they are part-global — so a text box's inner markers share the part's bookmark space. `bookmark_ids` is cleared between reused-parser notes/comments in `close_note` alongside `suppressed_tbl_depth` (defensive; a bookmark never legitimately spans two notes).

## Backward-compat / additivity

Strictly additive:
- `InlineNode::BookmarkStart` / `BookmarkEnd` are new internally-tagged variants; no existing variant's bytes change and no existing snapshot contains them.
- `Definitions::bookmarks` is `#[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]` → omitted when empty, so every existing snapshot and the v0→v1 migration byte-exact golden are **byte-identical** (v0 has no bookmarks; `Definitions::default()` gains an empty map that never serializes).
- `BookmarkId` reuses the `id_newtype!` macro (`#[serde(transparent)]`), `Bookmark` uses `deny_unknown_fields`.
- New `ModelError::DanglingBookmarkRef` is appended (enum is non-exhaustive to consumers via its own `match`es).

## Explicitly deferred (reported, never silently dropped)

Each reaches an existing report arm, so it surfaces in the compatibility report:
- **Block-level bookmark markers** (between paragraphs, `!paragraph_open`) — reported this slice. Follow-up: attach a block-level start to the next-opened paragraph's leading inlines and a block-level end to the just-closed paragraph's trailing inlines (position-preserving), rather than dropping.
- **Column bookmarks** (`w:bookmarkStart@w:colFirst/@w:colLast`, table-cell column ranges) — the attributes are ignored (bookmark still modeled by name/range); note in the report only if we choose to flag them.
- **Strict internal-hyperlink anchor resolution** (name→bookmark reverse index + deferred non-fatal report) — deferred per the LAX decision above.
- **Other range markup** (`commentRangeStart/End`, `moveFromRangeStart/End`, `moveToRangeStart/End`, `customXml*RangeStart/End`) — separate follow-up slices reusing this marker-pair pattern.

## Test plan

**Model (`v1/tests.rs`):**
- Bookmark round-trips: a body with `Definitions::bookmarks` + a `BookmarkStart`/`BookmarkEnd` pair serializes and re-parses identically.
- `DanglingBookmarkRef`: a marker whose `bookmark` is absent from `definitions.bookmarks` is rejected.
- Name domain: empty and >255-byte names rejected (`bookmark.name`).
- Unique ids: a bookmark def id colliding with a node id → `DuplicateNodeId`.
- Adjacent-run normalization is **not** defeated across a marker (two equal-property runs separated by a `BookmarkStart` stay two runs — regression guard for the merge-boundary reset).
- A `BookmarkStart` with no `BookmarkEnd` (lax pairing) validates; a marker inside a hyperlink/revision/text box validates.
- Extend the `any_inline` test walker with both variants.

**Import (`import/src/tests.rs`):**
- Inline start+end pair → two markers + one `Bookmark{name}` def, both resolving; `bookmarkStart/End` no longer appear in the report.
- Bookmark spanning two paragraphs → start in para 1's inlines, end in para 2's inlines, same `BookmarkId`.
- Internal hyperlink whose anchor equals the bookmark name → hyperlink still modeled (unchanged), no error, no spurious report (lax).
- Orphan `bookmarkEnd`, missing/oversized `name`, duplicate `w:id`, block-level marker → each reported + dropped, no dangling ref, id counts balanced.
- Bookmark inside a text box / inside a `w:ins` / inside a `w:hyperlink` → modeled in the right container.
- Bookmark defined in a header/footer/comment part → lands in the single `Definitions::bookmarks`.
- Extend the fidelity walker (`tools/opendoc-fidelity`) and the export-presence walker with `BookmarkStart`/`BookmarkEnd` arms.
- Update any fixture/snapshot that currently asserts `bookmarkStart`/`bookmarkEnd` occurrence counts in the compatibility report (they move from Omitted to modeled).

All gates (fmt, clippy, unit, doctest, wasm, MSRV, doc) as in prior slices.

## CHANGELOG (Unreleased → Added)

> Bookmarks in schema v1: `w:bookmarkStart`/`w:bookmarkEnd` are modeled as an
> additive `Bookmark{name}` definition table (`Definitions::bookmarks`, keyed by a
> new `BookmarkId`) plus a paired `InlineNode::BookmarkStart`/`BookmarkEnd` marker
> range delimiting the bookmark's extent in document flow. Marker→definition
> integrity is validated (`DanglingBookmarkRef`); internal-hyperlink anchor
> resolution remains lax (forward/cross-part/well-known targets). Block-level
> markers, column bookmarks, and strict anchor resolution remain reported (not yet
> modeled). Additive: existing snapshots and the v0→v1 migration golden are
> byte-identical.