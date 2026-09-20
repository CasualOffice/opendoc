# 110 — RTF Import Profile

**Status:** Accepted for first implementation increment
**Date:** 2026-09-20
**Parent:** `94-MULTI-FORMAT-IMPORT-EXPORT-ARCHITECTURE.md`
**Siblings:** `95-ODT-IMPORT-PROFILE.md`, `96-ODT-EXPORT-PROFILE.md`, `21-PARSER-LIMITS.md`

## 1. Purpose and scope

Define the first bounded Rich Text Format (`.rtf`) admission and semantic
import profile, and the crate seam it lands on. This is an implementation
contract, **not** a claim that RTF fidelity is complete. Section 9 is the
honest, family-by-family support matrix; section 10 is the published
limitations statement. Read those two before quoting any coverage claim.

**This increment is import only.** No RTF exporter is registered, the RTF
format descriptor advertises `can_export: false`, and the format never appears
in `FormatRegistry::export_formats()`. Section 11 records exactly what an
exporter would need so it can be added without reworking the import seam.

### Why RTF is in scope

RTF is a word-processing document interchange format, which is squarely inside
the product scope in `docs/10` and the working contract: *documents — DOCX, ODT,
TXT, JSON snapshot, and remaining document interchange formats.* It is also the
format most commonly produced by legacy line-of-business systems, mail-merge
tools and clinical/legal record exporters, so a document runtime that cannot
open one refuses a large class of real files.

### Why RTF does not reuse the package pipeline

DOCX and ODT are ZIP packages, so both go through `casual-doc-package`'s
`PackageLimits` and a manifest/part model. **RTF is not a package.** It is a
single 7-bit-clean byte stream of brace-delimited groups and backslash control
words:

```rtf
{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\froman Times New Roman;}}
{\colortbl ;\red255\green0\blue0;}
\pard\f0\fs24 Hello \b world\b0\par}
```

Consequently:

- there is no ZIP bomb axis, no manifest, no relationship graph, and no
  encrypted-package refusal to make;
- the hostile-input axes are instead **group nesting depth**, **stream length**,
  **declared binary object length** (`\binN`), **control-word and parameter
  length**, and **expansion counts** (paragraphs, inline nodes, table cells,
  pictures);
- detection is a byte-signature probe, not a package open.

So RTF gets its own crate, `casual-doc-rtf`, peer to `casual-doc-odf`, with its
own `RtfLimits`. It depends only on `casual-doc-model`; it pulls in **no new
third-party dependency**. The parser is hand-written: RTF's grammar is small
enough that a dependency would add supply-chain and licence surface (the
`dependency-policy` CI job judges any addition) for no correctness benefit,
and none of the available crates carry the loss-reporting the compatibility
report requires.

## 2. Crate seam

| Layer | Item | Responsibility |
| --- | --- | --- |
| `casual-doc-rtf` | `RtfLimits` | Host-configurable admission bounds with non-bypassable compiled hard ceilings. |
| `casual-doc-rtf` | `lexer` | One linear, non-recursive pass turning bytes into groups, control words, control symbols, hex escapes, binary payloads, and text bytes. |
| `casual-doc-rtf` | `import_rtf` | Semantic mapping into `casual_doc_model::v1::Document`, resources, and an `RtfCompatibilityReport`. |
| `casual-doc-rtf` | `code_page` | Generated single-byte code-page tables for `\'hh` decoding. |
| `casual-doc-io` | `RtfAdapter` | `FormatImporter` implementation: probe, admission, report translation, source retention. |
| `casual-doc-io` | `formats::RTF` | Stable format id `application.rtf`. |

`casual-doc-io` stays a dispatch layer and parses nothing itself, exactly as it
does for DOCX and ODT.

## 3. Detection

`RtfAdapter::probe` returns `Definite("rtf.signature")` when the input, after
skipping an optional UTF-8 BOM and leading ASCII whitespace, begins with `{`,
optional whitespace, then `\rtf` followed by an optional decimal version. Every
other input is `NoMatch("rtf.no-signature")`.

`Definite` matters: the plain-text adapter reports `Possible` for any valid
UTF-8, and a typical RTF file *is* valid UTF-8 (it is 7-bit ASCII with escapes).
Detection takes the highest confidence, so RTF wins the tie without needing a
filename hint. A `.rtf` whose bytes do not carry the signature is refused rather
than silently opened as plain text, because opening a control-word stream as
prose is exactly the "wrong characters" failure this profile refuses to ship.

`\rtf` versions other than `1` are admitted and reported
(`rtf.version.unexpected`, `Mapped`/`NotApplicable`): no version other than 1
has ever shipped, and refusing on it would reject files that are otherwise
ordinary.

## 4. Text decoding — the layer that must be correct

A document that opens with the wrong characters is worse than one that refuses
to open, so this layer is specified first and is the strictest part of the
profile.

RTF carries text three ways, and all three are implemented:

| Source | Handling |
| --- | --- |
| Literal ASCII bytes | Passed through. |
| `\uN` Unicode escape | `N` is a signed 16-bit value; negatives are re-read as `N + 65536`. Surrogate halves are paired across consecutive `\u` escapes into one scalar; a lone half is replaced with `U+FFFD` and reported. Each `\uN` is followed by `\ucM` fallback characters, which are consumed and discarded — `M` defaults to 1 and is per-group state. |
| `\'hh` hex byte | Decoded through the code page in force for the current run. |

**The code page in force** is, in order of precedence: the `\fcharset` of the
run's current font (`\fN` → the `\fonttbl` entry's charset), else the
document's `\ansicpg`, else the character set keyword (`\ansi` → 1252,
`\mac` → 10000, `\pc` → 437, `\pca` → 850), else 1252.

Tabulated single-byte code pages: **1250, 1251, 1252, 1253, 1254, 1255, 1256,
1257, 1258, 874, 437, 850, Mac OS Roman (10000), KOI8-R (20866)**. Every table
was generated from the authoritative Unicode mapping (a committed generator
run over the platform `codecs` tables), not typed by hand, and unit tests pin
representative code points in each so a corrupted table fails the build.

Two deliberate refusals rather than guesses:

- A byte a code page leaves **undefined** becomes `U+FFFD` and one
  `rtf.text.undecodable-byte` finding (`Degraded`). It is never guessed at
  through a different table.
- A **double-byte** code page (Shift JIS 932, GBK 936, EUC-KR 949, Big5 950)
  has no table here. Its `\'hh` bytes each become `U+FFFD` with
  `rtf.codepage.multibyte-unsupported` (`Degraded`).

  **This is the profile's largest real gap, and it is worse than "some
  characters are wrong".** Measured against the corpus in §9.5: the Korean
  (cp949) and Traditional Chinese (cp950) files in it import with 17,738 and
  11,673 replacement characters respectively, out of documents of roughly
  28,000 characters — i.e. **most of the text is lost**, because those
  producers wrote DBCS bytes and no `\uN` escapes at all. A CJK RTF written by
  a DBCS-era producer therefore opens structurally intact and substantially
  unreadable. It is reported, not silent, but the user's document is still
  gone. Double-byte tables are the top follow-up for this adapter, ahead of
  every unmapped formatting family.
- An unrecognised `\ansicpg` number is reported as
  `rtf.codepage.unsupported` and falls back to 1252 for ASCII-range bytes
  only; high bytes become `U+FFFD` with a finding.

`\tab`, `\line`, `\page`, `\~` (non-breaking space), `\-` (optional hyphen),
`\_` (non-breaking hyphen), `\{`, `\}`, `\\`, `\emspace`, `\enspace`,
`\qmspace`, `\bullet`, `\endash`, `\emdash`, `\lquote`, `\rquote`,
`\ldblquote`, `\rdblquote` are all mapped. A bare CR or LF in the stream is
ignored (RTF line wrapping); a backslash immediately followed by CR or LF is a
paragraph mark, per the specification.

## 5. Semantic mapping

| RTF source | Normalized destination | First-profile disposition |
| --- | --- | --- |
| `\par`, `\pard` | `Paragraph` + property reset | Mapped |
| `\ql \qr \qc \qj` | `ParagraphProperties.alignment` | Mapped |
| `\li \ri \fi` | `Indentation` (start/end/first-line; a negative `\fi` also sets `hanging_twips`) | Mapped |
| `\sb \sa \sl \slmult` | `Spacing` (before/after/line, `LineRule` from `\slmult` and the sign of `\sl`) | Mapped |
| `\tx \tb` + `\tqr \tqc \tqdec \tql`, `\tldot \tlhyph \tlul \tleq` | `TabStop` with alignment and leader | Mapped |
| `\keep \keepn \pagebb \widctlpar \nowidctlpar \noline` | `keep_lines`, `keep_next`, `page_break_before`, `widow_control`, `suppress_line_numbers` | Mapped |
| `\outlinelevel` | `ParagraphProperties.outline_level` | Mapped |
| `\rtlpar \ltrpar` | `ParagraphProperties.bidi` | Mapped |
| `\b \i \strike \ul` + `\ulnone` and the `\ul*` family | `RunProperties.bold/italic/strike/underline` + `UnderlineStyle` | Mapped for the eight modeled underline styles; other `\ul*` tokens degrade to `Single` with a finding |
| `\fs` | `size_half_points` | Mapped |
| `\cf` + `\colortbl` | `RunProperties.color` | Mapped. A colour-table entry with no `\red/\green/\blue` is RTF's *auto* colour and becomes `Color::Auto`, not black; an index the table does not define leaves the run colour unset |
| `\highlight` + `\colortbl` | `RunProperties.highlight` | Mapped on an **exact** sRGB match to one of Word's sixteen highlight names. A colour outside that set is carried as run shading and reported — never snapped to a nearest neighbour the author did not choose |
| `\f` + `\fonttbl` | `RunProperties.font_ref` (`FontRef::Named`) and `Definitions.font_table` | Mapped (name, family from `\froman`/`\fswiss`/…, pitch from `\fprq`, charset from `\fcharset`) |
| `\super \sub \nosupersub` | `RunProperties.vertical_alignment` | Mapped |
| `\up \dn` | `RunProperties.position_half_points` | Mapped |
| `\caps \scaps \outl \shad \v \impr \embo \striked1 \expndtw \kerning \charscalex \rtlch \ltrch` | matching `RunProperties` fields | Mapped |
| `\plain` | run-property reset | Mapped |
| `\paperw \paperh \margl \margr \margt \margb \headery \footery \gutter \landscape` | `SectionBoundary.page_size`, `page_margins`, `orientation` | Mapped (document-level; see below) |
| `\cols \colsx \linebetcol` | `SectionColumns` | Mapped (equal-width only) |
| `\sect \sectd` | a further `SectionBoundary` + `ParagraphProperties.section_break` | Mapped |
| `\trowd \cellx \cell \row \lastrow` | `Table` / `TableRow` / `TableCell`, grid from `\cellx` boundaries | Mapped |
| `\clmgf \clmrg` | `TableCellProperties.grid_span` | Mapped |
| `\clvmgf \clvmrg` | `TableCellProperties.vertical_merge` | Mapped |
| `\trleft \trrh \trhdr \trkeep \trql \trqc \trqr`, `\clvertalt/c/b`, `\clcbpat`, `\clbrdrt/l/b/r` + `\brdr*`/`\brdrw`/`\brdrcf` | row height/header/keep/alignment, cell vertical alignment, cell shading, cell borders | Mapped for exactly these. `\tcelld` resets the pending cell definition; `\trgaph` and `\trpadd*` cell padding are **reported** (`rtf.table.cell-padding`), not silently consumed |
| `\listtable`, `\listoverridetable`, `\ls`, `\ilvl` | `AbstractNumbering` / `NumberingInstance` / `NumberingRef` | Mapped for level number-format, start-at, justification and level text |
| `{\pntext …}`, `{\*\pn …}` | — | Dropped with `rtf.list.legacy-pn` (`Omitted`); the literal bullet glyph is deliberately **not** emitted as body text |
| `{\pict …}` `\pngblip \jpegblip \emfblip \wmetafile \dibitmap \wbitmap` | `Drawing` + `MediaReference` + resource bytes | Mapped for PNG/JPEG; metafile and bitmap forms are carried as bytes and reported as not renderable |
| `\binN` | consumed as bounded binary | Mapped inside `\pict`; elsewhere consumed and reported (`rtf.binary-data`) |
| `\nestcell` `\nestrow` `\nesttableprops` | — | **Dropped with `rtf.table.nested`**; nested tables are not built in this increment |
| `\brdrt/l/b/r` + `\brdr*` style, `\brdrw`, `\brdrcf` | `ParagraphBorders` | Mapped; `\box`, `\brdrbtw`, `\brdrbar` are reported |
| `{\info …}` `\title \subject \author \operator \keywords \doccomm \category \company \manager` | `DocumentProperties.core` / `.app` | Mapped. The statistics (`\nofpages`, `\nofwords`, `\nofchars`) and timestamps (`\creatim`, `\revtim`) are **not** mapped and are reported as unknown control words |
| `{\*\generator …}` | `DocumentProperties.app.application` | Mapped |
| `{\field{\*\fldinst …}{\fldrslt …}}` | the `\fldrslt` content, inline | **Degraded** — the visible result text is kept, the instruction is reported as dropped |
| `{\*\bkmkstart}` / `{\*\bkmkend}` | — | Dropped with a finding |
| `{\header*}` `{\footer*}` `{\footnote}` `{\*\annotation}` | — | **Dropped with a finding**, one per family; see §9 |
| `{\stylesheet …}` | — | Dropped with a finding; direct formatting is unaffected because RTF repeats it on every run |
| `{\*\<unknown>}` ignorable destination | — | Subtree consumed; reported once per distinct control word |
| `\objdata`, `\objemb`, `\objlink`, `{\*\shpinst}`, `\datastore`, `\themedata`, `\colorschememapping`, `\mmath`/`\*\mmathPr` | — | Consumed and reported; no embedded object, OLE payload, or drawing is executed, fetched, or modeled |

**Sections.** RTF page setup is document-level until the first `\sect`. The
importer therefore always emits at least one `SectionBoundary`, carrying the
document-level geometry, and emits a further boundary for each `\sect`. A
paragraph that ends a section carries `section_break`. When the document
declares no geometry at all, US Letter (12240 × 15840 twips) with one-inch
margins is used and reported as `rtf.section.defaulted` so the default is never
mistaken for source data.

## 6. Determinism and identity

- Node ids come from one `IdGenerator` whose namespace is an FNV-1a hash of the
  admitted source bytes, so the same bytes always produce the same ids and two
  different documents never collide by construction. No host filename, clock,
  or iteration order participates.
- Definition maps and compatibility findings are emitted in sorted order.
- Import is **atomic**: no document, resource, or partial report escapes after
  any lexical, bound, or model-validation failure. The model is re-validated by
  `Document::new` before the artifact is constructed, so an internally
  inconsistent import fails rather than producing a half-document.
- Adjacent runs that would carry identical properties are merged before
  construction, because `ModelError::AdjacentEquivalentTextRuns` makes an
  unmerged pair an invalid document, not a cosmetic issue.

## 7. Security and resource bounds

Every bound below is checked *during* the single pass, not after it. The first
fourteen are host-configurable fields of `RtfLimits`, each clamped by a
compiled hard ceiling a host cannot exceed; the last two are associated
constants and are not configurable at all, because a control word longer than
the specification allows is not a large document, it is a malformed one.

| Bound | Default | Hard ceiling | Why this axis exists |
| --- | ---: | ---: | --- |
| `max_input_bytes` | 64 MiB | 256 MiB | Matches the plain-text adapter and the browser's own 64 MiB open limit. |
| `max_group_depth` | 128 | 1024 | A hostile file can open thousands of `{` in a few kilobytes. The parser keeps an explicit stack and refuses past the bound, so no input can overflow the native or wasm call stack. Word itself has never nested past ~20 in practice. |
| `max_paragraphs` | 262 144 | 4 000 000 | 262 144 is the browser block ceiling measured in `docs/104` HF-158; making it the **default** means the browser is safe without the host having to remember to pass a smaller value. |
| `max_inline_nodes` | 2 000 000 | 32 000 000 | Runs, tabs and breaks expand independently of paragraph count. |
| `max_text_scalar_values` | 20 000 000 | 200 000 000 | Bounds decoded text separately from input bytes, because `\uN` escapes and `\'hh` bytes expand. |
| `max_table_rows` | 100 000 | 1 000 000 | `\row` count. |
| `max_table_cells` | 500 000 | 4 000 000 | `\cell` count; a single row may declare thousands of `\cellx`. |
| `max_fonts` | 4 096 | 65 536 | `\fonttbl` entries. |
| `max_colors` | 4 096 | 65 536 | `\colortbl` entries. |
| `max_list_definitions` | 4 096 | 65 536 | `\listtable` entries. |
| `max_pictures` | 4 096 | 65 536 | `\pict` count. |
| `max_picture_bytes` | 32 MiB | 128 MiB | Per-picture decoded payload. A `\binN` can **declare** a gigabyte: the declaration is first bounds-checked against the bytes actually remaining in the stream (refused as `Malformed`, allocating nothing), and the payload is then charged against this bound before it is copied. |
| `max_total_picture_bytes` | 64 MiB | 256 MiB | Aggregate, so a thousand 32 MiB pictures cannot pass the per-picture bound. |
| `MAX_CONTROL_WORD_BYTES` (constant) | 32 | — | The specification's own limit on a control word's letters. |
| `MAX_PARAMETER_DIGITS` (constant) | 10 | — | Bounds the integer scan; a longer run of digits is refused rather than wrapped, because a wrapped `\binN` length is a memory-safety-shaped bug. |
| `max_findings` | 4 096 | 65 536 | A hostile file must not turn the compatibility report into the memory-exhaustion vector. Findings aggregate by feature id and control word, so this bounds *distinct* features, not occurrences; a document that produces more is refused rather than under-reported. |

The parser makes **one linear pass** with O(1) work per byte and no
backtracking, so run time is linear in the admitted input length. There is no
input for which it loops.

## 8. Preservation and reporting

The adapter's `SourceEnvelope` is tagged with the RTF format id and the adapter
version. Under `retain_source` it owns the original bytes so a future exporter
can offer `ExactIfUnchanged`; `PreserveWhenSafe` is **not** advertised, because
there is no RTF writer yet to preserve anything into.

Every construct this profile does not map produces a `CompatibilityEntry` with
the two-axis outcome from doc 94 (`ModelOutcome` × `RetentionOutcome`). Silent
drops are a defect, not a shortcut: the acceptance gate in §12 requires a
finding for each unmapped family.

Picture bytes are placed in `ImportArtifact.resources` unconditionally — not
only under retention — so cross-format export (RTF → DOCX) can write real image
parts. This is the ODT lesson recorded in `crates/casual-doc-io/src/odt.rs`,
where images existed in a format-private side table and cross-format export
wrote zero-byte parts.

## 9. Support matrix — enumerated by family

Per the evidence rule in the working contract (§9 rule 3), **absence from a
support matrix is an overstatement by omission**, so this table lists the
families that are *not* done as well as those that are. "Mapped" means a user
sees it after opening the file; "reported" means a `CompatibilityEntry` exists
and the user can see it in the compatibility report.

### 9.1 Mapped

| Family | Detail |
| --- | --- |
| Text and encoding | ASCII, `\uN` (incl. surrogate pairs), `\'hh` over 14 single-byte code pages, `\ucN` skip counts, `\fcharset` per-font override, the escape-glyph family (`\~ \- \_ \bullet \endash \emdash \lquote \rquote \ldblquote \rdblquote \emspace \enspace \qmspace`) |
| Paragraphs | `\par`, `\pard`, alignment, indents (incl. hanging), spacing (before/after/auto/line + rule), tab stops with alignment and leader, keep/keep-next/page-break-before/widow-control, suppress-line-numbers, contextual spacing, outline level, RTL, edge borders |
| Runs | bold, italic, underline (8 styles), strike, double-strike, size, colour, highlight, font, super/sub, raise/lower, caps, small caps, hidden, outline, shadow, emboss, imprint, character spacing, scaling, kerning, RTL |
| Structure | `\line`, `\page`, `\tab`, `\sect`/`\sectd`, page size, margins, orientation, equal-width columns |
| Tables | `\trowd`/`\cellx`/`\cell`/`\row`, grid from cell boundaries, header rows, row height and rule, row alignment, keep-together, horizontal merge (`\clmgf`/`\clmrg` folded to `gridSpan`), vertical merge, cell width, cell shading, cell borders, cell vertical alignment |
| Lists | `\listtable` / `\listoverridetable` / `\ls` / `\ilvl`, per-level number format, start-at, justification and level text |
| Images | `\pict` with `\pngblip` and `\jpegblip`, hex and `\binN` payloads, size from `\picwgoal`/`\pichgoal` (falling back to `\picw`/`\pich`), `\picscalex`/`\picscaley` |
| Metadata | `\info` title, subject, author, operator, keywords, comment, category, company, manager, and `\*\generator` |

### 9.2 Admitted, carried, but not rendered

| Family | Outcome |
| --- | --- |
| `\wmetafile`, `\emfblip`, `\dibitmap`, `\wbitmap` pictures | Bytes reach `resources` and a `Drawing` is created, but the renderer has no WMF/EMF/DIB rasteriser, so `rtf.pict.not-renderable` is reported (`Degraded`). |

### 9.3 Admitted, degraded or dropped, and reported — **not** silently

This is the complete set of finding ids the importer can emit. Each is a
family that is not fully mapped; none of them vanishes without a report.

| Finding | Outcome | What it covers |
| --- | --- | --- |
| `rtf.section.header-footer` | `Omitted` | `\header`, `\headerl/r/f`, `\footer`, `\footerl/r/f` |
| `rtf.note` | `Omitted` | `\footnote` (footnotes and endnotes) |
| `rtf.annotation` | `Omitted` | `\*\annotation`, `\*\atnid`, `\*\atnauthor`, `\*\atndate`, `\*\atnref`, `\*\atnparent`, `\*\atrfstart`, `\*\atrfend` |
| `rtf.revision` | `Omitted` | `\*\revtbl`, `\revised`, `\deleted`, `\revauth`, `\revdttm` |
| `rtf.bookmark` | `Omitted` | `\*\bkmkstart`, `\*\bkmkend` |
| `rtf.field.instruction` | `Degraded` | `\field`, `\*\fldinst`, and the inline field characters `\chpgn`, `\chdate`, `\chtime`, `\chftn`, `\chatn`, `\sectnum`. The `\fldrslt` text is kept |
| `rtf.stylesheet` | `Omitted` | `\stylesheet`, and the `\s`/`\cs`/`\ds`/`\ts` references to it. Direct formatting is unaffected because RTF repeats it on every run |
| `rtf.shape` | `Omitted` | `\*\shp`, `\*\shpinst`, `\*\shptxt`, `\shpgrp`, `\shprslt`, `\*\background` |
| `rtf.object` | `Omitted` | `\object`, `\*\objdata`, `\objclass`, `\objname`, `\objalias`, `\objsect`, `\objtime`, `\result` — never decoded, executed, or fetched |
| `rtf.math` | `Omitted` | `\*\mmath`, `\*\mmathPr`, `\*\mmathPict`, legacy `\*\eqn` |
| `rtf.frame` | `Omitted` | Positioned frames: `\absw`, `\absh`, `\posx`, `\posy`, `\phpg`, `\pvpg`, `\dxfrtext` |
| `rtf.table.nested` | `Omitted` | `\nestcell`, `\nestrow`, `\nesttableprops` |
| `rtf.table.cell-padding` | `Omitted` | `\trgaph`, `\trpaddl/r/t/b`, `\trpaddfl/fr/ft/fb` |
| `rtf.paragraph.border` | `Omitted` | `\box`, `\brdrbtw`, `\brdrbar` (the four edge borders themselves *are* mapped) |
| `rtf.list.legacy-pn` | `Omitted` | `\pn`, `\pntext`, `\pntxta`, `\pntxtb`, `\pnseclvl` |
| `rtf.list.unresolved` | `Omitted` | A body `\lsN` with no matching `\listoverride`; no numbering reference is produced, so the document never carries a dangling one |
| `rtf.list.level-defaulted` | `Degraded` | A resolved list whose definition declares no `\listlevel` |
| `rtf.run.language` | `Omitted` | `\lang`, `\langfe`, `\langnp`, `\langfenp`, `\alang` — the model wants a BCP-47 tag and RTF supplies a Windows LCID |
| `rtf.run.underline-approximated` | `Degraded` | A `\ul*` variant outside the model's eight underline styles |
| `rtf.run.highlight-not-standard` | `Degraded` | A `\highlight` colour outside Word's sixteen named highlights; carried as run shading instead |
| `rtf.text.undecodable-byte` | `Degraded` | A `\'hh` byte the code page in force leaves undefined |
| `rtf.text.unpaired-surrogate` | `Degraded` | A `\uN` surrogate half with no partner, including one at end of stream |
| `rtf.codepage.multibyte-unsupported` | `Degraded` | `\'hh` under a double-byte code page (932, 936, 949, 950, 1361, 54936) |
| `rtf.codepage.unsupported` | `Degraded` | `\'hh` under a code page with no table here |
| `rtf.pict.not-renderable` | `Degraded` | `\wmetafile`, `\emfblip`, `\dibitmap`, `\wbitmap` — bytes are carried, but nothing rasterises them |
| `rtf.pict.unknown-format` | `Omitted` | A `\pict` declaring no known blip type |
| `rtf.pict.empty` | `Omitted` | A `\pict` with no payload |
| `rtf.binary-data` | `Omitted` | A `\binN` payload outside a `\pict` |
| `rtf.index-entry` | `Omitted` | `\:` (index subentry) and `\|` (formula character) |
| `rtf.upr.unicode-alternate` | `Omitted` | `\*\ud`, the Unicode half of a `\upr` pair; the ANSI half is imported so no text is lost |
| `rtf.metadata-store` | `Omitted` | `\*\datastore`, `\*\docvar`, `\*\userprops`, `\*\template`, `\*\filetbl`, `\*\protusertbl`, `\*\xmlnstbl`, `\*\xmlopen`, `\*\datafield` |
| `rtf.producer-table` | `Omitted` | `\*\themedata`, `\*\colorschememapping`, `\*\latentstyles`, `\*\rsidtbl`, `\*\expandedcolortbl`, `\*\panose`, `\*\falt`, `\*\fname`, and the other producer-private tables |
| `rtf.section.defaulted` | `Degraded` | The document declared no page geometry at all; US Letter with one-inch margins was used |
| `rtf.version.unexpected` | `Mapped` | `\rtfN` with `N` other than 1 |
| `rtf.destination.ignored` | `Omitted` | Any other `{\*\…}` ignorable destination; the control word is recorded on the finding |
| `rtf.control-word.unknown` | `Omitted` | Any other control word reaching body content; the control word is recorded on the finding |
| `rtf.control-symbol.unknown` | `Omitted` | Any other `\<symbol>` control symbol |

This table is **derived, not hand-maintained**. Re-derive it, and confirm it
still matches the implementation exactly, with:

```sh
grep -ohE '"rtf\.[a-z0-9.-]+"' crates/casual-doc-rtf/src -r | tr -d '"' | sort -u
grep -oE '^\| `rtf\.[a-z0-9.-]+`' docs/110-RTF-IMPORT-PROFILE.md \
  | sed 's/^| `//; s/`$//' | sort -u
```

Both lists must be identical; a difference in either direction is a support
matrix that overstates or understates, and both are wrong.

Findings aggregate: repeats increment `occurrences` rather than adding rows.
Unknown control words are reported when they reach **body** content; inside the
header tables (`\fonttbl`, `\colortbl`, `\listtable`, `\listoverridetable`,
`\info`, `\pict`) only the mapped subset in §5 is consumed and the rest is
ignored, which §9.4 states as a partial-family gap rather than pretending the
tables are complete.

### 9.4 Not done at all, and not claimed

Headers/footers, notes, comments, tracked changes, bookmarks, fields as
fields, styles as styles, shapes, OLE objects, equations, frames and text
boxes, **nested tables**, non-equal column widths, `\*\pn` legacy lists,
run language, document statistics and timestamps, cell padding, table-level
properties (`\tblPr`-equivalents), multi-byte code pages, RTF **export**, and
edit-tolerant preservation.

Two families are partial rather than absent, and are named here so the support
matrix is not read as complete:

- **the header tables.** `\fonttbl` maps name/charset/family/pitch; `\colortbl`
  maps RGB and "auto"; `\listtable` maps number format, start-at,
  justification and level text. Everything else in those tables — panose data,
  alternate names, level indents and follow characters, list templates — is
  consumed without a finding, because reporting per-entry table detail would
  bury the content findings that matter.
- **borders.** Cell and paragraph edge borders map style, width and colour.
  Border spacing, `\box`, between-borders and bar borders do not.

### 9.5 Interoperability evidence

The profile was measured against 25 real `.rtf` files written by Apple's RTF
producer and shipped with macOS (`/System/Library` and `/Library`, found with
`find /System/Library /Library -name '*.rtf'`), so the result is reproducible
on any machine rather than resting on fixtures written to match the parser.

| Measure | Result |
| --- | --- |
| Files admitted without error | 25 of 25 |
| Files importing with **zero** replacement characters | 23 of 25 |
| Files with replacement characters | 2 — the Korean and Traditional Chinese licences, both double-byte code pages (§4) |
| Largest file | 1,354,196 bytes → 6,398 blocks, 1,256,574 characters |
| Distinct finding families raised across the corpus | `rtf.control-word.unknown`, `rtf.field.instruction`, `rtf.section.defaulted`, `rtf.producer-table`, `rtf.codepage.multibyte-unsupported`, `rtf.destination.ignored` |

No file panicked, hung, or was refused. What this evidence does **not** cover:
Word-produced RTF, tables, lists, images, headers/footers and notes are
absent from this corpus, so the families in §9.1 beyond text and paragraph
formatting are covered by unit fixtures only, and the fidelity of a
Word-written `.rtf` is untested against a real producer.

## 10. Published limitations (interoperability status)

The RTF adapter opens a **bounded, deterministic subset** of RTF 1.9. It is not
a general RTF support claim, and the following must not be paraphrased away:

- **Import only.** Saving a document opened from `.rtf` produces DOCX, ODT,
  JSON, or TXT. There is no `.rtf` output.
- **The text layer is the guarantee — for single-byte code pages.**
  Paragraphs, runs, `\uN` escapes, the fourteen tabulated single-byte code
  pages, and the core character/paragraph formatting subset are complete for
  the families in §9.1. **A document in a double-byte code page (Japanese,
  Chinese, Korean) written without `\uN` escapes loses most of its text**, with
  a finding; see §4 for the measured numbers. Everything else is either
  reported or listed in §9.4.
- **Structure above paragraph level is partial.** Flat tables and lists are
  mapped; headers, footers, footnotes, comments and **nested tables** are
  dropped with findings, so an RTF whose content lives in those containers
  opens missing that content — the report says so, and the user is told.
- **No embedded code, ever.** OLE payloads, `\objdata`, shapes, macros and data
  stores are consumed as opaque bytes and discarded. Nothing is executed and no
  external resource is fetched, including field instructions that name a URL.
- **No schema validation exists to perform.** RTF has no grammar document to
  validate against; the admission bar is the lexical profile plus §7's bounds.

## 11. What an RTF exporter will need (so import does not have to be reworked)

The import seam is deliberately shaped so a writer is additive:

1. **The descriptor is the only switch.** `FormatDescriptor.can_export` is
   `false` and no exporter is registered. Registering an `RtfAdapter` as a
   `FormatExporter` and flipping the flag is the whole registry change; the
   descriptor is already constructed in one place.
2. **`SourceEnvelope` already retains the original bytes** under
   `retain_source`, so `ExportMode::ExactIfUnchanged` needs no import change.
   `RtfAdapter::retained_source_bytes` reads them today, so the retention is
   reachable and tested rather than a promise waiting on the writer.
3. **`RtfLimits` already separates input from output concerns.** A writer adds
   `max_output_bytes`, mirroring `PlainTextLimits`; nothing existing moves.
4. **What a writer must add:** a control-word emitter with the `\'hh`/`\uN`
   encoder as the inverse of §4 (including the `\ucN` fallback run every
   `\uN` must carry); a `\fonttbl` and `\colortbl` builder that assigns indices
   from `Definitions.font_table` and the colours actually used; `\pict`
   hex-encoding from `DocumentResources`; and a loss report for every model
   construct RTF cannot express (comments, content controls, math, anchored
   drawings, embedded objects).
5. **What the writer must *not* do:** round-trip through the retained bytes for
   a *changed* document. `PreserveWhenSafe` should stay unadvertised until
   edit-tolerant preservation is designed, exactly as doc 97 does for ODT.

## 12. Acceptance gates

This increment is complete only when:

1. the text layer in §4 has positive tests for ASCII, `\uN`, surrogate pairs,
   `\ucN` skipping, and each tabulated code page, with pinned code points;
2. every family in §9.3 has a test asserting the **finding exists**, not merely
   that the import succeeded;
3. hostile inputs — truncated stream, unbalanced braces in both directions,
   nesting past `max_group_depth`, a `\binN` declaring more bytes than exist,
   and one declaring a huge length — are each refused or handled cleanly with a
   typed error, with no panic, no hang, and no unbounded allocation;
4. every new guard has been driven **red** by mutating the production code, and
   the red output is recorded in the commit;
5. a dedicated `rtf_import` fuzz target compiles under the independent fuzz
   lockfile and asserts the returned document re-validates. It is armed in the
   `fuzz-build` job of `.github/workflows/ci.yml`, which also refuses a stale
   `fuzz/Cargo.lock`;
6. the workspace format (pinned toolchain), strict Clippy, test, doctest,
   rustdoc, and `wasm32-unknown-unknown` check gates pass.

## 13. Normative references

- Microsoft, *Rich Text Format (RTF) Specification, version 1.9.1* (2008).
- Microsoft, *[MS-OI29500]* for the WordprocessingML semantics the normalized
  model mirrors.
- `docs/94-MULTI-FORMAT-IMPORT-EXPORT-ARCHITECTURE.md` for the adapter contract
  and the two-axis compatibility outcome vocabulary.
- `docs/21-PARSER-LIMITS.md` for the house rule that every admission bound is
  host-configurable beneath a compiled hard ceiling.
