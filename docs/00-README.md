# Casual Docs Runtime & SDK — Architecture Blueprint

**Status:** Draft v0.1  
**Audience:** CasualOffice maintainers, SDK contributors, platform integrators  
**Primary implementation:** Rust  
**Primary hosts:** Tauri desktop, WebAssembly/web, headless/server  
**License target:** Apache-2.0

## Purpose

This document set defines the initial product and engineering blueprint for a reusable document editing runtime that can power Casual Docs and can also be embedded by third parties.

The runtime is not a UI toolkit and is not a DOCX-only editor. It is a deterministic document engine with:

- a stable document model;
- editing and transaction semantics;
- layout and pagination;
- rendering;
- import/export;
- collaboration hooks;
- extension APIs;
- native and WebAssembly bindings.

## Document set

1. `01-ORD.md` — outcome and product requirements.
2. `02-ARCHITECTURE.md` — target architecture and design principles.
3. `03-HLD.md` — major components, data flow, deployment, and interfaces.
4. `04-LLD.md` — detailed modules, traits, data structures, algorithms, and error model.
5. `05-SDK-API-SPEC.md` — public SDK surface and embedding contract.
6. `06-ROADMAP-AND-DELIVERY.md` — implementation phases, milestones, staffing, and acceptance gates.
7. `07-QUALITY-SECURITY-AND-COMPATIBILITY.md` — testing, performance, security, compatibility, and release criteria.
8. `08-ADR-REGISTER.md` — initial architecture decisions.
9. `09-REPOSITORY-AND-CONTRIBUTION.md` — proposed repository structure and engineering workflow.

## Recommended project names

- Product/runtime: **Casual Document Runtime**
- Rust workspace: `casual-docs-runtime`
- Core crate: `casual_doc`
- Public SDK facade: `casual_doc_sdk`
- Native renderer: `casual_doc_renderer`
- WebAssembly package: `@casualoffice/document-runtime`
- Tauri integration crate: `casual_doc_tauri`

## Core boundary

The engine owns document state, transactions, selection, layout, pagination, hit testing, and serialization.

Host applications own windows, menus, dialogs, authentication, storage policy, network policy, telemetry, and product-specific UI.

## Non-negotiable constraints

- Native-first, WebAssembly-compatible architecture.
- No browser DOM as the source of truth.
- Deterministic behavior for the same document, fonts, viewport, and engine version.
- No mandatory server dependency.
- No mandatory React dependency.
- No mandatory collaboration provider.
- Loss-aware DOCX round-trip behavior.
- Stable, versioned public API.
