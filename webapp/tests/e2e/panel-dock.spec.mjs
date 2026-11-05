// The dock: one slot per side, the page re-centred and never covered, Escape
// from inside a panel returns focus (`docs/184` D4 as amended 2026-11-06,
// PROPOSAL D-10, `docs/109` UX-055; audit DIALOGS-01/02/04/05).
//
// WHAT THIS ASSERTS IS THE GUARANTEE, NOT THE MECHANISM.
//
// * "At most one panel per side" is checked after EVERY open of EVERY panel,
//   with nothing closed in between — the exact sequence that left Pages,
//   Version history, Paragraph properties and Symbol side by side at 1280px with
//   no part of the page visible (DIALOGS-01, P0). The roster is the product's:
//   the last test fails if a side panel appears that is not listed here.
// * "The page is re-centred" is the page's two gaps to the viewport compared to
//   each other (±2px), at 1280 and 1440, with a right panel opened while the
//   comment column was showing — the condition that made the page scroll
//   sideways under the panel (DIALOGS-02). No pixel position is pinned.
// * "Escape returns focus" is `document.activeElement` after Escape from inside
//   each panel: on the control that opened it, or on the editor — never <body>,
//   never inside the closed panel, and never a DIFFERENT control (Paragraph
//   properties opened from the palette used to land on the ribbon's Paragraph
//   button). The palette routes, and the two galleries the audit measured
//   (Insert table, Shape), are asserted on the editor.
// * The roster includes the panels built at runtime (the object inspector, the
//   chart panel): they are opened, not just listed.
// * A right panel displacing the comment column keeps what was being written
//   there: a typed comment comes back with the column (SKILL §12).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  expectEditorFocused,
  runPaletteCommand,
  stableBox,
  MOD,
} from "./fixtures.mjs";

/** Every docked panel with the route that opens it, and the side it docks on. */
const PANELS = [
  { id: "outlinePanel", side: "left", invoker: "railOutline", open: (page) => page.locator("#railOutline").click() },
  { id: "pagesPanel", side: "left", invoker: "railPages", open: (page) => page.locator("#railPages").click() },
  { id: "comparePanel", side: "right", invoker: "railCompare", open: (page) => page.locator("#railCompare").click() },
  { id: "versionPanel", side: "right", invoker: "railVersions", open: (page) => page.locator("#railVersions").click() },
  {
    id: "symbolDialog",
    side: "right",
    invoker: "insertSymbolBtn",
    open: async (page) => {
      await page.locator('[data-tab="insert"]').click();
      await page.locator("#insertSymbolBtn").click();
    },
  },
  {
    id: "emojiDialog",
    side: "right",
    invoker: "insertEmojiBtn",
    open: async (page) => {
      await page.locator('[data-tab="insert"]').click();
      await page.locator("#insertEmojiBtn").click();
    },
  },
  {
    id: "paragraphPropertiesPanel",
    side: "right",
    invoker: "paraOptsBtn",
    open: async (page) => {
      await page.locator('[data-tab="home"]').click();
      await page.locator("#paraOptsBtn").click();
    },
  },
  { id: "reviewSidebar", side: "right", open: (page) => page.locator("#railReview").click() },
];

/** The visible dock panels, grouped by side, read from the live DOM. */
const DOCKED = () => {
  const left = new Set(["outlinePanel", "pagesPanel"]);
  const shown = (el) => !el.hidden && el.getClientRects().length > 0;
  const panels = [...document.querySelectorAll(".workarea > .side-panel"), document.getElementById("reviewSidebar")];
  const out = { left: [], right: [] };
  for (const el of panels) {
    if (!el || !shown(el)) continue;
    out[left.has(el.id) ? "left" : "right"].push(el.id || el.className);
  }
  return out;
};

test("every panel opened in turn, nothing closed between: at most one per side, and the page stays visible", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  for (const panel of PANELS) {
    await panel.open(page);
    await expect(page.locator(`#${panel.id}`)).toBeVisible();
    const docked = await page.evaluate(DOCKED);
    expect(docked.left.length, `after opening ${panel.id}: left side holds ${docked.left}`).toBeLessThanOrEqual(1);
    expect(docked.right.length, `after opening ${panel.id}: right side holds ${docked.right}`).toBeLessThanOrEqual(1);
    expect(docked[panel.side], `${panel.id} holds its own side`).toContain(panel.id);
    // Some of the page is on screen: the viewport is at least a third of the
    // window wide (four panels side by side left it 0).
    const viewportWidth = await page.locator("#viewport").evaluate((el) => el.clientWidth);
    expect(viewportWidth, `after opening ${panel.id} the document column is ${viewportWidth}px`).toBeGreaterThan(400);
  }
  expect(consoleErrors).toEqual([]);
});

for (const width of [1280, 1440]) {
  test(`a right panel re-centres the page at ${width}px, even when it displaces the comment column`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width, height: 800 });
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    // The condition: the comment column is showing, so its gutter is reserved.
    await page.locator("#railReview").click();
    await expect(page.locator("#reviewSidebar")).toBeVisible();
    for (const id of ["versionPanel", "paragraphPropertiesPanel"]) {
      await PANELS.find((p) => p.id === id).open(page);
      await expect(page.locator(`#${id}`)).toBeVisible();
      await expect(page.locator("#reviewSidebar")).toBeHidden();
      const gaps = await page.evaluate(() => {
        const viewport = document.getElementById("viewport");
        const v = viewport.getBoundingClientRect();
        const p = document.querySelector(".page-wrap").getBoundingClientRect();
        return { left: p.left - v.left, right: v.left + viewport.clientWidth - p.right, scroll: viewport.scrollWidth - viewport.clientWidth };
      });
      expect(gaps.scroll, `${id} at ${width}: the page scrolls sideways by ${gaps.scroll}px`).toBeLessThanOrEqual(0);
      expect(Math.abs(gaps.left - gaps.right), `${id} at ${width}: gaps ${gaps.left} / ${gaps.right}`).toBeLessThanOrEqual(2);
    }
    // Closing the panel gives the comment column its slot back.
    await page.locator("#paragraphPropertiesClose").click();
    await expect(page.locator("#reviewSidebar")).toBeVisible();
    expect(consoleErrors).toEqual([]);
  });
}

for (const panel of PANELS.filter((p) => p.id !== "reviewSidebar")) {
  test(`Escape from inside ${panel.id} closes it and returns focus to the invoker or the editor`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await panel.open(page);
    const root = page.locator(`#${panel.id}`);
    await expect(root).toBeVisible();
    // Focus something inside the panel the way a keyboard user gets there.
    await root.evaluate((el) => {
      const target = el.querySelector("button:not([disabled]), input, select, [tabindex='0']");
      target?.focus();
    });
    await page.keyboard.press("Escape");
    await expect(root).toBeHidden();
    const focus = await page.evaluate((id) => {
      const active = document.activeElement;
      return {
        body: !active || active === document.body,
        inside: !!document.getElementById(id)?.contains(active),
        id: active?.id ?? "",
      };
    }, panel.id);
    expect(focus.body, `${panel.id}: focus fell to <body>`).toBe(false);
    expect(focus.inside, `${panel.id}: focus stayed inside the closed panel (${focus.id})`).toBe(false);
    // On the control that opened it, or on the document — nothing else.
    expect([panel.invoker, "pages", "editorTextInput"], `${panel.id}: focus went to "${focus.id}"`).toContain(focus.id);
    expect(consoleErrors).toEqual([]);
  });
}

/** Opened from the palette, a panel or gallery has no control to go back to:
 *  Escape puts focus in the document. Paragraph properties used to land on the
 *  ribbon's Paragraph button, and the Insert table grid and the Shape gallery on
 *  <body> (audit DIALOGS-05). */
const FROM_PALETTE = [
  // Home showing, so the ribbon's Paragraph button is ON screen and able to take
  // focus: the case the audit measured, and the one a hidden button would mask.
  { command: "layout.paragraph", query: "Paragraph properties", surface: "#paragraphPropertiesPanel", tab: "home" },
  // Review showing, so the galleries' ribbon triggers are OFF screen.
  { command: "insert.table", query: "Insert table", surface: "#insertTableMenu", tab: "review" },
  { command: "insert.shape", query: "Shape", surface: "#shapeGalleryMenu", tab: "review" },
];
for (const route of FROM_PALETTE) {
  test(`${route.command} opened from the palette: Escape returns focus to the document`, async ({ page, consoleErrors }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await page.locator(`[data-tab="${route.tab}"]`).click();
    await clickIntoFirstPage(page);
    await runPaletteCommand(page, route.command, route.query);
    const surface = page.locator(route.surface).first();
    await expect(surface).toBeVisible();
    // Focus is inside the surface (the panel and both galleries take it on open).
    await expect.poll(() => surface.evaluate((el) => el.contains(document.activeElement))).toBe(true);
    await page.keyboard.press("Escape");
    await expect(surface).toBeHidden();
    await expectEditorFocused(page);
    expect(consoleErrors).toEqual([]);
  });
}

/** Panels built at runtime take the right slot like the rest, and are on the roster. */
const LISTED = new Set([...PANELS.map((p) => p.id), "pluginPanel", "tablePropertiesPanel", "chartPanel"]);
const UNLISTED = () =>
  [...document.querySelectorAll(".workarea > .side-panel")]
    .filter((el) => !el.classList.contains("object-inspector"))
    .map((el) => el.id || el.className);

test("the chart panel, built at runtime, takes the right slot and is on the roster", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#railVersions").click();
  await expect(page.locator("#versionPanel")).toBeVisible();
  await page.locator("#tabInsert").click();
  await page.locator("#insertChartBtn").click();
  await page.getByRole("menuitem", { name: /^Clustered column$/ }).click();
  await page.locator("#chartDataDialog").getByRole("button", { name: "Done" }).click();
  await page.locator("#tabChart").click();
  await page.locator("#chartSettingsBtn").click();
  await expect(page.locator("#chartPanel")).toBeVisible();
  const docked = await page.evaluate(DOCKED);
  expect(docked.right, `right side holds ${docked.right}`).toEqual(["chartPanel"]);
  const unlisted = await page.evaluate(UNLISTED);
  expect(unlisted.filter((id) => !LISTED.has(id))).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("the object inspector, built at runtime, takes the right slot", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0 && document.getElementById("status")?.textContent === "", null, { timeout: 45_000 });
  await page.locator("#railVersions").click();
  await expect(page.locator("#versionPanel")).toBeVisible();
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();
  await expect(page.locator(".object-inspector")).toBeVisible();
  const docked = await page.evaluate(DOCKED);
  expect(docked.right.length, `right side holds ${docked.right}`).toBe(1);
  await expect(page.locator("#versionPanel")).toBeHidden();
  const unlisted = await page.evaluate(UNLISTED);
  expect(unlisted.filter((id) => !LISTED.has(id))).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("a typed comment survives a right panel opening over the column and closing again", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.insertText("Displaced draft line");
  await page.keyboard.press("Shift+Home");
  await page.locator("#selComment").click();
  const composer = page.getByTestId("review-comment-composer");
  await composer.fill("half a thought");
  for (const asDraft of [false, true]) {
    // Once with the composer still open, once after Escape made it a Draft.
    if (asDraft) {
      await composer.press("Escape");
      await expect(page.locator(".review-margin-composer.is-draft")).toBeVisible();
    }
    await page.locator("#railVersions").click();
    await expect(page.locator("#versionPanel")).toBeVisible();
    await expect(page.locator("#reviewSidebar")).toBeHidden();
    // Displacement is not the person closing the column: focus is not moved to
    // the rail's Comments button.
    await expect(page.locator("#railReview")).not.toBeFocused();
    await page.locator("#versionPanelClose").click();
    await expect(page.locator("#reviewSidebar")).toBeVisible();
    await expect(composer, `the ${asDraft ? "Draft" : "open composer"} came back`).toHaveValue("half a thought");
  }
  expect(consoleErrors).toEqual([]);
});

test("a side's width is one, resizable on the inner edge between 17.5 and 30rem, and remembered", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#railVersions").click();
  const handle = page.locator("#versionPanel > .dock-resize");
  await expect(handle).toHaveAttribute("role", "separator");
  const before = await page.locator("#versionPanel").evaluate((el) => el.getBoundingClientRect().width);
  await handle.focus();
  for (let i = 0; i < 40; i++) await page.keyboard.press("ArrowLeft");
  const rem = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).fontSize));
  const wide = await page.locator("#versionPanel").evaluate((el) => el.getBoundingClientRect().width);
  expect(wide).toBeGreaterThan(before);
  expect(Math.round(wide / rem)).toBe(30);
  for (let i = 0; i < 40; i++) await page.keyboard.press("ArrowRight");
  const narrow = await page.locator("#versionPanel").evaluate((el) => el.getBoundingClientRect().width);
  expect(Math.round((narrow / rem) * 2) / 2).toBe(17.5);
  // The same width for the next right panel, and after a reload.
  await page.keyboard.press("ArrowLeft");
  const chosen = await page.locator("#versionPanel").evaluate((el) => el.getBoundingClientRect().width);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertSymbolBtn").click();
  await expect(page.locator("#symbolDialog")).toBeVisible();
  const symbol = await page.locator("#symbolDialog").evaluate((el) => el.getBoundingClientRect().width);
  expect(Math.abs(symbol - chosen)).toBeLessThanOrEqual(1);
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#railVersions").click();
  const reloaded = await page.locator("#versionPanel").evaluate((el) => el.getBoundingClientRect().width);
  expect(Math.abs(reloaded - chosen)).toBeLessThanOrEqual(1);
});

test("the roster above is every docked side panel in the page", async ({ page }) => {
  await gotoEditor(page);
  const inPage = await page.evaluate(() =>
    [...document.querySelectorAll(".workarea > .side-panel[id]")].map((el) => el.id),
  );
  // The add-in panel and the table properties panel share slots this roster
  // already exercises: the add-in panel needs an installed plugin (it is in
  // `panel-alignment.spec.mjs`'s roster), and Table is the Properties panel's
  // second tab. Both are right-slot panels the dock reads live.
  expect(inPage.filter((id) => !LISTED.has(id))).toEqual([]);
});
