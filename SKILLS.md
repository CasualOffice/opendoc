# Skills and Working Practices

This document defines the recurring skills needed to build the Casual Document Runtime.

## Core Skills

### Document Formats

- DOCX package structure;
- WordprocessingML;
- DrawingML;
- relationships and content types;
- style, numbering, theme, and section semantics;
- preservation of unsupported but safe XML.

### Runtime Engineering

- Rust systems design;
- stable public API design;
- FFI and WASM boundaries;
- deterministic serialization;
- resource limits and cancellation;
- typed error models.

### Editor Semantics

- command and transaction design;
- undo/redo;
- stable positions and anchors;
- selection and IME;
- clipboard models;
- collaboration adapter boundaries.

### Layout and Rendering

- text shaping;
- bidi and grapheme handling;
- line breaking;
- pagination;
- tables and floats;
- display-list rendering;
- visual regression testing.

### Product Quality

- competitive analysis;
- UX review;
- bug hunting;
- accessibility review;
- security review;
- performance profiling;
- CI design.

## Working Practice

For each substantial feature:

1. define the expected user or integrator outcome;
2. document the design;
3. compare against relevant competitors or existing editor behavior;
4. identify UX risks and bug classes;
5. define acceptance gates;
6. update the execution tracker;
7. implement;
8. verify through tests, fixtures, and docs.

## Reference Policy

External information may be used for research, but repository docs and finalized project decisions are the source of truth. When modern product, dependency, browser, OS, or standard behavior matters, verify with current primary sources before relying on it.
