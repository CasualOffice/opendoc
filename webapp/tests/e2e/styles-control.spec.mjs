// The Styles control, after docs/115: ONE control, a SHORT list, and the full
// stylesheet still reachable — off the ribbon.
//
// The owner reported this four times. The Styles group carried three controls for one
// job: a native `#paragraphStyle` select listing every paragraph style in the document
// flat, a three-card gallery strip, and a "▾" popover that listed every style over
// again as cards. All three called `setParagraphStyle`. The suite had a test named
// "the Styles selector exposes every style and the quick gallery applies a real style",
// which passed throughout — a guard that asserts the control EXISTS cannot see that
// there are three of them, or that one of them is an OS dropdown.
//
// So these are the assertions that can actually fail on the defect:
//
//   0. the one control is a DROPDOWN that names the caret's style unopened, which
//      is the shape the owner asked for and the one Docs ships. A card gallery
//      passes every assertion below and is still the wrong control;
//   1. exactly one control in the Home band applies a paragraph style;
//   2. the band does not offer every style — it offers at most six;
//   3. a style the document is USING is always offered, even outside those six;
//   4. every style stays reachable from two surfaces that are not the band;
//   5. every `<select>` in the chrome wears the product's field, from COMPUTED style.
//
// Each was driven red by mutating main.js/style.css; the mutations and their output
// are in the branch's commit message.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  openCommandPalette,
} from "./fixtures.mjs";

/** The style at the caret. `#stylesTrigger`'s `data-active-style` is written straight
 *  from `doc.paragraphStyleAt(...)` on every toolbar refresh, so it is the DOM-visible
 *  proxy for the engine's paragraph style — the role the deleted select used to play. */
async function reflectedStyle(page) {
  return page.locator("#stylesTrigger").getAttribute("data-active-style");
}

/** Opens the Styles menu and leaves it open. */
async function openStyles(page) {
  await page.locator("#stylesTrigger").click();
  await expect(page.locator("#stylesMenu")).toBeVisible();
}

/** The styles the band currently offers, in menu order. Leaves the menu closed. */
async function offered(page) {
  await openStyles(page);
  const names = await page.$$eval("#stylesMenu .style-option", (rows) =>
    rows.map((row) => row.dataset.style),
  );
  await page.keyboard.press("Escape");
  return names;
}

/** Picks a style from the menu. */
async function pickStyle(page, name) {
  await openStyles(page);
  await page.locator(`#stylesMenu .style-option[data-style="${name}"]`).click();
}

/** Every paragraph style the open document defines, read from the surface that still
 *  carries the complete list: Paragraph properties ▸ Style. */
async function allDefinedStyles(page) {
  return page.$$eval("#paraPanelStyle option", (opts) =>
    opts.map((o) => o.value).filter(Boolean),
  );
}

/** Applies a named style through the command palette — the by-name surface (Word's
 *  Apply Styles). Used to reach a style the short list does not offer. */
async function applyStyleFromPalette(page, name) {
  await openCommandPalette(page);
  await page.locator("#cmdInput").fill(`Style: ${name}`);
  const row = page.locator(
    `#cmdList .cmd-item[data-command-id="style.${name}"]`,
  );
  await expect(row, `the palette must offer "Style: ${name}"`).toBeVisible();
  await row.click();
}

test("the Styles control is a dropdown that names the caret's style unopened", async ({
  page,
  consoleErrors,
}) => {
  // The shape, asserted on its own, because every other test in this file passed
  // against a card gallery — which is what shipped, and which the owner rejected.
  // "One control offering at most six styles" was satisfied by six always-visible
  // cards; what was missing is a control that reads as a dropdown and says which
  // style you are in without being opened. That is Google Docs' shape and it is
  // the one asked for.
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const trigger = page.locator("#stylesTrigger");
  await expect(trigger).toBeVisible();
  await expect(trigger).toHaveAttribute("aria-haspopup", "listbox");

  // Closed to begin with, and nothing of the list is on the band.
  await expect(page.locator("#stylesMenu")).toBeHidden();
  await expect(trigger).toHaveAttribute("aria-expanded", "false");
  const bandRows = await page
    .locator('.ribbon-panel[data-panel="home"] .style-option')
    .count();
  expect(bandRows, "the options belong in the popup, not on the band").toBe(0);

  // It names the current style with the menu shut. A gallery cannot do this, and
  // it is the one thing the deleted 14-entry select did that was worth keeping.
  const current = await reflectedStyle(page);
  expect(
    current,
    "the caret is in a paragraph, so it is in a style",
  ).toBeTruthy();
  await expect(page.locator("#stylesTriggerLabel")).toHaveText(current);

  // Opening shows the short list and marks the current one.
  await trigger.click();
  await expect(page.locator("#stylesMenu")).toBeVisible();
  await expect(trigger).toHaveAttribute("aria-expanded", "true");
  await expect(
    page.locator(`#stylesMenu .style-option[data-style="${current}"]`),
  ).toHaveAttribute("aria-selected", "true");

  // Clicking away closes it — the defect the owner reported about the product's
  // dropdowns generally, asserted here because this one is new.
  await page.mouse.click(5, 5);
  await expect(page.locator("#stylesMenu")).toBeHidden();
  await expect(trigger).toHaveAttribute("aria-expanded", "false");

  expect(consoleErrors).toEqual([]);
});

test("each option is drawn in the style it applies", async ({
  page,
  consoleErrors,
}) => {
  // The reason this is a product popup and not a native <select>: an OS popup
  // renders every row in the system font, so "Heading 1" would be a word rather
  // than a picture of what applying it does. If the rows all render identically
  // the control may as well be native, and the trade made here is not paid for.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openStyles(page);

  const rows = await page.$$eval(
    "#stylesMenu .style-option .style-option-name",
    (labels) =>
      labels.map((el) => {
        const cs = getComputedStyle(el);
        return {
          style: el.closest(".style-option").dataset.style,
          size: parseFloat(cs.fontSize),
          weight: cs.fontWeight,
        };
      }),
  );
  expect(rows.length).toBeGreaterThan(1);
  const distinct = new Set(rows.map((r) => `${r.size}/${r.weight}`));
  expect(
    distinct.size,
    `every row renders identically (${rows.map((r) => `${r.style} ${r.size}px/${r.weight}`).join(", ")})`,
  ).toBeGreaterThan(1);

  expect(consoleErrors).toEqual([]);
});

test("exactly one control in the Home band applies a paragraph style", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const band = page.locator('.ribbon-panel[data-panel="home"]');

  // No OS dropdown in the band at all. `#paragraphStyle` was the last one, and it
  // sat directly above the gallery doing the same job.
  await expect(band.locator("select")).toHaveCount(0);
  await expect(page.locator("#paragraphStyle")).toHaveCount(0);

  // And no second style picker of any shape: every control in the band that can
  // apply a style is inside the ONE gallery. `[data-style]` is how a style-applying
  // control identifies itself, so a re-added strip, popover trigger or duplicate
  // card set anywhere else in the band fails here.
  const strays = await band.evaluate((panel) => {
    const trigger = panel.querySelector("#stylesTrigger");
    return [...panel.querySelectorAll("[data-style], select")]
      .filter((el) => el !== trigger && !trigger.contains(el))
      .map((el) => el.id || el.dataset.style || el.tagName);
  });
  expect(strays, "the band must hold exactly one Styles control").toEqual([]);

  // The one control is a dropdown, and its list is real and applying.
  await expect(
    page.locator('#stylesTrigger[aria-haspopup="listbox"]'),
  ).toHaveCount(1);
  expect((await offered(page)).length).toBeGreaterThan(1);

  expect(consoleErrors).toEqual([]);
});

test("the band offers a SHORT list, not every style in the document", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const cards = await offered(page);
  const defined = await allDefinedStyles(page);

  // Six is the cap: exactly what Docs offers, and the bottom of Word's visible
  // gallery range (docs/115 §§2–4). `OFFERED_STYLE_COUNT`.
  expect(cards.length, `offered: ${cards.join(", ")}`).toBeLessThanOrEqual(6);
  // And it really is a SHORTENING — the document defines more than the band offers.
  // This is the assertion the three-control era could never have passed: both the
  // select and the ▾ popover listed all of `defined`.
  expect(defined.length).toBeGreaterThan(cards.length);
  // Every offered style is one the engine can actually apply.
  for (const name of cards) expect(defined).toContain(name);
  // The offered ones are recommended names, not "whatever came first alphabetically".
  // `Envelope Return` and `Table Heading` are defined by the fixture and must not be
  // on the band: they are not styles a user applies while writing.
  expect(cards).not.toContain("Envelope Return");
  expect(cards).not.toContain("Table Heading");
  expect(cards).toContain("Normal");

  expect(consoleErrors).toEqual([]);
});

test("a style the document is using stays offered, and applies from the band", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const before = await offered(page);
  const defined = await allDefinedStyles(page);
  // A style the SHORT list does not offer — so it can only become offered by being
  // put to use in the document.
  const outside = defined.find((name) => !before.includes(name));
  expect(
    outside,
    "the fixture should define a style beyond the offered set",
  ).toBeTruthy();

  await applyStyleFromPalette(page, outside);

  // The document is now using it, so the band offers it and marks it current — the
  // guarantee that makes deleting the select cost nothing. Word reaches the same
  // guarantee by scrolling its gallery to the applied style.
  await expect.poll(() => reflectedStyle(page)).toBe(outside);
  // The trigger says so without being opened — the one thing the deleted select
  // did that a card gallery did not.
  await expect(page.locator("#stylesTriggerLabel")).toHaveText(outside);
  await openStyles(page);
  const option = page.locator(
    `#stylesMenu .style-option[data-style="${outside}"]`,
  );
  await expect(option).toBeVisible();
  await expect(option).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Escape");
  // Still short.
  expect((await offered(page)).length).toBeLessThanOrEqual(6);

  // It stays offered after the caret leaves it, so the user can re-apply it
  // elsewhere — which is the whole point of tracking in-use styles.
  const elsewhere = before.find((name) => name !== outside);
  await pickStyle(page, elsewhere);
  await expect.poll(() => reflectedStyle(page)).toBe(elsewhere);
  expect(await offered(page)).toContain(outside);

  // And re-applying it from the band is a real, single-undo edit.
  await pickStyle(page, outside);
  await expect.poll(() => reflectedStyle(page)).toBe(outside);
  await page.locator("#undoBtn").click();
  await expect.poll(() => reflectedStyle(page)).toBe(elsewhere);

  expect(consoleErrors).toEqual([]);
});

test("every style the document defines stays reachable from two surfaces off the band", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const defined = await allDefinedStyles(page);
  expect(defined.length).toBeGreaterThan(6);

  // Surface 1 — Paragraph properties ▸ Style: the full list, in a dialog. `defined` was
  // read from that select, so this proves the surface is genuinely OPENABLE rather than
  // merely present in the DOM, and that it offers every style the band does not.
  await page.locator("#paraOptsBtn").click();
  await expect(page.locator("#paragraphPropertiesPanel")).toBeVisible();
  await expect(page.locator("#paraPanelStyle")).toBeVisible();
  for (const name of await offered(page)) expect(defined).toContain(name);
  await page.keyboard.press("Escape");

  // Surface 2 — the command palette, searchable by name (Word's Apply Styles). EVERY
  // style must have a row: shortening the band is only acceptable because nothing became
  // unreachable, and "most of them are in the palette" is not that. Asked one name at a
  // time because the palette filters, so an unfiltered render is not the whole set.
  const missing = [];
  for (const name of defined) {
    await openCommandPalette(page);
    await page.locator("#cmdInput").fill(`Style: ${name}`);
    const count = await page
      .locator(`#cmdList .cmd-item[data-command-id="style.${name}"]`)
      .count();
    if (count === 0) missing.push(name);
    await page.keyboard.press("Escape");
  }
  expect(missing, "every defined style needs a palette row").toEqual([]);

  expect(consoleErrors).toEqual([]);
});

test("every <select> in the chrome wears the product's field, not the OS's", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const raw = await page.$$eval("select", (list) =>
    list.map((el) => {
      const cs = getComputedStyle(el);
      return {
        which:
          el.id || el.className || `${el.parentElement?.className} > select`,
        appearance: cs.appearance,
        hasCaret: cs.backgroundImage !== "none",
      };
    }),
  );
  expect(
    raw.length,
    "the chrome should still have dialog/menu selects",
  ).toBeGreaterThan(4);

  // Computed style, not the presence of a rule: `.ctl select` sets the `background`
  // SHORTHAND, so a shared `background-image` rule placed above it would be reset and
  // the caret would silently vanish with every rule still in the file.
  const bare = raw.filter((s) => s.appearance !== "none" || !s.hasCaret);
  expect(
    bare,
    "a <select> is rendering OS chrome. One interaction model for one job " +
      "(docs/115 §6): every select in the chrome carries the shared appearance " +
      "treatment and the product's caret.",
  ).toEqual([]);

  expect(consoleErrors).toEqual([]);
});

test("the compact chrome borrows the SAME Styles control, and the ribbon gets it back", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const offeredInRibbon = await offered(page);
  expect(offeredInRibbon.length).toBeGreaterThan(1);

  // Compact mode ADOPTS `#stylesTrigger` rather than cloning it, so the two chromes
  // cannot drift about what the offered set is or which style is current. Nothing
  // covered compact mode before, and this change moved what it adopts from the
  // deleted `#paragraphStyle` to the gallery and now to the trigger — so a broken
  // adoption would ship as a Styles control that simply was not there.
  await page.locator("#modeCompact").click();
  const adopted = page.locator("#compactToolbar #stylesTrigger");
  await expect(adopted).toBeVisible();
  expect(
    await offered(page),
    "the same element, so the same offered set",
  ).toEqual(offeredInRibbon);
  // And it still applies: one element means one set of listeners. Asserted rather than
  // guarded by an `if` — the menu offers more than one style and the caret is in one
  // of them, so a missing target is a defect, not a reason to skip the assertion.
  const current = await reflectedStyle(page);
  const target = offeredInRibbon.find((s) => s !== current);
  expect(
    target,
    "the offered set should hold a style other than the caret's",
  ).toBeTruthy();
  await pickStyle(page, target);
  await expect.poll(() => reflectedStyle(page)).toBe(target);

  // Leaving compact mode must put the element BACK in the ribbon, not leave a hole
  // where the Styles group was — `releaseAdoptedControls` is what guarantees that.
  await page.locator("#modeRibbon").click();
  await expect(
    page.locator('.ribbon-panel[data-panel="home"] #stylesTrigger'),
  ).toBeVisible();
  await expect(page.locator("#compactToolbar #stylesTrigger")).toHaveCount(0);
  expect(await offered(page)).toEqual(offeredInRibbon);

  expect(consoleErrors).toEqual([]);
});
