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
// cut off the CI serif). The margin is deliberately large: these now sit well
// inside the shell's own ceiling, so a face with wider metrics has room to wrap
// a note without turning this red.
//
// THE ROSTER IS NOW ALL OF THEM, and that is the finding this round came from.
// This file used to carry its own list of eight "dialogs the density pass
// reshaped", with a comment saying it "should grow to the rest as they are done,
// not be weakened to fit one that has not been". It never grew. Measured
// 2026-10-05: Keyboard shortcuts held 872px of content in a 612px body on a
// 1280x720 laptop AND had no footer, so the keymap ran off the bottom edge of
// the card with nothing under it — the same class of defect this spec had
// already been written for once, in a dialog it did not cover. A guard whose
// subject list is maintained by hand will always be behind the product, so the
// list is `modal-roster.mjs` — the one `dialog-contract.spec.mjs` already proves
// is complete against `editor.html`.
import { test, expect } from "./fixtures.mjs";
import { DIALOGS } from "./modal-roster.mjs";

/** A laptop. The complaint does not reproduce at 900px of viewport height,
 *  which is why the earlier passes measured it away rather than at it. */
const LAPTOP = { width: 1280, height: 720 };

for (const dialog of DIALOGS()) {
  test(`${dialog.name}: the whole dialog is on the screen of a laptop`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(LAPTOP);
    await dialog.open(page);
    await expect(page.locator(`#${dialog.id}`)).toBeVisible();

    const fit = await page.evaluate((id) => {
      const root = document.getElementById(id);
      const card = root.querySelector(".dialog-card");
      // A confirmation carries its whole message in the head, so it has no
      // `.dialog-body` at all (`.dialog-card:not(:has(.dialog-body))` in the
      // stylesheet says so). There is then nothing that scrolls, and the card
      // check below is the whole question for it.
      const body = root.querySelector(".dialog-body");
      const box = card.getBoundingClientRect();
      return {
        cardHeight: Math.round(box.height),
        top: Math.round(box.top),
        bottom: Math.round(box.bottom),
        client: body ? body.clientHeight : 0,
        scroll: body ? body.scrollHeight : 0,
        // A real, PAINTED action row. `.dialog-foot` alone is not the
        // guarantee: an empty one would read as satisfied and close nothing.
        actions: [...root.querySelectorAll(".dialog-foot button")].filter(
          (b) => b.getClientRects().length > 0,
        ).length,
      };
    }, dialog.id);

    // A dialog whose body scrolls must also SAY where it ends. Keyboard
    // shortcuts had no footer at all, which is what turned its overflow from
    // "scroll for more" into "the keymap runs off the edge of the card with
    // nothing under it" (HF-265 D3). Every other dialog here already closes
    // from its own action row as well as from the X.
    if (fit.client > 0) {
      expect(
        fit.actions,
        `#${dialog.id} has a scrolling body and no painted action row under it, so its ` +
          `content ends at the edge of the card with nothing beneath it`,
      ).toBeGreaterThan(0);
    }

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
