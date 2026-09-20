# 112 — Autosave, drafts and crash recovery

**Status:** Design + shipped (first increment). **Opened:** 2026-09-20.
**Rows:** `docs/109` row 2 — **HF-011** (P0, data-safety), restated as `docs/105` **OO-004**.
**Unblocked by** owner decision **D-1** (`docs/104`: browser storage — yes, one store, opencalc
shape). **Does not** implement HF-068 (version history) or HF-073 (recent documents); both are
meant to sit on the store this document defines.

## 1. The problem, stated concretely

The open document lives in the wasm heap and nowhere else. `webapp/src/main.js:2977` says so in
the comment above the `beforeunload` guard, and that guard is the entire safety net today: it
catches a deliberate close and nothing else.

- A renderer OOM kill, a wasm trap, an OS restart or a power cut fires no `beforeunload`.
- After the crash there is no trace of the work anywhere on the machine.

`docs/104`'s own P0 definition is "data loss, corruption, unrecoverable state". Losing an
afternoon of editing to a tab crash is the plainest instance of it, which is why HF-011 was
re-graded P1 → P0 on 2026-09-20.

## 2. What this is NOT

- **Not version history** (HF-068). One draft per tab slot, superseded in place. No history, no
  restore points, no diff.
- **Not recent documents** (HF-073).
- **Not a server.** No upload, no sync, no account. Local-first is a structural advantage
  (SKILL.md §1); the draft never leaves the browser's own storage on the user's machine.
- **Not a replacement for Save.** A draft is a safety net for an unclean exit, not the user's
  copy of their work. The `beforeunload` guard (HF-002) stays exactly as it is.

## 3. Measurement, not assumption

Everything below was measured in the e2e Chromium (Chrome 151, Apple Silicon host), against the
engine built from this branch, by exporting through the real `WasmDocument::exportAs` the editor
already calls. Medians of 5–7 runs.

### 3.1 What a snapshot costs

| Document | Words / pages | `normalized-json` | `docx` `preserve_when_safe` |
| --- | --- | --- | --- |
| `sample.docx` (real producer) | 1,652 w / 14 p | **5.4 ms**, 595,206 B | **2.5 ms**, 1,012,199 B |
| prose 500 ¶ | 20,000 w / 30 p | 3.6 ms, 206,683 B | **0.6 ms**, 150,554 B |
| prose 2,000 ¶ | 80,000 w / 118 p | 11.8 ms, 825,629 B | **1.3 ms**, 595,500 B |
| prose 8,000 ¶ | 320,000 w / 471 p | 48.5 ms, 3,301,445 B | **5.2 ms**, 2,375,316 B |

Both formats are linear in document size. The worst realistic case measured — a 471-page,
320,000-word document — costs **5.2 ms** and **2.4 MB** per draft in the format this design
chose. That is one frame, once per quiesce, on a document larger than most people ever edit.

Storage round trip for the same 3.4 MB payload: **IndexedDB write 1.4 ms, read 0.7 ms**
(structured clone of a `Uint8Array`, `readwrite` transaction, measured end to end). Re-opening a
draft costs **24 ms** (14 pages) to **418 ms** (471 pages) — paid once, during recovery, not
during editing.

gzip via `CompressionStream` compresses the 3.4 MB snapshot to 91,217 B in 11.1 ms (~37×).
**Not adopted** — see §4.3.

### 3.2 The finding that chose the format

The task brief proposed the normalized JSON snapshot, "the obvious candidate". It is the wrong
one, and the engine says so itself. Exporting `sample.docx` to `org.casualoffice.normalized-json`
returns a compatibility report with two entries:

```json
{"feature":"binary_resources","occurrences":1,"modelOutcome":"omitted","retentionOutcome":"not_retained"}
{"feature":"source_envelope","occurrences":1,"modelOutcome":"omitted","retentionOutcome":"not_retained"}
```

Driven to a conclusion on `demo.docx`, which carries one embedded PNG:

| Path | `word/media/*` entries in the re-exported DOCX |
| --- | --- |
| `demo.docx` as opened | `word/media/image1.png` |
| direct DOCX export (`preserve_when_safe`) | `word/media/image1.png` |
| DOCX export **taken through a normalized-JSON snapshot** | **none** |

**A normalized-JSON draft loses every picture in the document.** Recovering from it would hand
the user back their text with the images silently gone — precisely the silent data loss AGENTS.md
and SKILL.md §12 forbid, in the row whose whole purpose is not losing data.

So the draft is written in **the document's own source format, through the same export ladder
`exportDocumentAs` uses for Save**. That choice is also faster (6–9× at scale), smaller (~28% at
scale), and reports **zero** compatibility occurrences on `sample.docx`, against 2 for JSON and 5
for a semantic DOCX export.

Verified separately: a normalized-JSON round trip does preserve review markup (120 tracked
revisions and 20 comments in, 120 and 20 out, word count unchanged). The defect is binary
resources and the retained source envelope, not the model.

## 4. The design

### 4.1 Where drafts live

**IndexedDB**, database `opendoc-drafts`, version 1, origin-scoped.

- `localStorage` is out: 5–10 MB per origin against a measured 2.4 MB per draft, strings only
  (a `Uint8Array` would have to be base64'd, +33%), and **synchronous** — writing a megabyte on
  the main thread is exactly the per-keystroke cost `docs/107` §4 forbids.
- The Cache API and OPFS were considered. The Cache API models HTTP responses, not records;
  OPFS's synchronous access handles are worker-only. IndexedDB stores a `Uint8Array` directly by
  structured clone, is transactional, and is the store the sibling `opencalc` already uses —
  D-1 says to port that shape rather than invent one.

Two object stores, keyed by slot id:

| Store | Row | Why |
| --- | --- | --- |
| `meta` | `{slotId, docKey, name, formatId, revision, bytes, words, savedAt, heartbeatAt, engine}` — a few hundred bytes | Boot reads only this. Deciding whether to offer recovery must not read megabytes. |
| `bytes` | the snapshot `Uint8Array` | Read only when the user actually restores. |

The split is the sibling's (`editor.drafts.js:136-150`) and it is what keeps the boot path at
kilobytes: a 3.4 MB draft costs one ~200-byte read to decide against offering it.

**Quota.** Chrome grants a large fraction of free disk per origin, Firefox 10% of disk, Safari
~1 GB; all of them are orders of magnitude above the ceiling here, which is bounded by design:
at most `MAX_DRAFT_SLOTS = 8` slots × one snapshot each. `QuotaExceededError` is still handled
rather than assumed away — the writer deletes the oldest other slot and retries once, and if it
still fails, autosave switches to an **unavailable** state that says so in the status bar
(`Autosave unavailable`, with the reason in its tooltip) and announces it once. Autosave never
fails silently.

### 4.2 What is persisted

The snapshot, in the document's own source format, through the ladder
`exact_if_unchanged` → `preserve_when_safe` → `semantic` (the first two are what
`exportDocumentAs` already tries; the third is the honest last resort). If all three throw,
autosave goes **unavailable** and says so. There is no cross-format fallback: falling back to
normalized JSON would silently drop images, which is the defect this section exists to avoid.

Two source formats are **promoted to DOCX** because they cannot carry the model:

| Source format | Draft format | Why |
| --- | --- | --- |
| `text.plain` | DOCX | Plain text carries no formatting at all; a draft of it would lose every style the user applied. |
| `org.casualoffice.normalized-json` | DOCX | Measured above: drops binary resources. |
| everything else (DOCX, ODT, RTF) | itself | Full-fidelity containers; the draft is byte-for-byte what Save would have produced. |

When a draft is promoted, the recovered document's name takes the new extension
(`downloadNameForFormat`), so the editor never claims a `.txt` and then saves a `.docx`.

`meta.revision` is the engine revision watermark the snapshot was taken at — the same
`currentRevision` that `documentIsDirty()` reads, so the draft's freshness is engine-authoritative
and not inferred from the UI.

### 4.3 What is deliberately not persisted

- **No compression.** Measured: 37× smaller for 11 ms, on a payload whose uncompressed ceiling is
  already bounded and small. It buys nothing at 8 slots, and it puts an asynchronous stream
  between the snapshot and the commit on the one path that must be short (`pagehide`). HF-068
  will store many snapshots and should revisit it with these numbers.
- **No caret, scroll position, undo stack or review-panel state.** Recovery restores the
  *document*, not the session. Undo history is not in the model and no export carries it.
- **No draft in a host iframe.** `webapp/src/home-embed.js` boots `editor.html?demo=1` in an
  iframe on the marketing home page; without this rule every visitor who types in the hero demo
  would leave a draft on the origin, and a later real session would be offered it.
  `?autosave=1` forces autosave on inside a frame for a host that wants it. See open question O-2.

### 4.4 When a draft is written

Per `docs/107` §4 the owner's hard constraint is that **per-keystroke work is O(1) in document
size**. Snapshotting is O(document); therefore no snapshot may ever run on the keystroke path.

What the keystroke path does (`noteDocumentEdited`, the one choke point every applied edit already
passes through) is: store an integer revision, `clearTimeout`, `setTimeout`. Three constant-time
operations, no engine call, no allocation proportional to the document.

The snapshot itself runs from a timer, on one of four triggers:

| Trigger | Timing | Why |
| --- | --- | --- |
| **Quiesce** | 5 s after the last edit | The sibling's cadence. A typist never pays for it; a pause does. |
| **Ceiling** | at most 60 s since the last write, while edits keep arriving | Bounds worst-case loss for continuous typing at one minute. |
| **`visibilitychange` → hidden** | immediately | The only reliable "the tab may be about to die" signal. `beforeunload`/`unload` are not fired on mobile or on a background-tab discard; `pagehide` is also subscribed as a belt. |
| **Mode changes that are not edits** | on rename | The name is part of what Save produces, and `documentIsDirty()` already tracks it independently of the revision. |

A write happens only when `documentIsDirty()` — a draft of an already-saved state would be a
stale copy waiting to be offered back.

**The crash-safety property this buys:** the draft on disk is at most 60 s behind, and after any
5-second pause it is exactly current. Nothing about recovery depends on code running *during* the
crash, which is the whole point — an OOM kill runs no handler at all.

### 4.5 Slots, several documents, and several tabs

A **slot** is one tab's draft. The slot id is created at boot and kept in `sessionStorage`, whose
lifetime is exactly a tab: it survives a reload and a renderer-crash reload (so a tab reclaims its
own slot) and disappears when the tab closes.

A tab must not offer a draft that belongs to a window the user still has open — otherwise opening
a second tab immediately offers to "recover" the document being edited in the first. The first
design answered that from a `heartbeatAt` timestamp refreshed every 10 s, and **the crash spec
proved it wrong before it shipped**: a crashed tab stops writing heartbeats, but the last one it
wrote is *seconds* old, so for the next 30 seconds the lease reported the dead tab as alive and
withheld its draft — exactly when the user was reopening the editor to get the work back. The bar
stayed empty. That is the single most important thing this test found, and it is why the test
crashes a renderer instead of asserting that a row exists.

Presence is therefore **asked, not inferred**. Every editor tab listens on a `BroadcastChannel`
(`opendoc-drafts`) and answers a ping with its own slot id. A tab that has crashed, been killed or
been closed answers nothing. A boot pings once, waits `LIVE_PING_MS = 600 ms`, and treats only the
slots that answered as live — and it pays that wait **only** when some candidate row's heartbeat is
recent enough to be ambiguous, so reopening after a crash with nothing else running waits for
nothing at all.

`heartbeatAt` is still written (every draft write, plus a 10 s meta-only put while a draft exists)
and is still used for two things: choosing which slots are worth pinging, and as the fallback rule
where `BroadcastChannel` does not exist — silence is only proof when there was a way to ask.

So, with several documents in several tabs: each tab keeps its own draft under its own slot, and
a tab only ever offers drafts whose owning tab is gone. Eight slots are kept; a ninth write evicts
the oldest.

### 4.6 What the user sees after a crash

On boot, after the engine is up, the editor reads the `meta` store and prunes anything expired
(§4.7). If any offerable draft remains, a **non-modal bar** appears under the app header —
`role="region"`, `aria-label="Recovered unsaved work"`, announced once through the existing polite
live region:

> **Unsaved work recovered.** “opendoc-demo.docx” — autosaved 2 minutes ago, 16 KB.
> [ Restore ] [ Delete ] [ Dismiss ✕ ]

with one row per recoverable draft when there is more than one.

**It offers; it never applies.** Word's Document Recovery task pane lists the recovered files and
waits; Google Docs keeps the version and tells you it is there. Neither silently replaces what is
on screen, for the obvious reason: the editor cannot know whether the draft or the file the user
just opened is the one they want, and guessing wrong destroys the other. So:

- nothing is restored until **Restore** is pressed;
- if the currently open document has unsaved changes, Restore goes through the existing
  `confirmDiscardIfEdited()` gate first — the same gate File ▸ Open uses;
- a restored document is marked **Edited**, not "Opened": it has never been written to disk, so
  reporting it clean would re-arm the original data-loss bug one level up;
- **Delete** asks for confirmation (it destroys the only copy of that work) and then removes the
  slot;
- **Dismiss** hides the bar for this session and keeps the draft, so dismissing cannot lose
  anything either.

The bar is chrome, not a dialog: it does not trap focus, it does not block the document, and it is
reachable by keyboard in DOM order. The same offer is reachable again from **File ▸ Recover
unsaved work…** and from the command palette (`file.recoverDrafts`), which are disabled with the
reason "No unsaved work to recover" when the store is empty — never present-and-silent (SKILL.md
§10).

While editing, the footer carries a quiet **`Draft saved 14:32`** indicator (status-bar priority 3,
so it is the first thing shed on a narrow window), which reads `Autosave unavailable` with an
explanatory tooltip if the store ever refuses a write.

### 4.7 Lifecycle — when a draft goes away

| Event | Effect |
| --- | --- |
| Successful Save / export | The draft for this slot is deleted (`markDocumentSaved()`); the user has the bytes. |
| Restore | The restored draft's slot is deleted; this tab's own slot takes over autosaving the restored document immediately, because it is unsaved work. |
| Delete (bar, confirmed) | Row removed. |
| A new document opened in the tab | The slot is re-keyed to the new document; the previous draft is dropped **only if it was clean**, otherwise it stays as a separate orphan row for recovery. |
| Age | Pruned at boot and before every write at `DRAFT_TTL_MS = 24 h`, the same age gate the docs-repo reference uses. |
| Slot pressure | Oldest of >8 slots evicted on write. |
| User request | Settings ▸ Autosave ▸ **Delete saved drafts** clears the store; the **Keep a local draft** switch turns autosave off and clears it. |

Retention posture, which D-1 required be written down before this landed: **full document bytes
are held on this device, in this browser profile, for at most 24 hours, are never uploaded, and
can be deleted at any time from Settings ▸ Autosave.** That sentence is also in the settings panel
next to the control.

## 5. Where the code lives

- `webapp/src/drafts.mjs` — the store and all the policy: slot ids, the quiesce/ceiling scheduler,
  the age/lease/offer rules, age formatting, document keys. Pure functions and injectable
  `indexedDB`/clock/timers, so `webapp/tests/drafts.test.mjs` exercises the rules in node with no
  browser. New logic goes in a module rather than into `main.js` (HF-085's direction); `main.js`
  has zero exports and binds ~360 ids at import.
- `webapp/src/main.js` — wiring only: the snapshot callback, the recovery bar, the settings
  section, and the four call sites (`noteDocumentEdited`, `markDocumentSaved`, `openBytes`,
  rename).
- `webapp/editor.html` — the bar, the footer indicator, the settings section.

## 6. Guards

| Guard | Proves |
| --- | --- |
| `tests/e2e/draft-recovery.spec.mjs` — *crash* | Types, waits for the draft, **crashes the renderer over CDP (`Page.crash`)** so no shutdown handler runs, opens a fresh page, and asserts the work comes back through the bar. This is the only test shape that proves recovery rather than persistence. |
| same — *offer, never apply* | After the crash the reloaded document does **not** contain the typed marker until Restore is pressed. |
| same — *restored work is unsaved* | The state pill reads **Edited** after a restore. |
| same — *saving clears the draft* | After a Save the bar does not appear on the next load. |
| same — *off means off* | With autosave suppressed (`?autosave=0`, the switch the iframe rule uses), typing leaves no draft and a crash offers nothing. |
| same — *never a dead control* | With nothing to recover, File ▸ Recover unsaved work… is disabled and its tooltip says why. |
| `tests/drafts.test.mjs` | Scheduler cadence (quiesce, ceiling, no work per keystroke), the 24 h age gate, the presence protocol **including a crashed tab whose heartbeat is two seconds old**, the heartbeat fallback, slot eviction, the promotion table, and that `documentKey` samples rather than scans. |

Every one of them was driven red by mutating the production code before being trusted (SKILL.md
§4); the mutations and their output are in the commit message.

## 7. Open questions

- **O-1 — RTF and ODT draft fidelity.** The promotion table assumes ODT and RTF self-export is
  lossless enough for a draft. That is asserted, not measured; the measurement here covers DOCX and
  JSON. If either reports compatibility findings on a real corpus, it belongs in the promotion
  table with DOCX as its draft format.
- **O-2 — embedded editors.** Autosave is off inside an iframe, per `docs/104` HF-011's
  "do not autosave in a host iframe". D-6 (the embed contract) is still open; when it settles, the
  right answer may be "the host decides", with `?autosave=` as the seam.
- **O-3 — cross-tab handover.** A slot is leased, not shared. Two tabs editing the *same file*
  keep two independent drafts and the recovery bar will offer both. That is safe and slightly
  untidy. Merging them needs the OT work (`docs/107`), not a storage change.
- **O-7 — a heavily throttled background tab.** Presence gives a live tab 600 ms to answer. A tab
  the browser has throttled hard could miss that window and have its draft offered while it is
  still open. The cost is an untidy offer, never data loss — restoring opens a copy, and the live
  tab keeps its own document — but if it shows up in practice the answer is the Web Locks API
  (`navigator.locks`), which releases a lock on crash with no round trip at all. It was not used
  here only because the channel ping needs no new permission story and is trivially testable.
- **O-4 — `exact_if_unchanged` on the draft path.** Drafts are only taken while dirty, so this
  mode should never be the one that succeeds; it is kept in the ladder because
  `exportDocumentAs` has it and divergence between the two ladders would be a bug generator.
- **O-5 — private modes.** Firefox private browsing has historically refused IndexedDB entirely;
  Safari evicts it after 7 days of no use. Both land on the same handled path (autosave
  unavailable, said out loud), but neither is covered by a test because neither browser is in the
  e2e matrix.
- **O-6 — a draft larger than the quota.** With an 8-slot cap and a bounded snapshot this is
  hypothetical, but a single document big enough to exhaust the origin's quota on its own would
  loop through "evict, retry, fail" and end at unavailable. That is the right end state; it is
  untested.

## 8. Adjacent change shipped alongside — the browser tab names the document

`document.title` was never assigned anywhere in `webapp/src`, so every editor tab read the static
`OpenDoc Editor — local document editing in WebAssembly` from `editor.html` and several open
documents were indistinguishable in a tab strip. The title now follows the platform convention:

```
report.docx — OpenDoc            (saved)
• report.docx — OpenDoc          (unsaved changes)
OpenDoc Editor — …               (no document open: the static fallback is kept)
```

The name comes first because that is what a tab strip truncates *to*. The `•` marker is driven by
`documentIsDirty()` — the same engine revision watermark the `beforeunload` guard and
`confirmDiscardIfEdited()` read, so the tab, the close warning and the discard gate can never
disagree. It is refreshed from the same places that already own document identity and dirty state:
open, New blank, rename, save, and a draft restore. This belongs in this document because the
dirty signal it renders is the one autosave also reads.
