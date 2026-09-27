# 83 — SDK Packaging, Embedding, Collaboration, MCP and Extensibility Architecture

**Status:** Approved Architectural Specification  
**Date:** 2026-07-31  
**Depends on:** docs 05, 14, 15, 45, 56, 57, 59, 63, 68  
**Primary Implementation:** Rust (`crates/casual-doc-sdk`, `crates/casual-doc-wasm`)  
**NPM Package:** `@casualoffice/opendoc-embed` — what actually ships today.
`@casualoffice/document-runtime` appears below as the FUTURE full-runtime package name and
is **not published**; §Phase 3's task list is where it gets reserved. Corrected 2026-09-27:
this field, and the install line in the overview, both named the unpublished one in the
present tense, so a reader following this document ran `npm install` against nothing.  

> **Status correction (2026-09-27):** the MCP/AI wording originally in section 5
> and Phase 6 described an aspiration as if it already shipped. OpenDoc does **not**
> currently provide an MCP server, semantic index, embedded AI model, or document-
> assistance API. Doc 132 and proposed ADR-035 are authoritative for that experimental
> scope. Nothing there is accepted, implemented, supported, or committed to v1.

---

## 1. Overview & Architectural Goals

OpenDoc is designed as a **deterministic, headless document engine** written in Rust, compiled to **WebAssembly (WASM)**, and exposed to host environments through stable TypeScript, Rust, and C ABI interfaces.

This specification describes the long-term architecture for distributing OpenDoc
as an **embeddable, customizable SDK**. The repository currently builds
`@casualoffice/opendoc-embed` and verifies its tarball; it does not prove that a
public registry release exists. The custom element, capability contract, host
commands/events, and white-label seams are implemented. Multiplayer co-editing,
document assistance, and MCP remain separate experimental designs and are not
implemented or supported.

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
Co-editing (real-time multiplayer) and single-user transactional editing are
designed around the four foundational invariants defined in
[doc 45](45-EXTENSIBILITY-AND-COLLABORATION-SEAMS.md):

```rust
// Invariant I1: Single Mutation Choke Point
pub trait ExecutionContext {
    fn apply(&mut self, op: Operation) -> Result<OperationInverse, SdkError>;
}
```

* **Invariant I1 (Single Mutation Choke Point):** Every edit (keystroke, remote collaborator, AI agent) MUST route through `casual_doc_edit::apply`.
* **Invariant I2 (Closed, Invertible Ops):** `Operation` is a closed, serializable enum where every variant has an exact deterministic `inverse()`.
* **Invariant I3 (Stable Anchor Identity):** Block and inline nodes key on 128-bit `NodeId` anchors, insulating operation offsets from global document array index shifts.

### 3.2 Proposed provider-neutral collaboration boundary

No collaboration adapter package ships today, and no Yjs package name is
reserved. The current proposal binds the unified OpenDoc transaction journal to
a versioned `CollaborationAdapter`; a host may implement that port using the
reference relay or another provider that passes the same conformance suite.

```
 [ Local User ]                                 [ Remote Peer ]
       |                                               |
  (Keystroke)                                    (Remote Op)
       v                                               v
+--------------+     SequencedEvent      +---------------------------+
| Local Session| ----------------------> | CollaborationAdapter      |
| .apply(op)   |                         | (host/provider selected)  |
+--------------+                         +---------------------------+
       |                                               |
       | Transformed Op                                v
       +-----------------------------------> [ Sync Provider / Server ]
```

1. **Transaction Event Streaming:** Each committed transaction emits a `SequencedEvent` carrying the committed revision, operation delta, and affected `NodeId` anchors.
2. **Operational Transformation:** proposed ADR-033 selects relay-ordered OT over
   revisioned transactions. A CRDT remains possible later behind the same port;
   it is not the first implementation.
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

## 5. Experimental Document Assistance, Semantic Search, and MCP

### 5.1 Current status and product boundary

This capability is **not implemented or supported**. The earlier package name and raw
tool list were conceptual placeholders, not released APIs.

The proposed architecture begins with user jobs: understand and summarize; rewrite,
proofread, translate, shorten, or expand an explicit selection; transform text, lists,
and tables; apply deterministic formatting; add anchored review findings; and execute
larger workflows through an inspectable plan. A shared Document Assistance Layer owns
scope resolution, bounded context, provider policy, proposal validation, preview, and
commit. UI features, host integrations, local models, and external agents use that
same layer.

### 5.2 Semantic retrieval and sidecar isolation

Structure-aware chunks, embeddings, summaries, provider metadata, and unaccepted
proposals belong in a rebuildable sidecar keyed by `NodeId` and range—not in the
normalized document or OOXML preservation envelope. Browser-local profiles run
document-size work in Workers, prefer hybrid lexical plus vector retrieval, update
incrementally from committed transactions, and rebuild after an event gap. Local-only
profiles may not silently fall back to a remote provider.

### 5.3 MCP is an optional adapter

MCP is evaluated only after the assistance and stable SDK boundaries exist. It can
project bounded document resources, read/search/summarize tools, and proposal-oriented
mutation tools over the same services. It does not define product capabilities, embed
the model runtime, own the semantic index, or expose an unrestricted
`apply_document_edits` bypass. Every mutable result remains a typed proposal that the
host previews and approves before one normal transaction commit.

An embedded browser assistant calls the application API directly. A native/headless
MCP server can use a local process transport. Connecting an external desktop agent to
an active browser session would require a separately accepted, explicitly paired local
companion with origin/session binding and revocable least-privilege access; it is not
implied by the WASM package.

See doc 132 and proposed ADR-035 for scenarios, contracts, threat model, execution
profiles, topology options, open decisions, and graduation gates.

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

### Phase 4: Co-Editing & Collaboration Layer

The former Weeks 11–14/Yjs estimate is superseded. Docs 143–144 define the
current experimental architecture and C0–C8 execution checklist: first unify the
operation/transaction path and complete host document I/O, then prove the
provider-neutral protocol against a deterministic simulator, then implement OT,
the optional relay, presence, history/restore/diff integration, and integrator
hardening. No collaboration implementation begins from this older calendar.

### Phase 5: Customization & Plugin Architecture (Weeks 15–18)
* **Tasks:** Build `@casualoffice/react` and `@casualoffice/vue` headless bindings; design plugin registration framework (`engine.registerPlugin()`); build modular UI component library.
* **Exit Gate:** Third-party developer can build a custom editor with custom toolbar buttons and custom validation rules using only public SDK APIs.

### Phase 6: Experimental Document Assistance, Semantic Retrieval & Optional MCP
* **Tasks:** No implementation is scheduled. First resolve doc 132's DAI-0 owner decisions, unify the public mutation path, define the versioned scope/proposal contracts, and validate a read-only browser-local retrieval spike. MCP may be considered afterward as an optional adapter, beginning read-only.
* **Exit Gate:** This phase cannot start from this timeline alone. It requires accepted ADR-035 plus the proposal safety, preservation, privacy/offline, injection-resistance, retrieval-quality, resource, cross-browser, and MCP conformance/authorization gates in docs 132 and 15. No package or tool name is reserved before that decision.

### Phase 7: Developer Portal, CI Gates & NPM Release (Weeks 22–24)
* **Tasks:** Set up automated visual regression tests (Playwright) and benchmark gates; launch interactive documentation portal with live CodeSandbox demos; publish `@casualoffice/document-runtime` to npm.
* **Exit Gate:** NPM package published, passing 100% of CI release gates with green status.
