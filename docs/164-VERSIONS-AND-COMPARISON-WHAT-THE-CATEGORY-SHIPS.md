# 164 — Versions and comparison: what the category actually ships, and where ours diverges

**Status:** Competitive study plus a measurement of our own behaviour, 2026-10-04. It changes no
behaviour. Its output is the ranked defect list in §8, which is work for other lanes.

**Occasioned by:** an owner judgement that our take on **document versions** and on **Compare** is
wrong — "not only the visual UX, but the definition, the presentation, and whether the feature as
conceived is even useful" — and a request for the competitive study *before* any more
implementation.

**Number:** 164 and not 163. A concurrent lane had a `163-PAGELESS-SURFACE-AND-MODE-SWITCH-COMPETITIVE-STUDY.md`
in the shared working tree, uncommitted, while this was being written. `SKILL` §5a.3 records what
happened the last time two lanes minted one number — two published ADRs were deleted by a merge
resolution while five documents went on citing them — so the number was conceded rather than
contested.

**Evidence base, and the three tiers it is written in.** `SKILL` §9 exists because
`webapp/fidelity.html` carried fabricated competitive claims twice, so every paragraph below is in
one of three states and says which:

| Tier | Meaning |
| --- | --- |
| **[SOURCE]** | Read directly, in this session, from a file path or a URL that is cited. For ONLYOFFICE, from the AGPL-3.0 checkouts at `/Users/sachin/Desktop/melp/reference/{sdkjs,web-apps}` — **behaviour and structure only, no code taken**. For Google and Microsoft, from a first-party page whose URL is given. |
| **[SECONDARY]** | Hosted on the vendor's domain but written by a community moderator or a user — Microsoft Q&A, forum posts. Not product documentation. |
| **[UNSOURCED]** | Recollection. Fenced in the `docs/159` §7.3 shape and never load-bearing. |

ONLYOFFICE checkouts are v9.4.0 — sdkjs `72b0421c0bbf9d01eed9cf14834ae47eb2df1b50`, web-apps
`9c0ca538c3b211052347df09d2a4d6781f023403`, both "Merge branch release/v9.4.0 into master",
2026-05-19 — the same commits `157` and `158` cite.

**Relates to:** `158` (the on-canvas comparison study this extends and corrects in two places),
`139` (version-history PRD), `140` (version/restore/diff architecture), `107` §5 (snapshot-replay
versioning), `153` `review.compare-documents`, ADR-061, `109` OO-022 and OO-023.

---

> ## What this concludes, before the evidence
>
> 1. **A version is an automatic, unnamed, mergeable snapshot — naming is a rare opt-out, not the
>    definition.** Google allows **40** named versions per document and documents naming as the way
>    to stop versions being *merged*; Microsoft ships **no version naming at all**; ONLYOFFICE's
>    editor does not create versions in the first place. `107` §5.3 defines a version as "a named,
>    pinned point", which inverts the category. §1, §7.
> 2. **The documented version cadence is coarse, and ours already matches it.** Microsoft: AutoSave
>    writes "every few seconds" but "new versions are only added to the version history periodically
>    (about every 10 minutes)". Our `versionIntervalMinutes` default is **10**. The owner's
>    "versions with nothing in them" complaint is therefore **not** about cadence. §1, §6.
> 3. **Both web competitors answer "what changed in this version" by painting that version's own
>    edits, in place, in the author's colour. We paint nothing.** Google: "The color next to each
>    person's name. The edits they made appear in that color." ONLYOFFICE: closes the file, reopens
>    the historical URL read-only, and replays that version's changes as author-coloured
>    `CollaborativeMarks`. Our preview renders the checkpoint plain. §2, §6.
> 4. **Our "Show changes" on a version row is structurally guaranteed to report "No differences"
>    on the normal gesture.** Clicking a row opens a preview, the preview *replaces the live
>    document*, and the comparison then runs that version against itself. This is proved from code
>    in §6.2 and is the strongest candidate for what the owner saw twice.
> 5. **Compare produces a third document in two of three references, and ours mutates the open one.**
>    Word: "displays what changed between them in a third new document", sources untouched. Google:
>    "the differences between the docs as 'Suggested Edits' in a new document". Only ONLYOFFICE
>    mutates in place — and even ONLYOFFICE never routes version history through it. §3, §5.
> 6. **`139` §9.4 and `140` §11.4/§11.6 still publish the opposite of what we shipped.** "Version
>    diff … does not add tracked changes, comments, nodes, or marks to either source document" is
>    the published requirement; ADR-061 decided the reverse and the code does the reverse. §7.
> 7. **No competitor lets you compare two versions from the history panel.** Google's per-entry menu
>    is enumerated and holds no compare; ONLYOFFICE's history UI contains the string "compar"
>    **zero times**; Word anchors one side to the current document. The feature the owner's
>    complaint implies is uncontested — and therefore has to be *decided*, not assumed. §5.
> 8. **Compare is the weaker of the two features in the market, measurably.** It has no Google
>    help-centre article at all and no update since its 2019 launch post; it is **absent from Word
>    for the web**; it is absent from ONLYOFFICE mobile; and nine of ONLYOFFICE's ten comparison
>    options are unreachable from any UI. Version history, by contrast, is in continuous
>    development at all three. §6.

---

## 1. Version history — the model

### 1.1 Google Docs [SOURCE]

<https://support.google.com/docs/answer/190843> (fetched and transcribed twice in this session,
once by a research lane and once independently):

- **Named-version limit:** "You can add up to 40 named versions per document."
- **Why naming exists:** "You can create a named version to track your version history and make
  sure your versions aren't merged." Naming is a **pin against compaction**, not a label.
- **Compaction is admitted and unspecified:** "The revisions for your file may occasionally be
  merged." That is the whole of Google's statement on it.
- **Permission:** "To browse earlier versions of a file, you need permission to edit that file."
  A commenter or viewer sees **no** history at all, not a reduced one.
- **Owner-only deletion:** "As the file owner, you can manage your document's version history to
  remove historical content", with "Delete history" and "Delete this and older versions", and
  "You can remove all versions of your document that have not been explicitly named" — so the
  delete path *also* spares named versions.
- **Grouping exists but its key is not documented:** the article's only statement is the step
  "**Find grouped versions:** In the right panel, click Expand."

**What triggers an entry is not documented.** The nearest first-party statement is scoped to
encrypted files: "Each time the document is autosaved, comments, emoji reactions, and action items
are saved. They're stored as part of versions in encrypted documents and displayed in version
history" (<https://support.google.com/docs/answer/10519035>). Treat it as suggestive, not as the
general rule.

**Retention: Google documents none for native Docs.** The widely-quoted "30 days or 100 versions"
is explicitly *not* this: Drive's own page carries it — "A version might be permanently deleted
after 30 days or if there are 100 newer versions" — immediately beside the fence "Version history
for Google Docs, Sheets, and Slides is different than history for .pdf files, images, and other
files stored in Drive" (<https://support.google.com/drive/answer/2409045>). The Drive API's
revisions guide states that revisions for Google-native files cannot be deleted
(<https://developers.google.com/workspace/drive/api/guides/manage-revisions>). So Google's model is
**merge, never expire**.

### 1.2 Microsoft Word [SOURCE]

Word has no version store. "Version history in Microsoft 365 only works for files stored in
OneDrive or SharePoint in Microsoft 365"
(<https://support.microsoft.com/en-us/office/view-previous-versions-of-office-files-5c1e076f-a9c9-41b8-8ace-f77b9642e2c2>),
and "The Version History feature is an integral part of Microsoft's 365 built-in data protection in
SharePoint and OneDrive" (<https://learn.microsoft.com/en-us/sharepoint/version-overview>).

**Cadence, verbatim and independently re-fetched** (<https://support.microsoft.com/en-us/office/collab-files/what-is-autosave>):

> "AutoSave … saves your file automatically, every few seconds, as you work."
>
> "After that even though AutoSave is regularly saving your changes to the file, new versions are
> only added to the version history periodically (about every 10 minutes) for the remainder of your
> editing session."

**What creates a version** (<https://support.microsoft.com/en-us/office/how-versioning-works-in-lists-and-libraries-0f6cd105-974f-44a4-aadb-43ac5bdfd247>):
on create or upload; when properties change; "Periodically, when editing and saving Office
documents. Not all edits and saves create new versions."; and "During co-authoring of a document,
when a different user begins working on the document or when a user clicks save to upload changes."

**Retention is administrative, and the defaults are documented.** Org default is "Manual version
history limits with 500 major version count limit and no expiration"
(<https://learn.microsoft.com/en-us/sharepoint/set-default-org-version-limits>), applying to new
SharePoint libraries **and new OneDrive accounts**. Manual mode accepts "a value between 100 and
50,000". The recommended **Automatic** mode thins by age
(<https://learn.microsoft.com/en-us/sharepoint/plan-version-storage>): all versions in the first
30 days within the 500 cap, hourly from 30–60 days, daily from 60–180, weekly beyond 180
"available indefinitely until the maximum 500 count limit has reached". Its stated design
principle is worth borrowing or rejecting deliberately:

> "The algorithm behind automatic version history limits is based on the design principle that
> restores value of a version degrades as the version ages."

**Naming: not possible.** No feature, no UI, no API in any Microsoft documentation. The documented
nearest thing is a check-in comment, which requires enforced check-out: "They are prompted to
provide comments about the changes that they made, which helps to create a more meaningful version
history." A Microsoft Q&A thread titled "Can I name or label an entry in the Version history of a
Word Doc?" is answered with a major/minor-publishing workaround [SECONDARY]
(<https://learn.microsoft.com/en-us/answers/questions/5362343/>), which is the strongest available
evidence that the answer is no.

**Restore is copy-forward**, independently re-fetched
(<https://support.microsoft.com/en-us/office/restore-a-previous-version-of-an-item-or-file-in-sharepoint-f66dbda0-81f4-4d1e-b08c-793265c58934>):

> "SharePoint doesn't remove the earlier version you just restored. It creates a copy and makes it
> the latest version."

**Permissions are generous:** view version history requires "Full Control, Contribute, Read"
(<https://support.microsoft.com/en-us/office/what-permissions-do-i-need-for-sharepoint-versioning-95bce34c-db77-4fd4-8449-9ad7ce0363c0>).
Read access to the document implies read access to its whole past — the exact opposite of Google's
edit-only gate. This is a product decision to take deliberately, not to inherit.

### 1.3 ONLYOFFICE [SOURCE] — the editor does not create versions at all

This is the load-bearing finding of the ONLYOFFICE half, and it was read twice independently.

`web-apps/apps/api/documents/api.js:407-409`:

```
canUseHistory     = _config.events && !!_config.events.onRequestHistory;
canHistoryClose   = _config.events && !!_config.events.onRequestHistoryClose;
canHistoryRestore = _config.events && !!_config.events.onRequestRestore;
```

and the contract at `:329-332`: `'onRequestHistory': <request version history>,// must call
refreshHistory method`. The editor's entire contribution is four `postMessage` calls
(`web-apps/apps/common/Gateway.js:233`, `:237`, `:244`, `:263`) and a tree widget. **If the
integrator does not implement `onRequestHistory`, version history does not exist.** It is not a
licence gate; nothing client-side writes a version — a sweep for `createVersion`, `markAsVersion`,
`newVersion`, `saveVersion` across both trees found only *consumers* of the host's payload.

**Their version/change distinction is the host's, too.** `onRefreshHistory`
(`web-apps/apps/documenteditor/main/app/controller/Main.js:704-863`) reads `opts.data.history[]`
and uses `version.versionGroup` as the grouping key, defaulting it to `version.version` when the
host omits it (`:742-743`). The row that opens a new group gets `markedAsVersion:
(group!==version.versionGroup)` (`:770`) and is the only one labelled `ver.N`. Each entry's
`version.changes[]` becomes level-1 children with their own author and timestamp (`:799-839`),
and the parent row *is* its own last change (`changeid: changes.length-1`, `:796`). So:

- a **version** = a `versionGroup` boundary the host declared;
- a **change** = one saved state inside it, individually selectable.

Two limits worth recording. Sub-rows exist only when `version.serverVersion ==
this.appOptions.buildVersion` (`:795`) — history written by another editor build **silently**
degrades to version-level rows. And the newest version is never restorable (`canRestore: … &&
(ver < versions.length-1)`, `:772`).

### 1.4 What we do [MEASURED]

`webapp/src/version_history.mjs` plus `webapp/src/version_panel.mjs`, read in this session.

| | Ours | Nearest reference |
| --- | --- | --- |
| Who creates a version | the editor, locally, into IndexedDB (`openHistoryStore`, schema v3 of `opendoc-drafts`) | nobody — Google's and Microsoft's are the storage platform's, ONLYOFFICE's is the integrator's |
| Automatic trigger | `VersionCapturePolicy.shouldCapture`: revision watermark moved **and** `versionIntervalMinutes` elapsed, default **10** | Word's documented "about every 10 minutes" — **a match** |
| Explicit triggers | `ALWAYS_CAPTURE`: open, save, name, manual, pre-restore, restore, recovery (`version_history.mjs:400-408`) | Word: create/upload, property change, co-author boundary |
| Naming | yes, `versionNamedLimit` default **15**, hard ceiling `MAX_PINNED` 100 | Google 40; Word none |
| Named versions pinned against pruning | yes (`planRetention` never prunes a pinned row) | Google — "make sure your versions aren't merged" — **a match** |
| Retention default | **25** versions, **7** days, floor of 3, 120 MB | Google: none documented, merge instead. SharePoint: 500 and no expiration by default |
| Restore | append-only: pre-restore capture, then a new `restore` head (`docs/140` §9) | Word: "creates a copy and makes it the latest version" — **a match** |
| Permission to read history | the local browser profile; gated on autosave being on and on the host's `hostAllows` | Google: edit only. Word: Read is enough |

Three of those are already right and should not be re-litigated: the **10-minute cadence**,
**append-only restore**, and **naming as a pin**. Two are divergences that need a decision rather
than a defect: the **7-day/25-version default**, which cannot answer `139` §1's own promise ("what
the document looked like before a mistake") a month later, and the **autosave gate**, which no
competitor has — Google's history is not conditional on a preference the user can switch off.

---

## 2. Version history — the presentation

### 2.1 Google [SOURCE]

The clearest first-party description is the Workspace Learning Centre
(<https://support.google.com/a/users/answer/9296687>), independently re-fetched and transcribed:

> 3. "Click a timestamp to see a previous version of the file. Below the timestamp, you can review:
>    - The names of people who edited the document.
>    - The color next to each person's name. **The edits they made appear in that color.**"
> 4. "(Optional) To revert to this version, click **Restore this version**."

Four facts follow, and all four are sourced: a **right-hand list keyed by timestamp**; **the
authors of that version listed under it**; **a colour per author**; and **that version's edits
rendered in the author's colour in the document itself**, in place — there is no second pane and no
side-by-side mode in any official description. Restore is a **button**, and the per-entry **More**
menu holds exactly *Make a copy*, *Name this version*, *Delete this and older versions*.

> **[UNSOURCED] — nothing in this paragraph was read from a source.** I recall a "Show changes"
> checkbox at the foot of the Google Docs version-history panel that toggles the author-coloured
> highlighting on and off. It is **not** on `answer/190843` (fully transcribed — that page
> documents Sheets' "Show unmodified rows" and no "Show changes"), nor on `answer/9296687`, nor in
> the 2017 named-versions post, and a domain-restricted search of `support.google.com` and
> `workspaceupdates.googleblog.com` returned nothing. `version_panel.mjs`'s design table cites it
> as Google's behaviour; that citation is currently unsourced. **Experiment that settles it:** open
> any Doc with ≥2 revisions, File ▸ Version history ▸ See version history, and photograph the foot
> of the right panel, recording the control's exact label and default state. Thirty seconds.

Whether **restore is itself recorded as a new entry** is likewise not documented for Docs. The only
adjacent statement is Sheets-specific and is about per-cell history, not the timeline, so it must
not be reported as "restore is destructive in Docs". Experiment: note the top entry, restore one
five entries down, reopen the panel, and count entries before and after via the Drive API.

### 2.2 ONLYOFFICE [SOURCE]

`sdkjs/common/apiBase.js:2988-3032`, `asc_showRevision`: on selecting an entry it calls
`asc_coAuthoringDisconnect()`, then `asc_CloseFile()`, then clones the doc info with
`put_Url(this.VersionHistory.url)` and `put_Mode('view')` and calls
`reopenFileWithReconnection(newDocInfo)`. So: **the file is closed and the historical one reopened
in the same canvas, forced read-only.** `disableEditing(true)` and the hiding of the users and
share buttons are at `documenteditor/main/app/controller/Main.js:725-730`.

Selecting a *later* change inside the same version takes the cheap branch and replays only the
delta: `Clear_CollaborativeMarks()`, `VersionHistory.applyChanges(oApi)`, `Apply_Changes()`.
`sdkjs/common/versionHistory.js:94-108` shows the paint: each replayed change is handed a
`CDocumentColor` derived from `this.colors[i]`, falling back to a hard-coded pale green
`(191, 255, 199)`; `sdkjs/word/Editor/RunChanges.js:374-379` turns a coloured change into
`oRun.CollaborativeMarks.Add(Pos, Pos + 1, Color)`, and `sdkjs/word/Editor/Run.js:6587`
`Draw_HighLights` paints those marks as a **background fill**. Author colour is
`Common.UI.ExternalUsers.getColor(version.user.id || version.user.name …)` — a hash of a
host-supplied id, not a palette the editor owns. Avatars and initials are on every row
(`Main.js:758-760`).

Two corrections to `158` belong here, because `158` is about *compare* and these are about
*history*, and the mechanisms are different:

- **History highlighting is a background fill, not the underline/strikethrough review markup.**
  `158` §3.1 correctly describes `ParagraphLineDrawState.addLines` for a *comparison*; version
  history does not go through it. There is no insert/delete distinction in history: insertions are
  tinted and deletions are simply **invisible**.
- Because deletions are invisible they bolted on a separate opt-in, `chHighlightDeleted`
  (`web-apps/apps/common/main/lib/view/History.js:148-156`, Document Editor only), which calls
  `asc_showDeletedTextInVersionHistory` (`sdkjs/word/api.js:14682-14702`) →
  `sdkjs/common/collaboration/deleted-text-recovery.js`. It **replays the operation log backwards**
  and re-marks the recovered runs as tracked deletions attributed to the version's author
  (`SetReviewInfo`, `:360-402`). It is derived from their op log, never from a content diff.

So ONLYOFFICE's history has **no diff**. That is a real and uncontested gap.

### 2.3 Word [SOURCE and SECONDARY]

Documented: "Select a version to open it in a separate window", and "If you want to restore a
previous version you've opened, select **Restore**." Grouping by day, avatars, and inline
per-version highlighting are **[NOT SOURCED]** — Microsoft documents only that the list shows when
and who. Inline change highlighting is documented for *Excel*, not Word.

Word desktop's documented answer to "what changed in this version" is therefore: open the version,
then **Compare** it — i.e. generate a whole redline document. The generic statement is sourced:
"During a review of the version history within a Microsoft Office document, such as a Word or Excel
file, versions can be compared to determine what the differences are"
(<https://learn.microsoft.com/en-us/sharepoint/version-overview>).

> [SECONDARY] A Word-for-the-web "Show Changes" button inside version history is reported by users
> and screenshotted by a Microsoft community moderator
> (<https://learn.microsoft.com/en-us/answers/questions/5299929/>) and appears in **no** product
> documentation. If real, it means Word's *web* client shows per-version inline changes while its
> desktop client does not. Do not publish it as documented behaviour.

### 2.4 What we do [MEASURED]

`version_panel.mjs`'s own design table (`:21-33`) is an honest Docs-first design, and most of it is
built: a right-hand panel rather than a full-window takeover, day grouping (`groupVersions`),
read-only preview on selection, a `role="grid"` list with a per-row ⋮ menu, F2 to name, Restore
behind one confirmation, Make a copy, Download, Clear history, and a retention disclosure in the
footer. Six entry points share one command id.

Two presentation facts are the gap:

1. **The preview shows the old version plain.** `showVersionPreview` (`webapp/src/main.js:15753`)
   swaps `doc`, clears cached geometry, sets `setReviewMode("viewing")` and re-renders. Nothing
   computes or paints that version's own changes. Both web competitors do (§2.1, §2.2). **This is
   the single largest presentation divergence and it is the question a reader opens the panel to
   ask.**
2. **No author on a row.** Deliberate and recorded (`139`'s table: one local actor, so a column
   reading "You" would be "noise pretending to be information"). Defensible today; it is also the
   column Google and ONLYOFFICE both lead with, so it is a collaboration prerequisite rather than a
   cosmetic.

---

## 3. Compare — the model

### 3.1 Word [SOURCE] — a third document, sources untouched

<https://support.microsoft.com/en-us/office/compare-document-differences-using-the-legal-blackline-option-dbfc7351-4022-43a2-a0c4-54d1898702a0>:

> "The legal blackline option compares two documents and displays what changed between them in a
> third new document."
>
> "the source documents that are being compared are not changed"

The API confirms the output's nature: `Application.CompareDocuments` "Compares two documents and
returns a **Document** object that represents the document that contains the differences between
the two documents, **marked using tracked changes**"
(<https://learn.microsoft.com/en-us/office/vba/api/word.application.comparedocuments>).

**Pre-existing tracked changes do not block it** — and the sentence "Word cannot compare documents
when tracked changes are present" is *not* Microsoft's and would be a fabrication if published.
What Microsoft says is:

> "If either version of the document has tracked changes, Microsoft Word displays a message box
> about this. You can click **Yes** to accept the changes and compare the documents."
>
> "Microsoft Word displays a new third document in which tracked changes in the original document
> are accepted, and changes in the revised document are shown as tracked changes."

**Compare vs Combine, officially:** Compare is two versions; Combine is many reviewers and is
repeatable — "You can combine comments and revisions from two documents into one document, and
repeat the process to combine multiple versions", and the Compare page routes the user away: "If
you want to compare changes from a number of reviewers, do not select this option."

**Compare is absent from Word for the web**, independently re-fetched: the browser-vs-desktop
feature table marks "Compare and Merge revisions" **desktop only**, while Track Changes and Version
history are ticked for both
(<https://support.microsoft.com/en-us/office/differences-between-using-a-document-in-the-browser-and-in-word-3e863ce3-e82c-4211-8f97-5b33c36c55f8>).
The Word-for-the-web service description, a long per-feature inventory, contains no Compare section
at all.

### 3.2 Google [SOURCE] — a third document of suggestions

The **only** first-party source for the entire feature is the 2019 launch post
(<https://workspaceupdates.googleblog.com/2019/06/compare-docs.html>), independently re-fetched:

> "This feature will show you the differences between the docs as 'Suggested Edits' in a new
> document."
>
> "enter the name of the user who will be labelled as the author of the suggested edits in the
> comparison output file"
>
> "Doc owners and those with edit access can use this feature to compare documents."
>
> "A new document will be generated that shows all existing suggested edits from both docs as
> accepted."

"Available to all G Suite Editions", "ON by default", Rapid Release 11 June 2019, Scheduled Release
25 June 2019.

The "Attribute differences to" field is the design tell, and it is a *consequence*: Google carries
the diff on the **suggestions** primitive, every suggestion must have an author, so the UI is
forced to ask the user to invent one. There is no author-less redline representation in Docs.

**There is no Google help-centre article for Compare documents.** A research lane searched
`support.google.com` by phrase, tried the help-centre search endpoint, and confirmed the Tool-finder
article (`answer/13466905`) lists neither Compare documents nor Version history. No Workspace
Updates post has touched the feature since 2019.

> **A fabrication to refuse by name.** Several third-party pages — and, twice in one research
> session, a search engine's own AI summary — assert that Google Docs offers "**Compare with
> current**" on a version's three-dot menu. **No Google source contains that string**, and it
> contradicts the enumerated More-menu contents on `answer/190843` (§2.1). This is exactly the
> shape of claim `SKILL` §9 exists for. It must not enter our docs, our site, or a PR body.

### 3.3 ONLYOFFICE [SOURCE] — the open document, mutated

`158` §2 established this and it re-verified. `sdkjs/word/Editor/Comparison.js:3864`,
`CompareBinary`: `oDoc1` is `Asc.editor.WordControl.m_oLogicDocument` — the document on screen; the
other side is read into a throwaway `CDocument` via `BinaryFileReader` with
`{disableRevisions: true, …}`; `CDocumentComparison.compare()` (`:2642`) then mutates `oDoc1` under
one `historydescription_Document_CompareDocuments` action and **returns nothing a caller could
render**. The changes are ordinary revisions (`nInsertChangesType = reviewtype_Add`,
`nRemoveChangesType = reviewtype_Remove`, `:2176-2177`).

Three details `158` did not have:

- **Authorship is the revised file's `cp:lastModifiedBy`**, falling back to the literal localised
  word "Author" (`getUserName`, `:2273-2284`), and their own help page says so.
- **The diff engine is vendored third-party code**: `sdkjs/vendor/delta.js`, Lorenz Schori's
  MIT-licensed `delta.js`, whose headers name "Myers linear space longest common subsequence" and
  the "skelmatch" tree matcher "heavily inspired by the XCC tree matching algorithm by Sebastian
  Rönnau and Uwe M. Borghoff". It is not listed in `sdkjs/3DPARTY.md`, which names only jQuery and
  XRegExp.
- **Compare is refused whenever more than one user is connected** —
  `Asc.c_oAscError.ID.CannotCompareInCoEditing` (`:3869-3874`), and the button is locked by
  `hasCoeditingUsers`. It is a single-user operation by construction, because it rewrites the whole
  document under one history action.

Granularity, proved by one line (`:3640`, `isBreakWordElement` at `:3615`): **a diff leaf is a
word** — characters accumulate until a space, tab, separator, newline, footnote/endnote reference or
punctuation. Character level is not a mode but a *nested second pass*, restricted to the strict
one-word-for-one-word replacement case (`applyInsertsToParagraphsWithRemove`, `:1382-1396` →
`resolveConflicts`, `:2202`). Their help page documents the consequence as expected behaviour:
"Documents are compared by words. If a word contains a change of at least one character … the
difference will be displayed as the change of the entire word."

### 3.4 What we do [MEASURED]

`webapp/src/compare_documents.mjs` plus `crates/casual-doc-diff/**` and
`crates/casual-doc-wasm/src/diff.rs`.

- One engine call serves both routes: `runComparison` drives `beginVersionDiff`/`step` in slices
  with progress and a Cancel. Orientation is review's — the other document is **left** (older),
  ours is **right** — so an insertion is what this document has.
- ADR-061: on a non-empty result the sidecar goes to
  `live.applyDiffAsRevisions(sidecar, author, date)` (`:987`), then `setShowingChanges(true)`.
  Author is the compared document's name, bounded to 80 code points, falling back to a localised
  "Compared document". One `Operation::UpdateReviewState` under `HistoryKind::Review`, so the whole
  comparison is one undo step.
- Four coded refusals route to sentences; `compare.refused.documentHasRevisions` is the one a reader
  will meet, and it is a **deliberate** refusal rather than a shortfall.
- `EditResult.pasteLoss` is rendered, not swallowed — what the comparison found and could not mark.
- **Algorithm** (`crates/casual-doc-diff/src/lib.rs`): Merkle projection over the ordered container
  forest, hash-tree short-circuit, prefix/suffix trim then patience anchoring then Myers, a second
  weaker-key pass for edited-vs-replaced blocks, hash join for moves, and a word-token aligner over
  **grapheme clusters** for inline text; property changes by serde reflection. Zhang–Shasha is
  explicitly rejected and the reason is written down. This is a better-specified diff than
  ONLYOFFICE's generic XML tree matcher, and it is ours.

Three model gaps against the references:

1. **No third document.** Word's and Google's answer is unavailable; `compare_documents.mjs`'s own
   header records it as a known gap. We match the minority reference.
2. **No options at all.** `renderChooser` is one "Choose a document…" button plus a warning
   sentence. Word documents 10 comparison toggles (formatting, case changes, white space, tables,
   headers and footers, footnotes and endnotes, text boxes, fields, comments, moves — all defaulting
   true), plus granularity, destination and `RevisedAuthor`; ONLYOFFICE declares the same ten and
   exposes one. We expose zero.
3. **The revisions refusal is a dead end.** ONLYOFFICE offers the consent — "In order to compare
   documents all the tracked changes in them will be considered to have been accepted. Do you want
   to continue?" — and Word offers the same Yes. Ours refuses and stops. Refusing is the *right*
   default (destroying a reviewer's suggestions is the loss `AGENTS.md` puts first); offering no way
   forward is not.

---

## 4. Compare — the presentation

**Word [SOURCE for Combine, SECONDARY for Compare].** The tri-pane description is on the *Combine*
page, verbatim: "The left section shows the **Revisions** made, the middle section shows the
**combined document**, and the right section -- which is split in two, shows the **Original
Document** and **Revised Document**"
(<https://support.microsoft.com/en-us/office/combine-document-revisions-f8f07f09-4461-4376-b041-89ad67412cfe>).
Microsoft's Compare page carries no pane description; several Microsoft Q&A threads treat the
tri-pane view as Compare's too [SECONDARY]. Source documents can be hidden, leaving "red vertical
lines showing where changes were made".

**A fully sourced change-summary spec**, which is the most directly reusable thing in this
document: "the summary section at the top of the Reviewing Pane displays the exact number of
visible tracked changes and comments that remain in your document"
(<https://support.microsoft.com/en-us/office/track-changes-in-word-197ba630-0f5f-4a8e-9a77-3712475e806a>),
broken down as "the total number of changes and the number of insertions, deletions, moves,
formatting changes, and comments"
(<https://support.microsoft.com/en-us/office/use-a-screen-reader-to-track-and-review-changes-in-a-document-in-word-8d415281-6ef2-41ea-8532-38e410be5988>).
Five categories, matching the compare toggles exactly. Note Microsoft's own admission that their
change list is for reading, not acting: "The Reviewing Pane … is not the best tool for making
changes to your document."

**ONLYOFFICE [SOURCE].** The output is ordinary revisions, so the whole review surface applies:
Accept/Reject current or all, Previous/Next, balloons, and four display modes
(`Asc.c_oAscDisplayModeInReview = {Edit:0, Final:1, Original:2, Simple:3}`, captioned "Markup and
balloons / Only markup / Final / Original"). **There is no summary count and no list-of-all-changes
pane**: a sweep for `revisionsCount`, `changesCount`, `countChanges` and `asc_GetRevisions` across
`web-apps/apps`, `sdkjs/word` and `sdkjs/common` found only a context-menu use of
`asc_GetRevisionsChangesStack()`, and a dump of ~90 `ReviewChanges`/`History` strings contains no
"N changes" string of any kind. Navigation is strictly Prev/Next plus balloons.

**Google [SOURCE + labelled inference].** The output is an ordinary Doc containing ordinary
suggestions. Suggestions are reviewable one at a time and in bulk via Tools ▸ Review suggested
edits with Accept all / Reject all
(<https://support.google.com/docs/answer/6033474>). That the compare output inherits all of that is
an **inference** from two sourced facts, not a Google statement about the compare output.

**What we do [MEASURED].** The panel is the index, per ADR-061: a sentence ("The differences are
now tracked changes in this document."), `compare.changeCount`, per-family counts, the
not-fully-compared findings, and then one `<li>` per change — kind label plus a text excerpt, or
typed field paths, or a bracketed object name, plus the story it is in. The differences themselves
are read on the canvas through the review surface that already existed: author-coloured
underline/strikethrough (`crates/casual-doc-layout/src/flow.rs:159` `apply_revision_markup`), the
review gutter, `listRevisions`, `decideRevision`, next/previous, and `w:ins`/`w:del` on export.

`158` §5's defect — "ours says `Removed`" with no object — **is fixed**: `renderResult` puts the
text excerpt first and `compare_documents.test.mjs` fails if a row can carry only its kind label.

Four presentation gaps remain:

1. **Entries are inert.** Each row is a `<li>` of `<span>`s with no click handler. ONLYOFFICE's
   balloons carry a `goto`; Word's Reviewing pane is a navigation surface. Ours points the reader at
   Review ▸ Next/Previous in a sentence. The blocker is real and named: only `right.path` survives
   into the live document and the facade exposes no `path → NodeId` resolver, so one wasm export —
   `nodeAtStoryPath(story, path) -> String` over `casual_doc_diff::projection::block_at_path`, which
   `applyDiffAsRevisions` already calls — is what is owed.
2. **Rows are ordered by FAMILY, not by document position** (`summariseDiff`,
   `compare_documents.mjs:418`). A list a reader cannot walk alongside the document is a list that
   cannot become a navigation surface even once (1) lands.
3. **The list is uncapped and unvirtualized.** Every change becomes a DOM node in one synchronous
   loop. A diff of a long document is an O(changes) main-thread render — the shape `SKILL` §8
   forbids.
4. **No summary by kind.** We publish a total and per-family counts; Word's sourced five categories
   (insertions, deletions, moves, formatting, comments) are the competitive spec and are the ones a
   reader of a redline actually counts.

One place we are **ahead**, and it should be protected rather than spent: ONLYOFFICE wraps the whole
of `CompareBinary` in `sync_StartAction(… BlockInteraction, SlowOperation)` — their comparison
**blocks the UI**. Ours slices, reports progress and cancels at a slice boundary, with the cost
measured as a ratio rather than a millisecond budget (`compare-cost.spec.mjs`). We also ship a
margin change bar, which ONLYOFFICE has none of.

---

## 5. The overlap — and the answer is that nobody has built it

| | Compare's output | Where differences live | Can you compare two versions from the history panel? |
| --- | --- | --- | --- |
| Word | a third document | tracked changes in it | **Prior vs current, yes** [SOURCE]. Two arbitrary prior versions: no documented path; every route anchors one side to current |
| Google Docs | a third document | suggestions, authored to a name you type | **No** — the per-entry menu is enumerated and holds no compare; Compare's only input affordance is "Choose document" |
| ONLYOFFICE | the open document, mutated | `reviewtype_Add`/`_Remove` on runs | **No** — zero occurrences of "compar" in all seven history UI files |
| **opendoc today** | the open document, mutated | revisions in it | **Version vs current, yes** — and it is the route that misfires (§6.2) |

The ONLYOFFICE absence was established by five independent greps and is worth recording as a
positive finding: `grep -rn -ie 'compar'` across `controller/History.js`, `view/History.js`,
`model/HistoryVersion.js`, `collection/HistoryVersions.js` and the three mobile
`VersionHistory` files returns **no matches**; `grep -rn -ie 'versionhistory' -e 'asc_showRevision'`
across `Comparison.js` and `Merge.js` returns **no matches**. Their published help page for version
history mentions "compare and merge" once, as a link to a different page.

And the reasons are architectural, not oversights. Compare mutates the live document under one undo
action and refuses with more than one user connected; history has already **closed the file and
reopened a different URL read-only with co-authoring disconnected**. Two unrelated reconstruction
mechanisms: compare reads a binary stream, history replays a change stream through the co-authoring
applier. Their "what changed" answer comes from the operation log, never from diffing content.

**Why the two features are separate everywhere, stated as the finding:** versions are a *recovery
and attribution* surface and comparison is an *adjudication* surface. Microsoft files version
history under "built-in data protection" and puts Compare on the Review ribbon beside Track
Changes. Google titles one article "Find what's changed in a file" and the other "See changes in
Google Docs over time with Compare Documents", and its inputs are two files. The intersection —
"diff two points in *this* file's history, with accept/reject" — is what none of them ships.

> **Call-out for a decision, not an assumption.** Two things the owner's complaint implies are
> things **no competitor does either**:
>
> - **Comparing two arbitrary stored versions.** Nobody. Word gets closest and still anchors one
>   side to current.
> - **A comparison launched from version history that writes into the live document.** Nobody —
>   ONLYOFFICE mutates in place from Review ▸ Compare *only*, and its history is read-only by
>   construction. **We are the only product that does this, and it is the behaviour §6.2 shows
>   misfiring.**
>
> Both are therefore open product questions. Doing them would be new, not catching up; and "no
> competitor does it" is an argument about risk, not an argument against.

---

## 6. Usefulness, honestly

### 6.1 The two features are not equally alive, and the evidence is market evidence

**Version history is in continuous development at all three.** Google: named versions 2017, Show
editors 2021, condensed history in Sheets 2025, and owner-managed history deletion added to the
main article. Microsoft: Automatic version trimming with a published thinning schedule and
PowerShell surface, plus a stated design principle. ONLYOFFICE: a two-level version/change tree,
per-change selection, deleted-text recovery, and a mobile implementation.

**Compare is the weaker feature, and five sourced signals say so:**

1. **Google has no help-centre article for it at all** — only a 2019 blog post, and it is not in the
   Tool finder.
2. **No Google update in seven years.**
3. **It is absent from Word for the web** [SOURCE], while Track Changes and Version history are
   both present. Microsoft has not brought Compare to the browser.
4. **It is absent from ONLYOFFICE mobile** entirely, while version history is present there.
5. **Nine of ONLYOFFICE's ten comparison options are unreachable from any UI**, and two more
   (`moves`, `fields`) are commented out in the constructor while the move machinery exists.
   `insertionsAndDeletions` is declared, exported, and read by nothing.

The honest verdict: **Compare is a desktop, legal-review feature that the web-first products have
either barely shipped or not shipped at all.** It is not vestigial — Word's is serious, and
Microsoft ships `RemovePersonalInformation` and `RemoveDateAndTime` specifically so a redline can be
sent to a counterparty — but it is **low-frequency and high-ceremony**, reached for when two files
must be reconciled, not during ordinary authoring.

**The real user jobs, as the sources frame them:**

- *Version history*: "undoing unintended changes, whether accidental or due to malicious activities
  like ransomware. It also ensures auditability" (Microsoft), and "Find what's changed in a file"
  (Google). It is the undo that survives a reload, plus blame. High frequency, low ceremony.
- *Compare*: Google names two audiences in its own words — educators "compare essays and track
  revisions, saving them time when grading", and business users see "what terms have changed
  throughout the negotiation process". Word's carries its audience in the feature's name, "legal
  blackline", a term of art from legal practice.

### 6.2 What the owner saw: "versions are written with no changes in them"

Reported twice, tracked as `109` OO-023, "Open — reproduction in flight", with the note that
"reading has failed twice". Reading a third time found **two distinct mechanisms**, and neither is
the digest comparison that the earlier passes correctly cleared.

**Mechanism A — "Show changes" on a selected version compares that version against itself.**
This is a chain of four facts, each read in this session:

1. Clicking a version row calls `select(...)` and then `void openPreview(row.versionId)`
   immediately — "a click is a decision already made" (`version_panel.mjs:501-505`). Enter on a
   focused row does the same (`:1536`).
2. `openPreview` parses the checkpoint and calls `showPreview(next)`, which is
   `showVersionPreview` — and that function does `doc = previewDoc`
   (`webapp/src/main.js:15758`). **The module-level live document is now the historical one.**
3. The Compare panel's right-hand side is `currentBytes: () => comparableBytes(doc, currentSourceFormat)`
   (`webapp/src/main.js:10533`) — an export of whatever `doc` currently is.
4. `comparableBytes` tries `exact_if_unchanged` first. A freshly parsed preview document has
   `revision == 0`, and the wasm facade passes `source_unchanged: self.revision == 0`
   (`crates/casual-doc-wasm/src/lib.rs:991`); the DOCX adapter's `ExportMode::ExactIfUnchanged`
   returns `source.original_bytes` verbatim (`crates/casual-doc-io/src/docx.rs:247-255`), and the
   wasm import path retains the source (`registry.import(…, true)`,
   `crates/casual-doc-wasm/src/lib.rs:26141-26150`).

So the bytes handed to the engine as "mine" are **byte-identical to the checkpoint bytes handed in
as "theirs"**, and the comparison is guaranteed to find nothing. The panel prints "No differences."
`showChangesFor`'s own doc comment still asserts the opposite — "It never touches the live document"
(`version_panel.mjs:1242`) — which was true before ADR-061 and is now false in both directions.

This is not covered by any guard, and the existing one cannot catch it:
`webapp/tests/e2e/compare.spec.mjs:228` asserts
`toContainText(/Compared with|No differences/i)` — it passes whether the comparison worked or found
nothing. That is the `SKILL` §4 vacuity pattern: a test that cannot distinguish the fixed state from
the broken one. (The spec's *other* test does get real entries, because it clicks the row's ⋮
directly and the ⋮'s click handler does not open a preview — which is exactly why reading the spec
suite suggested the feature worked.)

**Experiment that settles it, and must go red before any fix is called done:** drive the real panel —
open the demo document, make one real edit, save (two rows), then **click the older row** and only
then open its ⋮ ▸ Show changes. Assert `[data-compare-total]` is non-zero. Mutate by removing the
`openPreview` call and confirm the guard flips.

**Mechanism B — the duplicate-suppression compares SOURCE BYTES, not document content, and the
repo's own guard asserts the defect as correct.** `captureVersion`'s `skipIfUnchanged` suppresses a
row whose `checkpointId` — a SHA-256 of the artifact bytes — already appears in the lineage. That is
sound within one export mode and unsound across two, because:

- the **import** row's bytes are the verbatim original file (`exact_if_unchanged`), and
- `source_unchanged` is `revision == 0`, a monotonic watermark — so **after any edit at all, even an
  edit that is immediately undone, `exact_if_unchanged` is permanently unavailable** and every later
  checkpoint is a `preserve_when_safe` re-export whose byte layout differs from the original file's.

`webapp/tests/e2e/version-history-noise.spec.mjs` makes this explicit without naming it. Its
`neutralEdit` helper is documented as "an edit that cancels itself out: the engine's revision
watermark moves, the text does not" — `insertText("x")` then `Backspace`. Then:

```
await neutralEdit(page);
await saveDocument(page);
await expectVersions(page, 2);
```

**Two rows, for a document whose content never changed**, and `expectNoDuplicateContent` passes
because the two `checkpointId`s differ. The spec's own recorded output names the hashes:
`import cp=fb07bd2d | saved cp=28c11967`. The guard asserts the *mechanism* (hash inequality) where
the guarantee is *content* inequality — `SKILL` §10's "tests should assert the guarantee, not the
mechanism", with a concrete price: the guard that was written to close the owner's report encodes
the owner's report as expected behaviour.

**Experiment:** after `neutralEdit` + save, run Show changes on the import row (without first
selecting it, so Mechanism A is not in the way) and assert the comparison reports **zero**
differences while the store holds **two** rows. That is the defect stated as a single assertion.

### 6.3 Our own usefulness verdict

- **Version history: keep investing, and fix the definition.** It is the feature the market is
  actively developing, it is the one the owner's product outcome (`139` §1) is written around, and
  our store is genuinely ahead — ONLYOFFICE's editor has no version store at all, and Word's is the
  storage platform's. What is wrong is not the existence of the feature but that the panel cannot
  answer *what changed in this version*, which is the question all three references answer first.
- **Compare: finish it, do not expand it.** It is reached for rarely, and the two things that would
  make it credible are small and known — a clickable, document-ordered change list (one wasm export)
  and a way forward from the tracked-changes refusal. A third merged document and a full
  Word-style options dialog are the expensive half and should wait for a user who asks.
- **The overlap is the opportunity, and it is a decision.** "What changed in this version" is a
  read-only projection, not a mutation. Building it would put us ahead of ONLYOFFICE (which has no
  content diff in history at all) and level with Google. "Compare any two stored versions" would be
  genuinely new. Neither should be built by routing version history through the mutating Compare
  path, which is what we do now.

---

## 7. What the study contradicts in our own published designs

Recorded so these are corrected deliberately rather than discovered later.

### 7.1 `107` §5.3 — four claims the study contradicts

| `107` says | The study found |
| --- | --- |
| "a **version** is a named, pinned point a user can name, restore, and compare" | Inverted. A version is an **automatic, unnamed, mergeable snapshot**; naming is a capped opt-out (Google 40, ours 15) and does not exist at all in Word. Defining a version as a named point describes the exception. |
| "Version history list — Log walk grouped by **session and author**" | No reference groups by author. Google groups (key undocumented) and lists authors *within* an entry; ONLYOFFICE groups by a host-supplied `versionGroup`; our shipped panel groups by day. A log walk is also O(commits) to open, against `139` VH-001's O(versions). |
| "**Compare documents** — Replay both branches; diff the models; emit the result as tracked changes into the existing revision model" | Matches ONLYOFFICE and contradicts **both** Word and Google, which emit a third document and leave the sources untouched. `107` presents the in-place answer as the only one. |
| "**Combine documents** — transform one branch's operations onto the other — *this is the OT transform already built*, applied offline" | `crates/casual-doc-transaction/src/combine.rs` already measured the limit and states it plainly: combine works "between participants of a room, and between replicas that called `adoptParticipantIdentity`, and **not between two offline forks of one file**". Word's Combine takes two arbitrary files. So the competitive case for Combine does **not** fall out of the transform. |

`107` §1's "OT subsumes the versioning requirement … snapshot-plus-replay gives version history,
restore, and document comparison as consequences" is overstated in two places that are already
corrected elsewhere and should be corrected here: `112` measured that a normalized snapshot drops
binary resources, which is why a checkpoint is a source-format artifact (`140` §4.4); and two
arbitrary files share no history, so comparison is a diff and not a replay (`combine.rs`).

One `107` decision the study **supports**: §5.2's "Named versions pin their snapshot so a restore
point is never compacted away" is exactly Google's documented reason for naming.

### 7.2 `139` §9.4 and `140` §11.4 / §11.6 publish the opposite of what shipped

`139` §9.4, *Diff isolation*, still reads:

> "Version diff is derived data. It does not add tracked changes, comments, nodes, or marks to
> either source document. Creating a comparison document is a separate explicit later action that
> produces a new document, matching Word's source-preserving behavior."

and `140` §11.4: "The diff overlay is a read-only render decoration keyed by sidecar anchors. It
does not modify normalized nodes, review revisions, source envelopes, or export." `140` §11.6
reserves tracked-change output for a separate "Create comparison document" command.

ADR-061 decided the reverse, and `compare_documents.mjs` does the reverse. Two documents therefore
publish a requirement the code violates — and, read against §5 of this document, **the superseded
requirement is the one that matches Word and Google**, while the shipped behaviour matches
ONLYOFFICE. That makes this more than a documentation defect: it is an open product question that
was closed by an ADR without the competitive study that is now in hand.

The reconciliation that the evidence supports, offered as a proposal and not a decision: keep
ADR-061's in-place behaviour for **Review ▸ Compare**, which is a deliberate, warned, undoable act
on the document in front of the reader; and make **version history's Show changes** a read-only
projection, which is what both web competitors do and what `139` §9.4 already specified.

### 7.3 Smaller corrections

- `version_panel.mjs:30`'s design table says Show changes "lists the differences in the Compare
  panel … ours lists them beside it" — stale since ADR-061; the differences are now written into the
  document.
- `version_panel.mjs:1242`'s `showChangesFor` doc comment — "It never touches the live document" —
  false since ADR-061.
- `153`:614 says ours is "a change LIST in a side panel - Google Docs' answer". Both halves are
  wrong now: it is no longer a list-only surface, and a change list is **not** Google's answer —
  Google's Compare produces a document of suggestions.
- `158` §4 says Compare "is gated by `config.canFeatureComparison`". The flag is real, and it is set
  `true` unconditionally at `documenteditor/main/app/controller/Main.js:479`. It is **not** a licence
  gate; the real gates are integrator capability (`canUseHistory`/`canHistoryRestore`) and
  `disableNetworkFunctionality` for URL and storage sources.
- `158` leaves the impression that version-history highlighting uses the same
  underline/strikethrough markup as a comparison. It does not — §2.2.

---

## 8. Ranked defects, worst first

Each row names the competitive behaviour it violates and a one-line fix. **No code here**; these are
work items for other lanes, and `109` is where they are queued.

**Status, 2026-10-09 (ADR-065, `109` UX-047):** rows 2 and 5 are fixed — a version preview is the
version with its changes against its predecessor painted in place (a read-only redline, Google's
model), and every Compare entry takes the reader to its change in document order. Row 3 was fixed by
ADR-064 and stays fixed: neither route writes into the reader's document until Compare's explicit
**Keep as tracked changes**. Row 6 is narrowed rather than closed: viewing a comparison no longer
refuses a document with suggestions; only Keep does.

| # | Defect | Measured at | Competitive behaviour violated | One-line fix |
| --- | --- | --- | --- | --- |
| 1 | **"Show changes" on a selected version reports "No differences", always.** Clicking a row opens a preview, the preview replaces the live document, and the comparison runs that version against itself. | `version_panel.mjs:501-505`, `main.js:15758`, `main.js:10533`, `casual-doc-wasm/src/lib.rs:991`, `casual-doc-io/src/docx.rs:247` | Google: selecting a timestamp shows that version *with its edits in author colour*. ONLYOFFICE: selecting an entry replays that version's changes as coloured marks. Neither can return "nothing changed". | Compare the selected version against its **predecessor**, not against `doc`; capture the comparison's right-hand side before the preview swap. |
| 2 | **A version preview shows no indication of what changed in it.** The checkpoint renders plain. | `main.js:15753-15792` — no markup, no diff, `setReviewMode("viewing")` only | Both web competitors answer this question first, in place and read-only (§2.1, §2.2). It is the question the panel is opened to ask. | Render the version-vs-predecessor diff as a **read-only projection** in the preview — `139` §9.4's own requirement. |
| 3 | **Version history routes through a mutation.** Show changes writes tracked changes into the reader's current document. | `version_panel.mjs:1276` → `compare_documents.mjs:1007` → `:987` | **No competitor does this.** ONLYOFFICE mutates only from Review ▸ Compare and its history is read-only by construction; Word and Google produce third documents. | Split the two routes: keep ADR-061 in place for Review ▸ Compare; make version history's Show changes read-only (ties to #2). |
| 4 | **The duplicate guard asserts the defect as correct behaviour.** `neutralEdit` + save ⇒ two rows for one document, and `expectNoDuplicateContent` passes because the hashes differ across export modes. | `version-history-noise.spec.mjs:93-99`, `:136-138`; `version_history.mjs` `skipIfUnchanged` | Google merges revisions rather than keeping indistinguishable ones; the owner's rule, recorded in ADR-038 decision 5 as reversed, is that nothing new is not a version. | Dedupe on a **content identity** (the semantic projection's digest), not on source-format bytes; make the guard assert "no two rows hold the same **document**". |
| 5 | **Compare entries are inert and ordered by family.** No row navigates; the list is sorted by change family, uncapped and unvirtualized. | `compare_documents.mjs:418` (sort), `:905-950` (render, no handler) | ONLYOFFICE's balloons carry `goto`; Word's Reviewing pane is a navigation surface. | Add the one wasm export `nodeAtStoryPath(story, path)` over `block_at_path`, order rows by document position, and virtualize the list. |
| 6 | **The tracked-changes refusal is a dead end.** | `compare_documents.mjs` `REFUSAL_KEY`, `compare.refused.documentHasRevisions` | ONLYOFFICE asks for consent and proceeds ("…will be considered to have been accepted. Do you want to continue?"); Word shows a message box and proceeds on **Yes**. | Offer the consent the refusal implies as an explicit second action, or give a comparison-authored revision a distinct origin so the two can coexist and be filtered. |
| 7 | **Compare exposes no options whatsoever.** | `renderChooser`, `compare_documents.mjs:736-757` | Word documents 10 toggles + granularity + destination + `RevisedAuthor`; ONLYOFFICE exposes word-vs-character. | Ship **word/character granularity** first (the only one ONLYOFFICE bothered with) as a real preference; defer the rest to a user who asks. |
| 8 | **Retention defaults cannot keep the promise `139` §1 makes.** 7 days, 25 versions, floor 3. | `webapp/src/settings_defaults.mjs:78-94` | Google documents no expiry for native Docs and merges instead; SharePoint's default is 500 with **no expiration**. | Raise the age window well above a week, or age-prune only above a generous count floor — and disclose the real window where the promise is made. |
| 9 | **No change summary by kind.** | `summariseDiff` returns a total and per-family counts | Word's Reviewing pane shows total + insertions + deletions + moves + formatting + comments — a fully sourced five-category spec (§4). | Publish the five categories alongside the family counts. |
| 10 | **Version history is gated on autosave and on a preference.** | `historyUnavailableReason`, `version_policy.mjs`; `settings_defaults.mjs:74` | No competitor makes history conditional on a user-flippable setting. Google gates on *edit permission*, which is a different thing. | Decide deliberately whether a document-safety net may be switchable, and if it stays switchable, say in the panel what turning it off costs. |
| 11 | **Stale published prose, four places.** `139` §9.4, `140` §11.4/§11.6, `version_panel.mjs:30`, `version_panel.mjs:1242`, `153`:614, `158` §4. | §7 | — | Correct each in the same PR as the behaviour it describes; do not leave a published requirement contradicting shipped code. |
| 12 | **`compare.spec.mjs:228` cannot fail.** `toContainText(/Compared with\|No differences/i)` passes either way. | `webapp/tests/e2e/compare.spec.mjs:228` | `SKILL` §4 — a guard that cannot fail is worse than no guard, because it gets cited as evidence. | Assert a non-zero `[data-compare-total]` on a known-different pair, and prove it red by reverting the route. |

### Things no competitor does either — to be decided, not assumed

- **Comparing two arbitrary stored versions.** Nobody. Word anchors one side to current; Google's
  per-entry menu is enumerated and holds no compare; ONLYOFFICE's history UI never mentions
  comparison. If we build it, we are first, and the risk is ours.
- **A comparison launched from version history that writes into the live document.** Only us
  (defect #3). Worth stating plainly: the behaviour the owner is unhappy with is not a weak version
  of a competitor's feature — it is a feature no competitor has, misfiring.
- **A clickable change list for a comparison.** Word has a Reviewing pane and ONLYOFFICE has
  balloons with `goto`; neither has a panel that indexes a comparison the way ours is shaped to. Our
  shape is defensible; it is unfinished (defect #5), not wrong.

---

## 9. Open questions this study did not close

1. **The Google "Show changes" checkbox.** Fenced as [UNSOURCED] in §2.1 and cited as sourced by
   `version_panel.mjs:30`. One screenshot settles it.
2. **Whether a Google restore appears as a new history entry.** Undocumented for Docs. The
   Sheets-specific warning about per-cell history must not be read across.
3. **Whether ONLYOFFICE's version highlight visibly covers all of a version's changes or only the
   last one.** The literal code passes a colour only for `i === newChangeId`
   (`sdkjs/common/versionHistory.js:105`) and `RunChanges.js:374` records a mark only when a colour
   is present. That is a code reading, not an observed behaviour; the product was not run.
4. **Who, server-side, decides that a save becomes a new ONLYOFFICE version or a new
   `versionGroup`.** Not in either checkout; the document server is a separate repository.
5. **Whether our `preserve_when_safe` DOCX export is byte-deterministic for an unchanged model.**
   Defect #4's fix does not depend on it, but if it is *not* deterministic then byte-hash dedupe is
   even weaker than §6.2 shows. Experiment: export the same unmodified document twice in one session
   and compare digests.
6. **Whether a merged third document is ever built** (`158` §8 question 4, still open). §5 now gives
   it competitive weight — it is two of three references — but not a decision.
