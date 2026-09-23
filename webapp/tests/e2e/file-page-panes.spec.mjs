import {
  test,
  expect,
  definedParagraphStyles,
  gotoEditor,
  openFilePage,
} from "./fixtures.mjs";

// docs/123 §6 — the File page's right side is one surface, not four dialogs
// parked in the same place.
//
// The owner's instruction was "basically in this view replace dialogs with this
// space", and it is also what ONLYOFFICE does: their File page has no dialogs at
// all — Advanced Settings, Info and Help are panes (`FileMenu.js:412-415`).
// Three defects were reported against the first attempt and each has a guard
// here:
//
//   1. Settings came up EMPTY after visiting another pane. `replaceChildren`
//      does not move a borrowed node out, it DROPS it, so switching pane to pane
//      deleted `#settingsPanel` from the document permanently.
//   2. Keyboard shortcuts came up empty, because every one of these panels fills
//      itself when its DIALOG opens and nothing had asked it to.
//   3. New showed nothing to pick. Templates now preview as a miniature of their
//      own first page, the way Google Docs' gallery does.

/** Clicks a rail category and returns the pane host. */
async function openPane(page, pane) {
  const row = page.locator(`#filePageBody [data-file-pane="${pane}"]`);
  await expect(row, `the rail has a ${pane} row`).toBeVisible();
  await row.click();
  await expect(row).toHaveAttribute("aria-pressed", "true");
  return page.locator("#filePageDetail");
}

/** The panes whose content is a flat block of text, not a grid of cards or a
 *  control: their own left edge is the page's. */
const TEXT_PANES = new Set(["properties", "settings", "shortcuts", "about"]);

const PANES = [
  ["export", "#filePageDetail .file-format-grid"],
  ["new", "#filePageDetail .file-template-grid"],
  ["properties", "#filePageDetail #propertiesPanel"],
  ["settings", "#filePageDetail #settingsPanel"],
  ["shortcuts", "#filePageDetail #shortcutsDialog"],
  ["about", "#filePageDetail #aboutDialog"],
  ["commands", "#filePageDetail #cmdPalette"],
];

test("every rail category renders its own pane, with a heading above it", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openFilePage(page);

  for (const [pane, content] of PANES) {
    await openPane(page, pane);
    await expect(page.locator(content), `${pane} renders its content`).toBeVisible();
    // One title and one measure beneath it on every pane, so the right side
    // reads as one surface.
    // ONE heading, the pane's own: a borrowed panel's dialog header is hidden,
    // because two titles saying the same thing over a close button that closes
    // nothing is what made this read as a dialog parked in a page.
    await expect(
      page.locator("#filePageDetail h2:visible"),
      `${pane} shows exactly one heading`,
    ).toHaveCount(1);
    await expect(page.locator("#filePageDetail .dialog-close:visible")).toHaveCount(0);
  }

  expect(consoleErrors).toEqual([]);
});

test("every pane starts at the same left edge — one measure, not four", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openFilePage(page);

  // A dialog card shrink-wraps and centres itself in its overlay, which is
  // right for a dialog and wrong here: About's facts sat in the middle of the
  // page while Export started at the heading. "Not consistent, no design
  // system" was the report, and this is the measurable part of it.
  for (const [pane] of PANES) {
    await openPane(page, pane);
    const edges = await page.evaluate(() => {
      const heading = document.querySelector("#filePageDetail h2");
      // The pane's first real content BOX — a borrowed dialog's card, the
      // palette's box, the first settings section, the first tile of a grid.
      // Not the wrapper around it: a full-width wrapper holding a
      // shrink-wrapped, centred card starts at the right place and still looks
      // adrift, which is exactly how About read.
      const box = document.querySelector(
        "#filePageDetail .dialog-card, #filePageDetail .cmd-box," +
          " #filePageDetail .settings-section, #filePageDetail .file-format-tile," +
          " #filePageDetail .file-template-tile",
      );
      // And the text inside that box, which a dialog's own body padding used
      // to indent by a further 22px — the pane's heading said one left edge,
      // the content another.
      const leaves = box
        ? [...box.querySelectorAll("*")].filter(
            (el) =>
              !el.querySelector("*") &&
              el.textContent.trim() &&
              el.getBoundingClientRect().width > 0,
          )
        : [];
      return {
        heading: Math.round(heading.getBoundingClientRect().left),
        content: box ? Math.round(box.getBoundingClientRect().left) : null,
        text: leaves.length
          ? Math.min(...leaves.map((el) => Math.round(el.getBoundingClientRect().left)))
          : null,
      };
    });
    expect(edges.content, `${pane} shows no content box`).not.toBeNull();
    expect(edges.content, `${pane} does not share the page's left edge`).toBe(edges.heading);
    // A tile is a card of its own and the palette's query field is a control;
    // both pay their own inset. A borrowed dialog BODY is the page itself, so
    // its text starts where the heading does.
    if (TEXT_PANES.has(pane) && edges.text !== null) {
      expect(edges.text, `${pane} indents its content past the heading`).toBe(edges.heading);
    }
  }

  expect(consoleErrors).toEqual([]);
});

test("a borrowed panel survives being shown, left, and shown again", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openFilePage(page);

  // The reported sequence: Settings, away to another borrowed panel, back.
  // Before the unconditional release, the second visit found nothing because
  // the first panel had been dropped out of the document entirely.
  for (const pane of ["settings", "properties", "settings", "shortcuts", "settings"]) {
    await openPane(page, pane);
  }
  const settings = page.locator("#filePageDetail #settingsPanel");
  await expect(settings).toBeVisible();
  // Exactly one settings form in the document — it is moved, never copied.
  expect(await page.locator("#settingsPanel").count()).toBe(1);

  // And it fits: the dialog was taller than the window, which is what put it in
  // the page in the first place.
  const fits = await page.evaluate(() => {
    const el = document.querySelector("#filePageDetail #settingsPanel");
    return el.getBoundingClientRect().bottom <= innerHeight + 1;
  });
  expect(fits, "the settings pane may not run off the bottom of the window").toBe(true);

  expect(consoleErrors).toEqual([]);
});

test("the panes that fill on dialog-open are filled as panes too", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openFilePage(page);

  await openPane(page, "shortcuts");
  const shortcutRows = await page.locator("#shortcutsDialog .shortcuts-row").count();
  expect(shortcutRows, "the shortcuts pane came up empty").toBeGreaterThan(10);

  await openPane(page, "about");
  const aboutText = await page.locator("#filePageDetail #aboutDialog").innerText();
  expect(aboutText, "the About pane shows no version").toMatch(/\d+\.\d+/);

  await openPane(page, "commands");
  const commandRows = await page.locator("#cmdList .cmd-item").count();
  expect(commandRows, "the command search pane came up with no rows").toBeGreaterThan(20);
  // You came to this pane to type a command name, so the field has the
  // keyboard. It is focused AFTER the panel is moved into the pane — moving a
  // node blurs whatever inside it held focus.
  await expect(page.locator("#cmdInput")).toBeFocused();

  expect(consoleErrors).toEqual([]);
});

test("the gear goes to the pane while the File page is open — there is one Settings", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);

  // Outside the page the gear opens the dialog.
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#settingsPanel")).toBeVisible();
  await expect(page.locator("#settingsPanel")).toHaveClass(/dialog-overlay/);
  await expect(page.locator("#settingsPanel")).not.toHaveClass(/panel-in-page/);
  await page.keyboard.press("Escape");
  await expect(page.locator("#settingsPanel")).toBeHidden();

  // Inside it, the same element is already parented in the pane. Opening the
  // dialog would have raised a half-dialog out of the page, so the gear
  // selects the pane instead.
  await openFilePage(page);
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#filePageDetail #settingsPanel")).toBeVisible();
  await expect(page.locator("#settingsPanel")).toHaveClass(/panel-in-page/);
  await expect(
    page.locator('#filePageBody [data-file-pane="settings"]'),
  ).toHaveAttribute("aria-pressed", "true");
  // Still exactly one settings form, and no scrim over the page.
  expect(await page.locator("#settingsPanel").count()).toBe(1);
  await expect(page.locator("body")).not.toHaveClass(/modal-open/);

  expect(consoleErrors).toEqual([]);
});

test("each template previews as a miniature of its own first page", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openFilePage(page);
  await openPane(page, "new");

  const tiles = page.locator(".file-template-tile");
  expect(await tiles.count(), "every template needs a tile").toBeGreaterThan(3);

  const previews = await page.evaluate(() => {
    return [...document.querySelectorAll(".file-template-tile")].map((tile) => {
      const frame = tile.querySelector(".file-template-page");
      const box = frame?.getBoundingClientRect();
      const lines = [...tile.querySelectorAll(".file-template-line")];
      const sizeOf = (el) => parseFloat(getComputedStyle(el).fontSize);
      const scaled = (el) => el.getBoundingClientRect().height;
      return {
        id: tile.dataset.templateId,
        ratio: box ? box.width / box.height : null,
        width: box ? box.width : 0,
        lines: lines.length,
        // Text as the browser actually lays it out, after the page's own scale.
        sizes: lines.map(sizeOf),
        painted: lines.map(scaled),
        text: lines.map((el) => el.textContent.trim()).filter(Boolean),
      };
    });
  });

  for (const preview of previews) {
    // A real Letter sheet: 8.5 / 11 = 0.7727. A thumbnail with the wrong
    // proportions is not showing the document that would open.
    expect(preview.ratio, `${preview.id} is not page-shaped`).toBeCloseTo(8.5 / 11, 2);
    expect(preview.width, `${preview.id} preview is too small to read`).toBeGreaterThan(100);
    expect(preview.lines, `${preview.id} previews no paragraphs`).toBeGreaterThan(0);
    // The sheet is scaled as one block, so every line must end up painted at
    // less than its point size — a preview drawn at full size would overflow.
    for (const height of preview.painted) {
      expect(height, `${preview.id} is not scaled down`).toBeLessThan(20);
    }
  }

  const report = previews.find((preview) => preview.id === "report");
  expect(report.text[0]).toBe("Report title");
  // The title is bigger than the body text, which is the only reason a
  // miniature is legible as a report at all.
  expect(Math.max(...report.sizes)).toBeGreaterThan(Math.min(...report.sizes) * 2);

  const blank = previews.find((preview) => preview.id === "blank");
  expect(blank.text, "the blank template previews an empty page").toEqual([]);

  expect(consoleErrors).toEqual([]);
});

test("picking a template opens that document, and closes the File page", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openFilePage(page);
  await openPane(page, "new");

  await page.locator('.file-template-tile[data-template-id="report"]').click();
  await expect(page.locator("#panelFile")).toBeHidden();
  await expect(page.locator("#docTitle")).toHaveValue(/Report/);

  // The styles the template names are the styles the opened document has — the
  // preview is not a picture of a document nobody can produce.
  await expect(page.locator("#a11yDocument")).toContainText("Report title");
  await expect(page.locator("#a11yDocument")).toContainText("Recommendation");
  const styles = await definedParagraphStyles(page);
  // The registry reports a style's NAME, which is what `w:name` carries — the
  // headings are "heading 1", not the "Heading1" id the body references.
  for (const style of ["Title", "Subtitle", "heading 1"]) {
    expect(styles, `the opened template must define ${style}`).toContain(style);
  }

  expect(consoleErrors).toEqual([]);
});
