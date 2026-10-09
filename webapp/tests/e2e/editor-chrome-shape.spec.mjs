// The editor's chrome, asserted as the thing a person sees rather than as the
// rules that produce it.
//
// Every assertion here stands for a defect found by LOOKING at the editor at
// 1280x900 and measuring the screenshot:
//
//   * the header's right-hand controls were vertically centred over the WHOLE
//     58px bar (`.controls` at y 14..44) while the chrome under them was two
//     bands (title y 3..27, navigation y 27..55) — so a state chip, a
//     toolbar-mode toggle, an import-finding count and two icon buttons all
//     landed on a line belonging to neither band;
//   * the Home band drew no group captions at all, so 24 groups of icons had no
//     stated structure — neither Word's labelled groups nor Docs' single row;
//   * four 28px controls were jammed into a 2x2 grid at the band's left edge,
//     two of which belong to Clipboard and Font in Word;
//   * the rail's captions were 9px and "Comments" ellipsised to "Comme…";
//   * the margin "add a comment" button sat beside the page dimmed, round-less
//     and with a `title` that never said what was missing.
import { test, expect, gotoEditor, gotoSampleDocument, clickIntoFirstPage } from "./fixtures.mjs";

test("the header is two bands: the document over the application", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);

  const rows = await page.evaluate(() => {
    const mid = (selector) => {
      const el = document.querySelector(selector);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { top: r.top, bottom: r.bottom, centre: r.top + r.height / 2 };
    };
    return {
      title: mid(".document-title-row"),
      nav: mid(".ribbon-nav"),
      state: mid("#documentState"),
      findings: mid("#compatibilityStatus"),
      mode: mid(".chrome-mode"),
      settings: mid("#settingsBtn"),
      bar: mid(".bar"),
    };
  });

  // Band 1 carries the document's identity AND its state. A chip whose centre
  // is below the title row's bottom edge is on the wrong band — which is
  // precisely how the old single flex row placed all five controls.
  for (const [name, box] of [
    ["documentState", rows.state],
    ["compatibilityStatus", rows.findings],
  ]) {
    expect(box, `${name} is missing`).not.toBeNull();
    expect(
      box.centre,
      `${name} sits below the title band (centre ${box.centre}, band ends ${rows.title.bottom})`,
    ).toBeLessThanOrEqual(rows.title.bottom);
  }

  // Band 2 carries navigation AND the controls that change how the application
  // is presented. Same test, the other way round.
  for (const [name, box] of [
    ["chrome-mode", rows.mode],
    ["settingsBtn", rows.settings],
  ]) {
    expect(
      box.centre,
      `${name} sits above the navigation band (centre ${box.centre}, band starts ${rows.nav.top})`,
    ).toBeGreaterThanOrEqual(rows.nav.top);
  }

  // And the two bands are genuinely separate, not one row drawn twice.
  expect(rows.title.bottom).toBeLessThanOrEqual(rows.nav.top + 1);
  expect(consoleErrors).toEqual([]);
});

test("the import-finding count reads as status, not as an alert", async ({ page }) => {
  // `sample.docx`: the rich fixture has no findings left to count (FID-AT-08/09/10).
  await gotoSampleDocument(page);
  const chip = page.locator("#compatibilityStatus");
  await expect(chip).toBeVisible();
  // It was `color-mix(--accent-text 82%, --ink)` body text with no container,
  // which made it the loudest thing in the chrome. It is a bordered neutral
  // chip now, the same object as the state chip beside it.
  const shape = await chip.evaluate((el) => {
    const s = getComputedStyle(el);
    const state = getComputedStyle(document.getElementById("documentState"));
    return {
      colour: s.color,
      muted: getComputedStyle(document.documentElement).getPropertyValue("--muted").trim(),
      borderWidth: parseFloat(s.borderTopWidth),
      sameBorderAsState: s.borderTopWidth === state.borderTopWidth,
    };
  });
  expect(shape.borderWidth, "the count has no container of its own").toBeGreaterThan(0);
  expect(shape.sameBorderAsState).toBe(true);
});

test("the Home band is Word's: captioned groups, and Word's grouping", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // Anything measured below must be ON the band, not in the `⋯` menu, where a
  // relocated group measures perfectly healthy (SKILL §11).
  await expect(page.locator("#ribbonOverflowBtn")).toBeHidden();

  const band = page.locator('.ribbon-panel[data-panel="home"]');
  const captions = band.locator(".rgroup > .rgroup-label");
  await expect(captions).toHaveText(
    ["Undo", "Clipboard", "Font", "Paragraph", "Styles", "Editing"],
  );
  // Drawn, not merely present: the old rule clipped them to a 1px box.
  const drawn = await captions.evaluateAll((els) =>
    els.map((el) => {
      const r = el.getBoundingClientRect();
      return { text: el.textContent, w: Math.round(r.width), h: Math.round(r.height) };
    }),
  );
  for (const caption of drawn) {
    expect(caption.h, `the ${caption.text} caption is ${caption.h}px tall`).toBeGreaterThan(6);
    expect(caption.w, `the ${caption.text} caption is ${caption.w}px wide`).toBeGreaterThan(14);
  }
  // Each caption sits UNDER its own group's controls, which is what makes it a
  // caption rather than a floating word.
  const placed = await band.evaluate((panel) =>
    [...panel.querySelectorAll(".rgroup")].map((group) => {
      const label = group.querySelector(".rgroup-label").getBoundingClientRect();
      const controls = group.querySelector(".rgroup-ctl").getBoundingClientRect();
      const box = group.getBoundingClientRect();
      return {
        group: group.dataset.group,
        below: label.top >= controls.bottom - 1,
        inside: label.left >= box.left - 1 && label.right <= box.right + 1,
      };
    }),
  );
  for (const group of placed) {
    expect(group.below, `${group.group}'s caption is not under its controls`).toBe(true);
    expect(group.inside, `${group.group}'s caption overhangs its group`).toBe(true);
  }

  // Word's grouping, not a jam of four micro icons at the left edge: Format
  // Painter is a Clipboard command and Clear Formatting a Font command.
  const home = (id) =>
    page.locator(`#${id}`).evaluate((el) => el.closest(".rgroup").dataset.group);
  expect(await home("formatPainter")).toBe("clipboard");
  expect(await home("clearFormatting")).toBe("font");
  expect(await home("undoBtn")).toBe("undo");
  // …which leaves the Undo group a single two-button stack.
  expect(
    await page.locator('[data-group="undo"] .fmt').count(),
    "the Undo group is a 2x2 grid of micro icons again",
  ).toBe(2);
  expect(consoleErrors).toEqual([]);
});

test("the rail's destinations are readable, not ellipsised", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  const cut = await page.evaluate(() =>
    [...document.querySelectorAll(".rail-btn > span:not(.ms)")]
      .filter((el) => el.scrollWidth > el.clientWidth + 1)
      .map((el) => el.textContent),
  );
  expect(cut, "a rail caption is cut off").toEqual([]);
  const size = await page
    .locator("#railReview > span:not(.ms)")
    .evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
  expect(size, "the rail caption is back at micro size").toBeGreaterThanOrEqual(10);
});

test("the margin comment button is round and says what it needs", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const button = page.locator("#reviewMarginComment");
  await expect(button).toBeVisible();
  const shape = await button.evaluate((el) => ({
    radius: parseFloat(getComputedStyle(el).borderTopLeftRadius),
    width: el.getBoundingClientRect().width,
    disabled: el.disabled,
    title: el.title,
  }));
  // Docs draws this button round; it was a `--radius` square, which is what made
  // it read as a bare "+" floating over the margin.
  expect(shape.radius).toBeGreaterThanOrEqual(shape.width / 2 - 1);
  // Never a dead control: disabled, it has to SAY what is missing.
  expect(shape.disabled, "nothing is selected, so it should be disabled").toBe(true);
  expect(shape.title, "a disabled control with no reason").toMatch(/select/i);
});
