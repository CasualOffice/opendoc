# Bundled fonts

Fonts embedded into `casual-doc-layout` for deterministic, WASM-safe text layout
(the shaper registers these into an empty font collection — no system-font
discovery — so layout metrics are reproducible on every host; see
`docs/43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md` §1).

| File | Family | License | Source |
| --- | --- | --- | --- |
| `Roboto-Regular.ttf` | Roboto | Apache-2.0 | [github.com/googlefonts/roboto-2](https://github.com/googlefonts/roboto-2) |

Roboto is licensed under the Apache License 2.0, the same license as this
repository, so it may be redistributed with the source. The fuller font set
(additional weights/styles, DOCX font-name matching, and fallback) is added by
the font-resolution slice (`P1C-002`, `docs/40-FONT-MANAGEMENT-DESIGN.md`).
