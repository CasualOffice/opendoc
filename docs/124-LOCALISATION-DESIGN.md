# 124 — Localisation: the seam, the pipeline, and the first eighteen locales

## 0. Why this document exists

The owner's instruction: *"also add in pipline for i18 ... i need atleast 15 languages
covered .. and also nthing shoudl be left.. cover everything"*.

Three requirements, and the third is the hard one. "At least 15 languages" is a list.
"In the pipeline" is a CI job. **"Nothing should be left"** is a property of the codebase
that has to stay true after every future change — which means it cannot be delivered by
translating what exists today. It has to be delivered by a gate that fails the build when
somebody adds an English literal to a user-facing surface.

`109` HF-081 has carried this since the first audit: *"No localization seam — every string
is an English literal inside a 14.9k-line file"*, P1, blocked by HF-085 because there was
no module to extract strings into. HF-085 is closed and `main.js` now has 30+ modules
around it, so the blocker is gone.

## 1. The size of it, measured

Counted on `main` at 2026-09-24, not estimated:

| Surface | Sites |
| --- | ---: |
| `editor.html` — `aria-label` | 256 |
| `editor.html` — `title` | 190 |
| `editor.html` — `placeholder` | 19 |
| `editor.html` — visible text nodes | ~599 |
| `webapp/src/*.mjs` — label/title/status assignments | 82 |
| `webapp/src/main.js` — same | 312 |
| **Total string sites** | **~1,458** |

ONLYOFFICE ships 4,479 keys across 46 locales, so this is a quarter of their surface —
consistent with a product that does documents and collaboration and not sheets or slides.

## 2. What a "seam" has to do, beyond swapping strings

A word processor is a harder localisation target than most applications, because four
things other than labels are locale-dependent and all four are already wrong:

1. **Keyboard chords.** `formatShortcut` already renders `⌘⇧P` against a platform, but the
   words around it ("Press", "then") are literals. A German user reads `Strg`, not `Ctrl`.
2. **Plurals.** `"1 word"` / `"2 words"` is hand-rolled with a ternary in at least six
   places. Russian has three plural forms, Arabic six. A ternary cannot express that;
   `Intl.PluralRules` can, and it is in every browser we support.
3. **Numbers, dates and lists.** The status bar's counts, the draft's "for up to 24 hours",
   `Document properties`' dates. `Intl.NumberFormat` / `DateTimeFormat` / `ListFormat`.
4. **Direction.** Arabic and Hebrew are right-to-left. The chrome must mirror; **the
   document must not** — a `.docx` carries its own `w:bidi` and a left-to-right document
   opened by an Arabic-speaking user is still left-to-right. This is the one place where
   "localise everything" is wrong, and the seam has to make the distinction structural
   rather than leave it to whoever writes the next stylesheet.

## 3. The design

### 3.1 One catalogue format, flat, and English is DERIVED

`webapp/locales/<tag>.json`, flat `{"key": "text"}` — ONLYOFFICE's shape
(`apps/documenteditor/main/locale/en.json`, 4,479 flat dotted keys), because a flat map
diffs cleanly, merges without structure conflicts, and is what every translation tool
consumes.

**`en.json` is generated, never hand-edited**, by `webapp/tools/build-locale.mjs` — the
same rule `webapp/dict/glossary.txt` already lives under, for the same reason: an artifact
somebody can edit by hand is an artifact that drifts from its source. The gate fails on a
stale `en.json` exactly as `glossary_artifact.test.mjs` does.

### 3.2 Keys name the surface, not the sentence

`toolbar.font.bold`, not `Bold`. Text-as-key is tempting and wrong here: two surfaces that
happen to share an English word do not share a translation ("Save" the button and "Save"
the menu row differ in several languages), and changing English copy would silently
orphan every translation.

Command ids already exist and are already stable (`file.export.pdf`), so command labels,
descriptions and disabled reasons key off them: `cmd.file.export.pdf.label`.

### 3.2a What the seam must NOT touch

Three kinds of string look routable and are not, each learned by routing it and
watching what broke:

1. **Transient status the shell owns.** `#status` is written and cleared by the
   editor; a key made the sweep write its markup text back over the cleared value.
   Its text is the pre-boot placeholder and belongs to no catalogue (§4's allowlist).
2. **Anything the shell rewrites after the sweep.** A tooltip restored from a
   boot-time snapshot, an `aria-label` recomputed on every state sync, a status pill
   repainted on save — all were English until the user switched language, which is
   the worst shape a localisation bug can take: invisible to whoever tests by
   switching, visible to everyone who opens the editor in their own language. These
   ARE routed, but the shell must resolve the key at the point of assignment rather
   than rely on the sweep having been there.
3. **Keyboard glyphs inside a routed string.** The catalogue carries `⌘` because the
   markup it was extracted from does, so every relabel re-introduces it and the
   platform sweep has to run again afterwards (`105` UX-009, re-broken by the markup
   pass and re-closed).

### 3.3 Markup declares its own strings

In `editor.html`, `data-i18n="key"` sets text content, `data-i18n-title`,
`data-i18n-label` (for `aria-label`) and `data-i18n-placeholder` set attributes. The
English text stays in the markup as the fallback and as the extraction source, so the file
stays readable and a missing catalogue degrades to English rather than to key names.

### 3.4 The runtime is `Intl`, not a library

No dependency. `t(key, params)` interpolates `{named}` placeholders; `t` consults
`Intl.PluralRules` when a key declares plural forms (`key.one`, `key.other`, …);
`n()`, `d()` and `list()` wrap the three `Intl` formatters against the active locale.

Catalogues are fetched on demand — an English user downloads none of them.

### 3.5 Direction is scoped, deliberately

`<html lang dir>` carries the UI locale. Every document surface — `.page`, the ruler, the
thumbnails — is pinned `dir="ltr"` unless the *document* says otherwise, because a
document's direction belongs to the document. This is stated here so the next stylesheet
does not quietly mirror the page.

## 4. The pipeline

Three jobs, all in the existing `browser-smoke` lane so no new CI job is needed:

1. **Extraction is idempotent.** Run the tool; `en.json` must be unchanged. A new string
   with no key fails here.
2. **No unrouted literal — as a RATCHET.** A scanner walks `editor.html` and
   `webapp/src/**` and counts every user-facing literal that does not go through `t()` or
   a `data-i18n*` attribute. Failing outright on the first one would have meant a single
   PR touching 1,458 sites, which is not a PR anybody can review; failing when the count
   RISES is the same guarantee delivered incrementally, and it is the idiom `main.js`'s
   line ceiling already uses here.

   The count is per file, so one file cannot grow while another shrinks, and a ceiling
   that drifts more than 12 below its measurement fails too — a ratchet nobody lowers is
   a comment. **The measured debt on the day the seam landed: 1,319 sites**, of which
   853 are in `editor.html` and 367 in `main.js`.

   The scanner is deliberately a scanner and not a parser, and it is biased towards
   counting a site it is unsure about: a false positive costs one line in a table, a
   false negative is a string that ships untranslated.

   **The allowlist exists now, and has one entry.** It was promised here and stayed
   empty until something earned a place in it: `Loading engine…`, the pre-boot
   placeholder, which is on screen before any script runs — before a catalogue could
   exist — so English is the only thing it can honestly say. It was routed once, in
   the markup pass, and the sweep then wrote it back over the value the shell had
   cleared: the no-document editor claimed to be loading forever. An entry is a claim
   that routing a string would be WRONG, not that routing it is inconvenient, so
   `no_unrouted_strings.test.mjs` requires an argued reason on each and caps the list
   at five.
3. **Every locale is complete.** Each catalogue has exactly the keys `en.json` has —
   no missing key, no orphan. A partially translated locale is not shipped half-lit; it
   is a build failure.

## 5. The eighteen

Chosen by speaker count, by overlap with ONLYOFFICE's own list (so a migrating
organisation finds its language), and to cover every script class the renderer must
already handle — Latin, Cyrillic, Greek-free CJK, Arabic RTL, Indic:

`ar`, `de`, `es`, `fr`, `hi`, `id`, `it`, `ja`, `ko`, `nl`, `pl`, `pt-BR`, `ru`, `tr`,
`uk`, `vi`, `zh-Hans`, `zh-Hant` — eighteen, plus `en`. Arabic brings RTL; Japanese,
Korean and both Chinese scripts bring CJK line breaking; Hindi brings Devanagari shaping.

**One defect found by doing this, and fixed here.** The skip link was hidden the classic
way, `left: -9999px`. Measured from the right-to-left origin that is 9,999px off the other
edge: the document became 11,279px wide the moment the interface went RTL and opened
scrolled away from its own content, so the Arabic editor painted a blank window. It is
hidden by clipping now, which takes no space in either direction.

**HF-176 interacts with this and is not solved by it.** No CJK, Arabic or Indic face is
bundled, so a `zh-Hans` UI renders in whatever the OS provides and a CJK *document* still
shows tofu on the browser build. Shipping a Chinese UI on a build that cannot draw Chinese
document text would be a worse lie than shipping no Chinese at all, so the locale list and
the font provisioning decision (HF-132, an owner call) are tracked together.

## 6. Translation provenance, stated rather than implied

The non-English catalogues in the first delivery are **machine-produced and not natively
reviewed**. Each carries `"@@reviewed": false` in its header, the language picker does not
hide it, and `109` records it as an open row rather than a closed one. This is a
deliberate choice made visible: eighteen unreviewed languages that a native speaker can
correct one PR at a time is worth more than one reviewed language and seventeen absent,
but calling them finished would be a quality claim nobody has earned.

## 7. Guards

| Guard | What it pins |
| --- | --- |
| `webapp/tests/locale_artifact.test.mjs` | `en.json` is what the extractor produces from the current source |
| `webapp/tests/locale_coverage.test.mjs` | every locale has exactly `en.json`'s keys, and declares its own direction and review state |
| `webapp/tests/i18n.test.mjs` | interpolation, plural selection against `Intl.PluralRules`, fallback to English, and unknown-key behaviour |
| `webapp/tests/no_unrouted_strings.test.mjs` | the per-file count of unrouted literals never rises, and the scanner itself detects what it claims to |
| `webapp/tests/locales.test.mjs` | locale negotiation: `de-AT`→`de`, `zh-TW`→`zh-Hant` by script rather than truncation, fall-through to the next preference |
| `webapp/tests/e2e/localisation.spec.mjs` | switching locale relabels the chrome, sets `lang`/`dir`, mirrors the chrome in `ar`, and leaves the document's own direction alone |
| the same spec | **no routed string shows its English at FIRST PAINT** — the whole class, compared against the catalogues rather than a list, so a new surface regressing this way fails without anyone adding a case |
| the same spec | the sweep does not re-assert text the shell owns (`#status`), asserted on the SETTLED value rather than a transient one |
| `webapp/tests/e2e/shortcut-labels.spec.mjs` | the platform's own chord glyphs survive a relabel — the catalogue carries ⌘ because the markup does |

## 8. Delivery order

1. **Shipped.** The seam, the loader, the negotiation, the extractor, the three gates,
   the English catalogue, all eighteen translated catalogues, the language picker, and
   the first surface routed end to end — the footer's counts, which is where the plural
   and number problems both live. The editor speaks eighteen languages today; what it
   says in them is the 14 keys routed so far, and the ratchet is what makes that number
   climb.
2. The remaining 1,319 sites, routed a surface at a time. Each PR lowers ceilings, adds
   keys to `en.json`, and translates them into eighteen files — the coverage gate refuses
   a locale that lags, so a surface is either translated everywhere or nowhere.
3. Native review, one language at a time, flipping `@@reviewed` as it happens.

The gate in (1) is what stops (2) from being endless, and what stops the debt growing
while it is paid down.
