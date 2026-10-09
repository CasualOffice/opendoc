// Version history shows a version WITH ITS CHANGES, on the page (ADR-065).
//
// The owner's ask: "a diff canvas for version diff, to see the changes on that
// version — what anyone has removed or added or changed in position — just like
// Google Docs." Google paints a version's edits into the document in the
// editor's colour. So the guards are about the PAGE — saturated (author-coloured)
// ink, which black text cannot produce — about which text is shown as changed,
// and about the reader's own document being left alone.
//
// `keyboard.insertText`, never `keyboard.type`: printable characters are
// `preventDefault`ed on this editor, so `type` can leave a spec green over input
// that never arrived (SKILL §4).
import { test, expect, clickIntoFirstPage, mirrorBlocks, runAppMenuCommand, saveDocument } from "./fixtures.mjs";

const MOD = process.platform === "darwin" ? "Meta" : "Control";

function textFile(name, body) {
  return { name, mimeType: "text/plain", buffer: Buffer.from(body, "utf8") };
}

/** Author-coloured pixels on page one. Black text and its antialiasing move r,
 *  g and b together; every review author hue does not. */
async function saturatedInk(page) {
  await page.evaluate(() => document.fonts.ready);
  return page.evaluate(() => {
    const canvas = document
      .querySelector('#pages .page-wrap[data-page-number="1"]')
      ?.querySelector("canvas.page");
    if (!canvas) return 0;
    const { data } = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height);
    let saturated = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] < 128) continue;
      const max = Math.max(data[i], data[i + 1], data[i + 2]);
      const min = Math.min(data[i], data[i + 1], data[i + 2]);
      if (max - min > 48) saturated += 1;
    }
    return saturated;
  });
}

/** Three versions of a known plain-text document: the import baseline, one
 *  save that ADDED a sentence, and one save that REMOVED a paragraph. */
async function threeVersions(page) {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles(textFile("story.txt", "Alpha\nBeta goes away later\nGamma\n"));
  await expect(page.locator("#docTitle")).toHaveValue("story.txt");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  expect(await saturatedInk(page), "precondition: black text paints no author ink").toBe(0);

  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.insertText("Added in the middle version. ");
  await saveDocument(page);

  // Remove the whole second paragraph: select it from its start to the start of
  // the third and delete.
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Home");
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Delete");
  expect((await mirrorBlocks(page)).join(" ")).not.toContain("Beta goes away later");
  await saveDocument(page);

  await runAppMenuCommand(page, "file", "file.versionHistory");
  await expect(page.locator("#versionPanel")).toBeVisible();
  const rows = page.locator("#versionPanelBody .version-item");
  await expect(rows, "three versions: baseline, the addition, the removal").toHaveCount(3, {
    timeout: 45_000,
  });
  return rows;
}

const nav = (page) => page.locator("#versionPreviewChanges");

test("a version is shown with its changes painted in, and stepping through them names each one", async ({
  page,
  consoleErrors,
}) => {
  const rows = await threeVersions(page);
  const liveBefore = await mirrorBlocks(page);

  // The MIDDLE version: what it changed against its predecessor (the baseline)
  // is the added sentence — and only that.
  await rows.nth(1).locator(".version-item-entry").click();
  await expect(page.locator("#versionPreviewBanner")).toBeVisible({ timeout: 45_000 });
  await expect(nav(page)).toBeVisible({ timeout: 45_000 });
  await expect(nav(page)).toContainText(/Changes by/);
  await expect(nav(page).locator('.diff-legend-item[data-diff-kind="added"]')).toBeVisible();
  await expect(nav(page).locator('.diff-legend-item[data-diff-kind="removed"]')).toHaveCount(0);
  await expect(page.locator("#versionPreviewShowChanges")).toBeChecked();

  // ON THE PAGE, in the author's colour.
  await expect(async () => {
    expect(await saturatedInk(page)).toBeGreaterThan(0);
  }).toPass({ timeout: 45_000 });

  // Next steps to the one change and says what it is.
  await nav(page).locator(".diff-nav-step").last().click();
  await expect(nav(page).locator(".diff-nav-position")).toContainText(/1 of 1 · Added/);

  // THE SWITCH: off shows the version plain (no author ink, no navigator);
  // on brings the changes back.
  await page.locator("#versionPreviewShowChanges").uncheck();
  await expect(nav(page)).toBeHidden({ timeout: 45_000 });
  await expect(async () => {
    expect(await saturatedInk(page)).toBe(0);
  }).toPass({ timeout: 45_000 });
  await page.locator("#versionPreviewShowChanges").check();
  await expect(nav(page)).toBeVisible({ timeout: 45_000 });

  // BACK TO CURRENT: the reader's document is exactly as it was — no tracked
  // change was written into it by looking at the past.
  await page.locator("#versionPreviewBack").click();
  await expect(page.locator("#versionPreviewBanner")).toBeHidden();
  expect(await mirrorBlocks(page)).toEqual(liveBefore);
  await expect(page.locator("#reviewSidebar .review-margin-revision")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("the latest version shows the paragraph it REMOVED, struck where it was", async ({
  page,
  consoleErrors,
}) => {
  const rows = await threeVersions(page);
  // The HEAD: Google's "current version" view — what the latest save changed.
  // Its predecessor had "Beta goes away later"; the head does not, and that
  // whole paragraph is put back, struck, rather than listed as "not marked".
  await rows.first().locator(".version-item-entry").click();
  await expect(nav(page)).toBeVisible({ timeout: 45_000 });
  await expect(nav(page).locator('.diff-legend-item[data-diff-kind="removed"]')).toBeVisible();
  await expect(async () => {
    expect(await saturatedInk(page)).toBeGreaterThan(0);
  }).toPass({ timeout: 45_000 });
  // The head is the document already in place: nothing to restore.
  await expect(page.locator("#versionPreviewRestore")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("the earliest version has nothing before it, so it is shown plain and Show changes says why", async ({
  page,
  consoleErrors,
}) => {
  const rows = await threeVersions(page);
  await rows.last().locator(".version-item-entry").click();
  await expect(page.locator("#versionPreviewBanner")).toBeVisible({ timeout: 45_000 });
  await expect(nav(page)).toBeHidden();
  // The row menu's Show changes is disabled WITH ITS REASON (SKILL §10).
  await rows.last().locator(".version-item-menu").click();
  const changes = page.locator('#versionRowMenu [data-command-id="version.changes"]');
  await expect(changes).toBeDisabled();
  await expect(changes).toHaveAttribute("title", /earliest version/i);
  await page.keyboard.press("Escape");
  // And on the middle row it is live, and opens the preview with changes on.
  await rows.nth(1).locator(".version-item-menu").click();
  await expect(changes).toBeEnabled();
  await changes.click();
  await expect(nav(page)).toBeVisible({ timeout: 45_000 });
  expect(consoleErrors).toEqual([]);
});
