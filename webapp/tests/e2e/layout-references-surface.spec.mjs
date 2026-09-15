// The Layout and References ribbon tabs (docs/105 UX-010, and the IA half of
// OO-001/OO-005).
//
// Every command on these two tabs already existed. Page margins, orientation,
// size and columns were four fieldsets of one dialog reachable from a single
// unlabelled `draft` icon parked on the View tab; the absolute indent and
// spacing fields were reachable only from a paragraph-mark button on Home; wrap
// and object geometry were reachable only from a floating bar that appears when
// an object is already selected. The palette even declared `group: "Layout"`
// with no tab for those commands to live on. So this spec is not about new
// capability — it is about whether the capability is now REACHABLE, from more
// than one surface, and whether the three things that genuinely do not exist
// (table of contents, cross-reference, update fields) say so instead of sitting
// there as buttons that do nothing.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

/** Opens a ribbon tab and waits for its panel. */
async function openTab(page, tab, panel) {
  await page.locator(`#${tab}`).click();
  await expect(page.locator(`#${panel}`)).toBeVisible();
}

/** The palette row for `commandId`, found by searching for its label. Returns
 *  `{ disabled, hint }` — the hint column carries the disabled reason. */
async function paletteRow(page, query, label) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill(query);
  const row = page.locator("#cmdList .cmd-item", { hasText: label }).first();
  await expect(row).toBeVisible();
  const state = {
    disabled: await row.isDisabled(),
    hint: await row.locator(".cmd-hint").innerText(),
  };
  await page.keyboard.press("Escape");
  await expect(page.locator("#cmdPalette")).toBeHidden();
  return state;
}

test("the ribbon exposes Layout and References as real tabs, with Review still last", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // Word's order: Layout and References follow Insert and precede the
  // contextual tabs. Review stays last because the tab strip's End key is
  // asserted (twice, in two other specs) to land there.
  const tabs = await page
    .locator(".ribbon-tab[data-tab]")
    .evaluateAll((els) => els.map((el) => el.dataset.tab));
  expect(tabs).toEqual(["home", "insert", "layout", "references", "table", "view", "review"]);

  // ARIA: each tab must name a panel that actually exists, and selecting it must
  // be reflected on the tab, not only by the panel becoming visible.
  for (const [tab, panel] of [
    ["tabLayout", "panelLayout"],
    ["tabReferences", "panelReferences"],
  ]) {
    const tabEl = page.locator(`#${tab}`);
    await expect(tabEl).toHaveAttribute("role", "tab");
    await expect(tabEl).toHaveAttribute("aria-controls", panel);
    await expect(tabEl).toHaveAttribute("aria-selected", "false");
    await tabEl.click();
    await expect(tabEl).toHaveAttribute("aria-selected", "true");
    await expect(page.locator(`#${panel}`)).toBeVisible();
    await expect(page.locator(`#${panel}`)).toHaveAttribute("aria-labelledby", tab);
  }

  // Arrow-key roving focus must reach the new tabs, or they are mouse-only.
  await page.locator("#tabInsert").focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#tabLayout")).toBeFocused();
  await expect(page.locator("#panelLayout")).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#tabReferences")).toBeFocused();
  await expect(page.locator("#panelReferences")).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

// The 1280px budget (docs/64). The Home band has ~55px of slack, so a new tab is
// only "free" if its own band fits too: a Layout tab whose Arrange group is
// exiled into the "⋯" menu at the default width would be a worse surface than
// the scattered controls it replaced. 1280 is Playwright's Desktop Chrome
// default, which is why it is the number this repo holds.
test("both new bands fit 1280px inline, with no overflow control and no horizontal scroll", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await gotoEditor(page);

  for (const [tab, panel, groups] of [
    ["tabLayout", "panelLayout", ["page-setup", "layout-paragraph", "arrange"]],
    ["tabReferences", "panelReferences", ["ref-navigation", "ref-notes", "ref-fields"]],
  ]) {
    await openTab(page, tab, panel);
    // Every authored group is still a child of the panel — none was relocated
    // into #ribbonOverflowMenu.
    const inline = await page
      .locator(`#${panel} > .rgroup`)
      .evaluateAll((els) => els.map((el) => el.dataset.group));
    expect(inline, `${panel} lost a group to the overflow menu at 1280px`).toEqual(groups);
    await expect(page.locator("#ribbonOverflowBtn")).toBeHidden();
    // And the band itself does not scroll: the overflow mechanism exists
    // precisely so a scrollbar never appears (docs/64).
    const overflow = await page
      .locator(`#${panel}`)
      .evaluate((el) => el.scrollWidth - el.clientWidth);
    expect(overflow, `${panel} overflows its width at 1280px`).toBeLessThanOrEqual(1);
  }

  expect(consoleErrors).toEqual([]);
});

// Page setup is ONE dialog reached with four different intents. Landing every
// button on the orientation control would make three of the four feel like the
// wrong button, so each opens the dialog focused on its own fieldset.
test("each Page setup button opens the dialog on the field it names", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await openTab(page, "tabLayout", "panelLayout");

  for (const [button, focused] of [
    ["#layoutMarginsBtn", "pageMarginTop"],
    ["#layoutSizeBtn", "pageWidth"],
    ["#layoutColumnsBtn", "pageColumnCount"],
  ]) {
    await page.locator(button).click();
    await expect(page.locator("#pageSetupMenu")).toBeVisible();
    await expect(page.locator(`#${focused}`)).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.locator("#pageSetupMenu")).toBeHidden();
  }

  // Orientation lands on the currently chosen orientation segment, which is the
  // control a user pressing "Orientation" wants their hands on.
  await page.locator("#layoutOrientationBtn").click();
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
  await expect(
    page.locator('#pageOrientationSeg button[aria-pressed="true"]'),
  ).toBeFocused();
  await page.keyboard.press("Escape");

  // A plain Page setup opening must NOT inherit the last deep link's target.
  await openTab(page, "tabView", "panelView");
  await page.locator("#pageSetupBtn").click();
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
  await expect(
    page.locator('#pageOrientationSeg button[aria-pressed="true"]'),
  ).toBeFocused();
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

// The prototype drew inline numeric Indent/Spacing fields in the band. The
// paragraph-properties panel already owns those values, reflects them from the
// engine, and applies them through the gated edit path — so the ribbon routes to
// it instead of holding a second, independently-updated copy of the same
// numbers. What has to be true for that to be honest is that the button lands
// you ON the field it names.
test("Layout ▸ Paragraph reaches the real indent and spacing fields", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openTab(page, "tabLayout", "panelLayout");

  await page.locator("#layoutIndentFieldsBtn").click();
  await expect(page.locator("#paragraphPropertiesPanel")).toBeVisible();
  await expect(page.locator("#indentLeft")).toBeFocused();

  await page.locator("#layoutSpacingFieldsBtn").click();
  await expect(page.locator("#paraLineSpacing")).toBeFocused();

  // The relative nudges are the same command the Home band runs, so the proof is
  // that the shared field moves.
  const before = await page.locator("#indentLeft").inputValue();
  await page.locator("#layoutIndentIncBtn").click();
  await expect.poll(() => page.locator("#indentLeft").inputValue()).not.toBe(before);
  await page.locator("#layoutIndentDecBtn").click();
  await expect.poll(() => page.locator("#indentLeft").inputValue()).toBe(before);

  expect(consoleErrors).toEqual([]);
});

// Arrange acts on a selected object. Before there is one the buttons must be
// disabled AND say why — the alternative this repo keeps shipping by accident is
// a live-looking control that does nothing.
test("Arrange is disabled with a reason until an object is selected, then works", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openTab(page, "tabLayout", "panelLayout");

  const wrap = page.locator("#layoutWrapBtn");
  const position = page.locator("#layoutPositionBtn");
  await expect(wrap).toBeDisabled();
  await expect(wrap).toHaveAttribute("title", /Select an image, shape or text box first/);
  await expect(position).toBeDisabled();

  // Select an object through the keyboard command that exists for exactly this
  // (`object.selectNext`), so the test does not depend on where a picture lands
  // on screen.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Select next object");
  await page.locator("#cmdList .cmd-item", { hasText: "Select next object" }).first().click();
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  await openTab(page, "tabLayout", "panelLayout");
  await expect(wrap).toBeEnabled();
  await expect(position).toBeEnabled();

  // Wrap text lands on the inspector's wrap select; Position lands on Left.
  await wrap.click();
  await expect(page.locator(".object-inspector")).toBeVisible();
  await expect(
    page.locator(".object-inspector [data-object-inspector-wrap-select]"),
  ).toBeFocused();
  await position.click();
  await expect(page.locator(".object-inspector [data-object-prop=left]")).toBeFocused();

  expect(consoleErrors).toEqual([]);
});

// References ▸ Notes is where Word keeps Footnote and Endnote, and the only
// place it keeps them. They must still actually insert from here — a moved
// button that lost its handler would be the worst outcome of this change.
test("References ▸ Notes inserts a footnote and an endnote", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openTab(page, "tabReferences", "panelReferences");

  await page.locator("#refFootnoteBtn").click();
  await expect(page.locator("#status")).toContainText("Footnote added");

  await page.locator("#tabReferences").click();
  await page.locator("#refEndnoteBtn").click();
  await expect(page.locator("#status")).toContainText("Endnote added");

  expect(consoleErrors).toEqual([]);
});

// Bookmark and Insert field appear on BOTH Insert and References, as they do in
// Word. One command, two faces — so the second face has to run the same command,
// not a copy of it.
test("the References faces of Bookmark and Insert field run the same commands", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await openTab(page, "tabReferences", "panelReferences");

  await expect(page.locator("#refBookmarkBtn")).toHaveAttribute("data-command", "insert.bookmark");
  await expect(page.locator("#refFieldBtn")).toHaveAttribute("data-command", "insert.field");

  await page.locator("#refBookmarkBtn").click();
  await expect(page.locator("#bookmarkDialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#bookmarkDialog")).toBeHidden();

  await page.locator("#tabReferences").click();
  await page.locator("#refFieldBtn").click();
  await expect(page.locator("#fieldDialog")).toBeVisible();
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

// The three commands that do not exist. docs/63's rule is that a control appears
// only when the command is real; disabled-carrying-a-reason satisfies it,
// because the user learns what the editor cannot do and why. A button that looks
// live and does nothing does not. The reason has to be present on BOTH surfaces
// — the ribbon's tooltip and the palette's hint column — or a keyboard user gets
// the greyed row with no explanation.
test("table of contents, cross-reference and update fields are disabled WITH a reason, on both surfaces", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openTab(page, "tabReferences", "panelReferences");

  const unavailable = [
    ["#refTocBtn", "Table of contents", /field evaluation/i],
    ["#refCrossRefBtn", "Cross-reference", /REF field engine/i],
    ["#refUpdateFieldsBtn", "Update fields", /field-evaluation pass/i],
  ];
  for (const [selector, label, reason] of unavailable) {
    const button = page.locator(selector);
    await expect(button, `${label} must not look available`).toBeDisabled();
    await expect(button, `${label} must say why`).toHaveAttribute("title", reason);
    const row = await paletteRow(page, label, label);
    expect(row.disabled, `${label} must be disabled in the palette too`).toBe(true);
    expect(row.hint, `${label} must carry its reason in the palette`).toMatch(reason);
    await openTab(page, "tabReferences", "panelReferences");
  }

  // Same treatment for the one Layout command with no engine operation.
  await openTab(page, "tabLayout", "panelLayout");
  await expect(page.locator("#layoutBringForwardBtn")).toBeDisabled();
  await expect(page.locator("#layoutBringForwardBtn")).toHaveAttribute("title", /z-order/i);
  const forward = await paletteRow(page, "Bring object forward", "Bring object forward");
  expect(forward.disabled).toBe(true);
  expect(forward.hint).toMatch(/z-order/i);

  expect(consoleErrors).toEqual([]);
});

// ≥2 surfaces, enforced rather than asserted case by case: every button on the
// two new bands carries the id of the command it runs, and every one of those
// ids must have a palette row. This is the guard that makes the next button
// somebody adds to these tabs fail CI if they forget the palette — the exact
// defect that left Picture off the Insert ribbon and Accept-change off every
// durable surface.
test("every Layout and References ribbon button is reachable from the palette too", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const ribbonCommands = await page
    .locator("#panelLayout .rgroup button[data-command], #panelReferences .rgroup button[data-command]")
    .evaluateAll((buttons) => [...new Set(buttons.map((b) => b.dataset.command))]);
  expect(ribbonCommands.length).toBeGreaterThan(10);

  // The palette lists every command when the query is empty, and each row now
  // carries the id it runs — so this is an exact set comparison, not a label
  // match that any similarly-named row could satisfy.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  const paletteCommands = await page
    .locator("#cmdList .cmd-item[data-command-id]")
    .evaluateAll((rows) => rows.map((row) => row.dataset.commandId));
  await page.keyboard.press("Escape");

  const missing = ribbonCommands.filter((id) => !paletteCommands.includes(id));
  expect(
    missing,
    "these Layout/References ribbon buttons have no palette row, so they are single-surface",
  ).toEqual([]);

  expect(consoleErrors).toEqual([]);
});

// The icon font is the one self-hosted Material Symbols Outlined face
// (src/fonts.css); nothing here may reach a CDN. A ligature name the face does
// not carry renders as its literal letters — "wrap_text" as nine glyphs — which
// is both wrong and unmistakably wide. Measuring the rendered box is therefore a
// real check that every new icon resolves in the CHECKED-IN subset.
test("every icon on the new bands resolves to a real glyph in the self-hosted face", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await gotoEditor(page);

  for (const [tab, panel] of [
    ["tabLayout", "panelLayout"],
    ["tabReferences", "panelReferences"],
  ]) {
    await openTab(page, tab, panel);
    const glyphs = await page.locator(`#${panel} .ms`).evaluateAll((els) =>
      els.map((el) => ({ name: el.textContent.trim(), width: el.getBoundingClientRect().width })),
    );
    expect(glyphs.length).toBeGreaterThan(0);
    for (const glyph of glyphs) {
      // A resolved symbol is one square glyph at the button's font size (~20px);
      // an unresolved ligature is one box per letter, so even the shortest name
      // here ("toc") is far wider than this ceiling.
      expect(
        glyph.width,
        `"${glyph.name}" did not resolve to a glyph in the self-hosted Material Symbols face`,
      ).toBeLessThan(34);
    }
  }

  expect(consoleErrors).toEqual([]);
});
