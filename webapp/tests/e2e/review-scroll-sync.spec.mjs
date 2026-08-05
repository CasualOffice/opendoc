// Review-sidebar scroll-sync / card-anchoring for a CLUSTER of review items
// (docs/81 REVIEW-GAP-019). When several comments and tracked changes sit close
// together, selecting one of them — e.g. the last suggestion in the cluster —
// must (1) keep that item's own on-canvas marker in view (never overshoot so the
// paragraph leaves the viewport, never fail to reveal an off-screen item) and
// (2) keep the selected card locked to ITS OWN marker as the canvas scrolls, with
// the other cards stacking around it. Before the fix, a later item in a dense
// cluster was pushed hundreds of px below the change it points at (a plain
// top-down destack can only push the active card DOWN), and a range selection
// paints a highlight, not a caret, so the old `scrollCaretIntoView` never moved
// an off-screen marker into view at all.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

async function settle(page) {
  await page.evaluate(
    () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
  );
}

async function pastePlainText(page, text) {
  await page.evaluate((value) => {
    const data = new DataTransfer();
    data.setData("text/plain", value);
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, text);
}

// Refocuses the canvas and puts the caret at the start of a cluster line,
// counted UP from the trailing empty paragraph ([…][PARAONE][PARATWO][empty]):
// linesUp 2 = PARAONE, 1 = PARATWO. Robust to leading document content and to
// prior comment/insertion edits (they never change the line count).
async function caretToClusterLine(page, linesUp) {
  await clickIntoFirstPage(page); // focus the editor surface
  await page.keyboard.press(`${MOD}+End`);
  for (let i = 0; i < linesUp; i++) await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Home");
}

async function commentWholeLine(page, note) {
  await page.keyboard.press("Shift+End");
  await page.locator("#selComment").click();
  const composer = page.locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill(note);
  await page.locator('[data-testid="review-comment-submit"]').click();
  await expect(composer).toHaveCount(0);
}

// Builds a tight cluster of four review items — two comments and two tracked
// insertions — on two adjacent single-word paragraphs at the END of the document
// (so there is ample room above the cluster; a cluster jammed against the very
// top of the document cannot be perfectly aligned by anyone). The engine anchors
// a comment to editable top-level text and allows one per paragraph, so the two
// comments live on two adjacent lines; the four cards still collide in the
// sidebar — exactly the clustered condition the bug is about. Returns the sidebar
// item id of the LAST suggestion.
async function buildCluster(page) {
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.type("PARAONE");
  await page.keyboard.press("Enter");
  await page.keyboard.type("PARATWO");
  await page.keyboard.press("Enter");

  await caretToClusterLine(page, 2); // PARAONE
  await commentWholeLine(page, "first note");
  await caretToClusterLine(page, 1); // PARATWO
  await commentWholeLine(page, "second note");

  await setReviewMode(page, "suggesting");
  await caretToClusterLine(page, 2); // PARAONE
  await page.keyboard.press("End");
  await pastePlainText(page, " SUGGONE");
  await caretToClusterLine(page, 1); // PARATWO
  await page.keyboard.press("End");
  await pastePlainText(page, " SUGGTWO");
  await settle(page);

  const insertions = page.locator("#reviewSidebar .review-margin-card.review-margin-insertion");
  await expect(insertions).toHaveCount(2);
  await expect(page.locator("#reviewSidebar .review-margin-card.review-margin-comment")).toHaveCount(2);
  const n = await insertions.count();
  return insertions.nth(n - 1).getAttribute("data-review-item-id");
}

// Selects a sidebar card WITHOUT Playwright's actionability auto-scroll (which
// would itself move the viewport and mask the app's own scroll behaviour).
async function selectCard(page, itemId) {
  await page.evaluate((id) => {
    document
      .querySelector(`[data-review-item-id="${CSS.escape(id)}"]`)
      .dispatchEvent(new MouseEvent("click", { bubbles: true }));
  }, itemId);
  await settle(page);
}

function deselect(page) {
  return page.evaluate(() =>
    document
      .getElementById("reviewSidebarBody")
      .dispatchEvent(new MouseEvent("click", { bubbles: true })),
  );
}

// Geometry of the selected item and its own on-canvas marker. A review target
// paints its selection as a highlight, so the highlight IS that item's marker on
// the canvas; the card must stay aligned to it.
async function geometry(page, itemId) {
  return page.evaluate((id) => {
    const vp = document.getElementById("viewport").getBoundingClientRect();
    const card = document.querySelector(`[data-review-item-id="${CSS.escape(id)}"]`);
    const hl = [...document.querySelectorAll(".overlay .highlight")];
    if (!card || !hl.length) return null;
    const cardTop = card.getBoundingClientRect().top;
    const markerTop = Math.min(...hl.map((h) => h.getBoundingClientRect().top));
    const markerBottom = Math.max(...hl.map((h) => h.getBoundingClientRect().bottom));
    return {
      gap: Math.abs(cardTop - markerTop),
      markerVisible: markerTop >= vp.top && markerBottom <= vp.bottom,
      expanded: card.getAttribute("aria-expanded") === "true",
    };
  }, itemId);
}

function scrollViewport(page, top) {
  return page.evaluate((t) => {
    const v = document.getElementById("viewport");
    v.scrollTop = Math.max(0, Math.min(t, v.scrollHeight - v.clientHeight));
    return v.scrollTop;
  }, top);
}

const ALIGN_TOL = 28; // baseline card→marker offset is ~13px; the bug produced 240+.

test("the selected card stays locked to its own marker in a clustered paragraph, before and after scroll (REVIEW-GAP-019)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const lastId = await buildCluster(page);

  // Bring the cluster (at the document end) fully into view.
  await scrollViewport(page, 1e9);
  await settle(page);

  // Select the LAST suggestion in the cluster.
  await selectCard(page, lastId);

  // It is the active/expanded card, its marker is visible, and the card top is
  // aligned to that marker (not pushed far down the stack).
  const afterSelect = await geometry(page, lastId);
  expect(afterSelect).not.toBeNull();
  expect(afterSelect.expanded).toBe(true);
  expect(afterSelect.markerVisible).toBe(true);
  expect(afterSelect.gap).toBeLessThan(ALIGN_TOL);

  // Manually scroll the canvas up a little: the card and its marker move
  // together, staying locked (they no longer diverge / drift the wrong way).
  await scrollViewport(page, (await page.evaluate(() => document.getElementById("viewport").scrollTop)) - 120);
  await settle(page);
  const afterScroll = await geometry(page, lastId);
  expect(afterScroll.gap).toBeLessThan(ALIGN_TOL);
  // Alignment is preserved across the scroll (no desync).
  expect(Math.abs(afterScroll.gap - afterSelect.gap)).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});

test("selecting a clustered item whose marker is off-screen scrolls that marker into view (not the paragraph out of it)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const lastId = await buildCluster(page);

  // Scroll so the cluster (at the document end) is well below the viewport —
  // its markers off-screen — with nothing selected.
  await deselect(page);
  await settle(page);
  await scrollViewport(page, 0);
  await settle(page);
  const before = await page.evaluate((id) => {
    const vp = document.getElementById("viewport").getBoundingClientRect();
    const hl = [...document.querySelectorAll(".overlay .highlight")];
    if (!hl.length) return { markerVisible: false };
    const markerBottom = Math.max(...hl.map((h) => h.getBoundingClientRect().bottom));
    const markerTop = Math.min(...hl.map((h) => h.getBoundingClientRect().top));
    return { markerVisible: markerTop >= vp.top && markerBottom <= vp.bottom };
  }, lastId);
  expect(before.markerVisible).toBe(false); // precondition: off-screen

  // Selecting the item brings ITS OWN marker into view and lands the card on it.
  await selectCard(page, lastId);
  const after = await geometry(page, lastId);
  expect(after.markerVisible).toBe(true);
  expect(after.gap).toBeLessThan(ALIGN_TOL);

  expect(consoleErrors).toEqual([]);
});
