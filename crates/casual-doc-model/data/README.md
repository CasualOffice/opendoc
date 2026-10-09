# `casual-doc-model/data`

## `preset_shapes.txt` — the 187 DrawingML preset geometries

The definitions behind every `a:prstGeom@prst` value (`ST_ShapeType`): adjust
defaults, guide formulas, adjust handles, text rectangle and paths, for all 187
preset shapes. `crates/casual-doc-model/src/v1/preset_shapes.rs` embeds the file
and compiles it, once per process, into the same geometry engine that draws an
authored `a:custGeom` (`docs/119`, `docs/109` FID-L-04).

### Where it comes from

| | |
| --- | --- |
| **Work** | ECMA-376 *Office Open XML File Formats*, Part 1, electronic annex `presetShapeDefinitions.xml` (the DrawingML preset geometry definitions) |
| **Copy used** | the file as redistributed by the Apache POI project (Apache Software Foundation), path `poi/src/main/resources/org/apache/poi/sl/draw/geom/presetShapeDefinitions.xml` on its `trunk` branch, fetched 2026-10-09 |
| **SHA-256 of that file** | `4a762444d8d85876881c02a5b1dedf6f73006fcd8acb7b4e393435615b37c780` (538 970 bytes) |
| **Generator** | `crates/casual-doc-import/examples/generate_preset_shapes.rs` |
| **Command** | `cargo run -p casual-doc-import --example generate_preset_shapes -- presetShapeDefinitions.xml > crates/casual-doc-model/data/preset_shapes.txt` |
| **Integrity** | the generator stamps the table body with an FNV-1a 64 checksum; the loader refuses a table whose body does not match it, and `a_hand_edited_table_is_refused` holds that |

The generator keeps `avLst`, `gdLst`, `ahLst`, `rect` and `pathLst` and drops
`cxnLst` (connection sites, which only connector routing consumes). It
normalizes formula whitespace and nothing else; every name, number and command
is the standard's.

### Terms

The definitions are part of an Ecma International standard. Ecma's bylaws
(§9.4) make every approved standard "available to all interested parties
without restriction", and Ecma's copyright licence expressly permits derivative
works "by making use of this specification in standard conformant products by
implementing (e.g. by copy and paste wholly or partly) the functionality
therein" — which is what this file is for — on condition that the copyright
notice and the licence accompany the derivative work. They do:
[`LICENSE-ECMA-376.txt`](LICENSE-ECMA-376.txt) beside the table, cited in the
repository's `NOTICE`. (The licence wording was checked verbatim against the
copy Ecma's TC39 publishes with its own standards, `tc39/ecmarkup`'s
`boilerplate/standard-copyright.html`.) Microsoft's Open Specification Promise
covers the patent side of ECMA-376.

The Apache Software Foundation ships this same file inside its Apache-2.0
licensed Apache POI release and records the basis in POI's own `LICENSE` file
("Office Open XML schemas … a part of the Office Open XML ECMA Specification
(ECMA-376) … available to all interested parties without restriction"). The
copy used here was taken from that ASF distribution so the provenance is a
published, licence-reviewed one rather than a transcription.

**Not used, deliberately:** ONLYOFFICE's hand-transcribed preset table
(`CreateGeometry.js`, AGPL-3.0) and LibreOffice's generated tables (MPL-2.0).
Neither was read for data; `docs/119` §3 records the one behavioural reading of
ONLYOFFICE's engine and what it was used for.

### One erratum in the source, kept as the source has it

`circularArrow`, `leftCircularArrow` and `leftRightCircularArrow` each contain
`+- xH 0 dxB 0` — four operands to a three-operand opcode. The table keeps the
standard's text; the engine ignores operands beyond an opcode's arity, because a
producer that embeds these definitions writes the same formula into documents
(`geometry_program.rs`, `operands_beyond_the_arity_are_ignored_as_the_standards_annex_requires`).

### Regenerating

Obtain `presetShapeDefinitions.xml` (the ECMA-376 Part 1 annex, or the Apache
POI copy above), check its SHA-256 against the value recorded here, run the
command, and commit the result. If the digest differs, record the new source and
digest in this file in the same commit; the loader's checksum makes a table that
was edited instead of regenerated fail the model's tests.
