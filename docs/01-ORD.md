# Outcome Requirements Document (ORD)

## 1. Product statement

Build an embeddable, Rust-based document runtime and SDK that enables CasualOffice and third-party developers to create native, web, desktop, and headless document editors without rebuilding document parsing, editing, layout, rendering, selection, undo, and export.

## 2. Problem

Existing browser-first editors typically couple the editor model to DOM behavior, framework state, or a specific rich-text toolkit. This creates limitations:

- inconsistent pagination and text layout;
- difficult DOCX fidelity;
- framework lock-in;
- duplicated logic between desktop and web;
- weak headless support;
- fragile selection and input handling;
- hard-to-control performance on large documents;
- UI and engine changes becoming inseparable.

The current Casual Docs repository already has a capable browser editor, DOCX parser/serializer, paginated layout, collaboration, and React component. The new runtime must preserve learned behavior and compatibility while creating a reusable, native-grade engine boundary.

## 3. Desired outcomes

### O1 — Embeddable engine

A developer can initialize the runtime, load a document, render it, apply commands, observe events, and save it without using Casual Docs UI.

### O2 — Shared native and web behavior

The same core transaction, document, layout, and import/export logic runs in:

- Tauri desktop;
- browser via WebAssembly;
- headless CLI/service;
- automated test harness.

### O3 — DOCX fidelity

The runtime supports loss-aware reading and writing of WordprocessingML, retaining unsupported but preservable XML where practical.

### O4 — Predictable performance

The runtime remains interactive on normal office documents and degrades gracefully on very large documents.

### O5 — Host freedom

Consumers can build their own ribbon, toolbar, command palette, comments panel, style inspector, AI panel, or minimal writing interface.

### O6 — Safe extensibility

Consumers can add commands, panels, decorations, custom inline/block objects, import/export handlers, and collaboration adapters without forking the engine.

## 4. Personas

### SDK integrator

Builds a document editor inside a SaaS product, records system, case management system, knowledge platform, or vertical workflow.

### CasualOffice product engineer

Builds the official Tauri desktop editor and web editor.

### Plugin author

Adds domain objects, commands, annotations, AI operations, templates, or compliance tooling.

### Infrastructure engineer

Runs headless conversion, validation, rendering, indexing, thumbnail generation, or automated document processing.

## 5. Functional requirements

### FR-1 Document lifecycle

The SDK shall support:

- create blank document;
- load from DOCX bytes;
- load from normalized JSON/CBOR;
- save to DOCX;
- export normalized format;
- inspect document metadata;
- close and release resources safely.

### FR-2 Editing

The SDK shall support transactions for:

- text insertion and deletion;
- paragraph operations;
- character and paragraph formatting;
- lists and numbering;
- tables;
- images and anchored objects;
- sections;
- page and column breaks;
- headers and footers;
- comments;
- bookmarks;
- tracked changes;
- links;
- fields;
- footnotes and endnotes.

### FR-3 Selection and input

The SDK shall support:

- caret;
- range selection;
- multi-range selection later;
- pointer hit testing;
- keyboard navigation;
- grapheme-aware movement;
- word and paragraph movement;
- IME composition;
- clipboard model operations;
- drag selection;
- table cell selection.

### FR-4 History

The SDK shall provide:

- atomic transactions;
- undo and redo;
- transaction grouping;
- command metadata;
- optional persistent operation log;
- integration points for collaborative undo.

### FR-5 Layout

The SDK shall support:

- paginated layout;
- continuous layout as a host option;
- page size and margins;
- sections;
- columns;
- headers and footers;
- line breaking;
- tabs;
- floating and inline objects;
- tables;
- widow/orphan controls;
- keep-with-next;
- page-break-before;
- incremental relayout.

### FR-6 Rendering

The SDK shall expose a backend-neutral display list or scene graph and provide reference renderers for:

- native GPU/2D rendering;
- WebAssembly canvas;
- headless raster/PDF path.

### FR-7 Events

The SDK shall emit typed events for:

- document changed;
- selection changed;
- layout invalidated;
- pages changed;
- command state changed;
- save state changed;
- resource required;
- warning;
- recoverable error;
- fatal error.

### FR-8 Collaboration

The engine shall not depend on Yjs or a specific CRDT. It shall expose:

- transaction serialization;
- stable positions/anchors;
- operation application;
- presence decorations;
- remote transaction metadata;
- adapter hooks.

Initial adapters may include Yjs compatibility for the existing web stack and a future native CRDT.

### FR-9 Extensions

The SDK shall support:

- command registration;
- custom block/inline object registration;
- custom annotations/decorations;
- import/export extensions;
- resource providers;
- spell/grammar providers;
- AI/DocOps operations;
- host capability registration.

### FR-10 Accessibility

The host integration shall be able to expose document semantics to platform accessibility APIs. The engine shall expose semantic structure independently of pixels.

## 6. Non-functional requirements

### NFR-1 Determinism

Given the same engine version, normalized document, fonts, layout configuration, and resources, page layout and display-list output shall be reproducible.

### NFR-2 Performance targets

Initial targets on a modern laptop:

- engine startup: under 100 ms excluding dynamic library/WASM download;
- load 100-page normal DOCX: under 2 seconds;
- normal typing transaction: p95 under 16 ms;
- incremental relayout after local edit: p95 under 50 ms for visible pages;
- pointer hit test: p95 under 2 ms;
- save 100-page normal DOCX: under 2 seconds;
- idle memory for a 100-page document: under 250 MB.

These are release gates, not promises for every document.

### NFR-3 Safety

- no unsafe Rust in public crates unless documented and reviewed;
- bounded parsing;
- zip-bomb protection;
- image dimension limits;
- external resource access disabled by default;
- deterministic cancellation and cleanup.

### NFR-4 Portability

The core shall compile for:

- macOS;
- Windows;
- Linux;
- `wasm32-unknown-unknown`.

### NFR-5 API stability

Public API follows semantic versioning. Serialized document and operation formats have explicit schema versions.

### NFR-6 Observability

The runtime provides structured diagnostics, timings, warnings, and optional traces without forcing a telemetry backend.

## 7. Out of scope for v1

- full spreadsheet or slide model;
- VBA/macros;
- exact Microsoft Word bug compatibility;
- embedded browser UI;
- mandatory cloud collaboration service;
- full Office add-in compatibility;
- arbitrary HTML/CSS editing;
- native mobile UI;
- perfect fidelity for every legacy VML construct.

## 8. Success metrics

- official Tauri editor uses the SDK without bypassing its transaction model;
- web demo uses the same Rust core through WASM;
- at least one external sample integration with no CasualOffice UI dependency;
- 95%+ pass rate on the defined DOCX corpus;
- zero data-loss regressions in supported round-trip tags;
- public SDK documentation and examples are sufficient to build a minimal editor;
- stable beta API published under the repository's MIT license.

## 9. Acceptance definition for v1

A third-party developer can:

1. add the SDK;
2. create a host surface;
3. load a DOCX;
4. display pages;
5. click to place a caret;
6. type and format text;
7. undo and redo;
8. save a valid DOCX;
9. subscribe to changes;
10. register one custom command and one custom decoration.
