import { test, expect } from "@playwright/test";

// The Examples gallery is memory-safe by construction: each card is a STATIC
// poster until the user activates it, and only ONE live editor iframe is ever
// alive at a time (activating another tears the previous down). This exercises
// that invariant end-to-end so a regression to auto-booting N editors — which
// would blow the editor's tab-memory budget — fails here.
test("cards stay static until activated, then boot exactly one live editor", async ({ page }) => {
  // Collect console errors from the gallery page only — not from the heavy
  // editor iframe, whose own console cleanliness is covered by its own specs.
  const pageErrors = [];
  page.on("console", (m) => {
    if (m.type() === "error" && !m.location().url.includes("editor.html")) pageErrors.push(m.text());
  });
  page.on("pageerror", (e) => pageErrors.push(String(e)));

  await page.goto("/examples.html");

  const cards = page.locator(".example-card");
  await expect(cards).toHaveCount(6);

  const liveFrames = page.locator("iframe.example-frame");

  // Static state: posters are shown and NO editor has booted.
  await expect(page.locator(".example-poster").first()).toBeVisible();
  await expect(liveFrames).toHaveCount(0);

  // Activate the first card (Tables) — a single live editor boots into it.
  const first = cards.nth(0);
  await first.getByRole("button", { name: /Run/i }).click();
  await expect(liveFrames).toHaveCount(1);
  await expect(first).toHaveClass(/is-live/);
  await expect(liveFrames).toHaveAttribute("src", /editor\.html\?demo=tables/);

  // Activating a second card (Tracked changes) tears the first down — the
  // single-live-instance invariant: still exactly one editor iframe.
  const second = cards.nth(1);
  await second.getByRole("button", { name: /Run/i }).click();
  await expect(liveFrames).toHaveCount(1);
  await expect(first).not.toHaveClass(/is-live/);
  await expect(second).toHaveClass(/is-live/);
  await expect(liveFrames).toHaveAttribute("src", /editor\.html\?demo=changes/);

  // Closing returns the gallery to a fully static, zero-editor state.
  await second.getByRole("button", { name: /Close/i }).click();
  await expect(liveFrames).toHaveCount(0);
  await expect(second).not.toHaveClass(/is-live/);

  // The run controls are real, keyboard-focusable buttons (a11y).
  const run = first.getByRole("button", { name: /Run/i });
  await run.focus();
  await expect(run).toBeFocused();

  expect(pageErrors).toEqual([]);
});
