// Paper carries the whole document, whatever the reader has folded away.
//
// `withPagedLayout` is the one wrapper both print entry points and the PDF
// handoff go through, and ADR-049 put two rules in it: paper is paper (the
// reflow view is forced to `Paged`), and **print, PDF and DOCX export are always
// fully expanded**. The second one was specified and not built, so a print while
// a heading was collapsed produced sheets — and, through "Print to PDF", FILES —
// with whole sections missing, nothing in the output to say so, and no message.
// That is the silent-data-loss class `AGENTS.md` forbids outright, and it is
// exactly the failure a user cannot detect: the pages that are there look right.
//
// These guards are over the WRAPPER rather than over a printer, because the
// wrapper is where the guarantee lives. A guard on `printViaRaster` would be
// pinned to today's circumstance — that the raster fallback is the path which
// actually read the folded layout, while the real-text PDF writer paginates from
// the model and was expanded by accident. The guarantee is "whatever runs inside
// this wrapper sees an expanded document", and that is what is asserted.
//
// Buildless: `withPagedLayout` touches no DOM, so `node --test` can drive it
// against a fake engine. The fake answers the four calls the wrapper makes and
// records them in order, so "restored" can be distinguished from "restored
// twice" and from "restored before the build finished".
import { test } from "node:test";
import assert from "node:assert/strict";
import { withPagedLayout } from "../src/print.mjs";

/** An engine fake: a fold set, an optional reflow view, and a call log. */
function fakeDoc({ folded = [], reflow = null, foldSeam = true } = {}) {
  const calls = [];
  let current = [...folded];
  return {
    calls,
    foldedNow: () => current,
    layoutView: JSON.stringify(reflow ?? { reflow: false }),
    setLayoutView(...args) {
      calls.push(`setLayoutView(${args.join(",")})`);
    },
    foldState() {
      if (!foldSeam) throw new Error("this engine has no fold seam");
      return JSON.stringify({ folded: current, available: true, withheldReason: "" });
    },
    unfoldAll() {
      calls.push("unfoldAll");
      current = [];
    },
    setFoldSet(json) {
      calls.push(`setFoldSet(${json})`);
      current = JSON.parse(json);
    },
    // Implemented although the wrapper must not use it, so that a restore
    // written one heading at a time FAILS on the call count rather than on a
    // missing method. A mutation that reddens a guard by breaking the fake has
    // proved the fake, not the guard (`SKILL.md` §4).
    setFold(node, collapsed) {
      calls.push(`setFold(${node},${collapsed})`);
      current = collapsed ? [...current, node] : current.filter((n) => n !== node);
    },
  };
}

// ---------------------------------------------------------------------------
// The rule itself.
//
// Mutation that reddens it: delete `const refold = expandFolds(doc);` and the
// `refold()` in the `finally` from `withPagedLayout`. The build then observes
// `["h1","h4"]` still folded, which is the short PDF.
test("what runs inside the print wrapper sees a fully expanded document", async () => {
  const doc = fakeDoc({ folded: ["h1", "h4"] });
  let seenDuringBuild = null;
  await withPagedLayout(doc, () => {
    seenDuringBuild = JSON.parse(doc.foldState()).folded;
  });
  assert.deepEqual(
    seenDuringBuild,
    [],
    "a printer must see no collapsed heading: whatever is folded on screen is still " +
      "part of the document, and paper that omits it says nothing about the omission",
  );
  assert.deepEqual(
    doc.foldedNow(),
    ["h1", "h4"],
    "and the reader's own folds are back afterwards — printing is not an edit and must " +
      "not leave the view it borrowed",
  );
});

// ---------------------------------------------------------------------------
// Restoring is O(1) in the number of folded headings, not O(folded).
//
// Guarded by DOUBLING rather than by a millisecond budget (`SKILL.md` §8): a
// timing threshold cannot tell a slow constant from a linear walk, and the thing
// that kills a real document is the walk. Each `setFold` costs its own
// `O(document)` re-layout, so restoring one heading at a time would make a
// reader who collapsed two hundred headings pay two hundred full re-layouts to
// get them back — after the print dialog closed, with no progress and no way to
// cancel.
//
// Mutation that reddens it: restore with `for (const n of folded)
// doc.setFold(n, true)` instead of one `setFoldSet`. The call count then
// doubles with the fold count — 2 against 4 — which is what this measures.
test("the restore costs one engine call however many headings were folded", async () => {
  const count = async (n) => {
    const doc = fakeDoc({ folded: Array.from({ length: n }, (_, i) => `h${i}`) });
    await withPagedLayout(doc, () => {});
    return doc.calls.filter((c) => c.startsWith("setFoldSet") || c.startsWith("setFold(")).length;
  };
  const two = await count(2);
  const four = await count(4);
  assert.equal(two, 1, `restoring two folds must be one call, not ${two}`);
  assert.equal(
    four,
    two,
    `restoring twice as many folds must cost the same ${two} call(s), not ${four} — a ` +
      "per-heading restore is a per-heading full re-layout",
  );
});

// ---------------------------------------------------------------------------
// Nothing folded, nothing paid.
//
// Mutation that reddens it: drop the `if (!folded.length) return () => {}` early
// exit. Every print of every unfolded document then pays an `unfoldAll` and a
// `setFoldSet`, each an `O(document)` re-layout, for no change at all.
test("printing a document with nothing folded makes no fold call", async () => {
  const doc = fakeDoc({ folded: [] });
  await withPagedLayout(doc, () => {});
  assert.deepEqual(
    doc.calls,
    [],
    "the common case must be free: an unfolded document has nothing to expand and a " +
      "re-layout in each direction is the whole document's worth of work for nothing",
  );
});

// ---------------------------------------------------------------------------
// The restore is in the `finally`, and it runs before the view is put back.
//
// A printer error, a dismissed dialog, an engine that throws mid-render: every
// one of those must leave the reader where they were. This is the half that
// `withPagedLayout` already got right for the layout view and that a second rule
// in the same wrapper could easily have got wrong.
//
// Mutation that reddens it: move `refold()` out of the `finally` and onto the
// line after `await build()`. The rejection then skips it and the reader is left
// with an expanded document they did not ask for.
test("a printer that throws still gives the reader their folds back", async () => {
  const doc = fakeDoc({ folded: ["h2"], reflow: { reflow: true, contentWidthTwip: 9000, tileHeightTwip: 15840, gutterTwip: 240 } });
  await assert.rejects(
    () => withPagedLayout(doc, () => Promise.reject(new Error("the printer said no"))),
    /the printer said no/,
    "the error must reach the caller — a print that failed silently is worse than one " +
      "that said so",
  );
  assert.deepEqual(doc.foldedNow(), ["h2"], "and the fold is back");
  assert.deepEqual(
    doc.calls,
    ["setLayoutView(0,0,0)", "unfoldAll", 'setFoldSet(["h2"])', "setLayoutView(9000,15840,240)"],
    "in this order: paper first, then expand, then — on the way out — the folds and the " +
      "reader's own reflow view. Both restores, both after the failure",
  );
});

// ---------------------------------------------------------------------------
// An engine with no fold seam prints what it has.
//
// The asymmetry with the layout-view arm is deliberate and worth a guard: a
// missing fold seam means the output MAY be short, and the only honest response
// is to leave the document alone rather than half-restore it. What must not
// happen is that the print is abandoned.
//
// Mutation that reddens it: let `expandFolds` propagate instead of returning a
// no-op. `printDocument` then throws on any engine without `foldState`, so an
// older or embedded build loses printing entirely.
test("an engine with no fold seam still prints, and nothing is restored", async () => {
  const doc = fakeDoc({ folded: ["h1"], foldSeam: false });
  let ran = false;
  await withPagedLayout(doc, () => {
    ran = true;
  });
  assert.ok(ran, "the build must still run: no fold seam is not a reason not to print");
  assert.deepEqual(doc.calls, [], "and nothing is forced or restored on an engine that refused");
});
