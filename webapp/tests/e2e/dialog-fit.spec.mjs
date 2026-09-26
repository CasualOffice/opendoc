// "All dialogs wasting too much of space" — the measurable half of it.
//
// `dialog-contract.spec.mjs` asks whether anything is painted OUTSIDE the box
// that holds it. That is a different question from this one, and it passes
// happily on a dialog that is twice as tall as the screen: `.dialog-body` is a
// scroller, so content below the fold is legal geometry. Document properties
// reported 696px of content in a 560px body and Settings 779px in 542px, and
// every existing assertion about both was green.
//
// What was actually wrong is that the thing you opened the dialog to do was
// below the fold — the file's Revision and Language in one, Autosave and both
// proofing toggles in the other. So this spec asserts the property those
// numbers were failing: on a laptop, the dialog fits.
//
// WHY THIS IS NOT A PINNED PIXEL VALUE. It asserts a relation (`scrollHeight`
// against `clientHeight`) rather than a height, because a height that fits in
// Inter on a Mac clips in the Linux runner's fallback faces — a mistake already
// made in this stylesheet (`.drop-cap-sample`, where a 52px box fit Georgia and
// cut off the CI serif). The margin is deliberately large: these four now sit
// 107px to 265px inside the shell's own ceiling, so a face with wider metrics
// has room to wrap a note without turning this red.
//
// Scoped to the four dialogs the density pass reshaped. It should grow to the
// rest as they are done, not be weakened to fit one that has not been.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

/** A laptop. The complaint does not reproduce at 900px of viewport height,
 *  which is why the earlier passes measured it away rather than at it. */
const LAPTOP = { width: 1280, height: 720 };

const DIALOGS = [
  {
    id: "propertiesPanel",
    name: "Document properties",
    async open(page) {
      await page.locator("#propertiesBtn").click();
    },
  },
  {
    id: "settingsPanel",
    name: "Settings",
    async open(page) {
      await page.locator("#settingsBtn").click();
    },
  },
  {
    id: "watermarkDialog",
    name: "Watermark",
    async open(page) {
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="layout"]').click();
      await page.locator("#watermarkBtn").click();
      // The text half is what the dialog is FOR, and it is the taller state —
      // measuring the default "No watermark" state would measure the easy one.
      await page.locator("#watermarkKindText").check();
    },
  },
  {
    id: "dropCapDialog",
    name: "Drop cap",
    async open(page) {
      await clickIntoFirstPage(page);
      await page.keyboard.press(`${MOD}+Shift+P`);
      await expect(page.locator("#cmdPalette")).toBeVisible();
      await page.locator("#cmdInput").fill("drop cap");
      await page.locator(".cmd-item", { hasText: /drop cap/i }).first().click();
    },
  },
];

for (const dialog of DIALOGS) {
  test(`${dialog.name}: the whole dialog is on the screen of a laptop`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(LAPTOP);
    await gotoEditor(page);
    await dialog.open(page);
    await expect(page.locator(`#${dialog.id}`)).toBeVisible();

    const fit = await page.evaluate((id) => {
      const root = document.getElementById(id);
      const card = root.querySelector(".dialog-card");
      const body = root.querySelector(".dialog-body");
      const box = card.getBoundingClientRect();
      return {
        cardHeight: Math.round(box.height),
        top: Math.round(box.top),
        bottom: Math.round(box.bottom),
        client: body.clientHeight,
        scroll: body.scrollHeight,
      };
    }, dialog.id);

    expect(
      fit.scroll,
      `#${dialog.id} hides ${fit.scroll - fit.client}px of its content below the fold: ` +
        `the body holds ${fit.scroll}px of content in ${fit.client}px of box, on a ` +
        `${LAPTOP.width}x${LAPTOP.height} viewport. The card is ${fit.cardHeight}px tall.`,
    ).toBeLessThanOrEqual(fit.client);

    // And the card itself is inside the window, not merely un-scrolled: a body
    // that fits inside a card taller than the screen would satisfy the check
    // above and still be unusable.
    expect(fit.top, `#${dialog.id} starts above the top of the window`).toBeGreaterThanOrEqual(0);
    expect(fit.bottom, `#${dialog.id} runs past the bottom of the window`).toBeLessThanOrEqual(
      LAPTOP.height,
    );

    expect(consoleErrors).toEqual([]);
  });
}
