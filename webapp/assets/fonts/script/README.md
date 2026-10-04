# Document font faces, self-hosted

The faces the **document** needs to draw its own text, served from this origin.
Distinct from the editor *chrome* fonts one directory up (`../inter-*.woff2`,
`../material-symbols-outlined.woff2`), which style the UI rather than the page.

None of these are in the WASM bundle, and none are `include_bytes!`-ed by Rust.
They are fetched at runtime and registered into the engine's dynamic font
registry through `WasmDocument.registerFont` / `registerFallbackFont` — the one
host-populatable seam described in `crates/casual-doc-layout/src/font_registry.rs`.
The manifest is [`../../../src/web_fonts.mjs`](../../../src/web_fonts.mjs) and
it declares, for every face, the exact byte count and SHA-256 that
`fetchFontBytes` verifies before any bytes reach the shaper.

## Why they are here rather than on a CDN

Every one of these used to be fetched from `cdn.jsdelivr.net`. `109` HF-176.
That cost local-first (a firewalled or offline deployment could not render a
CJK, Arabic, Indic or Thai document, and the demo fixture's own ☐/☒ checklist
markers needed a third-party request), it cost supply-chain integrity (the URL
was commit-pinned but the bytes were never checked), and it leaked every
reader's IP plus a profile of which scripts their documents contain to a third
party.

`../../../tests/web_fonts.test.mjs` asserts that no **committed** face's primary
URL reaches a third-party host, that every committed file matches its declared
hash, and that a deployment which declares the remaining four provisioned serves
every one of the 24 itself.

## What is committed here, and what is not

| Group | Faces | Bytes | Committed? |
| --- | --- | --- | --- |
| Named Latin/Greek/Cyrillic | Roboto, Noto Sans, Noto Serif (upright + italic variable) | 9,726,952 | **yes** |
| Script fallbacks, small | Arabic, Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam, Sinhala, Hebrew, Thai, Symbols 2 | 2,504,980 | **yes** |
| CJK | Noto Sans CJK JP / KR / SC | 49,338,212 | no — provisioned |
| Colour emoji | Noto Color Emoji (COLRv1) | 4,991,984 | no — provisioned |

The totals are **derived** by `selfHostingBytes()`, not maintained here; the
table is a reader's summary and the test is the authority.

The named Latin faces are committed because they are fetched *eagerly, on every
editor load, for every reader, whatever script the document is in*. Mirroring
them would put a third-party request on the critical path of every single page
load. The CJK and emoji faces are 51.8 MB together and are only fetched by
documents that actually contain those scalars, so they are provisioned instead
of committed:

```sh
node webapp/tools/provision-script-fonts.mjs          # fetch + verify
node webapp/tools/provision-script-fonts.mjs --check  # verify only
```

Each is verified against the manifest's SHA-256 before it is written, and the
four filenames are named individually in the repository `.gitignore` so a
blanket `git add` cannot commit 51.8 MB that nobody notices in a diff.

A deployment that does not run the tool still renders CJK: those four faces are
fetched from their pinned upstream mirror instead, hash-verified on that path
too. The trade is a third-party dependency for CJK and colour-emoji documents
only.

After running the tool, tell the editor so, from a classic `<script>` ahead of
its module graph:

```html
<script>window.OPENDOC_PROVISIONED_FONTS = true;</script>
```

or with an explicit list, if only some were provisioned:

```html
<script>window.OPENDOC_PROVISIONED_FONTS = ["Noto-COLRv1.ttf"];</script>
```

**This declaration is required, and deliberately so.** Without it the four
mirrored faces are fetched from upstream even if they are sitting in this
directory. The alternative — always ask our own origin first and fall back on
failure — looks like the stronger rule and is worse: on an unprovisioned
deployment it is a guaranteed 404 and a console error for every mirrored face a
document needs. Nothing here speculates about what the origin holds.

## Licences

Every face is **SIL Open Font License 1.1**, verified from each file's own
`name` ID 13/14 licence record, as the bundled-font convention in
`crates/casual-doc-layout/fonts/README.md` does. The OFL is permissive, governs
only the font file and not this Apache-2.0 code, and carries no copyleft
effect. The faces are redistributed unmodified.

| Licence text | Covers |
| --- | --- |
| `OFL-1.1-Roboto.txt` | Roboto (upright + italic variable) |
| `OFL-1.1-NotoSans.txt` | Noto Sans (upright + italic variable) |
| `OFL-1.1-NotoSerif.txt` | Noto Serif (upright + italic variable) |
| `OFL-1.1-Noto.txt` | the 14 Noto script faces (upstream `fonts/LICENSE`) |

**Roboto here is the OFL build, not the Apache-2.0 one.** Google Fonts
relicensed Roboto to OFL-1.1 and serves it from its `ofl/` tree; the Roboto in
`crates/casual-doc-layout/fonts/` is the older Apache-2.0 build and a different
file. Both are redistributable alongside an Apache-2.0 project; they are just
not the same licence, and a reviewer reading one should not assume the other.

Noto Sans CJK and Noto Color Emoji — the provisioned faces — are also SIL OFL
1.1. A deployment that provisions them is redistributing them and should ship
the OFL text with them; `OFL-1.1-Noto.txt` is that text.

## Provenance

| Group | Source |
| --- | --- |
| Named Latin | `github.com/google/fonts`, `ofl/{roboto,notosans,notoserif}/`, commit `7ff85c87f93ea6cca5f41c69f2e4edcb90240f26` |
| Script fallbacks | `github.com/notofonts/notofonts.github.io`, `fonts/NotoSans<Script>/hinted/ttf/`, commit `eaa1a5cf8cb83ea73941197e492d659e51bb11dd` |
| CJK | `github.com/notofonts/noto-cjk`, `Sans/OTF/<Region>/`, commit `f8d157532fbfaeda587e826d4cd5b21a49186f7c` |
| Colour emoji | `github.com/googlefonts/noto-emoji`, `fonts/Noto-COLRv1.ttf`, commit `8998f5dd683424a73e2314a8c1f1e359c19e8742` |

Those commit shas live in `web_fonts.mjs` as the `*_REVISION` constants, so a
re-fetch is reproducible and the provisioning tool cannot drift from this table.

## What this does and does not fix

Registering a covering face fixes **coverage** — the scalar draws as itself
instead of as a notdef box. For CJK that is the whole problem: each ideograph,
kana and precomposed Hangul syllable is one cluster, one glyph, no reordering.

It is **not** the whole problem for Arabic or the Indic scripts, which need
contextual joining, reordering and conjunct formation. The engine shapes through
`harfrust` via `parley` and these faces carry the GSUB/GPOS tables shaping
reads, so the mechanism is present rather than stubbed — but no shaped-output
guard exists for either family. Treat them as "draws real glyphs, shaping
unverified", not as "supported".
