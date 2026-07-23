# Competitive Analysis

## Purpose

Competitive analysis is required project work. It informs product expectations, UX quality, compatibility targets, and bug hunting.

This document starts the analysis framework. It should be updated as research is performed.

## Products to Study

### Editing Products

- Microsoft Word desktop;
- Microsoft Word Online;
- Google Docs;
- LibreOffice Writer;
- OnlyOffice Docs;
- Collabora Online;
- Apple Pages;
- Zoho Writer;
- Dropbox Paper for collaboration patterns;
- Notion for lightweight document workflows.

### SDK and Embedded Editors

- TinyMCE;
- CKEditor;
- ProseMirror/Tiptap;
- Lexical;
- Slate;
- Syncfusion DocumentEditor;
- WebViewer document editor products;
- commercial DOCX conversion/rendering SDKs.

## Analysis Dimensions

For each relevant competitor, record:

- document fidelity;
- pagination behavior;
- table behavior;
- headers/footers;
- comments and tracked changes;
- collaboration UX;
- offline behavior;
- performance on large documents;
- import/export limitations;
- plugin/extension model;
- embedding API;
- accessibility;
- mobile/tablet stance;
- security and self-hosting model;
- pricing/licensing constraints where relevant.

## Required Outputs

Each analysis pass should produce:

- summary of observed behavior;
- screenshots or fixtures when permitted;
- user-visible strengths;
- user-visible weaknesses;
- implementation implications;
- bugs or edge cases to test;
- priority recommendation.

## Initial Observations

- Word compatibility is the benchmark users will judge against, even when exact compatibility is out of scope.
- Google Docs sets expectations for collaboration responsiveness and simple sharing.
- OnlyOffice and Collabora set expectations for self-hosted office editing.
- ProseMirror/Tiptap/Lexical show extension patterns, but they do not solve deterministic DOCX pagination by themselves.
- Existing Casual Docs is useful study material for practical browser editor behavior, but it is not the implementation target for this repository.

## Tracker

| Area | Status | Notes |
| --- | --- | --- |
| Microsoft Word desktop comparison | Not started | Need DOCX corpus and visual baseline. |
| Word Online comparison | Not started | Need browser workflow study. |
| Google Docs collaboration UX | Not started | Need interaction notes. |
| OnlyOffice/Collabora self-hosting | Not started | Need deployment and fidelity notes. |
| Embedded editor SDKs | Not started | Need API and extension model comparison. |
| Existing Casual Docs study | Started | Reference only; do not modify sibling repo unless explicitly asked. |
