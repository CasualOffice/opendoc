// HF-011 / OO-004 — a tab crash or OS kill must not be unrecoverable.
//
// The honest shape for this row is to SIMULATE THE CRASH. A spec that types,
// waits, and then asserts "a draft row exists in IndexedDB" proves persistence
// and says nothing about recovery: the whole defect is that the work is gone
// after a kill that runs no shutdown code. So the first test here kills the
// renderer over CDP (`Page.crash`) — no `beforeunload`, no `pagehide`, no
// `visibilitychange`, nothing — opens a fresh page, and asserts the work is
// offered back and comes back.
//
// Reading the document: the editor paints to CANVAS, so document text is not
// in `#pages`. `#a11yDocument` is the model-derived off-screen mirror and is
// where the text actually is.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  openAppMenu,
} from "./fixtures.mjs";

const MARKER = "CRASHRECOVERYMARKER";

/** Types a marker into the open document and confirms the model took it. */
async function typeMarker(page, marker = MARKER) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(marker);
  await expect(page.locator("#a11yDocument")).toContainText(marker);
}

/** Waits for the autosave cadence to actually put a draft on disk. The
 *  indicator is the product's own, so this cannot pass on a string the app
 *  never emits. Quiesce is 5s, so the budget is generous but finite. */
async function waitForDraftWritten(page) {
  await expect(page.locator("#draftStatus")).toContainText(/Draft saved/, { timeout: 25_000 });
}

/** Kills the renderer the way an OOM kill does: no handler gets to run.
 *
 *  The `send` is deliberately NOT awaited — the target it was sent to no longer
 *  exists to answer, so awaiting it hangs until the test times out. Wait for
 *  the crash event instead. */
async function crashRenderer(page) {
  const client = await page.context().newCDPSession(page);
  void client.send("Page.crash").catch(() => {});
  await page.waitForEvent("crash", { timeout: 15_000 }).catch(() => {});
}

test("work typed before a renderer crash is offered back, and comes back", async ({ page }) => {
  test.setTimeout(120_000);
  const context = page.context();

  await gotoEditor(page);
  await typeMarker(page);
  await waitForDraftWritten(page);

  await crashRenderer(page);
  await page.close({ runBeforeUnload: false }).catch(() => {});

  // A brand-new page in the same browser profile: this is the user reopening
  // the editor after the tab died.
  const recovered = await context.newPage();
  await gotoEditor(recovered);

  const bar = recovered.locator("#draftRecoveryBar");
  await expect(bar).toBeVisible({ timeout: 20_000 });
  await expect(bar).toContainText("opendoc-demo.docx");
  await expect(bar).toContainText(/autosaved/);

  // OFFERED, NOT APPLIED. The freshly opened file must still be the file — if
  // the draft had been applied silently, the marker would already be here and
  // whatever was on screen would have been replaced without a word.
  await expect(recovered.locator("#a11yDocument")).not.toContainText(MARKER);

  await bar.locator("button[data-draft-restore]").first().click();

  // The work is back.
  await expect(recovered.locator("#a11yDocument")).toContainText(MARKER, { timeout: 30_000 });
  // And it is still UNSAVED work: it has never been written to a file, so
  // reporting it clean would re-arm the same data loss one level up.
  await expect(recovered.locator("#documentStateText")).toHaveText("Edited");
  // …and the tab says so: the recovered document's name, with the unsaved mark.
  await expect(recovered).toHaveTitle("• opendoc-demo.docx — OpenDoc");
  await expect(bar).toBeHidden();

  // The guarantee behind "Edited", asserted through a real surface rather than
  // through the pill (which is display only): anything that would replace the
  // restored document now asks first, because `documentIsDirty()` says the
  // work is still unsaved.
  await openAppMenu(recovered, "file");
  await recovered.locator('#appMenuPopover .app-menu-item[data-command="file.new"]').click();
  await expect(recovered.locator("#confirmDialog")).toBeVisible();
  await recovered.locator("#confirmCancel").click();

  await recovered.close();
});

test("recovered work is protected again immediately, without waiting to be retyped", async ({
  page,
}) => {
  test.setTimeout(150_000);
  const context = page.context();

  await gotoEditor(page);
  await typeMarker(page, "TWICECRASHEDMARKER");
  await waitForDraftWritten(page);
  await crashRenderer(page);
  await page.close({ runBeforeUnload: false }).catch(() => {});

  const second = await context.newPage();
  await gotoEditor(second);
  await expect(second.locator("#draftRecoveryBar")).toBeVisible({ timeout: 20_000 });
  await second.locator("button[data-draft-restore]").first().click();
  await expect(second.locator("#a11yDocument")).toContainText("TWICECRASHEDMARKER", {
    timeout: 30_000,
  });

  // Crash again WITHOUT typing anything. If restoring deleted the row before
  // taking its own copy, the work now exists nowhere: the restored document is
  // in the wasm heap of a renderer that is about to die.
  await crashRenderer(second);
  await second.close({ runBeforeUnload: false }).catch(() => {});

  const third = await context.newPage();
  await gotoEditor(third);
  await expect(third.locator("#draftRecoveryBar")).toBeVisible({ timeout: 20_000 });
  await third.locator("button[data-draft-restore]").first().click();
  await expect(third.locator("#a11yDocument")).toContainText("TWICECRASHEDMARKER", {
    timeout: 30_000,
  });
  await third.close();
});

test("saving clears the draft, so the next load offers nothing", async ({ page }) => {
  test.setTimeout(120_000);
  const context = page.context();

  await gotoEditor(page);
  await typeMarker(page, "SAVEDWORKMARKER");
  await waitForDraftWritten(page);

  // Save through the real File ▸ Save route; the bytes leave the editor.
  const download = page.waitForEvent("download");
  await openAppMenu(page, "file");
  await page.locator('#appMenuPopover .app-menu-item[data-command="file.save"]').click();
  await download;
  await expect(page.locator("#documentStateText")).toHaveText("Downloaded");
  await expect(page.locator("#draftStatus")).toBeHidden();

  await crashRenderer(page);
  await page.close({ runBeforeUnload: false }).catch(() => {});

  const next = await context.newPage();
  await gotoEditor(next);
  // Nothing to recover: the user has the file. Give the boot scan a moment so
  // this cannot pass merely by being faster than the store.
  await next.waitForTimeout(1_500);
  await expect(next.locator("#draftRecoveryBar")).toBeHidden();
  await next.close();
});

test("the recovery offer survives being dismissed, and File ▸ Recover brings it back", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const context = page.context();

  await gotoEditor(page);
  await typeMarker(page, "DISMISSEDWORKMARKER");
  await waitForDraftWritten(page);
  await crashRenderer(page);
  await page.close({ runBeforeUnload: false }).catch(() => {});

  const recovered = await context.newPage();
  await gotoEditor(recovered);
  const bar = recovered.locator("#draftRecoveryBar");
  await expect(bar).toBeVisible({ timeout: 20_000 });

  // Dismiss must not destroy anything — nothing in this bar may lose work by
  // being closed.
  await recovered.locator("#draftRecoveryDismiss").click();
  await expect(bar).toBeHidden();

  await openAppMenu(recovered, "file");
  const row = recovered.locator(
    '#appMenuPopover .app-menu-item[data-command="file.recoverDrafts"]',
  );
  await expect(row).toBeEnabled();
  await row.click();
  await expect(bar).toBeVisible();
  await expect(bar).toContainText("opendoc-demo.docx");

  await recovered.close();
});

test("with nothing to recover, File ▸ Recover is disabled and says why", async ({ page }) => {
  await gotoEditor(page);
  await openAppMenu(page, "file");
  const row = page.locator('#appMenuPopover .app-menu-item[data-command="file.recoverDrafts"]');
  await expect(row).toBeVisible();
  await expect(row).toBeDisabled();
  // Never a dead control: a command that cannot act says so (SKILL.md §10).
  await expect(row).toHaveAttribute("title", /No unsaved work to recover/);
});

test("autosave can be turned off, and then keeps nothing", async ({ page }) => {
  test.setTimeout(120_000);
  const context = page.context();

  // `?autosave=0` is the same suppression switch the iframe check uses (the
  // marketing home page boots the editor in a frame and must leave no drafts).
  await page.goto("/editor.html?fixture=rich&autosave=0");
  await page.waitForFunction(
    () => document.querySelectorAll(".page-wrap").length > 0 && document.body.dataset.fontsReady === "true",
    null,
    { timeout: 45_000 },
  );
  await typeMarker(page, "SUPPRESSEDMARKER");
  // Well past the 5s quiesce window.
  await page.waitForTimeout(8_000);
  await expect(page.locator("#draftStatus")).toBeHidden();

  await crashRenderer(page);
  await page.close({ runBeforeUnload: false }).catch(() => {});

  const next = await context.newPage();
  await gotoEditor(next);
  await next.waitForTimeout(1_500);
  await expect(next.locator("#draftRecoveryBar")).toBeHidden();
  await next.close();
});
