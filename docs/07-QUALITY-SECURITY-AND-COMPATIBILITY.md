# Quality, Security, and Compatibility Plan

## 1. Test pyramid

### Unit tests

- model invariants;
- transaction mapping;
- style resolution;
- line breaking;
- pagination rules;
- table grid;
- command enablement;
- error conversion.

### Property tests

- operation plus inverse returns equivalent snapshot;
- position mapping remains valid;
- serializer/loader idempotence;
- style resolution terminates;
- random table operations preserve occupancy invariants.

### Fuzzing

Targets:

- ZIP package reader;
- XML parser;
- relationship parser;
- DOCX mapper;
- normalized snapshot decoder;
- operation decoder;
- image metadata parsers;
- custom object codecs.

### Corpus tests

Every fixture has:

- source;
- expected supported features;
- expected warnings;
- round-trip result;
- extracted semantic snapshot;
- render snapshots;
- performance budget.

### Visual regression

Render fixed fonts and compare:

- page count;
- block positions;
- glyph baselines;
- image bounds;
- table borders;
- headers/footers;
- comments/tracked-change overlays.

Use perceptual thresholds plus structural metrics. Pixel diff alone is insufficient.

### Cross-platform tests

Run selected render fixtures on macOS, Windows, Linux, and WASM. Differences must be explained and versioned.

## 2. Compatibility profiles

Publish profiles rather than claiming blanket DOCX support.

Example:

- **Core:** text, paragraphs, styles, lists, tables, images, sections.
- **Review:** comments and tracked changes.
- **Publishing:** headers/footers, notes, fields, columns.
- **Drawing:** shapes, groups, textboxes, anchors.
- **Preservation-only:** content retained but not fully editable/rendered.

Each feature has:

- import;
- render;
- edit;
- save;
- preserve;
- known limitations.

## 3. Round-trip policy

Categories:

- `supported`: editable and serialized semantically;
- `render-only`: displayed but edits restricted;
- `preserved`: not understood but retained;
- `flattened`: converted to a simpler equivalent with warning;
- `dropped`: removed only with explicit high-severity warning;
- `blocked`: rejected for security.

No silent dropping of package parts in release builds.

## 4. Performance engineering

Required benchmark classes:

- small note;
- 20-page report;
- 100-page specification;
- 500-page text-heavy document;
- image-heavy document;
- table-heavy document;
- comments/revisions-heavy document;
- pathological content.

Measure:

- load;
- first page visible;
- full pagination;
- typing;
- delete;
- style change;
- table edit;
- memory;
- save;
- scene generation;
- render time.

Profiling artifacts should be attached to performance-regression pull requests.

## 5. Security threat model

Threats:

- ZIP bombs;
- XML bombs and extreme nesting;
- path traversal;
- relationship URI abuse;
- external resource tracking;
- malformed image codecs;
- huge dimensions;
- crafted fonts;
- untrusted plugins;
- operation-log resource exhaustion;
- collaboration replay or privilege confusion;
- clipboard HTML/script injection;
- malicious hyperlinks.

Controls:

- hard configurable limits;
- deny external fetch by default;
- normalize package paths;
- safe XML parser configuration;
- sandbox image/font decoding where feasible;
- capability-based plugins;
- signed release artifacts;
- dependency audit;
- fuzzing;
- security advisories and private disclosure channel.

## 6. Privacy

The SDK itself:

- sends no telemetry by default;
- performs no network requests by default;
- stores no document outside host-selected persistence;
- exposes all external-resource requests to host policy;
- allows diagnostics redaction;
- does not include document text in logs by default.

## 7. Accessibility quality

Semantic snapshot must expose:

- headings;
- paragraphs;
- lists;
- tables and headers;
- links;
- images with descriptions;
- comments;
- revisions;
- selection/caret;
- reading order.

Official hosts must test platform screen readers and keyboard-only operation.

## 8. Release gate

A beta/stable release requires:

- all critical unit/property tests passing;
- fuzz targets run for defined minimum duration;
- no known critical security issue;
- compatibility report generated;
- benchmark regressions within threshold;
- public API diff reviewed;
- schema migration tests passing;
- example apps building on all supported targets;
- documentation and changelog complete.
