# Support Matrix

**Status:** Accepted for Phase 0
**Last updated:** 2026-07-24

This document distinguishes target support from implemented support. A target is
not considered supported until its required CI and conformance gates pass.

## Platform Tiers

| Tier | Contract |
| --- | --- |
| Tier 1 | Required CI on every change, release artifacts, and blocking regressions. |
| Tier 2 | Scheduled build/test coverage; regressions are release-blocking when reproducible. |
| Experimental | Best effort, no compatibility promise, and no release artifact requirement. |

## Native Targets

| Environment | Rust target | Planned tier | Current status |
| --- | --- | --- | --- |
| macOS Apple Silicon | `aarch64-apple-darwin` | Tier 1 | Workspace checks begin in Phase 0. |
| macOS Intel | `x86_64-apple-darwin` | Tier 2 | Compile coverage planned. |
| Windows 64-bit | `x86_64-pc-windows-msvc` | Tier 1 | Workspace checks begin in Phase 0. |
| Linux 64-bit glibc | `x86_64-unknown-linux-gnu` | Tier 1 | Workspace checks begin in Phase 0. |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | Tier 2 | Planned after headless CLI exists. |

The first release line uses Rust 2024 edition and an MSRV of Rust 1.85.0. Stable
Rust is the development baseline. MSRV is checked separately and may only be
raised through an ADR and a documented release note.

## WebAssembly

| Environment | Planned tier | Current status |
| --- | --- | --- |
| `wasm32-unknown-unknown`, core model/transactions | Tier 1 | Compile gate begins in Phase 0. |
| Browser SDK in current Chrome, Edge, Firefox, Safari | Tier 1 | Planned; no browser runtime exists yet. |
| Browser worker execution | Tier 1 | Planned with WASM facade. |
| WASM threads | Experimental | Requires host opt-in and cross-origin isolation. |
| Node.js WASM headless use | Tier 2 | Planned after WASM facade. |

The browser policy at beta will cover the latest two stable major versions
available at release time. Exact versions belong in each release conformance
report.

## Host Modes

| Host mode | v1 target | Current status |
| --- | --- | --- |
| Rust library | Yes | Initial facade begins in Phase 0. |
| Headless CLI/service | Yes | Planned. |
| Tauri desktop | Yes | Planned reference host. |
| Browser/WASM | Yes | Planned reference host. |
| C ABI | Yes | Planned after the Rust facade stabilizes. |
| React/Vue/Svelte wrappers | Optional | Must live outside the core runtime. |
| Native mobile UI | No | Out of scope for v1. |

## Format Capability Status

| Format/capability | v1 target | Current status |
| --- | --- | --- |
| Normalized JSON snapshot | Yes | Schema v0 designed; initial subset follows. |
| Canonical normalized CBOR | Yes | Designed, not implemented. |
| DOCX import/export | Yes | Designed, not implemented. |
| TXT import/export | Yes | Planned as a simple conformance path. |
| PDF render/export | Yes | Backend decision pending. |
| ODT import/export | Later | Not a v1 release gate. |
| HTML/Markdown interchange | Later | Not an editing source of truth. |
| Macros/VBA execution | No | Blocked by policy. |

## Feature Profile

| Area | v1 expectation | Current status |
| --- | --- | --- |
| Paragraphs, marks, lists | Supported | Model foundation in progress. |
| Tables and merged cells | Supported | Designed, not implemented. |
| Sections, headers, footers | Supported | Designed, not implemented. |
| Images and anchors | Supported | Designed, not implemented. |
| Comments and tracked changes | Supported | Designed, not implemented. |
| Fields and notes | Supported or render-only by subtype | Designed, not implemented. |
| Shapes, text boxes, VML | Preserve or flatten with warning | Designed, not implemented. |
| Real-time collaboration | Adapter-based | Post local transaction stability. |
| Accessibility semantics | Required | Designed, not implemented. |

## Required Release Evidence

A target becomes supported only when the release includes:

- a green required CI matrix;
- target-specific smoke tests;
- a published compatibility profile;
- parser and resource-limit conformance;
- documented known limitations;
- deterministic fixture results where layout or rendering applies.
