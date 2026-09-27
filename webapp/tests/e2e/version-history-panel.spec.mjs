// The version history SURFACE, driven through the real application — HF-068 /
// `105` OO-004, `docs/139` (product) and `docs/140` (architecture).
//
// `version-history-store.spec.mjs` already drives the store against real
// IndexedDB. This file asks the question that one cannot: is any of it reachable,
// and does a person get the Google Docs interaction the design names? #653 landed
// the whole store with no interface at all, and "built but unreachable" is the
// most expensive recurring pattern in this repository (SKILL §9.4) — so these
// tests go through the File surface, the ribbon, the keyboard and the panel,
// never through the module.
//
// EVERY TEST CREATES ITS CONDITION. A panel test that passes against an empty
// store proves nothing: the timeline would be empty, every action disabled, and
// the assertions would pass over a feature that does not work. So each test below
// first makes a version exist — by opening a document (the import baseline) and,
// where it needs two, by saving — and asserts on rows that are really there.
import {
  expect,
  expectEditorFocused,
  gotoEditor,
  runFilePageCommand,
  saveDocument,
  test,
} from "./fixtures.mjs";

const panel = "#versionPanel";
const list = "#versionPanelBody";
const rows = `${list} .version-item`;

/** Opens the timeline through the File surface — the primary entry point
 *  `docs/139` §8.1 names, and the one both Google Docs and ONLYOFFICE use. The
 *  helper asserts the row is present AND enabled, so an unreachable command fails
 *  loudly rather than quietly doing nothing. */
async function openTimeline(page) {
  await runFilePageCommand(page, "file.versionHistory");
  await expect(page.locator(panel)).toBeVisible();
}

/** Waits for the document's import baseline to reach the timeline. The capture
 *  rides the open path and is deliberately not awaited by it — a timeline is not
 *  what a reader is waiting for — so the row arrives shortly after the document. */
async function waitForBaseline(page) {
  await expect(page.locator(rows)).toHaveCount(1);
}

test("the timeline is reachable from the File surface AND from the View band", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // Surface one: File ▸ Version history.
  await openTimeline(page);
  await expect(page.locator("#versionPanelTitle")).toBeVisible();
  await page.locator("#versionPanelClose").click();
  await expect(page.locator(panel)).toBeHidden();

  // Surface two: the View band's panel toggle, beside Outline and Comments —
  // `105` UX-004, a capability reachable from one place is the recurring defect
  // here. It reflects state, so it is a toggle and not a second door that only
  // opens.
  await page.locator("#tabView").click();
  const button = page.locator("#viewVersionsBtn");
  await expect(button).toBeEnabled();
  await expect(button).toHaveAttribute("aria-pressed", "false");
  await button.click();
  await expect(page.locator(panel)).toBeVisible();
  await expect(button).toHaveAttribute("aria-pressed", "true");
  await button.click();
  await expect(page.locator(panel)).toBeHidden();
  await expect(button).toHaveAttribute("aria-pressed", "false");

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
  await expect(first).toHaveAttribute("role", "option");
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
  // permitted child of a listbox and the day is already in every option's name.
  const group = page.locator(`${list} .version-group`).first();
  await expect(group).toHaveAttribute("role", "group");
  await expect(group.locator(".version-group-day")).toHaveText("Today");

  // The disclosure `docs/139` §12 asks for by name: how much history there is,
  // where it lives, and what the policy will do with it.
  await expect(page.locator("#versionPanelSummary")).toContainText("1 version kept");
  await expect(page.locator("#versionPanelDetail")).toContainText("in this browser");
  await expect(page.locator("#versionPanelPolicy")).toContainText("7 days");
});

test("saving lays down a version, and the timeline is metadata only", async ({ page }) => {
  await gotoEditor(page);
  await openTimeline(page);
  await waitForBaseline(page);

  // An explicit Save is ALWAYS a version (ADR-038 / `docs/139` §18 q3): it is the
  // point a user recognises, and content-addressed checkpoints make a no-change
  // Save cost one row rather than a second copy of the document.
  await saveDocument(page);
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);
  // Newest first, which is the Docs order and the only one a timeline can have.
  await expect(page.locator(rows).first()).toContainText("Saved");
  await expect(page.locator(rows).last()).toContainText("Opened");
  await expect(page.locator("#versionPanelSummary")).toContainText("2 versions kept");
});

test("selecting an entry previews it read-only, and Back to current returns", async ({
  page,
}) => {
  await gotoEditor(page);
  // Two versions, so there is one that is NOT the head to preview. Previewing the
  // head is a no-op by design: it would swap the live session for a byte-identical
  // copy and throw the caret away for nothing.
  await saveDocument(page);
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  const earlier = page.locator(rows).last();
  await earlier.click();
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
  await saveDocument(page);
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  const earlier = page.locator(rows).last();
  await earlier.click();
  const restore = page.locator("#versionRestoreBtn");
  await expect(restore).toBeEnabled();
  await restore.click();

  // The confirmation is the only place the reader is TOLD that restoring keeps
  // their current work, which is why there is one at all (`docs/139` §18 q6 —
  // settled by this change). Google Docs does not ask; its restore is an
  // undoable edit to a server-side document, and this one replaces what is in
  // the tab.
  const dialog = page.locator("#confirmDialog");
  await expect(dialog).toBeVisible();
  await expect(page.locator("#confirmDescription")).toContainText("Nothing is lost");
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

test("the panel is a listbox: arrows move, Enter opens, Escape hands focus back", async ({
  page,
}) => {
  await gotoEditor(page);
  await saveDocument(page);
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  // Opening a real surface takes the keyboard, and the list is where the keyboard
  // belongs (`docs/139` §14).
  await expect(page.locator(list)).toBeFocused();
  await expect(page.locator(list)).toHaveAttribute("role", "listbox");

  const ids = await page.locator(rows).evaluateAll((els) => els.map((el) => el.id));
  await page.keyboard.press("ArrowDown");
  await expect(page.locator(list)).toHaveAttribute("aria-activedescendant", ids[0]);
  await page.keyboard.press("ArrowDown");
  await expect(page.locator(list)).toHaveAttribute("aria-activedescendant", ids[1]);
  await page.keyboard.press("ArrowUp");
  await expect(page.locator(list)).toHaveAttribute("aria-activedescendant", ids[0]);
  await page.keyboard.press("End");
  await expect(page.locator(list)).toHaveAttribute("aria-activedescendant", ids[ids.length - 1]);
  await page.keyboard.press("Home");
  await expect(page.locator(list)).toHaveAttribute("aria-activedescendant", ids[0]);

  // Escape closes the surface and gives the keyboard back, which is the contract
  // every panel in this editor keeps.
  await page.keyboard.press("Escape");
  await expect(page.locator(panel)).toBeHidden();
  const focused = await page.evaluate(() => document.activeElement?.id ?? "");
  expect(focused, "closing the panel left the keyboard on <body>").not.toEqual("");
});

test("naming a version pins it, and the filter shows only named versions", async ({ page }) => {
  await gotoEditor(page);
  await saveDocument(page);
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);

  await page.locator(rows).last().click();
  await page.locator("#versionNameBtn").click();
  // A real dialog, not `window.prompt` — which cannot be labelled, cannot be
  // localised, and freezes the wasm engine's event loop while it is up.
  await expect(page.locator("#versionNameDialog")).toBeVisible();
  await expect(page.locator("#versionNameInput")).toBeFocused();
  await page.locator("#versionNameInput").fill("Before the legal review");
  await page.locator("#versionNameConfirm").click();

  await expect(page.locator(rows).last()).toContainText("Before the legal review");
  // Naming PINS: the label makes it findable, the pin makes it durable, and the
  // store never prunes a pin — when a ceiling cannot be met without deleting one
  // it refuses the capture instead. So the panel says "Named" in the row's
  // accessible name rather than implying the name is best-effort.
  await expect(page.locator(rows).last()).toHaveAttribute("aria-label", /Named/);
  await expect(page.locator("#versionPanelDetail")).toContainText("1 of 15 named");

  // Docs' "Only show named versions".
  await page.locator("#versionNamedOnly").check();
  await expect(page.locator(rows)).toHaveCount(1);
  await expect(page.locator(rows).first()).toContainText("Before the legal review");
  await page.locator("#versionNamedOnly").uncheck();
  await expect(page.locator(rows)).toHaveCount(2);
});

test("with autosave off the entry is still there, and says why", async ({ page }) => {
  await gotoEditor(page);
  // Version history rides the autosave switch (ADR-038 / `docs/139` §18 q2): one
  // switch must not promise what the other has stopped doing. Turning it off must
  // leave a control that EXPLAINS ITSELF, never one that silently vanishes or
  // silently does nothing (SKILL §10).
  await runFilePageCommand(page, "view.settings");
  await page.locator("#autosaveToggle").uncheck();
  await page.keyboard.press("Escape");

  await page.locator("#tabView").click();
  const button = page.locator("#viewVersionsBtn");
  await expect(button).toBeDisabled();
  await expect(button).toHaveAttribute("title", /autosave/i);

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
