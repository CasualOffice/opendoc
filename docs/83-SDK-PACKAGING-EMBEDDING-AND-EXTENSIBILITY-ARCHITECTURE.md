# 83 — SDK Packaging, Embedding, Collaboration, MCP & Extensibility Architecture

**Status:** Approved Architectural Specification  
**Date:** 2026-07-31  
**Depends on:** docs 05, 14, 15, 45, 56, 57, 59, 63, 68  
**Primary Implementation:** Rust (`crates/casual-doc-sdk`, `crates/casual-doc-wasm`)  
**NPM Package:** `@casualoffice/document-runtime`  

---

## 1. Overview & Architectural Goals

OpenDoc is designed as a **deterministic, headless document engine** written in Rust, compiled to **WebAssembly (WASM)**, and exposed to host environments through stable TypeScript, Rust, and C ABI interfaces.

This specification details the architecture and in-depth implementation plan for distributing OpenDoc as an **embeddable, customizable SDK** that third-party developers can install via `npm install @casualoffice/document-runtime` to embed high-fidelity DOCX previewers, single-user editors, multiplayer co-editing, MCP AI agent tools, and custom plugin extensions into their applications.

---

## 2. Read-Only / Preview Mode Architecture

### 2.1 Principles
In Read-Only / Preview mode (`{ readOnly: true }`), the runtime acts strictly as a **layout and rendering engine**. All mutation entry points, caret blink loops, keyboard editing shortcuts, and IME listeners are completely bypassed.

```
+-----------------------------------------------------------------------+
|                             Host Web App                              |
|  +-----------------------------------------------------------------+  |
|  |                <CanvasView readOnly={true} />                   |  |
|  +-----------------------------------------------------------------+  |
+-----------------------------------||----------------------------------+
                                    || (Pointer & Scroll Events)
                                    \/
+-----------------------------------------------------------------------+
|                    @casualoffice/document-runtime                     |
|                                                                       |
|  +---------------------+   +-------------------+   +---------------+  |
|  | DocumentSession     |   | LayoutNG Engine   |   | Render Pipeline| |
|  | (Mutation Disabled) |-->| (Pagination/Reflow|-->| (Canvas2D /   |  |
|  +---------------------+   +-------------------+   |  WebGL Paint) |  |
|                                                    +---------------+  |
+-----------------------------------------------------------------------+
```

### 2.2 Core Read-Only Capabilities
1. **Virtualized Canvas Viewport:** Renders only visible pages plus a 1-page offscreen buffer, capping memory consumption at <100MB even for 1,000+ page documents.
2. **Page Navigation & Zoom:** Supports arbitrary scale factors (25% to 500%), `fit-width`, `fit-page`, and multi-column continuous scrolling.
3. **Interactive Selection & Clipboard (`⌘C`):** Computes engine-drawn highlight polygons from `selectionRects(range)` and extracts structured plain text/rich text via `copyText(range)` without exposing raw DOM nodes.
4. **Document Outline & Search:** Emits structured table-of-contents trees (`documentOutline()`) and executes high-speed document-wide text searches, emitting exact page-relative bounding boxes for match highlighting.
5. **Security & Sandbox:** All external fonts and hyperlink activations pass through host-owned allowlists. Zero external network requests are made by the WASM module directly.

---

## 3. Transactional Single-User & Co-Editing Architecture

### 3.1 Operational Invariants
Co-editing (real-time multiplayer) and single-user transactional editing are built on the four foundational invariants defined in [docs/45-EXTENSIBILITY-AND-COLLABORATION-SEAMS.md](file:///Users/sachin/Desktop/melp/services/opendoc-fixes/docs/45-EXTENSIBILITY-AND-COLLABORATION-SEAMS.md):

```rust
// Invariant I1: Single Mutation Choke Point
pub trait ExecutionContext {
    fn apply(&mut self, op: Operation) -> Result<OperationInverse, SdkError>;
}
```

* **Invariant I1 (Single Mutation Choke Point):** Every edit (keystroke, remote collaborator, AI agent) MUST route through `casual_doc_edit::apply`.
* **Invariant I2 (Closed, Invertible Ops):** `Operation` is a closed, serializable enum where every variant has an exact deterministic `inverse()`.
* **Invariant I3 (Stable Anchor Identity):** Block and inline nodes key on 128-bit `NodeId` anchors, insulating operation offsets from global document array index shifts.

### 3.2 Real-Time Collaboration Adapter (`@casualoffice/collaboration-yjs`)
Host platforms enable co-editing by binding the OpenDoc transaction event journal (`SequencedEvent`) to a CRDT / OT sync layer:

```
 [ Local User ]                                 [ Remote Peer ]
       |                                               |
  (Keystroke)                                    (Remote Op)
       v                                               v
+--------------+     SequencedEvent      +---------------------------+
| Local Session| ----------------------> | @casualoffice/            |
| .apply(op)   |                         | collaboration-yjs Adapter |
+--------------+                         +---------------------------+
       |                                               |
       | Transformed Op                                v
       +-----------------------------------> [ Sync Provider / Server ]
```

1. **Transaction Event Streaming:** Each committed transaction emits a `SequencedEvent` carrying the committed revision, operation delta, and affected `NodeId` anchors.
2. **Operational Transformation / CRDT Rebase:** The Yjs/Automerge adapter transforms incoming remote ops against local pending ops using `transform(op_a, op_b)`.
3. **Remote Presence & Carets:** Remote user selection ranges and carets are rendered as non-mutating visual overlay layers using custom user colors and names.

---

## 4. Extensibility & Plugin Architecture

### 4.1 Headless UI Bindings (`@casualoffice/react`)
The SDK provides React/Vue reactive hooks without enforcing any specific UI components:

```tsx
import { useDocumentSession, useCommandState, useSelection } from "@casualoffice/react";

export function CustomBoldButton() {
  const { session } = useDocumentSession();
  const { active, enabled } = useCommandState("format.toggle_bold");

  return (
    <button
      disabled={!enabled}
      className={active ? "is-active" : ""}
      onClick={() => session.execute("format.toggle_bold")}
    >
      Bold
    </button>
  );
}
```

### 4.2 Plugin Registration API
Developers can extend engine capabilities by registering custom plugins into `DocumentEngine`:

```rust
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn register(&self, registry: &mut PluginRegistry) -> Result<(), PluginError>;
}
```

Capabilities available to plugins:
* **Custom Commands:** Register new command identifiers and execution handlers.
* **Document Inspectors & Validators:** Enforce strict organizational formatting rules or compliance schemas.
* **Scene & Render Decorations:** Inject custom highlights, background shapes, or interactive canvas overlays.
* **Custom Node Codecs:** Import/export custom inline objects or block widgets.

---

## 5. MCP (Model Context Protocol) & AI Agent Integration

### 5.1 Overview
OpenDoc provides native support for AI agents (Claude, Gemini, custom LLMs) via an official Model Context Protocol (MCP) server package (`@casualoffice/mcp-server`).

### 5.2 MCP Tool Capabilities
The MCP server exposes standard tools over the SDK boundary:

| MCP Tool Name | Description | SDK Primitive |
| :--- | :--- | :--- |
| `read_document_snapshot` | Reads full or section-bounded document model as JSON/Markdown | `session.snapshot()` |
| `search_document_content` | Performs keyword or semantic search over document text | `session.search(query)` |
| `apply_document_edits` | Applies structured text insertions, deletions, or formatting | `session.apply(op)` |
| `add_document_annotation` | Attaches AI review comments, suggestions, or inline diffs | `session.addComment()` |

### 5.3 AI Sidecar Metadata Isolation (Invariant I4)
All AI-derived data (vector embeddings, semantic chunking indices, RAG metadata, and unaccepted AI revision suggestions) are stored in an **auxiliary sidecar database keyed by `NodeId`**. 

This isolation guarantees that:
* Original `.docx` files remain 100% compliant with standard Microsoft Word schema specifications.
* AI operations never cause silent document data loss.
* Users can review, accept, or reject AI-proposed changes visually in the document canvas.

---

## 6. Phase-Wise Implementation Plan & Exit Gates

### Phase 1: WASM Core & SDK Workspace (Weeks 1–3)
* **Tasks:** Set up `@casualoffice/document-runtime` monorepo workspace; optimize WASM binary size (<15MB); implement WebGL2/Canvas2D target bindings; set up CJK & Indic font provisioning.
* **Exit Gate:** `npm run build` generates clean TypeScript typings (`.d.ts`) and WASM binaries initializing in under 50ms.

### Phase 2: Read-Only / Preview SDK Surface (Weeks 4–6)
* **Tasks:** Build virtualized multi-page scroll engine; implement smooth zoom, text selection, copy-to-clipboard, document outline extraction, and search highlighting.
* **Exit Gate:** 100-page DOCX document scrolls continuously at 60 FPS in read-only mode with <100MB RAM usage.

### Phase 3: Transactional Editing SDK Surface (Weeks 7–10)
* **Tasks:** Implement keyboard/IME input handlers, caret blinking geometry, command dispatcher (`session.execute()`), queryable command state API, undo/redo stack, and `.docx` file export.
* **Exit Gate:** Complete edit cycle (load → edit → undo → save → reopen) passes 100% of semantic round-trip tests.

### Phase 4: Co-Editing & Collaboration Layer (Weeks 11–14)
* **Tasks:** Develop `@casualoffice/collaboration-yjs` adapter; implement `transform(op_a, op_b)` concurrent operation rebasing; render remote carets/selections; build offline sync queue.
* **Exit Gate:** 5 concurrent clients typing simultaneously converge to identical document snapshots without operational errors.

### Phase 5: Customization & Plugin Architecture (Weeks 15–18)
* **Tasks:** Build `@casualoffice/react` and `@casualoffice/vue` headless bindings; design plugin registration framework (`engine.registerPlugin()`); build modular UI component library.
* **Exit Gate:** Third-party developer can build a custom editor with custom toolbar buttons and custom validation rules using only public SDK APIs.

### Phase 6: MCP Server & AI Agent Tools (Weeks 19–21)
* **Tasks:** Implement `@casualoffice/mcp-server`; expose MCP tools (`read_document_snapshot`, `apply_document_edits`); build `NodeId`-keyed AI sidecar store (Invariant I4).
* **Exit Gate:** LLM agent inspects document via MCP tools and applies valid formatting edits with 0% schema violations.

### Phase 7: Developer Portal, CI Gates & NPM Release (Weeks 22–24)
* **Tasks:** Set up automated visual regression tests (Playwright) and benchmark gates; launch interactive documentation portal with live CodeSandbox demos; publish `@casualoffice/document-runtime` to npm.
* **Exit Gate:** NPM package published, passing 100% of CI release gates with green status.
