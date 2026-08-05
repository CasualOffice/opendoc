import { test, expect } from "./fixtures.mjs";

test("the developer landing page exposes the redesigned hero and real editor routes", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/");

  // Newsreader/Instrument display headline of the approved redesign. The <br>
  // carries no whitespace, so match tolerantly across the line break.
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    /The document engine,\s*not the dependency\./,
  );
  await expect(page.getByRole("link", { name: "Try the live editor" })).toHaveAttribute(
    "href",
    "./editor.html?demo=1",
  );
  await expect(page.getByRole("link", { name: "Read the source" })).toHaveAttribute(
    "href",
    "https://github.com/CasualOffice/opendoc",
  );

  // The hero live-editor embed is present but STATIC on load — no WASM editor
  // iframe has booted (the marketing page must stay within the tab-memory
  // budget; only a click boots the real editor).
  await expect(page.locator("#homeEmbed")).toBeVisible();
  await expect(page.locator("#homeEmbed iframe")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("the hero live-editor embed is memory-safe: static until clicked, one instance", async ({
  page,
}) => {
  // Collect console errors from the marketing page only — not from the heavy
  // editor iframe, whose own console cleanliness is covered by its own specs.
  const pageErrors = [];
  page.on("console", (m) => {
    if (m.type() === "error" && !m.location().url.includes("editor.html")) pageErrors.push(m.text());
  });
  page.on("pageerror", (e) => pageErrors.push(String(e)));

  await page.goto("/");

  const embed = page.locator("#homeEmbed");
  const frames = page.locator("#homeEmbed iframe.home-embed-frame");

  // Static: the styled poster shows and NO editor has booted.
  await expect(embed.locator(".home-embed-poster")).toBeVisible();
  await expect(frames).toHaveCount(0);

  // Click Run — exactly one live editor boots into the embed.
  await embed.getByRole("button", { name: /Run the live editor/i }).click();
  await expect(frames).toHaveCount(1);
  await expect(embed).toHaveClass(/is-live/);
  await expect(frames).toHaveAttribute("src", /editor\.html\?demo=1/);

  // Closing returns the hero to a fully static, zero-editor state (freeing the
  // WASM instance the iframe held).
  await embed.getByRole("button", { name: /Close/i }).click();
  await expect(frames).toHaveCount(0);
  await expect(embed).not.toHaveClass(/is-live/);

  // The run control is a real, keyboard-focusable button (a11y).
  const run = embed.getByRole("button", { name: /Run the live editor/i });
  await run.focus();
  await expect(run).toBeFocused();

  expect(pageErrors).toEqual([]);
});

test("the refreshed landing page has no narrow-viewport page overflow", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");

  const metrics = await page.evaluate(() => ({
    viewport: window.innerWidth,
    document: document.documentElement.scrollWidth,
  }));
  expect(metrics.document).toBeLessThanOrEqual(metrics.viewport);
  await expect(page.getByRole("link", { name: "Open the editor" })).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
