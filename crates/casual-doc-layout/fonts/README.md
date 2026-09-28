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
| `Carlito-Regular.ttf` | Carlito | SIL OFL-1.1 | [github.com/googlefonts/carlito](https://github.com/googlefonts/carlito) (The Carlito Project Authors; croscore) |
| `Carlito-Bold.ttf` | Carlito Bold | SIL OFL-1.1 | same |
| `Carlito-Italic.ttf` | Carlito Italic | SIL OFL-1.1 | same |
| `Carlito-BoldItalic.ttf` | Carlito Bold Italic | SIL OFL-1.1 | same |
| `liberation/LiberationSans-Regular.ttf` | Liberation Sans | SIL OFL-1.1 | [github.com/liberationfonts/liberation-fonts](https://github.com/liberationfonts/liberation-fonts) (Red Hat / Google; `liberation-fonts-ttf-2.1.5`) |
| `liberation/LiberationSans-Bold.ttf` | Liberation Sans Bold | SIL OFL-1.1 | same |
| `liberation/LiberationSans-Italic.ttf` | Liberation Sans Italic | SIL OFL-1.1 | same |
| `liberation/LiberationSans-BoldItalic.ttf` | Liberation Sans Bold Italic | SIL OFL-1.1 | same |
| `liberation/LiberationSerif-Regular.ttf` | Liberation Serif | SIL OFL-1.1 | same |
| `liberation/LiberationSerif-Bold.ttf` | Liberation Serif Bold | SIL OFL-1.1 | same |
| `liberation/LiberationSerif-Italic.ttf` | Liberation Serif Italic | SIL OFL-1.1 | same |
| `liberation/LiberationSerif-BoldItalic.ttf` | Liberation Serif Bold Italic | SIL OFL-1.1 | same |
| `liberation/LiberationMono-Regular.ttf` | Liberation Mono | SIL OFL-1.1 | same |
| `liberation/LiberationMono-Bold.ttf` | Liberation Mono Bold | SIL OFL-1.1 | same |
| `liberation/LiberationMono-Italic.ttf` | Liberation Mono Italic | SIL OFL-1.1 | same |
| `liberation/LiberationMono-BoldItalic.ttf` | Liberation Mono Bold Italic | SIL OFL-1.1 | same |
| `NotoEmoji-Variable.ttf` | Noto Emoji | SIL OFL-1.1 | [github.com/google/fonts](https://github.com/google/fonts) (`ofl/notoemoji/NotoEmoji[wght].ttf`) |

Roboto and Caladea are licensed under the Apache License 2.0 — the same license
as this repository. Each carries the Apache-2.0 license string in its `name`
table (verified on import).

Carlito, the **Liberation** families and **Noto Emoji** are licensed under the
**SIL Open Font License 1.1** (each carries the OFL license record and URL, `name` ID 13/14,
`scripts.sil.org/OFL` — verified on import). The OFL is a permissive license that
governs only the font file, not this Apache-2.0 code, and carries **no** copyleft
or relicensing effect. The fonts are embedded as `include_bytes!` asset bytes,
not pulled in as a crate dependency, so `cargo-deny` does not scan them (no
`deny.toml` allowlist change is required). The OFL obligations are met by shipping
each license text unmodified alongside the fonts in
[`LICENSES/OFL-1.1-Carlito.txt`](LICENSES/OFL-1.1-Carlito.txt) and
[`LICENSES/OFL-1.1-Liberation.txt`](LICENSES/OFL-1.1-Liberation.txt) and
[`LICENSES/OFL-1.1-NotoEmoji.txt`](LICENSES/OFL-1.1-NotoEmoji.txt); we
redistribute the fonts unmodified and do not sell them standalone or reuse a
reserved font name for a modified font. (Noto Emoji's OFL declares no Reserved
Font Name, and the file is redistributed byte-for-byte as upstream publishes it
— only the filename is spelled without the variable-axis brackets, which the
licence does not govern.)

Provenance: the Carlito TTFs and `OFL.txt` were downloaded from
`googlefonts/carlito` (`main`, commit
`3a810cab78ebd6e2e4eed42af9e8453c4f9b850a`) under `fonts/ttf/`. The Liberation
TTFs and `LICENSE` were extracted from the official
`liberation-fonts-ttf-2.1.5.tar.gz` release tarball
(`github.com/liberationfonts/liberation-fonts`); each `.ttf` was verified as a
valid TrueType (sfnt version `0x00010000`) before committing.

## Roles (font resolver, `P1C-002b`)

- **Roboto** is the default family and the ultimate fallback (`FontId(0)..=3`).
- **Caladea** (`FontId(4)..=7`) is metric-compatible with **Cambria** (matching
  advances, so line breaks are preserved); the resolver maps Cambria → Caladea.
- **Carlito** (`FontId(8)..=11`) is metric-compatible with **Calibri** (matching
  advances); the resolver maps Calibri → Carlito. Calibri is the most common Word
  font, so this preserves layout for the majority of real documents.
- **Liberation Sans / Serif / Mono** (`FontId(12)..=23`) are LibreOffice's own
  metric-compatible substitutes for **Arial/Helvetica**, **Times New Roman**, and
  **Courier New**. The resolver maps those requested families (and, by generic
  class, other missing fonts) to them, so a document whose fonts are not installed
  breaks lines and paginates the way LibreOffice does — the fix for page-count and
  line-breaking divergence. The mapping table lives in
  `src/font_substitution.rs`; every non-exact substitution is reported, never
  silently swapped.
- **Noto Emoji** (`FontId(24)..=27`) is the **monochrome emoji base**, last in
  the fallback chain so a pictographic face can never win a code point a text
  face covers. Emoji carry no bold or italic, so all four ids address the same
  bytes and the shaper registers the blob once.

  It is deliberately the monochrome face, not a colour one. Colour emoji are
  large: Noto Color Emoji is 10.87 MB as CBDT bitmaps (9.24 MB brotli) and
  4.99 MB as COLRv1 (2.47 MB brotli), against 1.89 MB (1.12 MB brotli) for this
  outline face. So the base guarantees an emoji always draws *as itself* —
  offline, headless, on a machine with no emoji font, and before any network
  face arrives — and a host upgrades it to colour through
  `ParleyShaper::register_fallback_font`, the same seam the browser already uses
  for CJK. A face carrying colour glyphs is placed at the FRONT of the fallback
  chain, so the upgrade actually takes effect.

  A code point the base covers is still reported through
  `FontRegistry::missing_coverage`, because a monochrome stand-in for a colour
  emoji is a downgrade a host must be able to learn about and repair; without
  that, bundling the base would have silently switched the browser's colour
  fetch off.
