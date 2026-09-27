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
 *  font inventory handed to the module.
 *  Lowered to 17,054 after a NEW shape of the merge trap above turned `main` red
 *  with two green PRs and no conflict. At 17,081 the file had zero slack, so
 *  #613 (popover anchoring) and #614 (drop caps) each bought their one new line
 *  by deleting the SAME trailing blank line at the end of the file. Git applies
 *  that deletion once, so the merge landed at 17,082 and this guard failed on
 *  `main` while both branches had passed it. The lesson is not a number: paying
 *  the ratchet in cosmetic whitespace is not paying it, because whitespace is
 *  not per-branch currency. So the fix moves code out, as the failure message
 *  asks — the swatch vocabularies (`TEXT_STANDARD_COLORS`, `HIGHLIGHT_COLORS`,
 *  the name→hex/label maps) went to `palettes.mjs`, which also makes "every
 *  highlight the engine can write is offerable" provable in node — and leaves
 *  27 lines of slack so the next two concurrent PRs do not collide the same way.
 *  Lowered again to 17,040 by the Insert-band reachability round (Page number,
 *  Date and Comment as direct band buttons): the three `INSERT_SURFACE` /
 *  `REVIEW_SURFACE` rows and their rationale cost more than the 14 lines of
 *  slack that were left, so the field VOCABULARY — the kind table and the
 *  host-side result formatter — moved to `field_kinds.mjs`. It is data plus one
 *  pure function, so what a date field caches is now answerable in node, which
 *  is the ratchet buying a seam rather than just a smaller file.
 *  RE-MEASURED to 17,045 on rebase, and this is the SAME merge trap the two
 *  notes above record, caught before it landed rather than after. 17,040 was
 *  measured against a base that did not yet carry the Resolve/Delete comment
 *  commands' menu-and-palette entries; rebasing onto a main that does added
 *  five lines the branch could not have known about, and the ceiling it had
 *  lowered was suddenly five under the file. Re-measured from the MERGED file,
 *  which is the only number that was ever meaningful — and 17,045 is still nine
 *  below main's 17,054, so the round still paid for itself.
 *  Lowered again to 17,000 by the radio-group round (`109` UX-021): six segmented
 *  controls each hand-rolled the same "set the clicked one, clear the others"
 *  loop and only two of them ever published a radio's state, so the pattern
 *  moved to `radio_group.mjs` and the call sites now say what they mean
 *  (`reflect`, `select`, `value`) instead of querying `[aria-pressed="true"]`.
 *  That is the ratchet doing its job: the file shrank BECAUSE six copies of one
 *  rule became one, not because anything was trimmed.
 *  Lowered to 16,831 by the landscape-ruler fix — the biggest single drop so far,
 *  and the clearest case for why the ratchet is worth paying. The fix needed a
 *  dozen lines in a file with ZERO slack, so the whole 244-line ruler moved to
 *  `ruler.mjs`: the strip, the indent markers, the tab stops and both drag
 *  interactions, which are one cohesive thing keyed to one scale — and the scale
 *  was the thing that was wrong. It also came out with no English in it (the five
 *  titles are passed in), so the unrouted-string ceilings did not move at all.
 *  RE-MEASURED on the merge of UX-021 and the landscape ruler, which is the
 *  THIRD time this trap has been recorded here and the first time both halves
 *  were extractions rather than additions. UX-021 measured 17,000 and the ruler
 *  16,831, each honestly against a `main.js` of 17,045 — and NEITHER describes
 *  the merged file, because the two removed different code. Git applied both
 *  deletions, so the merge is smaller than either branch predicted. The number
 *  below is measured from the merged file; it is the only one that was ever
 *  meaningful.
 *  Lowered to 16,721 by the keymap round (`109` UX-006 + UX-007): four separate
 *  `keydown` handlers and a `FORMAT_KEYS` table became one declarative chord ->
 *  command map in `keymap.mjs`, which is also what makes a label and its binding
 *  ONE declaration instead of two tables that drifted apart.
 *  Measured and lowered AFTER the ratchet settled, not during: this branch was
 *  told to leave the number alone while #617, #618 and #622 were all in flight
 *  against a moving `main.js`, because each would otherwise have measured a file
 *  the others were about to change — the merge trap the notes above record three
 *  times. With those merged, 16,779 is the merged measurement and 16,721 is this
 *  round's. Leaving the 58 lines of slack would hand the next change free growth,
 *  which is the one thing a ratchet exists to refuse.
 *  Lowered to 16,629 by OO-005 (captions and cross-references). The file was AT
 *  its ceiling with ZERO slack and the round needed a References surface row, two
 *  dialog hosts and a shared object-menu row builder, so it paid with two
 *  extractions rather than with whitespace — the mistake the 17,054 note above
 *  records. What came out: the 156-line OBJECT right-click menu, which reached
 *  the application through nothing but functions and so became a pure module
 *  with its own node test (it had none), and the Outline panel's row builder,
 *  which is the same enumeration the References tab is about and took its one
 *  English sentence into the catalogue on the way. Both are worth more than the
 *  lines: 156 lines of menu policy now have a test, and `buildOutline` is
 *  answerable without a browser.
 *  Lowered to 16,616 by the section-properties round. The file was AT its ceiling
 *  again, and the round needed a surface row, a palette row, a button id and a
 *  module construction — so it paid the same way: the two running-content variant
 *  toggles (`runningVariantState` / `toggleRunningVariant`) moved into
 *  `header_footer_settings.mjs`, which is the dialog whose Options group is those
 *  same two switches. That is worth more than the lines. The dialog's checkboxes
 *  and the Insert band's buttons are now ONE implementation, so they cannot
 *  disagree about what `w:titlePg` says, and the four English sentences those
 *  functions built went into the catalogue on the way — which is why the unrouted
 *  ceiling below moved too. Re-measured from the file AFTER rebasing onto
 *  `origin/main` at e8cdf0e, not before: measuring first is the trap the notes
 *  above record three times.
 *  Lowered to 16,602 by the host contract (`docs/126` phase 2). The file was AT
 *  its ceiling with ZERO slack again, and the contract needs 20 lines of it: one
 *  `let`, five notification hooks on choke points the editor already has, and the
 *  session/bridge construction at the end, where the command registry finally
 *  exists. It paid with two extractions. Out: the SAVE SURFACE — which formats
 *  the Save selector offers, what the compatibility chip reads, and the one step
 *  that puts bytes on a visitor's disk — now `save_formats.mjs`, so "does the
 *  selector offer every exporter the engine registered, and does it default to
 *  the format the document was opened as" is a node question rather than a
 *  browser one. And the smart-quote decision, which moved into `text_rules.mjs`
 *  beside the rule it calls: its only hard part is that an engine offset is a
 *  UTF-8 BYTE index (`docs/104` HF-055), which is exactly what that module is
 *  for, and it is now answerable with a plain function instead of a document.
 *  Lowered to 16,574 by white-labelling and region composition (`docs/126` phase
 *  3). The round needed three imports, a region application at boot, a brand
 *  product in the tab title and a pinned-accent branch, and the file was at its
 *  ceiling with zero slack again — so it paid with two extractions rather than
 *  whitespace.
 *
 *  Out: the guarded `localStorage` helpers, now `prefs.mjs`. Worth more than the
 *  lines because `localStorage` here is HOST POLICY — touching it throws with
 *  site data blocked, in a cross-origin embed, or in some private modes, and an
 *  unguarded module-scope read once left the whole editor inert — so "what
 *  happens when storage is refused" is now a node question with a fake store,
 *  where before only a browser with site data blocked could ask it.
 *
 *  And out: the appearance policy, now `appearance.mjs`. That one is the phase's
 *  own defect. `applySettings()` wrote an INLINE `--accent` on `:root` at import
 *  and REMOVED a host's `data-theme`, so a white-labelled build had its brand
 *  overwritten before its first frame (`docs/125` §2 F4). The rule "a host's
 *  brand outranks a visitor's stored preference" is now a function of two inputs
 *  rather than six lines in the middle of a DOM reflection, and can be driven red
 *  in node.
 *
 *  RE-MEASURED from the merged file after rebasing onto `origin/main`, not
 *  before: the trap the notes above record three times.
 *  Lowered to 16,579 by the structured-paste loss fix (`docs/129` §2). The round
 *  needed an import, a `pasteLossMessage` call and the comment saying why a
 *  SUCCESSFUL paste reports as `"error"` — and the file was at its ceiling with
 *  zero slack again, so it paid by removing a duplicated mechanism rather than by
 *  whitespace: `pasteStructured` and `pasteExternalStructured` differed only in
 *  which engine method they named and became one `runStructuredPaste(insert)`.
 *  That is worth more than the lines. Two copies of a paste path is precisely
 *  where the loss report gets wired into one and forgotten in the other — the
 *  `expectEditorFocused` lesson (`SKILL` §10) applied before the second patch
 *  rather than after it. RE-MEASURED from the merged file after rebasing onto
 *  `origin/main` at aa91bd0: the first number this branch carried was 16,606,
 *  measured against a `main` that then landed the host contract and took the
 *  ceiling to 16,589 underneath it. That is the merge trap the notes above record
 *  three times, and it is why this number is a measurement of the merged file and
 *  never arithmetic on two branches. */
const MAIN_JS_LINE_CEILING = 16574;

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
  // The caption / cross-reference VOCABULARY: which "Insert reference to"
  // options a reference type offers, when Word offers "Include above/below",
  // and what a caption will read as. Word's own rules, with no widget attached,
  // so `cross_reference_model.test.mjs` can drive every type in node — and
  // ONLYOFFICE carries the same mapping as a switch inside a Backbone view,
  // where nothing can test it.
  "cross_reference_model.mjs",
  // The object right-click menu. It builds plain command descriptors and reaches
  // the application only through its `io`, which is what lets
  // `object_context_menu.test.mjs` assert what a picture, a shape and a text box
  // offer, in each review mode, without a browser.
  "object_context_menu.mjs",
  "edit_errors.mjs",
  // The field vocabulary: the kind table plus the host-side result formatter.
  // No DOM and no engine, so "what does a date field cache" is a node question.
  "field_kinds.mjs",
  // The whole target -> cursor mapping. Its DOM half is `pointer_hover.mjs`;
  // keeping them apart is what lets `pointer_cursor.test.mjs` drive the entire
  // hover cascade with plain objects, with no browser and no engine.
  "pointer_cursor.mjs",
  // The swatch vocabularies: literal tables and one name->hex lookup, so the
  // question "does the picker offer every highlight the engine can write?" is
  // answerable against the Rust source in node (`palettes.test.mjs`).
  "palettes.mjs",
  // The memoized "does this document hold an object?" answer (`109` HF-183). The
  // engine call is injected, which is what lets `object_presence.test.mjs` count
  // engine calls and payload bytes at n and 2n objects in node — a complexity
  // guard, where a browser could only offer a stopwatch.
  "object_presence.mjs",
  "popover_position.mjs",
  // The radio-group pattern. It is handed its container and never reaches for a
  // global one, which is what lets `radio_group.test.mjs` drive the arrow
  // arithmetic in node and what would let a host mount a segmented control of
  // its own on the same contract.
  "radio_group.mjs",
  // Which command each Home / View / Table ribbon control stands for (`109`
  // UX-005). Selectors and ids only: the caller supplies the root, so the table
  // is checkable in node and cannot quietly grow a DOM opinion.
  "ribbon_faces.mjs",
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
  // The Table band's structural controls. Handed its root and its engine, so the
  // operation tables are set-comparable against `editor.html` in node.
  "table_band.mjs",
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
