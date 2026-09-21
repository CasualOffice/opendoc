// The Styles control, after docs/114: ONE control, a SHORT list, and the full
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

/** The style at the caret. `#stylesGallery`'s `data-active-style` is written straight
 *  from `doc.paragraphStyleAt(...)` on every toolbar refresh, so it is the DOM-visible
 *  proxy for the engine's paragraph style — the role the deleted select used to play. */
async function reflectedStyle(page) {
  return page.locator("#stylesGallery").getAttribute("data-active-style");
}

/** The styles the band currently offers, in card order. */
async function offered(page) {
  return page.$$eval("#stylesGallery .style-card", (cards) => cards.map((c) => c.dataset.style));
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
  const row = page.locator(`#cmdList .cmd-item[data-command-id="style.${name}"]`);
  await expect(row, `the palette must offer "Style: ${name}"`).toBeVisible();
  await row.click();
}

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
    const gallery = panel.querySelector("#stylesGallery");
    return [...panel.querySelectorAll("[data-style], select")]
      .filter((el) => !gallery.contains(el))
      .map((el) => el.id || el.dataset.style || el.tagName);
  });
  expect(strays, "the band must hold exactly one Styles control").toEqual([]);

  // The one control is a listbox of real, applying cards.
  await expect(page.locator('#stylesGallery[role="listbox"]')).toHaveCount(1);
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
  // gallery range (docs/114 §§2–4). `OFFERED_STYLE_COUNT`.
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
  expect(outside, "the fixture should define a style beyond the offered set").toBeTruthy();

  await applyStyleFromPalette(page, outside);

  // The document is now using it, so the band offers it and marks it current — the
  // guarantee that makes deleting the select cost nothing. Word reaches the same
  // guarantee by scrolling its gallery to the applied style.
  await expect.poll(() => reflectedStyle(page)).toBe(outside);
  const card = page.locator(`#stylesGallery .style-card[data-style="${outside}"]`);
  await expect(card).toBeVisible();
  await expect(card).toHaveAttribute("aria-selected", "true");
  // Still short.
  expect((await offered(page)).length).toBeLessThanOrEqual(6);

  // It stays offered after the caret leaves it, so the user can re-apply it
  // elsewhere — which is the whole point of tracking in-use styles.
  const elsewhere = before.find((name) => name !== outside);
  await page.locator(`#stylesGallery .style-card[data-style="${elsewhere}"]`).click();
  await expect.poll(() => reflectedStyle(page)).toBe(elsewhere);
  expect(await offered(page)).toContain(outside);

  // And re-applying it from the band is a real, single-undo edit.
  await card.click();
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
    const count = await page.locator(`#cmdList .cmd-item[data-command-id="style.${name}"]`).count();
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
        which: el.id || el.className || `${el.parentElement?.className} > select`,
        appearance: cs.appearance,
        hasCaret: cs.backgroundImage !== "none",
      };
    }),
  );
  expect(raw.length, "the chrome should still have dialog/menu selects").toBeGreaterThan(4);

  // Computed style, not the presence of a rule: `.ctl select` sets the `background`
  // SHORTHAND, so a shared `background-image` rule placed above it would be reset and
  // the caret would silently vanish with every rule still in the file.
  const bare = raw.filter((s) => s.appearance !== "none" || !s.hasCaret);
  expect(
    bare,
    "a <select> is rendering OS chrome. One interaction model for one job " +
      "(docs/114 §6): every select in the chrome carries the shared appearance " +
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

  // Compact mode ADOPTS `#stylesGallery` rather than cloning it, so the two chromes
  // cannot drift about what the offered set is or which style is current. Nothing
  // covered compact mode before, and this change moved what it adopts from the
  // deleted `#paragraphStyle` to the gallery — so a broken adoption would have
  // shipped as a Styles control that simply was not there.
  await page.locator("#modeCompact").click();
  const adopted = page.locator("#compactToolbar #stylesGallery");
  await expect(adopted).toBeVisible();
  expect(await offered(page), "the same element, so the same offered set").toEqual(offeredInRibbon);
  // And it still applies: one element means one set of listeners. Asserted rather than
  // guarded by an `if` — the gallery offers more than one style and the caret is in one
  // of them, so a missing target is a defect, not a reason to skip the assertion.
  const current = await reflectedStyle(page);
  const target = offeredInRibbon.find((s) => s !== current);
  expect(target, "the offered set should hold a style other than the caret's").toBeTruthy();
  await adopted.locator(`.style-card[data-style="${target}"]`).click();
  await expect.poll(() => reflectedStyle(page)).toBe(target);

  // Leaving compact mode must put the element BACK in the ribbon, not leave a hole
  // where the Styles group was — `releaseAdoptedControls` is what guarantees that.
  await page.locator("#modeRibbon").click();
  await expect(page.locator('.ribbon-panel[data-panel="home"] #stylesGallery')).toBeVisible();
  await expect(page.locator("#compactToolbar #stylesGallery")).toHaveCount(0);
  expect(await offered(page)).toEqual(offeredInRibbon);

  expect(consoleErrors).toEqual([]);
});
