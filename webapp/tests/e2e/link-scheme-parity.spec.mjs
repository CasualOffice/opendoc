// HF-019 (docs/104-HOTFIX-TRACKER.md, cluster T-03): a link target arrives from
// an imported file, a pasted fragment or the link dialog, and can be followed
// from three surfaces — the hover chip's Open, a click on the painted text, and
// the right-click "Open link". The http/https/mailto allowlist lived inside
// `activateLink` only, so the context menu handed `javascript:` straight to
// `window.open` and leaked the referrer while doing it. Both surfaces now
// resolve through `resolveExternalTarget`, so this spec drives the SAME hostile
// link from both and requires them to agree.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

// Records every `window.open` the page attempts instead of performing it, so a
// bypass shows up as a recorded target rather than as a popup the test would
// have to chase.
async function recordWindowOpen(page) {
  await page.addInitScript(() => {
    window.__openedTargets = [];
    window.open = (url) => {
      window.__openedTargets.push(String(url));
      return null;
    };
  });
}

const openedTargets = (page) => page.evaluate(() => window.__openedTargets);

// Turns the first eight characters of the document into a link to `url`.
async function linkFirstWord(page, url) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 8; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press(`${MOD}+k`);
  await expect(page.locator("#linkDialog")).toBeVisible();
  await page.locator("#linkUrlInput").fill(url);
  await page.locator("#linkApplyBtn").click();
  await expect(page.locator("#linkDialog")).toBeHidden();
}

// The screen point two characters into the linked range. Both surfaces are then
// driven from that one point: a left click raises the hover chip, a right click
// raises the context menu, and neither depends on where focus happens to be
// after the previous interaction.
async function pointInsideLink(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  const box = await page.locator(".overlay .caret").first().boundingBox();
  return { x: box.x + 2, y: box.y + box.height / 2 };
}

test("a javascript: target is refused identically by the link chip and the context menu", async ({
  page,
  consoleErrors,
}) => {
  await recordWindowOpen(page);
  await gotoEditor(page);
  await linkFirstWord(page, "javascript:alert(1)");

  const point = await pointInsideLink(page);

  // Surface 1 — the chip's Open button (the path that always enforced the list).
  await page.mouse.click(point.x, point.y);
  await expect(page.locator("#linkChip")).toBeVisible();
  await page.locator("#linkChipAction").click();
  await expect(page.locator("#status")).toContainText("Blocked javascript: link scheme");
  expect(await openedTargets(page)).toEqual([]);

  // Surface 2 — right-click ▸ Open link. This is the one that used to call
  // window.open unchecked; it must refuse in exactly the same words.
  await page.locator("#status").evaluate((el) => (el.textContent = ""));
  await page.mouse.click(point.x, point.y, { button: "right" });
  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  await menu.locator('[data-command-id="link.open"]').click();
  await expect(page.locator("#status")).toContainText("Blocked javascript: link scheme");
  expect(await openedTargets(page)).toEqual([]);

  expect(consoleErrors).toEqual([]);
});

test("an http target still opens from both surfaces, with noopener and noreferrer", async ({
  page,
  consoleErrors,
}) => {
  // The allowlist has to stay a filter, not a wall: the same two surfaces must
  // still follow an ordinary link, and both must withhold the referrer (the
  // context-menu path used to pass "noopener" alone).
  await page.addInitScript(() => {
    window.__openCalls = [];
    window.open = (url, target, features) => {
      window.__openCalls.push({ url: String(url), target: String(target), features: String(features) });
      return null;
    };
  });
  await gotoEditor(page);
  await linkFirstWord(page, "https://example.org/doc");

  const point = await pointInsideLink(page);

  await page.mouse.click(point.x, point.y);
  await expect(page.locator("#linkChip")).toBeVisible();
  await page.locator("#linkChipAction").click();

  await page.mouse.click(point.x, point.y, { button: "right" });
  await expect(page.locator(".editor-context-menu")).toBeVisible();
  await page.locator('.editor-context-menu [data-command-id="link.open"]').click();

  const calls = await page.evaluate(() => window.__openCalls);
  expect(calls).toHaveLength(2);
  for (const call of calls) {
    expect(call.url).toBe("https://example.org/doc");
    expect(call.target).toBe("_blank");
    expect(call.features).toContain("noopener");
    expect(call.features).toContain("noreferrer");
  }

  expect(consoleErrors).toEqual([]);
});
