// The owner's own requirement for the phone tier, as a property (docs/148 §6):
//
//   > no horizontal scroll, anywhere, at any phone width — the page itself,
//   > every panel, every dialog, every menu.
//
// WHY THIS IS A SPEC AND NOT A STYLESHEET REVIEW. Horizontal overflow is never
// declared; it is what happens when something in a box is wider than the box,
// and it can arrive from a fixed width, a flex row that will not wrap, a grid
// track in px, a `100vw` inside padding, or a word that cannot break. Reading
// CSS cannot find it. Measuring can, so this spec measures two different
// things about every candidate:
//
//   1. `scrollWidth` against `clientWidth` — the element scrolls sideways;
//   2. its children's bounding boxes against the WINDOW — something is painted
//      off the edge even though nothing scrolls, which is the same defect with
//      the scrollbar suppressed and is how an `overflow: hidden` "fix" hides a
//      control rather than fitting it.
//
// WHAT IS DELIBERATELY EXEMPT, and it is one thing: `#viewport`, the document.
// A Letter page's text column is 6.5in — 624 CSS px at 96dpi — and it cannot be
// both 390px wide and readable. `view_zoom.mjs`'s `FIT_ON_OPEN_FLOOR` already
// refuses to shrink a page to the ~31% that would make it fit, with the reason
// recorded in that file. The real answer is a reflow view, which Google ship as
// Pageless and ONLYOFFICE as `api.ChangeReaderMode()`, and which is layout
// engine work (docs/148 §9 item 1). So the paper is declared here rather than
// quietly excluded: `DOCUMENT_SURFACE` names it, and a second test asserts the
// exemption is still NEEDED, so that when reflow lands and the document stops
// overflowing, this file fails and the exemption comes out.
import { test, expect, gotoEditor, clickIntoFirstPage, openCommandPalette } from "./fixtures.mjs";

/** Two real phones. 390 is an iPhone 14/15 and a Pixel 7 in portrait; 320 is
 *  the narrowest viewport still shipping (iPhone SE 1st gen) and is where a
 *  layout that merely "works at 390" comes apart. */
const PHONES = [
  { name: "390px", width: 390, height: 844 },
  { name: "320px", width: 320, height: 568 },
];

/** The document canvas. See the header: named, not hidden. */
const DOCUMENT_SURFACE = "#viewport";

/**
 * Every element on the page that scrolls horizontally, or paints outside the
 * window, excluding the document surface and anything inside it.
 *
 * Runs in the page. O(nodes in the CHROME) — the document's pages are canvas
 * elements, so this does not walk the document (docs/107 §4).
 */
async function overflowReport(page) {
  return page.evaluate((documentSurface) => {
    const doc = document.querySelector(documentSurface);
    const scrollers = [];
    const painted = [];
    const name = (el) =>
      el.tagName.toLowerCase() +
      (el.id ? `#${el.id}` : "") +
      (typeof el.className === "string" && el.className ? `.${el.className.trim().split(/\s+/).join(".")}` : "");

    const root = document.scrollingElement;
    if (root.scrollWidth > root.clientWidth + 1) {
      scrollers.push({ el: "document", scrollWidth: root.scrollWidth, clientWidth: root.clientWidth });
    }

    for (const el of document.querySelectorAll("body *")) {
      if (doc && (el === doc || doc.contains(el))) continue;
      const box = el.getBoundingClientRect();
      // Nothing is measured about an element that is not on screen: `hidden`
      // dialogs are most of this document and they have no geometry yet.
      if (box.width === 0 || box.height === 0) continue;

      const style = getComputedStyle(el);
      // A scroll of one's own is legal ONLY where the element opts into it and
      // the content genuinely cannot be made to fit — and at this rung nothing
      // in the chrome is allowed to. `hidden`/`clip` are excluded because they
      // do not scroll; the bounding-box pass below is what catches those.
      if (el.scrollWidth > el.clientWidth + 1 && style.overflowX !== "hidden" && style.overflowX !== "clip") {
        scrollers.push({ el: name(el), scrollWidth: el.scrollWidth, clientWidth: el.clientWidth, overflowX: style.overflowX });
      }

      // Painted outside the window. A 1px tolerance, and leaf elements only for
      // the right-hand side: a container whose child spills reports twice, and
      // the child is the one worth naming.
      if (box.right > window.innerWidth + 1 || box.left < -1) {
        painted.push({ el: name(el), left: Math.round(box.left), right: Math.round(box.right) });
      }
    }
    return { width: window.innerWidth, scrollers, painted };
  }, DOCUMENT_SURFACE);
}

/** `painted` catches deliberately off-screen chrome too — the screen-reader
 *  mirror, the 1px text-input proxy parked at the caret — so those are named
 *  rather than the assertion being loosened to "not many". */
const OFFSCREEN_BY_DESIGN = [
  "#a11yDocument", // the model-derived a11y mirror, `sr-only`
  "#editorTextInput", // the 1px editable focus owner, moved to the caret
  ".sr-only",
  "#reviewLiveRegion",
  "#statusLiveRegion",
  "#statusAlertRegion",
  ".skip-link",
];

const real = (rows) => rows.filter((r) => !OFFSCREEN_BY_DESIGN.some((s) => r.el.includes(s.replace(/^[.#]/, ""))));

for (const phone of PHONES) {
  test(`the chrome never scrolls sideways at ${phone.name}`, async ({ page, consoleErrors }) => {
    await page.setViewportSize({ width: phone.width, height: phone.height });
    await gotoEditor(page);

    // Precondition: the tier under test is actually in force. Without this the
    // whole spec would pass on a desktop chrome that happened not to overflow,
    // which is the shape of guard `105` CQ-003 records three times.
    await expect(page.locator("body")).toHaveClass(/phone-mode/);
    await expect(page.locator("body")).toHaveClass(/compact-mode/);
    await expect(page.locator(".ribbon")).toBeHidden();

    const report = await overflowReport(page);
    expect(report.scrollers, `sideways scrollers at ${phone.name}`).toEqual([]);
    expect(real(report.painted), `painted outside the window at ${phone.name}`).toEqual([]);

    expect(consoleErrors).toEqual([]);
  });

  test(`menus, dialogs and panels fit the window at ${phone.name}`, async ({ page, consoleErrors }) => {
    await page.setViewportSize({ width: phone.width, height: phone.height });
    await gotoEditor(page);
    await expect(page.locator("body")).toHaveClass(/phone-mode/);

    // Each is opened, measured, and closed, so one surface's overflow cannot be
    // charged to the next. Escape is the shared dismissal (`light-dismiss-contract`).
    const surfaces = [
      { what: "the File menu", open: () => page.locator('.app-menu-button[data-menu="file"]').click(), shown: "#appMenuPopover" },
      { what: "the Format menu", open: () => page.locator('.app-menu-button[data-menu="format"]').click(), shown: "#appMenuPopover" },
      { what: "Settings", open: () => page.locator("#settingsBtn").click(), shown: "#settingsPanel" },
      { what: "the outline panel", open: () => page.locator("#railOutline").click(), shown: "#outlinePanel" },
      { what: "the comment sheet", open: () => page.locator("#railReview").click(), shown: "#reviewSidebar" },
      { what: "the command palette", open: () => openCommandPalette(page), shown: "#cmdPalette" },
    ];

    for (const surface of surfaces) {
      await surface.open();
      await expect(page.locator(surface.shown).first()).toBeVisible();

      const report = await overflowReport(page);
      expect(report.scrollers, `sideways scrollers with ${surface.what} open`).toEqual([]);
      expect(real(report.painted), `painted outside the window with ${surface.what} open`).toEqual([]);

      await page.keyboard.press("Escape");
    }

    expect(consoleErrors).toEqual([]);
  });

  test(`the context menu fits the window at ${phone.name}`, async ({ page, consoleErrors }) => {
    await page.setViewportSize({ width: phone.width, height: phone.height });
    await gotoEditor(page);
    await clickIntoFirstPage(page);

    // Opened as close to the RIGHT EDGE OF THE WINDOW as the page reaches, on
    // purpose: `popoverPosition` clamps a menu's left edge to the window and
    // can do nothing about a menu wider than the window, so the right edge is
    // where that shows. Window coordinates, not page coordinates — at this
    // width the sheet is wider than the screen (see the exemption at the foot
    // of this file), so its own right edge is somewhere nobody can click.
    const box = await page.locator(".page-wrap .page").first().boundingBox();
    const x = Math.min(phone.width - 8, box.x + box.width - 8);
    await page.mouse.click(x, box.y + box.height * 0.2, { button: "right" });
    await expect(page.locator(".editor-context-menu")).toBeVisible();

    const menu = await page.locator(".editor-context-menu").boundingBox();
    expect(menu.x, "the context menu starts inside the window").toBeGreaterThanOrEqual(-1);
    expect(menu.x + menu.width, "the context menu ends inside the window").toBeLessThanOrEqual(phone.width + 1);

    const report = await overflowReport(page);
    expect(report.scrollers, "sideways scrollers with the context menu open").toEqual([]);

    expect(consoleErrors).toEqual([]);
  });
}

test("the phone chrome replaces the desktop chrome rather than shrinking it", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoEditor(page);

  // The two navigation systems a phone has no width for are GONE, not narrower.
  await expect(page.locator(".ribbon")).toBeHidden();
  await expect(page.locator(".ribbon-tabs")).toBeHidden();
  await expect(page.locator(".chrome-mode")).toBeHidden();

  // The one it keeps is the one that is an axis, and it is whole: at 360px the
  // desktop bar clips under a fade mask (responsive-shell.spec.mjs measures
  // exactly that). Here every menu is on screen without a sideways gesture.
  const bar = page.locator("#appMenuBar");
  await expect(bar).toBeVisible();
  expect(await bar.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  const buttons = page.locator(".app-menu-button");
  const count = await buttons.count();
  expect(count).toBeGreaterThan(4);
  for (let i = 0; i < count; i += 1) {
    const b = await buttons.nth(i).boundingBox();
    expect(b.x + b.width, `menu ${i} is on screen`).toBeLessThanOrEqual(390 + 1);
    // WCAG 2.5.8 Target Size (Minimum), Level AA — and the coarse-pointer block
    // in style.css explicitly defers this row, so this is the assertion that
    // says the deferral ended at this rung.
    expect(b.height, `menu ${i} is a touch target`).toBeGreaterThanOrEqual(24);
  }

  // The command surface moved to the bottom, where all three references put it.
  const toolbar = await page.locator("#compactToolbar").boundingBox();
  expect(await page.locator("#compactToolbar").evaluate((el) => getComputedStyle(el).position)).toBe("fixed");
  expect(toolbar.y, "the toolbar is in the bottom half of the screen").toBeGreaterThan(844 / 2);

  // The rail turned its axis rather than disappearing — it is the only surface
  // `#pagesPanel` has (docs/148 §5.3). A horizontal strip is wider than it is
  // tall; a column is not.
  const rail = await page.locator(".rail").boundingBox();
  expect(rail.width, "the rail is a strip, not a column").toBeGreaterThan(rail.height);
  expect(await page.locator("#railPages").isVisible()).toBe(true);

  expect(consoleErrors).toEqual([]);
});

test("the document surface is still the only exemption, and still needs to be", async ({
  page,
  consoleErrors,
}) => {
  // The other half of an exemption list: an exemption that is no longer needed
  // is cover, and `one-axis-navigation.spec.mjs` already carries this rule for
  // its PALETTE_ONLY list. When reflow lands (docs/148 §9 item 1) the document
  // will stop overflowing at 390px and THIS test fails — which is the prompt to
  // delete the exemption rather than to leave it sitting there being true.
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoEditor(page);

  const overflows = await page.evaluate(() => {
    const v = document.getElementById("viewport");
    return { scrollWidth: v.scrollWidth, clientWidth: v.clientWidth };
  });
  expect(
    overflows.scrollWidth,
    "the document no longer overflows at 390px — remove DOCUMENT_SURFACE from this spec " +
      "and fold #viewport back into the general assertion (docs/148 §6)",
  ).toBeGreaterThan(overflows.clientWidth);

  expect(consoleErrors).toEqual([]);
});
