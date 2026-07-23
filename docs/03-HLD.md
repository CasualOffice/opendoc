# High-Level Design

## 1. Rust workspace

```text
casual-docs-runtime/
├── crates/
│   ├── casual-doc-sdk
│   ├── casual-doc-model
│   ├── casual-doc-transaction
│   ├── casual-doc-selection
│   ├── casual-doc-style
│   ├── casual-doc-layout
│   ├── casual-doc-scene
│   ├── casual-doc-render
│   ├── casual-doc-render-skia
│   ├── casual-doc-render-web
│   ├── casual-doc-ooxml
│   ├── casual-doc-collab
│   ├── casual-doc-plugin
│   ├── casual-doc-accessibility
│   ├── casual-doc-diagnostics
│   ├── casual-doc-ffi
│   └── casual-doc-wasm
├── bindings/
│   ├── javascript
│   ├── c
│   └── tauri
├── apps/
│   ├── minimal-tauri-editor
│   ├── minimal-web-editor
│   ├── doc-inspect-cli
│   └── render-cli
├── fixtures/
├── fuzz/
├── benches/
└── docs/
```

## 2. Component responsibilities

### `casual-doc-sdk`

Stable facade. Creates sessions, loads/saves documents, exposes commands/events/snapshots, and hides internal crate types.

### `casual-doc-model`

Normalized document tree, IDs, attributes, style references, extension bags, immutable snapshots.

### `casual-doc-transaction`

Operations, transaction builder, validation, application, mapping, inverses, revision management.

### `casual-doc-selection`

Selections, carets, navigation, anchor mapping, IME state.

### `casual-doc-style`

Style inheritance, direct formatting, themes, units, computed style cache.

### `casual-doc-layout`

Text shaping abstraction, paragraphs, lists, tables, sections, pagination, floats, hit testing, incremental invalidation.

### `casual-doc-scene`

Backend-neutral display list and semantic overlay.

### `casual-doc-render-*`

Reference rendering backends.

### `casual-doc-ooxml`

ZIP package security, XML parser/writer, WordprocessingML/DrawingML mapping, relationships, media, preservation.

### `casual-doc-collab`

Adapter traits, serialized operations, remote cursors, conflict metadata.

### `casual-doc-plugin`

Registries and extension capabilities.

### `casual-doc-ffi` and `casual-doc-wasm`

Language-safe handles, serialization boundaries, callback/event bridges, memory lifecycle.

## 3. Runtime objects

```text
Engine
 ├── EngineConfig
 ├── ResourceRegistry
 ├── PluginRegistry
 └── creates DocumentSession

DocumentSession
 ├── DocumentStore
 ├── TransactionManager
 ├── SelectionManager
 ├── HistoryManager
 ├── LayoutCoordinator
 ├── EventBus
 ├── Diagnostics
 └── optional CollaborationAdapter
```

## 4. Load flow

```text
Host bytes
  -> format sniff
  -> bounded package reader
  -> parse package parts
  -> OOXML normalization
  -> validate model
  -> build indexes
  -> create revision 0
  -> request required resources
  -> initial layout of visible pages
  -> emit Ready
```

Loading is asynchronous and cancellable. Progressive load may be added after v1.

## 5. Edit flow

```text
Pointer/key/IME event
  -> host input adapter
  -> SDK command
  -> command handler
  -> transaction builder
  -> validation
  -> apply atomically
  -> update anchors/selections/history
  -> invalidate style/layout
  -> schedule incremental layout
  -> publish events
  -> render new scene
```

## 6. Render flow

```text
Viewport request
  -> ensure page fragments
  -> build scene for viewport
  -> host renderer submits display list
  -> overlays/caret/composition added
  -> accessibility tree updated
```

## 7. Save flow

```text
Session snapshot
  -> normalize/check invariants
  -> OOXML projection
  -> merge preserved extension data
  -> write relationships/media
  -> deterministic ZIP package
  -> validation report
  -> bytes/stream to host
```

## 8. Public host interfaces

- `FontProvider`
- `ImageProvider`
- `ClipboardProvider`
- `ExternalResourcePolicy`
- `Logger/TraceSink`
- `Clock`
- `RandomSource`
- `StorageProvider` only for optional autosave
- `CollaborationAdapter`
- `RenderSurface`
- `AccessibilityBridge`

## 9. Host integrations

### Tauri

Rust core linked directly. Tauri commands should be used only where frontend UI needs access. Prefer shared memory/texture or compact display-list transfer over per-glyph JSON.

### WebAssembly

WASM owns the session. JavaScript sends compact command objects and input events. Rendering options:

1. WASM renders directly to canvas;
2. WASM emits binary display lists consumed by JavaScript;
3. WebGPU renderer later.

Use transferable buffers for import/export and binary events.

### Headless

CLI/service uses the same SDK for:

- DOCX validation;
- rendering thumbnails;
- converting;
- extracting outline/text;
- applying DocOps;
- generating PDFs;
- regression testing.

## 10. Persistence

The runtime does not define a cloud storage product.

It supports:

- complete snapshot;
- DOCX bytes;
- normalized binary snapshot;
- operation log;
- optional checkpoint plus operations.

Host applications decide persistence and encryption.

## 11. Failure behavior

Errors are typed and categorized:

- malformed input;
- unsupported feature;
- resource unavailable;
- policy denied;
- cancelled;
- out of memory/resource limit;
- plugin failure;
- renderer failure;
- internal invariant violation.

Unsupported content should usually produce warnings and preservation data, not hard failure.
