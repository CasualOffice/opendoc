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
// NOTHING IS EXEMPT ANY MORE, AND THAT IS NEW. Until reflow landed this file
// carried one declared exception — `#viewport`, the document — because a Letter
// page's text column is 6.5in (624 CSS px at 96dpi) and cannot be both 390px
// wide and readable, and because `view_zoom.mjs`'s `FIT_ON_OPEN_FLOOR` refuses
// to shrink it to the ~31% that would make it fit. It measured `scrollWidth 794`
// against `clientWidth 326`, it was named in `docs/148` §8 and ADR-044, and two
// tests here existed to say so: one asserting the exemption was still NEEDED,
// one asserting the engine seam had not landed without the shell that spends it.
//
// `docs/151` §6 built that shell. Reflow defaults on below the phone rung, lays
// the body out at the window's width and cuts it into tiles, so the document no
// longer overflows — and both tripwires fired, which is exactly what they were
// for (`SKILL.md` §9.4: "built" is not "reachable"). The exemption is gone, the
// two tests with it, and `#viewport` is now swept by the general assertion below
// like every other element. `tests/e2e/reflow.spec.mjs` holds the positive
// claim; this file holds the property that no longer has a hole in it.
import { test, expect, gotoEditor, clickIntoFirstPage, menuCommandRow, openCommandPalette } from "./fixtures.mjs";

/** Two real phones. 390 is an iPhone 14/15 and a Pixel 7 in portrait; 320 is
 *  the narrowest viewport still shipping (iPhone SE 1st gen) and is where a
 *  layout that merely "works at 390" comes apart. */
const PHONES = [
  { name: "390px", width: 390, height: 844 },
  { name: "320px", width: 320, height: 568 },
];

/**
 * Every element on the page that scrolls horizontally, or paints outside the
 * window. The document surface included, since reflow landed — see the header.
 *
 * Runs in the page. O(nodes in the CHROME plus the materialized sheets) — the
 * document's pages are canvas elements and only the ones near the viewport
 * exist, so this does not walk the document (docs/107 §4).
 */
async function overflowReport(page, { skipDocument = false } = {}) {
  return page.evaluate(([offscreen, skip]) => {
    const document_ = skip ? document.getElementById("viewport") : null;
    const inOffscreenChrome = (el) =>
      offscreen.some((selector) => el.closest(selector)) ||
      (document_ !== null && (el === document_ || document_.contains(el)));
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
      // Inside a deliberately off-screen region there is nothing to measure. The
      // accessibility mirror is the case that matters: it is a 1px clipped box
      // holding the whole document as text, so every paragraph in it "scrolls"
      // by hundreds of pixels inside a 1px client box — a scroll no reader can
      // perform and no scrollbar exists for. Skipped by CONTAINER rather than by
      // element, because the mirror's children carry no class of their own.
      if (inOffscreenChrome(el)) continue;
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
  }, [OFFSCREEN_BY_DESIGN, skipDocument]);
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
    //
    // The panels are opened from the MENU BAR rather than from the rail, and
    // that is the change rather than an incidental rewrite: this rung withholds
    // the rail now that `view.pages` exists (`docs/148` §5.3a), so the rail's
    // tiles are not the route to these panels on a phone — the menu bar is, as
    // doc 122 says it should be for the compact chrome. The panels themselves
    // are untouched and still measured here; only the door moved.
    // Through `menuCommandRow`, which opens a submenu flyout when the band the
    // command lives in folds into one — `runAppMenuCommand` cannot be used at
    // this rung because it starts by clicking `#modeCompact`, which the phone
    // withholds.
    const menuItem = (menu, command) => async () => {
      await (await menuCommandRow(page, menu, command)).click();
    };
    /** Flips reflow and waits for the relayout to land.
     *
     *  Needed because reflow is ON by default at this rung — that is what
     *  retired this file's exemption — and it WITHHOLDS the Pages navigator,
     *  whose thumbnails would be tiles rather than pages (`docs/151` §6.4). The
     *  panel is one toggle away and the disabled row says so; `reflow.spec.mjs`
     *  asserts that refusal, and this lets the panel it refuses still be
     *  measured here. */
    const reflowing = () =>
      page.evaluate(() => document.getElementById("viewport").classList.contains("is-reflow"));
    const toggleReflow = async () => {
      const before = await reflowing();
      await (await menuCommandRow(page, "view", "view.reflow")).click();
      await expect.poll(reflowing).toBe(!before);
    };
    const surfaces = [
      { what: "the File menu", open: () => page.locator('.app-menu-button[data-menu="file"]').click(), shown: "#appMenuPopover" },
      { what: "the Format menu", open: () => page.locator('.app-menu-button[data-menu="format"]').click(), shown: "#appMenuPopover" },
      { what: "the Aa sheet", open: () => page.locator("#compactFormatBtn").click(), shown: "#compactFormatMenu" },
      { what: "the + sheet", open: () => page.locator("#compactInsertBtn").click(), shown: "#compactInsertMenu" },
      { what: "Settings", open: () => page.locator("#settingsBtn").click(), shown: "#settingsPanel" },
      { what: "the outline panel", open: menuItem("view", "view.outline"), shown: "#outlinePanel" },
      // The one row that measures the PANEL and not the document behind it, and
      // it is not the old exemption coming back. To reach a page navigator on a
      // phone the reader has to turn reflow OFF, and a document laid out on
      // Letter paper in a 390px window pans — which is what it was always going
      // to do, and is now a state nobody arrives in by default. This test is
      // about whether a panel fits the window; `reflow.spec.mjs` holds the
      // claim about the document, at the tier a reader actually opens.
      { what: "the Pages panel", prepare: toggleReflow, open: menuItem("view", "view.pages"), shown: "#pagesPanel", restore: toggleReflow, skipDocument: true },
      { what: "the comment sheet", open: menuItem("review", "review.toggle"), shown: "#reviewSidebar" },
      { what: "the command palette", open: () => openCommandPalette(page), shown: "#cmdPalette" },
    ];

    for (const surface of surfaces) {
      await surface.prepare?.();
      await surface.open();
      await expect(page.locator(surface.shown).first()).toBeVisible();

      const report = await overflowReport(page, { skipDocument: surface.skipDocument });
      expect(report.scrollers, `sideways scrollers with ${surface.what} open`).toEqual([]);
      expect(real(report.painted), `painted outside the window with ${surface.what} open`).toEqual([]);

      await page.keyboard.press("Escape");
      await surface.restore?.();
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

  // The rail and the ruler are gone, which is the OPPOSITE of what this test
  // asserted when it was written, and the reversal is the point (docs/148
  // §5.3a). They survived the first version of this rung against all three
  // references on one argument — `#railPages` and `setTabStop` each had exactly
  // one surface — and that argument was paid off by `view.pages` and
  // `layout.tabStops`. `phone-command-surface.spec.mjs` holds the other half:
  // the panels are still in the DOM and still reachable from the View menu.
  await expect(page.locator(".rail")).toBeHidden();
  await expect(page.locator(".ruler")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("the status toast does not land on the docked command bar", async ({ page, consoleErrors }) => {
  // `109` HF-233, one surface later. The toast owns the bottom-start corner and
  // the command bar has just moved into it; the toast is `pointer-events: none`
  // so nothing about clicking the bar can fail, and the overlap is therefore
  // invisible to every other kind of assertion. It was found by screenshotting
  // this rung and looking — "Rendering 1 page at 100%…" lay across the style
  // picker — which is why this measures geometry rather than behaviour.
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoEditor(page);

  // A toast that is genuinely up. `file.print` refuses without a print target
  // in a headless run either way, and any status line will do: what matters is
  // that the card is on screen when it is measured.
  await page.evaluate(() => {
    const toast = document.querySelector(".toast");
    toast.textContent = "A status message long enough to be a real card";
    toast.classList.add("is-shown");
    toast.hidden = false;
  });

  const toast = await page.locator(".toast").boundingBox();
  const bar = await page.locator("#compactToolbar").boundingBox();
  expect(toast, "the toast is on screen to be measured").not.toBeNull();
  expect(
    toast.y + toast.height,
    "the toast must sit clear above the command bar, not across it",
  ).toBeLessThanOrEqual(bar.y + 1);

  expect(consoleErrors).toEqual([]);
});
