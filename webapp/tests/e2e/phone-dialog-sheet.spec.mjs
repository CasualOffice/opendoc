// On a phone, every dialog is a full-width bottom sheet (HF-265 D5).
//
// `body.phone-mode .dialog-overlay` is `place-items: end stretch` and
// `body.phone-mode .dialog-card` is `width: 100%` — a considered Material 3
// modal-bottom-sheet pattern, and the right answer: a dialog centred in a 320px
// window with 24px of overlay padding each side is a 272px card holding controls
// sized for a laptop.
//
// Two dialogs were escaping it, and the mechanism is specificity rather than
// anything about phones. `#splitCellDialog .dialog-card { width: fit-content }`
// is (1,1,0); the phone rung is (0,2,0). An ID beats two classes wherever it is
// declared, so Split cell rendered as a 292px slab pinned to the bottom-LEFT of
// a 390px screen with 98px of empty scrim beside it. `#confirmDialog` has the
// same shape and only reached full width when the runtime message it is handed
// happened to be long enough — so it was a bug whose visibility depended on the
// text, which is the worst kind to leave to a reviewer's eye.
//
// NOTHING MEASURED THIS. There was no guard anywhere on a phone dialog's left
// edge, which is why a (1,1,0) selector could sit over the rung for as long as
// it liked. This one measures all 23, because the defect is not about those two
// dialogs — it is about any rule that can outrank the rung.
//
// WHY THE WHOLE ROSTER, AND NOT AN OPENED-FOR-REAL CARD PER DIALOG. These rows
// reveal the overlay the way `registerModal.open()` does — `hidden = false` plus
// `body.modal-open` — rather than driving 23 phone-width user journeys, several
// of which (a table insert, an object selection, a file chooser) are their own
// specs' subject. That makes the measurement STRICTER rather than weaker: an
// unpopulated body is narrower than a populated one, so a `fit-content` card
// measures smaller and sits further from the left edge, and a card that passes
// here cannot fail once its content arrives. What it does not cover is a dialog
// whose JS sets an inline width on open, and nothing in this product does.
import { test, expect, gotoEditor } from "./fixtures.mjs";
import { DIALOGS } from "./modal-roster.mjs";

/** A small phone. 390 is the iPhone/Pixel class width the shell's phone rung is
 *  written for, and narrower than the Pixel 7 descriptor this project runs at —
 *  `setViewportSize` keeps `isMobile` and `hasTouch`, so this is still a phone
 *  and not a narrow desktop. */
const PHONE = { width: 390, height: 844 };

test("every dialog is a full-width bottom sheet on a phone", async ({ page }) => {
  await page.setViewportSize(PHONE);
  await gotoEditor(page);
  await expect(page.locator("body")).toHaveClass(/phone-mode/);

  const measured = await page.evaluate((ids) => {
    const out = [];
    for (const id of ids) {
      const overlay = document.getElementById(id);
      // The open path, as `modal.mjs` performs it: `element.hidden = false` and
      // `body.classList.add("modal-open")`. Nothing else in that function
      // touches layout.
      overlay.hidden = false;
      document.body.classList.add("modal-open");
      const card = overlay.querySelector(".dialog-card");
      const box = card.getBoundingClientRect();
      const overlayBox = overlay.getBoundingClientRect();
      out.push({
        id,
        left: Math.round(box.left),
        right: Math.round(box.right),
        width: Math.round(box.width),
        overlayLeft: Math.round(overlayBox.left),
        overlayRight: Math.round(overlayBox.right),
        bottom: Math.round(box.bottom),
        overlayBottom: Math.round(overlayBox.bottom),
        cssWidth: getComputedStyle(card).width,
      });
      overlay.hidden = true;
      document.body.classList.remove("modal-open");
    }
    return out;
  }, DIALOGS().map((dialog) => dialog.id));

  const wrong = measured.filter(
    (m) => m.left !== m.overlayLeft || m.right !== m.overlayRight,
  );
  expect(
    wrong.map((m) => `#${m.id} is ${m.width}px at x=${m.left} in a ${m.overlayRight - m.overlayLeft}px window`),
    `a phone dialog is a bottom sheet across the full width of the window. These are not: ` +
      `a card narrower than the overlay is a slab pinned to one side of the scrim, which is ` +
      `what a rule outranking \`body.phone-mode .dialog-card\` produces.`,
  ).toEqual([]);

  // And it is a BOTTOM sheet: `place-items: end stretch` puts the card on the
  // bottom edge, which is where a thumb is. Asserted as a relation to the
  // overlay rather than as a y, because the card's height is its content's.
  const floating = measured.filter((m) => m.bottom !== m.overlayBottom);
  expect(
    floating.map((m) => `#${m.id} ends ${m.overlayBottom - m.bottom}px above the window`),
    "a phone dialog sits on the bottom edge of the window",
  ).toEqual([]);
});
