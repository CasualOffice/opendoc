# Low-Level Design

## 1. Identity and addressing

```rust
pub struct NodeId(u128);
pub struct RevisionId(u64);
pub struct TransactionId(u128);

pub struct Position {
    pub node: NodeId,
    pub offset: TextOffset,
    pub affinity: Affinity,
}

pub enum TextOffset {
    Grapheme(u32),
    ChildIndex(u32),
}
```

Node IDs are stable within the logical document. Imported documents receive generated IDs unless an engine-specific ID is already present in a supported extension.

Public positions use grapheme or child boundaries, not raw UTF-8 offsets.

## 2. Normalized model

```rust
pub struct Document {
    pub id: NodeId,
    pub body: Vec<BlockNode>,
    pub sections: SectionTable,
    pub styles: StyleSheet,
    pub numbering: NumberingTable,
    pub comments: CommentStore,
    pub notes: NoteStore,
    pub metadata: DocumentMetadata,
    pub extensions: ExtensionBag,
}

pub enum BlockNode {
    Paragraph(Paragraph),
    Table(Table),
    SectionBoundary(SectionBoundary),
    Custom(CustomBlock),
}

pub struct Paragraph {
    pub id: NodeId,
    pub properties: ParagraphProperties,
    pub inlines: Vec<InlineNode>,
}

pub enum InlineNode {
    Text(TextRun),
    Break(Break),
    Tab(Tab),
    Image(InlineImage),
    Drawing(DrawingObject),
    Field(Field),
    BookmarkBoundary(BookmarkBoundary),
    CommentBoundary(CommentBoundary),
    Custom(CustomInline),
}
```

Text storage should use a rope or piece-tree abstraction behind an internal trait. The public model should not expose the storage implementation.

## 3. Transactions

```rust
pub struct Transaction {
    pub id: TransactionId,
    pub base_revision: RevisionId,
    pub origin: TransactionOrigin,
    pub operations: Vec<Operation>,
    pub metadata: TransactionMetadata,
}

pub enum Operation {
    InsertText { at: Position, text: String, marks: MarkSet },
    DeleteRange { range: Range },
    SplitParagraph { at: Position },
    JoinParagraph { first: NodeId, second: NodeId },
    SetNodeProperties { node: NodeId, patch: PropertyPatch },
    AddMark { range: Range, mark: Mark },
    RemoveMark { range: Range, kind: MarkKind },
    InsertNode { parent: NodeId, index: u32, node: NodePayload },
    RemoveNode { node: NodeId },
    MoveNode { node: NodeId, parent: NodeId, index: u32 },
    Table(TableOperation),
    Annotation(AnnotationOperation),
    Custom(CustomOperation),
}
```

Application pipeline:

1. validate base revision or rebase policy;
2. validate structural constraints;
3. normalize operation sequence;
4. apply to mutable working copy;
5. verify invariants;
6. calculate inverse;
7. commit revision;
8. produce transaction map and invalidation set.

No observer sees a partially applied transaction.

## 4. Transaction mapping

Each committed transaction produces a `PositionMap` describing inserts, deletes, splits, joins, and moves.

It maps:

- current selection;
- comments;
- bookmarks;
- tracked-change anchors;
- plugin decorations;
- remote cursor positions;
- pending IME range.

Deleted anchors follow configured stickiness:

- before;
- after;
- nearest valid;
- invalidate.

## 5. Command layer

Commands are higher level than operations.

```rust
pub trait Command: Send + Sync {
    fn id(&self) -> CommandId;
    fn query(&self, ctx: &CommandContext) -> CommandState;
    fn execute(
        &self,
        ctx: &mut CommandContext,
        args: CommandArgs,
    ) -> Result<CommandResult, CommandError>;
}
```

Examples:

- `text.insert`
- `paragraph.set_alignment`
- `format.toggle_bold`
- `table.insert`
- `history.undo`
- `document.insert_page_break`
- `comment.add`

Command state supports enabled, active, mixed, and value.

## 6. Style resolution

Order:

1. document defaults;
2. theme;
3. based-on style chain;
4. linked style;
5. table style conditional formatting;
6. paragraph/run properties;
7. revision overlays;
8. host decorations that affect paint only.

Computed styles are cached by `(node_id, revision, style_context_hash)`.

Cycles in style inheritance are cut deterministically and reported.

## 7. Text shaping

Abstract interface:

```rust
pub trait TextShaper {
    fn shape(&self, request: ShapeRequest) -> Result<ShapedRun, ShapeError>;
}
```

Reference native implementation may use HarfBuzz through a safe wrapper. Web can use the same compiled shaping stack when licensing and binary size are acceptable.

Shaping input includes script, direction, language, font cascade, features, size, letter spacing, and text.

Fallback must be deterministic for a configured font set.

## 8. Paragraph layout

Pipeline:

1. resolve computed paragraph and run styles;
2. segment by bidi/script/font;
3. shape runs;
4. calculate break opportunities;
5. form lines with tabs and indentation;
6. position inline objects;
7. apply justification;
8. build line fragments;
9. calculate paragraph fragmentation across columns/pages.

Paragraph layout cache key includes content revision, width, style hash, font-set version, and locale settings.

## 9. Pagination

Pagination works with block fragments and constraints:

```rust
pub struct PaginationContext {
    pub page: PageGeometry,
    pub columns: Vec<ColumnGeometry>,
    pub header_footer: HeaderFooterMetrics,
    pub footnote_area: FootnoteArea,
}
```

Rules include:

- explicit breaks;
- keep-with-next;
- keep-lines-together;
- widow/orphan;
- table row splitting;
- repeat table headers;
- section transitions;
- footnote reservation;
- float exclusion regions.

Incremental pagination starts from the earliest invalid block and stops when page boundary state converges with the previous layout.

## 10. Tables

Use a logical grid independent of visual fragments.

```rust
pub struct TableGrid {
    pub columns: Vec<GridColumn>,
    pub rows: Vec<Row>,
    pub occupancy: CellOccupancyMap,
}
```

Layout stages:

1. resolve grid and spans;
2. compute intrinsic min/max widths;
3. allocate table width;
4. layout cells;
5. calculate row heights;
6. fragment across pages;
7. resolve borders using conflict rules;
8. repeat header rows.

Merged cell editing operates on the logical occupancy map.

## 11. Scene model

```rust
pub enum DisplayItem {
    PushTransform(Transform),
    PopTransform,
    PushClip(Clip),
    PopClip,
    GlyphRun(GlyphRunItem),
    Rect(RectItem),
    Path(PathItem),
    Image(ImageItem),
    LinkRegion(LinkRegion),
    Debug(DebugItem),
}
```

Scene snapshots include:

- page geometry;
- display items;
- hit-test index;
- semantic nodes;
- optional paint-only decorations.

Binary serialization should be available for WASM/FFI transfer.

## 12. Hit testing

Spatial indexes are built per page.

Pointer to position:

1. resolve page;
2. query block/line candidates;
3. account for transforms and writing direction;
4. find nearest glyph cluster;
5. return grapheme position and affinity.

Position to caret rect uses layout fragment maps.

## 13. IME

Composition is represented as ephemeral state, not committed document text until appropriate platform events occur.

The engine supports:

- start;
- update;
- commit;
- cancel;
- composition selection;
- candidate-window caret rectangle.

Remote edits that intersect composition trigger deterministic rebase or composition cancellation with an event to the host.

## 14. History

History entries store transaction plus inverse and selection before/after.

Grouping heuristics:

- consecutive typing;
- consecutive deletion;
- explicit host boundary;
- command boundary;
- timeout boundary;
- remote transaction boundary.

Memory limits may compact history or persist checkpoints.

## 15. DOCX preservation

Each imported element can carry:

```rust
pub struct ExtensionBag {
    pub namespaces: NamespaceMap,
    pub unknown_attributes: Vec<PreservedAttribute>,
    pub unknown_children: Vec<PreservedXml>,
    pub package_parts: Vec<PreservedPartRef>,
}
```

Preservation is only attempted when the engine can prove the fragment remains attached to a compatible semantic parent. Otherwise it emits a warning.

## 16. Events

Events are ordered by revision:

```rust
pub enum RuntimeEvent {
    Ready(ReadyEvent),
    TransactionCommitted(TransactionEvent),
    SelectionChanged(SelectionEvent),
    LayoutUpdated(LayoutEvent),
    CommandStatesChanged(CommandStateEvent),
    ResourceRequested(ResourceRequest),
    Warning(RuntimeWarning),
    Error(RuntimeError),
    Closed,
}
```

Callbacks must not execute while internal write locks are held.

## 17. Cancellation

Long-running methods accept a cancellation token:

- import;
- export;
- full layout;
- rendering;
- image decode;
- font scan.

Cancellation returns a typed non-fatal error and leaves the session in a valid state.

## 18. Error model

```rust
pub struct SdkError {
    pub code: ErrorCode,
    pub message: String,
    pub severity: Severity,
    pub context: ErrorContext,
    pub source_chain: Vec<String>,
}
```

Stable numeric/string codes are part of the public API. Internal Rust error types are not exposed over FFI/WASM.

## 19. Memory and limits

Configurable limits:

- ZIP expanded bytes;
- ZIP entry count;
- XML depth;
- XML node count;
- text length;
- image pixels;
- table cells;
- relationship count;
- custom extension bytes;
- max pages eagerly laid out;
- history bytes.

## 20. Serialization

Normalized snapshot format:

- canonical CBOR for binary;
- JSON for debugging;
- schema version in root;
- node IDs encoded as fixed-width bytes/string;
- unknown fields preserved where possible.

Operation format is separately versioned and designed for forward compatibility.
