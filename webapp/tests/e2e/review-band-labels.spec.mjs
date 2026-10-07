// The Review band says what its headline actions are (`109` UX-040).
//
// Measured before this change: 18 of the band's 19 controls were unlabelled
// icons, Track changes, Accept and Reject among them, and the two proofing
// toggles shared one glyph. Word labels Track Changes, Accept and Reject on its
// Review tab; ONLYOFFICE labels every one of its Collaboration-tab review
// buttons. docs/123 §4.3's own rule is "a control carries a visible label when it
// is the headline action of its tab", and these three are the Review tab's.
//
// And the chord. Track changes' tooltip named no shortcut, while Ctrl+Shift+E has
// driven the mode since UX-007 — but it CYCLES Editing → Suggesting → Read only
// rather than toggling tracking the way Word's does, so the tooltip that names it
// has to say that too, or the chip would be a small lie. Accept and Reject had
// chords (⌘⌥⏎ / ⌘⌥⌫) advertised nowhere on the band.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  MOD,
  shortcutHint,
  stableBox,
} from "./fixtures.mjs";

async function openReview(page) {
  await page.locator("#tabReview").click();
  await expect(page.locator("#panelReview")).toBeVisible();
}

/** The custom tooltip a hover raises, once it is up. */
async function tooltipFor(page, selector) {
  await page.mouse.move(0, 0);
  await page.locator(selector).hover();
  const tooltip = page.locator(".ribbon-tooltip");
  await expect(tooltip).toBeVisible({ timeout: 3_000 });
  return tooltip;
}

test("Track changes, Accept and Reject carry visible words, inside their buttons", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await openReview(page);
  for (const [id, words] of [
    ["#reviewTrackBtn", "Track changes"],
    ["#reviewAcceptBtn", "Accept"],
    ["#reviewRejectBtn", "Reject"],
  ]) {
    const label = page.locator(`${id} .fmt-big-label`);
    // Visible, and painted at a readable size inside the button — `toBeVisible`
    // alone passes for a label clipped to a sliver.
    await expect(label, `${id} has no visible label`).toBeVisible();
    await expect(label).toHaveText(words);
    const button = await stableBox(page.locator(id));
    const text = await stableBox(label);
    expect(text.width, `${id}'s label is squeezed`).toBeGreaterThan(20);
    expect(text.x).toBeGreaterThanOrEqual(button.x - 1);
    expect(text.x + text.width).toBeLessThanOrEqual(button.x + button.width + 1);
  }
  expect(consoleErrors).toEqual([]);
});

test("Track changes' tooltip names its chord and says that the chord cycles three modes", async ({
  page,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openReview(page);
  const tooltip = await tooltipFor(page, "#reviewTrackBtn");
  await expect(tooltip).toContainText("Track changes");
  await expect(tooltip.locator("kbd")).toHaveText(shortcutHint("⌘⇧E"));
  // The second line is what keeps the chip honest.
  await expect(tooltip.locator(".ribbon-tooltip-detail")).toContainText(
    "cycles Editing, Suggesting and Read only",
  );
});

test("the chord the tooltip names does what the tooltip says", async ({ page }) => {
  // A tooltip asserted without driving the chord is a label test; this drives it.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const segment = (mode) => page.locator(`#reviewModeControl [data-review-mode="${mode}"]`);
  await expect(segment("editing")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press(`${MOD}+Shift+E`);
  await expect(segment("suggesting")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#reviewTrackBtn")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press(`${MOD}+Shift+E`);
  await expect(segment("viewing")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press(`${MOD}+Shift+E`);
  await expect(segment("editing")).toHaveAttribute("aria-pressed", "true");
});

test("Accept and Reject advertise the chords that run them", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openReview(page);
  // Disabled until there is a change to decide, and a disabled control's tooltip
  // is its reason — so make one first.
  await page.locator('#reviewModeControl [data-review-mode="suggesting"]').click();
  await clickIntoFirstPage(page);
  await page.keyboard.type("Q");
  await expect(page.locator("#reviewAcceptBtn")).toBeEnabled();
  let tooltip = await tooltipFor(page, "#reviewAcceptBtn");
  await expect(tooltip.locator("kbd")).toHaveText(shortcutHint("⌘⌥⏎"));
  tooltip = await tooltipFor(page, "#reviewRejectBtn");
  await expect(tooltip.locator("kbd")).toHaveText(shortcutHint("⌘⌥⌫"));
});
