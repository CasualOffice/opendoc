# Parser and Resource Limits

**Status:** Accepted for Phase 0
**Last updated:** 2026-07-24
**Tracker:** F-007

Package ZIP limits are implemented by `casual-doc-ooxml` as of 2026-07-24.
XML, relationship, image, font, and semantic limits remain implementation work.

## Security Position

All document input is untrusted. Parsing is bounded before allocation or
decompression wherever possible, cancellable at long-running boundaries, and
network-isolated by default.

Limits have:

- a secure runtime default;
- an optional lower host override;
- a hard ceiling that normal hosts cannot exceed;
- a stable `ODC-1003` error with observed and allowed values.

Raising a default or hard ceiling requires security review and corpus evidence.

## DOCX Package Defaults

| Limit | Secure default | Hard ceiling |
| --- | ---: | ---: |
| Input package bytes | 256 MiB | 1 GiB |
| ZIP entries | 10,000 | 50,000 |
| Total expanded bytes | 1 GiB | 4 GiB |
| Single expanded entry | 256 MiB | 1 GiB |
| Expansion ratio per entry | 200:1 | 1,000:1 |
| Package path bytes | 1,024 | 4,096 |
| Relationships per package | 100,000 | 500,000 |
| Relationship target bytes | 8 KiB | 64 KiB |
| Preserved unknown bytes | 64 MiB | 256 MiB |

ZIP entry names are normalized as package paths. Absolute paths, drive prefixes,
NUL bytes, and traversal outside the package root are rejected. Duplicate
normalized part names are rejected.

Encrypted packages, executable activation, macros, and automatic external
relationship fetching are unsupported in v1. Macro parts may only be retained
under an explicit future preservation policy.

## XML Defaults

| Limit | Secure default | Hard ceiling |
| --- | ---: | ---: |
| XML depth | 256 | 1,024 |
| Elements per part | 5,000,000 | 20,000,000 |
| Attributes per element | 4,096 | 16,384 |
| Attribute value bytes | 1 MiB | 8 MiB |
| Text node bytes | 16 MiB | 64 MiB |
| Namespace declarations per element | 256 | 1,024 |

DTD processing and external entities are disabled. Entity expansion beyond XML
built-ins is rejected. Parsers must count work incrementally rather than
building an unbounded DOM before enforcing limits.

## Semantic Defaults

| Limit | Secure default | Hard ceiling |
| --- | ---: | ---: |
| Total Unicode scalar values in text | 50,000,000 | 200,000,000 |
| Paragraphs | 2,000,000 | 8,000,000 |
| Table cells | 1,000,000 | 4,000,000 |
| Comments and notes combined | 500,000 | 2,000,000 |
| Bookmarks and field boundaries | 1,000,000 | 4,000,000 |
| Maximum structural nesting | 128 | 512 |

The parser stops at the first hard failure and does not return a partially
editable session. A future inspection mode may return bounded diagnostics
without creating a session.

## Images and Fonts

| Limit | Secure default | Hard ceiling |
| --- | ---: | ---: |
| Encoded image bytes per image | 64 MiB | 256 MiB |
| Decoded image pixels | 100 megapixels | 400 megapixels |
| Image dimension | 32,768 px | 65,535 px |
| Embedded font bytes per font | 32 MiB | 128 MiB |
| Aggregate decoded image cache | Host configured | Runtime budget required |

Dimensions and metadata are validated before full decode when the codec permits.
Decoders must honor cancellation and memory budgets. Untrusted embedded fonts
are disabled until a sandboxed or separately reviewed font path exists.

## Runtime Budgets

Layout, history, scene, image cache, and operation-log budgets are separate from
parser limits. The host may trade memory for responsiveness, but exceeding a
budget must trigger deterministic eviction, deferred work, or a typed error,
never uncontrolled growth.

## Test Requirements

Each limit requires:

- one boundary-accepted fixture;
- one boundary-rejected fixture;
- one value far above the hard ceiling;
- cancellation coverage for expensive work;
- proof that rejected input does not create a partial session;
- diagnostics that omit document content.
