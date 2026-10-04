// REFLOW, ASSERTED WHERE IT IS PAINTED (`docs/151` §6, ADR-046).
//
// Three claims are worth guarding and they are all about what is on the glass,
// not about what the model thinks. A model-tier assertion has passed through
// every real defect in this repository (`SKILL.md` §4), and this feature is
// especially prone to it: `setLayoutView` returning `{"reflow":true}` says
// nothing at all about whether a reader can read or type.
//
//   1. AT 390px THE DOCUMENT NO LONGER SCROLLS SIDEWAYS. This is the whole
//      point: `#viewport` was the one declared exception to the phone tier's
//      no-horizontal-scroll rule (`docs/148` §8), measured at `scrollWidth 794`
//      against `clientWidth 326`. It is retired by this file passing, and
//      `phone-no-horizontal-scroll.spec.mjs` now folds `#viewport` into its
//      general assertion rather than naming it.
//
//   2. THE DOCUMENT IS STILL EDITABLE IN IT. This is where we diverge from
//      ONLYOFFICE on purpose — their reader mode sets `SelectEnabled = false`
//      (`docs/151` §3.2) — so a reflow view that cannot be typed into would be a
//      reader mode wearing a different name, and it would be indistinguishable
//      from this one in any test that only asked about geometry.
//
//   3. PAGED -> REFLOW -> PAGED RETURNS THE SAME LAYOUT. Reflow is a VIEW, never
//      an edit (ADR-046 §3.1): no operation, no revision, nothing on the export
//      path. If leaving it left the document laid out differently, something on
//      the way through had written to the document — which is the silent
//      data-loss class the engineering priority order forbids outright.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  mirrorBlocks,
  openCommandPalette,
  runAppMenuCommand,
  runPaletteCommand,
} from "./fixtures.mjs";

const PHONE = { width: 390, height: 844 };
const DESKTOP = { width: 1280, height: 900 };
/** The window `docs/154` §3.2 measured the defect at: 1,408 CSS px of column,
 *  241 characters of 11pt Calibri, three times WCAG 1.4.8's 80. */
const WIDE = { width: 1440, height: 900 };

/** `gotoEditor` for a fixture other than `rich`. Same readiness condition —
 *  status clear, sheets present, the font upgrade done — so a geometry
 *  assertion here cannot race the repaint any more than one there can. */
async function gotoFixture(page, fixture) {
  await page.goto(`/editor.html?fixture=${fixture}`);
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length > 0 &&
        document.body.dataset.fontsReady === "true"
      );
    },
    null,
    { timeout: 45_000 },
  );
}

/** `#viewport`'s own overflow, read the way `phone-no-horizontal-scroll.spec.mjs`
 *  reads it, so the two files cannot disagree about what "scrolls sideways"
 *  means. */
async function documentOverflow(page) {
  return page.evaluate(() => {
    const viewport = document.getElementById("viewport");
    return {
      scrollWidth: viewport.scrollWidth,
      clientWidth: viewport.clientWidth,
      reflowing: viewport.classList.contains("is-reflow"),
    };
  });
}

/** A fingerprint of the whole laid-out document, from the DOM rather than from
 *  an app internal: the band carries the full stacked geometry (its width and
 *  height are written from `pageBandModel`), and the status bar carries the
 *  count. Together they change if any page moved, resized or appeared. */
async function layoutFingerprint(page) {
  return page.evaluate(() => {
    const band = document.querySelector(".page-band");
    return {
      width: band?.style.width ?? "",
      height: band?.style.height ?? "",
      pages: document.getElementById("statPages")?.textContent ?? "",
      pageWidthVar: document.getElementById("viewport").style.getPropertyValue("--page-width"),
    };
  });
}

/** Waits until the shell has finished a render in the requested view. The class
 *  is written by `reflow_chrome.mjs` inside the same pass that talks to the
 *  engine, so it is the shell's own report rather than a sleep. */
async function expectReflow(page, on) {
  await expect
    .poll(async () => (await documentOverflow(page)).reflowing, { timeout: 30_000 })
    .toBe(on);
}

// ---- 1. The exemption, retired ---------------------------------------------

test("at a phone width the document is reflowed and does not scroll sideways", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(PHONE);
  await gotoEditor(page);

  // Precondition, stated rather than assumed: without it this test would pass on
  // a desktop chrome that happened not to overflow, which is the shape of guard
  // `105` CQ-003 records three times.
  await expect(page.locator("body")).toHaveClass(/phone-mode/);

  const overflow = await documentOverflow(page);
  expect(overflow.reflowing, "reflow defaults ON below the phone rung (docs/151 §3.4)").toBe(true);
  expect(
    overflow.scrollWidth,
    `#viewport scrolls sideways: ${overflow.scrollWidth} into ${overflow.clientWidth}. ` +
      "That is the exemption docs/148 §8 named and docs/151 exists to retire",
  ).toBeLessThanOrEqual(overflow.clientWidth);

  // And nothing inside it is painted off the right edge either — the same
  // second question `phone-no-horizontal-scroll.spec.mjs` asks of the chrome,
  // because `overflow: hidden` would satisfy the first one while hiding the
  // document rather than fitting it.
  const spilling = await page.evaluate(() => {
    const out = [];
    for (const el of document.querySelectorAll("#viewport *")) {
      const box = el.getBoundingClientRect();
      if (box.width === 0 || box.height === 0) continue;
      if (box.right > window.innerWidth + 1) out.push(`${el.className}@${Math.round(box.right)}`);
    }
    return out;
  });
  expect(spilling, "painted past the right edge of a 390px window").toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("a document above the phone rung still gets its own paper by default", async ({ page }) => {
  // The other half of the default (`docs/151` §3.4): one person's phone must not
  // reformat another person's monitor. Without this the first test could be
  // satisfied by reflowing unconditionally.
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  expect((await documentOverflow(page)).reflowing).toBe(false);
});

// ---- 2. Still editable ------------------------------------------------------

test("the document can still be typed into in reflow, and the text lands", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(PHONE);
  await gotoEditor(page);
  await expectReflow(page, true);

  const before = await mirrorBlocks(page);
  await clickIntoFirstPage(page);
  await page.keyboard.insertText("reflowed-and-edited");

  // Asserted through the accessibility mirror, which is built from the MODEL and
  // not from the canvas, so this says the keystroke reached the document — the
  // thing a reader would call editing — rather than that some pixels changed.
  await expect
    .poll(async () => (await mirrorBlocks(page)).some((t) => t.includes("reflowed-and-edited")))
    .toBe(true);
  const after = await mirrorBlocks(page);
  expect(after.length, "typing must not have restructured the document").toBe(before.length);
  expect(consoleErrors).toEqual([]);
});

// ---- 3. A view, not an edit -------------------------------------------------

test("Paged to Reflow and back returns the document to the same layout", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await expectReflow(page, false);
  const paper = await layoutFingerprint(page);

  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);
  const reflowed = await layoutFingerprint(page);
  expect(reflowed, "entering reflow must actually change the layout").not.toEqual(paper);

  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, false);
  expect(
    await layoutFingerprint(page),
    "leaving reflow left the document laid out differently — something wrote to it",
  ).toEqual(paper);

  // And no edit was recorded: reflow issues no operation and bumps no revision,
  // so there is nothing to undo and the document is not dirty.
  await expect(page.locator("#undoBtn")).toBeDisabled();
  expect(consoleErrors).toEqual([]);
});

// ---- The command has more than one face ------------------------------------

test("view.reflow is reachable from the ribbon and from the View menu, and both agree", async ({
  page,
  consoleErrors,
}) => {
  // `SKILL.md` §10: a capability reachable from one surface is this repo's
  // recurring defect. Driven from BOTH, on the same document, and required to
  // produce the same state.
  //
  // The RIBBON face goes first, deliberately: `runAppMenuCommand` switches the
  // editor into the compact chrome and stays there, so the other order would
  // leave the ribbon hidden and this test would be measuring which chrome is on.
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await expectReflow(page, false);

  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);
  await expect(page.locator("#viewReflowBtn")).toHaveAttribute("aria-pressed", "true");
  const fromRibbon = await layoutFingerprint(page);

  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, false);

  await runAppMenuCommand(page, "view", "view.reflow");
  await expectReflow(page, true);
  expect(await layoutFingerprint(page), "the two faces must be two faces of ONE command").toEqual(
    fromRibbon,
  );
  expect(consoleErrors).toEqual([]);
});

test("the toggle says what reflow costs rather than letting the chrome vanish", async ({
  page,
}) => {
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  const title = await page.locator("#viewReflowBtn").getAttribute("title");
  expect(title, "the control must carry its state").toMatch(/off/i);
  expect(title, "and what it withholds — the ruler and the Pages panel").toMatch(
    /ruler.*Pages panel/i,
  );
});

// ---- The chrome that has to stand down -------------------------------------

test("the ruler and the Pages panel are withheld in reflow, each with a reason", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await expect(page.locator(".ruler")).toBeVisible();
  await page.locator("#railPages").click();
  await expect(page.locator("#pagesPanel")).toBeVisible();

  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);

  // The ruler: gone, and recorded as withheld rather than merely absent — an
  // absent ruler is also what "no document open" looks like.
  await expect(page.locator(".ruler")).toBeHidden();
  // Its OWN sentence, not the Pages panel's: a reader who went looking for the
  // ruler is told where tab stops and indents went, which is the thing they
  // actually wanted from it.
  expect(await page.locator(".ruler").getAttribute("data-withheld")).toMatch(/ruler.*margins/i);

  // The Pages panel: closed, its tile disabled and carrying the reason, and the
  // command itself disabled rather than silently doing nothing.
  await expect(page.locator("#pagesPanel")).toBeHidden();
  await expect(page.locator("#railPages")).toBeDisabled();
  expect(await page.locator("#railPages").getAttribute("title")).toMatch(/reflow/i);

  await openCommandPalette(page);
  const row = page.locator('#cmdList .cmd-item[data-command-id="view.pages"]');
  await expect(row).toBeDisabled();
  await page.keyboard.press("Escape");

  // And it all comes back. The View tab is re-selected first because opening the
  // palette went through the File page, which takes the ribbon's place.
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, false);
  await expect(page.locator(".ruler")).toBeVisible();
  await expect(page.locator("#railPages")).toBeEnabled();
  expect(consoleErrors).toEqual([]);
});

test("the status bar does not print a tile index as a page number", async ({ page }) => {
  // `docs/151` §6.5. In reflow the caret's "page" is a TILE, cut at a line
  // boundary wherever 11in of column happens to end. "Page 3 of 12" built from
  // those numbers is wrong in both halves and indistinguishable from one that is
  // right, which is the class of claim `SKILL.md` §9 exists to stop.
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await expect(page.locator("#statPages")).toHaveText(/of/i);

  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);
  await clickIntoFirstPage(page);
  await expect(page.locator("#statPages")).not.toHaveText(/of\s+\d/i);
  await expect(page.locator("#statPages")).toHaveText(/%/);
});

// ---- The tiles read as one column ------------------------------------------

test("tiles are stacked with no seam between them", async ({ page, consoleErrors }) => {
  // The `demo` fixture at a PHONE width, because that is the combination that
  // produces more than one tile: a tile is 11in of column, and `rich` reflowed to
  // 390px is 232px tall — one tile, on which a guard about the gap BETWEEN tiles
  // would assert nothing at all while passing. `demo` is 9,690px, so it has
  // several. The count is asserted below rather than assumed, because that is
  // the premise the whole test rests on.
  await page.setViewportSize(PHONE);
  await gotoFixture(page, "demo");
  await expectReflow(page, true);

  const seams = await page.evaluate(() => {
    const sheets = [...document.querySelectorAll(".page-band > .page-wrap")].sort(
      (a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top,
    );
    const gaps = [];
    for (let i = 1; i < sheets.length; i += 1) {
      const above = sheets[i - 1].getBoundingClientRect();
      const below = sheets[i].getBoundingClientRect();
      if (Math.abs(below.top - above.bottom) > 1) gaps.push(below.top - above.bottom);
    }
    return {
      count: sheets.length,
      gaps,
      shadow: sheets.length ? getComputedStyle(sheets[0]).boxShadow : "",
      radius: sheets.length ? getComputedStyle(sheets[0]).borderRadius : "",
    };
  });
  expect(seams.count, "the fixture must produce more than one tile, or this checks nothing")
    .toBeGreaterThan(1);
  expect(seams.gaps, "a gap between two tiles is a white band across a paragraph").toEqual([]);
  expect(seams.shadow, "a sheet edge drawn mid-sentence").toBe("none");
  expect(seams.radius).toBe("0px");
  expect(consoleErrors).toEqual([]);
});

// ---- THE CAP, AT THE PAINT TIER (`docs/154` §5.1, ADR-048) ------------------
//
// The measure-tier arithmetic — 80 characters, 7,024 twips, which step caps
// where — belongs to `tests/reflow_view.test.mjs`. What is here is what a reader
// is looking at, because a pure function returning 7,024 says nothing about
// whether somebody is reading a 1,408px line.
//
// A WARNING ABOUT THE SHAPE OF THESE GUARDS. Three of the first reflow guards
// measured the FIXTURE rather than the guarantee and passed for the wrong reason,
// and `docs/154` §3.2 records the same failure for the whole feature: the design
// was only ever evaluated at 390px, where the missing cap does not bite. So each
// test below CREATES the condition it needs — a wide window, a chosen step — and
// states its precondition as an assertion rather than inheriting whatever the
// default happens to produce.

/** The painted column: the tile's own box and where it sits in the viewport.
 *
 *  Read from the SHEET the engine rastered, not from an app internal, because the
 *  width a reader's eye measures is the width of that box. */
async function paintedColumn(page) {
  return page.evaluate(() => {
    const viewport = document.getElementById("viewport");
    const sheet = document.querySelector(".page-band > .page-wrap");
    if (!sheet) return null;
    const box = sheet.getBoundingClientRect();
    const port = viewport.getBoundingClientRect();
    return {
      width: Math.round(box.width),
      clientWidth: viewport.clientWidth,
      scrollWidth: viewport.scrollWidth,
      // Slack on each side, in the viewport's own box. Equal slack is centred.
      leftGap: Math.round(box.left - port.left),
      rightGap: Math.round(port.right - box.right),
    };
  });
}

/** WHAT COLOUR IS PAINTED INSIDE THE MEASURE AND BESIDE IT.
 *
 *  The `inside` half is read out of the RASTER — `getImageData` on the sheet's
 *  own canvas, which `main.js` fills with `putImageData` from the engine's
 *  pixels — because the colour a reader's eye measures inside the column is the
 *  colour the engine painted, not the colour a stylesheet declares. Sampled two
 *  device pixels in, which is inside the tile's 16px gutter, so it is the
 *  surface rather than a glyph.
 *
 *  The `left`/`right` halves are read the way a browser resolves a painted
 *  background: take the element at that point and walk up until something
 *  declares a background that is not transparent. `.pages` and `.page-band`
 *  declare none, so whatever answers is the thing actually painting the field
 *  beside the column.
 *
 *  Returns `null` when there is no raster yet, so a caller asserts on a measured
 *  colour or on nothing — never on a default. */
async function surfaceColours(page) {
  return page.evaluate(() => {
    const viewport = document.getElementById("viewport");
    const wrap = document.querySelector(".page-band > .page-wrap");
    const canvas = wrap?.querySelector("canvas.page");
    if (!viewport || !wrap || !canvas) return null;
    const box = wrap.getBoundingClientRect();
    const port = viewport.getBoundingClientRect();

    const pixel = canvas.getContext("2d").getImageData(2, Math.floor(canvas.height / 2), 1, 1).data;
    const painted = (x, y) => {
      let el = document.elementFromPoint(x, y);
      while (el) {
        const bg = getComputedStyle(el).backgroundColor;
        const parts = bg.match(/[\d.]+/g)?.map(Number) ?? [];
        // A fully transparent background paints nothing; keep walking up.
        if (parts.length >= 3 && (parts[3] === undefined || parts[3] > 0)) {
          return { rgb: parts.slice(0, 3), from: el.id || el.className || el.tagName };
        }
        el = el.parentElement;
      }
      return { rgb: null, from: null };
    };
    const midY = Math.round(Math.max(port.top + 4, Math.min(port.bottom - 4, box.top + box.height / 2)));
    return {
      inside: { rgb: [pixel[0], pixel[1], pixel[2]], alpha: pixel[3], from: "raster" },
      left: painted(Math.round(port.left + 4), midY),
      right: painted(Math.round(port.right - 4), midY),
      leftGap: Math.round(box.left - port.left),
      rightGap: Math.round(port.right - box.right),
      boundary: {
        boxShadow: getComputedStyle(wrap).boxShadow,
        borderRadius: getComputedStyle(wrap).borderRadius,
        canvasRadius: getComputedStyle(canvas).borderRadius,
      },
    };
  });
}

/** Asserts the guarantee this view exists to provide: ONE surface, so the
 *  measure is where the text is and not where the paper is.
 *
 *  Stated as "the same colour on both sides of the column as inside it", which
 *  is a guarantee, rather than as "there are 0px of desk", which is a
 *  measurement that would redden on a change that removed nothing. The
 *  precondition — that there is a field beside the column at all to be the wrong
 *  colour — is asserted explicitly rather than inherited. */
function expectOneSurface(surface, where) {
  expect(surface, `${where}: no raster to sample`).not.toBeNull();
  expect(surface.inside.alpha, `${where}: the raster is not opaque`).toBe(255);
  // The precondition. Without a field on each side this would pass on a tile
  // that filled the window — which is exactly how a 390px-only evaluation hid
  // the missing cap (`docs/154` §3.2).
  expect(surface.leftGap, `${where}: nothing beside the column to be a desk`).toBeGreaterThan(20);
  expect(surface.rightGap, `${where}: nothing beside the column to be a desk`).toBeGreaterThan(20);
  for (const side of ["left", "right"]) {
    expect(
      surface[side].rgb,
      `${where}: the field on the ${side} of the measure is ${JSON.stringify(surface[side].rgb)} ` +
        `(painted by ${surface[side].from}) against ${JSON.stringify(surface.inside.rgb)} inside ` +
        "it. A differently-coloured field around a white rectangle is a page " +
        "silhouette, which is the one thing reflow must not be",
    ).toEqual(surface.inside.rgb);
  }
  // And nothing draws the edge the colours no longer draw.
  expect(surface.boundary.boxShadow, `${where}: a tile with a sheet shadow`).toBe("none");
  expect(surface.boundary.borderRadius, `${where}: a tile with rounded corners`).toBe("0px");
  expect(surface.boundary.canvasRadius, `${where}: a raster with rounded corners`).toBe("0px");
}

/** Chooses a width step through the ribbon popover and returns the column once
 *  the engine has been re-shaped at it. Polled on `aria-checked` rather than
 *  slept on, so it waits for the control's own report. */
async function chooseWidth(page, step) {
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewTextWidthBtn").click();
  await expect(page.locator("#textWidthMenu")).toBeVisible();
  await page.locator(`#textWidthMenu [data-text-width="${step}"]`).click();
  await expect
    .poll(async () =>
      page.locator(`#textWidthMenu [data-text-width="${step}"]`).getAttribute("aria-checked"),
    )
    .toBe("true");
  return paintedColumn(page);
}

test("in Reading at 1440px the column is capped and CENTRED, not window-wide", async ({
  page,
  consoleErrors,
}) => {
  // THE DEFECT, at the tier it is visible. `docs/154` §3.2 measured 1,408 CSS px of
  // column at this window — 241 characters of 11pt Calibri, three times WCAG 2.1
  // SC 1.4.8's 80. The cap is 468px of text plus two 16px gutters.
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  await expectReflow(page, false);

  const paper = await paintedColumn(page);
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);

  const reading = await chooseWidth(page, "reading");
  // The precondition, stated rather than assumed: a window wide enough for the cap
  // to bind at all. Without it this would pass at 390px, which is exactly how the
  // defect survived every guard in the repository.
  expect(
    reading.clientWidth,
    "the window must be wider than the cap, or nothing is being tested",
  ).toBeGreaterThan(700);

  expect(
    Math.abs(reading.width - 500),
    `the painted column is ${reading.width}px; the Reading cap is 468px of text plus ` +
      "two 16px gutters",
  ).toBeLessThan(40);
  expect(
    reading.width,
    `${reading.width}px of a ${reading.clientWidth}px window is the uncapped column ` +
      "docs/154 §3.2 measured at 241 characters",
  ).toBeLessThan(reading.clientWidth * 0.6);

  // Centred, so the measure sits in the middle of the surface rather than
  // against one edge.
  expect(
    Math.abs(reading.leftGap - reading.rightGap),
    `the column sits ${reading.leftGap}px from the left and ${reading.rightGap}px from ` +
      "the right — a capped column pinned to one edge is half the fix",
  ).toBeLessThanOrEqual(2);

  // THIS ASSERTION USED TO READ `expect(reading.leftGap).toBeGreaterThan(100)`
  // — "there is real desk on each side" — and it was guarding the behaviour the
  // owner rejected. A desk is what makes the capped column a PAGE: a #ffffff
  // tile in a #eef1f7 (or #141619) field is a sheet with its shadow filed off,
  // and the surface Docs' pageless presents is one region, which is why Google
  // renames *Page color* to *Background color* there
  // (support.google.com/docs/answer/10296604, first-party).
  // The guarantee that replaces it is that the surface is one colour.
  expectOneSurface(await surfaceColours(page), "Reading at 1440px");

  expect(reading.scrollWidth).toBeLessThanOrEqual(reading.clientWidth);
  // The fixture's own paper is wider than the cap, so this is a narrowing of the
  // reading column and not a coincidence of the fixture's geometry.
  expect(paper.width, "the fixture's paper must be wider than the cap").toBeGreaterThan(
    reading.width,
  );
  expect(consoleErrors).toEqual([]);
});

test("at 390px every width step still fills the window, with no horizontal scroll", async ({
  page,
  consoleErrors,
}) => {
  // ADR-044's retired `#viewport` exemption, guarded against the one thing that
  // could bring it back. A phone-only evaluation is what hid the missing cap, so
  // the cap gets a phone guard in the other direction: the two POLICIES and Full
  // all reduce to `available` at the rung, and NO step may paint wider than the
  // window.
  //
  // Driven from the PALETTE, not the ribbon or the menu bar: the phone rung
  // withholds both the ribbon and `#modeCompact`, and the palette belongs to no
  // chrome region. That is also the surface a phone reader has.
  await page.setViewportSize(PHONE);
  await gotoEditor(page);
  await expect(page.locator("body")).toHaveClass(/phone-mode/);
  await expectReflow(page, true);
  const uncapped = await paintedColumn(page);
  expect(uncapped.width, "there must be a tile to measure").toBeGreaterThan(0);

  for (const step of ["reading", "fit", "full", "narrow"]) {
    await runPaletteCommand(page, `view.textWidth.${step}`, "text width");
    await expect.poll(async () => (await paintedColumn(page)) !== null).toBe(true);
    const painted = await paintedColumn(page);
    expect(
      painted.scrollWidth,
      `${step}: #viewport scrolls sideways, ${painted.scrollWidth} into ${painted.clientWidth}`,
    ).toBeLessThanOrEqual(painted.clientWidth);
    expect(painted.width, `${step} painted wider than a 390px window`).toBeLessThanOrEqual(
      painted.clientWidth,
    );
    // `narrow` is deliberately narrower than a phone's own measure — 55 characters
    // against the rung's 60 — and `reflow_view.test.mjs` records why that is a
    // finding rather than a regression. Everything else must be unchanged.
    if (step === "narrow") continue;
    expect(
      painted.width,
      `${step} changed the phone's column from ${uncapped.width}px — the 60 characters ` +
        "docs/154 §3.2 calls correct",
    ).toBe(uncapped.width);
  }
  expect(consoleErrors).toEqual([]);
});

test("a viewport past the old 22in refusal lays out instead of refusing", async ({
  page,
  consoleErrors,
}) => {
  // DEFECT TWO (`docs/154` §3.3), at the tier the reader met it. `LayoutView::reflow`
  // refuses a column over 22in; with no ceiling upstream, 1,104px at 50% zoom asked
  // for more, `sync` caught the throw, reverted the preference to paper, and the
  // reader was told something about twips. 50% is two clicks away — both a
  // `ZOOM_STEPS` entry and the value of `FIT_ON_OPEN_FLOOR`.
  //
  // Asserted with FULL chosen, because that is the only step that does not cap:
  // the other three would pass this while the refusal was still reachable through
  // the new control, which is the trap a step-blind version of this test falls in.
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);
  await chooseWidth(page, "full");

  // Down to 50%, one `ZOOM_STEPS` rung at a time, which is how a reader gets
  // there: 100 -> 90 -> 75 -> 50. Asserted on arrival rather than assumed, because
  // the step list is free to change and a test that silently stopped at 75% would
  // be measuring a window that was never refused.
  for (let click = 0; click < 6; click += 1) {
    if ((await page.locator("#zoom").inputValue()).startsWith("50")) break;
    await page.locator("#viewZoomOut").click();
  }
  await expect
    .poll(async () => page.locator("#zoom").inputValue())
    .toMatch(/^50/);

  // The engine laid it out: still reflowing, a tile on screen, and the status line
  // is not carrying the engine's refusal.
  await expectReflow(page, true);
  const painted = await paintedColumn(page);
  expect(painted, "no tile at all — the engine refused and the band is empty").not.toBeNull();
  expect(painted.width).toBeGreaterThan(0);
  const status = page.locator("#status");
  await expect(status).not.toHaveClass(/error/);
  expect(await status.textContent(), "the twips refusal reached the reader").not.toMatch(/twip/i);
  // And the toggle still claims the state the engine holds, which is what `sync`'s
  // catch used to have to undo.
  await expect(page.locator("#viewReflowBtn")).toHaveAttribute("aria-pressed", "true");
  expect(painted.scrollWidth).toBeLessThanOrEqual(painted.clientWidth + 1);
  expect(consoleErrors).toEqual([]);
});

test("Reading to Paper and back to Reading returns the same column", async ({
  page,
  consoleErrors,
}) => {
  // A width is a VIEW, like reflow itself: no operation, no revision, nothing on
  // the export path. If a round trip through another step left the column
  // somewhere else, something on the way through had written to the document — or
  // a cap was being computed from state it had itself moved.
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);

  const first = await chooseWidth(page, "reading");
  const paper = await chooseWidth(page, "fit");
  expect(
    paper.width,
    "Paper and Reading must be different widths here, or this checks nothing",
  ).not.toBe(first.width);
  const again = await chooseWidth(page, "reading");
  expect(again.width, "a round trip moved the column").toBe(first.width);
  expect(again.leftGap, "a round trip moved the column off centre").toBe(first.leftGap);
  await expect(page.locator("#undoBtn")).toBeDisabled();
  expect(consoleErrors).toEqual([]);
});

test("the width control is reachable from the ribbon and the View menu, and both agree", async ({
  page,
  consoleErrors,
}) => {
  // `SKILL.md` §10's ≥2-surfaces floor, and the recurring defect it answers.
  // Driven from BOTH on the same document and required to produce the same painted
  // column — two faces of ONE command, not two commands that look alike. The
  // ribbon goes first because `runAppMenuCommand` switches to compact chrome and
  // stays there.
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);

  const fromRibbon = await chooseWidth(page, "narrow");
  const full = await chooseWidth(page, "full");
  expect(full.width, "the two steps must differ, or the menu could change nothing").not.toBe(
    fromRibbon.width,
  );
  await runAppMenuCommand(page, "view", "view.textWidth.narrow");
  await expect.poll(async () => (await paintedColumn(page)).width).toBe(fromRibbon.width);
  expect(consoleErrors).toEqual([]);
});

test("the width control is disabled WITH A REASON on paper, never dead", async ({ page }) => {
  // A text width is meaningless in paged layout — there the measure is the
  // document's own — so the control stands down. "Never a dead control": it says
  // why rather than sitting greyed and silent.
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await expectReflow(page, false);
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  const button = page.locator("#viewTextWidthBtn");
  await expect(button).toBeDisabled();
  expect(await button.getAttribute("title")).toMatch(/reflow/i);

  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);
  await expect(button).toBeEnabled();
  // And it names the step in effect, so the default's narrowness is discoverable
  // rather than mysterious.
  expect((await button.textContent()).trim()).not.toBe("");
  expect(await button.getAttribute("title")).toMatch(/80 characters/i);
});

// ---- 5. The surface, and the panel that navigates it ------------------------
// `docs/151` §6.2 corrected, and item 3 of the owner's own description: "no
// separate of page with background, all became uniform same colour, not visible
// boundary, like an outline of left and whole scroll on right with no concept of
// pages." The light-theme half of the surface is asserted inside the capped
// column test above, where the guard it replaces used to live.

test("the reflow surface is one colour in the DARK theme too, where a desk shows most", async ({
  page,
  consoleErrors,
}) => {
  // The theme a light-only e2e suite cannot see, and the most visible instance
  // of the defect: a 500px #ffffff strip in a #141619 room. The surface joins
  // the PAPER layer rather than the chrome palette, so a dark reader gets a
  // white surface — deliberate, and the only answer that keeps the `--paper-*`
  // marker family contrast-safe without an ink rule.
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  await page.addStyleTag({
    content: "*, *::before, *::after { transition: none !important; animation: none !important; }",
  });
  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  // THE PRECONDITION, and it is the whole reason this test is not vacuous: in
  // PAPER the two regions must differ. Without it a stylesheet that had never
  // themed the desk at all would satisfy everything below.
  const paper = await surfaceColours(page);
  expect(paper, "no raster in the paged view").not.toBeNull();
  expect(
    paper.left.rgb,
    `the dark desk is ${JSON.stringify(paper.left.rgb)} against ` +
      `${JSON.stringify(paper.inside.rgb)} of paper — if those are equal the dark ` +
      "theme is not painting a desk and nothing here is being tested",
  ).not.toEqual(paper.inside.rgb);

  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);
  await chooseWidth(page, "reading");
  expectOneSurface(await surfaceColours(page), "Reading at 1440px, dark theme");

  // The scrollbar is the first thing a white surface breaks: `--scrollbar-thumb`
  // is a CHROME token, so in dark theme it is a near-white translucent thumb and
  // the document scroller would have no visible thumb at all over the surface.
  const thumb = await page.evaluate(() => {
    const viewport = document.getElementById("viewport");
    return getComputedStyle(viewport).scrollbarColor;
  });
  expect(thumb, "the reflow scroller kept the chrome thumb over a white track").toMatch(
    /rgba?\(\s*74[,\s]/,
  );
  expect(consoleErrors).toEqual([]);
});

test("turning reflow on opens the outline, and the reader's close STICKS", async ({
  page,
  consoleErrors,
}) => {
  // There are no page numbers in reflow, so the outline is the navigation that
  // replaces them — item 3 of the owner's description, and orthogonal-to-mode in
  // Docs (support.google.com/docs/answer/6367684, first-party, which describes
  // the outline with no mode qualifier). It is asked for once per transition INTO
  // reflow rather than enforced per render, which is the difference between a
  // default and a panel the reader cannot get rid of.
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  await expectReflow(page, false);
  const outline = page.locator("#outlinePanel");
  // The precondition: on paper it is shut, so "it is open" below is this view's
  // doing and not the app's startup state.
  await expect(outline, "the outline is not supposed to be open on paper").toBeHidden();

  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);
  await expect(outline).toBeVisible();
  // And the rail tile agrees — a panel opened without its tile leaves a control
  // that says the panel is shut while it is on screen.
  await expect(page.locator("#railOutline")).toHaveAttribute("aria-pressed", "true");
  // Built, not merely shown: an empty panel that happens to be visible is not
  // navigation (`SKILL.md` §9.4).
  await expect(page.locator("#outlineBody .outline-tree[role='tree']")).toBeVisible();

  // The reader still owns it. A width step is a full O(document) re-render in
  // reflow, so if the request were made per render rather than per transition
  // the panel would come back and the close would be unclosable.
  await page.locator("#outlineClose").click();
  await expect(outline).toBeHidden();
  await chooseWidth(page, "narrow");
  await expectReflow(page, true);
  await expect(outline, "the view re-opened a panel the reader had closed").toBeHidden();

  // Leaving and re-entering reflow is a new transition, so it asks again.
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, false);
  await page.locator("#viewReflowBtn").click();
  await expectReflow(page, true);
  await expect(outline).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("at the phone rung reflow does NOT open the outline, because there is no left there", async ({
  page,
  consoleErrors,
}) => {
  // The boundary of the rule above, and it is not a detail: `.side-panel` is a
  // bottom SHEET at this rung (`style.css` `body.phone-mode .side-panel`,
  // `max-height: 55vh`), so auto-opening it would cover more than half of the
  // document it exists to navigate — on boot, since reflow is on by default
  // here. "Outline left, document right" has no left in a 390px window.
  await page.setViewportSize(PHONE);
  await gotoEditor(page);
  await expect(page.locator("body")).toHaveClass(/phone-mode/);
  await expectReflow(page, true);
  await expect(page.locator("#outlinePanel")).toBeHidden();
  // And the document still fills the window rather than sharing it.
  const painted = await paintedColumn(page);
  expect(painted.scrollWidth).toBeLessThanOrEqual(painted.clientWidth);
  expect(consoleErrors).toEqual([]);
});

// ---- 6. What the view approximates, asked for and answered -----------------

/** The exact English of `reflowNotes.none`. Written out rather than imported so
 *  this spec fails if the sentence changes without anyone re-reading what it is
 *  claiming to a reader; the catalogue coverage is `locale_coverage.test.mjs`'s
 *  job, not this file's. */
const NOTHING_APPROXIMATED =
  "Nothing: every part of this document is laid out the way the document asks for it.";

test("the view says what it approximates, from two surfaces, in both views", async ({
  page,
  consoleErrors,
}) => {
  // THE DEFECT THIS CLOSES. `LayoutView::approximations` has reported what reflow
  // approximates since reflow shipped, `reflow_chrome.mjs` held the list — and
  // nothing in the product called the accessor. One getter, zero callers: the
  // engine's report reached no reader through any surface, in any view, behind
  // any click (`docs/151` §8 item 8, `SKILL.md` §9.4 — built is not reachable).
  //
  // So what is asserted here is REACHABILITY and TRUTHFULNESS, not wording:
  // the row exists on two surfaces, it is enabled in both views rather than
  // greyed, and in the paged view — where nothing IS approximated — it says so
  // instead of reciting the reflow list. That last clause is what makes this a
  // guard on the derivation rather than on the plumbing: the list it replaced
  // was a fixed constant that would have answered a paged reader with three
  // sentences about tiles.
  await page.setViewportSize(DESKTOP);
  await gotoEditor(page);
  await expectReflow(page, false);

  // Surface 1: the View menu, beside the reflow toggle, which is where the
  // question arises.
  await runAppMenuCommand(page, "view", "view.reflowApproximations");
  await expect(page.locator("#status")).toHaveText(NOTHING_APPROXIMATED);
  await expect(page.locator("#status")).not.toHaveClass(/error/);

  // Surface 2: the palette. `runPaletteCommand` asserts the row is offered AND
  // enabled before clicking it, so a dead control fails here rather than
  // silently doing nothing.
  await runPaletteCommand(page, "view.reflowApproximations", "approximates");
  await expect(page.locator("#status")).toHaveText(NOTHING_APPROXIMATED);

  // And in reflow it answers about the document rather than refusing. The
  // sentences are the engine's, and which of them apply is this document's
  // business — so the assertion is that SOMETHING is said and that the control
  // is not withheld, which is the part the shell owns.
  // Through the command rather than the ribbon button: `runAppMenuCommand` above
  // switched the shell into compact chrome, where the ribbon's `#viewReflowBtn`
  // is not painted. Driving the registry row is also the better test of the
  // pair — the two rows are faces of one command either way.
  await runPaletteCommand(page, "view.reflow", "reflow");
  await expectReflow(page, true);
  await runPaletteCommand(page, "view.reflowApproximations", "approximates");
  await expect(page.locator("#status")).not.toBeEmpty();
  await expect(page.locator("#status")).not.toHaveClass(/error/);

  expect(consoleErrors).toEqual([]);
});
