// ArrowUp must always move, or there must be nothing above the caret.
//
// Reported as "the editor stops responding". After Ctrl+End on the owner's
// loan agreement, sixty ArrowUps moved the caret zero pixels; walking up from
// mid-document it stopped dead at the paragraph between two tables and no
// further press did anything. Down was unaffected, so it read as the arrow
// keys dying rather than as a table bug.
//
// The mechanism is HF-024's: the layout crate's `move_vertical` steps by FLOW
// INDEX through a flat line list, and around a table fragment the neighbour by
// flow order is not the neighbour by reading order, so the search dead-ends and
// returns the position it was given. HF-024 was closed for the sideways-jump
// symptom; the UP direction still dead-ends. Until the engine's search is
// geometric, `probeVerticalNeighbour` recovers the move host-side — and only
// when the engine reported no movement at all.
//
// Why ~690 other specs missed it: they work near the document start, inside the
// first viewport, and they press Down. Every test here goes to the END of a
// multi-page document first.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";
import { makeLargeDocx } from "./large-docx.mjs";

/** Where the caret is, in document coordinates — the number that says whether
 *  a key press did anything at all. */
function caretPosition(page) {
  return page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    const view = document.getElementById("viewport");
    if (!caret) return null;
    const rect = caret.getBoundingClientRect();
    const viewRect = view.getBoundingClientRect();
    return {
      x: Math.round(rect.left),
      docY: Math.round(view.scrollTop + rect.top - viewRect.top),
      inTable: !document.getElementById("tabTable").disabled,
    };
  });
}

async function openFixture(page, file) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.setInputFiles("#file", file);
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status?.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length >= 2
      );
    },
    null,
    { timeout: 45_000 },
  );
  await clickIntoFirstPage(page);
}

const PAGE_BREAKS = {
  name: "page-breaks.docx",
  mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  buffer: Buffer.from(makeLargeDocx(6)),
};

// A committed 5-page document that ends in table rows — the shape the bug was
// reported on, without shipping the owner's own file.
const ENDS_IN_A_TABLE = "../fixtures/generated/pagination-fidelity.docx";

for (const [name, file] of [
  ["a page-break document", PAGE_BREAKS],
  ["a document that ends in a table", ENDS_IN_A_TABLE],
]) {
  test(`ArrowUp from the end of ${name} moves the caret`, async ({
    page,
    consoleErrors,
  }) => {
    await openFixture(page, file);

    await page.keyboard.press(`${MOD}+End`);
    const end = await caretPosition(page);
    expect(end, "no caret at the document end").not.toBeNull();

    // One press. This is the whole bug: it used to change nothing.
    await page.keyboard.press("ArrowUp");
    const once = await caretPosition(page);
    expect(
      once.docY,
      `ArrowUp at the document end did nothing (still at ${end.docY})`,
    ).toBeLessThan(end.docY);

    // And it keeps working: twenty more presses keep climbing.
    let previous = once;
    for (let i = 0; i < 20; i++) {
      await page.keyboard.press("ArrowUp");
      const now = await caretPosition(page);
      expect(
        now.docY,
        `ArrowUp ${i + 2} from the end stopped moving at ${now.docY}`,
      ).toBeLessThan(previous.docY + 1);
      previous = now;
    }
    expect(previous.docY).toBeLessThan(end.docY - 100);

    expect(consoleErrors).toEqual([]);
  });
}

// The other half of the dead end — walking up from mid-document stopping at a
// paragraph between two tables, six lines from the top, with everything above
// it unreachable by keyboard — reproduced only on the owner's own document,
// which cannot be committed. `pagination-fidelity.docx` does not dead-end
// there, so a spec asserting it would have passed with the bug live and is
// deliberately not written. The fix is the same code path as the tests above,
// and it was verified against the owner's file by hand:
//
//   before: UP stuck at press 31 {"x":317,"docY":311}  (and never recovered)
//   after:  the walk continues 311 → 285 → 270 → 213 → 182 → …
//
test("the document start and end stay no-ops", async ({ page, consoleErrors }) => {
  // The fallback must not invent movement where there is none: a caret on the
  // first line has nothing above it, and one on the last line nothing below.
  await openFixture(page, PAGE_BREAKS);

  await page.keyboard.press(`${MOD}+Home`);
  const top = await caretPosition(page);
  for (let i = 0; i < 5; i++) await page.keyboard.press("ArrowUp");
  expect(await caretPosition(page)).toEqual(top);

  await page.keyboard.press(`${MOD}+End`);
  const bottom = await caretPosition(page);
  for (let i = 0; i < 5; i++) await page.keyboard.press("ArrowDown");
  expect(await caretPosition(page)).toEqual(bottom);

  expect(consoleErrors).toEqual([]);
});
