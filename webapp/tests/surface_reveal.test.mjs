// What `revealControl` has to guarantee, and the one property that was broken.
//
// `docs/159` §3: the Measurement-unit row opened Settings and left the reader
// looking at the Theme radio group. The cause was not that the focus call was
// missing — it was there, at `main.js:10517-10520`, and it was DEAD CODE,
// because `modal.mjs` defers the dialog's own initial focus with
// `queueMicrotask` and therefore always overwrote it. A synchronous focus could
// never win, so the line never had an observable effect.
//
// So the property under test is an ORDERING one: a reveal must land after the
// owning surface has finished its own deferred focus management, for any number
// of queued microtasks. `requestAnimationFrame` gives that, because microtasks
// always drain before the next animation frame. `setTimeout(…, 0)` is a
// macrotask and stands in for the frame faithfully with respect to this exact
// property, which is what lets the guarantee be checked in node at all.
//
// THE MUTATION THAT PROVES THIS CAN FAIL (`SKILL` §4): make `revealControl`
// invoke its callback synchronously — i.e. reintroduce the original bug —
//
//     export function revealControl(el, options = {}) {
//       const schedule = options.frame ?? ((callback) => requestAnimationFrame(callback));
//       schedule(() => { … });            ->   (() => { … })();
//
// and the first test below fails with
//
//     the reveal lost to the owning surface's deferred focus …
//     + actual - expected
//     + 'the surface's own initial focus'
//     - 'the control that was asked for'
//
// which is the reported defect, reproduced. The other three tests cover the
// rest of the contract: the scroll (because `modal.mjs` focuses with
// `preventScroll`, so focus alone reveals nothing), the labelled-group target,
// and tolerating a lookup that found nothing.

import assert from "node:assert/strict";
import test from "node:test";

import { revealControl } from "../src/surface_reveal.mjs";

/** A frame scheduler that, like `requestAnimationFrame`, runs after every
 *  queued microtask. Returns a promise that settles once it has run. */
function frameScheduler() {
  const ran = [];
  const frame = (callback) => {
    ran.push(new Promise((resolve) => setTimeout(() => (callback(), resolve()), 0)));
  };
  return { frame, settled: () => Promise.all(ran) };
}

/** A stand-in control. `closest` answers for one selector only, which is how
 *  the labelled-group fallback gets exercised in both directions. */
function stubControl({ groupFor = null, group = { scrolled: null } } = {}) {
  const calls = { focus: [], scrolled: null };
  const el = {
    calls,
    closest: (selector) => (selector === groupFor ? groupEl : null),
    focus: (options) => calls.focus.push(options),
    scrollIntoView: (options) => (calls.scrolled = options),
  };
  const groupEl = {
    scrollIntoView: (options) => (group.scrolled = options),
  };
  return { el, group };
}

test("the reveal lands after the owning surface's deferred focus, not before it", async () => {
  // CREATE THE CONDITION rather than assume it: a surface that, like
  // `modal.mjs`, sends focus somewhere else one microtask after being opened.
  let holder = "nothing was focused";
  const { frame, settled } = frameScheduler();
  const { el } = stubControl();
  el.focus = () => (holder = "the control that was asked for");

  queueMicrotask(() => (holder = "the surface's own initial focus"));
  revealControl(el, { frame });
  await settled();

  assert.equal(
    holder,
    "the control that was asked for",
    "the reveal lost to the owning surface's deferred focus — a synchronous focus " +
      "is overwritten by the dialog's queued `initialFocus`, which is exactly the " +
      "dead `.focus()` call docs/159 §3.1 describes",
  );
});

test("it scrolls as well as focuses, because focus alone is preventScroll", async () => {
  const { frame, settled } = frameScheduler();
  const { el } = stubControl();

  revealControl(el, { frame });
  await settled();

  assert.deepEqual(
    el.calls.scrolled,
    { block: "nearest" },
    "a focused control in a scrolling dialog body can still be off screen: " +
      "`modal.mjs` focuses with preventScroll, so the reveal must scroll itself",
  );
  assert.deepEqual(
    el.calls.focus,
    [{ preventScroll: true }],
    "having scrolled deliberately, the focus must not scroll again and fight it",
  );
});

test("it scrolls the labelled group when there is one, so the label is visible too", async () => {
  const { frame, settled } = frameScheduler();
  const { el, group } = stubControl({ groupFor: ".settings-section" });

  revealControl(el, { frame, group: ".settings-section" });
  await settled();

  assert.deepEqual(
    group.scrolled,
    { block: "nearest" },
    "the enclosing section is the target: a select scrolled to the top edge with " +
      "its caption cut off answers `where is it` with `here, unlabelled`",
  );
  assert.equal(el.calls.scrolled, null, "the control itself is not scrolled as well");
});

test("a lookup that found nothing is tolerated, not thrown", async () => {
  const { frame, settled } = frameScheduler();
  revealControl(null, { frame, group: ".settings-section" });
  await assert.doesNotReject(settled());
});
