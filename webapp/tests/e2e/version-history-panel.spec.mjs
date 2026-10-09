// The version history SURFACE, driven through the real application — HF-068 /
// `105` OO-004, `docs/139` (product) and `docs/140` (architecture).
//
// `version-history-store.spec.mjs` already drives the store against real
// IndexedDB. This file asks the question that one cannot: is any of it reachable,
// and does a person get the Google Docs interaction the design names? #653 landed
// the whole store with no interface at all, and "built but unreachable" is the
// most expensive recurring pattern in this repository (SKILL §9.4) — so these
// tests go through the File surface, the ribbon, the rail, the keyboard and the
// panel, never through the module.
//
// EVERY TEST CREATES ITS CONDITION. A panel test that passes against an empty
// store proves nothing: the timeline would be empty, every action absent, and the
// assertions would pass over a feature that does not work. So each test below
// first makes a version exist — by opening a document (the import baseline) and,
// where it needs two, by CHANGING the document and saving — and asserts on rows
// that are really there. The change is not decoration: a Save with nothing in it
// no longer lays down a version (`docs/139` §18 q3, reversed by the owner on
// 2026-09-28), so a test that saved an untouched document would now be asserting
// two rows against one and would have been asserting a duplicate before.
//
// The second round of this panel moved every per-version action out of a bar
// below the list and onto the row it acts on, and added the rail entry the owner
// asked for. Those two changes are what the tests from "the row's ⋮ menu" onward
// exist for: a ⋮ inside a list is only worth having if a keyboard and a screen
// reader can reach it, so the keyboard route is asserted for every action rather
// than assumed from the pointer route working.
import { clickIntoFirstPage, expect, expectEditorFocused, gotoEditor, moveCaretToDocStart, runFilePageCommand, saveDocument, stableBox, test } from "./fixtures.mjs";

const panel = "#versionPanel";
const list = "#versionPanelBody";
const rows = `${list} .version-item`;
const rowMenu = "#versionRowMenu";

/** Opens the timeline through the File surface — the primary entry point
 *  `docs/139` §8.1 names, and the one both Google Docs and ONLYOFFICE use. The
 *  helper asserts the row is present AND enabled, so an unreachable command fails
 *  loudly rather than quietly doing nothing. */
async function openTimeline(page) {
  // The command is a TOGGLE, which is what both entry points' `aria-pressed`
  // reflects — so running it on an already-open panel closes it. Idempotent here
  // so a test that saved while the panel was open does not close it by asking
  // for it again.
  if (!(await page.locator(panel).isVisible())) {
    await runFilePageCommand(page, "file.versionHistory");
  }
  await expect(page.locator(panel)).toBeVisible();
}

/** Waits for the document's import baseline to reach the timeline. The capture
 *  rides the open path and is deliberately not awaited by it — a timeline is not
 *  what a reader is waiting for — so the row arrives shortly after the document. */
async function waitForBaseline(page) {
  await expect(page.locator(rows)).toHaveCount(1);
}

/** A SECOND version, which now takes a real change.
 *
 *  `docs/139` §18 q3 originally said an explicit Save always lays down a version,
 *  identical bytes or not; the owner reversed that on 2026-09-28 — "version should
 *  not be logged if nothing has changed" — so a Save over an unmodified document
 *  reports `history.unchanged` and writes nothing. Every test below that needs two
 *  rows therefore CHANGES the document before saving, which is a better condition
 *  than the one it replaces: the row it then asserts on is a version of something
 *  that really happened rather than a duplicate of the head. */
async function saveAnEdit(page, marker) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(marker);
  await saveDocument(page);
}

/** The id of whatever really holds the keyboard, and the class of its cell —
 *  which is what a roving-tabindex grid publishes instead of
 *  `aria-activedescendant`. */
function focusedCell(page) {
  return page.evaluate(() => {
    const el = document.activeElement;
    if (!(el instanceof HTMLElement)) return { row: "", cell: "" };
    return {
      row: el.closest(".version-item")?.id ?? "",
      cell: el.classList.contains("version-item-menu") ? "menu" : "entry",
    };
  });
}

test("the timeline is reachable from File, from the View band AND from the rail", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // Surface one: File ▸ Version history.
  await openTimeline(page);
  await expect(page.locator("#versionPanelTitle")).toBeVisible();
  await page.locator("#versionPanelClose").click();
  await expect(page.locator(panel)).toBeHidden();

  // Surface two: the View band's panel toggle.
  await page.locator("#tabView").click();
  const view = page.locator("#viewVersionsBtn");
  await expect(view).toBeEnabled();
  await expect(view).toHaveAttribute("aria-pressed", "false");
  await view.click();
  await expect(page.locator(panel)).toBeVisible();
  await expect(view).toHaveAttribute("aria-pressed", "true");
  await view.click();
  await expect(page.locator(panel)).toBeHidden();
  await expect(view).toHaveAttribute("aria-pressed", "false");

  // Surface three: the rail, below Comments. The owner asked for it by name
  // ("i need version icon/shortcut on left bar below comments as well"), and the
  // rail is where this editor's panels live, so the assertion is both that it is
  // there and that it is LAST — a fourth entry above Outline would not be the
  // thing that was asked for.
  const railOrder = await page
    .locator(".rail .rail-btn")
    .evaluateAll((els) => els.map((el) => el.id));
  // `railCompare` is last, below Versions, and that order is the point: a
  // comparison is read BESIDE the document the way the outline and the comments
  // are, and it is the newest of the five. It is one face of `review.compare`
  // (`docs/153` `review.compare-documents`) — `compare.spec.mjs` drives it and the
  // Review band's button and asserts both open the same panel.
  expect(railOrder).toEqual([
    "railOutline",
    "railPages",
    "railReview",
    "railVersions",
    "railCompare",
  ]);

  const rail = page.locator("#railVersions");
  await expect(rail).toBeEnabled();
  await expect(rail).toHaveAttribute("aria-pressed", "false");
  // The same glyph the View band's button uses: two faces of ONE command must
  // not look like two commands.
  await expect(rail.locator(".ms")).toHaveText("history");
  await expect(view.locator(".ms")).toHaveText("history");

  // THE GESTURE, not the wiring: pressing it opens the panel and the button says
  // so. A rail button that ran a command without reflecting its state would be a
  // second door rather than a toggle.
  await rail.click();
  await expect(page.locator(panel)).toBeVisible();
  await expect(rail).toHaveAttribute("aria-pressed", "true");
  // And the OTHER entry point agrees, because one command has one state.
  await expect(view).toHaveAttribute("aria-pressed", "true");
  await rail.click();
  await expect(page.locator(panel)).toBeHidden();
  await expect(rail).toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});

test("the timeline lists this document's own past, and the panel is not the document", async ({
  page,
}) => {
  await gotoEditor(page);
  await openTimeline(page);
  await waitForBaseline(page);

  // The baseline: opening a file is the point it rejoins its own timeline
  // (`docs/140` §7.5), and the row says so in words rather than in a code.
  const first = page.locator(rows).first();
  await expect(first).toHaveAttribute("role", "row");
  await expect(first).toContainText("Opened");
  // The current version is marked as such, so a reader can tell where they are.
  await expect(first).toContainText("Current version");
  // The exact instant is present beside the friendly one — `docs/139` §14
  // requires it, and a relative label may never replace it.
  const exact = await first.locator("time").getAttribute("datetime");
  expect(exact, "the row carries no machine-readable timestamp").toMatch(
    /^\d{4}-\d{2}-\d{2}T/,
  );

  // A day heading, hidden from the accessibility tree because a heading is not a
  // permitted child of a grid and the day is already in every row's accessible
  // name. The group is a `rowgroup` for the same reason `group` is not one.
  const group = page.locator(`${list} .version-group`).first();
  await expect(group).toHaveAttribute("role", "rowgroup");
  const day = group.locator(".version-group-day");
  await expect(day).toHaveText("Today");
  await expect(day).toHaveAttribute("aria-hidden", "true");
  // The group carries no `aria-label` of its own, which is only defensible
  // because each row NAMES ITS OWN DATE — the full instant, not the friendly
  // "Today" a sighted reader sees above it. Asserted against the row's own
  // `title`, so this cannot pass by matching a substring of something else.
  const entry = first.locator(".version-item-entry");
  const stamp = await entry.getAttribute("title");
  expect(stamp, "the row has no full timestamp to name itself with").toMatch(/\d{4}/);
  expect(
    await entry.getAttribute("aria-label"),
    "the day heading is hidden from the accessibility tree and the row does not " +
      "carry the date either, so a screen reader is told when nothing",
  ).toContain(stamp);

  // The disclosure `docs/139` §12 asks for by name: how much history there is,
  // where it lives, and what the policy will do with it.
  await expect(page.locator("#versionPanelSummary")).toContainText("1 version kept");
  await expect(page.locator("#versionPanelDetail")).toContainText("in this browser");
  await expect(page.locator("#versionPanelPolicy")).toContainText("7 days");
});

test("the list IS the panel: no detached action bar, and no resting wall of disabled buttons", async ({
  page,
}) => {
  await gotoEditor(page);
  await openTimeline(page);
  await waitForBaseline(page);

  // The defect this panel shipped with, stated as a property rather than as a
  // list of five ids: with nothing selected, the only controls in the panel are
  // the ones that are ALWAYS meaningful — close, the filter, each row's own ⋮,
  // and Clear version history. A control that is disabled while resting is a
  // control the reader meets as a grey rectangle, and there were four of them.
  const resting = await page
    .locator(`${panel} button, ${panel} input`)
    .evaluateAll((els) =>
      els
        .filter((el) => el.getClientRects().length > 0 && el.disabled)
        .map((el) => el.id || el.className),
    );
  expect(
    resting,
    "these controls are on screen and disabled before the reader has done anything",
  ).toEqual([]);

  // And the timeline really is what fills the panel: the list is the tallest
  // thing in it by a wide margin. Measured rather than asserted from the markup,
  // because the complaint was about proportion, not about structure.
  const share = await page.evaluate(() => {
    const p = document.getElementById("versionPanel").getBoundingClientRect().height;
    const l = document.getElementById("versionPanelBody").getBoundingClientRect().height;
    return l / p;
  });
  expect(share, "the list is not the panel; something else is").toBeGreaterThan(0.5);
});

test("saving lays down a version, and the timeline is metadata only", async ({ page }) => {
  await gotoEditor(page);
  await openTimeline(page);
  await waitForBaseline(page);

  // A Save lays down a version when there is something to keep. It is NOT
  // unconditional: `docs/139` §18 q3 said it was, the owner reversed that on
  // 2026-09-28, and the Save with nothing in it has its own test below.
  await saveAnEdit(page, "VHSAVE");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);
  // Newest first, which is the Docs order and the only one a timeline can have.
  await expect(page.locator(rows).first()).toContainText("Saved");
  await expect(page.locator(rows).last()).toContainText("Opened");
  await expect(page.locator("#versionPanelSummary")).toContainText("2 versions kept");
});

test("a Save with nothing in it does not lay down a version", async ({ page, consoleErrors }) => {
  // THE OWNER'S DEFECT, and the condition has to be created or the test proves
  // nothing: open a document (which captures the import baseline), then save it
  // WITHOUT touching it, twice. A guard that saved once over a document that was
  // never opened would have no head to be identical to and would pass whatever the
  // code did.
  await gotoEditor(page);
  await openTimeline(page);
  await waitForBaseline(page);
  const opened = await page.locator(rows).first().getAttribute("data-version-id");

  await saveDocument(page);
  await openTimeline(page);
  // Still ONE row, and it is the same row: nothing was appended and the head did
  // not move.
  await expect(page.locator(rows)).toHaveCount(1);
  await expect(page.locator(rows).first()).toHaveAttribute("data-version-id", opened);
  await expect(page.locator("#versionPanelSummary")).toContainText("1 version kept");

  // Twice, because a fix that suppressed only the FIRST duplicate would still fill
  // a timeline for somebody who saves out of habit.
  await saveDocument(page);
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(1);

  // Nothing went wrong, so nothing is said and nothing is an error: the reader is
  // told by the Save ("Saved <name>"), and the timeline tells the rest by still
  // marking the head as the current version. A refusal here would be a broken
  // promise reported about an act that kept every promise it made.
  const status = page.locator("#status");
  await expect(status).not.toHaveClass(/error/);
  await expect(status).toContainText("Saved");
  await expect(page.locator(rows).first()).toContainText("Current version");

  // And a real change still does lay one down, which is what says the suppression
  // is about CONTENT and not about Save.
  await saveAnEdit(page, "VHCHANGED");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);
  await expect(page.locator(rows).first()).toContainText("Saved");

  expect(consoleErrors).toEqual([]);
});

test("previewing a version composes the editing chrome away, and leaving it puts everything back", async ({
  page,
  consoleErrors,
}) => {
  // The owner's other defect: the preview was read-only, said so, and disabled
  // Editing and Suggesting — over a document wearing the full editing ribbon.
  // `docs/126`'s container policy: "never, for you" is composition and is silent,
  // and a preview is that for as long as it is on screen.
  await gotoEditor(page);
  await saveAnEdit(page, "VHCHROME");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  // BEFORE: the ribbon is there, and a band that is NOT the default one is the
  // active tab — so "the tab came back" cannot pass by accident on Home.
  const ribbon = page.locator(".ribbon");
  const menuBar = page.locator("#appMenuBar");
  await page.locator("#tabInsert").click();
  await expect(page.locator("#tabInsert")).toHaveAttribute("aria-selected", "true");
  await expect(ribbon).toBeVisible();
  await expect(menuBar).toBeHidden();
  const selectionToolbarWas = await page.locator("#selToolbar").count();

  // DURING: select the older entry, which previews it.
  await page.locator(rows).last().locator(".version-item-entry").click();
  await expect(page.locator("#versionPreviewBanner")).toBeVisible();
  // No ribbon at all — not a ribbon of greyed bands. Asserted on the ribbon and on
  // a control inside it, because a band left painted inside a hidden ribbon would
  // still be a wall of dead controls to anyone who reached it.
  await expect(ribbon).toBeHidden();
  await expect(page.locator("#tabInsert")).toBeHidden();
  await expect(page.locator("#bold")).toBeHidden();
  await expect(page.locator("#selToolbar")).toBeHidden();
  // The composed-away set is published on `<body>`, so what went is readable
  // rather than inferred from eighteen classes.
  const withheld = (await page.locator("body").getAttribute("data-chrome-withheld")) ?? "";
  expect(withheld.split(" ")).toContain("ribbon");
  expect(withheld.split(" ")).toContain("selection");
  // ONE navigation axis, never none: withholding the ribbon reveals the menu bar,
  // which is where File ▸ Print lives (`109` UX-014, `docs/122`).
  await expect(menuBar).toBeVisible();
  // And the way BACK survives — the rail's Versions entry and the timeline itself.
  // Without these the reader is stranded in a preview of their own document.
  await expect(page.locator("#railVersions")).toBeVisible();
  await expect(page.locator(panel)).toBeVisible();
  await expect(page.locator("#versionPreviewBack")).toBeVisible();

  // AFTER: back to current restores exactly what was there, the active tab
  // included.
  await page.locator("#versionPreviewBack").click();
  await expect(page.locator("#versionPreviewBanner")).toBeHidden();
  // The keyboard goes somewhere: "Back to current" lives in the banner and leaves
  // with it, so the mode change that ends a preview MUST hand focus on rather
  // than let it fall to `<body>` — which is what a blanket "never take focus on a
  // mode change" would have done here, measured.
  await expectEditorFocused(page);
  await expect(ribbon).toBeVisible();
  await expect(menuBar).toBeHidden();
  await expect(page.locator("#tabInsert")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#panelInsert")).toBeVisible();
  expect(await page.locator("#selToolbar").count()).toBe(selectionToolbarWas);
  expect((await page.locator("body").getAttribute("data-chrome-withheld")) ?? "").toBe("");

  expect(consoleErrors).toEqual([]);
});

test("selecting an entry previews it read-only, and Back to current returns", async ({
  page,
}) => {
  await gotoEditor(page);
  // Two versions, so there is one that is NOT the head to preview. Previewing the
  // head is a no-op by design: it would swap the live session for a byte-identical
  // copy and throw the caret away for nothing.
  await saveAnEdit(page, "VHPREVIEW");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  const earlier = page.locator(rows).last();
  await earlier.locator(".version-item-entry").click();
  await expect(earlier).toHaveAttribute("aria-selected", "true");

  // The Docs interaction: the version is shown IN THE CANVAS, read-only, rather
  // than described in the list. The bar names when it is from.
  const banner = page.locator("#versionPreviewBanner");
  await expect(banner).toBeVisible();
  await expect(banner).toContainText("read-only");

  // Read-only through the editor's EXISTING fail-closed choke point rather than a
  // second gate: a preview sets `readOnlyReason` and viewing mode, so every
  // mutation route refuses with the preview's own sentence. Proved by trying to
  // type — the one route a hidden button cannot stop.
  await page.locator("#editorTextInput").focus();
  await page.keyboard.insertText("this must not land");
  await expect
    .poll(async () =>
      page.evaluate(() =>
        [
          document.getElementById("statusAlertRegion")?.textContent ?? "",
          document.getElementById("statusToast")?.textContent ?? "",
          document.getElementById("viewingBannerText")?.textContent ?? "",
        ].join(" "),
      ),
    )
    .toMatch(/earlier version/i);
  // And the mode control reflects it rather than leaving the reader to guess.
  await expect(page.locator('[data-review-mode="editing"]')).toBeDisabled();

  await page.locator("#versionPreviewBack").click();
  await expect(banner).toBeHidden();
  // Back on the live document, editing is possible again — the guarantee, not the
  // mechanism.
  await expect(page.locator('[data-review-mode="editing"]')).toBeEnabled();
});

test("restore is non-destructive: the document you replace becomes a version", async ({
  page,
}) => {
  await gotoEditor(page);
  await saveAnEdit(page, "VHRESTORE");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  // Through the PREVIEW BAR, which is Restore's second surface and the one
  // Google Docs uses: the bar that is already saying "this is not your document"
  // is the one that offers to make it your document. It exists only while a
  // preview is on screen, which is why it is never a resting disabled control.
  await page.locator(rows).last().locator(".version-item-entry").click();
  const restore = page.locator("#versionPreviewRestore");
  await expect(restore).toBeVisible();
  await restore.click();

  // The confirmation is the only place the reader is TOLD that restoring keeps
  // their current work, which is why there is one at all (`docs/139` §18 q6).
  // Google Docs does not ask; its restore is an undoable edit to a server-side
  // document, and this one replaces what is in the tab.
  const dialog = page.locator("#confirmDialog");
  await expect(dialog).toBeVisible();
  await expect(page.locator("#confirmDescription")).toContainText(
    "saved as a version first, so you can go back to it",
  );
  await expect(page.locator("#confirmNote")).toContainText("unsaved");
  await page.locator("#confirmAccept").click();

  // THE PROPERTY THAT MATTERS: append, never rewind (`docs/140` §1.2). The
  // restore adds a row; the version it restored FROM and the pre-restore capture
  // of the head are both still there. Four rows: Opened, Saved, the pre-restore
  // capture, and the restore itself.
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(4);
  const kinds = await page.locator(rows).allInnerTexts();
  const joined = kinds.join(" | ");
  expect(joined, "the restore is not in the timeline").toMatch(/Restored/);
  expect(joined, "the pre-restore capture of the head is missing").toMatch(/Before a restore/);
  expect(joined, "the version that was restored from was deleted").toMatch(/Opened/);

  // And the document is Edited/Unsaved, because the file on disk is still the old
  // one (`docs/139` §8.5 step 7).
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
});

test("the panel is a grid: arrows move rows and cells, and the keyboard really lands there", async ({
  page,
}) => {
  await gotoEditor(page);
  await saveAnEdit(page, "VHGRID");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  await expect(page.locator(list)).toHaveAttribute("role", "grid");
  // Opening a real surface takes the keyboard, and a GRID puts it on a cell —
  // `aria-activedescendant` is the listbox mechanism, and this list stopped being
  // one when its rows gained a menu button (a `role="option"` may not hold an
  // interactive child).
  const ids = await page.locator(rows).evaluateAll((els) => els.map((el) => el.id));
  expect(await focusedCell(page)).toEqual({ row: ids[0], cell: "entry" });

  // Exactly one tab stop, which is what makes a grid one Tab stop and not N.
  expect(
    await page.locator(`${rows} [tabindex="0"]`).count(),
    "a grid must have exactly one tab stop",
  ).toBe(1);

  await page.keyboard.press("ArrowDown");
  expect(await focusedCell(page)).toEqual({ row: ids[0], cell: "entry" });
  await expect(page.locator(`#${ids[0]}`)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowDown");
  expect(await focusedCell(page)).toEqual({ row: ids[1], cell: "entry" });
  await page.keyboard.press("ArrowUp");
  expect(await focusedCell(page)).toEqual({ row: ids[0], cell: "entry" });
  await page.keyboard.press("End");
  expect(await focusedCell(page)).toEqual({ row: ids[ids.length - 1], cell: "entry" });
  await page.keyboard.press("Home");
  expect(await focusedCell(page)).toEqual({ row: ids[0], cell: "entry" });

  // The second axis, which is the point of the grid: Right reaches the ⋮, Left
  // comes back, and the column is kept when the row changes.
  await page.keyboard.press("ArrowRight");
  expect(await focusedCell(page)).toEqual({ row: ids[0], cell: "menu" });
  await page.keyboard.press("ArrowDown");
  expect(await focusedCell(page)).toEqual({ row: ids[1], cell: "menu" });
  await page.keyboard.press("ArrowLeft");
  expect(await focusedCell(page)).toEqual({ row: ids[1], cell: "entry" });

  // Escape closes the surface and gives the keyboard back, which is the contract
  // every panel in this editor keeps.
  await page.keyboard.press("Escape");
  await expect(page.locator(panel)).toBeHidden();
  const focused = await page.evaluate(() => document.activeElement?.id ?? "");
  expect(focused, "closing the panel left the keyboard on <body>").not.toEqual("");
});

test("the timeline keeps the keyboard when a preview opens under it", async ({ page }) => {
  await gotoEditor(page);
  await saveAnEdit(page, "VHFOCUS");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);
  const ids = await page.locator(rows).evaluateAll((els) => els.map((el) => el.id));

  // THIS TEST CREATES ITS CONDITION, and that is the whole of it. Selecting a
  // version swaps the document, swapping the document sets the review mode, and
  // setting the review mode ends by focusing the editing surface — so a keyboard
  // reader walking the timeline was ejected from the panel by the panel's own
  // feature, and the next ArrowUp scrolled the document instead of moving to the
  // next version.
  //
  // It has to WAIT for the preview to really be on screen: the settle is 220 ms
  // and the parse takes longer still, so an assertion made straight after the
  // keypress measures the editor before the defect has had a chance to happen
  // and passes whatever the code does. Waiting for the banner is what turns this
  // from a race into a guard.
  await page.keyboard.press("End");
  await expect(page.locator("#versionPreviewBanner")).toBeVisible();
  await expect(page.locator('[data-review-mode="editing"]')).toBeDisabled();

  expect(
    await focusedCell(page),
    "the preview took the keyboard out of the timeline",
  ).toEqual({ row: ids[ids.length - 1], cell: "entry" });

  // The guarantee, not the mechanism: the arrows still walk the list.
  await page.keyboard.press("ArrowUp");
  expect(await focusedCell(page)).toEqual({ row: ids[0], cell: "entry" });

  // THE SECOND DOORWAY, and the reason this test has two halves. The row menu is
  // parented to `document.body` so it can paint over the canvas, so a fix that
  // asked "is focus inside `#versionPanel`" left the identical defect one step
  // further on: open a row's menu from the keyboard, and the preview the arrow
  // press before it scheduled settles underneath and empties the menu of focus.
  // The condition is created the same way, and in the order that produces it:
  // End schedules the older version's preview, and the menu is opened inside the
  // 220 ms before it settles — which is what a reader does when they arrow to a
  // version and reach straight for its actions.
  //
  // The head previews too now — with what its last save changed (ADR-065,
  // Google's "Current version" view) — so the signal that a preview has landed
  // is WHICH row carries `is-previewing`, not whether the bar is shown.
  const head = page.locator(`#${ids[0]}`);
  const oldest = page.locator(`#${ids[ids.length - 1]}`);
  await expect(head).toHaveClass(/is-previewing/);
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  await expect(page.locator(rowMenu)).toBeVisible();
  // If the preview had already landed, the wait below would prove nothing.
  await expect(
    oldest,
    "the preview settled before the menu opened, so this run cannot see the defect",
  ).not.toHaveClass(/is-previewing/);
  await expect(oldest).toHaveClass(/is-previewing/);

  expect(
    await page.evaluate(() => !!document.activeElement?.closest("#versionRowMenu")),
    "the preview took the keyboard out of the open row menu",
  ).toBe(true);
  // And the menu still works: the arrow moves the highlight rather than the page.
  const before = await page.locator(`${rowMenu} .menu-item.active`).textContent();
  await page.keyboard.press("ArrowDown");
  await expect(page.locator(`${rowMenu} .menu-item.active`)).not.toHaveText(before);
});

test("the row's ⋮ menu carries that row's actions, and opens from the keyboard", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await saveAnEdit(page, "VHMENU");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);
  const ids = await page.locator(rows).evaluateAll((els) => els.map((el) => el.id));

  // Route one: the pointer.
  const older = page.locator(rows).last();
  const trigger = older.locator(".version-item-menu");
  // A 44px target, painted at all times — never revealed by hover, which is no
  // affordance at all on a phone.
  const box = await stableBox(trigger);
  expect(box.width, "the ⋮ is below the touch floor").toBeGreaterThanOrEqual(44);
  expect(box.height, "the ⋮ is below the touch floor").toBeGreaterThanOrEqual(44);
  await expect(trigger).toHaveAttribute("aria-haspopup", "menu");
  // Its name carries the version it acts on: "More actions" once per row names
  // nothing a screen-reader user can tell apart.
  await expect(trigger).toHaveAttribute("aria-label", /Actions for the version from .+/);

  await trigger.click();
  await expect(page.locator(rowMenu)).toBeVisible();
  await expect(trigger).toHaveAttribute("aria-expanded", "true");
  const labels = await page
    .locator(`${rowMenu} .menu-item`)
    .evaluateAll((els) => els.map((el) => el.textContent.trim()));
  expect(labels).toEqual([
    "Restore this version",
    // The two ways out of a version that are not "replace my document with it"
    // (`docs/139` VH-007), in the order Google Docs offers them: after Restore,
    // before the editing rows.
    "Make a copy",
    "Download this version",
    "Name this version…F2",
    "Keep this version",
    "Show changes",
    expect.stringContaining("Delete this version"),
  ]);
  // Show changes, and THIS ROW IS THE OLDEST, which is now the one that refuses.
  //
  // Two corrections live in this one assertion. It used to be present and
  // disabled carrying "Comparing one version with another is not built yet",
  // which was wrong about which half was missing — the diff crate was complete
  // the whole time and `webapp/` called neither half. Then it was enabled here
  // and refused on the HEAD, because the comparison ran against the document on
  // screen. ADR-062 compares against a PREDECESSOR, so the head is live (the
  // most useful row: what changed in the latest save?) and the earliest version
  // kept is the one with nothing before it. `version-diff-canvas.spec.mjs` drives both and
  // asserts the comparison each produces.
  const changes = page.locator(`${rowMenu} [data-command-id="version.changes"]`);
  await expect(changes).toBeDisabled();
  await expect(changes).toHaveAttribute("title", /earliest version/i);
  await expect(changes).not.toHaveAttribute("title", /not built yet/i);
  await expect(changes).not.toHaveAttribute("title", /comparing it with itself/i);
  // Keep is a STATE, so it is a checkbox row rather than a label that flips.
  await expect(page.locator(`${rowMenu} [data-command-id="version.keep"]`)).toHaveAttribute(
    "role",
    "menuitemcheckbox",
  );

  // Light dismiss: a transient surface closes when you point somewhere else.
  //
  // The point is DERIVED from the menu's own box rather than typed in. This line
  // used to read `click(400, 500)`, and adding two rows to the menu (`docs/139`
  // VH-007) made that point land INSIDE it — so the "dismiss" ran a command and
  // put a confirmation on screen, and the failure showed up three assertions
  // later as focus that had left the grid. A fixed coordinate is a guard pinned
  // to a measured size; this one is pinned to the guarantee (a press OUTSIDE the
  // menu closes it) and survives the menu changing shape.
  const menuBox = await stableBox(page.locator(rowMenu));
  await page.mouse.click(Math.max(8, Math.round(menuBox.x) - 80), Math.round(menuBox.y) + 20);
  await expect(page.locator(rowMenu)).toBeHidden();
  await expect(trigger).toHaveAttribute("aria-expanded", "false");

  // Route two: the KEYBOARD, with no pointer at all. This is the assertion the
  // whole structure exists for — a ⋮ inside a list that a keyboard cannot reach
  // is worse than the bar it replaced.
  //
  // Reopened rather than continued: the light-dismiss press above landed on the
  // document, so the document has the keyboard and it is right that it does.
  // Opening the panel is what puts the keyboard back in the timeline, and that
  // is the gesture under test.
  await page.locator("#versionPanelClose").click();
  await openTimeline(page);
  // ASSERTED, not assumed, and that is what makes the rest of this test
  // trustworthy. The claim above — opening the panel puts the keyboard in the
  // timeline — is an EVENTUAL one: `open()` awaits the store before it places
  // focus, and the File page it was run from restores focus as it closes, so a
  // key pressed on the next line sometimes went to the document instead. The
  // test then failed three assertions later on a focus that had never arrived,
  // and it failed 2 runs in 5 on `main` before this branch touched it. Waiting on
  // the state change rather than sampling once is the fix this repository's own
  // note about clock-bound tests asks for.
  await expect
    .poll(() => page.evaluate(() => Boolean(document.activeElement?.closest("#versionPanelBody"))))
    .toBe(true);
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowRight");
  expect(await focusedCell(page)).toEqual({ row: ids[ids.length - 1], cell: "menu" });
  await page.keyboard.press("Enter");
  await expect(page.locator(rowMenu)).toBeVisible();
  // Focus is INSIDE the menu, or arrowing through it is impossible and it is a
  // pointer-only surface wearing menu semantics.
  expect(await page.evaluate(() => !!document.activeElement?.closest("#versionRowMenu"))).toBe(
    true,
  );
  await page.keyboard.press("Escape");
  await expect(page.locator(rowMenu)).toBeHidden();
  // …and Escape hands the keyboard back to the ⋮ it came from.
  expect(await focusedCell(page)).toEqual({ row: ids[ids.length - 1], cell: "menu" });

  // Route three: Shift+F10, the platform gesture, from the entry cell.
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Shift+F10");
  await expect(page.locator(rowMenu)).toBeVisible();
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("naming a version pins it — from the row menu by keyboard, and from F2", async ({
  page,
}) => {
  await gotoEditor(page);
  await saveAnEdit(page, "VHNAME");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  // The whole action, driven by keys only: End selects the older row, Right
  // reaches its ⋮, Enter opens the menu, Down reaches "Name this version…",
  // Enter runs it.
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  // Three rows down, because "Make a copy" and "Download this version" now sit
  // between Restore and the editing rows (`docs/139` VH-007). Walked rather than
  // clicked, because what is under test is that the keyboard reaches every row.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(page.locator(`${rowMenu} .menu-item.active`)).toHaveText(/Name this version/);
  await page.keyboard.press("Enter");

  // A real dialog, not `window.prompt` — which cannot be labelled, cannot be
  // localised, and freezes the wasm engine's event loop while it is up.
  await expect(page.locator("#versionNameDialog")).toBeVisible();
  await expect(page.locator("#versionNameInput")).toBeFocused();
  await page.locator("#versionNameInput").fill("Before the legal review");
  await page.locator("#versionNameConfirm").click();

  const older = page.locator(rows).last();
  await expect(older).toContainText("Before the legal review");
  // Naming PINS: the label makes it findable, the pin makes it durable, and the
  // store never prunes a pin — when a ceiling cannot be met without deleting one
  // it refuses the capture instead. So the row says "Named" in its accessible
  // name rather than implying the name is best-effort.
  await expect(older.locator(".version-item-entry")).toHaveAttribute("aria-label", /Named/);
  await expect(page.locator("#versionPanelDetail")).toContainText("1 of 15 named");
  // And the menu now says the version is kept, as a checked row.
  await older.locator(".version-item-menu").click();
  await expect(page.locator(`${rowMenu} [data-command-id="version.keep"]`)).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.keyboard.press("Escape");

  // Docs' "Only show named versions".
  await page.locator("#versionNamedOnly").check();
  await expect(page.locator(rows)).toHaveCount(1);
  await expect(page.locator(rows).first()).toContainText("Before the legal review");
  await page.locator("#versionNamedOnly").uncheck();
  await expect(page.locator(rows)).toHaveCount(2);

  // Name's SECOND surface: F2 on the focused row, which is the key every list
  // carries and which the menu advertises in its shortcut column.
  await page.locator(rows).last().locator(".version-item-entry").click();
  await page.keyboard.press("F2");
  await expect(page.locator("#versionNameDialog")).toBeVisible();
  await expect(page.locator("#versionNameInput")).toHaveValue("Before the legal review");
  await page.keyboard.press("Escape");
});

test("Delete removes a version from the row menu, and from the Delete key", async ({ page }) => {
  await gotoEditor(page);
  await saveAnEdit(page, "VHDELETE");
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  // The head is the document, so its own menu refuses — with the reason, not by
  // silence.
  await page.locator(rows).first().locator(".version-item-menu").click();
  const headDelete = page.locator(`${rowMenu} [data-command-id="version.delete"]`);
  await expect(headDelete).toBeDisabled();
  await expect(headDelete).toHaveAttribute("title", /current version/i);
  await page.keyboard.press("Escape");

  // The older one can go. Through the Delete KEY, which is Delete's second
  // surface; the menu row is the first.
  await page.locator(rows).last().locator(".version-item-entry").click();
  await page.keyboard.press("Delete");
  await expect(page.locator("#confirmDialog")).toBeVisible();
  await expect(page.locator("#confirmDescription")).toContainText("cannot be undone");
  await page.locator("#confirmAccept").click();
  await expect(page.locator(rows)).toHaveCount(1);
});

test("with autosave off BOTH entry points are still there, and both say why", async ({ page }) => {
  await gotoEditor(page);
  // Version history rides the autosave switch (ADR-038 / `docs/139` §18 q2): one
  // switch must not promise what the other has stopped doing. Turning it off must
  // leave a control that EXPLAINS ITSELF, never one that silently vanishes or
  // silently does nothing (SKILL §10).
  await runFilePageCommand(page, "view.settings");
  await page.locator("#autosaveToggle").uncheck();
  await page.keyboard.press("Escape");

  await page.locator("#tabView").click();
  for (const id of ["#viewVersionsBtn", "#railVersions"]) {
    const button = page.locator(id);
    await expect(button, `${id} vanished instead of explaining itself`).toBeVisible();
    await expect(button).toBeDisabled();
    await expect(button).toHaveAttribute("title", /autosave/i);
  }

  // And the File row, through the registry rather than the DOM, so the palette
  // and the menus say the same thing.
  await page.locator("#tabFile").click();
  const row = page.locator('#filePageBody .file-page-item[data-command="file.versionHistory"]');
  await expect(row, "the File row vanished instead of explaining itself").toBeVisible();
  await expect(row).toBeDisabled();
  await expect(row).toHaveAttribute("title", /autosave/i);
});

test("the editing surface still owns the keyboard after the panel closes", async ({ page }) => {
  await gotoEditor(page);
  await openTimeline(page);
  await waitForBaseline(page);
  await page.locator("#versionPanelClose").click();
  await page.locator("#editorTextInput").focus();
  await expectEditorFocused(page);
});
