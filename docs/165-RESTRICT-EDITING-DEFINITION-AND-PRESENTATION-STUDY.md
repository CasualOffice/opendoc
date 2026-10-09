# 165 — Restrict Editing: what it MEANS, where it LIVES, and whether it is worth having

**Status:** Research only. **No implementation, no code, no PR.** **Opened:** 2026-10-04.
**Owner:** unassigned.

**Question asked.** The owner's judgement is that our Restrict Editing is wrong at the level
of **definition and presentation** — *"the definition and presentation, everything, is not
what a user expects and not how others are doing it"* — and that the study must also answer
whether the feature is **useful**, mode by mode, rather than return a feature list.

This document answers six questions, establishes what **we** do today from the code, and
ends with a ranked defect list that separates *our UI is wrong* from *our MODEL is wrong*,
because the second is the expensive finding and it is the one the owner is naming.

---

## 0. Evidence rules this document obeys

`SKILL` §9 exists because the public pages carried fabricated claims twice. So:

- Every competitive claim carries a URL or an absolute path plus an identifier.
- ONLYOFFICE claims are read from the **checkout at `/Users/sachin/Desktop/melp/reference/`**
  (`sdkjs` HEAD `72b0421`, `web-apps` HEAD `9c0ca538`, both 2026-05-19, release **v9.4.0**),
  not from their marketing. Only the two *client* repos are checked out; the C++ converter
  (`x2t`) that actually parses `w:documentProtection` out of OOXML is **not**, and
  `grep -rn "w:documentProtection" reference/sdkjs` returns **zero** — so every statement
  below about their XML parsing is about their *binary* format, and every statement about
  enforcement is about their *browser client*.
- Every claim about **us** is measured from this tree.
- Recollection is fenced in `159` §7.3's shape and may not be cited downstream as sourced.
- Where nothing could be sourced, the row says so and names the experiment.

---

## 1. What "restrict editing" MEANS in each product

### 1.1 Word — two independent axes, four modes, and per-range exceptions

Word's Restrict Editing is **two restrictions in one pane**, and we ship only one of them.

**Axis A — formatting restrictions.** `w:documentProtection/@w:formatting`, which Microsoft's
SDK names *"Only Allow Formatting With Unlocked Styles"*
(<https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.documentprotection>).
The allowed-style whitelist is authored at **Home ▸ Styles dialog launcher ▸ Manage Styles ▸
Restrict**, whose exact labels Microsoft documents as `Limit formatting to permitted styles`,
`Allow AutoFormat to override formatting`, `Block Theme or Scheme switching`,
`Block Quick Style Set switching`
(<https://support.microsoft.com/en-US/Word/restrict-or-permit-formatting-changes>). The API
counterpart is `Document.Protect`'s `EnforceStyleLock`
(<https://learn.microsoft.com/en-us/office/vba/api/word.document.protect>).

> **UNSOURCED from Microsoft:** the Restrict Editing pane's own strings
> `Limit formatting to a selection of styles`, the `Settings…` link, and the dialog title
> *Formatting Restrictions*. These are third-party-attested only. *Experiment:* open
> Review ▸ Restrict Editing ▸ Settings… in a current M365 build and dump the pane's
> accessibility tree.

**Axis B — editing restrictions**, under the sourced label
`Allow only this type of editing in the document`
(<https://support.microsoft.com/en-us/office/allow-changes-to-parts-of-a-protected-document-187ed01c-8795-43e1-9fd0-c9fca419dadf>).
What each mode permits is **normative**, from `ST_DocProtect`, and the wording matters more
than the labels do:

| `w:edit` | What the spec says it restricts edits to |
| --- | --- |
| `none` | "no editing restrictions have been applied" |
| `readOnly` | "**the editing of regions delimited by range permissions** which match the editing rights of the user account which is performing the editing" |
| `comments` | "the insertion and deletion of comments **and** the editing of regions delimited by range permissions…" |
| `trackedChanges` | "edits … shall be tracked as revisions. **This value shall imply the presence of the `trackRevisions` element, and applications shall not allow that element's state to be changed to false.**" |
| `forms` | "the editing of form fields in sections where the `formProt` element has a value of true [and] no restrictions in sections where `formProt` is false" |

(ECMA-376 Part 4 `ST_DocProtect`, third-party rendering of the standard:
<https://c-rex.net/samples/ooxml/e1/Part4/OOXML_P4_DOCX_ST_DocProtect_topic_ID0EHTR2.html>.
Microsoft's own enum descriptions agree: "Allow No Editing", "Allow Editing of Comments",
"Allow Editing With Revision Tracking", "Allow Editing of Form Fields" —
<https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.documentprotectionvalues>.)

**Three things follow that we have modelled wrongly or not at all.**

1. **`readOnly` does not mean "nothing may change."** It means *nothing but the permitted
   ranges*. The exceptions mechanism is part of the definition of `readOnly` and `comments`,
   not a decoration on top of them. Our `ProtectionRefusal::ReadOnly` reads
   `"nothing may change"` and is only accidentally right, because we do not implement ranges.
2. **`trackedChanges` is an obligation on the application, not only a filter on operations.**
   The spec says the application **shall not** let tracking be turned off. Microsoft states
   the user-visible consequence: *"While tracked changes are locked, you can't turn off change
   tracking, and you can't accept or reject changes."*
   (<https://support.microsoft.com/en-us/word/training/track-changes-in-word>.) We enforce the
   second half exactly and the first half not at all.
3. **`forms` is section-scoped**, through `w:sectPr/w:formProt` ("Only Allow Editing of Form
   Fields", <https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2010/cc863047(v=office.14)>),
   and that is reachable from the UI: *"to protect only parts of the document, separate the
   document into sections and only protect the sections you want"* via `Select Sections`
   (<https://support.microsoft.com/en-us/office/create-a-form-in-word-that-users-can-complete-or-print-040c5cc1-e309-445b-94ac-542f732c8c8b>).
   The object model confirms it: `Section.ProtectedForForms` — *"When a section is protected
   for forms, you can select and modify text only in form fields"*
   (<https://learn.microsoft.com/en-us/office/vba/api/word.section.protectedforforms>).

### 1.2 Google Docs — there is no "restrict editing", and that is the finding

**Verified, including with a clean negative.** Google Docs has no protection pane, no
password on protection, no per-range editor list, no form-filling mode, and **no protection
request anywhere in the Docs API**: the entire `documents.batchUpdate` `Request` union is 48
request types and not one of them touches protection, locking, permissions, or editable
ranges (<https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/request>).

What serves the job instead is **sharing roles**, with these exact labels and definitions
(<https://support.google.com/drive/answer/2494822>):

| Role | Google's own words |
| --- | --- |
| Viewer | "can view and download files (by default), but cannot comment or edit" |
| Commenter | "can view, download, and comment, but cannot edit" |
| Editor | "can view, download, edit, and share files"; "an editor can't change the owner" |

API role strings, which are the contract a host actually codes against:
`owner`, `organizer`, `fileOrganizer`, `writer`, `commenter`, `reader`
(<https://developers.google.com/workspace/drive/api/guides/ref-roles>).

Plus exactly two per-document switches on the share dialog, both verbatim:
**"Viewers and commenters can see the option to download, print, and copy"**
(<https://support.google.com/a/users/answer/9283053>) and
**"Editors can change permissions and share"**
(<https://support.google.com/drive/answer/2494893>), plus **"Add expiration"** on eligible
work/school accounts (same page).

Two caveats that keep the claim honest:

- Google **does** have a binary document lock, but it lives in **Drive**, not Docs: the Drive
  API's `contentRestrictions.readOnly` — "prevent users from modifying the title, making
  content edits, uploading a revision, and adding or modifying comments" — with the explicit
  disclaimer **"A content restriction isn't an access restriction"**
  (<https://developers.google.com/workspace/drive/api/guides/content-restrictions>). Its
  end-user surface is Drive Approvals' `Lock` / `Unlock`
  (<https://support.google.com/drive/answer/9387535>).
- Google's **Suggesting** mode is not a document state. In the API, suggestion-ness is a
  property of the **write request**: *"To apply edits as suggestions, set the `writeMode`
  field of the `WriteControl` object to `SUGGEST`"*
  (<https://developers.google.com/workspace/docs/api/how-tos/suggestions>). So Word's
  `w:edit="trackedChanges"` has **no Google counterpart at all** — the nearest thing is a
  per-request flag, not a stored restriction.

### 1.3 ONLYOFFICE — Word's four modes, their own tab, and a 19-key host contract

Their help calls it the **Protection tab** and states its purpose verbatim: *"The Protection
tab of the Document Editor allows protecting your documents with a password while setting
restricted access rights"*
(<https://helpcenter.onlyoffice.com/docs/userguides/document_editor/protectiontab.aspx>),
shipped from **version 7.3**
(<https://helpcenter.onlyoffice.com/docs/userguides/document_editor/password.aspx>).

The code agrees, and gives the exact option set. `ProtectDialog.js` template lines 65-84
(`/Users/sachin/Desktop/melp/reference/web-apps/apps/documenteditor/main/app/view/ProtectDialog.js`)
holds a bold label `txtAllow` = **"Allow only this type of editing in the document"** — Word's
sentence, copied — over four radios in **this** DOM order, with `rbView` **checked by
default** at line 127:

| Radio | Label | Enum |
| --- | --- | --- |
| `rbView` | "No changes (Read only)" | `Asc.c_oAscEDocProtect.ReadOnly` |
| `rbForms` | "Filling forms" | `Forms` |
| `rbReview` | "Tracked changes" | `TrackedChanges` |
| `rbComments` | "Comments" | `Comments` |

Above the radios sit `inputPwd` (`maxLength` **15**, under the label "Password (optional)") and
`repeatPwd`; below them `txtWarning` = *"Warning: If you lose or forget the password, it cannot
be recovered. Please keep it in a safe place."* The dialog title and its OK button are both the
bare string "Protect". **Notably absent: no exceptions section and no formatting restriction.**
Word has both; ONLYOFFICE has neither.

Two incidental points. First, `document_protection.mjs`'s own header cites this file and its
identifiers, and **the quotation was verified literally correct** — every identifier, enum
value and line range in our comment is accurate against the checkout. Second, their
`_setDefaults` has an **empty body** (`ProtectDialog.js:202-205`), so the dialog never
pre-populates from the protection already in force — consistent with
`asc_setDocumentProtection` refusing a mode change without unprotecting first
(`reference/sdkjs/word/api.js:14369-14378`). **Ours does pre-populate** (`group.reflect(state.value)`),
and reads five document states rather than four, so this is one of the few places our dialog is
already the better one.

Their runtime model is two enums and a bridge. `c_oAscEDocProtect`
(`reference/sdkjs/word/apiDefines.js:398-404`) is the *document* value; `c_oAscRestrictionType`
(`reference/sdkjs/common/commonDefines.js:608-616`) is a **runtime bitmask**
`{None:0x00, OnlyForms:0x01, OnlyComments:0x02, OnlySignatures:0x04, View:0x80}`; and
`CDocProtect.prototype.getRestrictionType`
(`reference/sdkjs/word/Editor/DocumentProtection.js:111-127`) maps between them —
**with `TrackedChanges` falling through to `null`.**

That fall-through is load-bearing. At load (`reference/sdkjs/word/Editor/Serialize2.js:8519-8525`)
the restriction is applied only `if (enforcement !== false && restrictionType !== null)`, so
**`w:edit="trackedChanges"` applies no engine restriction in ONLYOFFICE at all.** It is
honoured by forcing `w:trackRevisions` true on save (`Serialize2.js:7227-7236`) and by locking
the toolbar (`Common.enumLock.docLockReview`). A client that ignores the lock edits untracked.
**Our engine is stronger than theirs on this mode**, and that is a real, demonstrable
difference rather than a marketing one.

Separately, they carry a **19-key host `permissions` object** — `chat`, `comment`,
`commentGroups.{edit,remove,view}`, `copy`, `deleteCommentAuthorOnly`, `download`, `edit`,
`editCommentAuthorOnly`, `fillForms`, `modifyContentControl`, `modifyFilter`, `print`,
`protect`, `review`, `reviewGroups`, `userInfoGroups`
(<https://api.onlyoffice.com/docs/docs-api/usage-api/config/document/permissions/>) — read in
`reference/web-apps/apps/documenteditor/main/app/controller/Main.js:1690-1830`. The
architecturally interesting part is that `comment`, `review` and `fillForms` **default to the
value of `edit`**: `edit` is the root of the lattice.

---

## 2. Where it lives, and what it is called

| | Word | ONLYOFFICE 9.4.0 | Google Docs | **us** |
| --- | --- | --- | --- | --- |
| Surface | **Review ▸ Protect ▸ Restrict Editing** — a docked **task pane** | **its own "Protection" ribbon tab**, inserted at index 7 | **nothing in the editor** — it is in **Share** | Review band ▸ one icon-only `lock` button → a **modal dialog** |
| Second entry point | Developer ▸ Restrict Editing (same feature) | File ▸ Protect (`fm-btn-protect`) | — | Review menu band `menuGroup.protect`; command palette |
| Name shown | "Restrict Editing" | tab "Protection"; button "Protect Document" | "Share" / role names | "Restrict editing" |
| Persistent state shown | pane stays open, says the document is protected, offers region navigation | a per-mode tip banner (§2.2) | the role is the chrome | **nothing** |

Sources: Word ribbon path *"On the Review tab, in the Protect group, select Restrict Editing"*
(<https://support.microsoft.com/en-us/office/allow-changes-to-parts-of-a-protected-document-187ed01c-8795-43e1-9fd0-c9fca419dadf>);
Developer path (<https://support.microsoft.com/en-us/office/create-a-form-in-word-that-users-can-complete-or-print-040c5cc1-e309-445b-94ac-542f732c8c8b>);
ONLYOFFICE tab registration `reference/web-apps/apps/documenteditor/main/app/controller/Toolbar.js:3998-4013`
(`if (config.canProtect) { … me.toolbar.addTab(tab, $panel, 7); }`), caption
`DE.Views.Toolbar.textTabProtect` = "Protection"; Google share roles as in §1.2.

### 2.1 The most important finding in this study, and it holds up

**Word models protection as a property of the document; Google models it as a property of the
share.** That is not a UI difference, it is a different object. Word's restriction is bytes
in `settings.xml` that travel with the file and bind every reader. Google's is a row in an
ACL that binds one person and does not travel with a copy.

The consequence for a web-first editor is sharp: a Google role is **knowable before first
paint** and can therefore shape the whole chrome; a Word restriction is knowable only after
the file is parsed, and binds everyone identically, so it cannot express "you may edit, they
may not." They answer different questions, and **the question most users actually have is
Google's.**

ONLYOFFICE is the proof that you can hold both — and it is also the proof of what that costs:
their Protection tab conflates **three unrelated features** (edit restriction, file
encryption, digital signature) behind one `permissions.protect` flag, glued together at
runtime from two controllers either side of a `<div class="separator long">`
(`reference/web-apps/apps/common/main/lib/view/Protection.js:58`).

### 2.2 Their presentation is a persistent sentence; ours is a transient toast

ONLYOFFICE ships one string per mode, shown as a `Common.UI.SynchronizeTip` on app-ready and
re-shown whenever a co-author changes protection
(`reference/web-apps/apps/documenteditor/main/app/controller/DocProtection.js:217,264`):

- `txtDocProtectedView` — "Document is protected.<br>You may only view this document."
- `txtDocProtectedTrack` — "Document is protected.<br>You may edit this document, but all changes will be tracked."
- `txtDocProtectedComment` — "Document is protected.<br>You may only insert comments to this document."
- `txtDocProtectedForms` — "Document is protected.<br>You may only fill in forms in this document."
- `txtWasProtectedView` / `…Track` / `…Comment` / `…Forms` — "Document has been protected by another user.\nYou may only view this document."
- `txtWasUnprotected` — "Document has been unprotected."

(All from `reference/web-apps/apps/documenteditor/main/locale/en.json`.) Word's equivalent is
the task pane itself, which **stays open** and carries the state plus
`Find Next Region I Can Edit` and `Show All Regions I Can Edit`.

**We ship none of this.** Our four sentences (`document.protectedReadOnly` and siblings in
`webapp/src/en_strings.mjs:1230-1233`) are *refusal* strings: they appear **after** the user
tries to type. See §7.3.

---

## 3. Is it security, or is it a guard rail?

**It is a guard rail, and every vendor says so in writing. This is the one question where the
sourcing is unambiguous.**

The strongest citation is the standard's own note, which Microsoft republishes verbatim on
Learn:

> "*Document protection* is a set of restrictions used to prevent unintentional changes to
> all or part of a WordprocessingML document. **[Note: This protection does not encrypt the
> document, and malicious applications might circumvent its use. This protection is not
> intended as a security feature. end note]**"
> — ISO/IEC 29500-1 §17.15.1.29, quoted in the Remarks of
> <https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.documentprotection>

`w:writeProtection` carries the same note: *"like document protection, this setting is not
intended as a security feature and can be ignored"*
(<https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.writeprotection>).

Microsoft's own engineering blog puts it in plain words:

> "The element (documentProtection tag) is used to 'Protect' a document in Open XML, not to
> 'Secure' a document. Document 'Protection' is in plain text XML (in the ZIP package.) One
> can open the zip and remove the protected element."
> — <https://learn.microsoft.com/en-us/archive/blogs/vsod/how-to-set-the-editing-restrictions-in-word-using-open-xml-sdk-2-0>

And the user-facing article is honest in both directions: *"The password's optional. But if
you don't add a password, anyone can click Stop Protection and edit the document"*, and
*"Making your document read only doesn't prevent someone from making a new copy of the
document and saving it with a different name or in a different place"*
(<https://support.microsoft.com/en-us/office/make-a-document-read-only-in-word-5c25909c-46d9-4eb0-9d1f-d072a560e340>).
Mark as Final gets the same treatment: *"The Mark as Final command is not a security
feature"*
(<https://support.microsoft.com/en-us/office/collab-files/help-prevent-changes-to-a-final-version-of-a-file>).

Word's **real** boundaries are two different features, and Microsoft separates them cleanly:
`Encrypt with Password` (<https://support.microsoft.com/en-us/office/protect-a-document-with-a-password-05084cc3-300d-4c1a-8416-38d3e37d6826>),
and IRM, whose limits Microsoft enumerates frankly ("IRM can't prevent restricted content
from being … hand-copied or retyped from a display; digitally photographed by a recipient")
with permission levels `Read` / `Change` / `Full Control`
(<https://support.microsoft.com/en-us/office/restrict-access-to-documents-with-information-rights-management-in-word-94aa8ab1-465e-42d7-a323-d61f911b2d0f>).
Within Restrict Editing itself, the `User authentication` radio is the IRM branch and
Microsoft notes the behavioural tell: the password branch lets people "work on the document
at the same time", while "encrypting the document prevents others from working on the
document at the same time".

Google says the same thing about its own switches: *"You can limit how people share, print,
download, and copy within Google Drive, Docs, Sheets, and Slides. However, you can't stop
how others share the file content in other ways"*
(<https://support.google.com/drive/answer/2494893>), and *"A content restriction isn't an
access restriction"*. Even the checkbox is named for the *option*, not the capability —
"Viewers and commenters can **see the option** to download, print, and copy".

ONLYOFFICE is the exception that proves the rule. They implement the password properly —
`generateHashParams()` returns `{spinCount: 100000, saltValue: 16 random bytes}`
(`reference/sdkjs/common/editorscommon.js:14917-14919`), default algorithm SHA-512
(`reference/sdkjs/word/api.js:14415-14416`), Word's legacy pre-mangle implemented as
`AscCommon.prepareWordPassword` (`reference/sdkjs/common/hash/hash.js:162`) with **both** hash
forms accepted (`api.js:14470-14480`), verified against `documentProtection.hashValue` on
unprotect (`api.js:14439-14462`). And it is still not a boundary:

- the comparison is a **string equality in the browser** (`api.js:14390`);
- `asc_setRestriction` is **publicly exported** (`reference/sdkjs/common/apiBase.js:6138-6140`)
  and never consults `Settings.DocumentProtection`, so one console call lifts everything;
- a plugin can do it with a string — `pluginMethod_SetEditingRestrictions` accepts `"none"`
  (`reference/sdkjs/word/api_plugins.js:1272-1290`);
- the password hash **rides the co-editing channel** to every connected client inside
  `CChangesDocumentProtection` (`reference/sdkjs/word/Editor/DocumentChanges.js:795-870`);
- and `password == null` skips verification entirely (`api.js:14484-14486`).

**The honest claim, which we may make and they cannot:** protection is an *authoring
contract* enforced at the operation, plus — separately — a *host-signed grant* enforced at
the operation and again at the relay. The first is policy for everyone; only the second is a
boundary, and only against a client that cannot forge the token. `crates/casual-doc-edit/src/access.rs`
already says exactly this in its module docs, which is the right place for it.

One last, cheap differentiator: ONLYOFFICE's protection is **licence-gated**.
`isProtectionSupport = licenseResult['protectionSupport']`
(`reference/sdkjs/common/apiBase.js:1884,4092-4097`) gates `isPasswordSupport`, so a licence
flag can switch the feature off. Apache-2.0 with it always on is the wedge (`SKILL` §1).

---

## 4. The forms case — the one mode with a distinct job

Forms is the only mode where the user is **not** a frustrated author. The job is "fill this
in and send it back", and all three products treat it as a different *product surface*, not a
different filter.

- **Word.** Editable content is "form fields **or embedded controls** that are part of the
  current section", and the rest of the section "cannot be edited by users"
  (`w:formProt`, ISO 29500 §17.6.6). It is per-section, with `Select Sections` in the pane.
  There is a real side effect worth knowing: `Document.Protect`'s `NoReset` parameter —
  *"False to reset form fields to their default values; True to retain the current form field
  values"*, and *"If Type is not wdAllowOnlyFormFields, NoReset is ignored"*
  (<https://learn.microsoft.com/en-us/office/vba/api/word.document.protect>). Protecting for
  forms **wipes entered values** unless the caller says otherwise.
- **ONLYOFFICE.** Forms is both a restriction *and a separate build.*
  `reference/web-apps/apps/documenteditor/forms/` is a sibling of `main/` with its own locale
  and a form-filler chrome rather than a ribbon: `textNext` = "Next field",
  `textClear` = "Clear all fields", `textSubmit` = **"Complete & Submit"**,
  `tipFillStatus` = "Filling status", `textSaveAs` = "Save as PDF". Three independent paths
  put the *main* editor into `OnlyForms`: in-document protection, host `permissions.fillForms`,
  and a **"Fill Form" preview toggle** on the Forms ribbon tab
  (`reference/web-apps/apps/documenteditor/main/app/view/FormsTab.js:964`, controller
  `FormsTab.js:320,596`) alongside `Previous Field` / `Next Field` / `Submit`. `IsFillingFormMode`
  is their single most-threaded predicate: **78** call sites in `reference/sdkjs/word/`, against
  14 for `CanEdit` and 12 for `isRestrictionView`. Their documented host recipe is
  `permissions.edit: false` + `permissions.fillForms: true` + `documentType: "pdf"`
  (<https://api.onlyoffice.com/docs/docs-api/get-started/how-it-works/embedding-forms-into-a-web-page/>),
  and the format history is explicit on the same page: *"PDF forms are available starting from
  version 7.0"*, *"Starting from version 8.0, the OFORM format is deprecated"*, *"Starting
  from version 8.1, the DOCXF format is deprecated"*. Per-signer field access is
  `user.roles` — "determines which PDF form fields a user can fill"
  (<https://api.onlyoffice.com/docs/docs-api/usage-api/config/editor/>).
- **Google Docs.** No form-filling mode and no form fields; the gap is filled by Marketplace
  add-ons. The one nuance that keeps the claim honest is **eSignature**, which does give Docs
  typed, per-signer fields — `Signature, Initials, Name, Text field, Date signed`, under
  "Insert fields for" with "Manage signers", max 200 fields and 10 signers
  (<https://support.google.com/docs/answer/12315692>). So the precise claim is: *Google has
  per-signer fields for signature requests, but nothing that says "this document may only be
  filled in."*

**What happens to the non-form parts** is the whole presentation question, and both references
answer it the same way: the rest of the document stops looking editable, and the chrome changes
to a navigator. Word gives `Find Next Region I Can Edit`. ONLYOFFICE goes much further, and the
measurements are the argument:

- The forms app has **no ribbon at all** — `forms/index.html:238` is literally
  `<div class="toolbar style-off-tabs" id="toolbar">` — and loads **6** controllers
  (`ApplicationController`, `Plugins`, `SearchBar`, `RightMenu`, `Fonts`, `Shortcuts`) against
  the main app's **30** (`main/app.js:137-171`). No `Toolbar`, `Statusbar`, `LeftMenu`,
  `DocProtection`, `Comments`, `ReviewChanges`, `Chat` or `History`. It is its own deploy target
  (`build/appforms.json`, `build/appforms.js`, wired at `build/Gruntfile.js:142`) and flags
  itself at runtime with `features: {uitype: 'fillform'}` (`forms/app.js:142`).
- And in the **main** editor under a forms restriction, the ribbon **collapses to a single
  Forms/Home tab, auto-selected** (`Toolbar.js:4043-4055`), with
  `DisableToolbar(…fillformmode…)` at `Toolbar.js:3786-3813`.
- The caret is moved into a field automatically —
  `private_CheckCursorPosInFillingFormMode` (`reference/sdkjs/word/Editor/Document.js:23382-23396`)
  calls `MoveToFillingForm(true)` when it is outside one — and a form field's inner content is
  immutable *outside* filling mode (`StructuredDocumentTags/InlineLevel.js:2181`).

One nuance that keeps their deprecation story honest: at v9.4.0 the forms app gates on **PDF
only** (`forms/app/controller/ApplicationController.js:689-690`, comment "can fill forms only in
pdf format") while the main editor still accepts `docxf|oform` for *authoring*
(`Main.js:601-603`). So it is a half-finished migration rather than a completed deprecation, and
the flag is still named `isOFORM`.

**We enforce this mode correctly and present it as nothing.** `w:edit="forms"` is the one mode
we have enforced since before ADR-052 (`crates/casual-doc-wasm/src/lib.rs:14787-14812`) and
HF-175 closed the loan-agreement defect, so typing in a `FORMTEXT` and ticking a
`FORMCHECKBOX` work. But there is **no field navigation at all** — `grep -rn "nextFormField"`
over `crates/` and `webapp/src` returns nothing — no fill-status, no submit, no caret
correction, and no chrome change. A reader of a 111-field loan agreement gets the full editing
ribbon and must hunt for the fields with the mouse.

---

## 5. Regions / exceptions — and we are behind **both** references

### 5.1 The OOXML representation

`w:permStart` / `w:permEnd`, paired by `w:id`
(<https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.permstart>):

| Attribute | Meaning |
| --- | --- |
| `w:id` | "a unique identifier for an annotation"; an unmatched `permStart` makes the document **non-conformant** |
| `w:edGrp` | "an alias (or editing group) which shall be used to determine if the current user shall be allowed to edit this range" — `none`, `everyone`, `administrators`, `contributors`, `editors`, `owners`, `current` |
| `w:ed` | a single named user, in one of three forms: `DOMAIN\username`, `user@domain.com`, or bare `user` |
| `w:colFirst` / `w:colLast` | "the zero-based index of the first/last column in this row which shall be part of this range permission" — the discontiguous-table-column mechanism |
| `w:displacedByCustomXml` | — |

The permitted-parent list is very broad — body, `w:p`, `w:tbl`, `w:tr`, `w:tc`, `w:hdr`,
`w:ftr`, footnote, endnote, comment, `w:sdtContent`, `w:customXml`, `w:hyperlink`,
`w:fldSimple`, `w:ins`/`w:del`/`w:moveFrom`/`w:moveTo`, OMML math, `w:bdo`, `w:dir`,
`w:rt`/`w:rubyBase`, `w:docPartBody`, `w:smartTag`. **So range permissions are a
uniform-flow concern, not a body-only one**, which is exactly the rule `SKILL` already states
for every block container.

Word's UI is sourced end to end: select a range, then *"Under Exceptions, to allow anyone who
opens the document to edit the part that you selected, select the Everyone check box in the
Groups list"*, or **More users** with *"Separate each name with a semicolon"* and *"be sure to
type e-mail addresses for user names"*. The reader-side affordances are
`Find Next Region I Can Edit`, `Show All Regions I Can Edit` and the
`Highlight the regions I can edit` checkbox, and the refusal sentence is
*"This modification is not allowed because the selection is locked."* (All from
<https://support.microsoft.com/en-us/office/allow-changes-to-parts-of-a-protected-document-187ed01c-8795-43e1-9fd0-c9fca419dadf>;
the `Highlight…` string is sourced only to a Microsoft support-agent reply at
<https://learn.microsoft.com/en-us/answers/questions/5726960/editable-region-shading-is-now-permanently-enforce>
— label that as agent answer, not product documentation.)

### 5.2 Does anyone else implement it? Yes — ONLYOFFICE, further than we assumed

This was the biggest correction of the study, and it **refutes an assumption inside this
repository**. `reference/sdkjs/word/Editor/annotations/` holds `paragraph-perm.js`,
`perm-ranges-manager.js` and `annotation-mark-base.js`. `ParagraphPermStart(rangeId, colFirst,
colLast, displacedByCustomXml, ed, edGrp)` models **Word's full attribute set**, including
`getEd()` and `getEdGrp()`, and round-trips all six through a dedicated serialisation enum —
`c_oSerPermission = { Id:0, DisplacedByCustomXml:1, ColFirst:2, ColLast:3, Ed:4, EdGroup:5 }`
(`reference/sdkjs/word/Editor/Serialize2.js:716-722`), written by `WritePermPr` (`:5986-6014`) and
read by `readPermStart`/`readPermEnd`/`readPermPr` (`:1763-1798`). The marks are even first-class
history items (`historyitem_type_ParagraphPermStart = 70<<16`,
`reference/sdkjs/common/HistoryCommon.js:1511-1512`).
Ranges are enforced through `CDocument.prototype.IsPermRangeEditing`
(`reference/sdkjs/word/Editor/Document.js:13516-13665`), highlighted while painting
(`reference/sdkjs/word/Editor/Paragraph/draw/line-draw-state.js:134`), the caret is corrected
into them (`CorrectCursorToPermRanges`), empty ones are garbage-collected
(`perm-ranges-manager.js:118-127`), and the engine tells the toolbar which of
text/paragraph/insert is permitted at the caret through `Asc.RangePermProp`
(`reference/sdkjs/common/apiCommon.js:7738-7756`,
`CDocument.prototype.UpdateInterfaceRangePermPr` at `Document.js:12310-12325`) — which is a
clean seam worth imitating whatever we decide about ranges themselves.

Three limits, each sourced, and each a gap we could beat rather than copy:

1. **`w:ed`/`w:edGrp` are cosmetic.** `grep -rn "getEd()\|getEdGrp()" reference/sdkjs` returns
   **four hits, all in the serialiser**. The manager says so itself
   (`perm-ranges-manager.js:57-71`), in a comment reading
   `TODO: Пока мы просто проверяем само наличие диапазона, в будущем надо проверяеть пользователя`
   — for now we only check the range exists; in future we must check the user. **Net: a `permStart` range is editable by every user**, so Word's
   "these people may edit this" becomes "anyone may edit this".
2. **Ranges apply only under `OnlyComments` and `View`**, never under `OnlyForms`
   (`IsPermRangeEditing` returns early unless `isRestrictionComments() || isRestrictionView()`).
3. **There is no UI to create one.** A search of their document-editor tree for
   `AllowEditRanges`, `rangePermission` and `CPermRange` returns zero; `ProtectRangesDlg`,
   `ProtectedRangesManagerDlg` and `ProtectedRangesEditDlg` exist only under
   `spreadsheeteditor`. Their own staff confirm it publicly: *"At the moment, ONLYOFFICE
   Document Editor supports only pre-existing allowed ranges for editing"*
   (<https://community.onlyoffice.com/t/make-portion-of-document-read-only/4523>, 2025-01-09 —
   forum, not documentation).

**Google has nothing**, and the negative is clean: range protection exists in **Sheets** —
`Data ▸ Protect sheets and ranges`, a first-class `ProtectedRange` API resource with an
`editors` list and a `warningOnly` flag, and a non-blocking mode whose own label is
*"Show a warning when editing this range"* which *"doesn't block people from editing, but
they'll see a message asking them to confirm"*
(<https://support.google.com/docs/answer/1218656>,
<https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/sheets>) —
and **no analogue in Docs**. Google built the feature, documented it, gave it an API resource,
and declined to build it for documents.

That `warningOnly` flag is worth sitting with: the company with the most data about what
people do with range protection shipped an explicitly **non-blocking** variant of it.

**And the cleanest evidence that document range permissions are half-built is an asymmetry
inside ONLYOFFICE's own codebase.** Their *spreadsheet* engine has **two** range mechanisms,
both genuinely enforced, both with real password checks and a full authoring UI:
`CProtectedRange` (`reference/sdkjs/cell/model/WorkbookProtection.js:893`) with
`asc_setPassword` / **`asc_checkPassword`** / `asc_getHashValue` / `asc_getSaltValue` /
`asc_getSpinCount` (`:1220-1299`) — Excel's "Allow Users to Edit Ranges" — and
`CUserProtectedRange` (`reference/sdkjs/cell/model/protectRange.js:40`), the Excel-365-style
per-user/per-group variant with `type = notView | view | edit` (`:49-50`), driven by
`ProtectRangesDlg` / `ProtectedRangesManagerDlg` / `ProtectedRangesEditDlg`. The *document*
engine gets neither the per-user check nor any authoring UI. The same company, the same
release, two sibling editors: one feature finished twice over, the other left at the
serialiser.

### 5.3 Do we read or write it? **No — and the fidelity page says we do**

`grep -rnE "permStart|permEnd|PermStart|perm_start|RangePermission" --include="*.rs"
--include="*.mjs" --include="*.js" .` over this tree returns **exactly one hit**, and it is a
prose claim, not an implementation: `webapp/src/fidelity.js:74`, in the
"Document protection & forms" family, which states that

> "The other `w:edit` modes (`readOnly`, `comments`, `trackedChanges`) and the
> `w:permStart`/`w:permEnd` editable ranges are modeled and round-trip but are NOT enforced
> yet"

**Both halves of that sentence are false, in opposite directions**, which is precisely the
shape `SKILL` §9 rule 6 records as having happened before:

- **Overstatement.** `w:permStart`/`w:permEnd` are **not modelled and do not round-trip**.
  There is no model type, no importer arm, no exporter arm. They reach
  `Reporter::report_element` through `body.rs`'s `_ if self.in_document` arm
  (`crates/casual-doc-import/src/body.rs:4659`), so they are **reported as `Omitted` loss** —
  no silent loss — and then dropped.
- **Understatement.** `readOnly`, `comments` and `trackedChanges` **are** enforced, at the
  operation, by `crates/casual-doc-edit/src/protection.rs` since ADR-052, and
  `fidelity_data.test.mjs:152-156` pins the grades that keep the stale sentence alive.

---

## 6. Usefulness, honestly — the take

The owner asked for a view, so here it is, mode by mode, with the frequency signal named
where one exists.

**`forms` — the only mode with a real, distinct user job, and the only one worth investing
in.** The user is not an author being restrained; they are a recipient being helped. The
evidence that this is the live mode is behavioural, not editorial: ONLYOFFICE threads
`IsFillingFormMode` through **78** call sites against 14 for `CanEdit`, built it a **separate
application**, and moved its format forward twice (OFORM → DOCXF → PDF) while leaving the
other three modes untouched for two years. Word gave it its own section scoping
(`w:formProt`), its own `NoReset` semantics, and its own entry point on the Developer tab.
Nobody does that for a feature nobody reaches.

**`trackedChanges` — a real job, but Word's framing of it is the wrong one for a web editor.**
"All edits must be tracked" is genuinely wanted. But as a *document* property it is weak:
ONLYOFFICE does not enforce it at all, Google has no counterpart, and in Google's API the
same intent is expressed as a property of the **write request** (`writeMode: SUGGEST`). The
right primitive is a **grant** — "this participant may suggest, not edit" — which is per-user,
which is what people actually mean, and which we already have as
`Capabilities::suggester()`.

**`comments` — almost entirely subsumed by a Commenter role.** Word's `comments` mode and
Google's Commenter role describe the same end state, and the role is better: it is per-person,
it is visible before the document opens, and it does not require writing to the file. The only
thing the document mode adds is that it travels with the bytes to an offline reader — which
matters for us, because we are local-first, but it is a narrow case.

**`readOnly` — the most-reached-for and the least honest.** It is what people click when they
mean "do not let *them* change this", and it cannot express that: it binds everyone, including
the author, and the author's first discovery of this is that they cannot edit their own
document. Word's own documentation concedes the gap twice in one page ("anyone can click Stop
Protection", "doesn't prevent someone from making a new copy"). Google replaced it with
Viewer, which says the true thing. We should treat `readOnly` as an **import-fidelity
obligation** we honour faithfully, not as a feature we promote.

**Vestigial, named as such:** the **formatting restriction** axis (Word's own help route for
it is in Manage Styles, not even in the pane; ONLYOFFICE round-trips the flag and enforces it
nowhere, and `setFromInterface` does not even assign it —
`reference/sdkjs/word/Editor/DocumentProtection.js:157-166`); **Block Authors**, whose only
surviving article is the legacy Office.com URL, which required SharePoint Foundation 2010
Workspaces and is widely reported non-functional in co-authoring
(<https://learn.microsoft.com/en-us/answers/questions/5190861/protect-block-authors>); and
`w:writeProtection`, which ONLYOFFICE parses and then **throws away** — the assignment is
literally commented out at `reference/sdkjs/word/Editor/Serialize2.js:16572-16579`.

> **No vendor publishes usage statistics for these modes**, so the ranking above is argued
> from implementation investment and from documentation structure, not measured. That is
> weaker than a number and it is the strongest evidence available. The one measurement that
> would settle it for *our* users is named in §10.

### 6.1 So: should the feature be redefined around sharing roles?

**Yes — the user-facing feature should be roles, and the document restriction should become an
import-fidelity obligation that a role resolver consumes.** For a web-first, embeddable,
Apache-2.0 editor, Google's model is simply the better design, for four reasons that are
structural rather than aesthetic:

1. **It answers the question users ask.** "Let Ravi comment and nobody else edit" is
   expressible as roles and is *not* expressible as `w:documentProtection` at all.
2. **It is knowable before first paint**, so the chrome can be right the first time instead of
   being corrected after the parse. We already resolve the container grant from the URL for
   exactly this reason (`webapp/src/capabilities.mjs`).
3. **It is the only one of the two that can be a real boundary.** A grant is signed by the
   host and re-checked by the relay; a document flag cannot be, and §3's citations say so in
   every vendor's own words.
4. **We have already built it, twice** (§7.5) — and that is the actual problem: we built the
   better model and shipped the worse one's UI.

What that does **not** mean is dropping `w:documentProtection`. Word writes it, Word enforces
it, and a file that arrives restricted must be honoured and must round-trip — that is
`AGENTS.md`'s compatibility priority and it is non-negotiable. The change is of *status*: it
stops being the feature and becomes one input to the feature.

---

## 7. What we do today, measured from this tree

### 7.1 The engine is the strong half, and stronger than ONLYOFFICE on one mode

`crates/casual-doc-edit/src/protection.rs` (1,574 lines) decides `readOnly`, `comments` and
`trackedChanges` **at the operation**, with an exhaustive match over all **60**
`Operation` variants so a 61st cannot arrive exempt. `comments` and `trackedChanges` are
decided by **projection equality** with no heuristic and no tolerance, which is the only exact
way to separate commenting, suggesting and accepting when all three travel as
`Operation::UpdateReviewState`. `w:edit="forms"` is enforced in the facade
(`crates/casual-doc-wasm/src/lib.rs:14753-14812`) because it alone needs state the engine
cannot see. `exempt_from_protection` closes the one-way-door trap in both choke points, and
the grant is the outer gate — `casual_doc_edit::access::refuse_if_not_permitted` runs first,
and is deliberately *above* the form-field exemption so a read-only guest cannot fill a form
either. ONLYOFFICE's `trackedChanges` applies no restriction at all (§1.3), so this is a
place where we are ahead.

### 7.2 The UI is one modal with one of Word's two axes

`webapp/src/document_protection.mjs` (302 lines) mounts `#restrictEditingDialog`: a stacked
radio group of five — `No restriction` / `No changes (read only)` / `Tracked changes` /
`Comments` / `Filling in forms` — a checkbox `Apply this restriction now`, and a note. It is
reachable from three surfaces (`#reviewProtectBtn`, the Review menu's `menuGroup.protect`
band, the command palette), so `SKILL` §10's two-surface rule is met. It is well tested:
`webapp/tests/document_protection.test.mjs` holds eleven guards **with a recorded mutation
proof**, and `dialog-contract.spec.mjs:292-318` covers it in the browser.

There is **no formatting-restriction section**, by the module's own admission — "this dialog
does not offer the style whitelist" — and no exceptions section.

### 7.3 A protected document opens in the full editing chrome and says nothing

This is the presentation defect, and it is a wiring defect underneath.

`editingUnavailableReason` is the designated seam for "say why editing is unavailable", and it
reports **exactly one** condition (`crates/casual-doc-wasm/src/lib.rs:1272-1277`):

```rust
pub fn editing_unavailable_reason(&self) -> String {
    if self.layout.is_windowed() { windowed_not_available("Editing") } else { String::new() }
}
```

`w:documentProtection` is not plumbed into it. The chrome derives everything from that string:
`readOnlyReason = String(doc.editingUnavailableReason ?? "")` and
`openMode = readOnlyReason ? "viewing" : SESSION.openMode(HOST_MODE)`
(`webapp/src/main.js:3100,3117`), and the banners are driven only by the mode and that string
(`createReviewBanners`, `webapp/src/review_chrome.mjs:165-172`). So a file carrying
`w:edit="readOnly" w:enforcement="1"`:

- opens in **Editing** mode, with the full ribbon enabled;
- shows **no banner** and no status line;
- announces its state only as `aria-pressed` on an unlabelled `lock` glyph
  (`webapp/editor.html:1082`; the whole Review band is icon-only, so that part is the band's
  convention rather than this control's defect);
- and tells the reader what is going on only **after** they type, as a toast
  (`document.protectedReadOnly` and siblings, `webapp/src/en_strings.mjs:1230-1233`).

Word keeps a pane open. ONLYOFFICE shows a tip per mode and re-shows it when a co-author
changes protection (§2.2). We are the only one of the three that stays silent.

The module records one deliberate omission in the same family: a `trackedChanges`-protected
document "does not force the editor into Suggesting mode on open, as Word does." Combined
with §1.1's point 2, the consequence is concrete — **the Track Changes button stays live on a
`trackedChanges`-protected document.** `review.mode.suggesting` (`webapp/src/main.js:7903`)
consults no protection, so the reader can turn suggesting off and arrive in a state where
every keystroke is refused. That is "never a dead control" inverted: an enabled control whose
only effect is to make the editor dead.

### 7.4 One attribute, two opposite silent misreadings — the sharpest finding

`crates/casual-doc-import/src/settings.rs:266-272` reads three attributes and
`attr_flag` (`:319-324`) is true only for `"1" | "true" | "on"`, so **absent means false.**
`crates/casual-doc-export/src/semantic.rs:3393-3404` writes `w:enforcement="1"` when the flag
is set and **omits the attribute** when it is not. `protection.rs:140-144` then early-returns
`Ok(())` on `!protection.enforcement`, reasoning that "Word writes a restriction with
`w:enforcement="0"` when the author set one up and then turned it off."

That reasoning is right about `="0"` and wrong about *absent*, and Microsoft's own implementer
notes say so:

> "The standard states that if the `enforcement` attribute is omitted, then protection
> settings are ignored by application. — **Word enforces protection when this attribute is
> missing.**"
> — MS-OI29500 Part 1 §17.15.1.29,
> <https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/e12d2773-e2d0-45bf-b1c2-b8f0548e156d>

So the same four lines produce two opposite silent failures:

- **On import:** `<w:documentProtection w:edit="readOnly"/>` — a document Word treats as
  protected — opens **fully editable** here, with no banner and no finding.
- **On export:** a restriction the author deliberately switched off (`enforcement: false`) is
  written without the attribute, and **Word then enforces it.** We silently *tighten* someone
  else's document, which is a document-safety defect, not a fidelity one.

The same deviation note gives three more rules a reimplementation needs: Word **prepends** the
salt; Word does not count the initial hash in `spinCount`; and Word prepends the **binary**
`saltValue`, not its base64 text.

### 7.5 We ship three overlapping authorities, and nothing composes them

| Authority | Source | Vocabulary | User-facing surface |
| --- | --- | --- | --- |
| Container grant — `webapp/src/capabilities.mjs` | host, from the URL, before first paint | roles `preview` / `readonly` / `commentor` / `edit` / `owner`; capabilities `open`, `save`, `download`, `print`, `edit`, `comment` | **none** |
| Participant grant — `webapp/src/session_access.mjs` (618 lines), `docs/152` §10 Q4/Q5, enforced by `crates/casual-doc-edit/src/access.rs` | host-signed token, verified at the boundary | `comment`, `suggest`, `review`, `edit`, `manageProtection`; presets `viewer()`…`owner()` | **none** |
| Document policy — `w:documentProtection` | the file | `readOnly`, `comments`, `trackedChanges`, `forms` | **the Restrict Editing modal** |

**The two Google-shaped authorities have no UI and the Word-shaped one has all of it.**
`grep -rniE "\"(share|invite)" webapp/src/en_strings.mjs` returns nothing; there is no share
dialog, no role picker, no invite surface. Both grants arrive from the host on the URL.

And nothing resolves the three. `editsBlocked` is
`reviewMode === "viewing" || !!readOnlyReason` (`webapp/src/main.js:5696`) — document
protection is absent from it. ONLYOFFICE, by contrast, has exactly one resolver, 17 lines
long, and it always narrows toward `View`
(`reference/web-apps/apps/documenteditor/main/app/controller/DocProtection.js:290-307`):
`ReadOnly → View` unconditionally; `Comments → OnlyComments` if `canComments` **else `View`**;
`Forms → OnlyForms` if `canFillForms` **else `View`**; `None` or `TrackedChanges` → fall back
entirely to host permissions. **That function is the answer to "is having both coherent?"** —
it is coherent if and only if there is one place that composes them and it can only ever
narrow.

The mode picker makes the incoherence visible to the user. We ship
`Editing / Suggesting / Read only` (`webapp/editor.html:3059-3061`) — a per-user, unenforced
preference — using **the same words** as the restriction dialog's levels, with no relationship
between the two controls. Two controls, one vocabulary, different meanings, and changing one
does not move the other.

### 7.6 Modelled, round-tripped, and consumed by nothing

Each of these is `SKILL` §9 rule 4 — *"modeled is not shipped"* — in the same feature family:

| Construct | State |
| --- | --- |
| `DocumentProtection::formatting` (`w:formatting`) | imported, exported, carried through the dialog untouched; the **only** consumer is the JSON getter at `crates/casual-doc-wasm/src/lib.rs:4231`. Never enforced; no UI. |
| `Style::locked` (`w:locked`) — doc comment: "Locked against use when document protection is active" | imported (`crates/casual-doc-import/src/styles.rs:531,642`), exported, **zero consumers** |
| `LatentStyles::default_locked_state` (`w:defLockedState`) | same |
| `WriteProtection` (`w:writeProtection`) — Word's "Always Open Read-Only" | imported (`w:recommended` only), exported, **zero consumers**; no UI. Its crypto group is dropped silently. |
| `DocumentSettings::track_changes` (`w:trackRevisions`) | imported, exported and, **since `109` HF-282, consumed**: a document that arrives with Track Changes on opens in Suggesting; the reader's switch between Editing and Suggesting writes it (`SetTrackRevisions`, one undoable edit), so a save carries it; under `trackedChanges` protection it may be turned on and never off (§17.15.1.29). Until then it had zero consumers, and a document that arrived with Track Changes on opened in Editing with tracking off — exactly the state `trackedChanges` protection depends on. **Correction, `109` FID-AT-11:** until then the importer read, and the writer wrote, `w:trackChanges` — an element in no schema — so the flag of a real Word document was never imported at all; `w:trackRevisions` is now read and written. |
| `w:sectPr/w:formProt` | **Modelled since `109` FID-AT-06**, as the side table `Definitions::form_protection` keyed by section id (read through `Definitions::section_form_protection`, which keeps absent, `false` and `true` apart), imported and written back in its `CT_SectPr` position. Until then it was reported-and-dropped, one of the two remaining findings in **every one of the eight real-producer corpus documents**, and Word's "protect only these sections for forms" was lost. **Still zero consumers:** the facade's forms check does not read it yet (M6). |
| `w:permStart` / `w:permEnd` | **not modelled.** Reported-and-dropped (§5.3). |
| `w:documentProtection` crypto group — `w:hash`, `w:salt`, `w:cryptProviderType`, `w:cryptAlgorithmClass`, `w:cryptAlgorithmType`, `w:cryptAlgorithmSid`, `w:cryptSpinCount`, `w:cryptProvider`, `w:algIdExt`, `w:algIdExtSource`, `w:cryptProviderTypeExt`, `w:cryptProviderTypeExtSource`, **plus the four Office-2010 ISO-verifier attributes `w:algorithmName`, `w:hashValue`, `w:saltValue`, `w:spinCount`** | dropped **with no finding at all** — see below |

**The crypto group is the one silent loss in the family, and the mechanism is exact.**
`apply_setting` returns `true` for `b"documentProtection"`
(`crates/casual-doc-import/src/settings.rs:266`), which marks the element *consumed*, so no
`Reporter` call is made for its unread attributes. `docs/160` §3 measured it in 1 of 19 real
documents and §7 item 5 queued it as "report at minimum"; ADR-052 records the same thing
derived from the code; it is still open.

Its consequence for a user is worse than "a password is lost", because of which save path
runs. The webapp imports with `retain_source = true`
(`crates/casual-doc-wasm/src/lib.rs:26140-26150`), so an **untouched** save takes
`ExportMode::ExactIfUnchanged` and returns the original bytes — the password survives. But
`word/settings.xml` is **always regenerated from the model**, unconditionally, in both other
modes (`crates/casual-doc-export/src/semantic.rs:680-687`), and retained parts are *extra*
opaque parts appended at `:1047`, never an override for it. So:

> **Open a password-protected document, type one character, save: the password is gone, the
> restriction remains, and nothing anywhere says so.** The recipient opens it in Word, clicks
> Stop Protection with no password, and edits freely. Worse, because of §7.4 the restriction
> may now be enforced in Word when the author had switched it off.

The four Office-2010 attributes widen the gap beyond what ADR-052 and `docs/160` record: both
enumerate the legacy five or seven, and Office writes the ISO verifier form when
`UseIsoPasswordVerifier` is set, so a modern Word file's password material may be in
`w:hashValue`/`w:saltValue`/`w:spinCount`/`w:algorithmName` — none of which appears anywhere
in this tree.

### 7.7 Two stale claims in our own published prose

1. **`webapp/src/fidelity.js:74`** — false in both directions (§5.3). This is the family's
   entry on a page that `SKILL` §9 records as having lied twice, and
   `webapp/tests/fidelity_data.test.mjs:152-156` pins the grades that keep the sentence alive.
2. **`crates/casual-doc-wasm/src/lib.rs:4249-4252`**, the published rustdoc on
   `setDocumentProtection`: *"**Who may do this: the local reader, and nobody else is
   checked.** There is no participant grant yet (`152` §10 Q4)."* The grant exists —
   `Capabilities::manage_protection`, `may_manage_protection()`, and
   `exempt_from_protection(op, capabilities)` — and ADR-060 shipped it. A public API doc that
   understates an access check is the same defect class as overstating one.
3. **`docs/153` contradicts itself** on this capability: row at line 629 grades
   "Protect the document" as **Gap**, reason "no command id reaches it: `review.protectDocument`
   is absent from `COMMAND_CONTRACT`", while the row at line 637 grades
   "Restrict editing to view, comments, forms or tracked changes" as **Partial**, reached by
   `review.restrictEditing`. One capability, two rows, two grades, and the pessimistic one is
   quoted first.

---

## 8. Ranked defects — worst first

**MODEL defects first, because that is what the owner is naming and it is the expensive half.**
Each row names the competitive behaviour it violates and a one-line fix. No code.

### 8.1 Our MODEL is wrong

| # | Defect | Violates | One-line fix |
| --- | --- | --- | --- |
| M1 | `w:enforcement` **absent** is read as off, and written as absent when off — two opposite silent failures from four lines (§7.4) | MS-OI29500 §17.15.1.29: "Word enforces protection when this attribute is missing" | Model the attribute as three-state (`absent` / `"0"` / `"1"`), treat absent as **enforced** on import, and always write it explicitly on export. |
| M2 | The crypto group — all 16 attributes, including the four Office-2010 ISO-verifier ones — is dropped **with no finding**, so a password-protected document round-trips passwordless and silently after any edit (§7.6) | `AGENTS.md` no-silent-data-loss; `docs/160` §7 item 5 | Report the group as a finding at import, and surface it at save as a named compatibility entry — before any decision about verifying it. |
| M3 | `w:permStart`/`w:permEnd` are not modelled at all, while `fidelity.js` claims they are "modeled and round-trip" (§5.3) | Word's Exceptions; ONLYOFFICE models the full attribute set and enforces range presence | Fix the published sentence in the same change as the first typed model of the pair; the attribute set to type is in §5.1, and the parent list makes it a uniform-flow construct. |
| M4 | `w:edit="trackedChanges"` does not force tracking on, and the Track Changes control stays live, so the reader can create a state where every keystroke is refused (§7.3) | `ST_DocProtect`: "applications **shall not** allow that element's state to be changed to false"; Microsoft: "you can't turn off change tracking" | Make the protection level an input to the mode resolver and pin Suggesting while it is in force, with the unprotect path exempt. |
| M5 | **No resolver composes the three authorities** — container grant, participant grant, document policy — and `editsBlocked` ignores protection entirely (§7.5) | ONLYOFFICE has exactly one, 17 lines, always narrowing (`DocProtection.js:290-307`) | One function: `(containerGrant, participantGrant, documentProtection) → effective mode + reason`, monotone in the narrowing direction, with every surface reading only its result. |
| M6 | `w:sectPr/w:formProt` was not modelled, so Word's per-section forms protection was lost — a finding in **8 of 8** real-producer corpus documents (§7.6). **The model half is done (`109` FID-AT-06):** `Definitions::section_form_protection(section)`; the facade half is not | `ST_DocProtect` `forms` is defined *in terms of* `formProt`; `Section.ProtectedForForms` | Make the facade's forms check consult `section_form_protection` rather than treating `forms` as document-global: a section is editable under forms protection exactly when it states `false`. |
| M7 | `w:formatting`, `w:locked`, `w:defLockedState`, `w:writeProtection` and `w:trackRevisions` are each modelled, round-tripped and consumed by **nothing** (§7.6). **`w:trackRevisions` is decided — enforced — since `109` HF-282; the other four remain** | `SKILL` §9 rule 4 | Decide each explicitly — enforce it, or record in the model's own doc comment that it is retention-only — rather than leaving five constructs that look done. |
| M8 | `ProtectionRefusal::ReadOnly` says "nothing may change", but `readOnly` normatively permits range-permission regions (§1.1) | `ST_DocProtect` `readOnly` | Treat the current sentence as provisional and tie it to M3; the refusal text has to change when ranges land, so say so where the enum is defined. |

### 8.2 Our UI is wrong

| # | Defect | Violates | One-line fix |
| --- | --- | --- | --- |
| U1 | A protected document opens in the **full editing chrome** with no banner; the reader learns the state only from a toast after typing (§7.3) | Word's pane stays open; ONLYOFFICE shows one tip per mode and re-shows it when a co-author changes protection | Plumb protection into `editingUnavailableReason`'s role — one persistent sentence per mode, present from the first paint. |
| U2 | **Two user-facing strings explain our feature by naming Microsoft Word**, translated into all 19 locales: `protect.enforce.hint` ("Word's \"Start enforcing protection\"…") and `protect.authority` ("…exactly as in Word when no password was set") | no product explains itself by a competitor's button name; a class defect — `crossRefDialog.aboveBelowNotForThis` and `toc.showLevels.title` do it too | Rewrite the four strings to say what *this* product does; keep the Word rationale in the code comment, where it already is and belongs. |
| U3 | Word's **two axes** are presented as one: the dialog is called "Restrict editing" and offers only the editing axis, while silently carrying `w:formatting` through | Word's pane has `1. Formatting restrictions` above `2. Editing restrictions` | Either name the dialog for what it restricts, or add the formatting axis — but stop shipping a pane that is half of a pane with the whole pane's name. |
| U4 | No **exceptions** UI, and no reader-side region affordance | Word: `Find Next Region I Can Edit`, `Show All Regions I Can Edit`, `Highlight the regions I can edit`. (ONLYOFFICE has none either — this one is beatable.) | Gated on M3; when it lands, copy ONLYOFFICE's `RangePermProp` seam — the engine tells the chrome which of text/paragraph/insert is permitted at the caret. |
| U5 | **Forms mode has no chrome**: no field navigation, no fill status, no submit, no caret correction, on a document whose body is entirely locked (§4) | Word: `Find Next Region I Can Edit`; ONLYOFFICE: a whole separate forms app plus `Next Field`/`Previous Field`/`Submit` and `MoveToFillingForm` | A forms-mode chrome driven by the same resolver as M5 — next/previous field and a count — before any further protection-mode work. |
| U6 | The mode picker (`Editing / Suggesting / Read only`) and the restriction levels share a **vocabulary** and have no relationship | Google's picker is the only mode concept in its product; Word's pane owns the restriction and never duplicates its words in a mode switch | Make the picker a *view* of the resolver's output (M5) rather than an independent authority, and bound it by the effective ceiling. |
| U7 | **No sharing surface at all** — no share dialog, no role picker — while two role models exist and are reachable only from a URL (§7.5) | Google: roles are the whole feature, in Share; ONLYOFFICE: a 19-key host contract | §9's decision gates this; it is the largest single piece of user-facing work in the family. |
| U8 | Published prose is stale in three places: `fidelity.js:74`, the `setDocumentProtection` rustdoc, and `docs/153`'s two contradictory rows (§7.7) | `SKILL` §9 rules 1, 4 and 6 | Correct all three in whichever change touches this family first; re-derive the `fidelity_data` pins rather than trusting them. |

---

## 9. The recommendation, and the cost of each path

**Recommended: Path B.**

**Path A — keep Word's task pane as the feature.** Build the formatting axis, the exceptions
UI, `w:formProt`, the password shape, region navigation. *Cost:* the largest build in the
family; we would be the only web editor with Word's full pane, and ONLYOFFICE's own trajectory
(ranges honoured but never authorable for years) is evidence of how little pull that has.
*Risk:* it is the wrong object. Nothing in it can express "Ravi may comment", which is the
request we will keep receiving. **Not recommended.**

**Path B — roles are the feature; the document restriction is an input to it.** Three steps,
in order, and the first is small:

1. **M5 — the resolver.** One function composing container grant, participant grant and
   document policy into an effective mode plus a reason, monotone in the narrowing direction.
   This is ONLYOFFICE's `applyRestrictions` done properly, it is the smallest change in this
   document, and it unblocks U1, U6 and the whole of forms-mode chrome.
2. **U1 + U6 — presentation.** A persistent per-mode sentence, and the mode picker demoted to
   a view of the resolver. This is the owner's "presentation" complaint, and it is mostly
   deletion of independent authority rather than new surface.
3. **U7 — the sharing surface**, with the existing role presets. *Cost:* this is where the real
   work is, because roles without an invite/identity story are a picker with nothing to pick.
   `docs/152` §10 Q4 is deliberately unsigned, so a signature profile is a prerequisite
   decision, not a task.

*Cost of Path B overall:* lower than A, and it banks what we already built. *Risk:* roles mean
nothing standalone — on your own machine you are the only authority, as
`Capabilities::local()` already says — so the honest product is "roles in a room, document
policy everywhere", and the resolver has to make that read as one feature rather than two.

**Independent of the choice, M1 and M2 ship first.** They are document-safety defects — one
silently loosens an incoming restriction and silently tightens an outgoing one, the other
silently discards a security property — and neither depends on any decision above. M1 in
particular is four lines of reading and one attribute on write.

---

## 10. Open questions and the experiments that settle them

1. **Does Word honour `w:permStart` under `comments` as the spec says, and does a named `w:ed`
   actually gate it?** The spec says the range must "match the editing rights of the user
   account". *Experiment:* one `.docx`, `readOnly` + two ranges (`edGrp="everyone"` and
   `ed="someone@else.com"`), opened by two accounts.
2. **Does Google Docs drop `w:documentProtection` on import, and emit nothing on export?**
   **UNSOURCED** either way; Google documents only that it *strips encryption passwords*
   (<https://workspaceupdates.googleblog.com/2026/01/edit-password-protected-office-files-google-drive.html>).
   *Experiment:* four files (`readOnly`, `trackedChanges`, `forms`, and `readOnly` + `permStart`),
   uploaded, edited as Owner and as Commenter, exported, then `grep` `word/settings.xml` and
   `word/document.xml` for `documentProtection`, `permStart`, `writeProtection`; and check
   `files.get` for `contentRestrictions`. Twenty minutes, and it converts the most important
   remaining unknown into a measurement.
3. **Is a Google Commenter *forced* into Suggesting, or does the picker merely omit Editing?**
   **UNSOURCED.** This decides whether Google's role model enforces at the write or only at
   the chrome. *Experiment:* share as Commenter, record which picker entries exist, type one
   character, then repeat through the Docs API as the Commenter.
4. **Which modes do *our* users actually reach for?** No vendor publishes this and §6 is argued
   from implementation investment. *Experiment:* the host telemetry seam already exists
   (`hostSession.noteWrite`); one counter per level chosen in the dialog, and one per
   protected document opened, answers it in weeks — and it is the only evidence that could
   justify or refute Path A.
5. **The pane's exact Word strings** — the two numbered section headings, the `Settings…` link,
   the *Formatting Restrictions* dialog title, and the current spellings of the three non-`readOnly`
   dropdown items. **UNSOURCED from Microsoft.** *Experiment:* UIA dump of the pane in a current
   M365 build. Until then, do not quote them as Word's wording.
6. **Whether ONLYOFFICE's restricted-editing protection is anything more than hash-gated.**
   Their help never claims it is, and never disclaims it either. *Experiment:* protect a
   `.docx` with `Protect Document ▸ Comments` and no encryption password, unzip
   `word/settings.xml`, and look for `w:hash`/`w:salt`.

---

## 11. What this document deliberately does not do

It does not change code, and it does not add the eight model rows and eight UI rows above to
`docs/109` as implementation work. Each is a work item under `SKILL` §6a item 2 — *a verified
gap is a work item, not a deliverable* — and the owner asked to read the research before any
work starts. The two that should not wait on that reading are **M1** and **M2**: both are
silent, both are document-safety, and both are small.
