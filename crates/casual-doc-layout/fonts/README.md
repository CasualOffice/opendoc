# Bundled fonts

Fonts embedded into `casual-doc-layout` for deterministic, WASM-safe text layout
(the shaper registers these into an empty font collection — no system-font
discovery — so layout metrics are reproducible on every host; see
`docs/43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md` §1).

| File | Family | License | Source |
| --- | --- | --- | --- |
| `Roboto-Regular.ttf` | Roboto | Apache-2.0 | [github.com/googlefonts/roboto-2](https://github.com/googlefonts/roboto-2) |
| `Roboto-Bold.ttf` | Roboto Bold | Apache-2.0 | same |
| `Roboto-Italic.ttf` | Roboto Italic | Apache-2.0 | same |
| `Roboto-BoldItalic.ttf` | Roboto Bold Italic | Apache-2.0 | same |
| `Caladea-Regular.ttf` | Caladea | Apache-2.0 | [github.com/huertatipografica/Caladea](https://github.com/huertatipografica/Caladea) (Huerta Tipografía; croscore) |
| `Caladea-Bold.ttf` | Caladea Bold | Apache-2.0 | same |
| `Caladea-Italic.ttf` | Caladea Italic | Apache-2.0 | same |
| `Caladea-BoldItalic.ttf` | Caladea Bold Italic | Apache-2.0 | same |

Every bundled family is licensed under the Apache License 2.0 — the same license
as this repository and within the `deny.toml` allowlist — so it may be
redistributed with the source. Each font's `name` table carries the Apache-2.0
license string (verified on import).

## Roles (font resolver, `P1C-002b`)

- **Roboto** is the default family and the ultimate fallback (`FontId(0)..=3`).
- **Caladea** (`FontId(4)..=7`) is metric-compatible with **Cambria** (matching
  advances, so line breaks are preserved); the resolver maps Cambria → Caladea.

### Why Carlito is *not* bundled

Calibri's metric-compatible partner is **Carlito**, but every published Carlito
build (Google Fonts, the croscore/`fonts-crosextra-carlito` package, LibreOffice)
is distributed under the **SIL Open Font License 1.1**, which is **not** in the
`deny.toml` license allowlist. To keep the bundle license-clean, Carlito is
deliberately omitted and **Calibri resolves to a documented visual fallback**
(Roboto) — reported as a substitution, never silently swapped. Dropping an
Apache-2.0 (or otherwise allowlisted) Carlito into this directory and adding it to
`FAMILIES` in `fonts.rs` is all that is needed to upgrade Calibri to a
metric-compatible match.
