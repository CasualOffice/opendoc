// The View band's zoom group: the markup and the action table must agree.
//
// The group is declared in two places — `data-zoom-action` attributes in
// `editor.html` and the `run` table in `view_zoom.mjs` — and the failure mode of
// any such pair is that one gains a row and the other does not. A button with no
// action is a dead control (the UI floor forbids shipping one), and an action
// with no button is code nothing can reach. Both directions are asserted here,
// against the real markup, so neither can drift.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  ZOOM_ACTIONS,
  ZOOM_STEPS,
  fitZoomFactor,
  nextZoomStep,
  parseZoomInput,
  zoomActionActive,
} from "../src/view_zoom.mjs";

const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");

/** The View panel's markup only — a `data-zoom-action` elsewhere in the file is
 *  not this group's business and must not silently satisfy the check. */
function viewPanel() {
  const start = html.indexOf('id="panelView"');
  assert.ok(start > 0, "the View panel must exist in editor.html");
  const end = html.indexOf('id="panelReview"', start);
  assert.ok(end > start, "the Review panel must follow the View panel");
  return html.slice(start, end);
}

test("every zoom button in the markup has an action the module knows", () => {
  const declared = [...viewPanel().matchAll(/data-zoom-action="([a-z-]+)"/g)].map((m) => m[1]);
  assert.ok(declared.length >= 5, `expected the whole zoom group, saw ${declared.length}`);
  for (const action of declared) {
    assert.ok(
      ZOOM_ACTIONS.includes(action),
      `editor.html declares data-zoom-action="${action}", which view_zoom.mjs cannot run — it would ship disabled`,
    );
  }
});

test("every action the module knows has a button in the markup", () => {
  const declared = new Set(
    [...viewPanel().matchAll(/data-zoom-action="([a-z-]+)"/g)].map((m) => m[1]),
  );
  for (const action of ZOOM_ACTIONS) {
    assert.ok(
      declared.has(action),
      `view_zoom.mjs can run "${action}" but no View-band button asks for it — unreachable from the ribbon`,
    );
  }
});

test("the zoom buttons are not duplicated", () => {
  const declared = [...viewPanel().matchAll(/data-zoom-action="([a-z-]+)"/g)].map((m) => m[1]);
  assert.equal(new Set(declared).size, declared.length, "two buttons claim the same zoom action");
});

test("only one zoom state reads as active at a time", () => {
  // Fit width, Fit page and Actual size are mutually exclusive, and the two
  // steppers are verbs that are never pressed. Two buttons claiming the state at
  // once is how a toolbar starts lying about what the document is doing.
  const states = [
    { mode: "fit-width", factor: 1 },
    { mode: "fit-page", factor: 0.6 },
    { mode: "custom", factor: 1 },
    { mode: "custom", factor: 1.5 },
  ];
  for (const state of states) {
    const active = ZOOM_ACTIONS.filter((a) => zoomActionActive(a, state));
    assert.ok(active.length <= 1, `${JSON.stringify(state)} lit ${active.join(" + ")}`);
  }
  assert.deepEqual(ZOOM_ACTIONS.filter((a) => zoomActionActive(a, { mode: "fit-width", factor: 1 })), ["fit-width"]);
  assert.deepEqual(ZOOM_ACTIONS.filter((a) => zoomActionActive(a, { mode: "custom", factor: 1 })), ["actual"]);
  assert.deepEqual(ZOOM_ACTIONS.filter((a) => zoomActionActive(a, { mode: "custom", factor: 1.5 })), []);
});

test("a fit mode that lands on 100% is not Actual size", () => {
  // `computeFitZoom` can return exactly 1 for a window that happens to match the
  // page. The document is still in Fit width — it will re-zoom on the next
  // resize — so Actual size must not also light up and claim it.
  assert.equal(zoomActionActive("actual", { mode: "fit-width", factor: 1 }), false);
  assert.equal(zoomActionActive("fit-width", { mode: "fit-width", factor: 1 }), true);
});

test("the steppers never report a pressed state", () => {
  for (const state of [{ mode: "custom", factor: 1 }, { mode: "fit-page", factor: 0.5 }]) {
    assert.equal(zoomActionActive("in", state), false);
    assert.equal(zoomActionActive("out", state), false);
  }
});

// ---- The arithmetic that came out of `main.js` -------------------------------
//
// Extracted by the reflow round (`docs/151` §6), which needed lines in a file at
// its ratchet. It is worth more than the lines: a ladder, a fit and a parse are
// three things a browser was previously the only way to ask about.

test("stepping walks the ladder and does not stall at either end", () => {
  const clamp = (z) => Math.min(5, Math.max(0.1, z));
  assert.equal(nextZoomStep(1, 1, clamp), 1.25);
  assert.equal(nextZoomStep(1, -1, clamp), 0.9);
  // A zoom BETWEEN two rungs steps to the next rung, not by a fixed amount —
  // which is what makes the ladder a ladder rather than a list of presets.
  assert.equal(nextZoomStep(1.1, 1, clamp), 1.25);
  assert.equal(nextZoomStep(1.1, -1, clamp), 1);
  // Past the ends it keeps moving by a tenth rather than sticking, and the
  // caller's clamp is what stops it. A stepper that stops responding at the top
  // of the ladder is a dead control at the one moment someone is pressing it.
  const top = ZOOM_STEPS[ZOOM_STEPS.length - 1];
  assert.ok(nextZoomStep(top, 1, clamp) > top);
  assert.ok(nextZoomStep(ZOOM_STEPS[0], -1, clamp) < ZOOM_STEPS[0]);
});

test("fit-width fits the width, and fit-page never exceeds it", () => {
  const units = { dpi: 96, twipsPerInch: 1440 };
  const letter = { widthTwip: 12240, heightTwip: 15840 }; // 8.5in x 11in
  const viewport = { width: 880, height: 600 };
  const width = fitZoomFactor("fit-width", letter, viewport, units);
  const page = fitZoomFactor("fit-page", letter, viewport, units);
  // (880 - 64) px of desk over (8.5in * 96dpi) = 816 px of paper: exactly 100%,
  // which is the one factor a wrong gutter or a wrong dpi cannot also produce.
  assert.ok(Math.abs(width - 1) < 1e-9, `fit-width gave ${width}`);
  assert.ok(page < width, "fit-page must also satisfy the height, so it is never larger");
});

test("a viewport with no size yet produces a usable factor rather than zero", () => {
  // Otherwise the clamp turns "not laid out yet" into "the user chose 10%".
  const factor = fitZoomFactor(
    "fit-page",
    { widthTwip: 12240, heightTwip: 15840 },
    { width: 0, height: 0 },
    { dpi: 96, twipsPerInch: 1440 },
  );
  assert.ok(factor > 0);
});

test("the typed zoom value is read as a mode, a factor, or neither", () => {
  assert.deepEqual(parseZoomInput("fit width"), { mode: "fit-width" });
  assert.deepEqual(parseZoomInput("  FIT Page "), { mode: "fit-page" });
  assert.deepEqual(parseZoomInput("width"), { mode: "fit-width" });
  assert.deepEqual(parseZoomInput("150%"), { factor: 1.5 });
  assert.deepEqual(parseZoomInput("150"), { factor: 1.5 });
  // Neither, so the caller restores the last valid display instead of guessing.
  // A zero or a negative is nonsense rather than a small zoom.
  for (const bad of ["", "abc", "0", "-50%", null, undefined]) {
    assert.deepEqual(parseZoomInput(bad), {}, `${String(bad)} should be rejected`);
  }
});
