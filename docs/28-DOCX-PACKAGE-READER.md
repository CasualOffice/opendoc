# DOCX Package Reader

**Status:** Accepted for Phase 0
**Decision date:** 2026-07-24
**Tracker:** P0-006
**Implementation:** Complete on 2026-07-24

## Outcome

Add `casual-doc-ooxml` with a security-bounded, read-only DOCX package layer.
This slice validates ZIP structure and exposes safe on-demand part reads. It
does not parse WordprocessingML into the normalized document yet.

## Dependency Decision

Pin `zip` 7.2.0 with default features disabled and the pure-Rust Deflate path
enabled.

Reasons:

- `zip` 7.2.0 declares Rust 1.83 support, compatible with OpenDoc's Rust 1.85
  MSRV;
- the current 8.6.0 release declares Rust 1.88 and cannot be adopted without an
  explicit OpenDoc MSRV decision;
- default `zip` features include encryption and multiple codecs that DOCX does
  not require;
- exact pinning keeps a security-sensitive parser dependency reviewable.

Primary references checked 2026-07-24:

- [zip 7.2.0 package metadata](https://crates.io/crates/zip/7.2.0);
- [zip 8.6.0 documentation and MSRV](https://docs.rs/zip/8.6.0/zip/);
- [ZipFile path and size APIs](https://docs.rs/zip/8.6.0/zip/read/struct.ZipFile.html).

The crate's `enclosed_name` guidance informs the threat model, but OpenDoc uses
its own platform-independent package-path validation so macOS, Linux, Windows,
and WASM accept and reject the same names.

## Supported ZIP Profile

Accepted:

- single-disk ZIP and ZIP64 metadata handled by the dependency;
- stored entries;
- Deflate-compressed entries;
- regular files and zero-sized directory records;
- UTF-8 part names that satisfy the package-path rules below.

Rejected:

- encrypted entries;
- symlinks and non-file special entries;
- unsupported compression methods;
- overlapping entry data;
- malformed central or local records;
- macro project parts;
- archives missing the minimal DOCX package parts.

No entry is extracted to the host filesystem.

## Required Parts

The foundation requires:

- `[Content_Types].xml`;
- `_rels/.rels`;
- `word/document.xml`.

Later relationship parsing will resolve the office-document target instead of
assuming `word/document.xml`. The Phase 0 restriction is explicit and may
reject structurally valid packages with unusual office-document part names.

## Package Path Rules

Raw entry names must be valid UTF-8 and no longer than the configured path-byte
limit. OpenDoc rejects:

- empty names for file entries;
- NUL bytes;
- leading `/`;
- `\` separators;
- Windows drive prefixes;
- empty, `.` or `..` segments;
- duplicate normalized file part names.

Directory records may end in `/`; their non-empty segments follow the same
rules. Directory records are not exposed as package parts.

Percent escapes must contain two hexadecimal digits. Escapes for `/`, `\`, NUL,
`.` in a traversal segment, or ASCII unreserved characters are rejected rather
than normalized ambiguously. Full OPC Pack URI canonicalization is part of the
relationship/content-type parser slice.

## Limits

`PackageLimits` exposes the accepted defaults and hard ceilings from
`21-PARSER-LIMITS.md`:

| Limit | Default | Hard ceiling |
| --- | ---: | ---: |
| Input bytes | 256 MiB | 1 GiB |
| Entries | 10,000 | 50,000 |
| Total expanded bytes | 1 GiB | 4 GiB |
| Single expanded entry | 256 MiB | 1 GiB |
| Per-entry expansion ratio | 200:1 | 1,000:1 |
| Path bytes | 1,024 | 4,096 |

Host values may lower defaults but cannot exceed hard ceilings. Every limit has
a stable boundary name and reports observed and allowed values without document
content.

Central-directory metadata is checked before decompression. Expanded totals use
checked arithmetic. A non-empty entry with zero compressed bytes has an infinite
ratio and is rejected.

## On-Demand Reads

Opening returns metadata only. `read_part` allocates and decompresses one
validated part on demand:

1. find the exact normalized part;
2. cap the reader at declared size plus one byte;
3. read to an owned byte vector;
4. require actual bytes to equal the declared expanded size;
5. rely on the ZIP reader's CRC validation;
6. return no partial bytes on failure.

The package-wide expanded-size limit is based on declared metadata even when a
caller reads only one part. Repeated host reads are a runtime cache/budget
concern and do not weaken package admission limits.

Admission and part reads accept a cloneable cancellation token. Central-record
loops and 64 KiB decompression chunks check it cooperatively; cancellation
returns no package or partial part bytes.

## Public Boundary

`casual-doc-ooxml` remains internal for this slice. It exposes:

- `PackageLimits`;
- `DocxPackage`;
- immutable `PackageEntry` metadata;
- `read_part`;
- typed `PackageError`.

The SDK does not expose `open_docx` until XML, relationships, and model mapping
can produce a valid document session. SDK error mapping will use `ODC-1001`,
`ODC-1003`, and `ODC-0002` when that boundary is added.

## Fixture Policy

Tests use repository-owned generated fixtures. Generator source and manifest
records are committed with deterministic metadata and checksums. No sibling
repository or customer document is copied.

The first package-reader batch covers:

- minimal valid DOCX;
- mixed-Unicode document XML;
- unknown safe package-part enumeration and exact read;
- traversal rejection;
- high-expansion rejection;
- duplicate part rejection;
- malformed ZIP rejection;
- deterministic part ordering.

Semantic unknown-XML preservation and visual/round-trip expectations remain
blocked on the XML and model-mapping slices.

## Acceptance Gates

- valid minimal DOCX metadata and part bytes are deterministic;
- every package limit has accepted and rejected boundary coverage;
- unsafe, duplicate, encrypted, overlapping, macro, and unsupported entries are
  rejected before returning a package;
- actual part reads remain bounded and return no partial bytes;
- admission and decompression cancellation return no partial result;
- error text does not include document content;
- all seven generated fixtures have manifest checksums and source;
- native, Windows, macOS, WASM, MSRV, docs, lint, audit, and policy gates pass.
