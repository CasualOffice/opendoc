# UX and Bug Hunting

## Purpose

UX review and bug hunting are first-class engineering activities for this runtime. Document editing is interaction-heavy, and correctness failures often appear as small visual, selection, or persistence defects.

## UX Review Areas

- opening and recovering documents;
- page navigation;
- caret movement;
- selection;
- IME;
- keyboard shortcuts;
- copy, cut, and paste;
- undo and redo;
- formatting command state;
- table editing;
- image and object manipulation;
- comments and tracked changes;
- save/export feedback;
- warnings for unsupported content;
- accessibility semantics;
- host integration ergonomics.

## Bug Hunting Areas

### Document Safety

- dropped XML;
- broken relationships;
- media loss;
- style loss;
- numbering corruption;
- section/header/footer corruption;
- comments or tracked changes detached from ranges;
- invalid output package.

### Layout

- pagination drift;
- line break differences;
- table border conflicts;
- merged cell occupancy errors;
- image anchor misplacement;
- header/footer overlap;
- footnote overflow;
- bidi and script handling;
- font fallback differences.

### Editing

- invalid position mapping;
- selection jumps;
- undo grouping errors;
- transaction rebase errors;
- IME composition corruption;
- clipboard data loss;
- command state mismatch;
- collaborative anchor drift.

### Performance

- full relayout after small edit;
- expensive hit testing;
- repeated shaping;
- excessive allocations;
- large image decode stalls;
- save/load spikes;
- WASM transfer overhead.

### Security

- ZIP bombs;
- XML entity or depth abuse;
- path traversal;
- external relationship fetching;
- malformed images/fonts;
- plugin capability escape;
- operation log exhaustion.

## Bug Report Template

Use this structure for bug reports and tracker entries:

- title;
- subsystem;
- severity;
- reproduction steps;
- expected behavior;
- actual behavior;
- affected documents or fixtures;
- security/data-loss risk;
- suspected root cause;
- proposed test;
- status.

## Severity

- Critical: data loss, security issue, crash, invalid saved document, public API contract break.
- High: serious fidelity failure, broken common editing workflow, severe performance issue.
- Medium: visible UX issue, unsupported edge case with warning, moderate performance issue.
- Low: polish, docs, minor mismatch, non-blocking improvement.

## Acceptance Rule

A bug is not done until:

- root cause is understood;
- fix is implemented;
- regression coverage exists or the gap is documented;
- docs/tracker are updated when behavior changes.
