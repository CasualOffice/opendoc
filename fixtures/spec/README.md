# Specification data

Normative data from ECMA-376 / ISO-IEC 29500, vendored so a generated artifact can be
re-derived from a committed input rather than trusted (`105` EV-001).

## `presetShapeDefinitions.xml`

The 187 DrawingML preset shape geometries of ECMA-376 Part 1 §20.1.9 — for each preset,
its adjust-value list (`a:avLst`), its guide list (`a:gdLst`), its text rectangle and its
path commands. This is the data a `a:prstGeom@prst` token names.

**Provenance.** Retrieved 2026-10-02 from Apache POI, which redistributes it under the
Apache License 2.0:
`poi/src/main/resources/org/apache/poi/sl/draw/geom/presetShapeDefinitions.xml`
(<https://github.com/apache/poi>). The content is the specification's own normative
preset definitions; POI ships it without a separate notice, under its project licence.

**Why this copy and not another.** Three sources exist and only one is usable here:

- **Apache POI — used.** Apache-2.0, compatible with this project's licence.
- **The ECMA-376 download — equivalent content.** Freely published by ECMA, and the
  canonical origin. Equally legitimate; POI was taken because it is a single file at a
  stable path rather than an artifact to extract from a specification bundle.
- **ONLYOFFICE — deliberately NOT used.** Their `CreateGeometry.js` hand-transcribes the
  same 187 presets into ~9,000 lines of imperative builder calls. It is
  **AGPL-3.0-only**, and copying it into this tree would destroy the licence asymmetry
  that is the whole product position (`106` §2). ONLYOFFICE was read as a *behavioural*
  reference — which opcodes exist, which built-in variables a real implementation must
  recognise — and nothing was reproduced from it.

**Not edited.** Byte-for-byte as retrieved, so the generator's output can be checked
against it. Editing this file would make the generated table unverifiable, which is the
one thing it exists to prevent.
