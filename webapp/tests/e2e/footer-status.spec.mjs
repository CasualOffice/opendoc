// The status bar gives the message the room to be read (`109` UX-043).
//
// Measured before this change at 1280x800 with the sample open: the access
// indicator painted "Full access on this device, where you are the only
// authority" across 357px of the row, the counts kept every pixel of theirs, and
// the engine's own message — the one thing on the row that is news — got 0-106px
// and read "Cou…", "Fetc…", "Draf…". A message nobody can read is a message the
// editor did not send.
//
// Three guarantees, each asserted against a REAL message (Viewing mode's refusal,
// 66 characters, set by the editor itself rather than written into the DOM):
//
//   1. at 1280x800 a message that long is painted whole, and any count that gave
//      way for it gave way WHOLE — no cut digit;
//   2. the access indicator states the level and carries the sentence in its
//      tooltip and accessible name, not across the row;
//   3. a message longer than the half it lives in ends in an ellipsis and puts its
//      full text on hover.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  stableBox,
} from "./fixtures.mjs";

const REFUSAL = "Viewing mode is read-only; switch to Editing to change the document";

async function provokeRefusal(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "viewing");
  await clickIntoFirstPage(page);
  await page.keyboard.type("x");
  await expect(page.locator("#status")).toHaveText(REFUSAL);
}

/** The status line's painted width against its text's, and every count that is
 *  only PARTLY inside the counts' box — the cut-digit case. */
const footerGeometry = () => {
  const status = document.getElementById("status");
  const stats = document.getElementById("stats");
  const box = stats.getBoundingClientRect();
  const cut = [...stats.children]
    .filter((count) => count.getClientRects().length > 0)
    .filter((count) => {
      const r = count.getBoundingClientRect();
      const inside = r.left >= box.left - 1 && r.right <= box.right + 1 && r.top >= box.top - 1 && r.bottom <= box.bottom + 1;
      const shedWhole = r.top >= box.bottom - 1 || r.left >= box.right - 1;
      return !inside && !shedWhole;
    })
    .map((count) => count.textContent);
  return {
    painted: status.clientWidth,
    needs: status.scrollWidth,
    title: status.getAttribute("title"),
    cut,
    words: (() => {
      const w = document.getElementById("statWords").getBoundingClientRect();
      return w.top < box.bottom - 1 && w.right <= box.right + 1;
    })(),
  };
};

test("at 1280x800 a 66-character message is painted whole, and the counts give way whole", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await provokeRefusal(page);
  const geometry = await page.evaluate(footerGeometry);
  expect(
    geometry.needs,
    `the message needs ${geometry.needs}px and is painted in ${geometry.painted}px`,
  ).toBeLessThanOrEqual(geometry.painted + 1);
  expect(geometry.cut, "a count was cut part-way rather than shed").toEqual([]);
  // The word count is the last to go, and at this width it stays.
  expect(geometry.words, "the word count was shed at 1280px").toBe(true);
  // Untruncated, so no tooltip repeating it.
  expect(geometry.title).toBeNull();
});

test("the access indicator says the level, and the sentence is in its tooltip and name", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  const badge = page.locator("#accessBadge");
  await expect(badge).toBeVisible();
  await expect(page.locator("#accessBadgeLevel")).toBeVisible();
  await expect(page.locator("#accessBadgeLevel")).toHaveText("Full access");
  // The attribution is not painted across the row…
  await expect(page.locator("#accessBadgeSource")).toBeHidden();
  const { width } = await stableBox(badge);
  expect(width, `the indicator takes ${width}px of the status bar`).toBeLessThan(120);
  // …and is not lost: a sighted reader hovers for it, a screen reader hears it.
  await expect(badge).toHaveAttribute("title", /Full access.*on this device, where you are the only authority/);
  await expect(badge).toHaveAttribute("aria-label", /Full access.*on this device/);
});

test("a message longer than its half ends in an ellipsis and carries its full text on hover", async ({
  page,
}) => {
  // 900px is wide enough to keep the informational half and too narrow for the
  // whole refusal beside the right half's controls.
  await page.setViewportSize({ width: 900, height: 800 });
  await gotoEditor(page);
  await provokeRefusal(page);
  const geometry = await page.evaluate(footerGeometry);
  expect(geometry.needs, "the message was expected to be truncated at 900px").toBeGreaterThan(
    geometry.painted + 1,
  );
  expect(geometry.cut).toEqual([]);
  await expect(page.locator("#status")).toHaveAttribute("title", REFUSAL);
  // And it goes again when the line has room — a tooltip that only repeats what
  // is already on screen is noise.
  await page.setViewportSize({ width: 1440, height: 800 });
  await expect(page.locator("#status")).not.toHaveAttribute("title", /./);
});
