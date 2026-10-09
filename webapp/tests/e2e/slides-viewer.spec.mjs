// A real `.pptx` opens in a real browser and paints.
//
// Everything below the page has its own guards — the importer's 41, the layout
// crate's 35, the facade's 9 parity tests — and not one of them proves a browser
// can open a deck. That is what this file is for, and it is the only place in the
// repository where the whole chain runs: bytes, WebAssembly, canvas, pixels.
//
// # Why the assertions are on pixels and positions rather than on presence
//
// "A canvas exists" passes on a blank one. This session has already corrected two
// guards that measured the wrong thing — one asserted on alpha where the surface
// starts opaque, so every pixel read 255 and an unpainted bitmap passed; one
// checked that a bitmap appeared, which a facade ignoring its `dpi` argument also
// satisfies. So each test here names a value the deck itself states.

import { expect, test } from "@playwright/test";
// The suite's one box reader — see `box_read_contract.test.mjs`.
import { stableBox } from "./fixtures.mjs";

/** The generated fixture, staged by `build.sh` beside the page. */
const DECK = "demo.pptx";

/** Opens the viewer and waits for the engine to boot. */
async function gotoSlides(page) {
  await page.goto("/slides.html");
  // The engine is megabytes of WebAssembly; the open control is inert until it
  // has booted. Waiting on the control rather than on a timer is the difference
  // between a spec that is slow and a spec that is flaky.
  await expect(page.locator("#slidesFile")).toBeAttached({ timeout: 45_000 });
  await page.waitForFunction(() => document.querySelector('[data-command="file.save"]') !== null, null, {
    timeout: 45_000,
  });
}

/** Opens the fixture deck through the page's own file input. */
async function openDeck(page) {
  const response = await page.request.get(`/${DECK}`);
  expect(response.ok(), `${DECK} must be staged by build.sh`).toBe(true);
  const bytes = await response.body();
  await page.locator("#slidesFile").setInputFiles({
    name: DECK,
    mimeType: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    buffer: bytes,
  });
  await expect(page.locator("#slideCanvas")).toBeVisible({ timeout: 30_000 });
}

test("a deck opens and its first slide paints real pixels", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);

  // The empty state is gone and the canvas has a backing store.
  await expect(page.locator("#slidesEmpty")).toBeHidden();
  const size = await page.locator("#slideCanvas").evaluate((canvas) => ({
    backing: canvas.width,
    css: Math.round(canvas.getBoundingClientRect().width),
  }));
  expect(size.backing).toBeGreaterThan(0);
  expect(size.css).toBeGreaterThan(0);

  // Something was PAINTED, measured on the colour channel. The slide surface is
  // opaque white, so an alpha assertion would pass for a blank canvas — the exact
  // mistake the raster guards in `casual-doc-render` were corrected for.
  const nonWhite = await page.locator("#slideCanvas").evaluate((canvas) => {
    const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
    let count = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] !== 255 || data[i + 1] !== 255 || data[i + 2] !== 255) count += 1;
    }
    return count;
  });
  expect(nonWhite, "the slide must actually paint, not just allocate a canvas").toBeGreaterThan(
    100,
  );
});

test("the sorter lists the deck in presentation order, not part order", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);

  // The card's accessible name is the slide's name; the caption under it is the
  // slide's number, as in the editor's page navigator.
  const labels = await page
    .locator("#slidesSorter .page-thumb")
    .evaluateAll((cards) => cards.map((card) => card.getAttribute("aria-label")));
  const numbers = await page.locator("#slidesSorter .page-thumb-num").allTextContents();
  expect(numbers).toEqual(["1", "2", "3"]);
  // The fixture's parts are slide1 ("Opening"), slide2 ("Detail") and slide10
  // ("Appendix"), presented 1, 2, 10. EVERY lexical ordering of those part names
  // gives 1, 10, 2 — "Opening", "Appendix", "Detail" — so a page built on part
  // order shows a different, entirely plausible deck. That is why this asserts
  // the sequence and not the count.
  expect(labels).toEqual(["Opening", "Detail", "Appendix"]);

  // A hidden slide is still IN the deck — retained, saved and sortable — so it is
  // marked rather than omitted. Omitting it would leave an author unable to see
  // what their own file contains.
  await expect(page.locator("#slidesSorter .slide-thumb-hidden")).toHaveCount(1);
});

test("arrow keys page through the deck and the position follows", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);

  const position = page.locator("#slidesPosition");
  await expect(position).toHaveText(/1.*3/);

  await page.locator("#slideStage").focus();
  await page.keyboard.press("ArrowRight");
  await expect(position).toHaveText(/2.*3/);
  await page.keyboard.press("End");
  await expect(position).toHaveText(/3.*3/);
  // End on the last slide and Home on the first are ordinary keystrokes: they
  // clamp rather than erroring, or the keyboard feels broken at both ends of
  // every deck.
  await page.keyboard.press("ArrowRight");
  await expect(position).toHaveText(/3.*3/);
  await page.keyboard.press("Home");
  await expect(position).toHaveText(/1.*3/);

  // The current slide is announced by STATE, not only by styling: a reader with a
  // screen reader has no idea which of forty thumbnails is showing otherwise.
  await expect(page.locator('#slidesSorter .page-thumb[aria-current="page"]')).toHaveCount(1);
});

test("clicking a thumbnail shows that slide", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);

  await page.locator("#slidesSorter .page-thumb").nth(2).click();
  await expect(page.locator("#slidesPosition")).toHaveText(/3.*3/);
  // `"page"`, not `"true"`: the mark is set by `reflectPagesPanelSelection`, the
  // document navigator's own reflection, reused rather than reimplemented — so
  // the deck's sorter and the document's page panel say the same thing to a
  // screen reader instead of two surfaces inventing two vocabularies.
  await expect(
    page.locator('#slidesSorter .page-thumb').nth(2),
  ).toHaveAttribute("aria-current", "page");
});

test("the fidelity report says what was not recovered", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);

  // Surfaced rather than hidden. This is the one claim this engine can make that
  // a converter cannot, and a page that opened silently would throw it away. The
  // count rides in the header chip the editor already uses for its own
  // import/export findings — a neutral status chip, never an alert.
  await expect(page.locator("#slidesFidelity")).toBeVisible();
  const chip = await page.locator("#slidesFidelity").textContent();
  expect(chip?.trim().length ?? 0).toBeGreaterThan(0);

  // The detail lives in a rail panel, as every other list on this shell does.
  // The fixture deliberately carries constructs this build does not cover, so the
  // report must be non-empty — an empty one would be the overstatement `SKILL` §9
  // forbids rather than good news.
  await expect(page.locator("#slidesFidelityPanel")).toBeHidden();
  await page.locator("#railFidelity").click();
  await expect(page.locator("#slidesFidelityPanel")).toBeVisible();
  await expect(page.locator("#railFidelity")).toHaveAttribute("aria-pressed", "true");
  const findings = await page.locator("#slidesFidelityList li").count();
  expect(findings, "the fixture loses things; the report must name them").toBeGreaterThan(0);
});

test("a file that is not a presentation is refused, visibly", async ({ page }) => {
  await gotoSlides(page);

  await page.locator("#slidesFile").setInputFiles({
    name: "not-a-deck.pptx",
    mimeType: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    buffer: Buffer.from("this is not a ZIP container at all"),
  });

  // The engine refuses a package it cannot read correctly instead of opening it
  // wrong, and the page shows that refusal in BOTH halves: the status bar a
  // reader sees, and the body-level live region assistive technology hears. Two
  // elements and not one for the reason `109` UX-017 records on the document
  // side — the visible strip sheds indicators as the window narrows, and a
  // `display: none` subtree is not in the accessibility tree.
  await expect(page.locator("#status")).not.toBeEmpty({ timeout: 30_000 });
  await expect(page.locator("#slidesError")).not.toBeEmpty();
  // Save is the registry's command on every surface — the ribbon button here —
  // and with nothing open it is disabled, saying why, rather than missing.
  await expect(page.locator('#slidesRibbonBody [data-command="file.save"]').first()).toBeDisabled();
  // The SHELL stays. An earlier revision cleared `doc-loaded` on a refusal, which
  // took the toolbar, the rail and the status bar off screen with it — so the one
  // moment a reader most needs to see what the page is and try again was the one
  // moment the page looked empty. Both reference products keep their chrome with
  // nothing open; every command here says why it is disabled instead.
  await expect(page.locator("body")).toHaveClass(/doc-loaded/);
});

// Sharpness has no logical consequence, so it needs its own guard and it has to
// run at more than one density — a facade that ignored its `dpi` argument renders
// a correct-looking deck at dpr 1 and a blurred one at dpr 2, and every other
// assertion in this file passes either way. This is the same reasoning
// `backing-store-density.spec.mjs` records for the document viewer.
for (const dpr of [1, 2]) {
  test(`a slide rasterizes at the display's density (dpr ${dpr})`, async ({ browser }) => {
    const context = await browser.newContext({ deviceScaleFactor: dpr });
    const page = await context.newPage();
    try {
      await gotoSlides(page);
      await openDeck(page);
      const density = await page.locator("#slideCanvas").evaluate((canvas) => {
        const box = canvas.getBoundingClientRect();
        return canvas.width / box.width;
      });
      // Within a pixel's rounding of the device ratio: the backing store is the
      // CSS box times the ratio, so a coarser one means the browser is upscaling
      // a raster, which is what "pixelated" looks like to a reader.
      expect(density).toBeGreaterThan(dpr - 0.1);
      expect(density).toBeLessThan(dpr + 0.6);
    } finally {
      await context.close();
    }
  });
}

test("a screen reader can read the slide, which the canvas itself says nothing to", async ({
  page,
}) => {
  await gotoSlides(page);
  await openDeck(page);

  // The canvas is PIXELS. Asserted first, because it is the reason the rest of
  // this test exists: there is no text in it to find, so everything a reader
  // hears has to come from the mirror beside it.
  await expect(page.locator("#slideCanvas")).toHaveText("");

  // The mirror is off screen — a sighted reader must not see the deck's words
  // twice — and still in the accessibility tree, which is the whole point.
  // `visually-hidden` is the repository's own utility for exactly that, and the
  // measurement is the one that distinguishes it from `display: none`: a box of
  // one pixel rather than no box at all.
  // `stableBox` and not a raw `boundingBox()`: it polls the element it is HANDED
  // until that element has a real box, so the wait and the measurement cannot
  // drift onto two different elements. `box_read_contract.test.mjs` enforces that
  // across the whole suite after a spec that waited for a wrapper and measured
  // the sheet inside it turned `main` red and made two neighbours flaky.
  const box = await stableBox(page.locator("#slideText"));
  expect(box, "the mirror must be in the layout, not display:none").not.toBeNull();
  expect(box.width).toBeLessThan(3);

  // The slide's TITLE is a heading. This is the single most useful thing the
  // mirror says: it is how a reader skims a deck, and `aria-current` on a
  // thumbnail tells them which slide is showing but never what it is about.
  await expect(page.locator("#slideTextOwn h3")).toHaveText("One");
  // And the body text reaches it, as ONE paragraph: the fixture's subtitle holds
  // an `a:br` between "order" and "second", which is a soft break inside a
  // paragraph and not a paragraph boundary.
  await expect(page.locator("#slideTextOwn")).toContainText(
    "First in presentation order second line",
  );
  // The master's PROMPT must never be read. The fixture carries "Click to edit
  // Master title style" — a placeholder's text on a master is what PowerPoint
  // shows in the editor and never on a slide, so a reader hearing it would be
  // hearing the deck's scaffolding instead of its content.
  await expect(page.locator("#slideText")).not.toContainText("Click to edit");

  // The mirror MOVES with the slide. A reader paging through a deck whose mirror
  // was built once hears slide 1 forever, and every other assertion in this file
  // passes either way — which is why this is asserted and not left to the
  // ordering inside `paint`.
  await page.locator("#slideStage").focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#slideTextOwn h3")).toHaveText("Two");
  // The second slide states `lvl="1"` and `lvl="2"`, so its body is NESTED three
  // deep: an indented sub-point announced at the same depth as the point above it
  // is a different claim about the slide. Asserted on the DEEPEST item, whose own
  // text is its own — an outer `li` contains the list nested inside it, so its
  // `textContent` is every descendant's and a `toHaveText` there passes for a flat
  // projection too.
  const deepest = page.locator("#slideTextOwn ul ul ul > li");
  await expect(deepest).toHaveCount(1);
  // The trailing "2" is the `a:fld type="slidenum"` cache. A slide that visibly
  // says 2 must not be read as if it said nothing.
  await expect(deepest).toHaveText("Third level, unbulleted2");
  // And nothing nests deeper than the file states, which is what a stack that
  // never unwound would produce.
  await expect(page.locator("#slideTextOwn ul ul ul ul")).toHaveCount(0);

  // The third slide's TABLE becomes a real table. A `<canvas>` says nothing about
  // a grid, and a list of sentences would turn a 3x3 table into nine of them with
  // no row, no column and no merge — so this is the one shape in the deck where
  // the mirror's structure carries information the text alone cannot.
  await page.keyboard.press("End");
  const table = page.locator("#slideTextOwn table");
  await expect(table).toHaveCount(1);
  await expect(table.locator("tr")).toHaveCount(3);
  // The merges reach the reader as spans, which is how both HTML and PowerPoint's
  // own accessibility tree state them.
  await expect(table.locator('td[colspan="2"]')).toHaveText("Spans two");
  await expect(table.locator('td[rowspan="2"]')).toHaveText("Tall right");
  // And the COVERED cell is absent. The fixture deliberately gives it the text
  // "Covered" — which a real PowerPoint file would never write — so that "a
  // covered cell paints nothing" can be told apart from "a covered cell had
  // nothing to paint". It paints nothing, so it is read as nothing.
  await expect(page.locator("#slideText")).not.toContainText("Covered");
});

test("the page is actually laid out, not bare markup", async ({ page }) => {
  await gotoSlides(page);

  // The real `<input type="file">` is hidden VISUALLY while a styled control
  // stands in for it. Unstyled, the browser's own "Choose File" control shows and
  // the stand-in is plain text beside it — which is what was on screen. Before a
  // deck is open the stand-in is the empty state's button; after, it is the
  // ribbon's Open, the editor's large captioned button.
  const standIn = (selector) =>
    page.evaluate((sel) => {
      const box = (el) => {
        const r = el.getBoundingClientRect();
        return { w: r.width, h: r.height };
      };
      return { input: box(document.getElementById("slidesFile")), control: box(document.querySelector(sel)) };
    }, selector);
  const empty = await standIn("#slidesOpen");
  expect(empty.input.w, "the raw file input is not what a reader sees").toBeLessThan(3);
  expect(empty.control.w, "the empty state's Open button").toBeGreaterThan(80);
  expect(empty.control.h).toBeGreaterThan(20);

  await openDeck(page);
  const ribbon = await standIn('#slidesPanel-home [data-command="file.open"]');
  expect(ribbon.input.w).toBeLessThan(3);
  expect(ribbon.control.w, "the ribbon's Open button").toBeGreaterThan(30);
  // A strip icon is one `--h-control` (30px) square; the editor's large
  // captioned button stacks a 22px icon over its caption and measures ~42px.
  expect(ribbon.control.h, "a large captioned button, not a strip icon").toBeGreaterThan(36);

  // THIS IS THE GUARD THE PAGE SHIPPED WITHOUT. Every other spec in this file
  // passes on a completely unstyled page: they query elements by id and read
  // pixels out of a canvas, and a canvas with a backing store paints correctly
  // whether or not anything around it has a layout. The page did ship that way —
  // no stylesheet existed for any of its classes — and the first person to find
  // out was the person who opened it. `slides_style.test.mjs` asserts the rules
  // exist; this asserts they take EFFECT.
  const layout = await page.evaluate(() => {
    const box = (sel) => {
      const el = typeof sel === "string" ? document.querySelector(sel) : sel;
      const r = el.getBoundingClientRect();
      return { x: r.x, y: r.y, w: r.width, h: r.height };
    };
    return {
      bar: box("header.bar"),
      rail: box("nav.rail"),
      sorter: box("#slidesSorterPanel"),
      stage: box("#slideStage"),
      footer: box("footer.footer"),
      sheet: box("#slideSheet"),
      thumb: box(".page-thumb"),
    };
  });

  // Every band of the editor's shell is present and has real height. An unstyled
  // page has a 0-height footer and no rail at all — which is exactly what
  // `body:not(.doc-loaded)` produces, so this also pins that the deck viewer
  // adopts the shell's own state contract rather than ignoring it.
  expect(layout.bar.h, "the header band").toBeGreaterThan(30);
  expect(layout.rail.h, "the rail").toBeGreaterThan(40);
  expect(layout.footer.h, "the status bar").toBeGreaterThan(10);

  // Rail, then sorter, then stage — left to right, in that order.
  expect(layout.sorter.x).toBeGreaterThan(layout.rail.x + layout.rail.w - 1);
  expect(layout.stage.x).toBeGreaterThan(layout.sorter.x + layout.sorter.w - 1);
  expect(layout.thumb.h, "a thumbnail is a card, not a line of text").toBeGreaterThan(60);

  // The slide sits on the editor's SHEET, inside the viewport, with desk either
  // side — not edge to edge, which is what dropping `DESK_MARGIN_PX` would give.
  expect(layout.sheet.w).toBeLessThan(layout.stage.w);
  expect(layout.sheet.w).toBeGreaterThan(layout.stage.w - 120);
});

// ---- The editor's shell, on the deck viewer ----------------------------------
//
// The owner's rule for this page: a reader moving between a document and a deck
// does not meet a different application. Each test below names one piece of the
// editor's shell and asserts the deck viewer behaves as the editor does.

test("the deck viewer has the editor's two toolbars behind the editor's switch, on the editor's preference", async ({ page }) => {
  await gotoSlides(page);
  // Ribbon by default, as in the editor: the tab strip and the band, not the
  // compact bar.
  await expect(page.locator("body")).toHaveClass(/ribbon-mode/);
  await expect(page.locator("#slidesRibbonTabs [role=tab]")).toHaveText(["File", "Home", "View"]);
  await expect(page.locator("#slidesRibbonBody")).toBeVisible();
  await expect(page.locator("#compactToolbar")).toBeHidden();

  // The switch.
  await page.locator("#modeCompact").click();
  await expect(page.locator("body")).toHaveClass(/compact-mode/);
  await expect(page.locator("#compactToolbar")).toBeVisible();
  await expect(page.locator("#slidesRibbonBody")).toBeHidden();
  // On the EDITOR's key, so the choice holds on both pages.
  expect(await page.evaluate(() => localStorage.getItem("opendoc.chromeMode"))).toContain("compact");

  // And it survives a reload.
  await gotoSlides(page);
  await expect(page.locator("body")).toHaveClass(/compact-mode/);
  await page.locator("#modeRibbon").click();
  await expect(page.locator("body")).toHaveClass(/ribbon-mode/);
});

test("a ribbon tab shows its own band and the arrows move between tabs", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);
  const view = page.locator('#slidesRibbonTabs [data-tab="view"]');
  await view.click();
  await expect(view).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#slidesPanel-view")).toBeVisible();
  await expect(page.locator("#slidesPanel-home")).toBeHidden();
  // The band's panel toggles reflect the panels' state, from the registry.
  await expect(page.locator('#slidesPanel-view [data-command="view.slides"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  // Roving: ArrowLeft from View lands on Home and shows its band.
  await view.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator('#slidesRibbonTabs [data-tab="home"]')).toBeFocused();
  await expect(page.locator("#slidesPanel-home")).toBeVisible();
  // And a ribbon button runs its command: Next slide moves the position.
  await page.locator('#slidesPanel-home [data-command="slide.next"]').click();
  await expect(page.locator("#slidesPosition")).toHaveText(/2.*3/);
});

test("a deck opens with the whole slide on screen, as the editor opens a page", async ({ page }) => {
  // A wide, short window: fitting the WIDTH alone would put the bottom of a 16:9
  // slide below the fold, which is how this page used to open every deck.
  await page.setViewportSize({ width: 1600, height: 760 });
  await gotoSlides(page);
  await openDeck(page);
  const fits = await page.evaluate(() => {
    const stage = document.getElementById("slideStage").getBoundingClientRect();
    const slide = document.getElementById("slideCanvas").getBoundingClientRect();
    return { slideBottom: slide.bottom, stageBottom: stage.bottom, slideTop: slide.top, stageTop: stage.top };
  });
  expect(fits.slideTop).toBeGreaterThanOrEqual(fits.stageTop);
  expect(fits.slideBottom, "the whole slide is visible without scrolling").toBeLessThanOrEqual(
    fits.stageBottom,
  );
});

test("the status bar's zoom control is the editor's: steppers, a typed value and presets", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);
  const input = page.locator("#zoom");
  await expect(input).toHaveValue("100%");
  const width = () => page.locator("#slideCanvas").evaluate((c) => c.getBoundingClientRect().width);
  const fitted = await width();

  await page.locator("#zoomIn").click();
  await expect(input).toHaveValue("125%");
  expect(await width()).toBeGreaterThan(fitted);

  await input.fill("50");
  await input.press("Enter");
  await expect(input).toHaveValue("50%");
  expect(await width()).toBeLessThan(fitted);

  // The presets, on the editor's ladder plus the two fits.
  await page.locator("#zoomMenuBtn").click();
  await expect(page.locator("#zoomMenu")).toBeVisible();
  await page.locator("#zoomMenu .zoom-preset", { hasText: "Fit slide" }).click();
  await expect(input).toHaveValue("100%");
  await expect(page.locator("#zoomMenu")).toBeHidden();
});

test("the findings chip is the editor's, and opens the editor's findings dialog", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);
  const chip = page.locator("#slidesFidelity");
  await expect(chip).toBeVisible();
  await expect(chip).toHaveText(/\d+ import finding/);
  await expect(page.locator("#documentState")).toBeVisible();
  await chip.click();
  const dialog = page.locator("#compatibilityFindingsDialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.locator("h2")).toHaveText("Compatibility findings");
  // Grouped as a document's findings are; the fixture deck loses things, so the
  // dialog has at least one group with entries in it.
  await expect(dialog.locator("section").first()).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("the language control is the editor's, on the editor's setting", async ({ page }) => {
  await gotoSlides(page);
  await page.locator("#languageStatus").click();
  const menu = page.locator("#languageMenu");
  await expect(menu).toBeVisible();
  // Every language names itself, as in the editor's footer menu.
  await expect(menu.locator('[role="menuitemradio"], [role="menuitem"]').first()).toBeVisible();
  const deutsch = menu.getByText("Deutsch", { exact: false }).first();
  await deutsch.click();
  await expect(page.locator('#slidesRibbonTabs [data-tab="home"]')).toHaveText("Start");
  // Saved where the editor saves it.
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("opendoc.settings") ?? "{}"));
  expect(saved.language).toBe("de");
});
