// A sparse header and status bar (`docs/184` D6, `109` UX-054), and the do-now
// chrome rows that ride with it (UI redesign lane L2).
//
// D6 was approved on 2026-11-01 and named this spec as its guard, but neither was
// built (audit SYSTEM-V-01), so every D6 item was still on screen: Share drawn
// disabled with no share host, the editing mode in the header chip AND the status
// bar, and the Table, Chart and Header & Footer tabs shown greyed with nothing
// selected. Each test asserts the GUARANTEE a reader sees, not the mechanism:
// what is in the accessibility tree, what is painted, what `elementFromPoint`
// hits, what the tab says.
import {
  test,
  expect,
  gotoEditor,
  gotoSampleDocument,
  clickIntoFirstPage,
  moveCaretToDocStart,
  runPaletteCommand,
  setReviewMode,
} from "./fixtures.mjs";

/** Puts a 2x2 table at the caret, which leaves the caret in its first cell. */
async function insertTable(page) {
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
}

/** The ids of the ribbon tabs a reader can see. */
function visibleTabs(page) {
  return page.evaluate(() =>
    [...document.querySelectorAll(".ribbon-tab")].filter((tab) => tab.getClientRects().length > 0).map((tab) => tab.id),
  );
}

test("Share is not offered at all when no host configured sharing, and is when one did", async ({
  page,
  browser,
  consoleErrors,
}) => {
  await gotoEditor(page);
  // Not disabled — absent: not painted, not in the accessibility tree, not a Tab
  // stop. A disabled primary button was the loudest control in the header and
  // could not even be focused to hear why it did nothing.
  await expect(page.getByRole("button", { name: "Share" })).toHaveCount(0);
  await expect(page.locator("#shareBtn")).toBeHidden();
  expect(consoleErrors).toEqual([]);

  // The positive control: with a host that can mint a share, Share is there and
  // live, so the absence above is the configuration speaking, not a broken header.
  const hosted = await browser.newPage();
  await hosted.addInitScript(() => {
    window.OPENDOC_COLLABORATION = { createShare: async () => ({}) };
  });
  await gotoEditor(hosted);
  await expect(hosted.getByRole("button", { name: "Share" })).toBeVisible();
  await expect(hosted.locator("#shareBtn")).toBeEnabled();
  await hosted.close();
});

test("contextual tabs appear only while their object is selected, and say they are contextual", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  const contextual = ["tabTable", "tabChart", "tabHeaderFooter"];
  // Nothing selected: none of them is on screen, greyed or otherwise.
  for (const id of contextual) expect(await visibleTabs(page)).not.toContain(id);
  await expect(page.getByRole("tab", { name: "Table" })).toHaveCount(0);

  await insertTable(page);
  await expect(page.locator("#tabTable")).toBeVisible();
  // Drawn as contextual: the accent, not the muted colour of the command tabs.
  const colours = await page.evaluate(() => ({
    table: getComputedStyle(document.getElementById("tabTable")).color,
    home: getComputedStyle(document.getElementById("tabHome")).color,
  }));
  expect(colours.table).not.toBe(colours.home);

  // Leaving the table takes the tab away again.
  await moveCaretToDocStart(page);
  await expect(page.locator("#tabTable")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("the editing mode has exactly one switch on screen, the header's, and the status bar has none", async ({
  page,
}) => {
  await gotoEditor(page);
  // Every element that publishes the mode is a row of the header's menu.
  expect(await page.locator("[data-review-mode]").count()).toBe(3);
  expect(await page.locator("#headerModeMenu [data-review-mode]").count()).toBe(3);
  // And nothing in the status bar names a mode, as a control or as a label.
  const footerModes = await page.evaluate(() =>
    [...document.querySelectorAll(".footer *")]
      .filter((el) => el.getClientRects().length > 0 && el.children.length === 0)
      .map((el) => el.textContent.trim())
      .filter((text) => /^(Editing|Suggesting|Read only)$/.test(text)),
  );
  expect(footerModes).toEqual([]);
  await expect(page.locator(".footer").getByRole("button", { name: /^(Editing|Suggesting|Read only)$/ })).toHaveCount(0);
  // The one switch works, so the absence above is not a mode that cannot change.
  await setReviewMode(page, "suggesting");
  await expect(page.locator("#headerModeText")).toHaveText("Suggesting");
});

test("the ruler is an opaque band flush with the chrome: scrolled text never shows above it", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoSampleDocument(page);
  const ruler = page.locator(".ruler");
  await expect(ruler).toBeVisible();
  await page.locator("#viewport").evaluate((viewport) => {
    viewport.scrollTop = 600;
  });
  await page.waitForFunction(() => document.getElementById("viewport").scrollTop >= 600);
  const probe = await page.evaluate(() => {
    const viewport = document.getElementById("viewport").getBoundingClientRect();
    const band = document.querySelector(".ruler").getBoundingClientRect();
    const hits = [];
    // Every pixel row from the top of the scroller to the ruler's bottom edge,
    // across the ruler's width: nothing there may be the page.
    for (let y = Math.ceil(viewport.top); y < band.bottom; y += 1) {
      for (const fraction of [0.02, 0.25, 0.5, 0.75, 0.98]) {
        const x = band.left + band.width * fraction;
        const el = document.elementFromPoint(x, y);
        if (el?.closest(".page-wrap, .page")) hits.push([Math.round(x), y]);
      }
    }
    return { gap: band.top - viewport.top, hits };
  });
  expect(probe.hits, "the scrolled page showed through above or under the ruler").toEqual([]);
  expect(probe.gap).toBeLessThanOrEqual(1);
});

test("big ribbon buttons draw the large icon", async ({ page }) => {
  await gotoEditor(page);
  const sizes = await page.evaluate(() => {
    const probe = document.createElement("span");
    probe.style.fontSize = "var(--icon-lg)";
    document.body.append(probe);
    const large = getComputedStyle(probe).fontSize;
    probe.remove();
    return {
      large,
      big: getComputedStyle(document.querySelector("#panelHome .fmt.fmt-big .ms")).fontSize,
      small: getComputedStyle(document.querySelector("#panelHome .fmt:not(.fmt-big) .ms")).fontSize,
    };
  });
  expect(sizes.big).toBe(sizes.large);
  expect(parseFloat(sizes.big)).toBeGreaterThan(parseFloat(sizes.small));
});

test("a ribbon button pressed under the mouse is darker than one merely hovered", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  // Reduced motion, so each read is the state's settled colour rather than a frame
  // of the 0.11s background transition between them.
  await page.emulateMedia({ reducedMotion: "reduce" });
  await gotoEditor(page);
  const italic = page.locator('#panelHome [aria-label="Italic"]').first();
  const settled = () =>
    italic.evaluate((el) => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(getComputedStyle(el).backgroundColor)))));
  const rest = await settled();
  await italic.hover();
  const hover = await settled();
  await page.mouse.down();
  const pressed = await settled();
  await page.mouse.up();
  expect(hover).not.toBe(rest);
  expect(pressed).not.toBe(hover);
});

// The label that wrapped is the one with spaces: "Header & Footer" at 1024px and
// "Kopf- und Fußzeile" at 1280px (audit MOBILE-17, CRITIC-06), on the tab that
// shows while a band is open. With the other contextual tabs hidden the strip has
// more room, so the condition is created where it still bites: without `nowrap`
// both labels break below about 960px.
for (const [lang, width] of [["en", 1024], ["en", 900], ["de", 900]]) {
  test(`ribbon tab labels stay on one line (${lang}, ${width}px, a footer open)`, async ({ page }) => {
    await page.setViewportSize({ width, height: 768 });
    await gotoEditor(page, `&lang=${lang}`);
    await expect(page.locator("html")).toHaveAttribute("lang", lang);
    await clickIntoFirstPage(page);
    await runPaletteCommand(page, "insert.footer", "footer");
    await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", "footer");
    await expect(page.locator("#tabHeaderFooter")).toBeVisible();
    const lines = await page.evaluate(() =>
      [...document.querySelectorAll(".ribbon-tab")]
        .filter((tab) => tab.getClientRects().length > 0)
        .map((tab) => {
          const range = document.createRange();
          range.selectNodeContents(tab);
          // A wrapped label has its lines at different heights.
          return [tab.textContent.trim(), new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size];
        })
        .filter(([, count]) => count > 1),
    );
    expect(lines).toEqual([]);
  });
}

test("the status bar never echoes the toolbar layout or a font download, at load or on a toggle", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.__statusSeen = [];
    window.__spoken = [];
    // From the first byte, so nothing written during boot is missed.
    new MutationObserver((records) => {
      for (const record of records) {
        const node = record.target.nodeType === Node.ELEMENT_NODE ? record.target : record.target.parentElement;
        const owner = node?.closest?.("#status, #statusLiveRegion");
        if (!owner) continue;
        (owner.id === "status" ? window.__statusSeen : window.__spoken).push(owner.textContent);
      }
    }).observe(document, { childList: true, characterData: true, subtree: true });
  });
  await gotoSampleDocument(page);
  const spokenAtLoad = await page.evaluate(() => window.__spoken);
  await page.locator('[data-chrome-mode="compact"]').click();
  await expect(page.locator("body")).toHaveClass(/compact-mode/);
  await page.locator('[data-chrome-mode="ribbon"]').click();
  await expect(page.locator("body")).toHaveClass(/ribbon-mode/);
  const seen = await page.evaluate(() => window.__statusSeen);
  expect(seen.filter((text) => /toolbar|Fetching|fonts for/i.test(text))).toEqual([]);
  // A layout nobody chose is not even spoken: only the reader's own toggle is.
  expect(spokenAtLoad.filter((text) => /toolbar/i.test(text))).toEqual([]);
});

test("the browser tab names the open document, even when the catalogue lands after the open", async ({
  page,
}) => {
  // The relabel writes `<title>`'s no-document fallback; hold the catalogue back
  // until the document is open so it lands last, as it does on a slow network.
  await page.route("**/locales/*.json", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 2500));
    await route.continue();
  });
  await page.goto("/editor.html");
  await page.waitForFunction(() => document.body.classList.contains("doc-loaded"));
  await page.waitForResponse("**/locales/*.json");
  await page.waitForTimeout(300);
  await expect(page).toHaveTitle(/^sample\.docx\b/);
});

test("a file dragged anywhere in the window is caught, and dropped outside the page it opens", async ({
  page,
}) => {
  await gotoEditor(page);
  const fileTransfer = () => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(["DROPPEDONFOOTER"], "dropped.txt", { type: "text/plain" }));
    return transfer;
  };
  const caught = await page.evaluate((make) => {
    const build = new Function(`return (${make})`)();
    return [".ribbon-tabs", "#docTitle", ".footer", "body"].map((selector) => {
      const over = new DragEvent("dragover", { dataTransfer: build(), bubbles: true, cancelable: true });
      document.querySelector(selector).dispatchEvent(over);
      return [selector, over.defaultPrevented];
    });
  }, fileTransfer.toString());
  // Cancelling `dragover` is what tells the browser the page takes the drop; a
  // target that does not would let the file replace the editor tab.
  for (const [selector, over] of caught) expect(over, `dragover on ${selector}`).toBe(true);
  await expect(page.locator("body")).toHaveClass(/file-dragging/);

  const dropped = await page.evaluate((make) => {
    const build = new Function(`return (${make})`)();
    const drop = new DragEvent("drop", { dataTransfer: build(), bubbles: true, cancelable: true });
    document.querySelector(".footer").dispatchEvent(drop);
    return drop.defaultPrevented;
  }, fileTransfer.toString());
  expect(dropped).toBe(true);
  await expect(page.locator("body")).not.toHaveClass(/file-dragging/);
  await expect(page.locator("#a11yDocument")).toContainText("DROPPEDONFOOTER");
});
