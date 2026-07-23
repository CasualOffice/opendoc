# SDK API Specification

**Status:** Target v1 API with an implemented Phase 0 subset

The broad interfaces below describe the intended v1 facade and are not all
implemented. Current executable support is limited to blank-document creation,
immutable snapshots, grapheme-aware text insertion/deletion, paragraph
split/join, undo/redo, revision checks, position mapping, and stable SDK errors.
Strict bounded normalized schema v0 JSON load/export and canonical mapped
session selection are also implemented. A bounded synchronous journal exposes
ordered transaction and selection events.

## Phase 0 Implemented Subset

```rust
use std::collections::BTreeSet;

use casual_doc_sdk::{
    Affinity, BlockSnapshot, Engine, EngineConfig, InsertTextRequest, Position,
    SelectionSnapshot, SetSelectionRequest,
};

let engine = Engine::new(EngineConfig::default())?;
let session = engine.create_blank()?;
let snapshot = session.snapshot()?;
let paragraph = match &snapshot.body[0] {
    BlockSnapshot::Paragraph(paragraph) => paragraph.id.clone(),
};

let result = session.insert_text(InsertTextRequest {
    base_revision: snapshot.revision,
    at: Position {
        node: paragraph,
        grapheme_offset: 0,
        affinity: Affinity::After,
    },
    text: "Hello".to_owned(),
    marks: BTreeSet::new(),
})?;

assert_eq!(result.revision.get(), 1);

let selection = session.selection()?;
session.set_selection(SetSelectionRequest {
    base_revision: result.revision,
    selection: SelectionSnapshot {
        anchor: selection.anchor,
        focus: selection.focus,
    },
})?;
```

The SDK owns its public IDs, positions, snapshots, marks, and errors. Internal
model and transaction types are not re-exported.

The additional implemented editing methods are:

```rust
let mut subscription = session.subscribe()?;
session.delete_range(DeleteRangeRequest { /* ... */ })?;
session.split_paragraph(SplitParagraphRequest { /* ... */ })?;
session.join_paragraphs(JoinParagraphRequest { /* ... */ })?;
session.undo(expected_revision)?;
session.redo(expected_revision)?;
let batch = subscription.drain(64)?;
```

Each successful call returns a `TransactionResult` with the committed revision
and ordered mapping steps.

Normalized JSON sessions use:

```rust
let session = engine.open_normalized_json(
    bytes,
    OpenNormalizedOptions::default(),
)?;
let deterministic_bytes = session.export_normalized_json()?;
```

## 1. Target API layers

### Rust native API

Most expressive API for Rust hosts and Tauri.

### C ABI

Stable handle-based API for non-Rust native consumers.

### JavaScript/WASM API

Promise-based, event-driven API with binary buffers.

### Optional framework bindings

React/Vue/Svelte wrappers belong outside the core SDK.

## 2. Rust example

```rust
use casual_doc_sdk::{
    Engine, EngineConfig, OpenOptions, Viewport, CommandArgs,
};

let engine = Engine::new(EngineConfig::default())?;
let session = engine.open_docx(docx_bytes, OpenOptions::default()).await?;

let subscription = session.subscribe(|event| {
    println!("{event:?}");
});

session.set_viewport(Viewport {
    width_px: 1200.0,
    height_px: 900.0,
    scale: 1.0,
    scroll_y: 0.0,
})?;

session.execute("text.insert", CommandArgs::text("Hello"))?;
let bytes = session.save_docx(Default::default()).await?;
```

## 3. JavaScript/WASM example

```ts
import { DocumentEngine } from "@casualoffice/document-runtime";

const engine = await DocumentEngine.create({
  locale: "en-US",
  externalResources: "deny",
});

const session = await engine.openDocx(fileBytes);

session.on("transactionCommitted", event => {
  console.log(event.revision);
});

session.setViewport({
  width: canvas.width,
  height: canvas.height,
  scale: devicePixelRatio,
  scrollY: 0,
});

canvas.addEventListener("pointerdown", e => {
  session.handlePointer({
    kind: "down",
    x: e.offsetX,
    y: e.offsetY,
    button: e.button,
    modifiers: readModifiers(e),
  });
});

await session.execute("format.toggle_bold");
const saved = await session.saveDocx();
```

## 4. Core facade

```rust
pub struct Engine { /* opaque */ }

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self, SdkError>;
    pub async fn open_docx(
        &self,
        bytes: Bytes,
        options: OpenOptions,
    ) -> Result<DocumentSession, SdkError>;
    pub fn create_blank(
        &self,
        options: NewDocumentOptions,
    ) -> Result<DocumentSession, SdkError>;
    pub fn register_plugin(
        &self,
        plugin: Arc<dyn Plugin>,
    ) -> Result<(), SdkError>;
}
```

## 5. Session API

```rust
impl DocumentSession {
    pub fn id(&self) -> SessionId;
    pub fn revision(&self) -> RevisionId;
    pub fn metadata(&self) -> DocumentMetadataSnapshot;

    pub fn execute(
        &self,
        command: impl Into<CommandId>,
        args: CommandArgs,
    ) -> Result<CommandResult, SdkError>;

    pub fn query_command(
        &self,
        command: impl Into<CommandId>,
    ) -> Result<CommandState, SdkError>;

    pub fn selection(&self) -> Result<SelectionSnapshot, SdkError>;
    pub fn set_selection(&self, request: SetSelectionRequest) -> Result<(), SdkError>;

    pub fn handle_key(&self, event: KeyEvent) -> Result<InputResult, SdkError>;
    pub fn handle_pointer(&self, event: PointerEvent) -> Result<InputResult, SdkError>;
    pub fn handle_ime(&self, event: ImeEvent) -> Result<InputResult, SdkError>;

    pub fn set_viewport(&self, viewport: Viewport) -> Result<(), SdkError>;
    pub async fn scene(&self, request: SceneRequest) -> Result<SceneSnapshot, SdkError>;

    pub async fn save_docx(
        &self,
        options: SaveOptions,
    ) -> Result<Bytes, SdkError>;

    pub fn snapshot(&self) -> Result<DocumentSnapshot, SdkError>;
    pub fn subscribe(
        &self,
        listener: EventListener,
    ) -> Subscription;

    pub fn close(self) -> Result<(), SdkError>;
}
```

## 6. Command conventions

Command IDs use namespaces:

- `document.*`
- `text.*`
- `format.*`
- `paragraph.*`
- `list.*`
- `table.*`
- `image.*`
- `section.*`
- `comment.*`
- `review.*`
- `history.*`
- `view.*`
- `plugin.<vendor>.*`

Commands accept versioned structured arguments. Unknown optional fields are ignored; unknown required schema versions fail.

## 7. Initial command catalog

### Document

- `document.select_all`
- `document.insert_page_break`
- `document.insert_section_break`
- `document.set_page_setup`

### Text and format

- `text.insert`
- `text.delete_backward`
- `text.delete_forward`
- `format.toggle_bold`
- `format.toggle_italic`
- `format.toggle_underline`
- `format.set_font_family`
- `format.set_font_size`
- `format.set_color`
- `format.clear`

### Paragraph

- `paragraph.set_alignment`
- `paragraph.set_line_spacing`
- `paragraph.set_spacing`
- `paragraph.increase_indent`
- `paragraph.decrease_indent`
- `paragraph.apply_style`

### Lists

- `list.toggle_bulleted`
- `list.toggle_numbered`
- `list.indent`
- `list.outdent`

### Tables

- `table.insert`
- `table.add_row`
- `table.add_column`
- `table.delete_row`
- `table.delete_column`
- `table.merge_cells`
- `table.split_cell`

### History

- `history.undo`
- `history.redo`

## 8. Snapshots and handles

Public snapshots are immutable. Native hosts may receive zero-copy references scoped by a guard. WASM and C consumers receive owned binary or plain value snapshots.

Never expose internal pointers without lifetime-safe wrappers.

## 9. Event delivery

The implemented Phase 0 transport is a future-only synchronous subscription over
a 256-event bounded journal. Each `SequencedEvent` has a session-local sequence.
`EventBatch::dropped_events` reports exact lag instead of silently skipping
events. Transaction events precede any selection event caused by the same
commit.

The target v1 adapters additionally support:

- async stream;
- callback;
- JavaScript EventTarget-style bridge.

Hosts can request event coalescing for high-frequency layout and selection events.

## 10. Resource requests

```rust
pub enum ResourceRequest {
    Font(FontRequest),
    Image(ImageRequest),
    ExternalRelationship(ExternalRelationshipRequest),
    Dictionary(DictionaryRequest),
}
```

The host responds asynchronously. Resource identity includes content hash/version to make cache invalidation explicit.

## 11. Plugin API

```rust
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn register(&self, registry: &mut PluginRegistry) -> Result<(), PluginError>;
}
```

Registration capabilities:

- commands;
- validators;
- decorations;
- custom object codecs;
- custom scene emitters;
- import/export handlers;
- document inspectors;
- semantic providers.

v1 plugins are trusted native code. WASM-sandboxed plugins are a separate roadmap item.

## 12. Compatibility policy

- patch releases: fixes only, no breaking API;
- minor releases: additive API;
- major releases: breaking API allowed with migration guide;
- deprecated APIs remain for at least two minor releases when practical;
- serialized schemas have independent compatibility guarantees.

## 13. Distribution

Proposed artifacts:

- crates.io Rust crates;
- npm WASM package;
- prebuilt native libraries for major desktop targets;
- C headers;
- generated TypeScript definitions;
- API docs site;
- sample apps;
- conformance fixtures.
