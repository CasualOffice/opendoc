// Getting a version OUT of the timeline — `docs/139` VH-007, the half a reader
// could not reach.
//
// The panel could show an old version and preview it, and then the only way out
// was to REPLACE the document with it. "Make a copy" and "Download this version"
// are what Google Docs offers at exactly that moment and are the natural next
// gesture after previewing.
//
// THE DOWNLOAD TEST ASSERTS BYTES, NOT A BUTTON. A spec that clicked Download
// and checked a file arrived would pass just as happily if the editor handed
// over the CURRENT document — which is the whole failure this feature can have,
// and the only one worth a guard. So it builds two versions whose contents
// really differ, downloads the older one, and hashes what came out: a
// checkpoint's id IS the SHA-256 of its bytes (`checkpointIdFor`), so the
// assertion is that the file's digest is the chosen version's checkpoint id and
// is NOT the head's. Nothing about that can be satisfied by the wrong document.
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

import {
  clickIntoFirstPage,
  expect,
  moveCaretToDocStart,
  runFilePageCommand,
  saveDocument,
  test,
} from "./fixtures.mjs";

const panel = "#versionPanel";
const rows = "#versionPanelBody .version-item";
const rowMenu = "#versionRowMenu";

async function openTimeline(page) {
  if (!(await page.locator(panel).isVisible())) {
    await runFilePageCommand(page, "file.versionHistory");
  }
  await expect(page.locator(panel)).toBeVisible();
}

/** Two versions that really differ: the import baseline, then a saved edit. A
 *  Save over an unmodified document writes nothing (`docs/139` §18 q3), so the
 *  edit is what makes the second row exist AND what makes the two artifacts
 *  distinguishable — which is the condition the digest assertion needs. */
async function twoVersions(page, marker) {
  await expect(page.locator(rows)).toHaveCount(1);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(marker);
  await saveDocument(page);
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(2);
}

/** Every version row the store really holds, newest first, read out of the same
 *  IndexedDB the panel reads. Metadata only — the artifacts are not touched. */
function storedVersions(page) {
  return page.evaluate(async () => {
    const open = indexedDB.open("opendoc-drafts");
    const db = await new Promise((resolve, reject) => {
      open.onsuccess = () => resolve(open.result);
      open.onerror = () => reject(open.error);
    });
    const store = db.transaction("version_meta", "readonly").objectStore("version_meta");
    const request = store.getAll();
    const all = await new Promise((resolve, reject) => {
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    db.close();
    return all
      .sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0))
      .map((row) => ({
        versionId: row.versionId,
        lineageId: row.lineageId,
        checkpointId: row.checkpointId,
        bytes: row.bytes,
        kind: row.kind,
      }));
  });
}

/** Puts the keyboard back in the timeline.
 *
 *  Typing into the document and saving leaves focus on the editing surface, and
 *  the panel only claims the keyboard when it OPENS — deliberately, so a capture
 *  that lands while somebody is typing does not snatch it. So a test that has
 *  just edited has to put it back, exactly as a reader would by clicking or
 *  tabbing into the list. */
async function focusTimeline(page) {
  await page.locator(`${rows} .version-item-entry`).first().focus();
}

/** Opens the ⋮ of the row at `index` and runs one of its commands. The pointer
 *  route; the keyboard route is asserted separately below, because a menu inside
 *  a list is only worth having if both reach it. */
async function runRowCommand(page, index, commandId) {
  await page.locator(rows).nth(index).locator(".version-item-menu").click();
  await expect(page.locator(rowMenu)).toBeVisible();
  const item = page.locator(`${rowMenu} [data-command-id="${commandId}"]`);
  await expect(item).toBeEnabled();
  await item.click();
}

test("the file a version downloads is that version's bytes, not the document on screen", async ({
  page,
}) => {
  await page.goto("/editor.html");
  await openTimeline(page);
  await twoVersions(page, "VHDOWNLOAD ");

  const stored = await storedVersions(page);
  expect(stored).toHaveLength(2);
  const [head, older] = stored;
  // The condition the whole test rests on: the two versions are really different
  // artifacts. If they were not, the digest assertion below could not tell the
  // right answer from the wrong one and would be a guard that cannot fail.
  expect(older.checkpointId).not.toBe(head.checkpointId);

  const waitForFile = page.waitForEvent("download");
  await runRowCommand(page, 1, "version.download");
  const file = await waitForFile;
  const bytes = await readFile(await file.path());
  const digest = `sha256-${createHash("sha256").update(bytes).digest("hex")}`;

  expect(digest).toBe(older.checkpointId);
  expect(digest).not.toBe(head.checkpointId);
  expect(bytes.length).toBe(older.bytes);
  // Named after the document, with the extension the VERSION was written in —
  // not "download" and not the browser's fallback.
  expect(file.suggestedFilename()).toMatch(/\.docx$/);

  // And the reader is told, on the one feedback channel, which file it was.
  await expect(page.locator("#status")).toContainText(file.suggestedFilename());
});

test("downloading the version being previewed is reachable from the preview bar too", async ({
  page,
}) => {
  // The second surface (SKILL §10). The row menu is the first; this is where a
  // reader already is when they want it, and it acts on the version the bar is
  // about rather than on a selection of its own.
  //
  // THIS TEST IS ABOUT REACHABILITY, not about bytes, and says so because a
  // mutation proved the difference: replacing the checkpoint's bytes with
  // `snapshot()` leaves this one green — while a preview is up, `snapshot()`
  // exports the preview, and an unmodified document re-exports verbatim. The
  // bytes guarantee is the test above, which downloads from a row with the LIVE
  // document on screen and can therefore tell the two apart.
  await page.goto("/editor.html");
  await openTimeline(page);
  await twoVersions(page, "VHBAR ");
  const stored = await storedVersions(page);

  await focusTimeline(page);
  await page.keyboard.press("End");
  await expect(page.locator("#versionPreviewBanner")).toBeVisible();
  const bar = page.locator("#versionPreviewDownload");
  await expect(bar).toBeVisible();
  await expect(bar).toBeEnabled();

  const waitForFile = page.waitForEvent("download");
  await bar.click();
  const file = await waitForFile;
  const digest = `sha256-${createHash("sha256")
    .update(await readFile(await file.path()))
    .digest("hex")}`;
  expect(digest).toBe(stored[1].checkpointId);
});

test("a copy opens here, as a new document with a timeline of its own", async ({ page }) => {
  await page.goto("/editor.html");
  await openTimeline(page);
  await twoVersions(page, "VHCOPY ");
  const before = await storedVersions(page);
  const originalLineage = before[0].lineageId;

  // The OLDER version, so the copy is demonstrably not just the document that
  // was already on screen.
  await runRowCommand(page, 1, "version.copy");

  // It CONFIRMS, and the confirmation is where the reader is told two things
  // they cannot see: that the copy arrives here rather than in a file manager
  // this product does not have, and that their current document is kept.
  const card = page.locator("#confirmDialog");
  await expect(card).toBeVisible();
  await expect(card).toContainText(/kept as a version/i);
  await page.locator("#confirmAccept").click();

  // The document is now the copy, under its own name.
  await expect(page.locator("#docTitle")).toHaveValue(/^Copy of /);
  // Unsaved: a copy has never been written anywhere.
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");

  // The copy is activated asynchronously behind the panel's one-job-at-a-time
  // queue, so the store is polled rather than read once: a single read right
  // after the click is a race, and a race in a guard is a guard that sometimes
  // proves nothing.
  await expect
    .poll(async () => new Set((await storedVersions(page)).map((row) => row.lineageId)).size)
    .toBe(2);
  const after = await storedVersions(page);
  const lineages = new Set(after.map((row) => row.lineageId));
  // A NEW timeline, not an append to the original's. Continuing the original
  // lineage would put the copy's future into the original's past and make the
  // two impossible to tell apart.
  expect(lineages.size).toBe(2);
  const copyLineage = [...lineages].find((id) => id !== originalLineage);
  expect(copyLineage).toBeTruthy();
  const copyRows = after.filter((row) => row.lineageId === copyLineage);
  expect(copyRows).toHaveLength(1);
  expect(copyRows[0].kind).toBe("import");
  // The copy's baseline is the chosen VERSION's artifact, byte for byte.
  expect(copyRows[0].checkpointId).toBe(before[1].checkpointId);

  // And the original's work was kept before the tab moved: its timeline grew by
  // the pre-copy capture rather than being abandoned mid-edit.
  const originalRows = after.filter((row) => row.lineageId === originalLineage);
  expect(originalRows.length).toBeGreaterThan(before.length - 1);
  expect(originalRows[0].kind).toBe("manual");

  // The timeline on screen is the COPY's, not the one it came from.
  await openTimeline(page);
  await expect(page.locator(rows)).toHaveCount(1);
});

test("both ways out are reachable by keyboard, and say so when the host withholds them", async ({
  page,
}) => {
  await page.goto("/editor.html");
  await openTimeline(page);
  await twoVersions(page, "VHKEYS ");

  // Keyboard: End selects the oldest row, Right reaches its ⋮, Enter opens the
  // menu. Both new rows are in it, in Google Docs' order, and both are enabled.
  await focusTimeline(page);
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  await expect(page.locator(rowMenu)).toBeVisible();
  for (const id of ["version.copy", "version.download"]) {
    await expect(page.locator(`${rowMenu} [data-command-id="${id}"]`)).toBeEnabled();
  }
  await page.keyboard.press("Escape");

  // A host that withheld `download` and `open` gets the rows DISABLED WITH THE
  // REASON, never removed: a reader who finds no Download at all cannot tell a
  // withheld permission from a missing feature (SKILL §10). `?can=` is the
  // per-capability contract (`docs/126` container policy §1) and narrows the
  // default set without taking the chrome away, which is what lets this assert
  // the ROWS rather than the absence of a menu.
  await page.goto("/editor.html?can=download,open");
  await openTimeline(page);
  // The same origin, so this load rejoins the timeline the earlier part of the
  // test built. How many rows it has does not matter; that every row's menu
  // refuses in the same words does.
  await expect(page.locator(rows).first()).toBeVisible();
  await page.locator(rows).first().locator(".version-item-menu").click();
  await expect(page.locator(rowMenu)).toBeVisible();
  for (const id of ["version.copy", "version.download"]) {
    const item = page.locator(`${rowMenu} [data-command-id="${id}"]`);
    await expect(item).toBeVisible();
    await expect(item).toBeDisabled();
    await expect(item).toHaveAttribute("title", /not granted/i);
  }
});

test("a restore from the preview bar keeps the LIVE document, not the preview", async ({ page }) => {
  // The pre-restore capture is the one thing standing between a restore and lost
  // work (`docs/140` §9) — and it was capturing the wrong document. `snapshot()`
  // exports whatever the canvas is showing, so taking it while a preview was up
  // recorded the OLD version as "the document you replaced" and let the reader's
  // unsaved edits go. Restore is reachable from the preview bar, which is the
  // main way anyone reaches it, so the defect was on the common path.
  await page.goto("/editor.html");
  await openTimeline(page);
  await twoVersions(page, "VHLIVE ");
  const before = await storedVersions(page);

  // Unsaved work, after the last version was written. This is what must survive.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("UNSAVEDLIVE ");

  await openTimeline(page);
  await focusTimeline(page);
  await page.keyboard.press("End");
  await expect(page.locator("#versionPreviewBanner")).toBeVisible();
  await page.locator("#versionPreviewRestore").click();
  await page.locator("#confirmAccept").click();

  await expect
    .poll(async () => (await storedVersions(page)).some((row) => row.kind === "pre_restore"))
    .toBe(true);
  const after = await storedVersions(page);
  const kept = after.find((row) => row.kind === "pre_restore");
  // The captured artifact is a document nothing had before: not the version
  // being previewed, and not the head as it stood before the unsaved typing.
  const known = new Set(before.map((row) => row.checkpointId));
  expect(known.has(kept.checkpointId)).toBe(false);
});
