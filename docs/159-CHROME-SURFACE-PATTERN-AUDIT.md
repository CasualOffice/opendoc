# 159 — Chrome surface patterns: what decides where a control lives, and four places we decided wrong

**Status:** Audit, and a rule proposed as **ADR-062**. 2026-10-04.
**Opened:** 2026-10-04.
**Scope:** `webapp/` chrome only — the surfaces through which a command, a preference or a
selection property reaches the reader. Nothing in `crates/`.
**Evidence base:** this tree, plus `/Users/sachin/Desktop/melp/reference/sdkjs` at
`72b0421c0bbf9d01eed9cf14834ae47eb2df1b50` and
`/Users/sachin/Desktop/melp/reference/web-apps` at
`9c0ca538c3b211052347df09d2a4d6781f023403` — both "Merge branch release/v9.4.0 into
master", 2026-05-19. **AGPL-3.0: behaviour and structure only, no code taken.**
Every competitive claim below names a file and an identifier, and the load-bearing ones
were re-read directly rather than accepted from a report (`SKILL` §6). The one section
resting on unsourced recollection is **fenced and labelled as such** (§7.3).
**Occasioned by:** two defect reports from the owner — the Measurement-unit row opening
Settings with no measurement parameter in sight, and spell check and grammar check sharing
one icon.
**Implements nothing.** This document changes no behaviour. It records what the surfaces
are, what decides which one a control belongs on, and the four classes where that decision
was made by available space rather than by the kind of control. The fixes follow the rule,
not this audit.

> **What this concludes, before the evidence.**
>
> 1. **Both reported defects are real, and neither is the defect it looks like.** The
>    measurement control exists and is correctly placed; what is broken is the *reveal* —
>    two independent bugs, a lost focus race and a missing pane guard, and the dead
>    `.focus()` call proves nobody ever saw it work. §3. The spell/grammar pair does not
>    need a second icon: both are **as-you-type preferences wearing a command's clothes**,
>    and the labelled switches they duplicate are already in Settings. §4.
> 2. **The two proofing buttons are not merely similar — they are identical on screen.**
>    The borrowed `.table-command-badge` is `position: absolute`, and only the *table* band
>    gives a `.fmt` button a containing block, so the "G" is placed in the corner of the
>    whole ribbon body rather than on its button. The one intended differentiator has never
>    rendered where it was meant to, which makes the report literally true. There is also a
>    **third** switch in that group with no labelled control anywhere. §4.2, §4.3.
> 3. **One rule predicts all four classes.** A control's surface is a consequence of
>    **what the control is** — a command, a preference, or a property of the selection —
>    not of where there was room in the band. Every defect below is a control filed under
>    the wrong one of those three. §2, proposed as ADR-062.
> 4. **The same contextual bar is implemented five times, with five positioners.** `#selToolbar`,
>    the object context bar, the link chip, the paste-options bar and the review inline
>    card are one surface kind built five ways; three claim `role="dialog"` for an action
>    bar, two claim `role="toolbar"` and deliver none of the keyboard contract it promises,
>    and the object bar has no role at all. §5.
> 5. **Nothing owns the icon vocabulary, so neither uniqueness nor existence can be
>    checked.** 165 buttons in `editor.html` carry a Material Symbols ligature hand-written
>    at the call site; only 38 are bound to a command id by a registry. `chrome_fonts.test.mjs`
>    checks self-hosting and the `wOF2` magic and **nothing asserts a ligature the chrome
>    uses is actually in the subset** — a whole class of silently-blank icons. §6.
> 6. **The competitor agrees with the rule, and disagrees with our placement.** ONLYOFFICE
>    puts the measurement unit and the spelling preferences on the same File ▸ Advanced
>    Settings page, as a labelled dropdown and labelled checkboxes; it puts the
>    as-you-type spell toggle in the **status bar**; and it ships **no grammar check at
>    all**. §7.

---

## 1. The surfaces that exist

Derived by reading the modules that own them. "Registered by" is the seam a new control
goes through; where there is none, the control is hand-written at its call site, which is
the finding rather than the background.

| Surface | Owner module | Registered by |
| --- | --- | --- |
| Ribbon band / group | `webapp/src/ribbon_faces.mjs`, markup in `editor.html` | markup + a registry row binding `command` → `buttons` |
| Menu bar menu | `webapp/src/command_taxonomy.mjs` (`APP_MENU_SECTIONS`), `menu_render.mjs` | taxonomy entry |
| Compact toolbar | `webapp/src/compact_toolbar.mjs` | `COMPACT_TOOLBAR` data table → `editorCommands({surface:"compact"})` |
| File page pane | `webapp/src/file_pane.mjs` (`PANEL_PANES`) | `PANEL_PANES` entry naming an existing panel element |
| Modal dialog | `webapp/src/modal.mjs` | `registerModal(...)` |
| Popover | `webapp/src/popover_manager.mjs` | `registerPopover(...)` |
| Context menu | `webapp/src/object_context_menu.mjs` and siblings | menu descriptor |
| Command palette | `editorCommands()` at `webapp/src/main.js:11562` | registry row |
| Status bar | markup in `editor.html` | hand-written |
| Contextual action bar | **five separate implementations** — §5 | nothing |

Two of these are exemplary and worth naming, because the rule in §2 is largely a
generalisation of what they already do. `compact_toolbar.mjs` is **declared as data** and
derives every label, tooltip, shortcut, enablement, refusal reason and pressed state from
the single `editorCommands()` registry. `popover_manager.mjs` gives one-at-a-time
anchoring, outside-pointerdown dismissal, Escape and focus return to anything that
registers. The defects below are all in places that did not go through a seam like these.

---

## 2. The rule

> **A control's surface is a consequence of what the control is, not of where there was
> room. There are three kinds, and each has one home.**
>
> | Kind | What it is | Home |
> | --- | --- | --- |
> | **Command** | does something once, when invoked | ribbon / menu / palette / context menu — icon **and** label; reachable from ≥2 surfaces (`SKILL` §10) |
> | **Preference** | a persistent choice that outlives the selection | a **labelled** control in Settings, the state readable in the label. A fast path may exist as a **menu row with a checkmark** or a status-bar control — never an icon-only ribbon button |
> | **Selection property** | changes with what is selected | contextual bar / properties dialog |
>
> **Three corollaries.**
>
> **C1 — a pointer must point.** A control on one surface that defers to a control on
> another must *reveal* it: open or select the surface that owns it, scroll the control
> into view, and place focus on it **after** that surface has finished its own focus
> management. Opening the container is not revealing the control.
>
> **C2 — an icon is a name in a shared vocabulary.** Two different commands may not wear
> one ligature, and no command may name a ligature the bundled subset lacks. The same
> ligature on several surfaces for the **same** command is correct and required.
>
> **C3 — one job, one mechanism.** Where two surfaces do the same kind of job, they go
> through the same seam. A second implementation of one rule is evidence the abstraction
> is wrong (`SKILL` §8).

The rule's whole content is that the three kinds are *distinguishable before any code is
written*, and that the UI follows. An icon-only toggle is the diagnostic case: an icon can
depict an action, and `aria-pressed` can tell a screen reader a state, but an icon cannot
*show* a sighted reader that a preference is on. That is why a badge had to be invented in
§4, and why the badge is the symptom rather than the bug.

---

## 3. Class A — a reveal that does not reveal

**Reported as:** "Measurement unit: Centimeters opens Settings and there is no measurement
parameter."

The control exists and is correctly placed. `#measurementUnitSelect` is at
`webapp/editor.html:337`, inside `#settingsPanel`, in the fourth `.settings-section` —
after Theme and Language, before Accent, Reviewer identity and Autosave. The intent was
written down before the code: `webapp/src/command_taxonomy.mjs:116-121` says the row
"lands on the chooser inside Settings rather than being a second copy of it", and files it
in the same menu group as Settings. By ADR-062 that placement is right: a measurement unit
is a **preference**, so Settings owns it and the ribbon row is a pointer. The pointer is
what is broken, in two independent ways.

### 3.1 The focus is overwritten one microtask later

`webapp/src/main.js:10517-10520`:

```js
openChooser: () => {
  toggleSettings(true);
  document.getElementById("measurementUnitSelect")?.focus();
},
```

`toggleSettings(true)` opens the Settings modal, and `modal.mjs` **defers its own initial
focus by a microtask** — `webapp/src/modal.mjs:264-266` queues `focusFirst(entry)`. The
synchronous `.focus()` above therefore runs *first*, and the queued `focusFirst` then
sends focus wherever the modal's `initialFocus` says, which for Settings is the Theme
radio group. Focus ends at the top of a scrolling `.settings-body`.

`focusFirst` also uses `target?.focus({ preventScroll: true })` (`modal.mjs:86`), so even
winning the race would not bring the section into view. **The `.focus()` call in
`openChooser` is dead code** — it has never had an observable effect, which is the
strongest evidence available that this path was never watched working.

### 3.2 From the File page it would raise a half-dialog out of a pane

`file_pane.mjs` **moves** `#settingsPanel` into the File page: `PANEL_PANES` maps
`view.settings` to the `settingsPanel` element, and the module saves and restores its home
parent. The gear button guards against this, and says why, at
`webapp/src/main.js:15990-16001`:

```js
// The Settings DIALOG and the Settings PANE are the same element, and while
// the File page is open it is parented INSIDE the page — opening the dialog
// would raise a half-dialog out of a pane. One Settings, so the gear goes to
// wherever it currently lives.
if (showSettingsPane()) {
  document.querySelector('#filePageBody [data-file-pane="settings"]')?.focus();
  return;
}
toggleSettings();
```

`openChooser` has no such guard — and the File menu, where the measurement row lives, is
exactly where the reader is when they click it. One path documents the hazard; the other
ignores it.

### 3.3 What the rule prescribes

Corollary C1, and it needs no `main.js` edit: `showSettingsPane` is already exported
(`webapp/src/file_pane.mjs:242`), so the measurement module can own its own reveal —

- if `showSettingsPane()`, select the File-page pane and reveal there; otherwise open the
  dialog;
- focus in a **`requestAnimationFrame`**, not synchronously. Microtasks always drain
  before the next animation frame, so one rAF is *guaranteed* to land after
  `modal.mjs`'s queued `focusFirst`. This is an ordering guarantee, not a race won by
  being later;
- `scrollIntoView` the enclosing `.settings-section`, because `preventScroll: true` means
  focus alone reveals nothing.

**The class, not the case** (`SKILL`, *Fix the family*): every control that defers to a
control on another surface has this shape. The reveal belongs in one helper, used by all
of them, and the guard in §8 asserts the guarantee — *the named control is on screen and
focused* — rather than any of the three mechanisms above.

---

## 4. Class B — a preference rendered as an icon-only command

**Reported as:** "spell check and grammar check share an icon."

They do. `webapp/editor.html:1043-1044`, both in `.rgroup[data-group="proofing"]`, both
carrying the `spellcheck` ligature, the second distinguished only by a hand-added "G":

```html
<button … id="reviewSpellCheckBtn"   … aria-pressed="false"><span class="ms" aria-hidden="true">spellcheck</span></button>
<button … id="reviewGrammarCheckBtn" … aria-pressed="false"><span class="ms" aria-hidden="true">spellcheck</span><span class="table-command-badge" aria-hidden="true">G</span></button>
```

Four things follow, in increasing order of importance.

1. **The badge is borrowed from another domain.** `.table-command-badge` is defined once,
   at `webapp/src/style.css:4706`, and is used four times in `editor.html` — three on the
   table band's `delete-row` / `delete-column` / `delete-table` buttons (`:885-887`, badges
   "R", "C", "T", all on the `delete` ligature) and one on this proofing button. A class
   named for the table domain is doing general-purpose "distinguish these two buttons"
   work. That is a vocabulary leak, and it is the tell that the button ran out of ways to
   say what it was.
2. **The borrowed badge does not render on the button, so the two buttons are in fact
   identical on screen.** This is the finding that settles the report. The badge is
   `position: absolute; right: 2px; bottom: 1px` (`style.css:4706-4720`), and the only rule
   that makes a `.fmt` button its containing block is the band-scoped
   **`.table-ribbon .fmt`** rule setting `position: relative`
   (`style.css:4695-4699`) — scoped to the table band. Base `.fmt` sets no
   `position` at all, and the nearest positioned ancestor of a button in `#panelReview` is
   **`.ribbon-body`** (`position: relative`). So the "G" is placed 2px from the right and
   1px from the bottom of the **whole ribbon body**, not of its button. The class works in
   the one band it was written for and was moved to a second. The owner's report —
   "spell check and grammar check share an icon" — is therefore *literally* true: the only
   intended differentiator has never appeared on the control.
3. **All three are toggles, not commands — and the third has no labelled home at all.**
   Both carry `aria-pressed`, and the registry confirms it: `webapp/src/main.js:7904-7905`
   binds `tools.spellCheck` and `tools.grammarCheck` to `settings.spellCheck` /
   `settings.grammarCheck`, flipping a stored preference. Their own titles say so — "Check
   spelling **as you type**". An icon-only button cannot show a sighted reader that a
   preference is on, which is precisely why one of them needed a badge. There is also a
   **third** switch in the same group, `#reviewSmartQuotesBtn` (`editor.html:1045`,
   `format_quote`, `aria-pressed`), and `smartQuotes` occurs **nowhere else in
   `editor.html`** — so unlike its two neighbours it has no labelled control in Settings at
   all. The group is three preferences, not two, and they are inconsistent with each other.
4. **The labelled switches already exist, for two of the three.** `#spellCheckToggle`
   (`editor.html:382`) and `#grammarCheckToggle` (`editor.html:386`) are checkboxes in the
   Settings dialog, each with a `<strong>` name and a `<small>` explaining what it
   underlines. The ribbon pair is a second, **less** readable copy of a control that
   already reads correctly.

So the fix is not a second glyph, and it is not a working badge either — a badge that
rendered correctly would still be an icon-only control trying to express a state. By
ADR-062 all three are **preferences**: labelled switches in Settings are their home (which
means adding the missing third one), a menu row with a checkmark is the legitimate fast
path, and the one thing that is genuinely a **command** — run a proofing pass over the
document — is what deserves the `spellcheck` icon on the ribbon. That resolves the icon
collision without touching the font subset, and removes the only cross-domain use of
`.table-command-badge` along with the mispositioning that use suffers.

**This is a behaviour change to shipped controls, and it is deliberately not made here.**
It is recorded as the rule's prediction, to be implemented once ADR-062 is accepted.

---

## 5. Class C — one job, five mechanisms

Five surfaces are the same thing — a small action bar that appears next to something and
goes away — and they share no code:

| Bar | Markup / logic | Positioner | Role | Dismiss |
| --- | --- | --- | --- | --- |
| `#selToolbar` | `editor.html:2530-2546`, `main.js:13527-13588` | bespoke in `positionSelToolbar` | `toolbar` | none of its own |
| `.object-context-bar` | `webapp/src/object_bar.mjs` | `objectBarPosition` (`object_guides.mjs:190`) | **none** | none |
| `#linkChip` | `editor.html:2555-2566`, `main.js:5760-5786` | raw `event.clientX/Y` | `dialog` | hand-rolled |
| `#pasteOptions` | `editor.html:2548-2553`, `main.js:14080-14213` | bespoke, caret-anchored | `dialog` | hand-rolled |
| review inline card | `main.js:11003-11150` | bespoke, body-level | `dialog` | hand-rolled |

Read against ADR-062's C3, each row is a divergence from a seam that already exists.
`popover_position.mjs` provides anchored, flipped, clamped placement and is used by none
of them. `popover_manager.mjs` provides outside-pointerdown dismissal, Escape and focus
return, and is used by none of them for the bar itself — only for their child popovers.
`ribbon_nav.mjs` provides the WAI-ARIA roving-tabindex pattern and is wired to the ribbon
body alone, so the two bars that claim `role="toolbar"` **promise arrow-key navigation
they do not deliver**, and three bars claim `role="dialog"` for something that is not a
dialog in any respect — no modality, no focus trap, nothing to confirm.

The object context bar is furthest from the rule: it has **no `role` and no `aria-label`
at all**, and it is appended to `document.body`, which puts its buttons at the end of the
tab order.

**The same split runs through the popovers, where there *is* a shared seam.** Of the
`.context-menu` elements in `editor.html` — same class, same `registerPopover` mechanism —
**14 carry `role="menu"` and 11 carry `role="dialog"`**. One mechanism, one visual
treatment, two contradictory accessibility contracts, split almost down the middle. So C3
is not only about the five bars that bypassed the seam; a shared seam does not by itself
make a surface consistent if what each caller declares is left to taste. The seam has to
own the role.

**This rule is not in tension with ADR-061** (the Compare lane, landing in parallel). That
decision presents a comparison *in the document*, as tracked changes through the revision
model that already exists, rather than as a side panel reporting counts — and its panel is
an **index that navigates to changes** rather than a report about them. That is corollary
C3 reached independently on a different problem, and the panel-as-index is the
selection-property row of §2 behaving correctly. It is evidence for the rule, not an
exception to it.

The missing piece is one `createContextualBar({ anchorRect, labelKey, items })` composing
the four seams above. This section is an inventory and a direction, not a design: the
design is a separate lane, because collapsing five live surfaces is a behaviour change to
each of them.

---

## 6. Class D — the icon vocabulary is unowned

Derived from `editor.html` and `main.js` (script in §8.4):

- **165** buttons in `editor.html` carry a Material Symbols ligature, hand-written at the
  call site.
- **38** of them are bound to a command id by a `command:` / `buttons: () => [...]`
  registry row. The other 127 are bound by direct `addEventListener`.
- Among those 38, **three ligatures serve two distinct commands each**:

  | Ligature | Commands |
  | --- | --- |
  | `spellcheck` | `tools.spellCheck`, `tools.grammarCheck` |
  | `format_list_numbered` | `layout.lineNumbers`, `reference.updateCaptionNumbers` |
  | `format_indent_increase` | `paragraph.indent.increase`, `layout.indent` |

**Three is a floor, not a total**, because the method can only see 38 of 165 buttons. That
limitation *is* the finding: no registry owns the mapping from ligature to command, so
neither of C2's two halves is checkable.

Reading the markup by hand past the registry's reach confirms the floor is well below the
true figure. Each of these was read directly in `editor.html`:

| Ligature | Buttons, all different commands |
| --- | --- |
| `tune` | `#paraOptsBtn` `:536` (paragraph options), `#headerFooterSettingsBtn` `:739`, `#tablePropertiesBtn` `:911` — **three** |
| `delete` | `[data-table-action="delete-row"]` `:885`, `delete-column` `:886`, `delete-table` `:887`, `#reviewDeleteBtn` `:1030` (delete comment) — **four**, the first three separated only by the badge of §4 |
| `superscript` | `#superscript` `:500` (format), `#refFootnoteBtn` `:837` (insert footnote) |
| `wrap_text` | `#layoutWrapBtn` `:798` (object text wrap), `#viewReflowBtn` `:948` (reflow view) |
| `toc` | `#refTocBtn` `:826` (insert a table of contents) vs `#viewOutlineBtn` `:927` / `#railOutline` `:2629` (show the outline panel) |
| `language` | `#reviewProofLanguagesBtn` `:1053` (proofing language) vs `#languageStatus` `:3071` (UI language) |

Two cautions for the guard in §8.2, both of which a naive rule would get wrong. First, most
apparent duplicates are **correct**: `add_comment` on four buttons is one command on four
surfaces, which `SKILL` §10 requires, so forbidding repeated ligatures outright would be
wrong — the collision that matters is one ligature serving **different** commands. Second,
`close` appears on dozens of dialog- and panel-close buttons as a deliberate convention,
and `arrow_drop_down` on every caret; both are affordances rather than command icons, and a
guard must distinguish them from a command's name rather than exempting them by id.

There is an **inverse** defect the same guard will not catch, recorded here so it is not
lost: one command wearing two different icons. `review.previous` / `review.next` are
`navigate_before` / `navigate_next` on the Review band (`:1007-1008`) and `expand_less` /
`expand_more` in the review sidebar (`:2738-2739`). By C2 that is the same violation seen
from the other side — the vocabulary is unowned in both directions.

The existence half is unguarded too. `webapp/tests/chrome_fonts.test.mjs` asserts that
`fonts.css` self-hosts Inter and Material Symbols, that no `fonts.googleapis.com` or
`gstatic.com` reference exists, that both binaries are checked in with a `wOF2` magic
number, and that their licences are present. **Nothing asserts that a ligature the chrome
actually uses is present in the subset.** A ligature absent from
`assets/fonts/material-symbols-outlined.woff2` renders as nothing — no error, no fallback,
a blank button — and this is the same family as the `pilcrow` glyph question already
waiting on the owner.

---

## 7. The competitive standard, source-verified

### 7.1 Measurement unit — ONLYOFFICE

A non-editable `Common.UI.ComboBox`, `cmbUnit`, declared at
`reference/web-apps/apps/documenteditor/main/app/view/FileMenuPanels.js:747`, in the
`txtWorkspace` ("Workspace") group of the File ▸ **Advanced Settings** page
(`DE.Views.FileMenuPanels.Settings`, `el: '#panel-settings'`, line 296). Its row markup is
at `:436-439` (`#fms-cmb-unit`), its label is `strUnit: 'Unit of Measurement'` (`:1239`),
and it offers exactly three values — `txtCm`, `txtPt`, `txtInch` (`:1240-1241, 1249`). The
page commits with an **Apply** button (`okButtonText: 'Apply'`, `:1236`), not OK/Cancel.

This **corrects the in-tree comment** at `command_taxonomy.mjs:116-121`, which cites
`FileMenuPanels.js:747` correctly but was itself only a comment. The line is right; it is
now read from the source rather than promoted from a comment, and the group is
**Workspace**, not Proofing.

So: a preference, on a settings page, as a labelled dropdown. ADR-062 and the competitor
agree, and our placement already matches — only the reveal is broken.

### 7.2 Spelling and grammar — ONLYOFFICE

- **There is no grammar check.** A case-insensitive search for `grammar` across
  `reference/web-apps/apps/documenteditor`, `reference/web-apps/apps/common/main/lib`,
  `reference/sdkjs/word` and `reference/sdkjs/common` yields three hits, none of them a
  command, button, menu item or setting: two JSDoc mentions of the OOXML `lang` attribute
  (`sdkjs/word/apiBuilder.js:13015`, `:16782`) and one dead colour branch for an external
  annotator (`sdkjs/word/Drawing/Graphics.js:2169`, whose feed
  `CDocument.prototype.GetCustomMarks` returns `null` at
  `sdkjs/word/Editor/Document.js:28563-28566`). I confirmed the absence with my own
  search before relying on it.
- **The spelling preferences are labelled checkboxes on the Advanced Settings page**, in a
  `txtProofing` ("Proofing", `:1265`) group: `chSpell` with
  `labelText: this.txtSpellCheck` (`FileMenuPanels.js:534-536`), whose value is
  `txtSpellCheck: 'Spell Checking'` (`:1243`);
  plus `chIgnoreUppercase` (`:545`) and `chIgnoreNumbers` (`:553`).
  `chSpell` **disables its two dependents when it is off** — a relationship an icon-only
  toggle has no way to express.
- **The as-you-type toggle is in the status bar, not the ribbon.** The button is built by
  `Common.Views.ReviewChanges.getButton('spelling')` at
  `reference/web-apps/apps/common/main/lib/view/ReviewChanges.js:895-910` (`enableToggle: true`,
  `iconCls: 'toolbar__icon btn-ic-docspell'`) and rendered **only** into the status-bar
  slot `#btn-doc-spell` (`.../documenteditor/main/app/controller/Statusbar.js:112-113`;
  slot at `.../app/template/StatusBar.template:22`). Their Collaboration ribbon tab has
  no spelling slot, and `Toolbar.js` contains **zero** occurrences of `spell` — which I
  verified directly.
- Their icon asset lives in the **small** icon directory
  (`apps/common/main/resources/img/toolbar/v2/1x/btn-ic-docspell.png`), with no `big/`
  sibling — consistent with status-bar and menu use and never a large ribbon button.

Every element of ADR-062's preference row is present in the competitor: labelled checkbox
in settings as the home, a status-bar control as the fast path, no icon-only ribbon toggle,
and the one icon reserved for the one thing that is not a preference.

### 7.3 Word and Google Docs — **UNSOURCED**

> **The remainder of this subsection is recollection, not evidence.** No Word or Google
> Docs artifact is checked out in this repository, so nothing here was read from a source,
> and `docs/99` §9 exists because this project published unsourced competitive claims
> twice. It is recorded because the rule should be checked against more than one
> competitor, and it is **fenced so that nothing downstream can cite it as sourced**.
> Treat every sentence as a hypothesis to verify before it informs a decision.
>
> - Word's measurement unit is File ▸ Options ▸ Advanced ▸ Display, "Show measurements in
>   units of" — a dropdown on a settings page, consistent with §7.1.
> - Word combines spelling and grammar into **one** feature, reached as the Editor task
>   pane (Review ▸ Editor, F7); the older modal "Spelling and Grammar" stepper has largely
>   been superseded by it. Proofing *preferences* — check spelling as you type, mark
>   grammar errors, ignore words in UPPERCASE — live in File ▸ Options ▸ Proofing.
> - Google Docs likewise combines them in a "Spelling and grammar" side card
>   (Tools ▸ Spelling and grammar), with separate *as-you-type* toggles in the same
>   submenu.
>
> If that recollection holds, both competitors agree with §4's conclusion: **one** command
> for the check, and the as-you-type switches as labelled rows elsewhere. **Verifying it
> is an open item (§9), not a result.**

---

## 8. Guards proposed

Each must be driven red before it is trusted (`SKILL` §4), and each asserts a guarantee
rather than a mechanism (`SKILL`, *Guards assert the guarantee*).

### 8.1 A reveal reveals

For every control that defers to another surface — the measurement row first — assert the
**guarantee**: after invoking it, the named control is visible within its scroll container
and is `document.activeElement`. Run it twice, once with the File page closed and once
open, because §3.2 is a different bug from §3.1 and a single-state test would miss it.
Mutation proof: remove the rAF, and the focus assertion must fail; remove the
`showSettingsPane` branch, and the File-page case must fail.

### 8.2 No ligature serves two commands

Derive the ligature-to-command mapping and fail on any ligature bound to more than one
distinct command id. Must be built on the **runtime registry**, not a regex over
`editor.html`, so it sees all 165 buttons rather than 38 — which means an e2e spec reading
`editorCommands()`, or a unit test over an extracted registry. The three known collisions
in §6 are the arming evidence: the guard is only meaningful once it reports them, so it
lands **with** the fixes, and is written to permit one ligature on many surfaces for the
same command.

### 8.3 Every ligature the chrome uses exists in the subset

Parse the `cmap`/ligature table of `assets/fonts/material-symbols-outlined.woff2`, collect
every `<span class="ms">` ligature in `editor.html` and the modules that build buttons, and
fail on any name the subset lacks. This closes §6's existence half and gives the `pilcrow`
decision a mechanical answer. Feasibility is unverified — the woff2 is 320,052 bytes and
whether a parser is available in this toolchain has not been tested, so this guard is
**proposed, not promised**.

### 8.4 The derivation behind §6

Two different methods, and §6 keeps them apart on purpose, because they do not have the
same standing.

The **165 / 38 / three** figures come from a script: it maps each button id to its first
`ms` ligature, maps registry rows to button ids via the
`const x = document.getElementById("x")` bindings,
and groups ligatures by distinct command id. Every
collision it reports is backed by two *command ids*, so those three are the rigorous
figure — and the 38-of-165 reach is why they are a floor rather than a count.

The **six further collisions** in the table below them were read by hand from `editor.html`,
and each cited line and ligature was re-read directly before publication. They are offered
as evidence that the floor is well below the true figure, **not** as a count — a hand reading
cannot claim completeness, and saying "three" without them would understate, which `docs/99`
§9.6 records as being as false as overstating.

Counts here are derived, not hand-maintained (`SKILL` §8). When guard 8.2 lands it
supersedes both methods and becomes the single source for this section; §6's numbers should
then be re-derived from it rather than carried forward, which is the same arithmetic trap
`SKILL` §5a describes for the `main.js` ratchet.

---

## 9. What this does not do, and what is open

**Not done here, deliberately.** No behaviour changes. §4's collapse of two ribbon toggles
into a command plus labelled switches, and §5's single contextual bar, are both changes to
shipped controls and wait on ADR-062 being accepted. The three icon collisions in §6 are
recorded, not resolved.

**Open questions.**

1. **Verify §7.3 against a real source.** No Word or Docs artifact is checked out. Until
   one is, §4's competitive support rests on ONLYOFFICE alone — which is sufficient for
   the preference/command distinction but does not establish the *combined* spelling-and-
   grammar pass that §4 proposes as the single command.
2. **Does the proofing pass exist as a command to put on the ribbon?** §4 assumes a
   document-wide check can be invoked. `webapp/src/` carries ten proofing modules
   (`spell_check.mjs`, `grammar.mjs`, `proof_sdk.mjs`, `proof_worker.js` and others); which
   of them is reachable as a command, and whether a pass is O(document) and therefore owed
   the off-main-thread, cancellable, progress-reporting treatment (`SKILL` §8), is not
   established here.
3. **`format_list_numbered` and `format_indent_increase` need icons, and §6's floor needs
   lifting.** Both collisions pair genuinely different commands. Whether the replacements
   are in the current subset is exactly what 8.3 would answer, and the choice is the
   owner's where it touches the deliberate design tokens (`SKILL` §11).
4. **Does `.table-command-badge` survive at all?** If the §4 fix removes its only
   cross-domain use, the remaining three uses are all table commands and the class is
   correctly named again. Whether a badge is the right affordance for those three is not
   examined here — they are three `delete` buttons distinguished only by an "R", a "C" and
   a "T" at 7px, which the rule has something to say about even once the class is back in
   its own domain.
4a. **The mispositioned badge is a defect in its own right and wants a `docs/104` row.**
   §4.2 is a visible rendering bug independent of the rule: anyone adding a badge to a
   button outside `.table-ribbon` gets the same breakage, because the positioning contract
   lives in a band-scoped rule rather than with the badge. Whether the fix is to give
   `.fmt` a `position: relative` or to make the badge's own class establish its containing
   block is a styling decision that touches the deliberate tokens (`SKILL` §11), so it is
   the owner's. **It is deliberately not filed as a row here**, because `SKILL` §7 says a
   lane does not edit the trackers; it is surfaced for the coordinator instead.
4b. **`#reviewSmartQuotesBtn` has no labelled control anywhere.** Its two neighbours each
   have a Settings checkbox; it has none, and `smartQuotes` appears nowhere else in
   `editor.html`. Under §2 it needs one, which means the §4 fix is three switches rather
   than two. Whether smart quotes belongs with the proofing switches or with an
   autocorrect group — ONLYOFFICE puts its equivalent behind an `AutoCorrectDialog`
   (a modal `AdvancedSettingsWindow`) launched by `#fms-btn-auto-correct`
   (`FileMenuPanels.js:387`, `txtAutoCorrect: 'AutoCorrect options...'` at `:1267`) from
   that same settings page — is not decided here.
5. **Where does a status-bar fast path belong in our chrome?** §7.2 shows the competitor
   using the status bar for the spelling toggle. Our status bar is hand-written markup
   with no registration seam (§1), so adopting that pattern needs the seam first.
