# opendoc-render

Batch corpus renderer for OpenDoc visual-regression checks. It runs the full
native pipeline for each `.docx` — import → `paginate_document` → `compose_page`
→ CPU raster — and writes one PNG per page to `<outdir>/<name>_p<page>.png`.

This is the committed generalization of the `casual-doc-render`
`render_docx_page` example: it loops over many files, wraps each in
`catch_unwind` (one bad file cannot abort the batch), and ends with an
`N ok, M error, K panic` tally.

Fonts are served through a `RegistryFontSource` taken from the shaper's dynamic
registry *after* pagination. The native OS system-font fallback tier is **on by
default** (through `casual-doc-render`'s `cfg(not(wasm32))` target dependency),
so CJK / complex-script / symbol runs rasterize from installed OS faces instead
of `.notdef` tofu — the whole point of a visual-regression tool.

## Usage

```
opendoc-render <outdir> <file.docx> [more.docx ...] [--dpi <f32>] [--max-pages <n>]
```

- `--dpi <f32>` — raster resolution (default `110`).
- `--max-pages <n>` — render at most `n` pages per file (default: all).

Example:

```
cargo run -p opendoc-render -- /tmp/render fixtures/corpus/*.docx --max-pages 2
```

Each line reports `OK <name>: <pages>p` with a compact disposition summary of
import report entries whose outcome was `Omitted`/`Degraded`, or
`ERROR <name>: …` / `PANIC <name>: …`.

**Note:** this is an evaluation tool, not a CI unit test. It renders
user-supplied files at runtime; no `.docx` or `.png` is committed.
