# Normalized Snapshot I/O

**Status:** Accepted for Phase 0
**Decision date:** 2026-07-24
**Tracker:** P0-003

## Outcome

Load and export normalized schema v0 JSON without making an unbounded generic
deserializer part of the public SDK boundary.

The loader must:

- reject oversized input before parsing;
- reject unknown v0 fields and invalid node IDs;
- validate model invariants;
- enforce semantic resource limits;
- create revision-zero sessions only from fully valid documents;
- allocate future node IDs without colliding with imported IDs;
- return stable, redacted SDK errors.

## Scope

Phase 0 implements compact UTF-8 JSON for diagnostics, tests, and controlled
host interchange.

It does not implement:

- canonical CBOR;
- schema migration;
- streaming JSON;
- partial/recovery loading;
- unknown-field preservation;
- operation-log loading;
- DOCX import.

Schema v0 remains experimental. Strictness is intentional: forward
compatibility begins with a versioned envelope and migration design, not by
silently ignoring fields.

## Public SDK Shape

```rust
let session = engine.open_normalized_json(
    bytes,
    OpenNormalizedOptions::default(),
)?;

let bytes = session.export_normalized_json()?;
```

Loading is synchronous in Phase 0 because the bounded v0 model is small and has
no I/O or resource requests. Async and cancellation become required before
large package or streaming import.

## Parse Pipeline

1. validate host-provided limits against non-bypassable hard ceilings;
2. reject input byte length above the configured limit;
3. parse one UTF-8 JSON value with recursion enabled only to serde's safe
   default;
4. reject unknown fields, duplicate typed fields, invalid enum values, malformed
   IDs, and unsupported schema version;
5. validate document invariants;
6. count blocks, text-run bytes, Unicode scalar values, extension entries, and
   extension payload bytes;
7. reject the first exceeded semantic limit with its stable limit name;
8. create a revision-zero session;
9. retain no input buffer or parser diagnostics containing document text.

No partial document session is returned after any failure.

## Limits

| Limit | Default | Hard ceiling |
| --- | ---: | ---: |
| Input JSON bytes | 64 MiB | 256 MiB |
| Body blocks | 2,000,000 | 8,000,000 |
| Unicode scalar values | 50,000,000 | 200,000,000 |
| Bytes per text run | 16 MiB | 64 MiB |
| Extension entries | 100,000 | 500,000 |
| Aggregate extension payload | 64 MiB | 256 MiB |

Hosts may lower defaults. Values above hard ceilings return
`ODC-0002 invalid_configuration`.

The JSON byte limit is enforced before `serde_json` allocation. Semantic limits
are defense in depth and compatibility constraints; they are not a substitute
for the pre-parse bound.

## Strict Schema Behavior

All typed v0 structs deny unknown fields. The root requires:

- `schemaVersion`;
- `documentId`;
- `body`;
- `extensions`.

Extension keys are deterministically ordered in memory and export. A future
extension registry will define key syntax, media types, and per-extension
validation. Current extension values remain inert bytes with a media type.

Duplicate fields on typed objects are rejected by deserialization. Duplicate
extension keys must also be rejected rather than resolved by last-write-wins.

## Deterministic Export

Export:

- validates the committed model before serialization;
- uses compact JSON with stable struct field order;
- uses document order for arrays;
- uses lexical ordering for extension maps and marks;
- emits fixed-width lowercase node IDs;
- emits no revision, timestamp, host path, or random metadata;
- returns the same bytes for equal normalized documents.

Session revision is runtime state and is not embedded in a normalized document
snapshot.

## Imported Identity

Imported node IDs are retained exactly.

Each session also has a runtime allocation namespace and monotonic counter. When
an editing operation needs a new node, the allocator skips candidates already
present in the imported document. Generated candidates are never reused after a
failed operation.

## Error Mapping

| Failure | Public code |
| --- | --- |
| invalid limit configuration | `ODC-0002` |
| malformed JSON, invalid ID, unknown field, unsupported v0 value | `ODC-1001` |
| model invariant failure | `ODC-1001` |
| input or semantic limit exceeded | `ODC-1003` |
| serialization/invariant failure of committed state | `ODC-2005` or `ODC-9001` |

Parser messages are not exposed verbatim because they may contain snippets of
document content. Safe context includes `limit_name`, `limit_value`,
`observed_value`, `schema_version`, and node ID.

## Rejected Alternatives

**Expose `serde_json::from_slice<Document>` directly**

Rejected because it has no SDK error stability, host limits, redaction, or
session invariant gate.

**Ignore unknown fields in v0**

Rejected because it creates accidental forward-compatibility behavior before
migrations and preservation are designed.

**Store runtime revision in the normalized file**

Rejected because revision is session history state, not semantic document
content.

**Use public SDK snapshots as the persistence schema**

Rejected because SDK snapshots are host projections and may evolve separately
from the normalized interchange schema.

## Acceptance Gates

- valid schema v0 JSON opens at revision zero;
- export/load/export is byte deterministic;
- unknown fields, invalid IDs, duplicate IDs, and unsupported versions fail;
- byte and semantic limits have accepted/rejected boundary tests;
- failed load creates no session;
- editing a loaded document allocates a non-colliding split node ID;
- errors contain no document text;
- native, WASM, MSRV, docs, lint, and policy gates remain green.
