// A hyperlink on a picture, in the browser.
//
// `docs/109` HF-179. `a:hlinkClick` was dropped on import, so the owner's
// Medical Incident Report Form opened as a picture of a clickable form: four
// linked images, none of them clickable, and the link lost on save.
//
// The engine reports a picture link through the SAME `linkAt` a text link uses,
// which is why the hover feedback and the link chip here are the editor's
// existing ones and not a second implementation. This asserts that routing
// actually reaches the surface — modelling a link nothing surfaces would be the
// whole feature missing.
import { test, expect, gotoEditor } from "./fixtures.mjs";

const LINKED = "../fixtures/generated/picture-hyperlink.docx";

/** Clicks across the page until the editor offers a link, and returns the point.
 *
 *  Driven from what the editor SAYS rather than from a guessed fraction of the
 *  page: the fixture's group children are a few hundred twips across, and an
 *  eyeballed point lands between them — which reads as "no link" and would make
 *  this pass or fail for the wrong reason.
 *
 *  One click per point, never two: a second click inside a group descends into
 *  it, which is a different gesture with its own assertion below. */
async function clickUntilLinkOffered(page) {
  const sheet = page.locator('.page-wrap[data-page-number="1"] .page');
  const box = await sheet.boundingBox();
  for (let fy = 0.03; fy < 0.7; fy += 0.015) {
    for (let fx = 0.05; fx < 0.95; fx += 0.03) {
      const point = { x: box.x + box.width * fx, y: box.y + box.height * fy };
      await page.mouse.click(point.x, point.y);
      if (await page.locator("#linkChip").isVisible()) return point;
    }
  }
  return null;
}

test("a linked picture offers its target the way a linked word does", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(LINKED);
  await expect(page.locator("#a11yDocument")).toContainText("Linked picture");

  const point = await clickUntilLinkOffered(page);
  expect(point, "clicking a linked picture must offer its target").not.toBeNull();
  await expect(page.locator("#linkChipTarget")).toContainText("example.org");
  // …and the object is still the selection: the chip is an addition, not a
  // replacement for the handles the same click put up.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", /.+/);

  expect(consoleErrors).toEqual([]);
});
