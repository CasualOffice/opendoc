// A drag-selection costs ONE chrome reflect, not one per pointer move.
//
// `drawSelection()` does eleven things, and `updateDragSelection` was calling
// all eleven on every `pointermove`. One of them is `updateToolbar()`, which
// asks the engine three questions about the caret — and `alignmentAt`
// (`crates/casual-doc-wasm/src/lib.rs:12408`) goes through
// `paragraph_properties`, the linear document walk SKILL §8 names by name as
// "a lookup-by-id that looks like an accessor at the call site". So dragging to
// select a phrase performed three document-length scans per frame, plus the
// ribbon reflect, the review controls, the ruler sync, the floating toolbar
// reposition, and a `noteSelection` callback to any embedding host — sixty times
// a second, for a selection that had not finished moving.
//
// `docs/107` §4 is explicit that per-interaction work is O(1) in document size.
// The engine half of that is the wasm lane's; this is the host half, and it does
// not need the engine to change: Word and Google Docs both settle the ribbon
// state and the mini-toolbar when the gesture ENDS, because during a drag the
// chrome would be describing a selection that is still changing. So the frame
// now paints the highlight and nothing else, and the chrome catches up once, in
// `resetPointerGesture` — the single place every pointer gesture ends, including
// the ones that end by `pointercancel`, by `blur` or by the button being
// released outside the window.
//
// ## Why this counts MUTATIONS and not milliseconds
//
// SKILL §8: "Guard complexity, not milliseconds. A timing threshold is flaky and
// cannot tell a slow constant from a quadratic." A `MutationObserver` on the
// ribbon counts the chrome writes a gesture actually performs, so the assertion
// is about the SHAPE of the cost — does it grow with the number of pointer
// moves — and a fast machine cannot pass it by being fast.
//
// The other two tests are not optional, and neither is satisfied by the first.
// "Fewer chrome records" is also what you get by never reflecting at all, and by
// never painting at all — so one asserts that the chrome HAS settled once the
// button is released, and one that the highlight grows WHILE it is still down.
// Measured with the catch-up removed: 0 records for both drag lengths, which
// passes the scaling test and fails the settling one, which is the right
// division of labour.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

/** Counts attribute writes anywhere in the ribbon. `updateToolbar` sets
 *  `aria-pressed` on the four alignment buttons unconditionally on every call,
 *  so each call is several records whether or not any value changed — which is
 *  exactly the work being removed from the frame. */
async function armChromeCounter(page) {
  await page.evaluate(() => {
    window.__chromeWrites = 0;
    const root = document.querySelector(".ribbon");
    if (!root) throw new Error("the ribbon is not in the DOM; this guard would be vacuous");
    window.__chromeObserver?.disconnect();
    window.__chromeObserver = new MutationObserver((records) => {
      window.__chromeWrites += records.length;
    });
    window.__chromeObserver.observe(root, { attributes: true, subtree: true, childList: true });
  });
}

const chromeWrites = (page) => page.evaluate(() => window.__chromeWrites ?? -1);
const resetCounter = (page) => page.evaluate(() => void (window.__chromeWrites = 0));

const highlightCount = (page) =>
  page.evaluate(() => document.querySelectorAll(".page-wrap .overlay .highlight").length);

async function caretPoint(page, offset) {
  await moveCaretToDocStart(page);
  for (let i = 0; i < offset; i += 1) await page.keyboard.press("ArrowRight");
  let previous = null;
  let box = null;
  await expect
    .poll(async () => {
      const next = await page.locator(".overlay .caret").first().evaluate((el) => {
        const r = el.getBoundingClientRect();
        return { x: r.x, y: r.y, height: r.height };
      });
      const settled = previous !== null && Math.abs(next.x - previous.x) < 0.5;
      previous = next;
      box = next;
      return settled;
    })
    .toBe(true);
  return { x: box.x + 1, y: box.y + box.height / 2 };
}

/** A paragraph of real words, so the drag has somewhere to go. */
async function seed(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("alpha bravo charlie delta echo foxtrot golf hotel india juliet ");
  await moveCaretToDocStart(page);
  await expect(page.locator(".overlay .caret").first()).toBeVisible();
}

test("the chrome reflect does not scale with the number of pointer moves", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await seed(page);
  const from = await caretPoint(page, 2);
  const to = await caretPoint(page, 52);
  expect(
    Math.abs(to.x - from.x),
    "the two ends of the drag resolved to the same pixel",
  ).toBeGreaterThan(20);

  await armChromeCounter(page);

  async function dragWith(steps) {
    await resetCounter(page);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps });
    await page.mouse.up();
    return chromeWrites(page);
  }

  const few = await dragWith(8);
  const many = await dragWith(32);

  // Four times the pointer moves must not be four times the chrome work. The
  // bound is generous on purpose — the point is the SHAPE, and a gesture is
  // allowed a constant amount of catching up at each end — but a per-frame
  // reflect cannot hide under it. MEASURED, both ways round:
  //
  //     per-frame reflect:   8 moves -> 3,010 records   32 moves -> 10,225
  //     settled on release:  8 moves ->   305 records   32 moves ->    300
  //
  // Flat, and an order of magnitude smaller even for the shortest drag.
  //
  // `+ 1` so that ZERO chrome work also passes HERE. It is not acceptable, but it
  // is a different failure, and it belongs to the test below rather than being
  // reported under this one's sentence.
  expect(
    many,
    `8 pointer moves wrote ${few} chrome records and 32 wrote ${many} — ` +
      `the reflect is still happening per frame`,
  ).toBeLessThan(few * 2 + 1);

  expect(consoleErrors).toEqual([]);
});

test("the chrome a drag deferred has settled by the time the button is released", async ({
  page,
  consoleErrors,
}) => {
  // The other half of the first test, and the one it CANNOT be satisfied without:
  // "fewer chrome records" is also what you get by never reflecting at all. So
  // this asserts the settle, through the most visible thing `drawSelection` does
  // that a drag defers — the floating selection toolbar, which Word and Docs both
  // raise on release rather than during the gesture.
  await gotoEditor(page);
  await seed(page);
  const from = await caretPoint(page, 2);
  const to = await caretPoint(page, 52);

  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await page.mouse.up();

  await expect(
    page.locator("#selToolbar"),
    "a drag-selection left the chrome describing no selection at all",
  ).toBeVisible();
  // And the ribbon agrees that there is a range: the run controls are live.
  await expect(page.locator("#bold")).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

test("a bare click settles the chrome too, not only a drag", async ({
  page,
  consoleErrors,
}) => {
  // The case the first version of this change got WRONG, and the reason the
  // settle is not conditional on having moved. A plain click raises a gesture
  // too: `dragging` goes true on pointer-down, the `drawSelection` inside
  // `onPointerDown` therefore defers the chrome, and nothing settled it because
  // the gesture never moved. So clicking away from a selection dropped the
  // selection and left every control still describing it —
  // `table-structure-gestures.spec.mjs:354` caught it with Merge cells still
  // enabled over no cell selection at all, which is a dead control pointing at
  // nothing.
  await gotoEditor(page);
  await seed(page);
  const from = await caretPoint(page, 2);
  const to = await caretPoint(page, 52);

  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 10 });
  await page.mouse.up();
  await expect(page.locator("#selToolbar")).toBeVisible();

  // One click, no movement. The selection is gone, so the chrome that describes
  // one must be gone with it.
  await page.mouse.click(from.x, from.y);
  await expect(
    page.locator("#selToolbar"),
    "a click collapsed the selection and the chrome went on describing it",
  ).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("the highlight still grows while the button is held down", async ({
  page,
  consoleErrors,
}) => {
  // The precondition for the test above, and the reason it cannot be satisfied
  // by simply painting less: a reader watches the highlight follow the pointer.
  await gotoEditor(page);
  await seed(page);
  const from = await caretPoint(page, 2);
  const to = await caretPoint(page, 52);

  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  expect(await highlightCount(page), "a press alone selects nothing").toBe(0);

  await page.mouse.move(to.x, to.y, { steps: 12 });
  // Still DOWN. The highlight has to be on screen now, not after the release.
  const during = await highlightCount(page);
  await page.mouse.up();

  expect(
    during,
    "no highlight was painted while the drag was in progress",
  ).toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});
