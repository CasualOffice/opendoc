import {
  test,
  expect,
  definedParagraphStyles,
  gotoEditor,
  openFilePage,
  runFilePageCommand,
  runPaletteCommand,
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

// ...and ENDS at the same right edge (HF-265 D4). The left edge was already
// guarded above; the measure was not.
//
// The pane is a backstage column, so it is as wide as the window leaves it —
// 960px at 1280 and 1600px at 1920 — and the TEXT inside it is capped at a
// reading measure. The HEADING over that text was capped at nothing, so it ran
// 152px past its own column at 1280 and 792px past it at 1920: a title that
// stops looking like it belongs to the thing under it.
//
// The decision recorded in `--file-page-measure`: the heading joins the column,
// rather than the column being centred or allowed to grow. Centring would put
// the content away from the left nav it is chosen from, and growing makes a
// four-field pane 1600px wide — the "a four-field dialog as wide as a
// twenty-field one" defect the stylesheet already names. A GALLERY is the
// exception and stays full-bleed: Export and New are grids of tiles, which is
// what ONLYOFFICE's Save As pane is (`filemenu.less` `.format-items`), and a
// gallery is not a reading measure.
//
// Asserted at TWO widths, because one width cannot tell a measure from a
// coincidence.
for (const width of [1280, 1920]) {
  test(`every text pane ends at the same right edge as its heading — ${width}`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    await gotoEditor(page);
    await openFilePage(page);

    for (const [pane] of PANES) {
      await openPane(page, pane);
      const edges = await page.evaluate(() => {
        const detail = document.getElementById("filePageDetail");
        const right = (el) => (el ? Math.round(el.getBoundingClientRect().right) : null);
        const style = getComputedStyle(detail);
        return {
          paneRight: Math.round(
            detail.getBoundingClientRect().right - parseFloat(style.paddingRight),
          ),
          heading: right(detail.querySelector("h2")),
          sub: right(detail.querySelector(".file-detail-sub")),
          // The borrowed PANEL, which is what carries the measure —
          // `.file-pane-body` is an uncapped wrapper around it.
          column: right(detail.querySelector(".panel-in-page")),
        };
      });

      // Every pane: a heading never runs past its own pane.
      expect(edges.heading, `${pane}: the heading runs past the pane`).toBeLessThanOrEqual(
        edges.paneRight,
      );

      if (!TEXT_PANES.has(pane)) continue;
      // A text pane: the heading, its blurb and the column it heads share one
      // right edge, whatever that edge is. A measured number would redden when
      // the measure is retuned; this does not, and still fails the moment the
      // three disagree.
      expect(edges.column, `${pane} shows no column`).not.toBeNull();
      expect(edges.heading, `${pane}: the heading and its content end apart`).toBe(edges.column);
      expect(edges.sub, `${pane}: the blurb and its content end apart`).toBe(edges.column);
      // And the measure really is narrower than the pane at this width —
      // otherwise this would be asserting that two full-width things are both
      // full width, which holds however badly the measure behaves.
      expect(
        edges.column,
        `${pane}: the column fills the whole pane at ${width}, so this proves nothing`,
      ).toBeLessThan(edges.paneRight);
    }

    expect(consoleErrors).toEqual([]);
  });
}

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

  // And all of it is REACHABLE. The dialog was taller than the window, and
  // scrolling it scrolled the document behind — that is what put Settings in
  // the page. A form taller than the viewport is fine here; a form whose
  // bottom cannot be reached, or one whose scrolling moves the document, is
  // the defect coming back. (The form grew past one screen when the language
  // picker joined it, which is precisely when an "it fits" assertion would
  // have started lying about the guarantee it was standing for.)
  const scrolling = await page.evaluate(() => {
    const pane = document.querySelector("#filePageDetail");
    const panel = document.querySelector("#filePageDetail #settingsPanel");
    const behind = document.querySelector(".page-wrap")?.getBoundingClientRect().top ?? 0;
    pane.scrollTop = pane.scrollHeight;
    return {
      scrollable: pane.scrollHeight > pane.clientHeight,
      bottomReached: panel.getBoundingClientRect().bottom <= pane.getBoundingClientRect().bottom + 1,
      documentMoved:
        Math.abs((document.querySelector(".page-wrap")?.getBoundingClientRect().top ?? 0) - behind) >
        1,
      windowScrolled: window.scrollY !== 0,
    };
  });
  expect(scrolling.bottomReached, "the bottom of the settings form cannot be reached").toBe(true);
  expect(scrolling.documentMoved, "scrolling the pane moved the document behind it").toBe(false);
  expect(scrolling.windowScrolled, "scrolling the pane scrolled the window").toBe(false);

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

// ADR-062 corollary C1 — a pointer must point. docs/159 §3: the Measurement-unit
// row deferred to the chooser inside Settings and never revealed it, for two
// independent reasons. It focused synchronously, so `modal.mjs`'s queued
// `initialFocus` overwrote it and focus stayed on the Theme radio group at the
// top of a scrolling body; and it had no `showSettingsPane()` guard, so from the
// File page — where the row lives — it would raise a half-dialog out of a pane.
//
// This asserts the GUARANTEE, not either mechanism: after invoking the row, the
// chooser is the focused element and it is inside the viewport. Both routes are
// driven, because the two bugs are different and one route would miss the other.
// `surface_reveal.test.mjs` holds the ordering half as a unit test with the
// frame scheduler injected; this is the half only a browser can answer.
//
// The two routes are the two the reader has, and BOTH end in the dialog — which
// is a correction to docs/159 §3.2 that writing this guard produced. Every File
// row calls `closeFilePage()` before `command.run()` (`main.js:12255-12267`,
// "one rule, no exception list"), and so does the palette
// (`main.js:13053-13056`, whose comment records this very hazard being fixed
// there). So neither route can reach `openChooser` with the page still open, and
// the half-dialog §3.2 predicted is NOT live on this command. The pane guard in
// `showSettings()` is a consolidation and a latent-case defence, not the fix for
// a reachable bug; the reachable bug was the focus race alone.
//
// Asserting a pane here would have been asserting the wrong thing — and it did,
// until this spec was run. That is the whole argument for driving a guard rather
// than reasoning about one.
for (const route of ["from the File page", "from the command palette"]) {
  test(`the measurement row reveals the chooser, not just the dialog — ${route}`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await gotoEditor(page);
    const onPage = route.endsWith("File page");

    if (onPage) await runFilePageCommand(page, "view.measurementUnits");
    else await runPaletteCommand(page, "view.measurementUnits", "measurement");

    const select = page.locator("#measurementUnitSelect");
    await expect(select, "the chooser is on screen").toBeVisible();
    // The actual report: Settings opened and the measurement parameter was not
    // to be found. Focus is the machine-checkable form of "the reader is looking
    // at it", and it is what the dead `.focus()` call failed to achieve.
    await expect(select, "the chooser is the focused control").toBeFocused();
    // Focused is not the same as revealed: `modal.mjs` focuses with
    // preventScroll, so without an explicit scroll it can be focused and still
    // below the fold.
    await expect(select).toBeInViewport();

    // Either route leaves the dialog, because both returned to the document
    // first. What must NOT happen is a dialog layered over a still-open File
    // page — two layers of chrome between the reader and the document, which is
    // what `closeFilePage()` before `run()` exists to prevent.
    await expect(page.locator("#settingsPanel")).toHaveClass(/dialog-overlay/);
    await expect(page.locator("body")).not.toHaveClass(/file-page-open/);
    expect(await page.locator("#settingsPanel").count()).toBe(1);

    expect(consoleErrors).toEqual([]);
  });
}

// The third route to Settings, which had no guard of its own before: the File
// page's own Settings ROW. It lands in the pane not because of the pane check in
// `showSettings()` but because `view.settings` is a `PANEL_PANES` entry
// (`file_pane.mjs:112`), so the page renders it in place and the command never
// runs. That is worth pinning precisely because it is a different mechanism from
// the gear's (covered above) and from the palette's: three routes, three
// mechanisms, one required outcome.
test("Settings from the File page row lands in the pane, not a dialog over it", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);

  await runFilePageCommand(page, "view.settings");

  await expect(page.locator("#filePageDetail #settingsPanel")).toBeVisible();
  await expect(page.locator("#settingsPanel")).toHaveClass(/panel-in-page/);
  await expect(page.locator("body")).not.toHaveClass(/modal-open/);
  expect(await page.locator("#settingsPanel").count()).toBe(1);

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
