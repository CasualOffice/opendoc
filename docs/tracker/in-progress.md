# In progress or partly delivered

Generated from the single open queue in [docs/109](../109-BACKLOG.md).
Edit the source row there, then run `node webapp/tools/build-tracker-views.mjs`.
A partial status records delivered work; it does not claim that a person is
currently assigned. Rows stay in their queue order.

| # | Id | Priority | Work | Status |
| ---: | --- | --- | --- | --- |
| 1 | HF-085 | P1 | `main.js` has been split into feature modules, but its composition still runs at import and cannot be mounted and unmounted under a host root | Partly fixed; module split merged (#867, #869, #871, #875) |
| 2 | HF-035 | P1 | No spelling or grammar checking anywhere — less feedback than a plain `<textarea>` | Partly fixed — spelling, glossary and a first grammar rule set shipped |
| 3 | HF-081 | P1 | 1,319 user-facing strings are still English literals in the source — the SEAM and the pipeline now exist, the strings have not moved yet | Open (partly delivered) |
| 4 | HF-109 | P1 | The packaged iframe custom element exists, but the editor still lacks an in-process `mountEditor(root, options)` and unmount lifecycle | Partly fixed; package and custom element shipped, in-process mount open |
| 5 | HF-114 | P1 | Share and join work through an optional host; production host integration and invitation access settings remain | Partly fixed; Share, presence and recovery implemented (`docs/169`) |
| 6 | HF-362 | P2 | Verify the merged shared, host-authored version history and close its acceptance row (`docs/174` RC-09) | Open — implementation merged in #890; acceptance and queue closure pending |
| 9 | HF-066 | P2 | Pasted hyperlinks are stored with no scheme filter and re-exported | Partly fixed |
| 20 | HF-160 | P2 | No `.rtf` support — RTF is in neither the import nor the export registry, so an RTF file cannot be opened at all | Partly fixed (#553) |
| 31 | HF-210 | P2 | ODT header and footer content now exports correctly and is still lost on import, so the round trip is asymmetric | Partly fixed — fields and hyperlink text import; pictures, tables, formatting and link targets remain |
| 49 | HF-299 | P2 | Comment cards: the thread's root comment cannot be edited (RC-11), the card stops showing what it referred to once the commented text is deleted (RC-12), posting or cancelling a comment drops the keyboard onto `<body>` (RC-15), and View ▸ "Show changes (read-only)" is mislabelled and, turned off, still underlines insertions (RC-14) | Partly fixed — modules built, `main.js` half routed |
| 90 | FID-L-05 | P1 | Footnote number format, restart, and position are entirely unconsumed | Partly fixed (#544) |
| 91 | EV-005 | P1 | The matrix drift guard cannot detect an overstatement, and does not cover the page | In progress |
| 94 | CQ-003 | P1 | Guards that cannot fail | Partly fixed |
| 99 | FID-L-07 | P1 | Floating tables (`w:tblpPr`) render inline | Partly fixed (#589) |
| 100 | OO-002 | P1 | No recent files — New, templates and the backstage all shipped | Partly fixed (#542, #596) |
| 111 | OO-006 | P2 | Hyphenation lacks line breaking; drop-cap advanced options are incomplete | Partly fixed |
| 112 | OO-023 | P2 | Version history writes rows for documents that did not change, and a row does not say what changed | Open — reproduction in flight |
| 119 | FID-L-14 | P2 | Emphasis marks, outline, shadow, emboss, imprint and run borders are modeled and cascaded but unpainted | Partly fixed (#541) |
| 121 | FID-L-16 | P2 | `w:gutter` and `w:mirrorMargins` never reach the page configuration | Partly fixed (#536) |
| 122 | FID-L-18 | P2 | `w:jc="distribute"` silently collapses to ordinary justification | Partly fixed (#541) |
| 124 | UX-015 | P2 | Single-surface capabilities — Pages panel, table style gallery, line/paragraph spacing, format painter, Settings | Partly fixed (#542) |
| 129 | FID-L-13 | P2 | Cell `noWrap`, `fitText`, `hideMark` and cell `textDirection` are unconsumed | Partly fixed |
| 136 | FID-R-05 | P2 | Retained opaque parts are never invalidated on edit | Partly fixed (`fid/attributes-v2`) |
| 150 | OO-014 | P2 | Charts and SmartArt are not drawn, so there is nothing to author | In progress |
| 181 | RM-49 | — | PDF into the model for editing (owner, 2026-10-28): fixed-layout import (each page a section of page-anchored text boxes, pictures and shape groups, paragraphs through the body pipeline), reflow as an explicit "Convert to flowing document", native evidence parsed in Rust by `hayro` (PDFium fallback), a resumable cancellable import job, exact/semantic save then a byte-preserving incremental update | Partly fixed — P0 to P6 merged in #899, #905, #906 and #909; open: the "Not built" lists of `docs/182` §16.4, §17.4, §17a.4, §18.5, §19.4 and §20.6 (Compare does not accept a PDF, no host option feeds predefined CMaps, pictures and shapes are not edited in place on a preserving save, OCR numerics and speed are unmeasured in a browser) |
| 184 | RM-18 | P2 | Character formatting: effects and spacing: effect toggles; Outline, shadow, emboss and engrave: author and paint; Character border and emphasis marks: author and paint; Font dialog, Advanced tab | Open — implementation merged in #835; acceptance and parity re-grade pending |
| 186 | RM-20 | P2 | Paragraph formatting: right-to-left; Pagination and outline; Tabs; Paragraph borders; Clear all paragraph formatting | In progress |
| 187 | RM-21 | P2 | Lists and styles: multilevel list gallery; List settings dialog; Numbering value and symbol bullets; Picture bullets: model, draw, save, author; Styles: new from selection, update to match, delete, next style, restore | In progress |
| 188 | RM-22 | P2 | Page setup and sections: paper and margin presets; Two-sided printing; Page colour and blank page; Columns and line numbers; Section breaks: insert and split; column balancing | In progress |
| 189 | RM-23 | P2 | Headers, footers and navigation: link to previous and remove a band; Page number positions; Go to a page; Outline: promote, demote, add heading; Fold in the header/footer audit findings (lane ux/header-footer) | Partly fixed — Link to previous, Remove and the page-number positions shipped; Go to a page and outline editing remain |
| 194 | RM-28 | P2 | Arranging objects: align and distribute; Commands on a multi-selection; Position relative to, distance, move with text, overlap; Wrap boundary editing and snapping | Partly fixed in #911 |
| 195 | RM-29 | P2 | Pictures: replace a picture and restore its size; Crop: reset, fit, fill; Insert from a URL or host storage; Border, effects, transparency and alt-text title; Save as picture, paste as picture | Partly fixed in #911 |
| 196 | RM-30 | P2 | Shapes: fills: solid, gradient, texture, pattern, picture; Outline dash, cap, join and arrowheads; Shape shadow; Change shape and the full gallery; Edit points, adjust handles, merge shapes | Partly fixed in #911 |
| 206 | RM-46 | P2 | Three SDK products on one engine (owner, 2026-10-23): a Viewer SDK (read-only, browser and server; vs docx-preview, mammoth.js), the Editor SDK (unchanged; vs ONLYOFFICE Docs) and a Native/headless SDK (create, read, modify, convert from Java, Python, .NET and C; vs POI XWPF, python-docx, docx4j, Open XML SDK, Aspose.Words). Viewer and Native are batch APIs with no UI, caret, selection, hit-testing, incremental paint or collaboration | Open, designed (`docs/176`, ADR-071 proposed); V0 built |
| 207 | RM-47 | P2 | MCP and an AI-native SDK surface (owner, 2026-10-24): an `opendoc-mcp` server on the Native SDK (open/inspect, search, read by `NodeId` + quote anchor, edit through a closed edit-plan vocabulary compiled to the op set, convert/render, compare, validate and loss report), the `opendoc.ai` layer it wraps (Python and JS first: outline, anchored chunks for RAG, edit plans), opt-in WebMCP tools in `<opendoc-editor>` gated by role, and later an agent as a room participant with a host-attested AI identity. Agent edits are tracked suggestions by default, enforced by the engine | In progress: A1, M1 and M2 (stdio) built |
| 211 | RM-14 | P3 | The second render backend is built only as an opt-in: the `vello_common` sparse-strip encoder (`casual-doc-render-gpu`, `vello_cpu` and `vello_hybrid` on `wgpu`) paints the display list within a stated tolerance of the CPU reference, but no host uses it by default, the browser has no GPU path yet, and the shipped module has no `simd128` | Open, measured and designed (`docs/architecture/179`, ADR-074 proposed); opt-in backend built |
| 219 | RM-45 | P3 | Collaboration: share and join a document; Mentions and comment ownership; Chat and co-editing mode | In progress |
