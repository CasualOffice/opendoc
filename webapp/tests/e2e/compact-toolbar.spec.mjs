// THE COMPACT BAR'S WIDTH BUDGET, AND WHAT GROUPING IT MAY NOT COST.
//
// Reported by the owner: "reduce width of style dropdown as well as font size
// .. its more space than necessary .. and adding scrolling even in normal
// resolution and scale .. im talking about compact view and also its better to
// group things like ordered list and unordered lists and alignments .. like in
// google docs".
//
// Measured before the fix, on the `rich` fixture in compact mode:
//
//   viewport 1280 x 800  ->  bar clientWidth 1272, scrollWidth 1413
//   the bar did not stop scrolling until a 1421px viewport
//   #stylesTrigger 227px, #fontFamily 136px, #fontSize 72px
//   and `paragraph.bullets` / `paragraph.numbering` rendered NOTHING, because
//   those are the CONTEXT-MENU ids; the registry calls them
//   `paragraph.list.bullet` / `paragraph.list.numbered`.
//
// So there were three defects behind one symptom, and the third one is the
// reason a "does every declared control exist" guard is in this file: the
// layout table claimed a bulleted and a numbered list button for months while
// the bar shipped with neither, and the comment above the table claimed a
// `compact-parity.spec.mjs` was failing the build over exactly that. No such
// file existed.
//
// The budget these tests enforce (docs/115 §8):
//
//   * At 1280 x 800 — the same viewport the ribbon's Home band is budgeted
//     against — EVERY group is inline and the bar does not scroll. This is the
//     load-bearing assertion: widen a control or add one and it goes red,
//     which is what stops the bar from silently growing back.
//   * At any narrower width the bar STILL does not scroll: whatever does not
//     fit folds into the "⋯" menu, the way Google Docs' toolbar does, and
//     everything folded is still reachable there.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

/** The viewport the chrome is budgeted against. Not a round number chosen for
 *  looks: docs/64 and docs/115 budget the ribbon's Home band at 1280px, and a
 *  second chrome that needs a wider screen than the first is not a compact
 *  one. 800 rather than 720 so the page area is realistic. */
const BUDGET = { width: 1280, height: 800 };

async function intoCompact(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#modeCompact").click();
  await expect(page.locator("#compactToolbar")).toBeVisible();
  // The fold is re-decided once the real font metrics land; wait for the
  // decision rather than for a duration.
  await expect
    .poll(() => page.evaluate(() => document.querySelectorAll("#compactToolbar .cgroup").length))
    .toBeGreaterThan(0);
}

const geometry = (page) =>
  page.evaluate(() => {
    const bar = document.getElementById("compactToolbar");
    const more = document.getElementById("compactOverflowBtn");
    return {
      client: bar.clientWidth,
      scroll: bar.scrollWidth,
      overflowX: getComputedStyle(bar).overflowX,
      moreShown: !!more && !more.hidden,
      inline: [...bar.querySelectorAll(":scope > .cgroup")].map((g) => g.dataset.group),
      folded: [...document.querySelectorAll("#compactOverflowMenu > .cgroup")].map(
        (g) => g.dataset.group,
      ),
    };
  });

test("at 1280px the whole compact bar is inline and nothing scrolls", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(BUDGET);
  await intoCompact(page);

  const g = await geometry(page);
  // The bug, stated as a number. Before the fix this read 1413 against 1272.
  expect(
    g.scroll,
    `the compact bar overflows its own width by ${g.scroll - g.client}px at ` +
      `${BUDGET.width}px. Narrow a control or move one into a group that can fold — ` +
      "do not let it scroll (docs/115 §8).",
  ).toBeLessThanOrEqual(g.client);
  // And it is not merely "fits because it folded": at the budget width the
  // user gets every group without opening a menu.
  expect(
    g.folded,
    "no group may be folded into the ⋯ menu at the budget viewport",
  ).toEqual([]);
  expect(g.moreShown, "the ⋯ button is not needed at the budget viewport").toBe(false);
  expect(g.inline).toEqual([
    "history",
    "zoom",
    "style",
    "font",
    "size",
    "text",
    "insert",
    "align",
    "spacing",
    "lists",
    "indent",
    "clear",
  ]);
  // A scrollbar cannot come back by CSS either.
  expect(g.overflowX).toBe("hidden");

  expect(consoleErrors).toEqual([]);
});

test("the narrowed controls still say what they are", async ({ page, consoleErrors }) => {
  await page.setViewportSize(BUDGET);
  await intoCompact(page);

  const sizes = await page.evaluate(() => {
    const w = (id) => Math.round(document.getElementById(id).getBoundingClientRect().width);
    return { style: w("stylesTrigger"), font: w("fontFamily"), size: w("fontSize") };
  });
  // The owner's "more space than necessary", as a ceiling. These were 227 /
  // 136 / 72.
  expect(sizes.style).toBeLessThanOrEqual(140);
  expect(sizes.font).toBeLessThanOrEqual(122);
  expect(sizes.size).toBeLessThanOrEqual(58);

  // Narrow is not the same as useless. "Heading 1" is the name the owner's
  // constraint names, and it must read in full rather than as "Heading…".
  const heading1 = await page.evaluate(() => {
    const label = document.getElementById("stylesTriggerLabel");
    const before = label.textContent;
    label.textContent = "Heading 1";
    const clipped = label.scrollWidth > label.clientWidth + 0.5;
    const room = label.clientWidth - label.scrollWidth;
    label.textContent = before;
    return { clipped, room };
  });
  expect(
    heading1.clipped,
    `"Heading 1" is ellipsised in the style trigger (${heading1.room}px short). ` +
      "Narrowing a control may not clip its value into uselessness.",
  ).toBe(false);

  // The largest font size the format allows must still be readable in the box.
  const biggest = await page.evaluate(() => {
    const input = document.getElementById("fontSize");
    const before = input.value;
    input.value = "1638";
    const clipped = input.scrollWidth > input.clientWidth + 0.5;
    input.value = before;
    return clipped;
  });
  expect(biggest, '"1638" is clipped in the compact size box').toBe(false);

  expect(consoleErrors).toEqual([]);
});

test("every control the layout table declares actually renders", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(BUDGET);
  await intoCompact(page);

  // Read the declaration from the module itself rather than restating it here,
  // so a row added to the table is covered on arrival.
  const missing = await page.evaluate(async () => {
    const mod = await import("/src/compact_toolbar.mjs");
    const declared = mod.compactCommandIds();
    const present = new Set(
      [...document.querySelectorAll("#compactToolbar [data-command-id], #compactAlignMenu [data-command-id], #compactOverflowMenu [data-command-id]")].map(
        (el) => el.dataset.commandId,
      ),
    );
    return declared.filter((id) => !present.has(id));
  });
  expect(
    missing,
    "a command id in COMPACT_TOOLBAR that the registry does not answer renders " +
      "NOTHING and says nothing — which is how the compact bar shipped with no " +
      "bulleted and no numbered list button",
  ).toEqual([]);

  expect(consoleErrors).toEqual([]);
});

test("alignment is one Docs-style dropdown that still offers all four, and reports the caret's", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(BUDGET);
  await intoCompact(page);

  const trigger = page.locator("#compactAlignBtn");
  await expect(trigger).toBeVisible();
  await expect(trigger).toHaveAttribute("aria-haspopup", "menu");

  await trigger.click();
  const menu = page.locator("#compactAlignMenu");
  await expect(menu).toBeVisible();
  // Every alignment is still reachable, each announced by its own name.
  expect(
    await menu.locator("[data-command-id]").evaluateAll((els) =>
      els.map((e) => [e.dataset.commandId, e.getAttribute("aria-label")]),
    ),
  ).toEqual([
    ["paragraph.align.start", "Align left"],
    ["paragraph.align.center", "Align center"],
    ["paragraph.align.end", "Align right"],
    ["paragraph.align.justify", "Justify"],
  ]);

  // Applying from the menu really aligns, and the trigger then reports it —
  // the one thing that makes a single button as informative as four.
  await menu.locator('[data-command-id="paragraph.align.center"]').click();
  await expect.poll(() =>
    page.evaluate(() =>
      document.getElementById("compactAlignBtn").getAttribute("aria-label"),
    ),
  ).toBe("Alignment: Align center");
  await expect(page.locator("#alignCenter")).toHaveAttribute("aria-pressed", "true");
  await expect(
    menu.locator('[data-command-id="paragraph.align.center"]'),
  ).toHaveAttribute("aria-checked", "true");

  // A transient surface closes when you point somewhere else
  // (light-dismiss-contract.spec.mjs states the rule; this control is new).
  await page.mouse.click(8, 8);
  await expect(menu).toBeHidden();

  // Reachable with no pointer at all: the trigger takes focus, Enter opens the
  // menu ON the current choice (the popover manager's keyboard path), and
  // Escape puts focus back where it was. Grouping four buttons into one menu
  // may not cost the keyboard a single one of them.
  await trigger.focus();
  await page.keyboard.press("Enter");
  await expect(menu).toBeVisible();
  expect(
    await page.evaluate(() => document.activeElement?.dataset?.commandId ?? null),
  ).toBe("paragraph.align.center");
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(trigger).toBeFocused();

  // And the capability is still on a second surface with its own name, which
  // is the rule grouping is most likely to break (docs/105 UX-004).
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("align");
  for (const label of ["Align left", "Align center", "Align right", "Justify"]) {
    await expect(
      page.locator("#cmdList .cmd-item", { hasText: label }).first(),
    ).toBeVisible();
  }
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("the list group offers checklist, bullets and numbering, and they work", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(BUDGET);
  await intoCompact(page);

  const lists = page.locator('#compactToolbar .cgroup[data-group="lists"]');
  await expect(lists).toHaveAttribute("role", "group");
  await expect(lists).toHaveAttribute("aria-label", "Lists");
  expect(
    await lists.locator("[data-command-id]").evaluateAll((els) =>
      els.map((e) => e.dataset.commandId),
    ),
  ).toEqual([
    "paragraph.list.checklist",
    "paragraph.list.bullet",
    "paragraph.list.numbered",
  ]);

  // The engine's own answer, not a pixel: `updateToolbar` reads the caret's
  // list kind back out of the document and reflects it, so the ribbon button
  // for the same command going pressed is the document saying "this paragraph
  // is a bulleted list now".
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "false");
  await lists.locator('[data-command-id="paragraph.list.bullet"]').click();
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");
  await lists.locator('[data-command-id="paragraph.list.numbered"]').click();
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});

test("below the budget the bar folds instead of scrolling, and the fold keeps everything reachable", async ({
  page,
  consoleErrors,
}) => {
  // 1152 x 800 is a 1440px screen at the 125% scale the owner mentioned, and
  // it is where the bar used to be worst.
  await page.setViewportSize({ width: 1152, height: 800 });
  await intoCompact(page);

  const g = await geometry(page);
  expect(g.scroll, "the bar must never scroll sideways").toBeLessThanOrEqual(g.client);
  expect(g.moreShown, "something folded, so the ⋯ button has to be showing").toBe(true);
  expect(g.folded.length).toBeGreaterThan(0);
  expect([...g.inline, ...g.folded].sort()).toEqual(
    [
      "align",
      "clear",
      "font",
      "history",
      "indent",
      "insert",
      "lists",
      "size",
      "spacing",
      "style",
      "text",
      "zoom",
    ].sort(),
  );

  // Folded is not gone: the menu opens and the controls in it are the real ones.
  await page.locator("#compactOverflowBtn").click();
  const menu = page.locator("#compactOverflowMenu");
  await expect(menu).toBeVisible();
  for (const group of g.folded) {
    await expect(menu.locator(`.cgroup[data-group="${group}"]`)).toBeVisible();
  }

  expect(consoleErrors).toEqual([]);
});

test("leaving compact mode gives the ribbon its borrowed controls back", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(BUDGET);
  await intoCompact(page);
  await expect(page.locator("#compactToolbar #stylesTrigger")).toBeVisible();

  // Where each borrowed control lives when the ribbon owns it. `#zoom` is the
  // status bar's, not the band's — which is exactly why this asserts the home
  // rather than "somewhere in the ribbon".
  const HOMES = {
    stylesTrigger: '.ribbon-panel[data-panel="home"]',
    fontFamily: '.ribbon-panel[data-panel="home"]',
    fontSize: '.ribbon-panel[data-panel="home"]',
    zoom: ".zoom .zoom-value-control",
  };
  await page.locator("#modeRibbon").click();
  for (const [id, home] of Object.entries(HOMES)) {
    await expect(page.locator(`#compactToolbar #${id}`)).toHaveCount(0);
    await expect(page.locator(`${home} #${id}`)).toHaveCount(1);
  }
  // And the size box gets its spinner back: the suppression is scoped to the
  // compact bar, where − and + already do that job.
  expect(
    await page.evaluate(() => getComputedStyle(document.getElementById("fontSize")).appearance),
  ).not.toBe("textfield");

  expect(consoleErrors).toEqual([]);
});

test("compact text-color menus anchor to the visible compact controls", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#modeCompact").click();

  for (const [command, menuId] of [
    ["format.color", "textColorMenu"],
    ["format.highlight", "highlightMenu"],
  ]) {
    const trigger = page.locator(`#compactToolbar [data-command-id="${command}"]`);
    await expect(trigger).toBeVisible();
    await expect(trigger).toHaveAttribute("aria-controls", menuId);
    await trigger.click();

    const menu = page.locator(`#${menuId}`);
    await expect(menu).toBeVisible();
    await expect(trigger).toHaveAttribute("aria-expanded", "true");
    const [triggerBox, menuBox] = await Promise.all([trigger.boundingBox(), menu.boundingBox()]);
    expect(triggerBox).not.toBeNull();
    expect(menuBox).not.toBeNull();
    expect(Math.abs(menuBox.x - triggerBox.x), `${menuId} should share its trigger's x`).toBeLessThanOrEqual(1);
    expect(
      Math.abs(menuBox.y - (triggerBox.y + triggerBox.height + 4)),
      `${menuId} should open four pixels below its trigger`,
    ).toBeLessThanOrEqual(1);

    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
    await expect(trigger).toHaveAttribute("aria-expanded", "false");
  }

  expect(consoleErrors).toEqual([]);
});
