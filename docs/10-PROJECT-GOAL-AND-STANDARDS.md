# Project Goal and Standards

## Goal

Build a production-grade document runtime and SDK for CasualOffice and third-party integrators.

This project is not an MVP. It is not a side project. It is the foundation for document editing products that must handle real files, real users, long-lived APIs, and compatibility pressure.

## Product Standard

The runtime should become a dependable engine for:

- CasualOffice desktop document editing;
- browser/WASM document editing;
- headless document processing;
- SDK embedding by external applications;
- future collaboration and extension workflows.

## Engineering Standard

Production-grade means:

- deterministic results where documented;
- explicit compatibility profiles;
- typed errors and recoverable warnings;
- security limits from the first parser;
- testable layout and rendering;
- stable public API boundaries;
- documented migrations;
- CI gates for every supported platform;
- no silent document data loss.

## Decision Standard

Important work follows this sequence:

1. research;
2. design;
3. discussion;
4. finalized decision;
5. tracker update;
6. implementation;
7. verification;
8. documentation update.

Skipping design is acceptable only for small documentation fixes, isolated typo fixes, or mechanical cleanup with no product or architecture impact.

## Non-Negotiables

- No browser DOM as the source of truth.
- No mandatory React dependency.
- No mandatory collaboration provider.
- No mandatory server dependency.
- No silent dropping of DOCX content in release behavior.
- No public direct mutation of the internal document model.
- No unbounded parsing or implicit external resource fetching.
- No undocumented public API behavior.
