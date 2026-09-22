// A control the user cannot read is not a control.
//
// The styles gallery was 202px wide with every card at `flex: 1 1 0`, so five
// cards got about 40px each and every name ellipsised away: the ribbon showed
// "No… Bo… H… Ca…". The whole point of a gallery over the dropdown beside it is
// that you can SEE what you are choosing, so a gallery of unreadable buttons is
// strictly worse than the dropdown alone. The font trigger was clipped the same
// way at 122px, reporting "Liberation S…".
//
// The two controls get different contracts on purpose, because the Home band has
// a hard 1280px no-scrollbar budget (docs/64) with only about 55px of slack, and
// the two fixes competed for it. Style names are short, come from a set we
// control, and the gallery is pointless if you cannot read them — so they win the
// budget and must fit outright. Font family names are arbitrary and can exceed
// any sane toolbar width, so widening that control enough to hold one would have
// spent most of the slack and pushed the whole Styles group into the overflow
// menu. It keeps Word's bargain instead: show a prefix, never lose the name.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

/** Rows the user cannot fully read, by either mechanism that hides them.
 *
 *  A row can be cut off two ways and they need separate checks. Its own box can
 *  be too narrow for its text, which shows as an ellipsis and is caught by
 *  `scrollWidth > clientWidth`. Or it can keep its natural width and be clipped
 *  away by an ancestor's `overflow: hidden` — where the row measures perfectly
 *  healthy and is simply not on screen. The first draft of this helper tested
 *  only the former, so squeezing the control left it green while half the names
 *  were invisible. Both are asked here. */
async function unreadable(locator) {
  return locator.evaluateAll((els) => {
    const bad = [];
    for (const el of els) {
      const name = el.textContent.trim();
      if (el.scrollWidth > el.clientWidth + 1) {
        bad.push(`${name} (ellipsised)`);
        continue;
      }
      const box = el.getBoundingClientRect();
      const clip = el.parentElement.getBoundingClientRect();
      // Half a pixel of tolerance for subpixel layout, not for a real overhang.
      if (box.left < clip.left - 0.5 || box.right > clip.right + 0.5) {
        bad.push(`${name} (clipped by its container)`);
      }
    }
    return bad;
  });
}

test("every style in the menu shows its whole name", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // First: the control has to actually be ON the ribbon. A group that no longer
  // fits the 1280px band is relocated wholesale into the "⋯" overflow menu, where
  // it is laid out differently and measures perfectly healthy — so every geometry
  // assertion below would pass while the user sees no Styles control at all. An
  // earlier draft of this file missed exactly that and stayed green through the
  // regression it was written to catch.
  await expect(
    page.locator('.ribbon-panel[data-panel="home"] #stylesTrigger'),
  ).toBeVisible();
  await expect(page.locator("#ribbonOverflowBtn")).toBeHidden();

  // The trigger's own label, closed: the name of the current style has to be
  // readable there, because that is the control's whole resting state.
  expect(
    await unreadable(page.locator("#stylesTriggerLabel")),
    "the trigger is cutting off the current style's name",
  ).toEqual([]);

  await page.locator("#stylesTrigger").click();
  const rows = page.locator("#stylesMenu .style-option-name");
  await expect.poll(() => rows.count()).toBeGreaterThan(2);

  // Names, not initials: a row has to be wide enough to tell "Heading 1" from
  // "Heading 2" at a glance, which is the only reason the list is worth opening.
  for (const name of await rows.allTextContents()) {
    expect(name.trim().length, `a style row reads "${name}"`).toBeGreaterThan(
      2,
    );
  }
  expect(await unreadable(rows), "style rows are being cut off").toEqual([]);

  expect(consoleErrors).toEqual([]);
});

test("the font control never loses the font name", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  const trigger = page.locator("#fontFamily");
  const label = page.locator("#fontFamilyLabel");
  await expect(label).toBeVisible();

  const shown = (await label.textContent()).trim();
  expect(shown.length).toBeGreaterThan(0);

  // The trigger is too narrow for long family names by design, so the ellipsis is
  // expected. What is NOT acceptable is the name becoming unrecoverable: a
  // document authored in a long-named face would then report a font that is not
  // its own, with no way to find out otherwise. The tooltip is what makes the
  // narrow control honest, so it is the thing under test.
  const tooltip = await trigger.getAttribute("title");
  expect(
    tooltip,
    "the font trigger has no tooltip carrying the full name",
  ).toBeTruthy();
  expect(tooltip).toContain(shown);

  // And the label must be the real family, not a pre-truncated string baked into
  // the DOM — clipping belongs to CSS, so the full name stays selectable,
  // searchable, and readable by assistive tech.
  expect(
    shown.endsWith("\u2026"),
    `the label text is truncated in the DOM: "${shown}"`,
  ).toBe(false);
  expect(tooltip.replace(/^Font:\s*/, "")).toBe(shown);

  expect(consoleErrors).toEqual([]);
});
