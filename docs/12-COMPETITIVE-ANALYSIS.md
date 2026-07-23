# Competitive Analysis

**Pass:** 1 - architecture and integration baseline
**Status:** Complete
**Research checked:** 2026-07-24

## Purpose

Competitive analysis informs product boundaries, compatibility expectations, SDK
ergonomics, UX quality, and test design. It does not justify copying proprietary
behavior or claiming fidelity without corpus evidence.

## Market Groups

### Full Office Editors

Microsoft Word remains the practical compatibility reference. Its extension API
exposes typed document objects across web and desktop hosts, but platform-specific
requirement sets also show that host parity is not automatic. Word for the web
still depends on an online service for editing, while desktop Word covers offline
authoring.

ONLYOFFICE Docs and Collabora Online demonstrate demand for self-hosted,
browser-based office suites. Their integration model is service-oriented:
storage/authentication remain with the integrator, while a document editing
service supplies the editor. ONLYOFFICE exposes configuration, permissions,
callbacks, co-editing modes, and JWT-protected service communication. Collabora
uses WOPI integration and distinguishes its monthly development edition from its
supported production product.

**Implication for OpenDoc:** do not compete by shipping another mandatory
document server. Provide the reusable local engine that a desktop app, browser
worker, server, or custom collaboration service can embed.

### Cloud-Native Collaboration

Google Docs sets the user expectation for fast shared editing. Its public
document API uses a structured document resource, revision IDs, UTF-16 indexed
ranges, and atomic `batchUpdate` calls. The API's need to plan for concurrent
changes reinforces the value of explicit revision and position-mapping
semantics.

**Implication for OpenDoc:** local transactions must be atomic and revision
aware before collaboration adapters are added. Public runtime positions use
grapheme boundaries and stable node identity rather than copying the Docs API's
global UTF-16 index model.

### Rich-Text Frameworks

Tiptap/ProseMirror, CKEditor 5, Lexical, and TinyMCE are strong references for
commands, extensions, immutable or transaction-based state, and host-defined UI.

- Tiptap uses a schema-driven tree, transactions, commands, and extensions, with
  JSON as the recommended stored form.
- CKEditor 5 separates its custom model from editing/data views and makes
  features granular plugins.
- Lexical keeps editor state, not the DOM, as source of truth; updates produce
  immutable snapshots and commands form an extensible event boundary.
- TinyMCE has a mature plugin surface, but DOCX import/export is provided through
  separately licensed conversion services that translate between DOCX and HTML.

These frameworks primarily solve web rich-text editing. Their architectural
patterns are useful, but none should be treated as proof of deterministic,
cross-platform DOCX pagination.

**Implication for OpenDoc:** retain a narrow model/transaction/command core and
an extension registry, while keeping pagination, scene generation, and
format-preservation as first-class engine subsystems.

## Comparison Matrix

| Product group | Main strength | Integration shape | OpenDoc lesson |
| --- | --- | --- | --- |
| Microsoft Word | DOCX behavior and mature authoring | Product host plus add-in APIs | Compatibility claims need feature profiles and platform evidence. |
| Google Docs | Collaboration UX and atomic cloud updates | Cloud document service | Revisions, atomic batches, and anchor mapping are core semantics. |
| ONLYOFFICE Docs | Embeddable full office UI and self-hosting | Browser client plus editing/conversion services | Integrators need explicit permissions, callbacks, and security policy. |
| Collabora Online | LibreOffice-based self-hosted office editing | WOPI-integrated service | Storage/auth boundaries and production/development channels must be clear. |
| Tiptap/ProseMirror | Headless schema and extension ecosystem | Browser editor library | Commands and extensions should compose without exposing mutable internals. |
| CKEditor 5 | Model/view conversion and granular plugins | Browser editing framework | Import, editing projection, and data projection need separate contracts. |
| Lexical | Small extensible state/update core | Browser editor framework | Immutable snapshots and controlled updates reduce host coupling. |
| TinyMCE | Mature integration and plugin catalog | Browser editor plus optional services | HTML conversion is useful interchange but insufficient as DOCX truth. |

## UX Baseline

The runtime does not own a ribbon, but its contracts determine whether hosts can
provide good UX. Phase designs must support:

- visible ready, saving, warning, and recovery states;
- command queries with enabled, active, mixed, and value states;
- predictable undo grouping and selection restoration;
- cancellable long-running load, save, and layout work;
- explicit unsupported-content warnings before destructive export;
- revision-ordered events with no partially committed state;
- keyboard, IME, screen-reader, and high-zoom host integration;
- responsive visible-page work before distant pagination completes.

## Competitive Bug Hunt

The first cross-product study should use rights-cleared fixtures that probe:

- page count and line-break drift after open/save;
- list numbering restart and nested-list behavior;
- merged tables, border conflict, and row split behavior;
- section breaks, first/odd/even headers and footers;
- floating image anchor movement;
- comments and tracked-change range survival;
- unsupported drawing preservation;
- concurrent insertion at the same logical boundary;
- browser refresh, offline recovery, and save conflict UX;
- keyboard/IME behavior inside tables and around inline objects.

Findings become fixtures and tracker items, not undocumented anecdotes.

## Product Position

OpenDoc's intended position is:

> A production-grade, local-first document runtime that gives native, web, and
> headless hosts the same deterministic model, transactions, layout, rendering,
> and loss-aware DOCX behavior without requiring a bundled editor UI, cloud
> service, framework, or collaboration vendor.

The difficult proof points are DOCX preservation, pagination determinism,
grapheme/IME correctness, large-document performance, and API stability. Feature
count alone is not a useful differentiator.

## Sources

Primary product and project documentation checked on 2026-07-24:

- [Microsoft Office JavaScript API reference](https://learn.microsoft.com/en-us/javascript/api/overview)
- [Microsoft Word for the web service description](https://learn.microsoft.com/en-us/office365/servicedescriptions/office-online-service-description/word-online)
- [Google Docs document structure](https://developers.google.com/workspace/docs/api/concepts/structure)
- [Google Docs document API concepts](https://developers.google.com/workspace/docs/api/concepts/document)
- [ONLYOFFICE Docs integration architecture](https://api.onlyoffice.com/docs/docs-api/get-started/how-it-works/)
- [ONLYOFFICE Docs editor configuration](https://api.onlyoffice.com/docs/docs-api/usage-api/config/editor/)
- [ONLYOFFICE Docs security](https://api.onlyoffice.com/docs/docs-api/get-started/how-it-works/security/)
- [Collabora Online SDK manual](https://sdk.collaboraonline.com/CO-SDK-manual.pdf)
- [Collabora Online Development Edition](https://www.collaboraonline.com/code/)
- [Tiptap core concepts](https://tiptap.dev/docs/editor/core-concepts/introduction)
- [Tiptap extensions](https://tiptap.dev/docs/editor/core-concepts/extensions)
- [CKEditor 5 architecture](https://ckeditor.com/docs/ckeditor5/latest/framework/architecture/intro.html)
- [CKEditor 5 conversion model](https://ckeditor.com/docs/ckeditor5/latest/framework/deep-dive/conversion/intro.html)
- [Lexical editor state](https://lexical.dev/docs/concepts/editor-state)
- [Lexical commands](https://lexical.dev/docs/concepts/commands)
- [TinyMCE plugin catalog](https://www.tiny.cloud/docs/tinymce/latest/plugins/)
- [TinyMCE DOCX conversion service](https://www.tiny.cloud/docs/tinymce/latest/individual-import-from-word-and-export-to-word-on-premises/)

## Next Pass

Pass 2 begins with runnable, rights-cleared DOCX fixtures and records measured
import, edit, export, pagination, and warning behavior. Marketing feature lists
are not accepted as fidelity evidence.
