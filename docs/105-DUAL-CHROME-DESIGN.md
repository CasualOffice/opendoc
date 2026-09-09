# 105 — Dual chrome: Ribbon and Toolbar layouts

Status: **CANCELLED by the owner, 2026-09-10.** Nothing in this note is implemented,
and the second layout is not being built. Kept because the evidence in §2, the measured
position in §3 and the P0/P1/P3 backlog in §5 are true and useful independently of the
second shell — §5 explicitly says those three are worth doing either way, and P3 has
since shipped.

> "fuck it up .. stop .. we have our old UI.. keep that one .. just polish it and fix
> that issues . and for other Ui .. check our own editor casualoffice/docs"

**What the sibling editor turned out to have done.** `CasualOffice/docs` was read after
the cancellation. It **built the two-layout experiment and then deleted it**: a
`toolbarLayout="google-docs" | "classic"` prop was fully specced and fully implemented,
and today `toolbarLayout` has zero occurrences anywhere in that repo — the toolbar
renders unconditionally. What replaced it is a lighter and genuinely different idea:
not *which of two layouts*, but *which regions of the one layout are present* — a
`chrome` preset over a feature map, resolved into per-region visibility flags in one
file. That is one more entry for §2's list of vendors who did not keep two shells, and
it is the shape §5's P1 was already reaching for.

Its `ds-bundle/` is **not** a design system to adopt: no tokens, a one-line CSS import
of stock ProseMirror styles, placeholder screenshots. Its own `tokens.css` records that
the vendored `@schnsrw/design-system` was an empty placeholder and that the editor now
owns its token spine — which matches the owner's standing rejection of that package.

Superseded prototypes (`docs/prototypes/word-2026.html`, `docs/prototypes/docs-2026.html`)
were deleted with the direction.

Supersedes the open decision in [doc 64](64-EDITOR-TOOLBAR-RIBBON-DESIGN.md) §5, which
weighed *A. tabbed ribbon* against *B. rich single toolbar (Google-Docs model)* and
recommended A. The owner has since asked for **both**, selectable by configuration.
Doc 64 §5 should be amended to point here once this note is accepted; until then the
two documents disagree and this one is the newer input, not the ruling.

Sibling precedent: opencalc at `services/enclave/opencalc` — `docs/91` (Excel-shaped
ribbon), `docs/93` (Sheets-shaped toolbar), and the two prototypes beside them.
Note the path: opencalc is **not** at `services/sheets`.

---

## 0. What the owner asked for, in their words

> "i need UI .. to be like MS word 2026 UI.. same UI, same ribbon/flatten ribbon.
> and file and other menues and rest"

> "two Ui's is like MS word.. i need other like google docs"

> "i comment complete differnt UX, UI .. whicch is configuration"

> "so make it simialr to onlyoffice or libre office.. not exact copy .. and simialr
> like they have"

The last message is the operative one and it settles the fidelity question: **familiar
composition, own skin** — the level of resemblance OnlyOffice and LibreOffice ship,
not a pixel copy.

---

## 1. The two layouts

Named **Ribbon** and **Toolbar** in the product. Never "Word" and "Docs" — no chrome
region names another company's product. opencalc enforces this with a test
(`tests/browser/editor.branding.spec.mjs`); opendoc should adopt the same test.

**Ribbon** — tab strip; grouped band with captions and dialog launchers; contextual
tabs on selection; File backstage as a full-window route; task panes; mini toolbar on
selection; dense status bar with view switcher and zoom.

**Toolbar** — app bar with document name and save state; full menu bar; one flat
toolbar in a lifted container; ruler; outline pane; right-hand comment and suggestion
column; mode selector; minimal status strip.

Both draw from one command registry. Both use opendoc's own icon sprite and opendoc's
own palette — a blue-family accent for Ribbon, a light tinted canvas for Toolbar,
neither sampled from Microsoft or Google.

---

## 2. The evidence, including the part that argues against this

Recorded because it is the strongest counter-argument and it should not be lost:

- **No vendor has maintained two co-equal shells in-house and held parity.** The two
  long-lived exceptions (foobar2000/Columns UI, Winamp) made the second shell a
  third-party plugin against a UI-module API.
- **JetBrains**, retiring its Classic UI: *"Maintaining two different UIs places a
  significant load on our development and testing resources that we cannot continue to
  bear indefinitely."* **Overleaf**: *"any change we make to our platform has to be done
  multiple times."* **Atlassian**: *"Maintaining two issue views would have slowed us
  down."*
- **LibreOffice**, the closest editor precedent: 21 hand-authored `.ui` trees,
  ≈14.7 MB; 312 notebookbar bugs (77 open); meta-bugs with 397 dependents (123 open);
  keyboard access open since 2017; two variants frozen behind an experimental flag.
  Across Writer's six variants the command union is 537 and **only 45 commands appear
  in all six**.
- **Zoho Writer ran this exact experiment in this exact product category** — rejected
  the ribbon in 2016 for contextual modes, then **reversed in March 2026** to a
  familiar classic word-processor UI citing "almost zero onboarding time".
- **Overleaf is the counter-example that defines the correct sequence**: they killed
  the legacy editor, unified on one substrate (CodeMirror 6), and *then* sustainably
  ran two full editing surfaces. Unify first, fork the surface second.

The conclusion that follows: **P0 and P1 below are not preliminaries to the skins.
They are the bet.** If the registry does not feed every surface, a second shell is
LibreOffice's outcome rather than Overleaf's.

### Legal note

Microsoft's Office UI licensing program is royalty-free and perpetual but excludes
*"a program which directly competes with Word, Excel, PowerPoint, Outlook, or
Access"* — precisely this product. Microsoft asserted design patents, copyright in
the screen displays, and trade dress over the ribbon's appearance. Copying the
**layout** is what LibreOffice, OnlyOffice and WPS all do; copying exact palette,
icon artwork and branding is where the exposure sits. This is not legal advice, and
the owner's "similar, not exact copy" ruling already lands on the safe side.

---

## 3. Where opendoc actually stands

Measured, not estimated:

| | |
| --- | --- |
| `editor.html` | 110,658 B, **260 hand-written `<button>`, 405 ids** |
| `main.js` | 15,951 lines, 132 click listeners, **342 module-scope `getElementById`** |
| Command registry | ~118 declared ids; surfaces are **not** built from it — zero `data-cmd` attributes |
| Surface parity tables | `INSERT_SURFACE`, `REVIEW_SURFACE` only. Home, Table, View, File uncovered |
| e2e suite | **116 of 128 files bind to DOM selectors** |
| Keyboard | 34 `keydown` listeners, 10 global, **seven independent Escape handlers**, no ordered stack |

That is the OnlyOffice architecture (four forked front-ends, mobile at 18% of desktop
UI strings), not the CKEditor one (six shells for 2.2% of package source, because they
compose over one command layer). Moving from the first position to the second is P0
and P1, and it is worth doing whether or not a second shell ever ships.

**Coverage against a Word Home tab** — the honest number, since it decides how much of
a ribbon can be filled without dead controls: Clipboard 5/5, Font 14/15, Paragraph
9/13, Styles present, Editing 2/3. Roughly **30 of 38 controls, ~79%**. The genuine
gaps are multilevel lists, sort, text effects and a formatting-marks toggle.
References and Mailings are near-empty and those tabs should not exist.

---

## 4. Architecture

**One DOM, one node per command, two shell containers, exactly one populated at a
time. Placement is declared in per-layout tables; instantiation relocates existing
nodes rather than re-rendering them.**

Relocation, not a descriptor-renderer, because a renderer requires replacing all 342
module-scope `getElementById` bindings before anything ships — a large refactor with
no user-visible output and the highest regression surface available, and the item most
likely to be cut, which would leave two shells with a silently divergent command set.
If every node always exists somewhere in the document, every binding stays valid by
construction.

The load-bearing half of the descriptor idea is kept: **the layout table is the roster,
and an unplaced command must be exiled in writing** (`notInSurface: [{id, reason}]`).

Two axes, following opencalc `docs/93` §12.3: **mount** (web / native / embedded) and
**layout** (ribbon / toolbar) are independent. Do not fold layout into the chrome
enum — `chrome: native × layout: toolbar` must stay representable.

Switch: `data-skin` on `<html>`, read at module eval beside the other boot prefs;
persisted in the existing `opendoc.settings` blob alongside theme and accent — one key,
not a new storage name; `?layout=` allowlisted and session-scoped. Chooser lives at
**View ▸ Layout**, ticked from the stored choice rather than the rendered one, mirroring
the shipped `View ▸ Theme` submenu. Not a second control in Settings — two controls for
one setting drift.

---

## 5. Phases

Each is independently shippable and independently verifiable.

| Phase | Contents | Size |
| --- | --- | --- |
| **P0** | Invert the 11 sites where the command palette works by synthesising clicks on ribbon DOM. One `restoreChrome()` preserving scroll, selection and focus across any chrome mutation. Anchor the find panel to `--chrome-bottom`. | S |
| **P1** | Surface tables for Home, Table, View, File. Layout tables where every command is placed or exiled in writing. Close the four parity holes this exposes (paragraph styles, object commands, table commands, accept/reject-at-caret are each single-surface today). | M |
| **P2** | Toolbar layout end to end. | L |
| **P3** | Ribbon accessibility: name the 24 groups, roving focus, `Ctrl+F6` regions, **one ordered keyboard dispatcher**, then KeyTips. | M |
| **P4** | Ribbon shape: QAT, Layout tab, dialog launchers, Styles gallery, contextual tabs, mini toolbar, status bar with zoom. Seven independent slices. | L |
| **P5** | Backstage — the full-window File route. Mandatory for a ribbon to read as a ribbon. | L |
| **P6** | Ribbon review surface: markup balloons with connector lines and a Revisions pane. The existing review sidebar is already the Toolbar-layout model. | M |

P0, P1 and P3 are backlog work that make fidelity work cheaper — a new command stops
being able to reach one surface and miss another. P2 and P4–P6 are displacement.

The keyboard dispatcher in P3 is a hard prerequisite for KeyTips: arming a mode that
consumes bare letters on top of ten uncoordinated global listeners and seven unordered
Escape handlers manufactures exactly the defect class `docs/104` keeps recording.
`docs/67` row 33 already asks for this independently.

---

## 6. Owner decisions still open

1. **Is the second layout a permanent commitment?** It taxes every future control with
   a placement decision in two tables. Must be answered before P2; P0, P1 and P3 are
   worth doing either way.
2. **Which layout first?** Recommendation: **Toolbar**, because its primary surface
   (`#appMenuBar`) is the one chrome already rendered from descriptors, it needs no tab
   strip, no KeyTips and no backstage, and it doubles as the coarse-pointer layout.
3. **Does opencalc follow?** Its ribbon prototype uses Excel green `#107c41` directly.
   If the two products are to read as siblings under the "similar, not exact copy"
   ruling, that choice wants revisiting too. Out of scope for this repo but worth one
   conversation.

Settled by the owner already: **separate identities per layout, own icon sprite, own
palette, familiar composition** (message of 2026-09-10).

---

## 7. Risks

1. **Half a layout is worse than none.** Mitigation: default stays Ribbon; an
   unfinished Toolbar is invisible unless opted into; no command may be unplaced
   without a written reason.
2. **Two movers over one node set.** `updateRibbonOverflow` already relocates live
   groups; the layout placer would move the same nodes; `armTip` stashes `title` on a
   node that may move out from under it; focus can land on `<body>`. Mitigation: P0
   makes `restoreChrome()` the single mutation seam, proven on `setRibbonCollapsed`
   first.
3. **Pixel-pinned ribbon specs get relaxed instead of parameterised.** Several assert
   geometry that is false under the other layout. Relaxing them to go green stops them
   guarding the first layout. Mitigation: per-layout fixtures, unrelaxed, plus two
   invariants that generalise.
4. **A gate ships green while being wrong.** Has happened twice in this repo already.
   Mitigation: mandatory red-first — delete a layout row, a token, a manifest row, and
   record each gate failing before it lands.
5. **The 1280px budget moves rather than disappears.** Mitigation: both layout tables
   declare a natural width and a named fold ladder before any control is placed, and
   the active rung is written to the root element so a test asserts which rung fired
   rather than inferring it from `scrollWidth`.
