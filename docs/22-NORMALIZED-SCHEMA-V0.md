# Normalized Schema v0

**Status:** Accepted for Phase 0 implementation
**Schema status:** Experimental, pre-compatibility
**Last updated:** 2026-07-24
**Tracker:** F-008, P0-001

## Purpose

Schema v0 establishes the first deterministic model contract. It is intentionally
small enough to implement and test before DOCX mapping begins, while preserving
the identity and extension principles needed by the full runtime.

Schema v0 is not a stable interchange promise. Compatibility guarantees begin
only when a later schema is explicitly frozen. Every v0 payload still carries a
version so migration behavior is tested from the start.

## Root Shape

```json
{
  "schemaVersion": 0,
  "documentId": "00000000000000010000000000000001",
  "body": [
    {
      "type": "paragraph",
      "id": "00000000000000010000000000000002",
      "inlines": [
        {
          "type": "text",
          "text": "Hello",
          "marks": ["bold"]
        }
      ]
    }
  ],
  "extensions": {}
}
```

JSON is a diagnostic and test representation. Field names use lower camel case.
The future binary representation uses canonical CBOR with the same semantic
fields and an independently versioned envelope.

## Identity

- node IDs are non-zero unsigned 128-bit values;
- JSON encodes IDs as exactly 32 lowercase hexadecimal characters;
- IDs are stable for the logical lifetime of a node;
- IDs are unique within a document;
- import-generated IDs use a runtime-owned generator, not array positions;
- serialization order does not affect identity.

The initial generator combines an explicit 64-bit namespace with a monotonic
64-bit counter. Hosts may provide deterministic namespaces for tests. A future
cryptographically random default source belongs behind the SDK host boundary.

## Initial Node Set

```text
Document
└── body: BlockNode[]
    └── Paragraph
        └── inlines: InlineNode[]
            └── TextRun
```

The initial implementation supports:

- a document with at least one block;
- paragraphs;
- text runs;
- `bold`, `italic`, `underline`, and `strike` marks;
- a bounded extension map reserved for future opaque values.

Tables, breaks, tabs, images, fields, sections, comments, notes, styles,
numbering, and format preservation are defined in the broader LLD but are not
claimed by the initial implementation.

## Normalization Rules

- a committed document body is never empty;
- an empty document contains one empty paragraph;
- node IDs are unique and non-zero;
- empty text runs are omitted;
- adjacent text runs with equal marks are merged;
- marks are unique and serialized in enum order;
- text is valid Unicode and stored without Unicode normalization;
- paragraph-breaking controls are represented by structural operations, not
  embedded in a text-insert operation;
- map-like fields use deterministic key ordering.

Unicode normalization is not automatic because changing code points can alter
author intent, search behavior, and source round-trip fidelity.

## Runtime Positions

Runtime text positions address a paragraph node and a zero-based extended
grapheme boundary. They do not use UTF-8 bytes or UTF-16 code units. Position
serialization belongs to the operation schema, not the document snapshot.

Affinity is `before` or `after` and determines mapping when content is inserted
at the exact position.

## Mutation

Snapshots are immutable. Commands create transactions; transactions validate and
atomically apply operations to a working copy. No public API mutates a node
collection directly.

The first operation is:

```text
InsertText {
  at: Position,
  text: String
}
```

It rejects an unknown node, an out-of-range grapheme boundary, stale base
revision, empty transaction, and text containing a paragraph, line, tab, or NUL
control that requires another operation.

## Deterministic Serialization

- arrays preserve semantic document order;
- maps use lexical key order;
- IDs use fixed-width lowercase hex;
- no timestamps, random values, pointer addresses, or platform paths appear;
- a serialize/deserialize/serialize cycle produces identical semantic output;
- unknown fields are rejected in strict v0 model loading until an explicit
  forward-compatibility envelope is designed.

Canonical CBOR is deferred until its exact encoding profile and golden vectors
are accepted in an ADR.

## Validation and Failure

Invalid input returns `ODC-1001`. An invariant failure found after a transaction
is `ODC-2005` and makes the affected session unusable. Validation diagnostics
identify node IDs and rules but omit surrounding document text by default.

## Exit Gate

Schema v0 is implemented when:

- constructors cannot produce invalid blank documents;
- validation detects duplicate/zero IDs and non-normalized runs;
- grapheme insertion works for combining sequences and emoji;
- transaction application is atomic;
- deterministic snapshots have golden tests;
- native tests pass and the model/transaction crates compile for WASM.
