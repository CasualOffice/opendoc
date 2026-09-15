# 106 — Roadmap: the Apache-2.0 Alternative to ONLYOFFICE for Documents

**Status:** Proposed. **Opened:** 2026-09-15. **Owner decision required** — see §9.
**Supersedes:** nothing. **Consumes:** `105-AUDIT-2026-09-TRACKER.md` (every row cited here
lives there), `99-REMAINING-WORK-AUDIT.md`, `104-HOTFIX-TRACKER.md`.
**Does not change:** `12` §Product Position, ADR-030 / `45` (extensibility invariants).

## 1. The goal, stated once

> OpenDoc is to be the **Apache-2.0 alternative to ONLYOFFICE Docs for documents and
> document collaboration**.

Scope boundaries, so the roadmap cannot drift:

| In scope | Out of scope |
| --- | --- |
| Word-processing documents: DOCX, ODT, TXT, JSON snapshot, and the remaining document interchange formats (§6.5) | **Spreadsheets** — opencalc (`../sheets`) owns these |
| Document **collaboration**: multi-user editing, presence, review, versions, roles | **Presentations** — a future sibling, not this repo |
| Embedding as a library, local-first, no mandatory server | PDF *editing* and PDF *forms* (ONLYOFFICE's Forms tab is PDF-only) |
| Desktop, browser, mobile browser, headless | Native mobile app shells (`18`: the browser build is the mobile story) |

"Alternative" is a specific claim with a testable meaning, and §7 is its definition of done:
**an integrator currently paying for ONLYOFFICE Developer Edition to embed a document
editor can switch to OpenDoc and lose nothing they depend on.** Not "has comparable
features" — feature count is explicitly rejected as a goal by `12` §Product Position.

## 2. Why Apache-2.0 is the whole wedge

This is the part the audit round nearly missed, and it determines the ordering below.

ONLYOFFICE Docs is **AGPL-3.0-only**. Version 9.4 adds supplementary attribution and
copyright terms; non-code assets (icons, illustrations, docs) are separately
**CC-BY-SA-4.0**, so the icon set cannot be reused in a proprietary product either; and no
trademark rights are granted. AGPL §13 means **network use triggers source distribution**:
any SaaS that embeds it must offer its own modified source. Escaping that costs a commercial
licence — Enterprise from **$1,500**, Developer from **$3,500** — and the Developer Edition
is counted in **concurrent browser tabs** ("one document opened by two users = two
connections"). White-labelling is Developer-only.

Crucially, the gating is **in the code, not just in the contract**:
`LayoutManager._applyCustomization` and `_isElementVisible` early-return when `!_licensed`,
so the entire host-customization and layout-hiding API — the thing an integrator embeds it
*for* — is licence-gated. `asc_onLicenseChanged` can degrade a live session. The OSS
`readLicense()` is a stub returning `hasLicense: false`.

Two consequences:

1. **The market is real and specific.** Every integrator who wants to embed a document
   editor in a closed-source product, or ship it in a SaaS without an AGPL obligation, is
   currently choosing between paying ONLYOFFICE, accepting AGPL, or using a rich-text
   framework that cannot do DOCX fidelity (`12` §Rich-Text Frameworks — TinyMCE's DOCX
   path is a separately-licensed conversion service). Apache-2.0 plus real DOCX fidelity
   plus embeddability is an unoccupied position.
2. **Embeddability is not a late-phase nicety; it is the product.** A permissively-licensed
   editor nobody can embed has no wedge at all. This is why §6.4 exists as its own phase
   and why a minimal mount seam is pulled forward into Phase 1.

## 3. The three structural advantages to protect

From the ONLYOFFICE source analysis (`105` §4.1). These are not features to be traded away
while chasing parity rows.

**A1 — Local-first is a capability they cannot match.** Format I/O in ONLYOFFICE is `x2t`, a
native C++ binary, and **`core/X2tConverter/build/` contains only `Android/` and `Qt/` —
there is no WebAssembly build.** Their browser client therefore cannot open or save a
document without a running DocumentServer. The document `key` is a server session
identifier invalidated after save; there is no durable local document model; persistence is
the host's `callbackUrl`. The only way they get local operation is the native core inside a
Chromium shell — the desktop and mobile apps — **which they do not offer as an embeddable
library.** OpenDoc is local by construction.

**A2 — Direct OOXML is a fidelity advantage by construction.** They convert
DOCX → `Editor.bin` → DOCX, so anything the intermediate model does not represent is
dropped rather than preserved. Their own API admits it: `compatibleFeatures`,
`forceWesternFontSize`, a `textConvertEquation` prompt asking users to convert legacy
equations to a supported form, and the whole `docxf`/`oform` family invented because the
model could not carry form metadata. OpenDoc reads and writes OOXML directly and keeps
unknown parts byte-for-byte. **But this advantage is only real if loss is detected and
reported** — which today it largely is not (`105` FID-R-01…FID-R-04). That promotes the
reporting substrate from hygiene to competitive work, and it is why Phase 2 exists.

**A3 — Collaboration is far cheaper than the deferred ADR assumes.** ONLYOFFICE's
co-editing is **not operational transform and not a CRDT** — zero `transform` hits in
`DocsCoServer.js`. It is a server-serialized change log plus pessimistic object-level
locking plus client rollback-and-replay: a global 60-second save lock serializes writers, a
monotonic `puckerIndex` totally orders the log, and each client undoes its local changes,
replays the ordered log, then re-collects and re-locks. Lock types are
`kLockTypeNone/Mine/Other/Other2`. Transport is socket.io.

**This retires a blocker, though not in the direction first proposed.** The open
OT-vs-CRDT decision (`08`, `45`) had been gating collaboration. The finding is that the
competitor's bar is met by an ordered log plus locks, so the bar was never the reason to
wait. **The owner resolved it on 2026-09-15 in favour of OT** — carried by transactions,
with snapshot-plus-replay versioning (`107`, and Q5 in §9). The reasoning stands up: the
op set is already closed *and* invertible, `PositionMap::map` already performs
affinity-correct position transform, and OT makes offline-then-merge, version history and
compare/combine fall out of one mechanism instead of three. Locks were also the worse fit
for A1, because a lock authority must be online. A CRDT adapter stays possible later behind
the same seam (ADR-006) for peer-to-peer or partition-tolerant merge, which the
relay-ordered OT design deliberately does not attempt.

## 4. Where we already stand

Measured, not estimated (`105` for method).

| | OpenDoc | ONLYOFFICE Docs 9.4 (document editor) |
| --- | --- | --- |
| Engine | 174,410 LOC Rust, 14 crates | C++ core + `sdkjs`; format I/O native-only |
| Client | 28,783 LOC (`main.js` = 15,951, **0 exports**) | ~208 KLOC client shell; 76.6k JS in `documenteditor/` |
| Rendering | CPU raster, `parley`/`harfrust`/`skrifa`/`tiny-skia` | Canvas + WASM font engine (FreeType 2.10.4) + WASM PDF engine |
| Editing ops | 47-operation closed set, 449 WASM exports, 97 command ids | 62 API classes / 1,894 methods; 147 named shortcut commands |
| Tests | 1,504 Rust, 455 browser in 114 specs | — |
| Ribbon | 5 tabs | 11 + 2 contextual |
| Formats (edit) | DOCX, ODT (bounded), TXT, JSON | DOCX, DOTX, ODT, OTT, FODT, RTF, TXT, HTML, XML, MD, EPUB, FB2 |
| Licence | **Apache-2.0** | **AGPL-3.0-only** + commercial |
| Server required | **No** | **Yes, structurally** (A1) |

The honest summary: the **engine** is competitive and in places ahead (table sorting and
decimal tab stops ship here and do not exist there; colour glyph rendering; a typed loss
taxonomy; `unsafe_code = "forbid"`). The **product around it** is not — no File backstage,
no References, no PDF, no spell check, no collaboration, no embedding package, one locale.
That asymmetry is what the phases are ordered around: stop building engine depth for a
while and build the product that makes the engine reachable.

**Owner assessment, 2026-09-15: code quality and UI/UX are not production or enterprise
quality as of now.** This is a statement about the current state, not a lowering of the
target — production-grade remains the baseline (`10`, AGENTS.md), and nothing here is to be
described as an MVP or a prototype. The measured gap is tracked as the **CQ** class in `105`
§2A and drives **Track Q** (§6.9), which gates the phases rather than trailing them. The
load-bearing numbers: two god-files (`main.js` 15,951 lines with **0 exports**;
`casual-doc-wasm/src/lib.rs` **26,374 lines in one file** carrying all 449 exports); the live
editing path references the transaction engine **0 times**, so ADR-005 is not honoured in
practice; three shipped guards cannot fail on their own subject; one browser engine and no
accessibility rule engine in CI; 3 of 4 fuzz targets never run; no layout/render/repaint
benchmark; 70 hardcoded ⌘ glyphs and one locale.

## 5. Sequencing principles

1. **Unlock before breadth.** One change (Phase 1's editable focus owner) unlocks mobile,
   IME, dictation, spell check and touch selection. Do it before any of the four.
2. **One dependency gates a whole tab.** A field-evaluation engine gates TOC, captions,
   cross-references and the table of figures — the entire References tab. Build the engine
   once (Phase 3) rather than four features separately.
3. **Protect A1–A3.** No row is closed in a way that requires a mandatory server, an
   intermediate format, or DOM-as-truth.
4. **Reporting before long-tail constructs**, so new gaps report themselves instead of
   accumulating silently (A2).
5. **Nothing ships `Done` without the gate set** and, for guards, a proven-red mutation
   (`105` §1; the standing tests-must-fail rule).
6. **Publish claims only from committed artifacts** (`105` EV rules).

## 6. The phases

Effort is **engine-inclusive** and assumes a small focused team (2–3 engineers). Ranges are
deliberately wide; §8 discusses total scale and what can be cut.

---

### Phase 0 — Evidence and foundations · *in flight*

**Why first:** while a public claim is false, no other number in the project is citeable.

| Work | Rows |
| --- | --- |
| Correct every unsupported public claim; bring the page under test | EV-001…EV-006 (done) |
| Arm the LibreOffice oracle geometry gate (written, inert — one dispatch + bless) | FID-P-01 |
| Acquire a rights-reviewed **Word-produced** corpus (zero exist today) | FID-P-02 |
| Source-vs-output round-trip test (current tests compare the importer to itself) | FID-P-03 |
| "Modeled is not shipped" guard: a model row names its consumer or is marked preservation-only | FID-P-04 |

**Gate:** every published number derives from a committed artifact; the oracle gate is
armed and green; a Word corpus is in `fixtures/` with measured parity.
**Effort:** 3–5 weeks. **Blocker:** FID-P-02 is a licensing/privacy question (§9 Q2).

---

### Phase 1 — Make it an editor, not a harness · **highest leverage**

**Why here:** the fidelity page's own disclosure says "editing is a developer test harness,
not a product". Until that is false, there is nothing to be an alternative *to*.

| Work | Rows |
| --- | --- |
| **Editable focus owner** — offscreen textarea / `plaintext-only` proxy at the caret, consuming `beforeinput`/composition. Unlocks soft keyboard, real IME, dictation, spell check, touch selection. ONLYOFFICE does exactly this (hidden `<textarea>` matching `/area_id/`, with `asc_enableKeyEvents` as the single choke point) — copy the shape | **UX-001**, UX-002 |
| Extract `commands.mjs` from the 15,951-line module; declare all five ribbon tabs; then make parity a **set-equality** test per surface | UX-003 (step 1), UX-004, UX-005 |
| Shortcut table off the registry: bind the ~15 missing standard chords; render per-platform labels (70 hardcoded ⌘ glyphs today) | UX-006, UX-007, UX-009 |
| **File backstage**: New blank document, Open Recent, Save a copy, Info, Print | **UX-011**, OO-002 |
| **Layout tab** — the commands already ship, scattered across View/Tools/Insert | UX-010 |
| Table menu; menu taxonomy; visible Print affordance; one insert-table behaviour | UX-012, UX-013, UX-014, UX-008 |
| Toast/feedback channel that is not `display:none` on phones; boot progress and boot-failure recovery | UX-017, UX-022 |
| Word count dialog with selection scope (engine already exposes the counts — UI only) | OO-015 |
| **Spell check** — ONLYOFFICE proves it is a client-side WASM problem (`spell.wasm`; their server service is retired) | OO-003 |
| Minimal `mount(element, config)` seam (same extraction as UX-003; full SDK is Phase 4) | HF-109 (partial) |

**Gate:** a person creates a document from nothing, writes and formats it, gets red-squiggle
feedback, and saves — on desktop **and on a phone** — without hitting a dead end; every
command is reachable from ≥2 surfaces, enforced by set equality; shortcut labels are correct
on Windows and Linux.
**Effort:** 3–4 months.

---

### Phase 2 — Open real documents correctly

**Why here:** this is A2 made real, and the "modeled but never consumed" class is eight
constructs deep — each looks finished in the model tracker and is invisible to a user.

| Work | Rows |
| --- | --- |
| **Loss-reporting substrate** — DOCX exporter has *no* reporting path; `ModelOutcome` is hardcoded `Omitted`; unknown *attributes* are outside the vocabulary; three parsers regenerate their parts with zero reporting | **FID-R-01…FID-R-04** |
| Opaque-part invalidation on edit; zero-byte media on export; sub-part byte-exactness | FID-R-05, FID-R-06, FID-R-07 |
| **Embedded fonts** (`.odttf`) — modeled, imported, never de-obfuscated or used. ~10 lines plus registry wiring: the cheapest large fidelity win in the repo | **FID-L-01** |
| `evenPage`/`oddPage` parity breaks; footnote number format/restart/position; line numbering; `w:gutter`/`mirrorMargins`; `w:kern` threshold + OpenType features; cell `noWrap`/`fitText`/`hideMark`; `w:kinsoku`; `w:jc="distribute"` | FID-L-03, FID-L-05, FID-L-09, FID-L-16, FID-L-15, FID-L-13, FID-L-17, FID-L-18 |
| **Hyphenation** (no hyphenator exists; default-on in many European templates, and it changes pagination) | **FID-L-02** |
| Watermarks; drop-cap authoring | FID-L-10, OO-006 |
| Shape paths: add a path/Bézier primitive, then table-drive ~180 presets; `custGeom` | FID-L-04 |
| Floating tables; contour wrap; column balancing and true `nextColumn` | FID-L-07, FID-L-12, FID-L-11 |
| Bidi base level (needs an upstream shaper API or pre-reordering); vertical/rotated text | FID-L-06, FID-L-08 |
| Emphasis marks, run borders, text effects | FID-L-14 |

**Gate:** the modeled-not-shipped class is empty or each member is explicitly marked
preservation-only; Word-corpus oracle parity meets its target; no construct is dropped
without a finding; the disposition taxonomy's nine legal combinations are validated.
**Effort:** 4–6 months. Parallelisable with Phase 1 (engine track vs editor track).

---

### Phase 3 — Long documents work · *one dependency, one tab*

**Why here:** References is the largest single IA gap, and **all of it reduces to a field
evaluation engine**. Build that once.

| Work | Rows |
| --- | --- |
| **Field evaluation engine** — a host-provided evaluation context, recalculation, and `Update field` / `Toggle field codes`. ONLYOFFICE ships only **14 field codes**, so the bar is low and reachable | prerequisite |
| Table of contents: generate, update entire / page numbers only, outline-level or style sources, leaders, format-as-links | **OO-001** |
| Captions (custom labels, chapter numbers, separators) and cross-references (7 reference types) | **OO-005** |
| Table of figures | OO-001 |
| Table formulas: 4 functions over 2 directions → cell refs, ranges, bookmarks, number formats (theirs: 18 functions, 7 formats, manual recalc only) | OO-008 |

**Gate:** a 50-page document with headings, numbered figures and tables gets a correct TOC,
table of figures, captions and cross-references, all of which update on demand and survive a
DOCX round trip through Word.
**Effort:** 3–4 months.

---

### Phase 4 — Make it embeddable · **this is the wedge**

**Why here:** §2. A permissively-licensed editor nobody can embed has no market position.
Phase 1 gives a mount seam; this makes it a contract someone can build a product on.

| Work | Rows |
| --- | --- |
| Consolidate the editor-proven commands and errors into a **versioned public SDK boundary** | `99` order 5 |
| A `DocsAPI`-equivalent host contract: config, a **permissions object** (theirs has 17 fields including group-scoped `reviewGroups`/`commentGroups`/`userInfoGroups`), events, and host-answered requests | HF-109 |
| Host storage contract (owner decision 2026-09: storage YES, opencalc shape) | — |
| Packaging: npm package, custom element / framework-agnostic mount, published crates | HF-085 (finish), HF-109 |
| Declarative host-driven UI hiding — copy their `data-layout-name` + one recursive walker (~120 knobs, zero `if (config…)` in feature code) | UX-003 |
| Full **i18n**: locale system, extraction, and the first non-English locales (theirs: 46 locales × 4,479 keys) | UX-009 → HF-081 |
| Accessibility: caret-anchored a11y tree, radiogroup semantics, ARIA correctness, an automated rule engine | UX-020, UX-021, UX-023, UX-024 |
| Mobile: touch handling, pinch zoom, selection handles, a phone layout tier | UX-018, UX-019 |

**Gate:** a third-party integrator embeds OpenDoc in a closed-source app **with no server**,
drives it through the documented config/permissions/events contract, localises it, and
passes an accessibility audit — using only published packages and public docs.
**Effort:** 2–3 months for the SDK; i18n/a11y/mobile add 2–3 more and can run in parallel.
**Blocker:** the **D-6 embed contract is an open owner decision** (§9 Q1).

---

### Phase 5 — Output and interchange

**Why here:** "cannot produce a PDF" disqualifies a document editor from most procurement,
and print today emits a 150-DPI raster with no selectable text.

| Work | Rows |
| --- | --- |
| **PDF export** — `98` designs it end to end: a `casual-doc-pdf` backend transcribing the shared `DisplayList`, selectable text, embedded subset fonts, vector paint, never rasterized. **Blocked on ADR-031** (writer/subsetter build-vs-buy) | `98`, ADR-031 |
| Print dialog: range, duplex with edge flip, colour/mono, margins, preview. Consider their pattern — the engine renders preview into a *second named surface* via the same renderer | **OO-010**, HF-030, HF-036, HF-105 |
| Complete ODT beyond the current bounded subset | `95`/`96`/`97` |
| **RTF, HTML, Markdown, EPUB, FB2** import/export; DOTX/OTT templates | new adapters |
| Tagged PDF / PDF-A (phased, per `98`) | `98` |

**Gate:** every format ONLYOFFICE edits natively, OpenDoc opens and saves, with a
compatibility report; PDF export produces selectable, searchable text with embedded subset
fonts and editor↔PDF layout parity.
**Effort:** 4–6 months. PDF is the bulk; the text interchange formats are individually small.

---

### Phase 6 — Collaboration: OT over transactions, snapshot/replay versioning

**Owner decision, 2026-09-15:** operational transformation, carried by transactions, with
snapshot-plus-replay providing versioning. Designed in full in
**`107-COLLABORATION-OT-SNAPSHOT-REPLAY-DESIGN.md`**; ADR-033 to be written from it. Q5 in
§9 is closed.

**Why it sits after Phase 4:** collaboration is designed *over* a stable
command/transaction/identity boundary, not under it (`99` order 6, ADR-006).

**The prerequisite that dominates this phase.** The OT substrate exists in a crate the live
editor never calls. `casual-doc-transaction` has `RevisionId`, `TransactionId`, `Commit`,
`base_revision` and an affinity-correct `PositionMap::map` over a **5-operation** set;
`casual-doc-wasm` references it **0 times** and applies `casual-doc-edit`'s **47** operations
directly, with a flat capped undo stack and no revision chain. `casual-doc-edit` has no
dependency on `casual-doc-transaction`. Unifying them (`107` §2.1) is therefore step one —
and it is debt that was owed anyway, because it is what ADR-005 already requires.

**What makes 47 operations tractable** (`107` §3): most of the op set addresses *nodes*, not
text offsets, so it needs anchor-validity checking rather than pairwise transform. Three
tiers — **T1 positional** (~9 ops: text, split/join, inline format — full pairwise transform,
the hot path, and `PositionMap` already covers 4 of its step kinds); **T2 node-addressed**
(the bulk — table structure, objects, properties, notes, bookmarks: liveness check plus a
tombstone rule, with every tombstone **reported through the disposition taxonomy**, never
silently dropped); **T3 document-scope** (styles, sections, core properties: serialise,
last-writer-wins). Convergence relies on TP1 proven by property test; TP2 is avoided by
transforming only against a totally ordered relay log.

**The relay is not a document server and is not mandatory.** It orders and fans out
operations. It does not parse documents, convert formats, hold the authoritative model, or
gate single-user editing — the A1 line (§3) that must not be crossed. Offline single-user
editing continues against a local log and reconciles on reconnect.

| Step | Work | Independently valuable? | Rows |
| --- | --- | --- | --- |
| 6.0 | Unify on one operation set; route every WASM mutation through `Transaction` + `Commit`; commit-log undo; emit mapping steps for structural ops. Plus the B1 prerequisite: `HF-111`, which re-validates the whole document per keystroke | **Yes** — closes ADR-005 and a perf defect | CQ-002, HF-111 |
| 6.1 | Durable log, snapshot, compaction; autosave and crash recovery | **Yes** — closes the oldest P1 data-safety row | **HF-011** |
| 6.2 | Version history: list, restore, per-author colouring | **Yes** | **OO-004**, HF-068 |
| 6.3 | T1 transform, tie-break by `(revision, site_id)`, TP1 property tests, the §4-budget benchmarks. **No network yet** | Yes | — |
| 6.4 | T2 anchor rebase and tombstoning with taxonomy reporting; T3 serialisation | Yes | FID-R-02 reuse |
| 6.5 | Compare and combine documents — *the transform applied offline* | **Yes** | **OO-007** |
| 6.6 | Relay adapter, presence, per-user cursors, author identity on the wire | Collaboration ships | OO-018 |
| 6.7 | Roles and permission enforcement against the Phase 4 permissions object | | OO-018 |

Steps 6.0–6.5 deliver four tracker rows and involve **no networking at all**. If
collaboration were cancelled tomorrow, everything through 6.5 would still be the right work.

**"Editing stays light" is a budget, not an aspiration** (owner constraint, 2026-09-15).
`107` §4 states it as seven measurable gates: per-keystroke work O(1) in document size;
transform cost O(concurrent ops since base), never O(log) or O(document); typing coalesced
into one transaction per run; no paragraph-rewrite op on the typing path (`SetInlines` is an
inverse vehicle, not an edit primitive); snapshots periodic, never per-operation; incremental
layout invalidation for remote ops as well as local; and an explicitly bounded log. Each gets
a benchmark — none of which the committed baseline's four cases currently covers.

**Gate:** several users edit one document concurrently with correct convergence (TP1 proven
by property test); a killed tab recovers; history restores a prior version; two divergent
files compare and combine; every tombstoned operation produces a disposition entry; budgets
B1–B7 are baselined — **and a test asserts single-user editing works with the network
disabled**, so A1 is protected by a gate rather than by intent.
**Effort:** 7–10 months, of which 6.0–6.2 (~3 months) is debt repayment owed regardless.
**Layering:** a CRDT adapter remains possible later behind the same seam (ADR-006) for
peer-to-peer or partition-tolerant merge, which relay-ordered OT deliberately does not attempt.

---

### Phase 7 — Enterprise surface

| Work | Rows |
| --- | --- |
| Document protection: `w:documentProtection` model plus 4 restriction levels — Read only / Comments / Filling forms / Tracked changes. These map cleanly onto the existing Editing/Suggesting/Viewing modes, so UI cost is low and enforcement is the work | **OO-011** |
| File password / encryption | OO-011 |
| Digital signatures (invisible + visible signature line) | OO-011 |
| **Content-control authoring** — `w:sdt` already models, round-trips and paints checkbox state; 7 control types plus the settings dialog remain | **OO-012** |

**Gate:** a protected, signed document opens, enforces its restriction level, and round-trips
its protection and signature state.
**Effort:** 2–3 months.

---

### Phase 8 — Breadth · *deliberately last, and partly optional*

| Work | Rows | Note |
| --- | --- | --- |
| **Charts**: render, then author | OO-014, FID-R-08 | A subsystem. Needs a scope decision (§9 Q3) — preserve-and-placeholder is honest and cheap |
| **SmartArt**: render | OO-014 | Their editing is **formatting-only** (no text pane, no promote/demote, no change-layout), so authoring parity is a much lower bar than 159 layouts suggests |
| **Equation editor** | OO-009 | `86` excludes it; `99` §2 requires an authority ADR first so UI-only synthesis cannot silently replace unsupported math |
| AutoCorrect: 4 tabs, math codes, autoformat list triggers | OO-016 | |
| Plugins and macros | OO-017 | Open ABI decision; ADR-030 reserves the seam |
| Mail merge | OO-013 | Theirs is xlsx-only, portal-bound, 100-recipient capped — a weak parity argument |
| Freehand drawing | OO-019 | Minimal even in theirs |
| Home/Insert control breadth; multi-page view; themes | OO-020, OO-021 | |

**Effort:** 6+ months, mostly charts. Much of this can trail the alternative claim.

---

---

### Track Q — engineering quality to enterprise standard · *runs across every phase*

**Owner assessment, 2026-09-15: code quality and UI/UX are not production or enterprise
quality as of now.** Production-grade remains the baseline (`10`, AGENTS.md); nothing here is
an MVP or a prototype. This track closes the gap between the standard and the state.

It is deliberately **not a phase.** A quality phase gets deferred; a quality *gate* does not.
Track Q items attach to the phases whose work would otherwise re-create the defect, and no
phase may be marked `Done` while its attached Q items are open. Rows are the **CQ** class in
`105` §2A, each carrying the measurement that makes it checkable.

| Q item | Attaches to | Why there | Row |
| --- | --- | --- | --- |
| Decompose `main.js` (15,951 lines, **0 exports**) into modules, starting with `commands.mjs` → `strings.mjs` → `shell.mjs` | **Phase 1**, then 4 | Phase 1 cannot deliver command-surface parity or an i18n seam without it, and Phase 4 cannot deliver a mount contract from a module that executes at import and binds 360 fixed DOM ids | CQ-001, CQ-010 |
| Decompose `casual-doc-wasm/src/lib.rs` (**26,374 lines in one file**, all 449 exports) | **Phase 4** | It *is* the public boundary Phase 4 is meant to version and publish. Note `M-001` reported crate roots reduced to ≤64 lines — true for the four crates it covered, not the workspace: four roots exceed 3,000 lines | CQ-001 |
| Route all mutation through transactions (ADR-005 is not honoured in practice today) | **Phase 6.0** | Same work as the OT prerequisite (`107` §2.1) | CQ-002 |
| Make the ribbon declarative (23 of ~90 controls today) and parity a set-equality test | **Phase 1** | The two tabs that are declarative are the two with real parity tests | CQ-004 |
| Prove every guard red by mutation; fix the three that cannot fail | **Phase 1** | Two of the three are the command-parity and IME guards Phase 1 depends on | CQ-003 |
| Localisation seam + first non-English locale (70 hardcoded ⌘ glyphs, 1 locale) | **Phase 1** seam, **Phase 4** locales | Enterprise procurement treats localisation as a hard requirement | CQ-005 |
| Browser matrix (Chromium-only today) + `@axe-core/playwright` + run all four fuzz targets (3 never run) + layout/render/repaint benchmarks (zero today) | **Phase 0** benchmarks, **Phase 4** a11y/browsers | The missing repaint benchmark is exactly how a fabricated "7 ms" reached a public page | CQ-006 |
| Derive counts and publish only from committed artifacts | **Phase 0** | Hand-maintained numbers have twice become false public claims | CQ-007 |
| Complete the quick-xml 0.42 port (**910** measured compile errors; CI saw 3 because compilation stops early) | **Phase 2** | It touches the same fail-closed parsers Phase 2 rewrites for reporting — do it once | CQ-008 |
| Keep design docs the spec of record | continuous | `63`/`64` drift is how the ARIA and radiogroup defects entered | CQ-009 |

**Standing rules this track establishes**

1. **A source-file ceiling** — proposal 2,000 lines, with recorded exceptions. Two files
   currently exceed it by an order of magnitude.
2. **Every guard must have been seen to fail** on the thing it exists to catch, and the
   mutation recorded. Already the house rule; CQ-003 shows it was not applied uniformly.
3. **Every published number is generated from a committed artifact.** No exceptions, on any
   page (`105` EV rules).
4. **A capability is not `Done` until it is reachable from the product.** Two subsystems —
   the transaction engine and eight modeled constructs — were recorded as built while
   unreachable. This is one failure mode at two layers, and it is the single most expensive
   pattern in the project's history.
5. **Enterprise UI/UX floor**, checked per phase: keyboard-operable, screen-reader-operable,
   localised, themed, touch-usable, and able to *say something* when it refuses.

**Gate:** the CQ exit gates in `105` §2A all hold.
**Effort:** ~4–6 months of work, but distributed — it is not a separable block, and
budgeting it as one is how it gets cut.

---

## 7. Definition of done for "alternative to ONLYOFFICE for documents"

The claim is earned when **all** of these hold, each backed by a gate that can fail:

1. Opens, edits and saves every format ONLYOFFICE edits natively, with a compatibility
   report on every operation (Phase 5).
2. Exports real-text PDF and prints with a proper dialog (Phase 5).
3. Generates and updates TOC, captions, cross-references and table of figures (Phase 3).
4. Track changes, comments, compare/combine and version history (Phase 6).
5. Multi-user collaboration with presence and roles — **and works offline single-user**,
   which they cannot (Phases 6 + A1).
6. Document protection, restriction levels and signatures (Phase 7).
7. Spell check (Phase 1).
8. Embeddable by a third party with **no server**, under Apache-2.0, through a documented
   versioned contract, localised, accessible, and usable on a phone (Phases 1 + 4).
9. No silent data loss, with the disposition taxonomy actually enforced (Phase 2).
10. Every public claim derived from a committed artifact (Phase 0).
11. **The Track Q gates hold** (§6.9, `105` §2A): no god-files, mutation through
    transactions, every guard proven failable, a declarative command surface, a localisation
    seam with a shipped non-English locale, a three-engine browser matrix with an
    accessibility rule engine, all fuzz targets running, and layout/render/repaint
    benchmarks baselined. An "enterprise alternative" claim with the current
    15,951-line-zero-export client and 26,374-line single-file WASM boundary would not
    survive a buyer's technical review, however complete the feature list.

Charts, SmartArt authoring, an equation editor, macros and mail merge are **explicitly not**
in this definition. They are Phase 8, and the claim can ship before them provided the
limitations are disclosed as precisely as `webapp/src/fidelity.js` now does.

## 8. Scale, honestly

Serial effort across Phases 0–7 is roughly **24–33 months** for 2–3 engineers; Phase 8 adds
6+ months and can trail. With two parallel tracks — an engine track (Phases 0, 2, 3, 5) and
an editor/product track (Phases 1, 4, 6, 7) — a credible alternative claim is **18–24
months** out, not one or two quarters. Anyone planning against a shorter horizon is planning
against the wrong number.

What could compress it:

- **Cut Phase 8 entirely** from the v1 claim and disclose the gaps. Recommended.
- **Narrow Phase 5's format set** to DOCX + ODT + TXT + PDF + Markdown; defer RTF, HTML,
  EPUB, FB2. Saves ~2 months.
- **Buy rather than build the PDF writer and font subsetter** (ADR-031). Decides ~2 months.
- **Ship collaboration as ordered-log-plus-locks only** and never build OT/CRDT unless
  offline-merge is demanded. Already assumed above; A3 is what makes it defensible.

What must not be compressed: Phase 0 (a false claim poisons everything), Phase 2's reporting
substrate (it is what makes A2 real), and the Phase 1 focus owner (four features sit behind
it).

## 9. Owner decisions this roadmap needs

| # | Decision | Why it blocks | Recommendation |
| --- | --- | --- | --- |
| Q1 | **D-6 embed contract** — still open (memory: owner decisions 2026-09). What exactly does a host mount, configure and receive? | Gates Phase 4, which is the wedge (§2) | Model the surface on their `DocsAPI` config/permissions/events shape — it is proven, and integrators already know it — but local-first: no `callbackUrl` requirement, no server-held document key |
| Q2 | **Word-produced corpus** — acquiring rights-cleared Word output is a licensing and privacy question | Gates Phase 0's exit, and therefore every fidelity claim | Generate with a licensed copy, review for redistribution; keep sensitive owner documents local and publish only measurements |
| Q3 | **Charts and SmartArt scope** — render them, or preserve-and-disclose? | Gates Phase 8's size; also the largest single capability gap | Preserve-and-disclose for the v1 claim; render charts in Phase 8 only if procurement demands it |
| Q4 | **ADR-031** — PDF writer/subsetter build vs buy | Gates Phase 5's biggest item | Decide early; it is on the critical path for a disqualifying gap |
| Q5 | ~~ADR-030 collaboration model~~ — **RESOLVED 2026-09-15 (owner): operational transformation, carried by transactions, with snapshot-plus-replay versioning.** Designed in `107`. This is the opposite of the recommendation in this row's original form, which argued for the cheaper ordered-log model; the owner's reasoning holds — the closed op set is already invertible, `PositionMap` already exists and is affinity-correct, and OT makes offline-then-merge and compare/combine consequences rather than separate features | — | **Closed.** ADR-033 to be written from `107`; Phase 6 rescoped below |
| Q6 | **Mobile commitment** — `18` declares mobile/tablet browsers supported; today the editor cannot accept a character on a phone | Either Phase 1 + Phase 4's mobile slice is funded, or the support matrix is downgraded | Fund it. It is mostly UX-001, which is already the highest-leverage row |
| Q7 | **`.docm` policy** — currently rejected at open, undecided | Minor, but an open policy question in `99` | Strip-and-open with an explicit finding; macros stay unexecuted per policy |

## 10. How this document is maintained

- `105` holds the rows; this document holds the **order** and the **gates**. When a row
  closes, update `105`; when a phase gate is met, update here and `14-EXECUTION-TRACKER.md`.
- No phase becomes `Done` without its gate set green and, for any new guard, a proven-red
  mutation.
- Re-derive §4's counts rather than editing them by hand — the drift that produced `104`'s
  stale summary and two fabricated-evidence incidents came from hand-maintained numbers.
- If a phase's premise changes (as `64`'s "only tabs we can fill" premise expired), say so
  in the phase and record why, rather than silently reordering.
