# 114 — Proofing: client-side spelling and grammar, a product glossary, a personal dictionary

**Status:** **Implemented.** Designed 2026-09-21, built 2026-09-23. The design below is
kept as written except where the implementation found it wrong, and every such place is
marked **CORRECTION** with what was done instead and why — three of them are substantive
(§2.3, §5.3, §5.4) and one changed a number rather than a decision. **Owner:** unassigned.
**Row:** `109` **HF-035** (P1, L) / `105` **OO-003**.

> **SCOPE CHANGED, 2026-09-23.** This document was written "spelling only — grammar is a
> separate row and is not designed here". The owner reversed that mid-build, in their own
> words: *"also in dictonaries /spelling try to add industry glossary dictionary and
> industry names.. and more imp atlearn our product names .. fo it wont flag that as
> incorrect spelling.. more imp is grammer rules than spelling.. as just spelling will
> loose alot of this is not that imp"*.
>
> So the scope is now **spelling + a product/industry glossary + grammar**, with grammar
> rated the most important of the three. §11 and §12 are the two new sections; everything
> above them is the original spelling design with its corrections marked. Grammar ships as
> **the seam plus a first rule set** — §11 says exactly which classes of error are covered
> and which are not, and the largest uncovered one (subject–verb agreement with a NOUN
> subject) is named rather than implied.

### What shipped

| Piece | Where |
| --- | --- |
| The rules: tokenizer, skip list, capitalization, suggestion generation and ranking, the debounce, the cache | `webapp/src/spelling.mjs` (pure, in `PURE_MODULES`) |
| The wiring: lazy fetch, windowed scan, overlay markers, context-menu rows | `webapp/src/spell_check.mjs` |
| The word lists and their generator | `webapp/dict/{en-US,en-GB}.txt`, `webapp/dict/LICENSE.SCOWL.txt`, `webapp/tools/build-dictionary.mjs` |
| The personal dictionary | `openWordStore` in `webapp/src/drafts.mjs`, a `words` store at database version 2 |
| The product/industry glossary and its generator | `webapp/dict/glossary.txt`, `webapp/tools/build-glossary.mjs` (§12) |
| The grammar rules | `webapp/src/grammar.mjs` (pure), painted and gated by `spell_check.mjs` (§11) |
| `w:lang` exposed to the host | `languageAt` in `crates/casual-doc-wasm/src/lib.rs` |
| Guards | `tests/spelling.test.mjs`, `tests/grammar.test.mjs`, `tests/spell_check.test.mjs`, `tests/word_store.test.mjs`, `tests/dictionary_artifact.test.mjs`, `tests/glossary_artifact.test.mjs`, `tests/source_bytes.test.mjs`, `tests/e2e/spell-check.spec.mjs`, and a Rust unit test for `shared_language_in_range` |

Deliberately **not** shipped, and why, in §8 (spelling) and §11.3 (grammar). Two
additions to §8: **header, footer, note and text-box stories are still body-only**, and
**`w:noProof` is not honoured yet**.

## 1. What has to be true

The editor gives no spelling feedback at all, which is less than a plain `<textarea>`
offers. What has to exist:

- a red squiggle under a misspelled word, painted without disturbing layout;
- right-click on a flagged word → suggestions, Ignore once, Ignore all, Add to dictionary;
- a personal dictionary that survives a reload;
- the checking language taken from the document's `w:lang` where present, with a default;
- an off switch that is reachable and remembered;
- **no typing-latency cost.** `docs/107` §4: per-keystroke work is O(1) in document size.

And two constraints that come from the project rather than the feature:

- **No server.** ONLYOFFICE retired their server spell service and ship `spell.wasm`
  client-side (SKILL.md §1), so this is provably a client-side problem, and local-first is
  one of the three structural advantages that must not be traded away.
- **Apache-2.0-compatible dictionary data.** The licence is the product's whole wedge.

## 2. The dictionary — the decision with the longest lead time

This is the part that can block the approach, so it is settled first.

### 2.1 What was rejected

| Option | Why not |
| --- | --- |
| A Hunspell engine in JS (`nspell`, `hunspell-asm`) | `webapp/package.json` has **zero runtime dependencies** — only `@playwright/test` as a dev dependency. Adding the first runtime npm dependency to the editor is a decision for the owner, not a side effect of a spelling row. |
| Hunspell `.dic` + `.aff` with our own affix expander | The `.dic` is stems plus affix flags, so it is useless without implementing `SFX`/`PFX` and then `NEEDAFFIX`, `ONLYINCOMPOUND`, `CIRCUMFIX`, `COMPOUND*` and `REP`. That is a subsystem, and it buys nothing a pre-expanded list does not already have. |
| A server-side check | Forbidden: §1. |
| macOS `/usr/share/dict/words` | Not shippable, not reproducible, and it lacks inflections while carrying archaic junk. |

### 2.2 What to use — SCOWL's pre-expanded word lists

**Source:** SCOWL 2020.12.07, `final/` directory.
`https://downloads.sourceforge.net/wordlist/scowl-2020.12.07.tar.gz` (2,569,810 bytes,
fetched and verified 2026-09-21). The upstream repository is `en-wl/wordlist`; note that a
**git checkout does not contain `final/`** — those files are generated by the Makefile from
`agid`, so the **release tarball** is the artifact to use, not the git tree.

`final/` holds already-inflected plain word lists, one word per line, split by dialect
(`english-*`, `american-*`, `british-*`, `british_z-*`, `canadian-*`, `australian-*`),
by kind (`-words`, `-upper`, `-contractions`, `-proper-names`, `-abbreviations`) and by
SCOWL size level (`.10` … `.95`). No affix flags, no expansion step.

**Licence — this is the load-bearing part.** From `README_en_US.txt` in
`LibreOffice/dictionaries/en/`, which is the same data:

> The English dictionaries come directly from SCOWL and is thus under the same copyright of
> SCOWL. … Copyright 2000-2018 by Kevin Atkinson
>
> Permission to use, copy, modify, distribute and sell these word lists, the associated
> scripts, **the output created from the scripts**, and its documentation for any purpose is
> hereby granted without fee, provided that the above copyright notice appears in all
> copies and that both that copyright notice and this permission notice appear in
> supporting documentation. Kevin Atkinson makes no representations about the suitability
> of this array for any purpose. It is provided "as is" without express or implied
> warranty.

That is an MIT/BSD-shaped, attribution-only permissive licence, and it names *the output
created from the scripts* explicitly — which is what a generated word list is. It carries
no copyleft and no share-alike, so it is compatible with an Apache-2.0 project. Parts of
SCOWL derive from Ispell (Geoff Kuenning's BSD licence) and from Moby Words II (placed in
the public domain); both notices are reproduced in the SCOWL `Copyright` file and both are
permissive.

**So the licensing question is answered: this is clean, and it is the only part of the
design that had a real chance of blocking the approach.** What ships with it:

- `webapp/dict/LICENSE.SCOWL.txt` — the full notice, verbatim, unmodified;
- a provenance note naming the release, its URL, its byte size and its SHA-256;
- attribution in the About dialog is *not* required by the licence (it requires the notice
  in copies and supporting documentation) but is cheap and should be there anyway.

**Do not** switch to the LibreOffice/Hunspell `en_*` packages for convenience: the word
lists there are the same SCOWL data, but their `.aff` files are a modified Ispell file and
some LibreOffice dictionary *bundles* carry GPL/LGPL components. Take the data from SCOWL
directly so the provenance is one hop and one licence.

### 2.3 Which words, and how big

Measured on this machine (`node` over the extracted `final/` directory; possessive `'s`
forms dropped because they are handled by rule in §5.3):

| Level | Language | words | plain text | gzip -9 |
| --- | --- | --- | --- | --- |
| ≤35 | en-US | 39,309 | 349,420 B | 103,598 B |
| ≤50 | en-US | 71,700 | 652,975 B | 198,876 B |
| **≤60** | **en-US** | **87,997** | **820,013 B** | **243,730 B** |
| ≤60 | en-GB | 89,400 | 838,440 B | 247,114 B |
| ≤70 | en-US | 128,616 | 1,231,045 B | 366,840 B |

> **CORRECTION — the measured sizes are smaller than the table above.**
> Re-running the same file set over the same release through the committed generator
> (`node tools/build-dictionary.mjs --scowl <dir>`) gives **83,775 words / 788,290 B** for
> en-US and **85,344 / 808,262 B** for en-GB, not the 87,997 / 820,013 B in the table. The
> difference is in how the possessive filter was counted when the table was written by
> hand; 29,504 `'s` forms are dropped per the rule below. The generator prints these
> numbers on every run and `tests/dictionary_artifact.test.mjs` re-derives the artifact's
> shape from the committed files, so the figures are now derived rather than transcribed
> (SKILL.md §8). The decision the table supports — level ≤60 — is unchanged.

**Take level ≤60.** That is exactly the level the official Hunspell `en_US` dictionary uses
("The normal (non-large) dictionaries correspond to SCOWL size 60"), so it is the tier that
has been shipped and error-checked for twenty years. Level 70 adds words that are *more
likely to be misspellings of common words* — the README names `ort` and `calender` — which
trades false negatives for a worse product.

File sets, per language:

- `en-US` = `english-{words,upper,contractions,proper-names}` +
  `american-{words,upper,proper-names}`, levels 10–60.
- `en-GB` = `english-{words,upper,contractions,proper-names}` +
  `british-{words,upper,proper-names}` + `british_z-{words,upper}`, levels 10–60.

### 2.4 How it ships

- Committed as plain text, one word per line, sorted: `webapp/dict/en-US.txt`,
  `webapp/dict/en-GB.txt`. 820 KB each on disk; git's own zlib brings the pack cost to
  roughly the gzip column, so ~500 KB of repository for both. Plain sorted text is chosen
  over a binary or front-coded encoding because it is reviewable and diffable, and the
  wire cost is the same — GitHub Pages serves `text/plain` gzipped.
- **Fetched lazily**, on the first check, for the one language in use — never both, and
  never at boot.
- `webapp/` is uploaded wholesale by `.github/workflows/pages.yml` (`path: webapp`), so a
  new `webapp/dict/` directory deploys with no workflow change, and `serve.py` serves it
  in e2e with no config change.
- **Cache-busting with no build change.** `stamp-assets.py` versions `.js`/`.mjs`/`.css`/
  `.wasm`/images, and would not touch a `fetch()` of a `.txt`. It does not need to: the
  module doing the fetch is itself stamped through the import map, so
  `new URL(import.meta.url).search` is `?v=<build>` at runtime and empty in dev. Resolve
  the dictionary relative to the module and re-append that search:

  ```js
  const stamp = new URL(import.meta.url).search;            // "?v=0123abcd" or ""
  const url = new URL(`../dict/${lang}.txt`, import.meta.url);  // query is dropped here
  await fetch(`${url}${stamp}`);
  ```

- The file format carries a **second section** for suggestion ranking (§5.4): the common
  tier (SCOWL ≤35) first, then a line containing only `---`, then the rest. Both sections
  sorted, so the file stays diffable and the parse is one `indexOf` plus two `split`s.
- `webapp/tools/build-dictionary.mjs` generates the files and is committed with them, so
  the artifact is **derived, not hand-maintained** (SKILL.md §8). It records the release
  URL, the SHA-256, the level, the file set and the filters, and re-running it must be
  byte-identical.

### 2.5 Languages other than English

`en-CA`, `en-AU`, `en-NZ`, `en-IE` → the `en-GB` list. Any other `en-*` → `en-US`.
**Anything not `en-*` is not checked**, and the status bar must say so by name ("No
spelling dictionary for fr-FR") rather than silently showing a clean document — SKILL.md
§10, never a dead control, and a checker that quietly stops is worse than one that is off.
Adding a language later is adding one file plus one row in the language map; nothing else.

## 3. Cost — the constraint that shapes the algorithm

`docs/107` §4 is a hard owner constraint: **per-keystroke work is O(1) in document size.**
Two consequences:

1. **Nothing on the keystroke path may touch the dictionary.** The keystroke handler does
   exactly three constant-time things — add the caret's paragraph id to a dirty set, clear
   a timer, set a timer. This is the shape `drafts.mjs`'s `DraftScheduler.noteDirty()`
   already uses for the same reason; copy it rather than inventing a second cadence.
2. **Checking is windowed, like everything else that walks the document.** The accessibility
   mirror was the last thing in this codebase that projected the *whole* document, and it
   cost 87% of the time to open a 16,000-paragraph file and killed the tab at 65,000
   (`docs/104` HF-158, `docs/113` §8.6). A spell checker that scans the document on open
   would reintroduce exactly that. It scans **the paragraphs in the page window** and
   caches per paragraph.

The word being typed is deliberately **not** flagged until the caret leaves it — Word's
behaviour, and it also means the common case of typing costs nothing at all.

## 4. The seams this needs (all verified in the tree)

| Need | Seam | Notes |
| --- | --- | --- |
| Paint a mark over text without touching layout | `page.overlay` (`.overlay` in `style.css`), the transparent layer above the raster canvas | The page is a WASM-rendered RGBA bitmap blitted onto `<canvas>` (`main.js` ~3805). Nothing can be drawn *into* it from the host, and nothing should be: the overlay is where the caret, selection, comment markers and revision markers already live. |
| Geometry for a word | `doc.selectionRects(node, start, node, end)` → flat `[page, x, y, w, h]` twips | Exactly what `paintReviewMarkers()` uses (`main.js` ~4184). Painting a squiggle is the same operation as painting a comment underline. |
| Paragraph plain text | `doc.copyText(node, 0, node, doc.paragraphLength(node))` | The pattern `paragraphTextForFind()` already uses (`main.js` ~14684). |
| Word bounds at a caret | `doc.wordAt(node, offset)` → `[start, end]` bytes | Already used by double-click select. |
| Next / previous paragraph | `doc.moveCaret(node, doc.paragraphLength(node), "right")` / `doc.moveCaret(node, 0, "left")` | Crosses paragraph, line and page boundaries, and crosses into table cells. |
| Which paragraph is at the top of a visible page | `doc.hitTest(pageNumber, xTwip, yTwip)` → `{node, offset, zone}` | `main.js` ~4041. |
| Which page a paragraph is on | `doc.caretRect(node, 0)[0]` | |
| Replace the flagged range | the same `doc.replaceRanges` path find/replace uses | One undoable action; fails closed in Viewing; routes through the suggestion path in Suggesting. Do **not** add a new mutation path. |
| Right-click context | `contextAt(anchor, link)` → `buildContextCommands(context)` (`main.js` 7377) | Add a `misspelling` field to the context and a block of rows at the top. |
| Command in ≥2 surfaces | `editorCommands()` (`main.js` 12544) + `APP_MENU_SECTIONS.tools` in `command_taxonomy.mjs` | `tools: [["tools.smartQuotes"], ["view.settings"]]` is the obvious home for `tools.spellCheck`; that also puts it in the palette. `menu_taxonomy.test.mjs` enforces the two agree. |
| Remembered off switch | `settings` / `DEFAULT_SETTINGS` (`main.js` 17325), persisted to `opendoc.settings` | |
| Personal dictionary | `webapp/src/drafts.mjs` — the existing IndexedDB store (`opendoc-drafts`, version 1, stores `meta` + `bytes`) | Add a `words` store at **version 2** with an upgrade path that creates it without disturbing the two existing stores. The store takes its `indexedDB` by injection so `drafts.test.mjs` runs it in node with no browser — keep that property. |

### 4.1 The one engine gap: `w:lang` is not exposed

`RunProperties.language` exists in the model (`crates/casual-doc-model/src/v1/properties.rs`
~1302, a `Language { value, east_asia, bidi }` retained opaquely), the importer parses
`w:lang` (`crates/casual-doc-import/src/properties.rs:186`), and `overlay_run` in
`crates/casual-doc-layout/src/cascade.rs` (~474, `set!(language)`) carries it through the
style cascade. **But nothing exposes it to JS** — there is no `js_name` export mentioning
lang or language anywhere in `casual-doc-wasm/src/lib.rs`, and `Format` (the struct
`formatAt`/`caretFormat` return) carries only the four toggles.

The minimal addition, which needs **no new helper in any other crate**:

```rust
/// The effective `w:lang` (Latin `w:val`) shared by every run the range covers,
/// or `""` when the range is unset or mixes languages. Read-only.
#[wasm_bindgen(js_name = languageAt)]
#[must_use]
pub fn language_at(&self, node: &str, start: u32, end: u32) -> String { … }
```

It is implementable in ~18 lines on top of the existing private
`effective_run_properties_in_range(&self.document, nid, start, end)` (`lib.rs` ~16299),
which already resolves document defaults, paragraph style, character style and direct
formatting through `StyleCascade::resolve_run`. Place it next to `document_metadata`
(~6766), **not** next to `format_at`/`move_caret` — the caret-movement region of that file
was being changed concurrently for the vertical goal column, and a 28,000-line file only
conflicts when two additions land in the same place.

Calling convention that keeps it cheap: one call per paragraph over its whole range. If it
returns a tag, every word in that paragraph uses it. If it returns `""` the paragraph
mixes languages, and only then fall back to one call per word **in that paragraph**.

`paragraphLanguages(node) → [[start, end, tag], …]` would be nicer, but it needs
`flatten_run_segments`, which is private to `casual-doc-edit`; making it `pub` to serve a
host convenience is a worse trade than a second call on the rare mixed paragraph.

**If the engine is unavailable, the feature still ships**: fall back to
`settings.spellLanguage` (default `en-US`) for every paragraph, and record in the code that
`w:lang` is not being honoured yet. Do not sniff `navigator.language` — it makes the tests
non-deterministic for no user benefit.

> **SHIPPED as designed**, with the shared-language decision split into a free function
> `shared_language_in_range` so it can be unit-tested without a `Document` handle — the
> same shape `effective_run_properties_in_range` is already tested in. `spell_check.mjs`
> still guards `typeof doc.languageAt === "function"`, so the host runs against an older
> engine build with the `settings.spellLanguage` fallback rather than throwing.

## 5. The algorithm

### 5.1 Modules

| File | Kind | Holds |
| --- | --- | --- |
| `webapp/src/spelling.mjs` | **pure**, add to `PURE_MODULES` in `module_seams.test.mjs` | Tokenizer, the skip rules, the capitalization rule, candidate generation, ranking, and the dictionary-file parser. No DOM, no engine — unit-testable in node. |
| `webapp/src/spell_check.mjs` | DOM | Lazy dictionary fetch, the windowed scan, the per-paragraph cache, overlay painting, suggestion-on-demand, the context-menu rows' behaviour. |
| `webapp/src/drafts.mjs` | DOM (existing) | The personal-dictionary store, added to the existing IndexedDB seam. |

### 5.2 Scanning

- **Enumerate**: `hitTest` the vertical middle of each page in the window (definitely body,
  not the header band), then walk `moveCaret` backwards and forwards, bounded, until
  `caretRect(node, 0)[0]` leaves the window. ~4 WASM calls per paragraph, ~30 paragraphs
  per page, and nothing proportional to document size.
- **Cache** `nodeId → { text, lang, misspellings: [{start, end, word}] }`. A cache entry is
  valid while `copyText` returns the same string, so scrolling back is free and an edit
  elsewhere invalidates nothing. Cap the map (~400 paragraphs) and evict furthest-from-
  window, so a long session cannot grow it without bound.
- **Trigger**: the page-window change that already drives `paintPagesInView()`, plus the
  debounced dirty set from the keystroke path (~400 ms idle).
- **Scope v1 to body text.** Header, footer, footnote, endnote and text-box stories are
  separate stories; `moveCaret` should not be assumed to walk out of one. Say so in the
  code and in the row: it is a stated gap, not an ambiguity. (This is the same boundary
  `docs/85` Location work has to cross anyway.)

### 5.3 Deciding a word is wrong

Tokenize on the JS string, then convert to the engine's **UTF-8 byte offsets**
(`byteOffsetToStringIndex` already exists for the other direction). Skip, and say why in
the code:

- anything containing a digit (default-on option, as `105` OO-003 asks);
- ALL-CAPS words (default-on option);
- anything that looks like a URL, an email or a path. **The corpus contains
  `info@docscentre.com`**, so `@` must never be used as a test probe marker either; use
  something like `QZX`;

  > **CORRECTION — the rule as designed cannot fire.** It said "a token containing `://`,
  > `@`, or a `.` between two letters". None of those characters is a word character, so
  > such a token never exists: the tokenizer splits `http://x.com` into `http`, `x`, `com`
  > and `info@docscentre.com` into `info`, `docscentre`, `com`. Four of those six are not
  > English words, so every URL and every email address in a document would have been
  > squiggled. Shipped instead as `addressSpans(text)`, which finds the addresses in the
  > TEXT — schemes, emails, dotted hosts and file names, and paths — and drops any token
  > that overlaps one. Guarded by a named test using that exact corpus string's shape.
- single characters;
- the word containing the caret (see §3);
- words in the personal dictionary or the session ignore set.

Capitalization follows the standard Hunspell rule, not a lowercase comparison: a **lower-
case** dictionary entry matches the word in lower case, Title Case or ALL CAPS; a
**Title-case** entry matches only Title Case and ALL CAPS. So `london` is flagged while
`London` is not, which is what Word does. Possessives are handled by rule: strip a trailing
`'s` / `’s` (straight and curly) and re-check the stem — that is why §2.3 drops the 15,000
`'s` forms from the file.

### 5.4 Suggestions — computed on demand only, never while checking

This is the design's other cost decision. Suggestions are produced **when the context menu
opens**, for one word.

- Generate Damerau-Levenshtein distance-1 edits (deletion, transposition, replacement,
  insertion) and keep those in the dictionary: ~`54n + 25` candidates for an `n`-character
  word, so ~600 `Set` lookups — microseconds.
- Only if that yields fewer than three, go to distance 2.

  > **CORRECTION — generating the distance-2 neighbourhood is 100× slower than estimated.**
  > The design put it at "~360,000 lookups, tens of milliseconds". Measured against the
  > shipped 83,775-word en-US list it is **1.5–3.5 seconds** per right-click: the cost is
  > building ~200,000 candidate strings, not looking them up. That would lock the tab.
  > Shipped instead as a **bounded Damerau-Levenshtein scan of the dictionary**, filtered
  > by length (±2) and by the first two characters, with an early abort when the row
  > minimum passes the bound — the standard approach, and the one aspell's own second pass
  > is. Measured **2.6–19.5 ms median** on the same words (`recieve`, `seperate`,
  > `docuemnt`, `acomodate`, `definately`, `occurence`), and it ranks better because it
  > knows each candidate's true distance instead of inferring it from which pass found it.
- Rank by: distance, then same first letter, then the common tier from §2.4, then the
  EDIT KIND, then length difference, then alphabetical. All of it pure and unit-tested with
  named cases (`recieve` → `receive`, `teh` → `the`, and four more).

  > **CORRECTION — the common tier alone does not get `teh` → `the`.** The design named
  > that case and the ranking it specified fails it: `tea`, `tee`, `ten` and `the` are all
  > one edit away, all in the common tier and all start with `t`, so the tie fell through
  > to alphabetical order and `the` came **fourth**. Fixed by ranking the edit KIND —
  > transposition, then a character too many, then a substitution, then a character missing
  > — which is the order Hunspell's own suggester tries (`swapchar` before `badchar` before
  > `forgotchar`). Also added: a candidate shorter than three characters is not offered for
  > a word of four or more, because `teh` → `Th` is noise.
- **Zero suggestions must still show a row**: a disabled "No spelling suggestions" entry,
  never an empty menu (SKILL.md §10).

### 5.5 The context menu

At the **top** of the menu, above clipboard, when `context.misspelling` is set — Word and
Docs both lead with the suggestions:

```
recieve                 ← up to 5 suggestion rows, or one disabled "No spelling suggestions"
receiver
─────────
Ignore once
Ignore all
Add to dictionary
```

The squiggle element itself is `pointer-events: none`, exactly like `.review-comment-marker`
(REVIEW-GAP-005), so the right-click falls through to the page's own hit-testing and the
flagged word is resolved from the caret anchor. A marker that swallowed the event would
break caret placement — that defect has already been fixed once in this file.

Ignore once is per occurrence and dies when the paragraph changes; Ignore all is
per session and per word, not persisted (Word's behaviour); Add to dictionary is persisted.

### 5.6 Off

`settings.spellCheck`, default **on**, persisted in `opendoc.settings` with the other
preferences. Off clears every marker, stops the scan, and keeps the command enabled so it
can be switched back on. Reachable from Tools ▸ Check spelling, the command palette, and
the right-click menu — three surfaces, against the ≥2 floor.

### 5.7 Colour

One new token, `--paper-spelling`.

> **CORRECTION — it belongs in the paper layer, not in the three theme blocks.** The
> squiggle is painted onto the document sheet, which is white in BOTH themes; `style.css`
> already has a `/* ---- Paper layer ---- */` group at `:root` for exactly this
> (`--paper-insert`, `--paper-delete`, `--paper-comment`), declared once and deliberately
> theme-independent. Declaring it in the three theme blocks as designed would have put a
> pale dark-theme red on white paper — the defect the paper layer's own comment warns
> about — and would also have failed `tests/style_tokens.test.mjs`, which requires the
> three blocks to define exactly the same token set.

`tests/style_tokens.test.mjs` still enforces that no colour literal escapes the palette
blocks, which is why the squiggle is two gradients over a token rather than a data-URI SVG. The squiggle is drawn from that token with two `linear-gradient`s on a
`::after`, so there is no data-URI SVG carrying a hex and no image request:

```css
.overlay .spell-error { position: absolute; pointer-events: none; }
.overlay .spell-error::after {
  content: ""; position: absolute; inset-inline: 0; bottom: -1px; height: 3px;
  background-image:
    linear-gradient(45deg,  transparent 40%, var(--paper-spelling) 40% 60%, transparent 60%),
    linear-gradient(-45deg, transparent 40%, var(--paper-spelling) 40% 60%, transparent 60%);
  background-size: 4px 3px; background-repeat: repeat-x;
  background-position: 0 100%, 2px 100%;
}
```

## 6. The `main.js` line ratchet — plan for it before writing code

`webapp/src/main.js` is **18,106** lines and `MAIN_JS_LINE_CEILING` in
`tests/module_seams.test.mjs` is **18,108**. That is **two lines of slack**, and the
ceiling must never be raised. The wiring this feature needs in `main.js` — imports, a
settings default, the context field, the menu rows, the command, the paint hook, the
keystroke hook — is roughly **50 lines**.

So an extraction is part of this work, not an afterthought. The best-shaped candidate found
while surveying is **Insert ▸ Symbol / Emoji pickers**, `main.js` 13850–14289 (~439 lines):
it is two curated data arrays plus one `createGlyphPicker` factory that already takes all
its DOM ids as parameters and returns `{ open }`, and `main.js` keeps only two one-line
wrappers. `bookmark manager` (13255–13558, ~303 lines) and `format painter`
(9785–10086, ~300 lines) are the alternatives; the format painter is the most coupled of
the three to selection state and should be last choice.

> **WHAT HAPPENED.** By 2026-09-23 the picker factory had already been extracted to
> `glyph_picker.mjs`, leaving the two curated data arrays — 242 lines — in `main.js`. Those
> moved to `webapp/src/glyph_sets.mjs` and were added to `PURE_MODULES`: pure data, zero
> behavioural risk, and already covered end to end by the Symbol/Emoji specs. The file was
> **17,927** lines against a ceiling of **17,927** — no slack at all, not the two the
> design measured — and finished at 17,797 with the spelling wiring in, so the ceiling was
> lowered to that. The wiring came to ~110 lines rather than the ~50 estimated, because
> the context-menu block, the on/off setter, the add-to-dictionary reporting and the
> settings-panel control were each larger than a line-count guess.

## 7. How the guards must be written

SKILL.md §4 — write the guard, mutate the production code to reintroduce the defect, see it
go red, record the output, restore. For this feature specifically:

- **Assert the positive case first.** "No squiggle on a correctly spelled word" passes when
  the feature does not exist at all. The first assertion must be that a misspelling
  *produces* a marker.
- **Assert through the paint layer.** The document text is not in `#pages` — the page is a
  canvas. `.overlay .spell-error` is a real element in the same layer existing specs already
  read (`.overlay .caret`, `.overlay .highlight`), so it is a legitimate target; carry the
  flagged word on a `data-` attribute so the assertion names the word.
- **Use `page.keyboard.insertText`, not `keyboard.type`** — `type` is `preventDefault`ed for
  printable characters and the element under test never receives them.
- **Prove page 5.** Every browser spec here works in the first viewport near the start of
  the document. Because §5.2 is windowed, "does a misspelling on page 5 get a squiggle" is
  a genuinely different question from page 1, and `pageSheet(page, n)` in
  `tests/e2e/fixtures.mjs` already knows how to scroll there.
- **Prove the cost claim, do not assert it in prose.** The keystroke path claim in §3 is
  falsifiable: instrument the scan, type N characters, assert the scan ran zero times
  before the debounce elapsed. `docs/107` §4 numbers are the budget.
- Never use `@` as a probe marker (§5.3).

## 8. What is deliberately not in this design

- **Grammar.** A separate row and a much larger problem. Nothing here should be named
  "proofing" or generalised towards it.
- **A spelling dialog / "check whole document" walk.** The squiggle plus the context menu
  is the interaction people notice in the first minute; a modal document sweep is a second
  increment, and it cannot use the windowed scan as-is.
- **Header, footer, note and text-box stories** (§5.2).
- **Custom dictionary management UI** (view, remove, export). Add-to-dictionary persists;
  a management surface is a follow-up. Until it exists, removing a word means clearing
  site data, which should be stated where the command lives rather than left to be
  discovered.
- **Languages other than English** (§2.5), and per-run language spans on mixed paragraphs
  beyond the fallback in §4.1.
- **Autocorrect.** Adjacent, tempting, and a different row.

Two more, found while building and stated here rather than discovered later:

- **`w:noProof`.** The model already carries the run flag and the importer already
  reads it, but the checker does not ask. A run Word has marked "do not proof" is
  currently checked like any other. One `RunProperties` field away, and it needs the same
  `languageAt`-shaped export to reach the host.
- **Accessibility.** A squiggle is an overlay `<div>` with no text, so a screen-reader
  user is told nothing: they can neither find a misspelling nor know one is there. The
  right-click rows are reachable (the context menu is keyboard-operable), but only if you
  already know to open it on that word. Word exposes misspellings through UIA and there is
  no browser equivalent for a canvas-rendered document, so the honest answer is a
  navigation command ("next misspelling") plus a live-region announcement — the same shape
  `review.next` already has for tracked changes, and the same follow-up problem `109`
  HF-178 records for the accessibility mirror. Not in this PR.

## 9. Open questions — answered where implementation forced an answer

1. **Does the owner accept ~820 KB of committed word list per language?** The alternative is
   generating it in `build.sh` from a fetched tarball, which makes the build depend on the
   network and on SourceForge staying up — worse for a local-first project, and it would
   fail a clean-room offline build. Recommendation: commit it.
   **Taken as recommended**: 788 KB + 808 KB committed, fetched lazily for the one language
   in use. Still the owner's to reverse; the generator makes that a one-command change.
2. **Is one surgical read-only method in `casual-doc-wasm` (§4.1) acceptable**, or should the
   first increment ship with the `settings.spellLanguage` fallback and honour `w:lang` in a
   second PR? The feature is useful either way; the method is ~18 lines and needs all six
   Rust gates.
   **Shipped in this PR.** `languageAt` plus a free `shared_language_in_range` and its unit
   test; all six Rust gates run. The host still degrades to the preference if the method is
   absent, so an older engine build is not a broken editor.
3. **Proper nouns.** SCOWL level ≤60 includes `english-proper-names` and
   `american-proper-names`. Including them means fewer false positives on names; excluding
   them means `london` is caught. Recommendation as designed: include them, and rely on the
   Hunspell capitalization rule (§5.3) so case still carries information.
4. **Where does the off switch live in the ribbon**, if anywhere? The Home band has ~55 px
   of slack at 1280 px and widening anything exiles a group into the `⋯` overflow
   (SKILL.md §11), so the design above deliberately gives it menu, palette and context-menu
   reach and no ribbon button. Confirm that is enough.
   **Shipped with no ribbon button, and one more surface than designed**: Tools ▸ Spell
   check, the command palette, and a checkbox in Settings ▸ Spelling — which is also where
   the "nothing is sent anywhere, and a word you add can only be removed by clearing site
   data" note lives. Still open for the owner: whether a ribbon home is wanted.
5. **Should the personal dictionary be per-origin only?** It lives in IndexedDB, so it is
   per-origin and per-browser, and an embedder gets its own. That is probably right, but it
   means a user's dictionary does not follow them, and the storage decision (D-1, one store,
   the opencalc shape) did not cover this case.

## 10. What the implementation found that the design did not ask about

Recorded here rather than only in the PR, per SKILL.md §8 ("document as you go").

1. **A scan on a timer must ask for its own repaint.** The scan runs ~90 ms after the page
   window moves and ~400 ms after an edit — i.e. always AFTER whatever repaint the scroll
   or the edit already did. Without a repaint of its own its results never reached the
   screen: scrolling to page 5 found the misspelling and drew nothing. It is guarded by a
   signature of what `paint` would draw, so an idle rescan that found the same words does
   not rebuild every overlay in the window for nothing.
2. **A one-shot status message is not enough for a standing condition.** "No spelling
   dictionary for fr-FR" was announced once by the first scan — and then the web-font
   upgrade re-rendered, `renderAll` wrote and cleared its own progress message, and the
   explanation was gone while the condition was still true. The user is then looking at an
   unchecked document with no reason given, which is precisely what the message exists to
   prevent. `renderAll` now asks `spellChecker.statusNote()` when it clears.
3. **The personal dictionary must not be autosave's to throw away.** Both live in the same
   IndexedDB database, but autosave can be off — by preference, or by policy in a
   cross-origin embed — and "turning autosave off deletes what is stored" must not take the
   user's own words with it. They are separate openers over one schema, and a test asserts
   `drafts.clear()` leaves the words alone.
4. **An extraction can leave a dangling reference, and only the browser sees it.**
   Moving the bookmark manager out of `main.js` (to pay for the grammar wiring
   under the ratchet) took `bookmarkEntries` with it — and the LINK dialog called
   it, to offer the document's bookmarks as internal targets. Insert ▸ Link threw
   `ReferenceError: bookmarkEntries is not defined` and never opened. `main.js`
   has no static analysis and there is no JS linter here, so nothing but a
   browser run could have said so.
   `command-activation-contract.spec.mjs` is the file whose entire purpose is
   "a control a user can see must do something" — and it swept straight past
   this, because Insert ▸ Link is greyed out without a selection and the sweep
   correctly skips greyed rows. So the contract had a hole exactly the shape of
   "every command that needs a selection". It now loads with a selection too and
   activates the DIFFERENCE, which is a handful of commands rather than a second
   full sweep. Mutation-proved: restoring the dangling call fails it with
   `selection ▸ insert.link changed nothing a user could perceive`.
   A general dangling-identifier check was attempted and **abandoned**: a regex
   lexer over an 17,500-line file reported 22 false positives (string and
   template-literal stripping runs away across lines), and a guard nobody trusts
   is worse than the gap. A real one needs a parser, which needs a dependency.

5. **Ignoring is not a mutation.** In Viewing mode the suggestion rows are disabled with a
   reason, and Ignore once / Ignore all / Add to dictionary stay enabled: they are host-side
   decisions, and a reader looking at a false positive should still be able to dismiss it.

## 11. Grammar — the seam, the first rule set, and what is not covered

Added 2026-09-23 when the owner raised grammar above spelling.

### 11.1 Why hand-authored rules, and not an existing rule set

This is a licensing conclusion, and it is the reason there is no import:

| Candidate | Licence | Verdict |
| --- | --- | --- |
| **LanguageTool** | LGPL-2.1, **including the rule data** (`grammar.xml`, the n-gram sets) | Not usable. The rules are not a separate permissively-licensed asset; taking them into an Apache-2.0 project is exactly the kind of licence contamination the product's whole wedge depends on avoiding. |
| **After the Deadline** | GPL | Not usable. |
| **retext / write-good / nlprule** | MIT / Apache-2.0 | Licence is fine, but all are **npm packages**, and `webapp/package.json` has zero runtime dependencies — adding the first is the owner's call, not this row's. Vendoring their word lists would also be importing somebody's curation rather than writing a rule. |
| **Hand-authored rules here** | Apache-2.0, ours | **Chosen.** |

So `grammar.mjs` cites the convention each rule encodes and copies nothing.

### 11.2 The rules that shipped

Every one is decidable from the literal text — no part-of-speech tagger, no
statistics — which is what keeps the precision high enough to be worth having.

| Rule | Catches | Deliberately does not catch |
| --- | --- | --- |
| `doubled-word` | `is is`, `the the` | `had had`, `that that` — ordinary English |
| `article-agreement` | `a apple`, `an dog` | `an hour`, `a university`, `a one-off`, `an FBI` — the four the letter test gets wrong |
| `subject-verb-agreement` | `he have`, `they was`, `you is` — a closed pronoun/verb table | **Any subject that is not a pronoun.** `the reports was filed` is missed. See §11.3. |
| `modal-of` | `could of`, `would of` | `thought of` |
| `space-before-punctuation` | `Wait , what ?` | Anything non-English — French puts a space there |
| `missing-space` | `one,two` | `3.5`, `4,50`, `fig. 2` |
| `repeated-punctuation` | `!!`, `??` | `...`, `--` |
| `sentence-capitalisation` | `This ends. next one` | `Dr. smith`, `e.g. this`, `J. Smith`, and anything after an ellipsis |

Marked in **blue** (`--paper-grammar`), not red, because Word and Docs both
distinguish them and the reader has to know which question is being asked. The
right-click menu differs too: a grammar mark leads with the RULE'S MESSAGE as a
disabled row, then its correction, then **Ignore this rule** — per-rule rather
than per-word, because "stop telling me about repeated punctuation" is what a
user wants from a rule that is wrong about their prose.

### 11.3 What is NOT covered, and why

- **Subject–verb agreement with a noun subject** — the big one. `the reports was
  filed` needs to know that `reports` is a plural noun and `was` is its verb,
  which needs a part-of-speech tagger. There is no Apache-2.0 tagger we can ship
  without a dependency, and a heuristic ("ends in s") mis-fires on `grass`,
  `analysis`, `news` and every mass noun — a rule that marks correct prose is
  worse than no rule.
- **Tense consistency, passive voice, run-on sentences, comma splices** — all
  need parse structure.
- **`its` / `it's`, `their` / `there` / `they're`, `affect` / `effect`** — these
  need the surrounding part of speech to be decided, and the naive versions are
  notorious false-positive generators. They are the obvious next rules once a
  tagger exists, and they are not guesses worth shipping without one.
- **Any language but English.** The rules encode English convention, and
  `grammarSupports()` gates them; running `space-before-punctuation` on French
  would mark every correct sentence.
- **Style advice** (wordiness, hedging, readability). A different product
  decision, not a grammar rule.

### 11.4 Cost

Grammar is a second pass over text the spelling scan **already read**: same
windowed paragraph set, same per-paragraph cache, same debounce, same repaint.
It adds no engine call and nothing to the keystroke path. The cache key carries
which passes are on, so turning grammar on does not serve stale spelling-only
results.

## 12. The product and industry glossary

`webapp/dict/glossary.txt`, generated by `webapp/tools/build-glossary.mjs`.

**Three tiers, and they stay apart.** This is the part that is easy to get
wrong by merging sets:

| Tier | Where it lives | Whose it is |
| --- | --- | --- |
| Dictionary | `dict/en-US.txt`, `dict/en-GB.txt` | SCOWL's; the language |
| **Glossary** | `dict/glossary.txt` | **the product's** — ships with the app, identical for everyone |
| Personal dictionary | the `words` store in IndexedDB | the user's, in their browser only |

A glossary term must never be reported as something the user added, and clearing
one tier must not unflag the other's words. `isKnownWord` takes them as three
separate sets and `spell_check.test.mjs` asserts the separation both ways.

**Derived, not curated.** The owner's instruction was explicit that a
hand-maintained list rots, so every term is earned from a file already in the
repository:

1. **Our own names** — crate names from every `Cargo.toml`, the webapp package
   name, and the site's own `<title>`s and domain. A rename changes those files,
   and the glossary follows.
2. **The fixture corpus's vocabulary** — the ids, profiles and feature tags in
   `fixtures/manifest.json`.
3. **The vocabulary of our documentation** — a term appearing at least 4 times
   across at least 2 files under `docs/`, plus `README.md` and `AGENTS.md`.

**Code is stripped first.** Fenced blocks, code spans, link targets and bare
URLs are removed before tokenizing. Without that the glossary would accept
`effective_run_properties_in_range` and every misspelling ever quoted inside an
error message — it would become a mechanism for accepting typos rather than
terms.

**Hyphens are excluded**, because `isKnownWord` already accepts a hyphenated
compound whose every part is known: `casual-doc-layout` needs no entry once
`casual`, `doc` and `layout` are covered. Admitting them produced fragments
(`byte-for`, `all-target`) that are words in no language.

467 terms, 3.5 KB, fetched once in parallel with the language dictionary — a
second lazy request, where §2.4 budgeted one. It is language-independent, so it
cannot be a section of either word list. A glossary that fails to load degrades
to "no glossary", which flags our own product names: visible, and better than
pretending.

The glossary is also searched for **suggestions**, so a mistyped product name
suggests the product name (`opendco` → `opendoc`). Without that the feature
stops at "not flagged" and never helps anyone fix a typo in it.

### 12.1 Consequence: a docs change can fail the glossary test

`tests/glossary_artifact.test.mjs` re-runs the generator and compares byte for
byte, because that is the only thing that makes "derived" true rather than
aspirational (SKILL.md §9: a published artifact is generated or it is not
published). Since `docs/` is a source, a documentation change that introduces a
term often enough will fail it until `node tools/build-glossary.mjs` is re-run.
That is a real cost and it is accepted deliberately: the alternative — asserting
only the artifact's shape — lets the list rot, which is the exact thing the owner
asked to avoid.

### 12.2 Known trade-off

The `≥4 occurrences in ≥2 files` rule admits a handful of short abbreviations
our own documentation uses often (`col`, `del`, `min`, `pre`, `src`). They are
in the diff and reviewable, which is the point of committing the artifact; a
stricter length floor would also have dropped `pdf`, `rtf`, `sdk` and `xml`,
which are worth more than the abbreviations cost.
