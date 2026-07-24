# Project Glossary

**Status:** Accepted for Phase 0
**Last updated:** 2026-07-24

This glossary defines terms used by the OpenDoc runtime, SDK, tests, and
documentation. Public API names should use these terms consistently.

## Product Terms

**OpenDoc**
The repository and open document-runtime project.

**Casual Document Runtime**
The embeddable engine produced by this repository.

**Host**
An application embedding the runtime. A host owns UI, storage, authentication,
network policy, telemetry, and platform integration.

**SDK facade**
The narrow public API used by hosts. Internal crates are not compatibility
boundaries.

**Document session**
An isolated live document with a current revision, selection, history, layout
state, resources, and event stream.

**Headless**
Operation without a product UI. Headless use includes validation, conversion,
inspection, rendering, and automated tests.

## Model Terms

**Normalized document**
The runtime's format-neutral semantic representation. It is not OOXML, HTML, a
DOM tree, or renderer state.

**Node**
A typed semantic item in a normalized document.

**Block**
A body-level node such as a paragraph or table.

**Inline**
Content inside a paragraph, such as text, a break, an image, or a field.

**Mark**
Formatting applied to text without changing block structure, such as bold or
italic.

**Node ID**
A stable 128-bit identity for a logical node. IDs are represented as 32
lowercase hexadecimal characters at serialization boundaries.

**Revision**
A monotonically increasing session-local version produced by a committed
transaction.

**Snapshot**
An immutable view of document or render state at one revision.

**Invariant**
A rule that every committed normalized document must satisfy.

**Extension bag**
Opaque, bounded, non-executable data retained to preserve safely attached
model extensions. It is not the DOCX source snapshot or preservation ledger.

## Editing Terms

**Position**
A node ID, local grapheme or child boundary, and affinity.

**Affinity**
The side of a boundary to which a position sticks when content is inserted at
that boundary.

**Range**
Two ordered positions describing document content.

**Selection**
One or more logical ranges plus direction and interaction state.

**Command**
A user- or host-level editing intention, such as inserting text or toggling
bold.

**Operation**
A validated structural mutation primitive produced by a command.

**Transaction**
An atomic, ordered group of operations against a declared base revision.

**Position map**
The mapping from positions before a transaction to positions after it.

**History entry**
A committed transaction plus inverse and selection metadata used for undo and
redo.

**Anchor**
A position with explicit behavior across edits, used by comments, bookmarks,
decorations, collaboration, and IME state.

## Layout Terms

**Layout**
The deterministic conversion of normalized semantic content and configured
resources into positioned fragments.

**Fragment**
The laid-out portion of a semantic node that fits in a line, column, or page.

**Pagination**
Assignment of layout fragments to page and column containers.

**Scene**
An immutable, backend-neutral display list plus hit-test and semantic data.

**Renderer**
A backend that paints a scene. Renderers do not inspect OOXML or mutate the
document.

**Hit testing**
Mapping a visual coordinate to a logical document position.

**Deterministic font set**
A versioned set of font files and fallback rules used for reproducible layout
and visual tests.

## Compatibility Terms

**DOCX**
An OPC ZIP package containing WordprocessingML, relationships, media, and
related OOXML parts.

**Source package snapshot**
An immutable, bounded record of an admitted package's parts, relationships,
hashes, and explicitly retained safe bytes.

**Provenance map**
Internal records connecting normalized semantic owners and properties to
source regions and mapping-rule versions without making OOXML locations model
identity.

**Preservation ledger**
Typed, bounded records for safely retained unsupported or source-specific
content, including ownership, order, invalidation, conflict, and future save
disposition.

**Mapping registry**
A versioned feature registry that owns source decoding, normalized mapping,
preservation, reverse mapping, dirty scope, security policy, and required test
evidence.

**Supported**
Imported, rendered, editable, and exported with defined semantics.

**Render-only**
Displayed, but editing is restricted to protect fidelity.

**Preserved**
Retained in validated source-snapshot or preservation-ledger data even though
the runtime cannot fully render or edit it. A warning alone is not preservation.

**Flattened**
Converted to a simpler representation with an explicit warning.

**Dropped**
Removed only with an explicit high-severity warning. Stable releases must not
silently drop content.

**Blocked**
Rejected because accepting the content would violate security or resource
policy.

**Compatibility profile**
A versioned feature matrix stating import, render, edit, save, and preservation
behavior.

**Corpus**
A rights-cleared set of document fixtures and expected outcomes.

**Round trip**
Importing a file and exporting it again, with or without semantic edits.

## Quality Terms

**Structural snapshot**
A deterministic machine-readable summary used to compare model or layout
behavior.

**Visual baseline**
An approved render result produced with fixed engine, configuration, and font
versions.

**Conformance report**
A versioned result set for compatibility, performance, and platform gates.

**Resource limit**
A configured parser or runtime bound whose breach returns a typed error.

**Warning**
A recoverable condition that may affect fidelity, behavior, or performance.

**Error code**
A stable public identifier with the `ODC-` prefix.
