// A gesture that changes nothing must produce no version row — driven through the
// real application, because that is the one thing two reading-based
// investigations could not establish.
//
// The owner reported this three times ("even in case of no changes why the fuck
// are you saving a version"), and three reading passes stopped at `shouldCapture`
// and `skipIfUnchanged` looking correct. They are correct, in isolation. What was
// wrong was what they were asked ABOUT: a reason nothing had declared, a
// comparison against the head rather than against the timeline, and — the one
// this file got wrong itself — a comparison of the source-format BYTES where the
// question is about the DOCUMENT.
//
// THIS FILE WAS PART OF THE DEFECT. Its first version made a self-cancelling edit,
// saved, and asserted TWO version rows; its `expectNoDuplicateContent` passed
// because the two rows' byte hashes differ across export modes. A guard written
// to close the owner's report asserted the report as correct behaviour, which is
// SKILL §10's "assert the guarantee, not the mechanism" with a price on it. The
// counts below are now the guarantee: no changes, no new version.
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

/** Every version row the store holds, oldest first, with both identities it
 *  carries: `contentId`, the engine's digest of the DOCUMENT, and
 *  `checkpointId`, a SHA-256 of the source-format BYTES.
 *
 *  Both, because the difference between them is the defect. The byte hash is not
 *  an identity for a document: the import row holds the original file verbatim
 *  and `source_unchanged` is a monotonic watermark, so after any edit at all —
 *  including one immediately undone — every later checkpoint is a re-export with
 *  a different layout. Two hashes, one document.
 *
 *  Read from IndexedDB rather than from the panel because the defect is about
 *  rows EXISTING: a filter, a group heading or a repaint bug could hide one from
 *  the list, and "the panel shows one" is a weaker claim than "the store holds
 *  one". The head pointer comes from the same read, because half of this fix is
 *  that the head has to follow the document. */
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
                  contentId: row.contentId ?? "",
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

/** No two rows in the timeline hold the same DOCUMENT.
 *
 *  The CLASS assertion, not an instance of it: every defect this file was written
 *  for ends with two rows holding one document, whichever gesture produced them,
 *  so this is the invariant and the per-gesture tests are the conditions that used
 *  to break it.
 *
 *  It asserts the GUARANTEE, which is the whole point of the rewrite. The version
 *  of this helper that shipped with the first attempt at this fix compared
 *  `checkpointId` — the source BYTES — and so it passed on two rows holding one
 *  document, because the two export modes that produced them serialize the same
 *  document differently. It was asserting the mechanism, and the mechanism was the
 *  bug, so the guard written to close the owner's report encoded the report as
 *  correct behaviour (SKILL §10: assert the guarantee, not the mechanism).
 *
 *  BOTH identities are now checked, which is strictly stronger than either:
 *  two rows are the same document if they share the engine's content digest, and
 *  byte-identical artifacts are the same document too. A row that carries no
 *  digest — one written before the field existed — still gets the byte test. */
function expectNoDuplicateContent(store) {
  const byContent = new Map();
  const byBytes = new Map();
  for (const row of store.rows) {
    if (row.contentId) {
      const seen = byContent.get(row.contentId);
      expect(
        seen,
        `two version rows hold the same document (${row.kind} and ${seen?.kind}, ` +
          `content ${row.contentId}) — one of them is a version of nothing`,
      ).toBeUndefined();
      byContent.set(row.contentId, row);
    }
    const seenBytes = byBytes.get(row.checkpointId);
    expect(
      seenBytes,
      `two version rows hold byte-identical artifacts (${row.kind} and ${seenBytes?.kind}, ` +
        `checkpoint ${row.checkpointId}) — one of them is a version of nothing`,
    ).toBeUndefined();
    byBytes.set(row.checkpointId, row);
  }
}

/** Every row carries the engine's content digest.
 *
 *  NON-VACUITY FOR THE IDENTITY ITSELF. `captureVersion` falls back to the byte
 *  hash when no digest is supplied, which is the right failure mode (over-keeping
 *  a version is recoverable; dropping one is not) and a terrible silent one: the
 *  defect would be back and every count assertion in this file would still pass
 *  on a store that was simply never asked for a digest. So the digest's PRESENCE
 *  is asserted, not just its effect. */
function expectContentIdentities(store) {
  for (const row of store.rows) {
    expect(
      row.contentId,
      `the ${row.kind} row carries no content identity, so its suppression fell back to ` +
        "comparing source bytes — the comparison this whole file exists to replace",
    ).toMatch(/^cid\d+-[0-9a-f]{32}$/);
  }
}

test("an edit that cancels itself out leaves one version, not two", async ({ page }) => {
  // THE OWNER'S REPORT, STATED AS A COUNT. "Even in case of no changes why the
  // fuck are you saving a version", three times.
  //
  // This assertion used to read `expectVersions(page, 2)` — a comment admitting
  // the document had not changed, immediately above a guard asserting that two
  // rows was correct. Its own recorded output named the two hashes:
  //
  //   import cp=fb07bd2d | saved cp=28c11967
  //
  // Two hashes, one document. The bytes differ because the import row is the
  // original file verbatim and the saved row is a re-export — `source_unchanged`
  // is `revision == 0`, a monotonic watermark, so the exact export mode is
  // permanently unavailable after the first edit and never comes back when that
  // edit is undone. Dedupe now compares the document, so there is ONE row.
  await gotoEditor(page);
  const opened = await expectVersions(page, 1);
  const baseline = opened.rows[0];

  // THE ASSERTION THE OWNER HAS BEEN ASKING FOR. Deliberately before every other
  // claim in this file, so that when it fails it fails on the count and the
  // failure reads "expected 1, received 2" rather than on some corollary.
  await neutralEdit(page);
  await saveDocument(page);
  const saved = await expectVersions(page, 1);
  expect(
    saved.rows[0].versionId,
    "and it is the SAME row: nothing was written, so the import row still stands",
  ).toBe(baseline.versionId);
  expectNoDuplicateContent(saved);
  expectContentIdentities(saved);

  // THE MEASURED REPRODUCTION of the second half. Before the first attempt at
  // this fix, a reload after a save added a third row byte-identical to the
  // first, forever, because the comparison only looked at the head.
  await gotoEditor(page);
  const reopened = await expectVersions(page, 1);
  expectNoDuplicateContent(reopened);
  // And the head followed the document: the one row holds what is on screen, so
  // that is the row the panel may call current.
  expect(reopened.heads).toContain(baseline.versionId);

  // A second reload is still one row: the fix is a rule, not a one-off.
  await gotoEditor(page);
  expectNoDuplicateContent(await expectVersions(page, 1));

  // NON-VACUITY. A store that suppressed everything would have passed every
  // assertion above, so a real change must still produce a row.
  await realEdit(page, "a genuine insertion");
  await saveDocument(page);
  const after = await expectVersions(page, 2);
  expectNoDuplicateContent(after);
  expectContentIdentities(after);
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
  await expectVersions(page, 1);

  // Reopening leaves the capture interval unarmed (a suppressed capture
  // deliberately does not start it), which is the state the `pagehide` capture
  // got through in. Recreating the condition rather than waiting ten minutes for
  // it is the difference between a guard and a clock.
  await gotoEditor(page);
  await expectVersions(page, 1);

  await neutralEdit(page);
  await page.evaluate(() => window.dispatchEvent(new Event("pagehide")));
  const store = await expectVersions(page, 1);
  expectNoDuplicateContent(store);
  expectContentIdentities(store);

  // NON-VACUITY: the same flush after a REAL edit does keep a version.
  await realEdit(page, "something worth keeping");
  await page.evaluate(() => window.dispatchEvent(new Event("pagehide")));
  expectNoDuplicateContent(await expectVersions(page, 2));
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
