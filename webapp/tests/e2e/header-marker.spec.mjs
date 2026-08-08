// The header/footer marker: making an absent header discoverable.
//
// Word and Docs rely on a bare double-click in the margin, which is invisible to
// anyone who has not been told about it — and an ABSENT header has nothing to
// double-click on, so the capability is unreachable without knowing the command
// exists. LibreOffice Writer raises a marker with a `+` when the pointer is in
// the band, and that is what docs/85 §8d adopted. It is the same
// one-surface-only reachability failure the command-surface audit kept finding.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

// Hovers a point a fixed distance inside the page's top or bottom edge. A
// fraction of the page height is a poor target here: the band's size comes from
// the document's own margins, so the fraction that lands inside it depends on
// the page geometry rather than on anything the test controls.
async function hoverBand(page, edge) {
  const canvas = page.locator(".page-wrap .page").first();
  await canvas.evaluate((el, e) => el.scrollIntoView({ block: e === "bottom" ? "end" : "start" }), edge);
  await page.waitForTimeout(120);
  const box = await canvas.boundingBox();
  const y = edge === "bottom" ? box.y + box.height - 10 : box.y + 10;
  await page.mouse.move(box.x + box.width * 0.5, y);
  return box;
}

// The middle of the page, which is body text and never a band.
async function hoverBody(page) {
  const canvas = page.locator(".page-wrap .page").first();
  await canvas.evaluate((el) => el.scrollIntoView({ block: "start" }));
  await page.waitForTimeout(120);
  const box = await canvas.boundingBox();
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.4);
}

const marker = (page) => page.locator(".running-marker");

test("the marker appears in the top margin and names the header", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await hoverBand(page, "top");
  await expect(marker(page)).toBeVisible();
  await expect(marker(page)).toHaveText("+ Header");
  // Labelled for assistive technology, not just visually.
  await expect(marker(page)).toHaveAttribute("aria-label", "Edit the page header");

  expect(consoleErrors).toEqual([]);
});

test("the marker names the footer in the bottom margin, and hides over the body", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await hoverBand(page, "bottom");
  await expect(marker(page)).toBeVisible();
  await expect(marker(page)).toHaveText("+ Footer");

  // The body is not a band: the marker must not hover over ordinary text.
  await hoverBody(page);
  await expect(marker(page)).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("clicking the marker enters the header, creating it if there is none", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page); // the rich fixture has no running content
  await clickIntoFirstPage(page);
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await hoverBand(page, "top");
  await marker(page).click();

  await expect.poll(() => page.locator("#pages").getAttribute("data-running-edit")).toBe("header");
  await page.keyboard.type("VIAMARKER");

  // It went into the header, not the body.
  await expect(page.locator("#undoBtn")).toBeEnabled();
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});

test("the marker stays out of the way while the context is open", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await hoverBand(page, "top");
  await marker(page).click();
  await expect.poll(() => page.locator("#pages").getAttribute("data-running-edit")).toBe("header");

  // Already editing the header, so an invitation to edit the header is noise.
  await hoverBand(page, "top");
  await expect(marker(page)).toBeHidden();

  expect(consoleErrors).toEqual([]);
});
