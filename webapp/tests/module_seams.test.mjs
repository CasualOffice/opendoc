// The ratchet on `main.js`, and the rules that keep an extracted module
// extracted.
//
// `109` HF-085 is not "main.js is big"; it is that main.js is the webapp, with
// no seam a test, a host page or a translator can reach. Extracting a few
// modules fixes that only if the file cannot quietly grow back — and it grew
// to 18k lines one reasonable commit at a time, with nothing objecting.
//
// So the ceiling below is a RATCHET, not a target. It is set to whatever the
// file measured when it was last lowered. A change that adds lines to main.js
// fails this test, and there are exactly two honest ways to go green: put the
// new code in a module, or take something else out. When you do take something
// out, lower the ceiling to the new measurement in the same commit — that is
// what makes the next change harder rather than easier.
//
// Never raise it. If a change genuinely cannot avoid growing main.js, that is
// a conversation with the owner and a recorded exception (SKILL.md §10: no
// file over ~2,000 lines without one), not a bumped number.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

const SRC = new URL("../src/", import.meta.url);

/** Measured 2026-09-21 on the merge of two independent extraction rounds:
 *  the review-layout, review-label and card-reuse modules that came with
 *  HF-088/HF-025 and the vertical-navigation fix, and the page-band round
 *  (`docs/113` section 8.6) that took the scroll model, the print path, the
 *  accessibility mirror, the Pages navigator, the shortcut reference and the
 *  About dialog out. Each branch measured its own half — 18,134 and 18,158
 *  against a shared 18,186 base — so neither number is right for the merge
 *  and this one is re-measured from the merged file rather than picked.
 *  Lowered again to 18,107 by the sticky-goal-column fix (HF-164): the goal
 *  column and its clears cost lines, so the selection-ordering and
 *  position-comparison helpers moved into `caret_navigation.mjs` (renamed from
 *  `caret_probe.mjs`) to pay for them.
 *  Was 18,373 before the first HF-085 extraction.
 *  Lowered again to 17,408 by the one-axis navigation restructure (`109` UX-014,
 *  `docs/122`), which is the ratchet working as intended: the application-menu
 *  bar's ~150 lines of rendering and keyboard behaviour moved to
 *  `command_menu.mjs`, where the new File PAGE reuses the same row builder rather
 *  than growing a second copy of it, and that extraction paid for the File page,
 *  the tab wiring, the five new band buttons and three small fixes with four
 *  lines to spare. Re-measured from the merged file per the MERGE NOTE below: the
 *  base read 17,412 against a ceiling of 17,425, and neither number described
 *  this file.
 *  Lowered again to 17,801 by the spelling row (`109` HF-035, `docs/114`):
 *  the file was AT its ceiling with zero slack, so the 242 lines of curated
 *  Symbol/Emoji data moved to `glyph_sets.mjs` — pure data, already covered by
 *  the picker's own specs — to pay for the ~110 lines of spell-check wiring.
 *  The checker itself is in `spell_check.mjs` and `spelling.mjs`, which is also
 *  what makes its rules unit-testable without a browser.
 *  Lowered again to 17,658 by the compact-toolbar width/grouping fix: the bar's
 *  layout table, its grouping and its overflow fold moved to
 *  `compact_toolbar.mjs`, which is also what lets a test read the declared
 *  control set without a browser.
 *  Lowered again to 17,568 when grammar joined it (the owner raised grammar
 *  above spelling on 2026-09-23): the bookmark manager moved to
 *  `bookmark_manager.mjs`, keeping only the review-mode gate and the repaint,
 *  which are about review mode and about pages rather than about bookmarks.
 *  MERGE NOTE: both of the above came off the same 17,801 base and each
 *  lowered the ceiling to its own half's measurement, so NEITHER number is
 *  right here — exactly the trap SKILL.md §5 records. This one is re-measured
 *  from the merged file.
 *  Lowered again when localisation landed on top of the red-main fixes: the
 *  seam, the loader and the count sentences became modules, and so did the
 *  review-formatting labels. BOTH sides of that merge lowered the ceiling from
 *  17,362 to their own half's measurement — 17,324 and 17,330 — so neither
 *  number was right for the merged file, which is the same trap this note
 *  records above. Re-measured from the merged file.
 *  Lowered again to 17,314 by the right-margin comment affordance: the file was
 *  AT its ceiling, so the four DOM factories every comment card is built from
 *  (`reviewCardButton`, `reviewIconButton`, `autoGrowTextarea`,
 *  `attachComposerKeys`) moved to `review_chrome.mjs`, which is also where the
 *  affordance's own controller lives. None of the four read a single piece of
 *  application state, so they had no business being in here.
 *  Lowered again to 17,122 by the line-numbering work (`109` OO-022): the file
 *  was AT its ceiling with one line of slack, so Word's whole Layout ▸ Page
 *  Setup group — the geometry dialog's 212 lines, plus the new Line Numbers
 *  popover — moved to `page_setup.mjs`. The two belong in one module because
 *  they share one question, "which section is the caret in", and answering it
 *  twice is how the dialog came to show one section's margins while writing
 *  them to another.
 *  Lowered again to 17,081 by the watermark dialog (`109` OO-006): the file was
 *  AT its ceiling with zero slack, so the per-author review COLOUR — the palette,
 *  the author key and the FNV hash over it — moved to `review_labels.mjs`, where
 *  the rest of the pure per-review-item vocabulary already lives and where the
 *  mapping is provable without a browser. The dialog itself went into
 *  `page_setup.mjs`, beside the rest of Word's Page Setup group; what the handful
 *  of lines still here buy is the button binding, the `LAYOUT_SURFACE` row that
 *  makes the command reachable from the palette as well as the ribbon, and the
 *  font inventory handed to the module. */
const MAIN_JS_LINE_CEILING = 17081;

/** Modules that must stay free of the browser: they are the ones a unit test,
 *  a host page or a non-DOM runtime can use, and the only thing that keeps
 *  them that way is that adding a `document.` to one fails here. */
const PURE_MODULES = [
  // Holds the vertical goal column and nothing else: no DOM and no engine, so
  // the arrow-key rule is unit-testable as a plain state machine.
  "caret_navigation.mjs",
  "command_taxonomy.mjs",
  // The Symbol / Emoji sets: literal data with no behaviour, so nothing in it
  // has any business reaching a global.
  "glyph_sets.mjs",
  // The localisation seam. Purity is the point: a locale's plural rules and
  // fallback chain are answerable without a document existing, which is what
  // lets `i18n.test.mjs` drive Russian's three plural forms in node. The DOM
  // half — walking `data-i18n` attributes — is `localize.mjs`.
  "i18n.mjs",
  "contrast.mjs",
  "edit_errors.mjs",
  // The whole target -> cursor mapping. Its DOM half is `pointer_hover.mjs`;
  // keeping them apart is what lets `pointer_cursor.test.mjs` drive the entire
  // hover cascade with plain objects, with no browser and no engine.
  "pointer_cursor.mjs",
  "review_labels.mjs",
  "review_layout.mjs",
  // Takes nodes as arguments and never reaches for a global one, which is what
  // lets `shortcut_labels.test.mjs` drive the sweep with plain objects.
  "shortcut_labels.mjs",
  // Every rule that decides whether a word is misspelled and what to suggest
  // instead. The DOM half is `spell_check.mjs`; keeping these apart is what
  // lets `spelling.test.mjs` run the whole dictionary through them in node.
  "spelling.mjs",
  "status_policy.mjs",
  "text_rules.mjs",
  "units.mjs",
];

const BROWSER_GLOBALS = /\b(document|window|navigator|localStorage|sessionStorage|indexedDB|globalThis)\s*\./;

function read(name) {
  return readFileSync(new URL(name, SRC), "utf8");
}

/** Source with comments removed, so prose about "the open document." is not
 *  mistaken for a DOM access. */
function code(text) {
  return text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
}

/** Every script under `src/`, page entry points included. */
function scriptFiles() {
  return readdirSync(SRC).filter((f) => /\.(mjs|js)$/.test(f) && f !== "main.js");
}

/** The importable modules. The convention this encodes: `.mjs` is a module
 *  something imports, `.js` is a page's entry script (`main.js` for the
 *  editor, `fidelity.js` and `home-embed.js` for the site pages), which runs
 *  for its side effects and has nothing to export. */
function moduleFiles() {
  return readdirSync(SRC).filter((f) => f.endsWith(".mjs"));
}

test("main.js is at or below its ratchet", () => {
  const lines = read("main.js").split("\n").length - 1;
  assert.ok(
    lines <= MAIN_JS_LINE_CEILING,
    `main.js is ${lines} lines, ${lines - MAIN_JS_LINE_CEILING} over the ceiling of ` +
      `${MAIN_JS_LINE_CEILING}. Put the new code in a module under webapp/src/ and ` +
      "import it, or take something else out — do not raise this number.",
  );
});

test("the ceiling is not left stale after an extraction", () => {
  const lines = read("main.js").split("\n").length - 1;
  assert.ok(
    MAIN_JS_LINE_CEILING - lines <= 200,
    `main.js is now ${lines} lines but the ceiling still reads ${MAIN_JS_LINE_CEILING}. ` +
      "Lower it to the new measurement in the same commit, or the slack it leaves is " +
      "room for the file to grow back for free.",
  );
});

// A file in `src/` that exports nothing is another main.js in miniature: its
// contents are reachable only by loading it for its side effects.
test("every module beside main.js exports something", () => {
  const silent = moduleFiles().filter((f) => !/^\s*export[\s{]/m.test(read(f)));
  assert.deepEqual(
    silent,
    [],
    "a module with no exports cannot be unit-tested, mounted or reused — which " +
      "is the whole of HF-085, one file smaller",
  );
});

test("the pure modules stay free of the browser", () => {
  for (const name of PURE_MODULES) {
    const offending = code(read(name))
      .split("\n")
      .map((line, i) => [i + 1, line])
      .filter(([, line]) => BROWSER_GLOBALS.test(line));
    assert.deepEqual(
      offending,
      [],
      `${name} must stay pure: the DOM half belongs to the caller. Reaching a ` +
        "browser global here puts the decision back out of reach of a unit test.",
    );
  }
});

// main.js imports the modules; a module importing main.js back would re-couple
// everything to the 18k-line file and make the mount seam (HF-109) impossible.
test("no module imports main.js", () => {
  const cyclic = scriptFiles().filter((f) => /from\s+["']\.\/main\.js["']/.test(read(f)));
  assert.deepEqual(cyclic, [], "the dependency runs one way: main.js → modules");
});
