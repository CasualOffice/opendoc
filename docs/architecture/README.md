# Feature architecture

Read the subsystem contract that matches the change. The numbered design files
are kept as evidence; the [single queue](../109-BACKLOG.md) owns implementation
status and remaining work. The two measured performance designs now live in
this folder; the other contracts retain their cited paths while they are
reviewed for consolidation.

| Area | Primary documents |
| --- | --- |
| Document model, DOCX and loss | [Schema v1](../38-SCHEMA-V1-DESIGN-REFERENCE.md), [OOXML fidelity](../34-OOXML-FIDELITY-ARCHITECTURE.md), [multi-format I/O](../135-MULTI-FORMAT-IMPORT-EXPORT-ARCHITECTURE.md) |
| Editing and review | [Transaction path](../147-TRANSACTIONS-AS-THE-ONE-MUTATION-PATH.md), [review projection](../133-REVIEW-PROJECTION-AND-FORMATTING-DESIGN.md) |
| Tables | [Table operations](../72-TABLE-DISTRIBUTION-DESIGN.md), [merged-cell geometry](../171-TABLE-MERGED-CELLS-GRID-MAP-AND-CHROME.md) |
| Layout and rendering | [Layout design](../43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md), [reflow](../151-REFLOW-PAGELESS-LAYOUT-DESIGN.md), [GPU backend measurement](179-GPU-RENDER-BACKEND-MEASURED.md), [worker measurement](180-WORKER-THREADING-MEASURED.md) |
| Shapes | [Preset shape model and rendering](../87-PRESET-SHAPE-MODEL-AND-PRIMITIVE-RENDERING-DESIGN.md) |
| Floating objects and text boxes | [Combined feature design](../47-TEXT-BOX-APPEARANCE-DESIGN.md) |
| Glyphs | [Color rendering and picker panel](../102-COLOR-GLYPH-RENDERING-DESIGN.md) |
| Editor and embedding | [Editor shell](../56-EDITOR-SHELL-AND-RENDER-ARCHITECTURE.md), [host contract](../125-EMBED-AND-HOST-CONTRACT-DESIGN.md), [three SDK products](../176-THREE-SDK-PRODUCTS.md) |
| Collaboration and AI | [OT and snapshot replay](../107-COLLABORATION-OT-SNAPSHOT-REPLAY-DESIGN.md), [protocol](../152-COLLABORATION-PROTOCOL-SESSION-AND-IDENTITY.md), [MCP and AI SDK](../178-MCP-SERVER-AND-AI-NATIVE-SDK.md) |

Short implementation notes and older audits remain discoverable through
`rg --files docs` and their citations. New feature design should extend one of
the contracts above unless it needs a new durable decision or interface.
