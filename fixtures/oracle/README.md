# Oracle geometry references (docs/94 H2)

Each `<fixture id>.geom.json` is the per-page geometry LibreOffice produces for the
corpus fixture of the same id, reduced to the quantity the
`casual-doc-render::oracle_geometry` test compares against: page size, the text
region's bounding box, and the extent of the lines excluded from that box because
the pinned faces cannot shape them. `.toolchain` records the LibreOffice build
that produced them.

**These references arm a gate.** With them committed, the oracle comparison runs
on every pull request (`.github/workflows/ci.yml`, job `test`, step
"Oracle geometry gate"), and `the_oracle_gate_is_armed` fails the build if one is
deleted, staled, or made untrustworthy. Before they existed the harness was merged,
described as protecting rendering fidelity, and had never once executed — backlog
row FID-P-01.

## What is in a reference

```json
{
  "schema": 3,
  "fonts": ["LiberationSerif", "Symbol"],
  "pages": [
    {
      "sizeTwips": [11906, 16838],
      "contentBboxTwips": [1136, 566, 2961, 16261],
      "excludedLines": 2,
      "excludedExtentTwips": 571
    }
  ]
}
```

- `schema` — the extraction semantics. Bumped whenever a field's *meaning*
  changes; a reference at an older schema is compared on page size only and the
  test prints the re-bless instruction, rather than being hand-patched.
- `fonts` — the faces the producing PDF actually embedded, subset prefix stripped.
  This is the gate's font provenance: a reference naming a face that is neither a
  pinned metric-compatible family nor the reviewed non-parity bullet face is
  refused. Reading it off the artifact beats reading it off the producing machine,
  which has already gone wrong once (packages installed, fontconfig substituted,
  geometry ~17% wide).
- `excludedLines` — review context only, not compared: line grouping is
  per-renderer. `excludedExtentTwips` is the compared quantity.

## Provenance and rights

The inputs are `fixtures/corpus/real-producer-*.docx`, which this repository owns
under Apache-2.0 (see `fixtures/manifest.json`). A reference contains only numbers
measured from them — no LibreOffice code, no third-party content — so nothing here
carries a licence beyond the repository's own.

Only **Latin-only** fixtures may be referenced. `real-producer-libreoffice.docx`
is deliberately absent: its CJK and Arabic runs are shaped by whatever face each
environment substitutes, and a substitute of a different width also moves where
the rest of the paragraph *wraps*, so excluding the offending line is not enough.

## How these were produced, and the honest caveat

The committed set was produced by `scripts/oracle/extract-geometry.sh` under
**LibreOffice 26.2.4.2 on macOS/arm64** (the app bundle ships its own Liberation,
Carlito and Caladea faces, and `pdffonts` confirms those are the faces it embedded
— see each reference's `fonts`).

`.github/workflows/oracle-geometry.yml` regenerates them on **Linux** with the same
pinned build. Whether the two platforms agree to within the gate's 40-twip band has
not yet been measured, because the job has never run; the job now prints the diff
against the committed set precisely so that the first run measures it. If Linux and
macOS turn out to disagree, the honest fix is to bless on one platform only and say
so here — not to widen the tolerance.

What does *not* vary by platform is the other side of the comparison: our engine
shapes with bundled faces, and its geometry is already gated identically on Linux
and macOS by the H1 golden (`crates/casual-doc-layout/tests/geometry_snapshot.golden`).
Windows shapes differently and is excluded from the geometry comparison.

## Re-blessing

Run the `Oracle geometry re-bless` workflow, review the diff, merge the PR it
opens. Locally, with LibreOffice 26.2.4.2 plus `poppler-utils` on `PATH`:

```sh
scripts/oracle/extract-geometry.sh fixtures/corpus/real-producer-rich.docx \
  > fixtures/oracle/docx-real-producer-rich.geom.json
soffice --version > fixtures/oracle/.toolchain
```

**Never hand-edit a reference to make the gate pass.** A reference that moves is a
reviewed change with a stated cause. A divergence our engine owns is registered in
`KNOWN_DIVERGENCES` with its measured delta and tracker row — three are, all of
them accumulated vertical drift through tables and lists (FID-L-21) — never
absorbed by widening the tolerance.
