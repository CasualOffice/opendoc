# 139 — Version History, Edit Management, Restore, and Diff PRD

**Status:** Accepted product requirements. **VH-1 and VH-2 are implemented and reachable.**
The store, capture trigger, naming, pinning, retention and atomic restore are in
`webapp/src/version_history.mjs` on the schema-v3 draft database (ADR-038); the SURFACE — a
right-hand panel, two durable entry points, the day-grouped timeline, read-only preview,
naming, pinning, deletion, the retention disclosure and a confirmed non-destructive restore —
is in `webapp/src/version_panel.mjs` and `webapp/src/version_policy.mjs` (**ADR-040**). What
is **not** built, and may not be quoted as shipped: **version diff** (VH-008, VH-009 — all of
§9), **Make a copy and Download a version** (VH-007), **per-change attribution and Show
editors** (VH-016, all of §10.4), restore as one Undo step (VH-015), and collaboration
(§13). Where the panel offers one of those, it offers it **disabled with that as the reason**;
where it does not, this document is the place that says so rather than the panel implying it.

**Opened:** 2026-09-27. Retention and the §18 questions settled by **ADR-038** (2026-09-28);
question 6 settled by **ADR-040** (2026-09-28), which is also the record of what the interface
decided that the store could not.

### What the interface turned out to be, against §8

Written here because the design is the thing a reader checks first, and three of these differ
from what §8 sketched.

| §8 | As built | Why it differs |
| --- | --- | --- |
| 8.1 "the editor opens a right-side panel without changing the document" | as specified | — |
| 8.1 entry points: File, the last-saved status, the palette, the host API | File ▸ Version history, the **View band's panel toggle**, the **left rail's Versions button**, the palette, **⌘⌥⇧H**, and the host API through the same command id | The last-saved status pill is `display: none` below 620px and is the first thing a narrow window sheds, so it cannot be a durable second surface. A View-band button is beside the editor's other two panel toggles and is present at every width the ribbon is. ⌘⌥⇧H is Google Docs' own chord. The rail entry was left out of the first round on the grounds that File and View were enough; the owner overruled that, and the rail is where this editor's panels live — a panel with no rail entry was the odd one out. All faces run the one `file.versionHistory` command and share one pressed state. |
| 8.1 each row shows author(s) | each row shows time, origin and size; the **author is stored and not displayed** | Version-level actor only, which §17 VH-1 requires ("no per-change attribution claim yet"). The local browser profile has exactly one actor, so a column reading "You" on every row would be noise pretending to be information. It appears the moment a second actor can exist. |
| 8.2 a group can be expanded to show its constituent points | day groups only; **no within-day collapsing**, so nothing needs expanding | Grouping that hides rows behind a count nobody can expand is worse than no grouping. Within-day session grouping arrives with the commit log (§17 VH-4), which is what can tell one session from another. |
| 8.3 Name this version | as specified, through a real dialog, from the row's own **⋮ menu** and from **F2** | — |
| §14 "row menus work by keyboard" | each entry carries a **⋮ menu** holding Restore, Name, Keep, Show changes and Delete, opened by the ⋮, by right-click, by Shift+F10 and by the ContextMenu key | The first round put these five in a bar under the list instead, and recorded a real reason: a `role="option"` may not hold an interactive child. With one or two versions that bar was most of the panel, four of its five buttons were disabled until a row was selected, and all five were detached from the row they acted on. The owner rejected it. The list is now a **`role="grid"`** — the pattern whose cells may be interactive — with a roving tabindex, Up/Down across rows and Left/Right across the entry and its ⋮. The day group is a `rowgroup` and carries no `aria-label`; every row names its own full instant instead, which the guard checks against the row's own `title`. |
| 8.4 preview shows Compare, Make a copy, Download | preview shows **Back to current** and **Restore this version**; Compare and the two copies are **present-and-disabled** or absent, per the status note above | This row claimed the preview bar carried Restore before it did — Restore was in the action bar, and the bar is what the reader saw. It is now really on the bar, which is also Restore's second surface and is contextual, so it is never a resting disabled control. |
| 8.5 restore contract, all eight steps | steps 1-7 as specified; step 8 (Undo treats Restore as one action) **not built** | The pre-restore version IS stored, which is what makes both in-session Undo and after-reload reversal possible later. Today, reversing a restore means restoring the pre-restore version — which is in the timeline, named "Before a restore". |
| 8.7 a visible **Clear version history** control reporting the freed size | as specified, in the panel's footer | — |

**Related:** docs 24, 27, 68, 71, 79, 82, 86, 105 OO-004/OO-007,
107, 112, 125, and 140.

**Target:** required v1 document-safety and collaboration foundation, delivered in
phases. This is not an experimental AI feature.

---

## 1. Product outcome

Give every OpenDoc document an understandable, durable past.

A user must be able to answer:

- what changed, when, and by whom;
- what the document looked like before a mistake;
- how this version differs from the current document or its predecessor;
- which versions are important enough to name and retain;
- how to restore an earlier version without destroying later history;
- how to copy or download an earlier version without changing the current one;
- which edits are ordinary edits, suggestions, Undo actions, restores, imports, or
  automated/agent-authored changes.

The product combines four related but distinct experiences:

1. immediate Undo/Redo for recent user actions;
2. Editing, Suggesting, and Viewing modes for edit governance;
3. durable version history across reloads and crashes;
4. read-only version diff, attribution, copy, download, and restore.

They share the same transaction and identity foundation, but the UI must not present
them as the same thing.

## 2. Problem

OpenDoc currently has:

- exact inverse-based Undo/Redo in the engine and browser session;
- semantic Undo/Redo labels and typing coalescing;
- Editing, Suggesting, and Viewing modes;
- comments, tracked revisions, author attribution, and accept/reject flows;
- source-format autosave drafts and crash recovery in the browser.

It does not have durable version history. The browser's 256-entry Undo stack disappears
with the session. Autosave retains one recoverable draft per tab slot and deliberately
is not a timeline. A user cannot browse earlier states, name a milestone, see a diff,
download an older version, or restore an earlier version while retaining the current
one.

This is a document-safety gap, not merely a convenience gap. Undo is unsuitable after a
reload, after unrelated later edits, or when the user knows the desired point by time or
collaborator rather than by action count.

## 3. Research baseline

Google Docs establishes the central interaction model:

- a right-side version timeline with grouped versions and expandable detail;
- authorship and change highlighting for a selected version;
- named versions and a named-only filter;
- preview, Restore, and Make a copy actions;
- owner-managed deletion of unnamed history;
- a selection-scoped **Show editors** action.

Google states that browsing history requires edit permission, named versions prevent
their merging, and a document may have up to 40 named versions. Those are product
behaviors, not limits OpenDoc adopts automatically:
<https://support.google.com/docs/answer/190843>.

Google's Suggesting mode keeps proposed edits distinct from authoritative content and
provides individual and bulk accept/reject plus a before/after preview:
<https://support.google.com/docs/answer/6033474>.

Microsoft Word exposes historical versions in a separate view and allows Restore:
<https://support.microsoft.com/en-us/office/collab-files/view-previous-versions-of-office-files>.
Word's Compare/Combine workflow leaves the source documents unchanged and represents
differences as revision marks in a new document:
<https://support.microsoft.com/en-us/word/compare-and-merge-two-versions-of-a-document>.

OpenDoc should match the useful interaction concepts while improving three safety
properties:

1. restore never erases the later timeline;
2. a history checkpoint preserves OpenDoc's loss-aware source envelope and binary
   resources, rather than storing text/model JSON alone;
3. version diff is complete about unsupported comparisons instead of silently omitting
   formatting, tables, drawings, or other construct families.

## 4. Goals

### G1 — Durable, understandable history

History survives reload, clean close, crash recovery, and normal browser restarts under
the active host retention policy.

### G2 — Safe restoration

Restoring an earlier version creates a new current version. It never deletes or rewrites
the commits that came after the target.

### G3 — Useful version diff

Users can compare a version with its predecessor or the current head and navigate a
structure-aware list of changes.

### G4 — Clear edit governance

Undo/Redo, authoritative edits, suggestions, comments, version history, and activity
records have explicit boundaries and consistent author attribution.

### G5 — Local-first and embeddable

The browser profile works without a server. Native, headless, embedded, and collaborative
hosts supply storage, identity, permissions, retention, and optional synchronization
through stable interfaces.

### G6 — Fidelity and security

A restored version must retain every construct the corresponding ordinary Save path
would retain. Historical deleted content is protected and purged according to explicit
policy.

## 5. Non-goals for the first release

- Git-style source control concepts in the ordinary editor UI;
- arbitrary branching and user-visible merge graphs;
- a mandatory cloud account or OpenDoc-hosted storage service;
- silently embedding OpenDoc history inside DOCX/ODT files;
- treating OOXML tracked changes as the durable version timeline;
- view-history surveillance or an Activity Dashboard;
- compare/combine of unrelated files in the first version-history slice;
- cryptographic proof of authorship or a tamper-evident compliance archive;
- unlimited retention.

Compare/combine remains a later capability under docs 107 and 140. View activity is a
separate privacy product and must not be inferred from edit history.

## 6. Terminology

| Term | Meaning |
| --- | --- |
| User action | One semantic editing gesture, such as Typing, Paste, Table structure, or Review. |
| Undo entry | Session-facing inverse for one user action. Short-lived unless a host explicitly persists it. |
| Commit | One successful atomic state transition with identity, origin, author, time, and parent. |
| Revision number | Monotonic engine/session sequencing value. Not a user-visible version name. |
| Version | A user-visible point in the commit history that can be previewed, diffed, copied, downloaded, or restored. |
| Named version | A version with a user-supplied label that is pinned against automatic compaction. |
| Checkpoint | A bounded, validated, fidelity-complete artifact from which a document state can be reconstructed. |
| Draft | The single-slot crash-recovery artifact defined by doc 112. It is not version history. |
| Review revision | A tracked insertion/deletion/format change in the document model. It is not an engine revision or historical version. |
| Restore | A new commit whose content is based on an older checkpoint. It never moves history backward or deletes later commits. |
| Version diff | A derived, read-only description of differences between two historical states. |

Product copy should prefer **version** and **change**. Internal APIs must qualify
`EngineRevision`, `ReviewRevision`, and `VersionId` to prevent the existing word
"revision" from carrying three meanings.

## 7. Users, roles, and capabilities

The host maps its identity and authorization system onto these capabilities:

| Capability | Allows |
| --- | --- |
| `history.read` | list metadata, preview permitted versions, and view permitted diffs |
| `history.name` | name or rename a version |
| `history.create` | create an explicit checkpoint/version |
| `history.restore` | restore a historical version as the new head |
| `history.copy` | create a new document/session from a version |
| `history.download` | export a historical version |
| `history.delete` | permanently purge history allowed by retention policy |
| `history.audit` | view detailed actor/origin metadata and selection-scoped editors |

The top-level local browser editor grants its local owner all capabilities. Embedded
hosts grant none implicitly. A commenter may use Suggesting mode without receiving
durable-history access. Read-only access to current content does not automatically grant
access to deleted historical content.

## 8. Primary user journeys

### 8.1 Open version history

Entry points:

- File -> Version history;
- the last-saved/last-edited status in the title area;
- command palette: `file.versionHistory`;
- host API command using the same command identity.

The editor opens a right-side panel without changing the document. The current version
is pinned at the top. Earlier versions are grouped by day, then by meaningful edit
session. Each row shows:

- name, or a generated time-oriented label;
- timestamp in the user's locale, with exact time available;
- author(s), including Local user, Unknown, Automation, or Agent where applicable;
- origin such as Edited, Suggested, Restored, Imported, Saved, or Named;
- a concise change summary when available;
- whether the version is named/pinned;
- unavailable/corrupt/partial state explicitly, never as an empty preview.

Selecting a row opens a read-only historical preview. It does not replace the live
session, selection, Undo stack, or dirty state.

### 8.2 Expand grouped versions

Ordinary commits are grouped to keep the panel usable. A group can be expanded to show
its constituent points. Explicit saves, named versions, restores, imports, and author or
session boundaries always create visible boundaries.

Grouping is presentation. It never deletes commits. Naming a member pins and exposes
that exact point.

### 8.3 Name a version

The user chooses **Name this version**, supplies a bounded non-empty name, and may rename
it later. Naming pins the referenced checkpoint and prevents automatic compaction from
making it unrestorable.

Duplicate names are allowed because identity is `VersionId`, not label text. The UI
distinguishes duplicates by timestamp and author.

### 8.4 Preview a historical version

Preview mode is visibly read-only and shows:

- version name/time/authors;
- **Back to current**;
- **Compare to current** / **Compare to previous**;
- **Restore this version** when authorized;
- **Make a copy** and **Download** when authorized;
- compatibility or missing-resource warnings.

Find, zoom, outline navigation, and page navigation continue to work against the preview.
Editing, comments, review decisions, and host mutations are disabled with a reason.

### 8.5 Restore an earlier version

Restore is the highest-risk workflow and follows this contract:

1. The target version is fully loaded and validated in isolation.
2. The UI shows target identity, timestamp, authors, and a summary of differences from
   current.
3. The user confirms **Restore this version**. The confirmation states that current work
   remains in history.
4. OpenDoc writes or verifies a checkpoint of the current head before changing it.
5. The engine/host atomically creates a restore commit with the target version as
   provenance and makes its reconstructed state current.
6. The old head and every intervening version remain available.
7. The restored state is marked Edited/Unsaved until the host successfully saves or
   synchronizes it.
8. While the session remains open, Undo treats Restore as one action. After reload, the
   pre-restore head remains a restorable version even if immediate Undo history expired.

If validation, storage, permission, quota, or activation fails, the current document,
history head, Undo/Redo stacks, and dirty state remain unchanged. There is no partial
restore.

Restoring a version that contains unsupported or missing external resources is refused
unless the normal open path can reconstruct a safe, explicitly degraded document and the
user accepts the disclosed loss. Silent degradation is forbidden.

### 8.6 Make a copy or download a version

**Make a copy** creates a new document identity and independent history lineage from the
selected checkpoint. It never inherits permissions or sharing unless the host explicitly
offers and applies that policy.

**Download** exports the selected version through the normal format registry and
compatibility report. Exporting a historical DOCX does not include the hidden OpenDoc
timeline.

### 8.7 Delete history

History deletion is owner/policy-only, destructive, and separate from restoring.

- named/pinned versions are not removed by automatic retention;
- deleting unnamed history or all history requires an explicit confirmation describing
  that deleted content may become unrecoverable;
- deletion is audit-visible when an audit provider exists;
- active checkpoints needed by the current head, a restore, or crash recovery cannot be
  deleted;
- local storage offers a visible **Clear version history** control and reports the freed
  size.

## 9. Version diff requirements

### 9.1 Compare modes

The user can choose:

- selected version vs previous visible version;
- selected version vs current;
- two explicitly selected versions;
- current selection -> **Show editors**, when attribution data is available.

### 9.2 Change families

The diff must classify, where supported:

- inserted, deleted, moved, and replaced text;
- character and paragraph formatting;
- paragraph/list structure and style changes;
- table insertion/deletion, row/column/cell and merge changes;
- drawing, image, text-box, shape, and anchor changes;
- section/page setup, headers, footers, notes, fields, bookmarks, and metadata;
- comments, suggestions, and review decisions;
- resource additions/removals and compatibility findings.

Unsupported comparison of a construct is an explicit `not_compared` finding. A text-only
diff may never be labelled as the complete document diff.

### 9.3 Presentation

Diff presentation has two coordinated surfaces:

1. a change list grouped by author and version group;
2. a read-only page preview with ephemeral insertion/deletion/change overlays.

Selecting a change scrolls to and focuses its live or historical anchor. Previous/Next
change is keyboard accessible. Users can filter by author and change family and hide
unchanged content only when the preview can preserve navigation context.

Color is supplemental. Every change carries a textual author and change-kind label.

### 9.4 Diff isolation

**This requirement stands for VERSION HISTORY and is superseded for Review ▸ Compare
(ADR-061, then ADR-062).** It was published, reversed for both routes by one ADR, and
restored for this one; the history is kept here rather than overwritten, because the version
that was superseded turned out to be the one that matched two of the three references.

For **version history's Show changes**, as shipped: version diff is derived data. It does not
add tracked changes, comments, nodes, or marks to either source document. The live document is
not read, not exported and not written; both sides are stored checkpoints, and the result is a
read-only unified diff in the Compare panel. This is what Word and Google Docs do (each
produces a third document and leaves the sources untouched) and what ONLYOFFICE's history does
by construction (`docs/164` §5). It is also what a reader is owed: asking what changed in a
past version must not change the document in front of them.

For **Review ▸ Compare**, ADR-061 governs and this paragraph does not: a comparison against a
document the reader chose is applied to the open document as tracked changes, with a warning
said up front, as one undo step. That is ONLYOFFICE's answer and it is deliberate.

Creating a merged **third** document remains an unbuilt, separate, explicit action — `docs/140`
§11.6, and `docs/164` §9 question 6 records that it is still an open product question rather
than a decision.

## 10. Edit-management requirements

### 10.1 Immediate Undo/Redo

- One completed user gesture is one Undo entry.
- Typing coalesces only across the existing exact continuity/session rules.
- Remote commits, restores, review decisions, structural edits, and mode changes form
  explicit boundaries.
- Undo/Redo labels remain engine-owned semantic labels.
- A failed Undo/Redo consumes nothing and changes nothing.
- Selection/caret restoration is part of the action contract, not inferred afterward.

### 10.2 Editing modes

- Editing commits authoritative changes.
- Suggesting commits review revisions attributed to the actor and requires accept/reject.
- Viewing rejects every mutation route, including SDK, host bridge, keyboard, toolbar,
  paste/drop, and agent tools.
- Version preview is a separate read-only state, not another editing mode.

### 10.3 Change origins

Every commit has a closed origin such as:

`human_edit`, `undo`, `redo`, `suggestion`, `review_decision`, `restore`, `import`,
`automation`, `agent_proposal`, or `system_recovery`.

Origins are not author names. An AI/automation commit records both the initiating user or
principal and the automation/provider identity when policy permits.

### 10.4 Attribution

Attribution is derived from committed transaction scope, not from rendered DOM or string
search. **Show editors** returns all attributable commit groups intersecting the selected
structural range. It says when compaction, import, or missing identity prevents exact
attribution.

## 11. Functional requirements

| ID | Requirement | Priority |
| --- | --- | --- |
| VH-001 | List durable versions without loading checkpoint bytes. | P0 |
| VH-002 | Open a version in an isolated read-only preview. | P0 |
| VH-003 | Restore as a new atomic commit while preserving later history. | P0 |
| VH-004 | Restore failure leaves current document and history unchanged. | P0 |
| VH-005 | Checkpoints retain source-format fidelity, resources, and preservation data. | P0 |
| VH-006 | Named versions are pin-able, renameable, and protected from automatic compaction. | P0 |
| VH-007 | Make a copy and Download operate on a selected historical version. | P1 |
| VH-008 | Compare selected version with previous/current and navigate changes. | P0 |
| VH-009 | Every unsupported diff family is reported explicitly. | P0 |
| VH-010 | Author, time, origin, and change summary are shown where known. | P0 |
| VH-011 | History capabilities are enforced below the UI. | P0 |
| VH-012 | Browser history works offline and never silently uploads content. | P0 |
| VH-013 | Retention is byte-bounded; quota failure is visible and does not corrupt history. | P0 |
| VH-014 | History survives reload; drafts remain a separate crash-recovery concept. | P0 |
| VH-015 | Restore is one Undo action in the active session and remains reversible after reload. | P1 |
| VH-016 | Selection-scoped Show editors reports exact or explicitly incomplete attribution. | P1 |
| VH-017 | Deleting history is explicit, permissioned, and irreversible only after confirmation. | P1 |
| VH-018 | Embedded hosts can replace browser storage with their own provider. | P0 |
| VH-019 | Historical content is excluded from ordinary DOCX/ODT export. | P0 |
| VH-020 | Accessibility and mobile workflows expose the same safe actions. | P0 |

## 12. Retention, privacy, and security

History contains content users deliberately deleted from the current document. It is
therefore at least as sensitive as the document itself.

Required policy:

- host-owned retention window and byte budget with an engine-enforced hard ceiling. **As
  built (ADR-038), that is three bounds and they are not the same kind of rule:** a version
  **count** cap and a **byte** budget are ceilings that always apply and prune the oldest
  eligible version first, while the **age window** is a promise to keep versions for at least
  that long — the newest few survive it whatever their age, because age-only pruning would
  empty the timeline of a document nobody had touched for a week. Defaults: 25 versions,
  7 days, a floor of 3, 120 MB, at most 15 named;
- named versions pinned until explicitly removed or policy-forced with disclosure;
- content bytes never included in telemetry by default;
- local browser history is origin-scoped and local-only;
- no persistence in embedded/demo modes unless the host explicitly grants it;
- purge on document deletion/account policy where the host can identify the lineage;
- credentials and encryption keys remain host-owned;
- checksums detect corruption but are not represented as proof against a malicious host;
- history metadata is untrusted input on reopen and is parsed with bounds;
- actor display names are escaped presentation data, not trusted HTML or authorization.

The product must disclose when private/incognito mode, browser eviction, disabled storage,
quota, or host policy makes durable history unavailable.

## 13. Offline, embedded, and collaboration behavior

### Local browser

History is stored on-device and works with network interception proving no upload. A
clear/disable control is visible. Browser eviction is reported when detected; it is not
described as guaranteed backup.

### Embedded web editor

Persistence defaults off, matching doc 112. A host may grant history capabilities and
provide a storage adapter. The iframe never gains ambient access to documents outside its
session.

### Native/headless

The host supplies a durable store and policy. The core SDK remains usable with an
in-memory/no-history provider.

### Collaboration

The ordered collaboration log enriches the same version identities and attribution. The
server is optional; local commits and checkpoints remain authoritative while offline.
Unmerged branches are shown explicitly rather than collapsed into a false linear history.

## 14. Accessibility, responsive behavior, and localization

- The history panel is a labelled landmark with a real list/tree structure.
- Group expand/collapse, filters, version selection, and row menus work by keyboard.
  **Met.** The list is a `role="grid"` with a roving tabindex: Up/Down walk rows
  (which selects, and so previews), Left/Right walk the entry and its ⋮, Enter on
  the ⋮ opens the menu, and Shift+F10 and the ContextMenu key open it from the
  entry. A listbox could not carry the menu — `role="option"` forbids an
  interactive child — so the structure changed rather than the requirement.
- Every per-version action is reachable from at least two surfaces: Restore from
  the row menu and the preview bar; Name from the row menu and **F2**; Delete
  from the row menu and **Delete**; Keep and Show changes from the ⋮ and from the
  row's right-click / Shift+F10 menu. Clear version history acts on the timeline
  rather than on a row and stays in the panel's footer.
- The panel keeps the keyboard when a preview opens beneath it. Swapping the
  document sets the review mode, which focuses the editing surface; without a
  compensating step a keyboard reader was ejected from the timeline 220 ms after
  each arrow press. Guarded by a test that waits for the preview banner before it
  asks where focus is, so it cannot pass by racing the defect.
- Preview focus moves to a version banner, and **Back to current** restores the prior
  document focus/selection when still valid.
- Change overlays have screen-reader text and are never color-only.
- Restore and destructive delete confirmations name the version and consequence.
- Dates use locale formatting but expose an exact machine-readable timestamp.
- Relative labels such as "3 minutes ago" never replace the exact timestamp.
- Narrow screens use the established viewport-inset drawer/full-screen panel pattern;
  Restore remains reachable without hover.
- Generated action names, origins, diff kinds, and error messages use the localization
  catalogue; user-supplied version names are never translated.

## 15. Performance and reliability requirements

- Opening the panel reads metadata only; checkpoint bytes are lazy.
- Editing-path history bookkeeping is O(1) in document size.
- No checkpoint, compression, diff, or full-document serialization runs on a keystroke.
- Checkpoint/diff work is cancellable and backgrounded where the host permits.
- The current 1.3-million-paragraph stress case from doc 116 cannot trigger eager diff or
  history serialization merely by opening the panel.
- Retention and compaction are incremental and bounded.
- A killed tab during checkpoint or restore leaves either the old head or the complete new
  head, never an unreferenced half-state presented as current.
- Corrupt versions are quarantined and identified; one corrupt record does not hide the
  remaining timeline.

Initial budgets are architecture gates, not invented numbers. The implementation phase must
measure checkpoint creation, metadata list, preview open, diff time, restore time, memory,
and storage on small, 500-paragraph, 8,000-paragraph, media-heavy, and pathological corpus
documents before setting release thresholds.

**Measured so far** (`webapp/tests/e2e/version-history-store.spec.mjs`, annotated on every
run): capturing a version of `sample.docx` — a real producer file — stores a 1,013,783 B
artifact and a 463 B metadata row and costs 2 ms of hash plus one IndexedDB transaction,
because the artifact is the one autosave has already exported rather than a second export.
The 5-second autosave tick itself does **zero** store work and constant work in the number of
stored versions (`webapp/tests/version_history.test.mjs` counts store requests rather than
milliseconds). Preview, diff and restore-into-the-session are not built, so they are not
measured; the 1.3-million-paragraph case from doc 116 is not measured against history yet and
must be before any release threshold is published.

## 16. Success metrics

Release evidence must show:

- 100% atomic restore across injected failures at every persistence/activation step;
- semantic and preservation-fixed-point restore for every supported source format;
- no missing images/resources or retained unknown parts on DOCX/ODT restore corpora;
- exact diff anchors for supported families and explicit findings for every unsupported
  family;
- zero unauthorized history reads/restores/deletes across the capability matrix;
- no network request in the local-only browser profile;
- keyboard and screen-reader completion of open -> preview -> diff -> restore -> return;
- main-thread, memory, storage, and cold-open budgets within the declared device profile;
- successful reversal of a restore both in-session and after reload through the preserved
  pre-restore version.

## 17. Delivery slices

### VH-0 — Decisions and contracts

Accept docs 139/140, settle retention and capability defaults, define identity and
fidelity-complete checkpoint contracts, and reconcile them with proposed ADR-033.

### VH-1 — Local history foundation

Metadata-only timeline, explicit/named checkpoints, isolated preview, copy/download, byte
budget, clear control, and source-format preservation. No per-change attribution claim yet.

### VH-2 — Safe restore

Atomic restore-as-new-head, pre-restore checkpoint, one-step session Undo, after-reload
reversibility, failure injection, and compatibility disclosure.

### VH-3 — Structural version diff

Current/previous comparison, change list and overlays, text/format/paragraph/list/table
families, navigation, filters, and explicit unsupported findings.

### VH-4 — Durable commit attribution

Unify the transaction path from doc 107, persist versioned commits, add authors/origins,
exact range attribution, **Show editors**, and history across reload.

### VH-5 — Collaboration and compare/combine

Remote commit attribution, explicit offline branches, collaboration grouping, and creation
of a separate tracked-comparison document. This does not block safe local version history.

## 18. Open product decisions

Questions 1 through 5 were **settled by ADR-038** and implemented; the answers are recorded
here rather than only in the ADR because this is the document a reader checks first. Question 3
was later **reversed by the owner** and the reversal is recorded in place, with its date and its
reason, rather than by leaving the old answer standing beside code that contradicts it.

1. **Settled.** Defaults are 25 versions / 7 days / a 3-version floor / 120 MB / 15 named,
   configurable in `settings_defaults.mjs` and clamped to engine hard ceilings. A single
   mobile/desktop split is deliberately not introduced: the byte budget is the device-sensitive
   bound and it is already a setting a host can lower.
2. **Settled: yes**, on by default and tied to the autosave switch — a data-safety net nobody
   turns on is not one, and one switch must not promise what the other has stopped doing.
3. **REVERSED by the owner, 2026-09-28. A capture with nothing new in it is suppressed.**
   The original answer was *always a version* — an explicit Save is a point a user recognises,
   and content addressing makes a no-change Save cost one ~300-byte row rather than a second
   copy of the document. That reasoning was about STORAGE, and storage was never the problem:
   the owner's words are *"version should not be logged if nothing has changed"*, and the cost
   is to the TIMELINE. Opening a document laid down an `import` entry identical to the head and
   saving an unmodified one laid a `saved` entry beside it, so a reader got entries they cannot
   tell apart, cannot act on, and which push real versions out of a 25-row budget.
   So a capture whose artifact is byte-identical to the lineage head reports
   `history.unchanged` and writes nothing — **per reason**, because the reasons are not the
   same kind of thing:
   - **suppressed:** `open`, `save`, and autosave's own `quiesce` / `ceiling` / `hidden` /
     `rename`. All implicit: nobody asked for a version. The one open that carries real
     information — the import baseline of a document with no timeline yet — has no head to be
     identical to, so it is never suppressed;
   - **kept:** `name` and `manual`, because they are explicit user acts and a command that
     appears to do nothing is the worse failure; and `pre_restore`, `restore` and `recovery`,
     because they are integrity captures. §9 of `docs/140` makes the pre-restore capture a
     *precondition* of restore — the current state becomes a version before the head moves, or
     the restore is refused — and that invariant is stated over a record existing. Trading it
     for one row, in the rare case of restoring without having edited, is a bad trade.
   The comparison is a SHA-256 checkpoint id against the head's, made inside the transaction
   that already reads those rows, on bytes the capture had already hashed: no extra read, no
   extra hash, nothing walked twice. `VersionCapturePolicy.shouldCapture` — the editing path's
   only contribution — is untouched and still O(1) with no storage access, because a content
   question needs bytes and it may not have them. Recorded in ADR-038 decision 5.
4. **Settled: yes.** Pinning is available without a label; the label makes a version findable,
   the pin makes it durable, and they are separable operations.
5. **Settled: yes**, a pin limit of 15 against the count cap of 25. It exists so that pins can
   never fill the store and leave automatic capture permanently refused.
6. **Settled: it confirms, every time** (ADR-040). Not out of caution — the confirmation is
   the only place the reader is TOLD that their current work becomes a version of its own,
   which is the whole of what makes restore legible as non-destructive. Google Docs does not
   ask and can afford not to: its restore is an undoable edit to a server-side document. Here
   it replaces the document in the tab, so the sentence has to be read before it happens
   rather than after. The card also states that the restored document is unsaved until it is
   written to a file.
7. Which diff families form the first public completeness claim.
8. Whether session Undo survives a clean reload once durable commits exist.
9. How anonymous/local actors are labelled across devices without implying verified identity.
10. Whether history export/import is a separate OpenDoc archive format, and who may use it.

