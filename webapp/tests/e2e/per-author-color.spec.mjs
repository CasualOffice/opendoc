import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

// REVIEW-GAP-015 (docs/81): every reviewer used to share the same green/red
// overlay colors and there was no author/date tooltip, so multiple reviewers
// were visually indistinguishable. Per-author color assigns each distinct
// author a stable palette color (webapp presentation only — docs/68 §50),
// applied to the inline insertion/deletion/comment markers and the sidebar card
// avatar chip, plus an attribution tooltip (author · type · date) on hover.
//
// These specs drive the real host-identity control (the same seam REVIEW-GAP-013
// established) to author changes as two distinct reviewers and assert that the
// two are rendered in two different, self-consistent colors with the right
// attribution.

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

async function addComment(page, marker, text) {
  await page.keyboard.type(marker);
  for (let i = 0; i < marker.length; i++) {
    await page.keyboard.press("Shift+ArrowLeft");
  }
  await page.locator("#selComment").click();
  const sidebar = page.locator("#reviewSidebar");
  const composer = sidebar.locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill(text);
  await sidebar.locator('[data-testid="review-comment-submit"]').click();
  await expect(
    sidebar.locator(".review-margin-card.review-margin-comment").filter({ hasText: text }),
  ).toBeVisible();
}

// The stable author color a card/marker carries — the `--review-author-color`
// custom property the webapp sets inline from the author's palette color.
function authorColorOf(locator) {
  return locator.evaluate((el) => el.style.getPropertyValue("--review-author-color").trim());
}

test("two authors' comments render in two distinct, self-consistent colors with attribution", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  await setIdentity(page, "Ada Lovelace");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await addComment(page, "AUTHORCOLOR_ADA", "By Ada");

  await setIdentity(page, "Grace Hopper", "GH");
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await addComment(page, "AUTHORCOLOR_GRACE", "By Grace");

  const sidebar = page.locator("#reviewSidebar");
  const adaCard = sidebar
    .locator(".review-margin-card.review-margin-comment")
    .filter({ hasText: "By Ada" });
  const graceCard = sidebar
    .locator(".review-margin-card.review-margin-comment")
    .filter({ hasText: "By Grace" });

  // Each author's avatar chip carries a non-empty palette color, and the two
  // authors differ — the core "visually distinguishable" requirement.
  const adaColor = await authorColorOf(adaCard.locator(".review-margin-avatar"));
  const graceColor = await authorColorOf(graceCard.locator(".review-margin-avatar"));
  expect(adaColor).not.toBe("");
  expect(graceColor).not.toBe("");
  expect(adaColor).not.toBe(graceColor);

  // Attribution tooltip on the card: author + state + date.
  await expect(adaCard).toHaveAttribute("title", /Ada Lovelace/);
  await expect(adaCard).toHaveAttribute("title", /Comment/);
  await expect(graceCard).toHaveAttribute("title", /Grace Hopper/);

  // The inline comment markers on the canvas carry the SAME per-author colors,
  // so a reviewer's card and their highlighted range match. Both authors'
  // colors appear among the painted markers.
  const markerColors = await page
    .locator(".overlay .review-comment-marker")
    .evaluateAll((els) =>
      els.map((el) => el.style.getPropertyValue("--review-author-color").trim()),
    );
  expect(markerColors).toContain(adaColor);
  expect(markerColors).toContain(graceColor);

  // At least one marker carries an author/date attribution tooltip.
  const markerTitles = await page
    .locator(".overlay .review-comment-marker")
    .evaluateAll((els) => els.map((el) => el.getAttribute("title") || ""));
  expect(markerTitles.some((t) => /Ada Lovelace/.test(t))).toBe(true);
  expect(markerTitles.some((t) => /Grace Hopper/.test(t))).toBe(true);

  expect(consoleErrors).toEqual([]);
});

test("two authors' tracked insertions render in two distinct colors and show change attribution on hover", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // Enter Suggesting mode first (clean Home-tab state), then author two tracked
  // insertions as two different reviewers.
  await clickIntoFirstPage(page);
  await setReviewMode(page, "suggesting");

  await setIdentity(page, "Ada Lovelace");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("ADAINS");

  await setIdentity(page, "Grace Hopper", "GH");
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.type("GRACEINS");

  // Two tracked insertions, one per author, each underlined in its author color.
  const markers = page.locator(".overlay .review-insertion-marker");
  await expect(markers).not.toHaveCount(0);
  const markerColors = await markers.evaluateAll((els) =>
    els.map((el) => el.style.getPropertyValue("--review-author-color").trim()),
  );
  const distinct = [...new Set(markerColors.filter(Boolean))];
  expect(distinct.length).toBeGreaterThanOrEqual(2);

  // Hovering a tracked insertion exposes an author · type · date attribution
  // through the native `title` tooltip (both authors' insertions are present).
  const markerTitles = await markers.evaluateAll((els) =>
    els.map((el) => el.getAttribute("title") || ""),
  );
  expect(markerTitles.some((t) => /Ada Lovelace/.test(t) && /Insertion/.test(t))).toBe(true);
  expect(markerTitles.some((t) => /Grace Hopper/.test(t) && /Insertion/.test(t))).toBe(true);

  // The sidebar cards for those insertions carry matching attribution tooltips.
  const sidebar = page.locator("#reviewSidebar");
  const insertionCards = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(insertionCards).not.toHaveCount(0);
  const cardTitles = await insertionCards.evaluateAll((els) =>
    els.map((el) => el.getAttribute("title") || ""),
  );
  expect(cardTitles.some((t) => /Insertion/.test(t))).toBe(true);

  expect(consoleErrors).toEqual([]);
});
