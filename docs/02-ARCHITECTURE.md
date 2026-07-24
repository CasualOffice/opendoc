# Target Architecture

## 1. Architectural style

The system is a **headless document engine with host adapters**.

```text
+---------------------------------------------------------------+
| Host Application                                               |
| Tauri | Browser | Electron alternative | Server | CLI | Mobile |
+------------------------------+--------------------------------+
                               |
                    Stable SDK / FFI / WASM API
                               |
+---------------------------------------------------------------+
| SDK Facade                                                      |
| Sessions | Commands | Events | Capabilities | Resources         |
+---------------------------------------------------------------+
| Document Runtime                                                |
| Model | Transactions | Selection | History | Anchors | Semantics|
+---------------------------------------------------------------+
| Layout Runtime                                                  |
| Shaping | Line break | Pagination | Tables | Floats | Hit test  |
+---------------------------------------------------------------+
| Scene / Display List                                            |
| Text runs | Paths | Images | Clips | Layers | Decorations       |
+---------------------------------------------------------------+
| Render Backends                                                 |
| Native GPU/2D | Web Canvas/WebGPU | Headless raster/PDF          |
+---------------------------------------------------------------+

Cross-cutting:
DOCX/ODT/MD/TXT import-export | Collaboration adapters | Plugins
Diagnostics | Font service | Image service | Accessibility
```

## 2. Design principles

### Engine state is authoritative

The DOM, React state, Tauri state, and canvas state are projections. They never become the canonical document.

### Commands produce transactions

UI actions call commands. Commands create validated transactions. Transactions mutate the model and produce invalidation information.

### Layout is incremental

An edit invalidates the smallest safe layout scope. The engine prioritizes visible pages and may complete distant layout asynchronously.

### Rendering is retained and backend-neutral

Layout generates a display list. A renderer consumes that display list. Rendering code does not inspect DOCX XML or mutate the document.

### Import preserves intent and unknown data

DOCX import maps recognized structures into normalized nodes while retaining
bounded source provenance and safely preservable unsupported content outside
the live editing model. Candidate ADR-027 proposes an immutable source snapshot,
typed preservation ledger, and one import/export mapping registry; see
`34-OOXML-FIDELITY-ARCHITECTURE.md`.

### Host controls policy

The host decides:

- where files are stored;
- whether external links/resources are allowed;
- which commands are exposed;
- authentication;
- network access;
- plugin trust;
- telemetry;
- collaboration provider.

### Public API is narrower than internal architecture

Internal crates may evolve. The SDK facade exposes stable IDs, value objects, commands, snapshots, events, and handles.

## 3. Main subsystems

### Document model

Stores semantic structure:

- document;
- sections;
- headers/footers;
- paragraphs;
- runs;
- tables;
- notes;
- drawing objects;
- comments;
- bookmarks;
- styles;
- numbering;
- relationships;
- metadata;
- extension data.

### Transaction engine

Validates and applies operations atomically. Produces:

- new revision;
- inverse operations;
- changed ranges;
- style invalidations;
- layout invalidations;
- event payloads.

### Position and anchor engine

Uses stable node IDs plus local offsets. Anchors survive ordinary edits through transaction mapping.

### Selection engine

Maintains logical selection, direction, affinity, table selections, and IME composition ranges.

### Layout engine

Transforms semantic nodes into paginated fragments using:

- resolved styles;
- shaped text;
- line boxes;
- block fragments;
- table fragments;
- page/column containers;
- floating object constraints.

### Scene builder

Converts layout fragments plus decorations into a renderer-neutral display list.

### Import/export

DOCX package reader/writer, source-shaped OOXML decoding, relationships, media,
style resolution, numbering, themes, provenance, typed preservation, and a
versioned bidirectional mapping registry.

### Resource services

Abstract:

- fonts;
- images;
- hyperlinks;
- dictionaries;
- locale;
- clipboard;
- file attachments.

### Collaboration adapters

Translate runtime transactions and anchors to an external synchronization mechanism.

### Plugin runtime

Registers trusted in-process extensions. Sandboxed plugin execution is a later phase.

## 4. Data ownership

- `DocumentSession` owns mutable runtime state.
- immutable snapshots may be shared across threads;
- layout jobs consume immutable document snapshots;
- render snapshots are immutable;
- resource providers are host-owned interfaces;
- renderer surfaces are host-owned;
- plugins cannot directly mutate internal collections.

## 5. Threading model

Recommended:

- UI thread: input events, command invocation, current visible render submission;
- document actor: serialized transaction application;
- layout worker pool: shaping, page layout, scene generation;
- I/O workers: DOCX load/save, image decode, font discovery;
- collaboration worker: network adapter;
- render thread: backend-specific.

WASM single-thread mode remains supported. Optional WASM threads can be enabled where cross-origin isolation is available.

## 6. Compatibility with current Casual Docs

The existing web editor remains operational during the rewrite.

Migration strategy:

1. extract fixtures, behavioral tests, DOCX corpus, and round-trip rules;
2. publish the Rust SDK as a parallel project;
3. use it first for conversion/inspection and headless tests;
4. introduce a native Tauri editor using the SDK;
5. introduce WASM runtime behind a feature flag;
6. move capabilities incrementally;
7. avoid a one-shot rewrite.

Existing Yjs collaboration should be retained through an adapter or bridge during migration.

## 7. Security boundary

Untrusted documents are parsed inside bounded, cancellable import jobs.

Default policies:

- no automatic URL fetching;
- no macro execution;
- no embedded executable activation;
- no unrestricted font loading;
- no plugin loading from document packages;
- no HTML/script execution;
- relationship and zip paths normalized;
- size and nesting limits enforced.

## 8. Versioning

Four independent versions:

- SDK API version;
- normalized document schema version;
- transaction/operation schema version;
- file compatibility profile version.

A runtime may support multiple older serialized schema versions through migrations.
