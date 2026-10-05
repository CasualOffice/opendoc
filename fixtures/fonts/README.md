# Test font faces

Font files that exist only so a test can create a condition. Not production
assets: nothing here is served to a reader, bundled into the engine, or
`include_bytes!`-ed by anything but a test target.

| File | Licence | Bytes |
| --- | --- | --- |
| `NotoSansCJK-TestSubset.otf` | SIL OFL 1.1 (`OFL-1.1-NotoSansCJK.txt`) | 8,652 |

## `NotoSansCJK-TestSubset.otf`

A 19-codepoint subset of **Noto Sans CJK SC**, covering one small sample of each
CJK script family:

| Script | Scalars |
| --- | --- |
| Han | 中 文 字 測 试 試(験) 日 本 語 漢 国 |
| Hiragana | あ い う |
| Katakana | ア イ ウ |
| Hangul | 한 글 |

Used by `crates/casual-doc-layout/tests/cjk_coverage.rs` and
`crates/casual-doc-render/tests/cjk_paint.rs` to prove, deterministically and
offline, that a face registered through the host seam
(`ParleyShaper::register_fallback_font`) is selected **because it covers the
code point** — checked against the face's own `cmap` — and that the renderer
rasterizes real glyphs rather than the notdef box. `109` HF-176.

### Why a subset and not the real face

The production face is 16.4 MB, and the owner's font-provisioning decision is
that the browser fetches CJK over the network rather than carrying it in the
bundle — so committing one would contradict the design the tests are there to
protect. Subsetting to exactly the scalars the tests name also keeps the fixture
honest about what it proves: it covers these 19 scalars and nothing else, so a
test cannot accidentally come to rely on coverage it was never given.

### It is NOT derived from any corpus document

The scalars above were chosen by hand to sample the four script families. The
owner's real CJK `.docx` files in `~/Downloads` are private, are excluded by
`.gitignore`, and nothing here is derived from them or quotes them.

### Regenerating

Requires `fonttools` (`pip install fonttools`) and the upstream face from
`github.com/notofonts/noto-cjk` at commit
`f8d157532fbfaeda587e826d4cd5b21a49186f7c`,
`Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf`:

```sh
python3 -m fontTools.subset NotoSansCJKsc-Regular.otf \
  --text="中文字測试试験日本語한글あいうアイウ漢国" \
  --output-file=NotoSansCJK-TestSubset.otf \
  --no-hinting --desubroutinize --drop-tables+=DSIG \
  --name-IDs='*' --notdef-outline --notdef-glyph
```

`--notdef-outline` is load-bearing and was missed the first time.
`pyftsubset` strips the `.notdef` outline by default, which left glyph 0 blank —
and `cjk_paint.rs` compares a real CJK glyph's raster against *this face's own*
`.notdef` raster, so a blank box would have made that comparison trivially true
and the guard worth less than it claims. The test asserts the `.notdef` has ink
precisely so that mistake cannot return silently.

`--name-IDs='*'` keeps the `name` table, including the OFL licence record
(ID 13/14) that the licence claim above is verified from.
