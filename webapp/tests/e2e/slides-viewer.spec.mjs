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

/** The generated fixture, staged by `build.sh` beside the page. */
const DECK = "demo.pptx";

/** Opens the viewer and waits for the engine to boot. */
async function gotoSlides(page) {
  await page.goto("/slides.html");
  // The engine is megabytes of WebAssembly; the open control is inert until it
  // has booted. Waiting on the control rather than on a timer is the difference
  // between a spec that is slow and a spec that is flaky.
  await expect(page.locator("#slidesFile")).toBeAttached({ timeout: 45_000 });
  await page.waitForFunction(() => document.getElementById("slidesSave") !== null, null, {
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

  const labels = await page
    .locator("#slidesSorter .slides-thumb-label")
    .allTextContents();
  // The fixture's parts are slide1 ("Opening"), slide2 ("Detail") and slide10
  // ("Appendix"), presented 1, 2, 10. EVERY lexical ordering of those part names
  // gives 1, 10, 2 — "Opening", "Appendix", "Detail" — so a page built on part
  // order shows a different, entirely plausible deck. That is why this asserts
  // the sequence and not the count.
  expect(labels).toEqual(["Opening", "Detail", "Appendix"]);

  // A hidden slide is still IN the deck — retained, saved and sortable — so it is
  // marked rather than omitted. Omitting it would leave an author unable to see
  // what their own file contains.
  await expect(page.locator("#slidesSorter .slides-thumb-hidden")).toHaveCount(1);
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
  await expect(page.locator('#slidesSorter .slides-thumb[aria-current="true"]')).toHaveCount(1);
});

test("clicking a thumbnail shows that slide", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);

  await page.locator("#slidesSorter .slides-thumb").nth(2).click();
  await expect(page.locator("#slidesPosition")).toHaveText(/3.*3/);
  await expect(
    page.locator('#slidesSorter .slides-thumb').nth(2),
  ).toHaveAttribute("aria-current", "true");
});

test("the fidelity report says what was not recovered", async ({ page }) => {
  await gotoSlides(page);
  await openDeck(page);

  // Surfaced rather than hidden. This is the one claim this engine can make that
  // a converter cannot, and a page that opened silently would throw it away.
  await expect(page.locator("#slidesFidelity")).toBeVisible();
  const summary = await page.locator("#slidesFidelitySummary").textContent();
  expect(summary?.trim().length ?? 0).toBeGreaterThan(0);

  // The fixture deliberately carries constructs this build does not cover, so the
  // report must be non-empty — an empty one would be the overstatement `SKILL` §9
  // forbids rather than good news.
  await page.locator("#slidesFidelityDetails summary").click();
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
  // wrong, and the page shows that refusal. A viewer that swallowed it would turn
  // a loud refusal into a blank page, which is the worse failure.
  await expect(page.locator("#slidesError")).toBeVisible({ timeout: 30_000 });
  await expect(page.locator("#slidesError")).not.toBeEmpty();
  await expect(page.locator("#slidesSave")).toBeDisabled();
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
  const box = await page.locator("#slideText").boundingBox();
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
});
