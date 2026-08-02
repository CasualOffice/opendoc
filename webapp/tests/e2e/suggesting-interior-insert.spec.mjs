// REVIEW-GAP-007 phase 2 (docs/86): typing *inside* a pending insertion must
// extend that same suggestion instead of starting a second one or failing. This
// exercises the real Suggesting-mode editor path (wasm `suggest_insert` ->
// `extend_authored_insertion_interior`).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

async function setIdentity(page, name, initials = "") {
  const settingsPanel = page.locator("#settingsPanel");
  if (await settingsPanel.isHidden()) {
    await page.locator("#settingsBtn").click();
  }
  await expect(settingsPanel).toBeVisible();
  await page.locator("#authorName").fill(name);
  await page.locator("#authorInitials").fill(initials);
  await page.keyboard.press("Escape");
  await expect(settingsPanel).toBeHidden();
}

async function find(page, query) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(query);
  const status = await page.locator("#findStatus").textContent();
  await page.keyboard.press("Escape");
  return status;
}

test("typing inside a pending insertion extends the same suggestion (REVIEW-GAP-007)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setReviewMode(page, "suggesting");
  await setIdentity(page, "Ada Lovelace");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Author one tracked insertion "HELLO" at the document start.
  await page.keyboard.type("HELLO");
  const insertionCards = page.locator(
    "#reviewSidebar .review-margin-card.review-margin-insertion",
  );
  await expect(insertionCards).toHaveCount(1);
  expect(await find(page, "HELLO")).toBe("1 match");

  // Move the caret into the middle of the pending insertion (between "HEL" and
  // "LO"). The two ArrowLefts also end the typing session, so this is the true
  // interior case — not a trailing-boundary continuation.
  await moveCaretToDocStart(page);
  for (let i = 0; i < 3; i += 1) await page.keyboard.press("ArrowRight");
  await page.keyboard.type("XX");
  await page.waitForTimeout(150);

  // Still exactly one tracked insertion, now reading "HELXXLO" — the typed text
  // joined the same suggestion rather than spawning a second card.
  await expect(insertionCards).toHaveCount(1);
  expect(await find(page, "HELXXLO")).toBe("1 match");
  expect(await find(page, "HELLO")).toBe("No match");

  expect(consoleErrors).toEqual([]);
});
