// A finger that scrolls must not raise the phone keyboard (owner report,
// 2026-11-02, iPhone: "as soon as I click anywhere, even to slide, it opens the
// keyboard"). The editor's text input is what raises the soft keyboard when
// focused, so the guarantee is: a touch focuses it only when the touch is a
// TAP. A touch the browser turns into a scroll (pointercancel) or that lifts
// far from where it landed leaves it unfocused; a mouse press still focuses at
// once.
//
// Seen red: with `focusAfterTap(event)` in editor_pointer.mjs reverted to
// `ed.focusEditorSurface()`, "a touch that turns into a scroll" fails — the
// input is focused on pointerdown.
import { test, expect, gotoEditor } from "./fixtures.mjs";

async function press(page, kind, sequence) {
  return page.evaluate(
    ({ kind, sequence }) => {
      const input = document.getElementById("editorTextInput");
      input?.blur();
      const sheet = document.querySelector(".page-wrap .page");
      const r = sheet.getBoundingClientRect();
      const x = r.left + r.width * 0.4;
      const y = r.top + 160;
      for (const [type, dx, dy] of sequence) {
        sheet.dispatchEvent(
          new PointerEvent(type, {
            bubbles: true,
            cancelable: true,
            composed: true,
            pointerId: 11,
            pointerType: kind,
            isPrimary: true,
            button: 0,
            buttons: type === "pointerdown" ? 1 : 0,
            clientX: x + dx,
            clientY: y + dy,
          }),
        );
      }
      return document.activeElement === input;
    },
    { kind, sequence },
  );
}

test("a touch that turns into a scroll does not focus the text input", async ({ page }) => {
  await gotoEditor(page);
  expect(await press(page, "touch", [["pointerdown", 0, 0]])).toBe(false);
  expect(await press(page, "touch", [["pointerdown", 0, 0], ["pointercancel", 0, 40]])).toBe(false);
  expect(await press(page, "touch", [["pointerdown", 0, 0], ["pointerup", 0, 40]])).toBe(false);
});

test("a tap focuses the text input, and a mouse press focuses at once", async ({ page }) => {
  await gotoEditor(page);
  expect(await press(page, "touch", [["pointerdown", 0, 0], ["pointerup", 2, 1]])).toBe(true);
  expect(await press(page, "mouse", [["pointerdown", 0, 0]])).toBe(true);
});
