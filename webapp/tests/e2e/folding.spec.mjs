// FOLDING, ASSERTED WHERE A READER WOULD SEE IT (ADR-049, `109` FOLD-001/FOLD-004).
//
// The engine tier and the chrome were both built and the chrome was imported by
// nothing, so every layer of this feature was green while no user could fold a
// heading — `SKILL.md` §9 rule 4, the most expensive recurring pattern in this
// repository. Nothing below asks the engine a question: each claim is read off
// the status bar, the outline tree, the accessibility mirror or the bytes Print
// handed the browser, because a model-tier assertion has passed through every
// real defect here.
//
// TWO AFFORDANCES, TWO SHAPES, and the distinction is the design rather than an
// accident — it is Word's. The outline panel's disclosure is a TREE disclosure:
// it appears on a heading that has a deeper heading under it, which is what
// ARIA's `aria-expanded` describes and what Word's Navigation pane shows. A
// heading whose only content is body text is a LEAF in that tree and carries no
// triangle, in Word either — the affordance for it is the one at the caret
// (Word's in-body chevron, and `view.fold.toggle`). Both drive the same
// `FoldSet`, and both are exercised below, because testing only the tree
// disclosure would leave the common case — a flat document of sibling headings —
// entirely uncovered.
//
// The four claims:
//
//   1. THE TREE DISCLOSURE FOLDS, AND PAGES GO AWAY. Reflow, not blanking: the
//      page total in the status bar falls. And the folded text leaves the
//      ACCESSIBILITY MIRROR, because a fold honoured on the canvas and ignored
//      by a screen reader is a lie to exactly the reader who cannot check it.
//      The collapsed heading itself stays — it is the affordance.
//
//   2. A LEAF HEADING FOLDS FROM THE CARET. The other surface, and the one that
//      covers a document with no nested headings at all.
//
//   3. COLLAPSE ALL AND EXPAND ALL ARE REACHABLE FROM THE PALETTE, and Expand
//      All returns the document to the page count it opened with. Two surfaces
//      for one state (`SKILL.md` §10), and the round trip is the "a fold is a
//      view, never an edit" half: landing anywhere else would mean something on
//      the way through had written to the document.
//
//   4. PRINTING WHILE FOLDED PRINTS THE WHOLE DOCUMENT. Asserted on the real
//      PDF bytes: the printed page count is the UNFOLDED one. A short PDF with
//      nothing in it to say a section was dropped is the silent-data-loss class
//      `AGENTS.md` forbids outright, and it is undetectable by the person who
//      produced it.
import { test, expect, gotoEditor, documentPageCount, runPaletteCommand } from "./fixtures.mjs";
import { tocDocx, TOC_HEADINGS } from "./toc-docx.mjs";

/** Enough filler under each heading that folding one must cost whole pages —
 *  a fold that only changed a number would satisfy a smaller fixture. */
const FILLER_LINES = 60;

/** The fixture's heading levels are 1, 1, 2 — so `Beta chapter` is a PARENT in
 *  the heading tree (`Gamma section` sits under it) and `Alpha chapter` is a
 *  LEAF whose only content is body text. One of each, which is what lets this
 *  file cover both affordances from one document. */
const [ALPHA_LEAF, BETA_PARENT] = TOC_HEADINGS;

/** Opens a generated document with three real `Heading 1/2` headings and a long
 *  body under each, and waits for it to be paginated.
 *
 *  `toc-docx.mjs` rather than a new builder: it already produces the shape this
 *  needs — `w:pStyle` headings with Word's own style definitions behind them, so
 *  `fold::heading_level` resolves them the way it would in a real document — and
 *  a second in-memory DOCX builder is a second thing to drift. */
async function openHeadings(page) {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(tocDocx("folding.docx", FILLER_LINES));
  await page.waitForFunction(
    () => /of\s+\d+/.test(document.getElementById("statPages")?.textContent ?? ""),
    null,
    { timeout: 45_000 },
  );
  return documentPageCount(page);
}

/** Opens the outline panel, which must be a real `role="tree"` — a flat list of
 *  buttons has no disclosure to click. */
async function openOutline(page) {
  await runPaletteCommand(page, "view.outline", "outline");
  await expect(
    page.locator("#outlineBody .outline-tree[role='tree']"),
    "the outline panel must be a tree, not a flat list",
  ).toBeVisible();
}

/** The outline row for `text`, as `{ row, item, twisty }`. */
function outlineRow(page, text) {
  const row = page.locator(".outline-row", {
    has: page.locator(`.outline-item:text-is("${text}")`),
  });
  return { row, item: row.locator(".outline-item"), twisty: row.locator("button.outline-twisty") };
}

/** Puts the caret in `heading` by picking its outline row, which is what a
 *  reader does, and is also the only way to reach a heading on a page the
 *  viewport has not scrolled to. */
async function caretInHeading(page, heading) {
  await outlineRow(page, heading).item.click();
}

/** The fold state the ENGINE holds for `heading`, read through a FRESHLY BUILT
 *  outline panel: closing and reopening it runs `buildOutline`, which syncs from
 *  `foldState()` before it renders.
 *
 *  Rebuilt rather than read off the screen because an assertion against the DOM
 *  already rendered — or a poll for the value that is already there — passes on
 *  exactly the stale reading it is meant to catch. Two earlier spellings of the
 *  print check stayed green while print restored nothing at all. */
async function foldStateFromEngine(page, heading) {
  await runPaletteCommand(page, "view.outline", "outline");
  await runPaletteCommand(page, "view.outline", "outline");
  return outlineRow(page, heading).row.getAttribute("aria-expanded");
}

/** Every block of text the accessibility mirror is currently projecting. */
async function mirrorText(page) {
  return page.evaluate(() =>
    [...document.querySelectorAll("#a11yDocument *")].map((el) => el.textContent ?? "").join("\n"),
  );
}

/** The number of page objects in a PDF's bytes.
 *
 *  Counted from `/Type /Page` (and its unspaced spelling), excluding `/Pages`,
 *  which is the tree node rather than a page. That is the printed page count —
 *  the number a reader would see in a PDF viewer — which makes this a guard over
 *  the OUTPUT rather than over an internal flag. */
function pdfPageCount(text) {
  return [...text.matchAll(/\/Type\s*\/Page(?![s])/g)].length;
}

/** Intercepts the blob Print produces and neuters the dialog, before any app
 *  script runs. Headless Chromium never reaches the frame's own `print()`,
 *  which is fine: what is asserted is WHAT was handed to the browser. Mirrors
 *  `print.spec.mjs`, which reads the same blob for the real-text claim. */
async function capturePrint(page) {
  await page.addInitScript(() => {
    window.__blobs = [];
    const original = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (object) => {
      window.__blobs.push(object);
      return original(object);
    };
    window.print = () => {};
  });
}

/** The PDF the Print command handed the browser, polled because the export runs
 *  across later turns of the event loop. */
async function printedPdfText(page) {
  const read = () =>
    page.evaluate(async () => {
      const blob = window.__blobs?.find((b) => b && b.type === "application/pdf");
      return blob ? await blob.text() : null;
    });
  await expect
    .poll(async () => (await read()) !== null, { message: "Print must hand the browser a PDF" })
    .toBe(true);
  return read();
}

test("the outline's disclosure folds a heading, and the pages go away", async ({
  page,
  consoleErrors,
}) => {
  const unfolded = await openHeadings(page);
  expect(unfolded, "the fixture must be long enough for pages to be lost").toBeGreaterThan(3);

  await openOutline(page);
  const { row, twisty } = outlineRow(page, BETA_PARENT);
  await expect(row, "a heading with a deeper heading under it is a tree parent").toHaveAttribute(
    "aria-expanded",
    "true",
  );

  // BODY lines, not heading titles: every heading's title also appears in the
  // document's table of contents at the top, so `not.toContain("Beta chapter")`
  // could never pass and `toContain` could never fail — a pair of assertions
  // that look like they are about the fold and are about the contents page.
  const before = await mirrorText(page);
  expect(before).toContain("Beta body line 7.");
  expect(before).toContain("Gamma body line 7.");

  await twisty.click();

  await expect(row, "the disclosure must report the new state").toHaveAttribute(
    "aria-expanded",
    "false",
  );
  await expect
    .poll(() => documentPageCount(page), {
      message: "folding must drop PAGES — reflow, not blanking",
    })
    .toBeLessThan(unfolded);

  // Claim 1b: the folded RANGE leaves the mirror, and nothing else does.
  const mirror = await mirrorText(page);
  expect(mirror, "a screen reader must not read what the canvas hides").not.toContain(
    "Beta body line 7.",
  );
  expect(
    mirror,
    "and the whole subtree goes: the level-2 heading's body is inside the fold too",
  ).not.toContain("Gamma body line 7.");
  // The anti-vacuity half, and the one that makes the two above mean something:
  // the mirror is WINDOWED, so "the text is absent" would also be true of a
  // window that had simply moved or shrunk. Content before the fold must still
  // be there, which only a RANGE filter produces.
  expect(
    mirror,
    "the fold is a range, not a truncation: content above it still reads",
  ).toContain("Alpha body line 7.");

  // And the reader is told that the on-screen page count is no longer the
  // printed one, because it is not (`fold_view.mjs`'s page-count caveat).
  await expect(page.locator("#status")).not.toHaveText("");

  expect(consoleErrors).toEqual([]);
});

test("a heading whose only content is body text folds from the caret", async ({
  page,
  consoleErrors,
}) => {
  const unfolded = await openHeadings(page);
  await openOutline(page);

  // The leaf has no tree disclosure, in Word either — so the affordance is the
  // caret one, and this is the case a document of sibling headings is made of.
  await expect(
    outlineRow(page, ALPHA_LEAF).twisty,
    "a leaf carries an inert spacer, not a button",
  ).toHaveCount(0);

  await caretInHeading(page, ALPHA_LEAF);
  expect(await mirrorText(page)).toContain("Alpha body line 7.");

  await runPaletteCommand(page, "view.fold.toggle", "collapse heading");

  await expect
    .poll(() => documentPageCount(page), {
      message: "a leaf heading must fold from the caret, or most headings cannot be folded",
    })
    .toBeLessThan(unfolded);
  const mirror = await mirrorText(page);
  expect(mirror, "the leaf's body must leave the mirror with its heading").not.toContain(
    "Alpha body line 7.",
  );
  // Anti-vacuity, as above: the mirror is windowed, so absence alone proves
  // nothing. What follows the fold must still read.
  expect(mirror, "a fold is a range: the next section still reads").toContain("Beta body line 7.");

  // The same command unfolds it, and the page count comes all the way back.
  await runPaletteCommand(page, "view.fold.toggle", "expand heading");
  await expect.poll(() => documentPageCount(page)).toBe(unfolded);

  expect(consoleErrors).toEqual([]);
});

test("Collapse All and Expand All are reachable from the palette, and the round trip is exact", async ({
  page,
  consoleErrors,
}) => {
  const opening = await openHeadings(page);
  await openOutline(page);

  await runPaletteCommand(page, "view.fold.all", "collapse all");
  await expect
    .poll(() => documentPageCount(page), { message: "Collapse All must drop pages" })
    .toBeLessThan(opening);
  // Every heading that is a tree parent now reads collapsed; a leaf has no
  // attribute to read, which is the ARIA rule the panel follows.
  const expanded = await page.$$eval(".outline-row", (rows) =>
    rows.map((r) => r.getAttribute("aria-expanded")).filter((v) => v !== null),
  );
  expect(expanded.length, "at least one tree parent must be visible").toBeGreaterThan(0);
  expect(expanded.every((v) => v === "false"), `every parent must read collapsed: ${expanded}`).toBe(
    true,
  );

  await runPaletteCommand(page, "view.fold.none", "expand all");
  await expect
    .poll(() => documentPageCount(page), {
      message:
        "Expand All must restore the EXACT opening page count: a fold is a view, so a round " +
        "trip that landed anywhere else would mean something had written to the document",
    })
    .toBe(opening);

  expect(consoleErrors).toEqual([]);
});

test("printing while a heading is folded prints the whole document and keeps the fold", async ({
  page,
  consoleErrors,
}) => {
  await capturePrint(page);
  const unfolded = await openHeadings(page);
  await openOutline(page);

  await outlineRow(page, BETA_PARENT).twisty.click();
  await expect.poll(() => documentPageCount(page)).toBeLessThan(unfolded);
  const folded = await documentPageCount(page);

  await runPaletteCommand(page, "file.print", "print");
  const pdf = await printedPdfText(page);

  expect(pdf.slice(0, 8), "the real-text writer, not the raster fallback").toBe("%PDF-1.7");
  // WHAT THIS ASSERTION PROVES, AND WHAT IT DOES NOT — said out loud, because a
  // guard cited for more than it checks is the recurring defect here.
  //
  // It is GREEN TODAY INDEPENDENTLY OF THE PRINT PATH: `casual_doc_io::pdf`
  // paginates from the MODEL (`export_document(request.document, …)`), so the
  // session's fold set never reaches the writer. Removing the expansion from
  // `withPagedLayout` leaves this passing — measured, not assumed. So it is a
  // REGRESSION DETECTOR for the one change that would break it, which that
  // file's own doc comment names as planned work: seeding the export's
  // pagination from the session's font registry would hand it the live layout,
  // and the folded document with it.
  //
  // The wrapper's rule — "whatever runs inside this wrapper sees an expanded
  // document" — is guarded in `tests/print_expansion.test.mjs`, where five
  // mutations drive it red. The path that really printed folded is the 150-DPI
  // raster fallback, which cannot be forced from a browser test because the
  // engine handle has no surface in the page (`main.js` has zero exports).
  expect(
    pdfPageCount(pdf),
    `the printed PDF must carry the whole document's ${unfolded} pages, not the ${folded} ` +
      "the reader can currently see. A PDF missing a section, with nothing in it to say so, " +
      "is the silent loss AGENTS.md forbids — and the person who printed it cannot tell",
  ).toBe(unfolded);

  // Printing is not an edit: the reader's fold survives it.
  //
  // Read back through a FRESHLY BUILT panel — closing and reopening the outline
  // re-reads `foldState()` from the engine — because an assertion against the
  // DOM already on screen, or a poll for the value that is already there,
  // passes on exactly the stale reading it is meant to catch. Both earlier
  // spellings of this check stayed green while print never restored anything.
  await expect
    .poll(() => foldStateFromEngine(page, BETA_PARENT), {
      message:
        "print must put the reader's folds back. Polled through a REBUILT panel, and " +
        "polled rather than read once, because the restore lands in " +
        "`withPagedLayout`'s `finally` — after the PDF blob the poll above waits on",
      timeout: 30_000,
    })
    .toBe("false");
  expect(await documentPageCount(page), "and the page count is the folded one").toBe(folded);

  expect(consoleErrors).toEqual([]);
});

// ---- The in-body disclosure: it works at all, and it survives reflow --------
//
// TWO claims, and the first one was not supposed to need a test. `grep -rn
// "fold-body-chevron" tests/e2e` was ZERO hits before this, and the control was
// inert in EVERY view: `preventDefault` on `mousedown` stops the browser's own
// focus and selection side effects and does nothing about the editor's handler,
// which is bound to `pointerdown` on `#pages`. So every press on the chevron
// also ran a hit test and moved the caret, and the `click` that followed asked
// `caretHeading()` about wherever the caret had just landed. A control that had
// been made clickable (#777) and still folded nothing — which is the shape
// `SKILL.md` §9.4 names: built is not reachable.
//
// The second claim is reflow. Two individually-correct rules used to compose
// into a capability that could not be reached from the document in the view
// where folding matters most: the chevron needs more margin than a reflow
// tile's 16px gutter, and it is only shown while the outline panel is open. The
// uniform surface answers the first — beyond the tile is now the same surface
// the text is on, not the desk — and reflow opening the outline answers the
// second.

/** Waits until the page band has been REBUILT at the reflow column.
 *
 *  Not `is-reflow`, and the difference is why the first version of this test was
 *  racy: `renderAll` is asynchronous and writes that class early, inside the
 *  same pass that talks to the engine, so a caret move placed in between paints
 *  a chevron against page records the finishing render then replaces. The tile's
 *  own width is the band's report that it is done.
 *
 *  Compared against the PAPER sheet's width, measured before reflow was turned
 *  on, rather than against a constant: this used to wait for "narrower than
 *  600px", which was a fact about the old 80-character default and stopped
 *  being true when the default became the page's own width (`docs/151` §6.2a),
 *  whose tile — the page plus two gutters — is WIDER than the sheet. */
async function reflowSettled(page, paperWidth) {
  await expect
    .poll(() => sheetWidth(page), {
      message: "the band never re-built at the reflow column",
      timeout: 30_000,
    })
    .not.toBe(paperWidth);
}

/** The first sheet's painted width, CSS px. */
async function sheetWidth(page) {
  return page.evaluate(() => {
    const wrap = document.querySelector(".page-band > .page-wrap");
    return wrap ? Math.round(wrap.getBoundingClientRect().width) : 0;
  });
}

test("the in-body chevron folds the heading, on paper and in reflow", async ({
  page,
  consoleErrors,
}) => {
  // A SHORT document, not `openHeadings`'s 60-line one, and the reason is a
  // separate finding rather than convenience: the chevron is painted by
  // `drawSelection`, which runs when the caret MOVES, while `navigateToNode`
  // scrolls afterwards — so for a heading on a page the window has not
  // materialized yet there is no overlay to anchor to at paint time, and nothing
  // re-runs `drawSelection` once the scroll settles. That is true on paper too,
  // it is not what this test is about, and it is reported rather than worked
  // around here. The fixture keeps all three headings on page one.
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(tocDocx("folding-reflow.docx", 2));
  await page.waitForFunction(
    () => /of\s+\d+/.test(document.getElementById("statPages")?.textContent ?? ""),
    null,
    { timeout: 45_000 },
  );
  const chevron = page.locator(".overlay .fold-body-chevron");
  const beta = () => outlineRow(page, BETA_PARENT).row;

  // ---- 1. ON PAPER: it is painted in the margin, and it FOLDS ---------------
  await openOutline(page);
  await caretInHeading(page, BETA_PARENT);
  await expect(chevron, "Word's margin chevron is not painted at all").toBeVisible();
  await expect(beta(), "the heading must start expanded, or a fold proves nothing").toHaveAttribute(
    "aria-expanded",
    "true",
  );
  await chevron.click();
  await expect(beta(), "the margin chevron folded nothing — a dead control").toHaveAttribute(
    "aria-expanded",
    "false",
  );
  // The ENGINE holds it, not the row: rebuilt from `foldState()`.
  expect(await foldStateFromEngine(page, BETA_PARENT)).toBe("false");
  await runPaletteCommand(page, "view.fold.none", "expand all");
  await expect(beta()).toHaveAttribute("aria-expanded", "true");

  // ---- 2. IN REFLOW: still painted, beside the measure, and it still folds --
  const paperWidth = await sheetWidth(page);
  await runPaletteCommand(page, "view.reflow", "reflow");
  await expect(page.locator("#viewport")).toHaveClass(/is-reflow/);
  await reflowSettled(page, paperWidth);
  await caretInHeading(page, BETA_PARENT);
  await expect(chevron, "a heading in reflow has no collapse control").toBeVisible();

  // Beside the measure, not on it. This is the rule a clamp got wrong (#775):
  // the chevron is an absolutely-positioned button, so one pixel over the
  // heading's first glyph captures the tap that should place the caret. And it
  // must still be inside the scroller — surface is somewhere to paint,
  // off-screen is not.
  const geometry = await page.evaluate(() => {
    const el = document.querySelector(".overlay .fold-body-chevron");
    const box = el.getBoundingClientRect();
    const wrap = el.closest(".page-wrap").getBoundingClientRect();
    const port = document.getElementById("viewport").getBoundingClientRect();
    return { left: box.left, right: box.right, textLeft: wrap.left + 16, portLeft: port.left };
  });
  expect(
    geometry.right,
    `the chevron reaches ${geometry.right}, over text that starts at ${geometry.textLeft}`,
  ).toBeLessThanOrEqual(geometry.textLeft + 1);
  expect(
    geometry.left,
    `the chevron is painted at ${geometry.left}, outside a scroller that starts at ` +
      `${geometry.portLeft}`,
  ).toBeGreaterThanOrEqual(geometry.portLeft);

  await chevron.click();
  expect(
    await foldStateFromEngine(page, BETA_PARENT),
    "the reflow chevron did not fold anything",
  ).toBe("false");
  expect(consoleErrors).toEqual([]);
});
