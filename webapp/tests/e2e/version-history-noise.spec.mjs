// A gesture that changes nothing must produce no version row — driven through the
// real application, because that is the one thing two reading-based
// investigations could not establish.
//
// The owner reported this twice ("even in case of no changes why the fuck are you
// saving a version"), and both earlier passes stopped at `shouldCapture` and
// `skipIfUnchanged` looking correct. They are correct, in isolation. What was
// wrong was what they were asked ABOUT: a reason nothing had declared, and a
// comparison against the head rather than against the timeline. Neither is
// visible from reading one function, and both are obvious from three lines of
// instrumented log. So this file drives the gestures and reads the store the
// panel reads, and every assertion below is on rows that are really there.
//
// NON-VACUITY IS ASSERTED, not assumed. This repository has shipped a spec whose
// `not.toHaveText(/no results/i)` could never fail because the app says "No
// match" (SKILL §4), and a suppression test is the easiest possible place to make
// that mistake: a store that suppressed EVERYTHING would pass every "no new row"
// assertion in here. So each test also makes a real change and asserts the row
// for it DOES appear, and the counts are exact rather than upper bounds.
//
// `keyboard.insertText`, not `keyboard.type`: printable characters are
// `preventDefault`ed on this surface, so `type` would leave the document
// untouched and the whole file green over nothing (SKILL §4 again).
import {
  clickIntoFirstPage,
  expect,
  gotoEditor,
  moveCaretToDocStart,
  runFilePageCommand,
  saveDocument,
  test,
} from "./fixtures.mjs";

const panel = "#versionPanel";
const rows = "#versionPanelBody .version-item";

/** Every version row the store holds, oldest first, with the content hash that
 *  decides whether two of them are the same document.
 *
 *  Read from IndexedDB rather than from the panel because the defect is about
 *  rows EXISTING: a filter, a group heading or a repaint bug could hide one from
 *  the list, and "the panel shows two" is a weaker claim than "the store holds
 *  two". The head pointer comes from the same read, because half of this fix is
 *  that the head has to follow the bytes. */
function timeline(page) {
  return page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("opendoc-drafts", 3);
        open.onerror = () => reject(new Error(String(open.error)));
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction(["version_meta", "documents"], "readonly");
          const versions = tx.objectStore("version_meta").getAll();
          const lineages = tx.objectStore("documents").getAll();
          tx.onerror = () => reject(new Error(String(tx.error)));
          tx.oncomplete = () =>
            resolve({
              rows: versions.result
                .slice()
                .sort((a, b) => a.createdAt - b.createdAt)
                .map((row) => ({
                  versionId: row.versionId,
                  kind: row.kind,
                  checkpointId: row.checkpointId,
                  bytes: row.bytes,
                  revision: row.revision,
                })),
              heads: lineages.result.map((row) => row.headVersionId),
            });
        };
      }),
  );
}

/** Waits until the store holds exactly `count` versions, then returns them.
 *
 *  A capture rides the open and the save paths and is deliberately not awaited by
 *  either — a timeline is not what a reader is waiting for — so this polls rather
 *  than sleeping. Polling for an EXACT count is what makes it a real assertion:
 *  it fails on a count that is too high just as loudly as on one that is too low,
 *  which is the direction this whole file is about. */
async function expectVersions(page, count) {
  await expect
    .poll(async () => (await timeline(page)).rows.length, { timeout: 15_000 })
    .toBe(count);
  return timeline(page);
}

/** An edit that cancels itself out: the engine's revision watermark moves, the
 *  text does not. The suppression rules' own doc comment admits this case, and it
 *  is the shape every "I changed nothing" report reduces to. */
async function neutralEdit(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.insertText("x");
  await page.keyboard.press("Backspace");
}

/** A change that really is one. */
async function realEdit(page, marker) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.insertText(marker);
}

/** No two rows in the timeline hold the same document.
 *
 *  The CLASS assertion, not an instance of it: every defect this file was written
 *  for ends with two rows sharing a content hash, whichever gesture produced them,
 *  so this is the invariant and the per-gesture tests are the conditions that used
 *  to break it. */
function expectNoDuplicateContent(store) {
  const byHash = new Map();
  for (const row of store.rows) {
    const seen = byHash.get(row.checkpointId);
    expect(
      seen,
      `two version rows hold the same document (${row.kind} and ${seen?.kind}, ` +
        `checkpoint ${row.checkpointId}) — one of them is a version of nothing`,
    ).toBeUndefined();
    byHash.set(row.checkpointId, row);
  }
}

test("reopening a document you already saved adds no version row", async ({ page }) => {
  // THE MEASURED REPRODUCTION. Before the fix this ended with three rows, the
  // third byte-identical to the first:
  //
  //   import cp=fb07bd2d | saved cp=28c11967 | import cp=fb07bd2d
  //
  // and one more arrived on every reload, because the comparison only looked at
  // the head and the head was `saved`.
  await gotoEditor(page);
  await expectVersions(page, 1);
  await neutralEdit(page);
  await saveDocument(page);
  const saved = await expectVersions(page, 2);
  expectNoDuplicateContent(saved);
  const baseline = saved.rows[0];

  await gotoEditor(page);
  const reopened = await expectVersions(page, 2);
  expectNoDuplicateContent(reopened);
  // And the head followed the bytes: the document on screen is the file from
  // disk, which is what the import row holds, so that is the row the panel may
  // call current. Leaving the head on `saved` is the lie that made suppressing
  // the duplicate look wrong.
  expect(reopened.heads).toContain(baseline.versionId);

  // A second reload is still two rows: the fix is a rule, not a one-off.
  await gotoEditor(page);
  expectNoDuplicateContent(await expectVersions(page, 2));

  // NON-VACUITY. A store that suppressed everything would have passed every
  // assertion above, so a real change must still produce a row.
  await realEdit(page, "a genuine insertion");
  await saveDocument(page);
  const after = await expectVersions(page, 3);
  expectNoDuplicateContent(after);
});

test("a tab going away after an edit that cancels out adds no version row", async ({ page }) => {
  // `pagehide` is fired by the real browser on a tab close or a navigation, and
  // `main.js` binds it through `bindDraftFlushOnExit`. The reason string reached
  // version history as a reason `CAPTURE_REASON` had never declared, and the
  // instrumented run recorded what that cost:
  //
  //   [VH] capture(pagehide) accepted kind=auto
  //   [VH] captureNow reason=pagehide kind=auto suppress=false bytes=16384 ...
  //   [VH] -> status=history.recorded ok=true
  //
  // The event is dispatched at `window`, which is the shape the browser produces
  // and the shape the listener is bound to — unlike the IME spec SKILL §4 names,
  // which invented a shape no real input produces.
  await gotoEditor(page);
  await expectVersions(page, 1);
  await neutralEdit(page);
  await saveDocument(page);
  await expectVersions(page, 2);

  // Reopening leaves the capture interval unarmed (a suppressed capture
  // deliberately does not start it), which is the state the `pagehide` capture
  // got through in. Recreating the condition rather than waiting ten minutes for
  // it is the difference between a guard and a clock.
  await gotoEditor(page);
  await expectVersions(page, 2);

  await neutralEdit(page);
  await page.evaluate(() => window.dispatchEvent(new Event("pagehide")));
  const store = await expectVersions(page, 2);
  expectNoDuplicateContent(store);

  // NON-VACUITY: the same flush after a REAL edit does keep a version.
  await realEdit(page, "something worth keeping");
  await page.evaluate(() => window.dispatchEvent(new Event("pagehide")));
  expectNoDuplicateContent(await expectVersions(page, 3));
});

test("saving an untouched document says so rather than silently doing nothing", async ({
  page,
}) => {
  // SKILL §10: never a silent no-op. The suppression was right and its silence
  // was not — "no row appeared, work it out" is what the owner could not work
  // out. The sentence names the version the document already matches.
  await gotoEditor(page);
  await expectVersions(page, 1);
  await runFilePageCommand(page, "file.versionHistory");
  await expect(page.locator(panel)).toBeVisible();
  await expect(page.locator(rows)).toHaveCount(1);

  const note = page.locator("#versionPanelNote");
  await expect(note).toBeHidden();

  await saveDocument(page);
  // The real string from the catalogue, not a regex over a guess: a spec in this
  // repository once asserted `/no results/i` against an app that says "No match"
  // and could never fail.
  await expect(note).toBeVisible();
  await expect(note).toHaveText(/^No changes since .+, so no new version was kept\.$/);
  await expect(page.locator(rows)).toHaveCount(1);

  // And it does not outlive the next version that really is one.
  await realEdit(page, "now something changed");
  await saveDocument(page);
  await expect(page.locator(rows)).toHaveCount(2);
  await expect(note).toBeHidden();
});

test("two version rows do not read the same", async ({ page }) => {
  // The owner's second report: "still no improvement in versions". Every row read
  // `Saved · 16 KB`, so the panel answered when and never what, and a timeline
  // whose rows are indistinguishable is one a reader cannot use. The row now
  // carries what changed — the engine's edit count, the byte delta, the author —
  // all derived from stored metadata with no document parse.
  await gotoEditor(page);
  await expectVersions(page, 1);
  await realEdit(page, "first change");
  await saveDocument(page);
  await expectVersions(page, 2);
  await realEdit(page, "a second, larger change with more words in it");
  await saveDocument(page);
  await expectVersions(page, 3);

  await runFilePageCommand(page, "file.versionHistory");
  await expect(page.locator(panel)).toBeVisible();
  await expect(page.locator(rows)).toHaveCount(3);

  const details = await page.locator(`${rows} .version-item-detail`).allInnerTexts();
  expect(details).toHaveLength(3);
  for (const text of details) expect(text.trim().length).toBeGreaterThan(0);
  expect(
    new Set(details.map((text) => text.trim())).size,
    `all three rows read the same (${details.join(" / ")}) — a timeline whose rows ` +
      "cannot be told apart answers when and never what",
  ).toBe(3);
  // At least one row says how many edits went into it, which is the fact that is
  // actually about the work rather than about the file.
  expect(details.join(" ")).toMatch(/\d+ edits?/);

  // The accessible name carries it too: a sighted reader can now tell two rows
  // apart and a screen-reader reader could not.
  const names = await page.locator(`${rows} .version-item-entry`).evaluateAll((cells) =>
    cells.map((cell) => cell.getAttribute("aria-label") ?? ""),
  );
  expect(new Set(names).size).toBe(3);
  expect(names.join(" ")).toMatch(/\d+ edits?/);
});
