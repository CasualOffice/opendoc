import { test, expect, gotoEditor, openFilePage, useCompactChrome } from "./fixtures.mjs";

// docs/123 §5 — the File page is a ROUTE, not an overlay.
//
// It shipped anchored to `--chrome-bottom`, which is where the chrome ends when
// a BAND is showing; on the File tab there is no band, so the page began ~74px
// below the chrome's real bottom edge and the ruler, the nav rail and the top of
// the sheet stayed on screen above it. A second, independent cause sat under
// that: `body` is a flex column, so `.ribbon`'s `z-index: 2` made it a stacking
// context and `#panelFile`'s own `z-index: 6` could never rise above the ruler's
// 4 — raising the page's z-index changed nothing.
//
// So this guard is GEOMETRIC, not structural. `toBeVisible()` on the page would
// have passed throughout the defect, and did: `one-axis-navigation.spec.mjs`
// already asserts the page is visible and more than half the viewport tall, and
// it was green the whole time the page was half-open. What cannot be green while
// the page half-opens is asking, at each covered surface's own centre, which
// element the browser would deliver a click to.

/** The element `document.elementFromPoint` reports at a selector's centre, and
 *  whether it belongs to the File page. Hit testing, not painting order —
 *  a surface that still takes the click is still in the way. */
async function hitAt(page, selector) {
  return page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return { missing: true };
    const b = el.getBoundingClientRect();
    const x = Math.round(b.left + b.width / 2);
    const y = Math.round(b.top + b.height / 2);
    const hit = document.elementFromPoint(x, y);
    return {
      at: `${x},${y}`,
      hit: hit ? `${hit.tagName}.${(hit.className || "").toString().trim().slice(0, 40)}` : null,
      inFilePage: !!hit?.closest("#panelFile"),
    };
  }, selector);
}

test("the File page covers the work area — ruler, rail, sheet and status bar", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openFilePage(page);

  const panel = page.locator("#panelFile");
  await expect(panel).toBeVisible();

  // It starts at the bottom of the navigation row and runs to the bottom of the
  // window, full width — ONLYOFFICE's `.toolbar-fullview-panel` is
  // `position: absolute; bottom: 0; width: 100%` with its `top` set to the tab
  // strip's height (`Viewport.js:182`).
  const geometry = await page.evaluate(() => {
    const r = (sel) => {
      const el = document.querySelector(sel);
      return el ? el.getBoundingClientRect().toJSON() : null;
    };
    return { panel: r("#panelFile"), header: r("header.bar"), vh: innerHeight, vw: innerWidth };
  });
  expect(Math.round(geometry.panel.top), "the page starts where the header ends").toBe(
    Math.round(geometry.header.bottom),
  );
  expect(Math.round(geometry.panel.bottom)).toBe(geometry.vh);
  expect(Math.round(geometry.panel.left)).toBe(0);
  expect(Math.round(geometry.panel.right)).toBe(geometry.vw);

  // …and nothing behind it is reachable. These four are the surfaces that were
  // visible above the half-open page, named individually so a failure says
  // WHICH one came back rather than "the page moved".
  for (const selector of [".ruler", ".rail", ".page-wrap", ".footer"]) {
    const probe = await hitAt(page, selector);
    expect(probe.missing, `${selector} must exist for this guard to mean anything`).toBeFalsy();
    expect(
      probe.inFilePage,
      `${selector} is still on top of the File page at ${probe.at} (hit ${probe.hit})`,
    ).toBe(true);
  }

  // The header is deliberately NOT covered: the tab strip is the way out, and
  // ONLYOFFICE leaves theirs live for the same reason.
  const onTab = await hitAt(page, "#tabHome");
  expect(onTab.inFilePage, "the tab strip must stay clickable").toBe(false);

  expect(consoleErrors).toEqual([]);
});

test("the File page is two columns, Back is the first row at the top left, and the rows are dense", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openFilePage(page);

  const layout = await page.evaluate(() => {
    const r = (sel) => {
      const el = document.querySelector(sel);
      return el ? el.getBoundingClientRect().toJSON() : null;
    };
    const rows = [...document.querySelectorAll("#filePageBody .file-page-item")].map(
      (el) => el.getBoundingClientRect(),
    );
    // The pitch between two rows of the SAME group — a group heading sits
    // between some pairs and is not a row.
    const pitches = [];
    for (let i = 1; i < rows.length; i++) {
      const d = rows[i].top - rows[i - 1].top;
      if (d > 0 && d < 60) pitches.push(Math.round(d));
    }
    return {
      rail: r(".file-page-inner"),
      detail: r(".file-page-detail"),
      back: r("#filePageBack"),
      firstRow: rows.length ? rows[0].toJSON() : null,
      rowCount: rows.length,
      minPitch: Math.min(...pitches),
    };
  });

  // Back is ONLYOFFICE's `#fm-btn-return`: the first element of the rail, at the
  // top left. It used to be a button at the top RIGHT — the one corner neither
  // ONLYOFFICE nor Word's backstage uses.
  expect(layout.back.left, "Back sits at the left gutter").toBeLessThan(40);
  expect(layout.back.top, "Back is above every command row").toBeLessThan(layout.firstRow.top);
  expect(layout.back.left).toBeLessThan(layout.rail.left + layout.rail.width / 2);

  // The rail is a column at the gutter, not a block centred in the window.
  expect(Math.round(layout.rail.left)).toBe(0);
  expect(layout.rail.width, "the rail is Docs' File-menu width, not 720px").toBeLessThan(400);

  // The pane fills the rest, so the page is never three quarters empty.
  expect(layout.detail.left).toBeGreaterThanOrEqual(layout.rail.right - 1);
  expect(layout.detail.width).toBeGreaterThan(400);

  // ONLYOFFICE's `li.fm-btn` is 28px tall with 4px beneath it, and Google Docs'
  // File menu item is 32px. Ours were 34 on a 36px pitch.
  expect(layout.rowCount).toBeGreaterThan(6);
  expect(layout.minPitch, "File rows sit on a 32px pitch").toBeLessThanOrEqual(32);

  expect(consoleErrors).toEqual([]);
});

test("the ribbon band draws no group captions, and the accessibility tree keeps every one", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);

  const captions = await page.evaluate(() => {
    const labels = [...document.querySelectorAll("#panelHome .rgroup-label")];
    return {
      count: labels.length,
      // A caption that occupies layout is a caption that is drawn. The visually
      // hidden pattern this stylesheet already uses collapses to 1x1.
      drawn: labels
        .filter((el) => el.getBoundingClientRect().height > 2)
        .map((el) => el.textContent),
      text: labels.map((el) => el.textContent.trim()),
    };
  });

  // ONLYOFFICE's `.group` (toolbar.less:498) has no caption element at all.
  expect(captions.count, "the captions must still be in the DOM").toBeGreaterThan(4);
  expect(captions.drawn, "no group caption may occupy the band").toEqual([]);
  // …and they are still the groups' accessible names, which is what doc 105 P3
  // asks for and what the overflow menu prints as its headings.
  expect(captions.text).toContain("Font");
  expect(captions.text).toContain("Paragraph");

  expect(consoleErrors).toEqual([]);
});

test("the ribbon tab strip lives in the header, and the band starts below it", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);

  const chrome = await page.evaluate(() => {
    const r = (sel) => {
      const el = document.querySelector(sel);
      return el ? el.getBoundingClientRect().toJSON() : null;
    };
    const strip = document.querySelector(".ribbon-tabs");
    return {
      inHeader: !!strip.closest("header.bar"),
      header: r("header.bar"),
      tabs: r(".ribbon-tabs"),
      title: r("#docTitle"),
      band: r("#panelHome"),
      page: r(".page-wrap"),
      clipped: strip.scrollWidth > strip.clientWidth,
    };
  });

  // The rule doc 123 §3 states: one navigation axis per chrome, and always in
  // the same place — the header, under the title. Google Docs' menu bar is
  // measurably there; ONLYOFFICE's one-level header puts the tab strip in the
  // same row as the title (`Toolbar.template`'s `.box-tabs`).
  expect(chrome.inHeader, "the tab strip belongs to the header").toBe(true);
  expect(chrome.tabs.top).toBeGreaterThanOrEqual(chrome.title.bottom - 1);
  expect(chrome.tabs.bottom).toBeLessThanOrEqual(chrome.header.bottom + 1);
  expect(chrome.tabs.left, "the strip aligns with the title above it").toBeCloseTo(
    chrome.title.left,
    0,
  );

  // The band no longer pays for a tab row of its own, and nothing overlaps.
  expect(chrome.band.top).toBeGreaterThanOrEqual(chrome.header.bottom);
  expect(chrome.page.top).toBeGreaterThan(chrome.band.bottom);
  // Every tab fits at 1280px; the fade-and-scroll fallback is for narrower
  // windows only, and a clipped strip at the design width would be a defect.
  expect(chrome.clipped, "no tab may be clipped at 1280px").toBe(false);

  expect(consoleErrors).toEqual([]);
});

test("the compact chrome keeps its menu bar, and the tab strip is not on screen", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await useCompactChrome(page);

  // Moving the strip out of `.ribbon` means `body.compact-mode .ribbon` no
  // longer hides it. One axis per chrome (docs/122) has to survive that.
  await expect(page.locator(".ribbon-tabs")).toBeHidden();
  await expect(page.locator("#appMenuBar")).toBeVisible();
  await expect(page.locator("#panelFile")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});
