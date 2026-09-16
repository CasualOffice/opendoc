import { test, expect } from "./fixtures.mjs";

test("the developer landing page exposes the redesigned hero and real editor routes", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/");

  // The 2026-09 redesign's headline, with its serif phrase as a real <em> so the
  // emphasis survives for assistive technology and not only for the eye.
  const h1 = page.getByRole("heading", { level: 1 });
  await expect(h1).toHaveText(/Build documents on an engine\s*you control\./);
  await expect(h1.locator("em")).toHaveText("you control.");

  // The primary route into the product stays same-origin and relative. The
  // design prototype linked the absolute production URL, which breaks every
  // preview deployment and every local serve.
  await expect(page.getByRole("link", { name: "Try the live editor" })).toHaveAttribute(
    "href",
    "./editor.html?demo=1",
  );
  await expect(page.getByRole("link", { name: "See how it works" })).toHaveAttribute(
    "href",
    "#developers",
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

test("the quickstart switches between the shell and embed examples as real tabs", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/");
  const shell = page.getByRole("tab", { name: "shell" });
  const embed = page.getByRole("tab", { name: "embed" });
  await expect(shell).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#qsShell")).toBeVisible();
  await expect(page.locator("#qsEmbed")).toBeHidden();

  await embed.click();
  await expect(embed).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#qsEmbed")).toBeVisible();
  await expect(page.locator("#qsShell")).toBeHidden();
  await expect(page.locator("#qsEmbed")).toContainText("doc.exportDocx()");

  // Arrow keys rove, as the WAI-ARIA tabs pattern requires — a tablist you can
  // only click is a pointer-only control wearing tab semantics.
  await embed.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(shell).toBeFocused();
  await expect(shell).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#qsShell")).toBeVisible();

  expect(consoleErrors).toEqual([]);
});
