// THE MODAL ROSTER — every `.dialog-overlay` surface in the editor, with the
// real user route that reaches it.
//
// It lived inside `dialog-contract.spec.mjs` and three other guards needed it,
// which is how `dialog-fit.spec.mjs` came to cover 8 of the 24: its own comment
// said the roster "should grow to the rest as they are done", and a roster that
// has to be grown by hand in a second file does not grow. Keyboard shortcuts
// was overflowing its own box by 260px at 1280x720 and was not on the short
// list, so nothing was red (HF-265 D3).
//
// So the list is here, once, and every guard that asks a question OF EVERY
// DIALOG imports it:
//
//   * `dialog-contract.spec.mjs`     — dismissal, focus trap, focus return
//   * `dialog-fit.spec.mjs`          — the whole card is on a laptop's screen
//   * `phone-dialog-sheet.spec.mjs`  — on a phone every card is a full-width
//                                      bottom sheet, flush to the left edge
//
// `dialog-contract.spec.mjs` also asserts that every `aria-modal` element in
// `editor.html` appears here, so a new dialog cannot ship outside the roster —
// and now cannot ship outside the fit and phone guards either.
//
// `opener`, `restore` and `focus` are the dismissal contract's business; a guard
// that only needs the geometry uses `id`, `name` and `open()` and ignores them.
import {
  expect,
  gotoEditor,
  clickIntoFirstPage,
  MOD,
  openAppMenu,
  runAppMenuCommand,
  runFilePageCommand,
  stableBox,
} from "./fixtures.mjs";

/** "Focus must return to the editing surface" — as opposed to a named opener
 *  element. Which element actually holds it is `expectEditorFocused`'s business,
 *  not each modal's. */
export const EDITOR_SURFACE = Symbol("editor surface");

/** Every dialog EXCEPT the command palette: `cmdPalette` is `.cmd-overlay`, is
 *  top-anchored by design (as VS Code's, Google Docs' and Spotlight's are) and
 *  has no `.dialog-card`/`.dialog-body`, so a guard that measures a card's box
 *  must not be handed it. `dialog-contract.spec.mjs` wants it; the geometry
 *  guards want this list. */
export const DIALOGS = () => MODALS.filter((modal) => modal.id !== "cmdPalette");

async function openPalette(page) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
}

async function runFromPalette(page, query, label) {
  await openPalette(page);
  await page.locator("#cmdInput").fill(query);
  await page.locator(".cmd-item", { hasText: label }).first().click();
}

async function insertTwoByTwoTable(page) {
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.locator("#tabTable").click();
}

// Every modal surface, with the real user route that reaches it and the control
// that must hold focus once it is open. `opener` is the chrome focus has to
// come back to; null means the route leaves no focusable opener on screen (the
// palette runs a command and closes itself), in which case the contract only
// requires that focus lands somewhere inside the editor rather than on <body>.
export const MODALS = [
  {
    id: "propertiesPanel",
    name: "Document properties",
    opener: "#propertiesBtn",
    focus: "#propTitle",
    async open(page) {
      await gotoEditor(page);
      await page.locator("#propertiesBtn").click();
    },
  },
  {
    id: "settingsPanel",
    name: "Settings",
    // It was an anchored popover with its own Escape handler and its own
    // pointerdown light dismiss — hand-rolled dismissal is what this contract
    // exists to end — and it outgrew the window, so scrolling it scrolled the
    // document behind. The owner's report: "that panel doesn't make any sense
    // now .. see dialog instead". Google Docs' Preferences and Word's Options
    // are both modal dialogs; this is now one, and answers to the same rules
    // as every other one here.
    opener: "#settingsBtn",
    focus: '#themeSeg button[aria-checked="true"]',
    async open(page) {
      await gotoEditor(page);
      await page.locator("#settingsBtn").click();
    },
  },
  {
    id: "pageSetupMenu",
    name: "Page setup",
    opener: "#pageSetupBtn",
    focus: '#pageOrientationSeg button[aria-checked="true"]',
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator("#tabView").click();
      await page.locator("#pageSetupBtn").click();
    },
  },
  {
    id: "watermarkDialog",
    name: "Watermark",
    // Word keeps Watermark on a Design tab this product does not have, so it is
    // the fifth control in Layout ▸ Page Setup — where the rest of the section's
    // furniture already lives.
    //
    // No surviving opener, and that is deliberate rather than a gap: every button
    // wired from `LAYOUT_SURFACE` goes through `onButton`, which preventDefaults
    // mousedown precisely so the ribbon never takes the keyboard off the document.
    // So the requirement here is the one that actually holds for this route —
    // Escape puts the keyboard back on the editing surface, where it was.
    opener: null,
    restore: EDITOR_SURFACE,
    // The radio group is the dialog's first decision (No watermark / Text
    // watermark), and the checked one is where Word lands too. Named by state
    // rather than by id so the row does not assume which kind the fixture's
    // document has.
    focus: '#watermarkDialog input[name="watermarkKind"]:checked',
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="layout"]').click();
      await page.locator("#watermarkBtn").click();
    },
  },
  {
    id: "headerFooterSettingsDialog",
    name: "Header and footer settings",
    // ONLYOFFICE's grouping of the band distances, the two running-content
    // switches and the section's page numbering
    // (`apps/documenteditor/main/app/view/HeaderFooterTab.js` L63-87). It goes
    // through the shared `registerModal`, so it answers this contract by
    // construction; it is in the table because the coverage test below is what
    // makes that a fact rather than an intention.
    //
    // No surviving opener, for the same reason as Watermark and Drop cap: the
    // button is wired from a surface table through `onButton`, which
    // preventDefaults mousedown so the ribbon never takes the keyboard off the
    // document. The requirement that actually holds for this route is that Escape
    // puts the keyboard back where it was.
    opener: null,
    restore: EDITOR_SURFACE,
    // "Header from top" is the dialog's first field and the thing it is most often
    // opened for, which is where `initialFocus` puts the keyboard.
    focus: "#headerFromTop",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="insert"]').click();
      await page.locator("#headerFooterSettingsBtn").click();
    },
  },
  {
    id: "dropCapDialog",
    name: "Drop cap",
    // Shipped in #614 carrying `aria-modal="true"` but never added to this
    // table, so the coverage test below was the only thing that failed — and it
    // failed on `main` rather than on the PR, because `browser-smoke` stops at
    // the unit lane and the unit lane was already red on the `main.js` ratchet.
    // Nothing was wrong with the dialog itself: it goes through the shared
    // `registerModal`, so it answers the contract as soon as it is asked to.
    //
    // No surviving opener, for the same reason as Watermark above: the button is
    // bound from `INSERT_SURFACE` through `onButton`, which preventDefaults
    // mousedown precisely so the ribbon never takes the keyboard off the
    // document. So the requirement is that Escape puts it back on the editing
    // surface.
    opener: null,
    restore: EDITOR_SURFACE,
    // The mode radio group is the dialog's first decision (None / Dropped / In
    // margin) and is what `createDropCapDialog` asks for initial focus on. Named
    // by state rather than by id so the row does not assume which mode the
    // fixture's first paragraph carries.
    focus: '#dropCapDialog input[name="dropCapMode"]:checked',
    async open(page) {
      await gotoEditor(page);
      // A caret is a precondition, not a nicety: `open()` returns early when
      // there is no selection node, so without this the dialog never appears and
      // the row fails for the wrong reason.
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="insert"]').click();
      await page.locator("#insertDropCapBtn").click();
    },
  },
  {
    id: "proofLanguagesDialog",
    name: "Proofing languages",
    // The language-pack install surface (`docs/146` §3/§10, ADR-042 Increment B),
    // designed from Word's File ▸ Options ▸ Language table. Reached from the Review
    // band here; its other two surfaces are the Review menu's Proofing band and the
    // command palette (`105` UX-004 forbids a capability with only one).
    //
    // No surviving opener, for the same reason as Watermark, Drop cap and Insert
    // caption: the button is bound from the surface table through `onButton`, which
    // preventDefaults mousedown so the band never takes the keyboard off the
    // document. So the requirement is that Escape puts it back on the editing
    // surface.
    opener: null,
    restore: EDITOR_SURFACE,
    // The first enabled row action — Install for a language with a pack published,
    // and the dialog asks for focus there because installing is what it is for.
    // Named by state rather than by id: the rows are built on open from what is
    // actually installed, so no id is stable across a run that installed something.
    focus: "#proofLanguageRows button:not(:disabled)",
    async open(page) {
      await gotoEditor(page);
      await page.locator('[data-tab="review"]').click();
      await page.locator("#reviewProofLanguagesBtn").click();
    },
  },
  {
    id: "captionDialog",
    name: "Insert caption",
    // Same as Drop cap and Watermark: the ribbon button is bound through
    // `onButton`, which preventDefaults mousedown so the band never takes the
    // keyboard off the document, so there is no surviving opener to return to.
    opener: null,
    restore: EDITOR_SURFACE,
    // Word's dialog opens on the Caption box, and so does this one
    // (`CaptionDialog.js:341`, `getDefaultFocusableComponent`). It is also the
    // only field the author must fill, which is the other reason it goes first.
    focus: "#captionText",
    async open(page) {
      await gotoEditor(page);
      // `requires: "bodyCaret"` — no caret, no insertion point, and the button is
      // disabled. Without this the row would fail for the wrong reason.
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="references"]').click();
      await page.locator("#refCaptionBtn").click();
    },
  },
  {
    id: "tocDialog",
    name: "Table of contents",
    // EDITOR_SURFACE, like every other ribbon-opened dialog here: a band button
    // does not take focus on click, so the caret keeps it and typing is never
    // interrupted. Measured — before the click the active element is the
    // editable proxy, and after Escape it is the editable proxy again.
    opener: null,
    restore: EDITOR_SURFACE,
    // Word's Table of Contents dialog opens on the levels control: the number of
    // heading levels is the only decision the author makes before inserting, and
    // everything the preview shows is derived from it.
    focus: "#tocLevels",
    async open(page) {
      await gotoEditor(page);
      // `requires: "bodyCaret"` — the band control is disabled without an
      // insertion point, so without this the row fails for the wrong reason.
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="references"]').click();
      await page.locator("#refTocBtn").click();
    },
  },
  {
    id: "tocUpdateDialog",
    name: "Update table of contents",
    opener: null,
    restore: EDITOR_SURFACE,
    // The mode radios ARE the dialog — Word's Update Table of Contents is two
    // radios and a pair of buttons — so focus lands on the checked one.
    focus: '#tocUpdateForm input[name="tocUpdateMode"]:checked',
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="references"]').click();
      // Update is disabled until a contents field exists, so insert one first.
      // The dialog under test is the UPDATE dialog, which is why this leaves
      // the insert dialog by its primary action rather than by Escape.
      await page.locator("#refTocBtn").click();
      await page.locator('button[form="tocForm"][type="submit"]').click();
      await expect(page.locator("#tocDialog")).toBeHidden();
      await page.locator('[data-tab="references"]').click();
      await page.locator("#refUpdateFieldsBtn").click();
    },
  },
  {
    id: "crossRefDialog",
    name: "Cross-reference",
    opener: null,
    restore: EDITOR_SURFACE,
    // The reference TYPE is the first decision: everything else in the dialog —
    // the option list, the target list, the list's own heading — is derived from
    // it, which is why Word and ONLYOFFICE both open on it
    // (`CrossReferenceDialog.js:_setDefaults`).
    focus: "#crossRefType",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="references"]').click();
      await page.locator("#refCrossRefBtn").click();
    },
  },
  {
    id: "restrictEditingDialog",
    name: "Restrict editing",
    // Word's Review ▸ Protect ▸ Restrict Editing, and the only surface in this
    // product that can LIFT a restriction a document arrived with — which is why
    // it had to exist at all (ADR-059). It shipped in #732 without joining this
    // table, so the last test in this file went red on `main`: that test is the
    // loop-closer, and it did its job.
    //
    // No surviving opener, the Watermark/Drop cap/Proofing-languages shape: the
    // Review band's button is bound from `COMMAND_CONTRACT` through `onButton`,
    // which preventDefaults mousedown precisely so the band never takes the
    // keyboard off the document. So the requirement is the one that actually
    // holds for this route — Escape puts the keyboard back on the editing
    // surface, not on <body> (HF-062).
    opener: null,
    restore: EDITOR_SURFACE,
    // The level radios ARE the dialog, so focus lands on the one in force.
    // Named by STATE rather than by id, like the Watermark and Update-table rows:
    // which level is checked depends on what the document arrived carrying, and a
    // row that named `[data-protect-level="off"]` would assert the fixture rather
    // than the contract.
    focus: '#restrictEditingLevels button[aria-checked="true"]',
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="review"]').click();
      await page.locator("#reviewProtectBtn").click();
    },
  },
  {
    id: "sessionRightsDialog",
    name: "Manage access",
    // The rights surface for a shared session. It is reachable here because the
    // PARTICIPANT grant arrives on the URL — `session_access.mjs`'s documented
    // channel for a document opened without a live `Welcome` — so the editor is
    // this room's owner for the length of this test. Without that the button is
    // present and disabled, which is the standalone state and the right one:
    // outside a room there are no other people's permissions to change.
    //
    // The membership list is EMPTY, because no relay answered, and the dialog
    // opens anyway and says so. That is the behaviour under test as much as the
    // modal contract is: a room of one is a real and common state — one doc, one
    // room — and a control that refuses to open cannot tell the reader there is
    // nobody to change.
    opener: null,
    restore: EDITOR_SURFACE,
    // Close, because with no rows there is no role picker to land on — which is
    // what `initialFocus` falls back to, deliberately, rather than focusing a
    // disabled Apply.
    focus: "#sessionRightsClose",
    async open(page) {
      await gotoEditor(page, "&granted=comment,edit,manageAccess,review,suggest&participant=0");
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="review"]').click();
      await page.locator("#reviewManageAccessBtn").click();
    },
  },
  {
    id: "aboutDialog",
    name: "About",
    // Opened from the File PAGE rather than the palette, because a durable
    // surface is the thing the product previously lacked entirely — there was no
    // About anywhere, so a bug report could not name its build. It was a Help
    // menu row; Help is a File-page group now, as it is in ONLYOFFICE.
    // No surviving opener: running a File-page row closes the page, so focus
    // goes back to the editing surface first and that is where Escape must
    // return it. A dialog that hands the keyboard to <body> is HF-062.
    opener: null,
    restore: EDITOR_SURFACE,
    focus: "#aboutClose",
    async open(page) {
      await gotoEditor(page);
      // Through the palette, not the File page: on the File page About is a
      // PANE now, not a dialog over it (the owner's "replace dialogs with this
      // space"), and a pane answers to `file-page-panes.spec.mjs` rather than
      // to the dialog contract. The dialog is still what every other surface
      // opens, and this row is about the dialog.
      await runFromPalette(page, "about", "About OpenDoc");
    },
  },
  {
    id: "shortcutsDialog",
    name: "Keyboard shortcuts",
    opener: null,
    // EDITOR_SURFACE, not the literal "#pages": focus is owned by the editable
    // proxy, not the page container (docs/105 UX-001). Naming the element would
    // make this row assert a mechanism rather than the guarantee, and it fails
    // for a reason that has nothing to do with this dialog.
    restore: EDITOR_SURFACE,
    focus: "#shortcutsClose",
    async open(page) {
      await gotoEditor(page);
      await runFromPalette(page, "keyboard shortcuts", "Keyboard shortcuts");
    },
  },
  {
    id: "splitCellDialog",
    name: "Split cell",
    opener: null,
    restore: EDITOR_SURFACE,
    focus: "#splitCellColumns",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await insertTwoByTwoTable(page);
      await page.locator("#splitCellBtn").click();
    },
  },
  {
    // Tab stops (`docs/148` §9 item 7). The ruler held the only calls to
    // `setTabStop` in the product, so this dialog is what lets the ruler be
    // withheld on a phone — and what makes tab stops a two-surface capability
    // at every other width, which they had never been.
    id: "tabStopsDialog",
    name: "Tab stops",
    opener: null,
    restore: EDITOR_SURFACE,
    focus: "#tabStopsPosition",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await runFromPalette(page, "tab stops", "Tab stops");
    },
  },
  {
    id: "styleNameDialog",
    name: "Create a style",
    opener: null,
    focus: "#styleNameInput",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.keyboard.press(`${MOD}+Home`);
      await page.keyboard.press("Shift+End");
      await runFromPalette(page, "Create style from selection", "Create style from selection");
    },
  },
  {
    // Version history's "Name this version" (`docs/139` §8.3, ADR-040). Same card
    // and the same `createNamePrompt` contract as Create a style, which is the
    // point: one implementation of "ask for one bounded line of text", so the
    // staged-result dance that keeps Escape from leaving the promise pending
    // exists once. Reached through the row's own ⋮ menu, because that is where
    // a version's actions live — naming acts on one version, so it belongs to
    // that version's row rather than to a bar that has to be told which row.
    id: "versionNameDialog",
    name: "Name this version",
    opener: null,
    focus: "#versionNameInput",
    async open(page) {
      await gotoEditor(page);
      await runFilePageCommand(page, "file.versionHistory");
      const row = page.locator("#versionPanelBody .version-item").first();
      await expect(row).toBeVisible();
      await row.locator(".version-item-menu").click();
      await page.locator('#versionRowMenu [data-command-id="version.name"]').click();
    },
  },
  {
    id: "bookmarkDialog",
    name: "Bookmark manager",
    opener: null,
    focus: "#bookmarkNameInput",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await runFromPalette(page, "Bookmark", "Bookmark…");
    },
  },
  {
    id: "fieldDialog",
    name: "Insert field",
    opener: null,
    focus: ".field-choice",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      // Through `runAppMenuCommand`, not by clicking the row directly. When the
      // long menus were folded into submenus, `insert.field` moved inside
      // `sub("menuGroup.fields", …)` and this entry went on clicking a row that
      // is no longer at the top level — all seven Insert-field cases below have
      // been timing out on a row the locator resolves but the reader cannot see.
      // The helper opens the flyout first, which is the click a reader makes,
      // and is a no-op for a row that never folded; every spec that names a
      // command by id should go through it rather than know which bands fold.
      await runAppMenuCommand(page, "insert", "insert.field");
    },
  },
  {
    id: "linkDialog",
    name: "Insert link",
    opener: null,
    focus: "#linkUrlInput",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.keyboard.press(`${MOD}+Home`);
      for (let i = 0; i < 4; i += 1) await page.keyboard.press("Shift+ArrowRight");
      await page.keyboard.press(`${MOD}+k`);
    },
  },
  {
    id: "altTextDialog",
    name: "Alt text",
    // The object context bar is rebuilt on every repaint, so the button that
    // opened this is a detached node by the time it closes.
    opener: null,
    restore: EDITOR_SURFACE,
    focus: "#altTextInput",
    async open(page) {
      await page.goto("/editor.html?fixture=float");
      await page.waitForFunction(
        () => {
          const s = document.getElementById("status");
          return s && s.textContent === "" && document.querySelectorAll(".page-wrap").length > 0;
        },
        null,
        { timeout: 45_000 },
      );
      const canvas = page.locator(".page-wrap .page").first();
      const box = await stableBox(canvas);
      await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
      await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
      await page.locator('.object-bar-btn[aria-label="Edit alt text"]').click();
    },
  },
  {
    id: "confirmDialog",
    name: "Discard confirmation",
    opener: null,
    focus: "#confirmCancel",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.keyboard.type("dirty");
      await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
      await page.locator("#file").setInputFiles("sample.docx");
    },
  },
  {
    id: "cmdPalette",
    name: "Command palette",
    opener: null,
    focus: "#cmdInput",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await openPalette(page);
    },
  },
];
