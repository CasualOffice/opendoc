// Touch selection on a phone (`docs/105` UX-018, `docs/148` §9 item 2).
//
// The arithmetic is driven from node in `tests/touch_selection.test.mjs`. What
// is here is the half node cannot answer: that a finger held on a word actually
// selects it, that the handles are big enough to hit, that dragging one changes
// what is selected, that the loupe appears and stays inside the window, and —
// the one that is easiest to lose — that the page still SCROLLS.
//
// REAL touch events, through CDP. Synthetic `PointerEvent`s dispatched from
// page script never reach the browser's own scroll decision, so the scroll
// guard below would pass with the fix removed — the trap
// `object-alignment-guides.spec.mjs` already records. `Input.dispatchTouchEvent`
// goes in ahead of that decision, which is the only place `touch-action` and a
// cancelled `pointerdown` mean anything.
//
// THE SCROLL GUARD OWNS ITS OWN PRECONDITIONS, because it used to borrow both of
// them from the paper layout and reflow took them away. It needs (1) a document
// taller than the window and (2) a press point whose sweep path crosses other
// text; `gotoScrollableDocument` and `crossingPressPoint` create each one and
// assert it, so a future layout change fails as a stated precondition rather
// than as a guarantee that cannot be reached.
import { test, expect, closeAppMenu, gotoEditor, menuCommandRow, stableBox } from "./fixtures.mjs";
import { MIN_TOUCH_TARGET_PX } from "../../src/phone_chrome.mjs";
import { LONG_PRESS_MS, MOVE_MIN_DIST_PX } from "../../src/touch_selection.mjs";

// A phone, with a finger. `hasTouch` is what makes Chromium deliver the CDP
// touch stream as pointer events with `pointerType === "touch"`, which is the
// only condition the module arms on.
test.use({ hasTouch: true, viewport: { width: 390, height: 844 } });

/** A finger, driven through the browser's own touch pipeline. */
async function finger(page) {
  const cdp = await page.context().newCDPSession(page);
  const send = (type, x, y) =>
    cdp.send("Input.dispatchTouchEvent", {
      type,
      touchPoints: type === "touchEnd" ? [] : [{ x, y, id: 1 }],
    });
  return {
    down: (x, y) => send("touchStart", x, y),
    move: (x, y) => send("touchMove", x, y),
    up: (x, y) => send("touchEnd", x, y),
    /** A straight drag in `steps` touchmoves, so the browser sees a gesture
     *  rather than a teleport. `pause` makes the whole gesture take real time —
     *  a finger scrolling a page is not instantaneous, and a gesture that
     *  finishes in 20ms can never reach the 750ms press timer, which is exactly
     *  the collision the slop exists to prevent. */
    async sweep(fromX, fromY, toX, toY, steps = 10, pause = 0) {
      for (let i = 1; i <= steps; i += 1) {
        await send("touchMove", fromX + ((toX - fromX) * i) / steps, fromY + ((toY - fromY) * i) / steps);
        if (pause) await page.waitForTimeout(pause);
      }
    },
  };
}

/** What the highlight rectangles cover, in client coordinates, or null when
 *  nothing is selected. This is how "what is selected" is observed without
 *  `main.js` exporting anything (it exports nothing — HF-109).
 *
 *  `span` is the total painted WIDTH summed over the rectangles, not the
 *  bounding box: a selection that grows onto a second line is wider in text and
 *  no wider in bounding box, so a bounding-box measure would call a working
 *  multi-line extension a failure. */
const highlight = (page) =>
  page.evaluate(() => {
    const rects = [...document.querySelectorAll(".overlay .highlight")];
    if (!rects.length) return null;
    let left = Infinity;
    let right = -Infinity;
    let span = 0;
    for (const el of rects) {
      const box = el.getBoundingClientRect();
      left = Math.min(left, box.left);
      right = Math.max(right, box.right);
      span += box.width;
    }
    return { left, right, width: right - left, span, count: rects.length };
  });

/** Collapses the selection by clicking where it already is.
 *
 *  Deliberately not `ArrowRight`: a caret move scrolls the caret into view, and
 *  a scroll of even a line would move the text out from under the coordinates
 *  every assertion below is written against. A click places a caret where the
 *  pointer already is, so nothing moves. */
async function clearSelection(page, at) {
  await page.mouse.click(at.x, at.y);
  await expect.poll(() => highlight(page)).toBeNull();
}

/** A point on the first page that a double click selects a word at.
 *
 *  Derived rather than guessed: the demo document's exact line positions are
 *  not this spec's business, and a hard-coded offset that lands in a margin
 *  makes every assertion below vacuous — which is the shape of green-but-wrong
 *  this repository has shipped before (`SKILL` §4).
 */
async function wordPoint(page) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  for (const fy of [0.1, 0.13, 0.16, 0.2, 0.25, 0.3]) {
    for (const fx of [0.3, 0.45, 0.6]) {
      const x = box.x + box.width * fx;
      const y = box.y + box.height * fy;
      await page.mouse.dblclick(x, y);
      const span = await highlight(page);
      if (span && span.width > 4) {
        await clearSelection(page, { x, y });
        return { x, y, width: span.width };
      }
    }
  }
  throw new Error("found no point on the first page where a double click selects a word");
}

/** The word a double click selects at a point, then collapsed again — or null
 *  where there is no word. The probe `wordPoint` runs, asked at one point. */
async function wordAt(page, x, y) {
  await page.mouse.dblclick(x, y);
  const span = await highlight(page);
  await clearSelection(page, { x, y });
  return span && span.width > 4 ? span : null;
}

test("a finger held on a word selects it — the same word a double click selects", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const at = await wordPoint(page);
  expect(await highlight(page)).toBeNull(); // the probe left nothing behind

  const touch = await finger(page);
  await touch.down(at.x, at.y);
  await page.waitForTimeout(LONG_PRESS_MS + 250);

  // THE GUARANTEE: the long press and the double click reach the same word,
  // because they are the same routine (`touch_selection.mjs`'s `selectWord`).
  const held = await highlight(page);
  expect(held).not.toBeNull();
  expect(held.width).toBeGreaterThan(4);
  expect(Math.abs(held.width - at.width)).toBeLessThan(2);

  await touch.up(at.x, at.y);
  expect(consoleErrors).toEqual([]);
});

test("the two handles are hittable targets, and they bracket the selection", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const at = await wordPoint(page);
  const touch = await finger(page);
  await touch.down(at.x, at.y);
  await page.waitForTimeout(LONG_PRESS_MS + 250);

  const handles = page.locator(".overlay .touch-handle");
  await expect(handles).toHaveCount(2);
  for (const which of ["start", "end"]) {
    const box = await stableBox(page.locator(`.overlay .touch-handle.is-${which}`));
    expect(box.width).toBeGreaterThanOrEqual(MIN_TOUCH_TARGET_PX);
    expect(box.height).toBeGreaterThanOrEqual(MIN_TOUCH_TARGET_PX);
  }

  // The start handle is at the start of the selection and the end handle at its
  // end — not two dots in the same place, which is what a broken ordering looks
  // like and what nothing else here would notice.
  const span = await highlight(page);
  const start = await stableBox(page.locator(".overlay .touch-handle.is-start"));
  const end = await stableBox(page.locator(".overlay .touch-handle.is-end"));
  expect(Math.abs(start.x + start.width / 2 - span.left)).toBeLessThan(8);
  expect(Math.abs(end.x + end.width / 2 - span.right)).toBeLessThan(8);

  // Named, not decorative: on a touchscreen a screen reader explores by touch,
  // and these are the only visible selection affordances there.
  await expect(page.locator(".overlay .touch-handle.is-start")).toHaveAttribute("aria-label", /.+/);
  await expect(page.locator(".overlay .touch-handle.is-end")).toHaveAttribute("aria-label", /.+/);

  // …and REACHABLE. The floating format bar appears over the same selection and
  // is a fixed element above the page, so an overlap is not merely untidy: the
  // bar takes the touch and the handle under it cannot be grabbed at all.
  // FOUND BY LOOKING at a screenshot — the bar cut the start dot in half, and
  // nothing clickable could fail.
  const bar = await stableBox(page.locator("#selToolbar"));
  for (const box of [start, end]) {
    const overlaps =
      box.x < bar.x + bar.width &&
      bar.x < box.x + box.width &&
      box.y < bar.y + bar.height &&
      bar.y < box.y + box.height;
    expect(overlaps, "the floating format bar covers part of a selection handle").toBe(false);
  }

  await touch.up(at.x, at.y);
  expect(consoleErrors).toEqual([]);
});

test("dragging the end handle extends the selection, and the loupe stays inside the window", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const at = await wordPoint(page);
  const touch = await finger(page);
  await touch.down(at.x, at.y);
  await page.waitForTimeout(LONG_PRESS_MS + 250);
  await touch.up(at.x, at.y);

  const before = await highlight(page);
  const end = await stableBox(page.locator(".overlay .touch-handle.is-end"));
  const grabX = end.x + end.width / 2;
  const grabY = end.y + end.height / 2;

  // DOWN and to the right, not straight right. The word the probe found may be
  // the last on its line — the demo's headings are five characters long — and a
  // hit test cannot walk off the end of a line into the next paragraph, so a
  // purely horizontal drag can legitimately select nothing new. Down a line is
  // the gesture that must always have somewhere to go.
  const toX = grabX + 40;
  const toY = grabY + 60;
  await touch.down(grabX, grabY);
  await touch.sweep(grabX, grabY, toX, toY);

  // The loupe is up WHILE the drag is in flight, which is the only time it is
  // any use, and it is inside the window — nothing may paint outside it
  // (`docs/148` §6's sibling rule).
  const glass = page.locator(".touch-magnifier");
  await expect(glass).toBeVisible();
  const lens = await stableBox(glass);
  const view = await page.evaluate(() => ({ w: window.innerWidth, h: window.innerHeight }));
  expect(lens.x).toBeGreaterThanOrEqual(0);
  expect(lens.y).toBeGreaterThanOrEqual(0);
  expect(lens.x + lens.width).toBeLessThanOrEqual(view.w);
  expect(lens.y + lens.height).toBeLessThanOrEqual(view.h);

  await touch.up(toX, toY);

  // THE GUARANTEE: more text is selected than before, and the selection did not
  // collapse on the way.
  const after = await highlight(page);
  expect(after).not.toBeNull();
  expect(after.span).toBeGreaterThan(before.span + 10);

  // The loupe goes away with the finger. A magnifier left behind is a 100px
  // hole in the document.
  await expect(glass).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

/** A document taller than the window, opened on purpose.
 *
 *  THE SCROLL PRECONDITION IS NOW PART OF THE TEST, because it used to be
 *  inherited by accident. `?fixture=rich` is one page of text; under the paper
 *  layout that page was 11 inches of sheet whatever was written on it, so
 *  `#viewport` had ~2,000px of scroll range that nothing in this file had asked
 *  for. Reflow (`docs/151`, ADR-046) trims a tile to its content, and at 390px
 *  that fixture measures `scrollHeight 751` into `clientHeight 751` — nothing to
 *  scroll, so the guard below failed with `Received: 0` while the product was
 *  scrolling perfectly well. It was measuring the fixture, not the guarantee.
 *
 *  `sample.docx` — the document the editor opens when no fixture is named, and
 *  the one a visitor actually sees — is genuinely long: 9,794px into 751px in
 *  reflow at this width. `scrollRoom` asserts that afresh, so if the sample ever
 *  shrinks this fails as a stated PRECONDITION with its own message instead of
 *  as a mysteriously immobile scroller. */
async function gotoScrollableDocument(page) {
  await page.goto("/editor.html");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length > 0 &&
        document.body.dataset.fontsReady === "true"
      );
    },
    null,
    { timeout: 45_000 },
  );
}

/** How much further `#viewport` can scroll down, in CSS pixels. Polled: reflow
 *  re-lays the document out on a debounced width feed, so the band's height is
 *  not final the instant the first sheet appears. */
const scrollRoom = async (page) => {
  let room = 0;
  await expect
    .poll(async () => {
      room = await page.evaluate(() => {
        const v = document.getElementById("viewport");
        return v.scrollHeight - v.clientHeight - v.scrollTop;
      });
      return room;
    })
    .toBeGreaterThan(0);
  return room;
};

/** The Edit ▸ Copy row, which is enabled by `hasRange()` and nothing else.
 *
 *  Here because the painted highlight is NOT a sufficient witness that nothing
 *  is selected. When this guard was red, the finger had built a two-node range
 *  for which `selectionRects` returned no rectangles at all, so `paintSelection`
 *  fell through to painting a caret: zero `.highlight` elements over a document
 *  that genuinely had a selection. Copy asks the model. */
const copyRow = (page) => menuCommandRow(page, "edit", "edit.copy");

/** A press point whose sweep path is known to cross OTHER text, within the
 *  finger's FIRST step.
 *
 *  This is the half of the precondition that decides whether the guard below can
 *  fail at all, and it was missing. A drag-selection can only be observed if the
 *  path the finger takes hit-tests to a different place than its start; pressed
 *  in the middle of a line, a purely vertical 13px step resolves to the same
 *  character offset, so a machine that wrongly drag-selects under a finger has
 *  nothing to show and the guard passes while the defect is live. MEASURED on
 *  `sample.docx` at 390px with the fix removed: at two of the three points the
 *  old probe found first, the sweep produced no selection; at the third it
 *  produced two handles and a 284px-wide highlight. A guard whose verdict
 *  depends on which line the probe happened to land on is not a guard.
 *
 *  So the point is chosen by asking: does a double click one step up the path
 *  select a DIFFERENT word? Only then is the gesture able to expose the defect,
 *  and the loop throws rather than proceeding if no such point exists. */
async function crossingPressPoint(page, step) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  for (const fy of [0.3, 0.35, 0.25, 0.4, 0.2, 0.45, 0.15, 0.5]) {
    for (const fx of [0.45, 0.3, 0.6]) {
      const x = box.x + box.width * fx;
      const y = box.y + box.height * fy;
      const here = await wordAt(page, x, y);
      if (!here) continue;
      const crossed = await wordAt(page, x, y - step);
      if (!crossed) continue;
      if (Math.abs(crossed.left - here.left) < 2 && Math.abs(crossed.right - here.right) < 2) {
        continue; // the same word: one step up this path changes nothing
      }
      return { x, y };
    }
  }
  throw new Error(
    "found no press point whose first sweep step crosses a different word — " +
      "without one this test cannot tell a working scroll from a scroll that selects",
  );
}

test("a drag SCROLLS the document and sweeps no selection out behind the finger", async ({
  page,
  consoleErrors,
}) => {
  await gotoScrollableDocument(page);

  // Past the slop, and — the part that matters — taking LONGER than the press
  // timer, which is what a real finger scrolling a page does. The slop is what
  // keeps the two gestures apart: without it the press fires in the middle of
  // the scroll and a word is selected out from under a moving finger.
  const travel = MOVE_MIN_DIST_PX + 140;
  // Six steps, not twelve: Chromium serves only one or two `pointermove`s before
  // it takes a touch for a pan, so the FIRST step has to be the one that crosses
  // other text or the guard is relying on a second one arriving. Each step is
  // therefore 26.7px — also past the module's 20px slop, so the press is
  // correctly cancelled on the first move too.
  const steps = 6;
  // …and the whole gesture still outlasts the 750ms press timer (1,128ms), which
  // is the collision this test's title is about.
  const pause = Math.ceil((LONG_PRESS_MS * 1.5) / steps);
  expect(
    await scrollRoom(page),
    "this test needs a document with more scroll room than the gesture travels",
  ).toBeGreaterThan(travel);

  const at = await crossingPressPoint(page, travel / steps);

  // THE OBSERVABLE, PROVED LIVE BEFORE IT IS USED AS EVIDENCE. A "Copy is
  // disabled" assertion proves nothing until it has been seen to be enabled —
  // this repository has shipped guards that could not fail (`SKILL.md` §4).
  await page.mouse.dblclick(at.x, at.y);
  await expect(await copyRow(page), "Copy is enabled while a word is selected").toBeEnabled();
  await closeAppMenu(page);
  await clearSelection(page, at);
  await expect(await copyRow(page), "…and disabled once it is not").toBeDisabled();
  await closeAppMenu(page);

  const scrollBefore = await page.evaluate(() => document.getElementById("viewport").scrollTop);
  const touch = await finger(page);
  await touch.down(at.x, at.y);
  await touch.sweep(at.x, at.y, at.x, at.y - travel, steps, pause);
  await touch.up(at.x, at.y - travel);
  await page.waitForTimeout(400);

  // THE GUARANTEE, both halves. The view moved…
  const scrollAfter = await page.evaluate(() => document.getElementById("viewport").scrollTop);
  expect(scrollAfter).toBeGreaterThan(scrollBefore);
  // …and the scroll selected nothing. Three surfaces, because the first two are
  // paint and the finger's defect was invisible in paint: no highlight, no
  // handles, and — asking the model rather than the glass — nothing to copy.
  expect(await highlight(page)).toBeNull();
  await expect(page.locator(".overlay .touch-handle")).toHaveCount(0);
  await expect(await copyRow(page), "a scroll left a selection behind").toBeDisabled();
  await closeAppMenu(page);
  expect(consoleErrors).toEqual([]);
});

test("a mouse selection gets no finger handles", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const at = await wordPoint(page);
  await page.mouse.dblclick(at.x, at.y);
  expect(await highlight(page)).not.toBeNull();
  // The handles are a finger's chrome. On a hybrid device — a touchscreen
  // laptop, a phone with a mouse — the pointer that made the selection decides,
  // not the device class (`touch_selection.mjs`, "Where it arms").
  await expect(page.locator(".overlay .touch-handle")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});
