// The editable focus owner — docs/105 UX-001.
//
// `#pages` paints pixels. It is a `div[tabindex="0"]`, and a non-editable
// element cannot own text input: no mobile browser raises a soft keyboard for
// one, and `compositionstart/update/end` never fire against it. So on a phone
// or tablet the editor could not accept a single character, and the IME path
// was unreachable for real CJK/Korean/Vietnamese input — while
// `ime-preedit.spec.mjs` stayed green because it dispatched synthetic
// CompositionEvents straight at `document`, which a real IME cannot do.
//
// These tests pin the contract that makes touch input, IME, dictation and any
// future spell-check possible. Each one was verified to FAIL against the
// pre-fix code (see the commit message for the mutations used) — a guard that
// cannot go red is not a guard (docs/105 CQ-003).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

/** What actually holds focus once the user is editing, and whether it is a
 *  thing a browser will accept text into. */
async function focusOwner(page) {
  return page.evaluate(() => {
    const el = document.activeElement;
    if (!el) return null;
    const style = getComputedStyle(el);
    const box = el.getBoundingClientRect();
    return {
      id: el.id,
      tag: el.tagName.toLowerCase(),
      editable: el.isContentEditable || ["textarea", "input"].includes(el.tagName.toLowerCase()),
      fontSizePx: parseFloat(style.fontSize),
      display: style.display,
      visibility: style.visibility,
      width: box.width,
      height: box.height,
      left: box.left,
      top: box.top,
    };
  });
}

test("the focused editor surface is an editable element, so a soft keyboard can open", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const owner = await focusOwner(page);
  expect(owner, "something must hold focus after clicking into the document").not.toBeNull();

  // The load-bearing assertion. Before UX-001 this was `div#pages`, and every
  // consequence below followed from that one fact.
  expect(
    owner.editable,
    `the editing surface must be focus-owned by an editable element; got <${owner.tag} id="${owner.id}">`,
  ).toBe(true);

  // A soft keyboard will not open for an element the browser considers
  // unfocusable, so the usual ways of hiding it are all unavailable: it must
  // stay displayed, visible, and non-zero-sized.
  expect(owner.display).not.toBe("none");
  expect(owner.visibility).not.toBe("hidden");
  expect(owner.width).toBeGreaterThan(0);
  expect(owner.height).toBeGreaterThan(0);

  // iOS Safari zooms the whole page when focus lands on a control with a font
  // smaller than 16px — the same rule the coarse-pointer block applies to real
  // inputs. That zoom detaches the fixed chrome from the viewport.
  expect(owner.fontSizePx).toBeGreaterThanOrEqual(16);

  expect(consoleErrors).toEqual([]);
});

test("the focus owner rides the caret, so IME candidates anchor where the user is typing", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const pageBox = await page.locator(".page-wrap .page").first().boundingBox();
  const atStart = await focusOwner(page);

  // An IME candidate window and the iOS autocorrect bar anchor to the focused
  // element's box. Parked at a fixed origin (or off-screen at -10000px, the
  // usual trick) the candidate list appears somewhere the user is not looking,
  // which is why position is part of the contract and not cosmetic.
  expect(atStart.left).toBeGreaterThanOrEqual(pageBox.x - 1);
  expect(atStart.top).toBeGreaterThanOrEqual(pageBox.y - 1);
  expect(atStart.left).toBeLessThanOrEqual(pageBox.x + pageBox.width + 1);

  // Move the caret down several lines; the owner must follow it.
  for (let i = 0; i < 5; i += 1) await page.keyboard.press("ArrowDown");
  await page.waitForTimeout(120);
  const moved = await focusOwner(page);

  expect(
    moved.top,
    "the focus owner must follow the caret down the page, not stay at its origin",
  ).toBeGreaterThan(atStart.top);

  expect(consoleErrors).toEqual([]);
});

test("beforeinput inserts text, which is the only path a soft keyboard has", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Android and iOS soft keyboards deliver printable characters as keyCode 229
  // / key "Unidentified", and swipe-typing, dictation and autocorrect emit no
  // usable keydown at all. `beforeinput` is the event they do fire, so this
  // dispatches exactly that — on the real focus owner, not on `document`.
  const marker = "SOFTKEYBOARDMARKER";
  await page.evaluate((text) => {
    const el = document.activeElement;
    el.dispatchEvent(
      new InputEvent("beforeinput", {
        inputType: "insertText",
        data: text,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, marker);
  await page.waitForTimeout(250);

  // Find is the oracle: it asks the engine, not the DOM, so this proves the
  // text reached the document model rather than merely the proxy's value.
  // Assert the EXACT status string. An earlier draft of this test asserted
  // `not.toHaveText(/no results/i)` while the app's miss string is "No match",
  // so it passed no matter what happened — the same unfailable-guard defect
  // this file exists to close (docs/105 CQ-003). It was caught by mutating the
  // beforeinput handler away and seeing the test stay green.
  await page.keyboard.press(process.platform === "darwin" ? "Meta+f" : "Control+f");
  await page.locator("#findInput").fill(marker);
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("the focus owner never accumulates text of its own", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Composition is the path that genuinely leaves residue. `beforeinput`
  // insertion cannot: the handler preventDefaults, so the browser never writes
  // into the proxy — two earlier drafts of this test (keyboard.type, then
  // keyboard.insertText) both stayed green with the clearing removed, because
  // neither exercised a path that can leave a value. A committed composition
  // does: `preventDefault` on compositionend does not reliably stop the browser
  // from leaving the composed text behind, which is why the commit path clears
  // it explicitly.
  await page.evaluate(() => {
    const el = document.activeElement;
    el.dispatchEvent(new CompositionEvent("compositionstart", { data: "", bubbles: true, cancelable: true }));
    el.dispatchEvent(new CompositionEvent("compositionupdate", { data: "\u3042", bubbles: true, cancelable: true }));
    // What a real IME leaves in the focused control as it composes.
    el.value = "\u3042";
    el.dispatchEvent(new CompositionEvent("compositionend", { data: "\u3042", bubbles: true, cancelable: true }));
  });
  await page.waitForTimeout(250);

  // The engine owns the text. A proxy holding a stale value would resend it on
  // the next composition and duplicate the user's input.
  const value = await page.evaluate(() => {
    const el = document.getElementById("editorTextInput");
    return el ? el.value : null;
  });
  expect(value).toBe("");

  expect(consoleErrors).toEqual([]);
});
