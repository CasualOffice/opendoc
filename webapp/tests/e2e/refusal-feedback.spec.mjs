// `109` UX-017 — the editor must be able to say something when it refuses, at
// every width, on screen and to a screen reader.
//
// The defect: the only feedback channel was painted text in `.foot-left`, and
// BOTH live regions were inside it. `.foot-left` is `display: none` below 620px,
// and a `display: none` subtree is not in the accessibility tree, so below that
// width a refused edit was invisible and unspeakable at once — the editor did
// nothing and said nothing.
//
// These tests assert the GUARANTEE, not the mechanism. "Is there a toast element"
// would pass over a card with `opacity: 0`, and "does the region have aria-live"
// passed for months while the region was being pruned. So the question asked here
// is the one a user asks: can the refusal be SEEN, and is it in the tree a screen
// reader reads — which is Chromium's own, fetched over CDP, not Playwright's
// serializer.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  expectEditorFocused,
  runAppMenuCommand,
  setReviewMode,
} from "./fixtures.mjs";

const PHONE = { width: 390, height: 800 };
const DESKTOP = { width: 1280, height: 860 };

/** How many nodes of Chromium's accessibility tree carry `text`.
 *
 *  This is the measurement the row turned on: before the fix it was 4 at desktop
 *  width and 0 at phone width. Anything a screen reader can reach is in here;
 *  anything pruned by `display: none` or `aria-hidden` is not. */
async function axNodesCarrying(page, text) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Accessibility.enable");
  const { nodes } = await cdp.send("Accessibility.getFullAXTree");
  return nodes.filter((node) => JSON.stringify(node).includes(text)).length;
}

/** Every element that is actually PAINTING `text` at a size a person could read.
 *
 *  The size floor is not arbitrary and it is not a mechanism check: an off-screen
 *  live region is a real element with real text and a 1x1 clipped box, so "area
 *  greater than zero" would count the announcer as something the user can see and
 *  this whole file would pass while the screen stayed blank. A box narrower than
 *  24px or shorter than 12px cannot show a word at any font size the editor uses. */
function paintedCarriers(page, text) {
  return page.evaluate((needle) => {
    const out = [];
    for (const el of document.querySelectorAll("body *")) {
      if (el.children.length) continue;
      if (!el.textContent.includes(needle)) continue;
      const r = el.getBoundingClientRect();
      if (r.width < 24 || r.height < 12) continue;
      if (getComputedStyle(el).visibility === "hidden") continue;
      out.push({ id: el.id, area: Math.round(r.width * r.height) });
    }
    return out;
  }, text);
}

/** Drives Viewing mode into refusing a keystroke, and returns the message. */
async function refuseAnEdit(page) {
  await clickIntoFirstPage(page);
  await setReviewMode(page, "viewing");
  await clickIntoFirstPage(page);
  await page.keyboard.type("X");
  await expect(page.locator("#status")).toContainText("read-only");
  return page.locator("#status").textContent();
}

test("a refusal at phone width is both seen and announced", async ({ page, consoleErrors }) => {
  await page.setViewportSize(PHONE);
  await gotoEditor(page);
  const message = await refuseAnEdit(page);

  // The premise: the status line itself paints nothing at this width. If this
  // ever stops being true the rest of the test is measuring the wrong thing, so
  // it is asserted rather than assumed (UX-019 may one day give the footer a
  // narrow layout, and this test must fail loudly and be rewritten, not quietly
  // pass for the wrong reason).
  const statusArea = await page
    .locator("#status")
    .evaluate((el) => {
      const r = el.getBoundingClientRect();
      return r.width * r.height;
    });
  expect(
    statusArea,
    "the status line is expected to paint nothing at 390px; if it now does, this test's premise is stale",
  ).toBe(0);

  // SEEN: something with a non-zero painted box carries the refusal.
  const painted = await paintedCarriers(page, message);
  expect(
    painted.length,
    `nothing on screen carries "${message}" at 390px — the editor refused silently`,
  ).toBeGreaterThan(0);

  // ANNOUNCED: the refusal is in the accessibility tree. This was 0 before.
  expect(
    await axNodesCarrying(page, message),
    `"${message}" is not in Chromium's accessibility tree at 390px — the editor refused inaudibly`,
  ).toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});

test("a refusal at desktop width is escalated above the footer line", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  const message = await refuseAnEdit(page);

  // Two surfaces, not one: the footer line AND something larger. A refusal is
  // where the user's action did not happen, and a 12px line in the footer is
  // where an answer is available rather than where it is noticed — which is what
  // Word and ONLYOFFICE both answer with a modal
  // (`documenteditor/main/app/controller/Toolbar.js:1521`) and Google Docs with a
  // snackbar.
  const painted = await paintedCarriers(page, message);
  expect(
    painted.length,
    "a refusal must reach more than the footer line at desktop width",
  ).toBeGreaterThan(1);

  expect(await axNodesCarrying(page, message)).toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});

test("a confirmation does not throw a card over the document when the line is showing it", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#modeCompact").click();
  await expect(page.locator("#status")).toContainText("Compact toolbar");

  // The rule has to cut both ways or it is just "always show a card", and a card
  // for every toolbar press would be worse than the defect it fixes.
  const painted = await paintedCarriers(page, "Compact toolbar");
  expect(
    painted.map((p) => p.id),
    "a confirmation the status line is already showing must not be escalated",
  ).toEqual(["status"]);

  expect(consoleErrors).toEqual([]);
});

test("a refusal is not destroyed by the informational message that follows it", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Review ▸ Accept-next with the caret outside a tracked change publishes TWO
  // messages in one activation: the refusal, then "no comments or tracked
  // changes" from the advance that follows. The refusal is the one that matters
  // and it was being wiped out in the same tick — the user saw it flash.
  await runAppMenuCommand(page, "review", "review.acceptNext");

  const toast = page.locator("#statusToast");
  await expect(toast).not.toBeHidden();
  await expect(toast).toContainText("Place the caret inside a tracked change");
  // And it survives: still there after the activation is long over, rather than
  // being yanked away by the message that came second.
  await page.waitForTimeout(600);
  await expect(toast).toContainText("Place the caret inside a tracked change");
  expect(consoleErrors).toEqual([]);
});

test("the escalated surface can never take a click or a Tab stop from the document", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await refuseAnEdit(page);

  // A feedback surface that steals input is a worse defect than the one it fixes:
  // it would sit over the document, swallow the next click, and add a Tab stop
  // between the chrome and the page. It is also `aria-hidden`, because the live
  // region is the single thing that speaks — two announcers means every refusal
  // is read out twice.
  const surface = await page.evaluate(() => {
    const el = [...document.querySelectorAll("body > *")].find(
      (candidate) =>
        getComputedStyle(candidate).position === "fixed" &&
        !candidate.hidden &&
        candidate.getAttribute("aria-hidden") === "true" &&
        candidate.textContent.trim().length > 0,
    );
    if (!el) return null;
    const r = el.getBoundingClientRect();
    // The question a click asks the page, asked directly: what is at this point?
    // `elementFromPoint` honours `pointer-events`, so if the card is transparent
    // to input this returns whatever is underneath it instead.
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return {
      id: el.id,
      swallowsClicksAtItsCentre: el === hit || el.contains(hit),
      focusable: el.querySelectorAll("a[href],button,input,select,textarea,[tabindex]").length,
      tabbable: el.matches("[tabindex]"),
    };
  });
  expect(surface, "the escalated surface is not on screen to be checked").toBeTruthy();
  expect(
    surface.swallowsClicksAtItsCentre,
    `${surface.id} is over the document and takes the click — the feedback surface would eat the next thing the user does`,
  ).toBe(false);
  expect(surface.focusable).toBe(0);
  expect(surface.tabbable).toBe(false);

  // And the editing surface still holds focus: being told why an edit was refused
  // must not cost the user their caret. Asserted through the shared contract
  // rather than against one element id, per `tests/focus_contract.test.mjs`.
  await expectEditorFocused(page);
  expect(consoleErrors).toEqual([]);
});
