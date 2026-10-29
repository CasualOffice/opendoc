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

The CPU renderer enforces the secure per-image dimension and decoded-pixel
defaults before PNG/JPEG decode and applies a matching 400,000,000-byte decoder
allocation ceiling. Media outside either bound follows the renderer's existing
graceful-skip behavior. Aggregate decoded-image caching remains host policy.

## PDF Import Defaults

Set 2026-10-29 from the P0 measurement (`docs/182` §16), whose artifact is
`fixtures/pdf/measurement.json` (regenerate with
`cargo run --release -p opendoc-pdf-measure -- measure fixtures/pdf --out fixtures/pdf/measurement.json`;
the `opendoc-pdf-measure` test fails if the committed numbers drift). Enforced by
`casual_doc_pdf_read::PdfReadLimits`; a breach is the typed refusal
`PdfReadError::Limit` with the limit's name, observed and allowed values, and no
partial document (PDFR-I3).

| Limit | Secure default | Hard ceiling | Measured basis |
| --- | ---: | ---: | --- |
| Input bytes | 256 MiB | 1 GiB | reuses the package input limit |
| Indirect objects | 500,000 | 4,000,000 | corpus max 5,002 (the page-flood file) |
| Pages | 2,000 | 10,000 | `docs/131` §14.2 provisional, kept |
| Page width or height | 14,400 pt | 72,000 pt | `docs/131` §14.2; the model's section cap is 1,584 pt, so a larger admitted page is clamped and reported |
| Decoded bytes per content stream | 32 MiB | 256 MiB | real-world corpus max page under 0.1 MiB; the 96 MiB bomb peaked at 235 MB heap |
| Decoded content bytes per document | 256 MiB | 1 GiB | as above |
| Operators per page | 1,000,000 | 4,000,000 | real-world max 19,274 per page; the 2,000,000-operator flood is refused |
| Evidence records per page (glyphs + paths + images) | 100,000 | 1,000,000 | `docs/131` §14.2 provisional, kept |
| Declared image pixels | 100 megapixels | 400 megapixels | reuses the image limit; real-world max 7.7 MP |
| Distinct fonts per document | 4,096 | 16,384 | real-world max 116 over 4 files |
| Embedded font program bytes | 32 MiB | 128 MiB | reuses the font limit |
| Form/pattern/Type3 nesting | 50 | 50 | hayro's fixed interpreter bound (`MAX_NESTED_INTERPRETATION_DEPTH`) |

Content-stream decoded size is checked with a bounded inflate **before** the
interpreter decodes a stream, so a bomb is refused without being expanded.
Encrypted files are refused (`pdf.import.encrypted_unsupported`), including
files with an empty user password. JavaScript, launch actions, embedded files
and form fields are never executed or extracted, and are reported.

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
