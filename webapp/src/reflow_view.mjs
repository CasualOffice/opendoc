// Reflow (pageless) — the shell half of ADR-046 and `docs/151` §6.
//
// The engine half is one setter: `doc.setLayoutView(contentWidthTwip,
// tileHeightTwip, gutterTwip)`. It lays the body out at the reader's width
// instead of on the document's paper and cuts the result into fixed-height
// tiles, so `renderPage`, `hitTest`, the caret and every overlay keep working
// and the document stays EDITABLE — which is where §3.2 diverges from
// ONLYOFFICE's read-only reader mode on purpose. What the shell owes is: a
// command with more than one face, the WIDTH, drawn tiles with no seam, the
// chrome that has to stand down, and print forced back onto paper.
//
// WHY THIS IS A MODULE AND NOT TEN LINES IN `main.js`. Two reasons, and the
// first is not the line ratchet. The interesting part of this feature is a pair
// of decisions about NUMBERS — which widths count as the same width, and how
// long to wait before believing one — and those decide whether a resize is O(1)
// or O(document) in the size of the reader's file. A decision that expensive
// should be answerable in `node` against a fake clock, not only in a browser
// against a stopwatch. The second reason is the ratchet (`module_seams`), and it
// is the smaller one.
//
// ---- THE COST MODEL, because it is the whole design -------------------------
//
// Entering, leaving, or CHANGING THE WIDTH OF reflow is O(document): the galley
// cache is scoped to the width it was shaped at (`flow.rs`'s
// `build_galley_cached`), so a different width keeps nothing. `docs/107` §4 is
// an owner constraint — per-interaction work is O(1) in document size — and a
// resize is an interaction. A drag fires `resize` per animation frame; a phone
// rotating, a soft keyboard opening and a URL bar retracting each fire a burst.
// So a width feed that passed every pixel straight through would be an
// O(document) re-shape sixty times a second on the slowest device we support.
//
// Two mechanisms answer that, and they are different in kind:
//
//   QUANTISATION decides which widths are the SAME width. It is what makes the
//   common case cost nothing at all: a drag that stays inside one bucket does
//   not re-shape once, however many events it fires, because the shell compares
//   the bucketed width with the one the engine already holds and finds them
//   equal. This is the O(1) guarantee.
//
//   THE DEBOUNCE bounds the WORST case — a drag that crosses many buckets. It
//   coalesces a gesture into its last value. Without quantisation it would be
//   the only defence and a slow drag would still re-shape at every step; with
//   quantisation it is the second line rather than the first.
//
// ---- THE TWO NUMBERS, and why these ----------------------------------------
//
// `docs/151` §6.2 proposed "the nearest 8px". This ships 16px, FLOORED, and
// both halves of that are corrections rather than preferences:
//
//   * FLOORED, not nearest. Rounding to the NEAREST bucket rounds UP half the
//     time, which makes the column wider than the space it was measured
//     against — and a column wider than the viewport is a horizontal scroll on
//     `#viewport`, the exact thing this feature exists to retire (`docs/148`
//     §8). A rounding rule that can reintroduce the defect it is part of the
//     fix for is the wrong rounding rule.
//
//   * 16, not 8. Because it is floored, the quantum IS the safety margin: the
//     column is between 0 and one quantum narrower than the space available.
//     That margin has to absorb a scrollbar that appears mid-gesture (15px on
//     Windows, 17px on a desktop GTK theme), sub-pixel rects from a fractional
//     device pixel ratio, and the twip rounding below. 8px does not cover a
//     scrollbar; 16px does. It costs at most 16px of text width, which is under
//     two characters at 11pt, and it halves the number of distinct widths a
//     390 -> 1280px drag can produce (56 rather than 111).
//
// ---- THE THIRD NUMBER, which this module shipped without --------------------
//
// `docs/154`, ADR-048. The two numbers above decide WHICH WIDTHS COUNT AS THE
// SAME; neither of them asks whether the window is a sensible measure in the
// first place, and `REFLOW_QUANTUM_PX`'s doc comment shows the shape of the
// omission — every word of it is about not EXCEEDING the viewport. There was a
// floor (`REFLOW_MIN_CONTENT_TWIP`) and no ceiling, so the reading column was
// the whole window at every width: 60 characters at 390px, which is right and
// was the only width ever evaluated, and 241 at 1440px, which is three times the
// WCAG 1.4.8 bound. `REFLOW_WIDTH_STEPS` and `reflowCapTwip` below are the
// ceiling; `reflowMeasure` is where the one clamp lives.
//
// The cap also makes a resize cheaper than quantisation alone can: above it the
// column does not move with the window AT ALL, so most desktop resizes are free
// for a second and better reason than the bucket.
//
// `REFLOW_DEBOUNCE_MS = 150` is a trailing debounce with no leading call. It is
// under the ~200ms at which an interface stops feeling attached to the gesture,
// and above the frame budget by an order of magnitude, so a burst of `resize`
// events collapses into one pass. There is no leading call BECAUSE the first
// event of a drag is the least likely to be the width the reader wants: acting
// on it guarantees at least two O(document) passes for one gesture.

import { TWIPS_PER_INCH } from "./units.mjs";

/** The width bucket, in CSS px. See the header for why 16 and why floored. */
export const REFLOW_QUANTUM_PX = 16;

/** Trailing debounce for the width feed, in ms. See the header. */
export const REFLOW_DEBOUNCE_MS = 150;

/** Breathing room between the text column and the edge of the window, per side,
 *  in CSS px. It is passed to the engine as the synthetic page's left and right
 *  margin, so the text never runs to the glass and a tile is still a box with a
 *  content area — which is what keeps indents, float wrap and table widths
 *  resolving against a measure rather than against the window. */
export const REFLOW_GUTTER_PX = 16;

/** The engine's own floor on a reading column: `LayoutView::reflow` refuses
 *  anything under an inch, at the seam, because an unconverted CSS pixel value
 *  (390 twips is a quarter of an inch) should fail where it can be read rather
 *  than inside a layout nobody can see.
 *
 *  It matters here because the twip width is a function of the ZOOM as well as
 *  the window: at 400% a 390px window is 1,404 twips of text, under the floor.
 *  The column then stops shrinking, and the tile is wider than the window — the
 *  same arbitration §6.3 gives a table too wide to fit, and honest for the same
 *  reason: what the reader is being shown is genuinely wider than their screen.
 *  The no-horizontal-scroll guarantee is stated at the zoom a phone actually
 *  opens at, which `FIT_ON_OPEN_FLOOR` pins at 100%. */
export const REFLOW_MIN_CONTENT_TWIP = TWIPS_PER_INCH;

/** The engine's own CEILING on a reading column, mirrored here so a correct
 *  caller never reaches it: `LayoutView::reflow` refuses `content_width >
 *  MAX_REFLOW_COLUMN` (`document_layout.rs`, `Twip(31_680)` = 22in).
 *
 *  THIS IS DEFECT TWO of `docs/154` §3.3, and it is a different defect from the
 *  missing cap even though one line fixes both. That bound was written as a
 *  UNIT-CONVERSION sanity check — its own doc comment says a caller asking for
 *  more "has converted units wrongly" — and with no maximum upstream it became
 *  reachable by a caller that had converted perfectly: `reflowMeasure` asks for
 *  more than 22in of column at a **2,160px window at 100% zoom**, and at
 *  **1,104px at 50%**, where 50% is both a `ZOOM_STEPS` entry and the value of
 *  `FIT_ON_OPEN_FLOOR`. `reflow_chrome.mjs`'s `sync` caught the throw, reverted
 *  the preference to paper and showed the reader a sentence about twips, so
 *  reflow turned itself off on a wide monitor and blamed the units.
 *
 *  It is mirrored rather than asked for because there is no `maxReflowColumn`
 *  getter on the seam — exactly as `REFLOW_MIN_CONTENT_TWIP` above mirrors the
 *  engine's floor, and for the same reason. It is applied to EVERY width policy
 *  including `full`, which is what makes the refusal unreachable rather than
 *  merely unlikely: a reader who asks for an uncapped column on a 4K monitor is
 *  given the widest column the engine can lay out, not an error. That is the
 *  honest arbitration — the engine genuinely cannot shape a column wider than
 *  this — and it is the same shape as the floor's. */
export const REFLOW_MAX_CONTENT_TWIP = 22 * TWIPS_PER_INCH;

/** Where the per-viewer preference lives (ADR-046: a viewer's choice, never a
 *  document property — one person's phone must not reformat another person's
 *  monitor). `null` from `readPref` means "never chosen", which is what lets the
 *  phone rung supply the default without overriding anybody. */
export const REFLOW_PREF_KEY = "docReflow";

// ---- THE MEASURE: how wide a reading column may be --------------------------
//
// `docs/154`, ADR-048. The defect this half answers has a number rather than a
// taste: `reflowMeasure` had a floor and NO CEILING, so the reading column was
// the window minus two gutters at every width — 352 CSS px at the 390px phone
// rung, which is 60 characters of 11pt Calibri and correct, but 1,408 px and
// **241 characters** at a 1440px window, and 323 at 1920. Every reference caps
// it: Google Docs' View ▸ Text width (Narrow/Medium/Wide, per-viewer, stated by
// Google to be invisible to collaborators), Word's Immersive Reader ▸ Column
// Width (four steps, documented purpose "changes line length to improve focus
// and comprehension"), Word's Read Mode (adjustable columns). ONLYOFFICE do not
// cap — they divide the paper's width by the device pixel ratio and grow the
// TYPE instead — and they ship no desktop reading view at all, so on this
// surface they are not a reference.
//
// WHY 80 AND NOT 66. The number is WCAG 2.1 SC 1.4.8 Visual Presentation (Level
// AAA): "Width is no more than 80 characters or glyphs (40 if CJK)", inside a
// criterion that asks for "a mechanism". It is the only normative, first-party,
// quotable figure in the whole field. Bringhurst's 45-75/66 is the figure
// everyone reaches for and it could NOT be verified against his text — no
// reachable source quotes the sentence with a page citation (`154` §7 item 1) —
// and this repository has published false claims twice (`105` EV-007, `99` §9),
// so the choice is made on EVIDENCE rather than on typography. 80 is also the
// conservative end of the two, so the default errs towards the paper the reader
// is already used to. Note that a Letter page's own 6.5in text column is 107
// characters at 11pt, already past 80, which is why the cap cannot simply be
// "the paper".
//
// WHY THE TARGET IS IN CHARACTERS. Every source above states its answer in
// characters, and it is the only unit that transfers across faces and sizes. It
// also makes a reader TYPE-SIZE control (`151` §8 item 2, still deferred) a
// consequence rather than a second policy: changing the size changes the width
// and leaves the measure where it was.

/** Mean advance per character, in em, of the faces this engine bundles —
 *  measured from each face's own `hmtx` table over a fixed 674-character English
 *  prose sample (the openings of *Pride and Prejudice* and *On the Origin of
 *  Species*, public domain), so that spaces and punctuation are weighted as they
 *  actually occur rather than an alphabet being measured. The derivation is
 *  reproducible in ten lines of `fontTools` and is printed in full in `154` §9.
 *
 *  Keyed by the metric-compatible name a document actually asks for as well as
 *  by the bundled file's family, because a DOCX says `Calibri` and the engine
 *  substitutes Carlito. Lower-cased on lookup.
 *
 *  Complexity: O(1). */
const MEAN_ADVANCE_EM = new Map([
  ["calibri", 0.3991],
  ["carlito", 0.3991],
  ["times new roman", 0.393],
  ["liberation serif", 0.393],
  ["cambria", 0.4156],
  ["caladea", 0.4156],
  ["arial", 0.431],
  ["helvetica", 0.431],
  ["liberation sans", 0.431],
]);

/** The advance to use for a face whose metrics are not known here, in em.
 *
 *  Stated as an APPROXIMATION WITH ITS ERROR rather than as a constant with no
 *  source: the spread across the four bundled base text faces is 0.393-0.431 em,
 *  under 10%, so 0.40 em sits in the middle of the measured range and a cap
 *  computed from it is within about ±4% of one computed from the real face. That
 *  is nowhere near the 3× discrepancy this cap exists to close. */
export const MEAN_ADVANCE_EM_FALLBACK = 0.4;

/** Above this mean advance a face is treated as FULL-WIDTH, i.e. CJK, and WCAG
 *  1.4.8's 40-glyph target applies instead of 80.
 *
 *  Measured rather than guessed at from a family name: a CJK face's ideographs
 *  are one em wide by construction, so the mean advance over running text is
 *  near 1.0 em, while every Latin text face measured above is under 0.44. 0.70
 *  is the midpoint of that gap and nothing measured sits near it.
 *
 *  **Reachable but not reached today, and that is a FONT gap rather than a logic
 *  gap**: this engine bundles no CJK face, so `MEAN_ADVANCE_EM` has no full-width
 *  entry and the fallback is Latin-shaped. The rule is unit-tested against a
 *  synthetic 1.0 em face so it is a live branch and not a comment; the moment a
 *  CJK face is bundled and measured, or the seam grows a mean-advance getter, the
 *  40 applies with no change here. Recorded in the report rather than left to be
 *  discovered. */
export const FULL_WIDTH_ADVANCE_EM = 0.7;

/** WCAG 2.1 SC 1.4.8's two numbers, which are the Reading step's targets. */
export const READING_TARGET_CHARS = 80;
export const READING_TARGET_CHARS_CJK = 40;

/** The five width steps, per-viewer, and what each one caps the column at.
 *
 *  WCAG 1.4.8 asks for "a mechanism" and does not say how many rungs it has:
 *  Google Docs offers Narrow/Medium/Wide (secondary sources add Full), Word's
 *  Immersive Reader four (Very Narrow/Narrow/Moderate/Wide). Labelled by what
 *  they do rather than by a number, with the targets stated here in the code.
 *
 *  Each step is one `X` in `min(available, X)` — **ONE mechanism, five values**,
 *  not five layout paths. `LayoutView::Reflow` is untouched by all of them.
 *
 *  | step | `X` | where the number comes from |
 *  | --- | --- | --- |
 *  | `narrow` | 55 characters | NOT a sourced number; see below |
 *  | `reading` | 80 characters (40 CJK) | WCAG 2.1 SC 1.4.8 (AAA) |
 *  | `fit` | the document's own text measure | the document. Invents no constant. |
 *  | `wide` | the document's own PAGE width | the document. **The default.** |
 *  | `full` | uncapped | the pre-cap behaviour, named on purpose |
 *
 *  `wide` is the page with its margins taken away, which is how pageless is
 *  described: Google's announcement says it adds *"more horizontal space for
 *  content like tables and images"*, and TechRepublic's walk-through (secondary)
 *  that it *"removes both the empty space around the page and the rigid
 *  page-based margins"* (`docs/166` §5, §7). Like `fit` it is DERIVED from the
 *  document rather than invented: the author's sheet, edge to edge. On Letter at
 *  100% that is 816 CSS px of text where `reading` gave 468 — which is the
 *  difference the owner reported as "the width of the page is too small".
 *
 *  `narrow`'s 55 is the one number here with no normative source, and it is a
 *  STEP and never a default for exactly that reason. It is offered because a
 *  mechanism whose only rungs are "the AAA bound" and "off" gives a reader who
 *  finds 80 too wide nowhere to go; it sits inside the 45-75 range that is
 *  attributed to Bringhurst and that `154` §7 could not verify, and it is
 *  labelled as a choice rather than published as a finding.
 *
 *  `fit` is ADR-048's *pageless authoring* policy: it never makes a line LONGER
 *  than the paper the author is writing for, which is the property an author
 *  needs and a reader does not care about. It ships labelled **Paper** rather
 *  than *Fit* because the same View band already carries Fit width and Fit page
 *  in its Zoom group, and a third Fit in one band is a worse menu than one
 *  accurate noun.
 *
 *  `full` is named on purpose and is not a confession: a host embedding the
 *  editor in a 400px column has the narrow case for free and may want the wide
 *  one, and a named Full makes the default's narrowness discoverable instead of
 *  mysterious. It is still bounded by `REFLOW_MAX_CONTENT_TWIP`, because the
 *  engine cannot shape a wider column and a refusal is not a width.
 *
 *  Each step carries its own catalogue KEYS rather than having them built from
 *  the id with a template literal. Two reasons and the second is the binding one:
 *  a key spelled out is a key `build-locale.mjs`'s extractor and
 *  `reflow_view.test.mjs`'s completeness guard can both see, and a `t()` called
 *  on an interpolated key is read by that extractor as the literal
 *  `textWidth.${step}.short` and fails the build. */
export const REFLOW_WIDTH_STEPS = Object.freeze([
  Object.freeze({
    id: "narrow",
    chars: 55,
    charsCjk: 28,
    shortKey: "textWidth.narrow.short",
    rowKey: "textWidth.narrow.row",
    titleKey: "textWidth.narrow.title",
    commandKey: "textWidth.narrow.command",
  }),
  Object.freeze({
    id: "reading",
    chars: READING_TARGET_CHARS,
    charsCjk: READING_TARGET_CHARS_CJK,
    shortKey: "textWidth.reading.short",
    rowKey: "textWidth.reading.row",
    titleKey: "textWidth.reading.title",
    commandKey: "textWidth.reading.command",
  }),
  Object.freeze({
    id: "fit",
    chars: null,
    charsCjk: null,
    shortKey: "textWidth.fit.short",
    rowKey: "textWidth.fit.row",
    titleKey: "textWidth.fit.title",
    commandKey: "textWidth.fit.command",
  }),
  Object.freeze({
    id: "wide",
    chars: null,
    charsCjk: null,
    shortKey: "textWidth.wide.short",
    rowKey: "textWidth.wide.row",
    titleKey: "textWidth.wide.title",
    commandKey: "textWidth.wide.command",
  }),
  Object.freeze({
    id: "full",
    chars: Infinity,
    charsCjk: Infinity,
    shortKey: "textWidth.full.short",
    rowKey: "textWidth.full.row",
    titleKey: "textWidth.full.title",
    commandKey: "textWidth.full.command",
  }),
]);

/** THE DESKTOP DEFAULT, and the ONE LINE that changes it.
 *
 *  `wide` — the document's own page width (`docs/151` §6.2a, ADR-048 as amended
 *  2026-10-06). It was `reading`, and ADR-048 recorded that call as the owner's;
 *  the owner has now made it: *"at present width of page is too small"*, against
 *  Google Docs' pageless view as the reference. Measured at 1440px, Reading gave
 *  a 469px column — NARROWER than the 624px text column the same document shows
 *  on paper, so turning the pages off took width away, which is the opposite of
 *  what pageless is for. Wide gives the page edge to edge (816px on Letter at
 *  100%), never asks for more than the window, and leaves the WCAG 1.4.8 step
 *  one click away for a reader who wants it.
 *
 *  Not `full`: an uncapped column at 1920px is 300+ characters, which `154`
 *  §3.2 measured and nobody wants by default. Not `fit`: the paper's own text
 *  column is what the reader was already looking at, so it is not "more room".
 *
 *  It is ONE default and not a per-device pair on purpose: on a phone every step
 *  reduces to `available` anyway, because a 390px window is narrower than the
 *  narrowest cap, so the phone's 60 characters and ADR-044's retired
 *  horizontal-scroll exemption are untouched by this value whatever it is. A
 *  phone-only evaluation is precisely what hid the missing cap (`154` §3.2), so
 *  the defaults are deliberately not split by device again. */
export const REFLOW_WIDTH_DEFAULT = "wide";

/** Where the width step lives: per-viewer, beside `docReflow`.
 *
 *  Per-viewer because that is what Google does with Text width and says in as
 *  many words — "Your text width choice won't affect how collaborators see your
 *  docs" — and because neither DOCX nor ODT has anywhere to put it, so a
 *  document property here would be a sidecar that reformats every
 *  collaborator's screen (ADR-048). */
export const REFLOW_WIDTH_PREF_KEY = "docReflowWidth";

/** The step `id` names, or the default when it names none. O(1). */
export function reflowWidthStep(id) {
  return (
    REFLOW_WIDTH_STEPS.find((step) => step.id === id) ??
    REFLOW_WIDTH_STEPS.find((step) => step.id === REFLOW_WIDTH_DEFAULT)
  );
}

/** The mean advance of `face`, in em: measured where it is known, the
 *  stated-error fallback where it is not. O(1).
 *
 * @param {string|null|undefined} face the document's default family, as
 *        `stylePreview("Normal")` reports it.
 */
export function meanAdvanceEm(face) {
  const key = String(face ?? "")
    .trim()
    .toLowerCase();
  return MEAN_ADVANCE_EM.get(key) ?? MEAN_ADVANCE_EM_FALLBACK;
}

/**
 * A character target resolved to twips, through the document's default face.
 *
 * `chars x mean_advance x size`. The em is the font size, so the twips per em
 * are `pt x 20`; nothing here depends on the zoom, because the cap is a measure
 * in the DOCUMENT's space and the zoom is a property of the glass.
 *
 * Complexity: O(1).
 *
 * @param {number} chars the target, in characters of the default face.
 * @param {number} fontSizePt the default size in points. 11 is Word's default.
 * @param {number} advanceEm the mean advance per character, in em.
 * @returns {number} the cap in twips, or `Infinity` for an uncapped target.
 */
export function charTargetTwip(chars, fontSizePt, advanceEm) {
  if (!(chars > 0) || !(fontSizePt > 0) || !(advanceEm > 0)) return Infinity;
  if (!Number.isFinite(chars)) return Infinity;
  return Math.round(chars * advanceEm * fontSizePt * 20);
}

/**
 * THE CAP: the `X` in `min(available, X)` for one width step and one document.
 *
 * This is the whole of ADR-048's policy layer. It returns a number of twips and
 * nothing else, so the clamp below it is one `Math.min` and the engine needs no
 * change at all — `LayoutView::Reflow { content_width }` already takes the
 * measure as a parameter, which is why ADR-046 getting the seam right is what
 * makes this cheap.
 *
 * Complexity: O(1). Safe to call once per render pass; it reads no document.
 *
 * @param {string} stepId one of `REFLOW_WIDTH_STEPS`.
 * @param {{face?: string|null, fontSizePt?: number, docMeasureTwip?: number|null,
 *          docPageTwip?: number|null}} doc
 *        the document's default face and size (`stylePreview("Normal")`), its
 *        own text measure for the `fit` step and its page width for `wide`. Each
 *        may be missing: a cap is a reading comfort and must never be the reason
 *        a document fails to lay out, so an unknown face falls back and an
 *        unknown measure falls through to `Infinity` — i.e. to `available`,
 *        which is where this started.
 * @returns {number} twips, possibly `Infinity`.
 */
export function reflowCapTwip(stepId, { face, fontSizePt, docMeasureTwip, docPageTwip } = {}) {
  const step = reflowWidthStep(stepId);
  if (step.id === "fit") return docMeasureTwip > 0 ? docMeasureTwip : Infinity;
  if (step.id === "wide") return docPageTwip > 0 ? docPageTwip : Infinity;
  const advanceEm = meanAdvanceEm(face);
  const chars = advanceEm >= FULL_WIDTH_ADVANCE_EM ? step.charsCjk : step.chars;
  return charTargetTwip(chars, fontSizePt > 0 ? fontSizePt : 11, advanceEm);
}

/** The bucket `px` falls in: the largest multiple of `quantum` that is not
 *  wider than it. Floored, never nearest — see the header.
 *
 *  Complexity: O(1).
 *
 * @param {number} px
 * @param {number} [quantum]
 * @returns {number} 0 for a width that is not a usable positive number.
 */
export function quantiseReflowWidth(px, quantum = REFLOW_QUANTUM_PX) {
  if (!Number.isFinite(px) || px <= 0 || !(quantum > 0)) return 0;
  return Math.floor(px / quantum) * quantum;
}

/**
 * The engine's three arguments, from the space the shell actually has — and
 * ONE CLAMP, which is the whole of ADR-048 at the call site.
 *
 * The tile's TOTAL width — `content + 2 * gutter` in the engine's synthetic
 * page — is what gets painted, so below the cap it is the quantised bucket that
 * has to be the total, not the content. Getting that the other way round is how
 * a gutter becomes an overflow.
 *
 * `contentWidthTwip = min(available, capTwip, REFLOW_MAX_CONTENT_TWIP)`. Two
 * ceilings, two different jobs, and they are kept apart on purpose: `capTwip` is
 * a READING COMFORT chosen by the viewer and may be `Infinity`;
 * `REFLOW_MAX_CONTENT_TWIP` is the engine's hard bound and is never waived, so
 * `full` is a width rather than a refusal. See both constants.
 *
 * ABOVE THE CAP THE RETURNED `totalPx` STOPS MOVING, which is deliberate and is
 * what the width feed reads: a desktop resize that changes only how much desk
 * surrounds the column produces the same bucket, so it costs one division and no
 * document work at all. That is a second and better reason for the §6.2
 * quantisation's O(1) guarantee, and it holds for every window width above the
 * cap rather than only inside a 16px bucket.
 *
 * Complexity: O(1). Called once per render and once per settled resize.
 *
 * @param {number} clientWidthPx the scroller's content box, excluding its own
 *        scrollbar — `clientWidth`, not `getBoundingClientRect().width`.
 * @param {number} cssPerTwip CSS px per twip at the current zoom, exactly as
 *        `renderAll` computes it. The zoom belongs in here: at 150% the reader
 *        wants bigger text in the same window, which is fewer twips of measure,
 *        and that falls out of this conversion rather than needing a rule.
 * @param {{quantum?:number, gutterPx?:number, minContentTwip?:number,
 *          capTwip?:number, maxContentTwip?:number}} [options]
 * @returns {{totalPx:number, availablePx:number, contentWidthTwip:number,
 *            gutterTwip:number, capped:boolean}|null}
 *          `null` when there is no usable width yet (a viewport that has not
 *          been laid out reports 0), so the caller leaves the view alone rather
 *          than asking the engine for a column of nothing. `totalPx` is the
 *          PAINTED tile's width; `availablePx` is the bucket the window offered,
 *          and the two differ exactly when `capped`.
 */
export function reflowMeasure(clientWidthPx, cssPerTwip, options = {}) {
  const quantum = options.quantum ?? REFLOW_QUANTUM_PX;
  const gutterPx = options.gutterPx ?? REFLOW_GUTTER_PX;
  const minContentTwip = options.minContentTwip ?? REFLOW_MIN_CONTENT_TWIP;
  const capTwip = options.capTwip ?? Infinity;
  const maxContentTwip = options.maxContentTwip ?? REFLOW_MAX_CONTENT_TWIP;
  const availablePx = quantiseReflowWidth(clientWidthPx, quantum);
  if (availablePx <= 0 || !(cssPerTwip > 0)) return null;
  const gutterTwip = Math.max(0, Math.round(gutterPx / cssPerTwip));
  // Floored, so the two gutters and the column can never add up to more than
  // the bucket after rounding — a rounded-up content width is a one-twip
  // overflow, which is invisible until it is a scrollbar.
  const availableTwip = Math.floor(availablePx / cssPerTwip) - 2 * gutterTwip;
  if (availableTwip < minContentTwip) {
    // Below the engine's floor the column stops shrinking rather than being
    // refused: see REFLOW_MIN_CONTENT_TWIP. The gutter goes with it, because a
    // gutter wider than the column it pads is the other thing the seam refuses.
    // No cap is consulted here: every cap is wider than an inch, so a floor and
    // a ceiling can never both bind, and asking would only invite them to.
    return {
      totalPx: availablePx,
      availablePx,
      contentWidthTwip: minContentTwip,
      gutterTwip: 0,
      capped: false,
    };
  }
  const contentWidthTwip = Math.min(availableTwip, capTwip, maxContentTwip);
  const capped = contentWidthTwip < availableTwip;
  return {
    totalPx: capped ? (contentWidthTwip + 2 * gutterTwip) * cssPerTwip : availablePx,
    availablePx,
    contentWidthTwip,
    gutterTwip,
    capped,
  };
}

/**
 * Whether reflow can be entered at all, and the sentence to say when it cannot.
 *
 * ONE condition today, and it is the engine's, not a guess: `setLayoutView`
 * refuses a body laid out one page-window at a time, because reflow's defining
 * promise is that the document stays editable in it and a windowed body is
 * already read-only. The shell cannot ask the engine that question without
 * calling the setter — there is no `reflowUnavailableReason` getter — but
 * `editingUnavailableReason` is non-empty under EXACTLY the same predicate
 * (`self.layout.is_windowed()` in both), so it is a faithful read rather than a
 * shadow copy. If the two ever diverge, the call site still catches the throw
 * and shows the engine's own words, so the failure mode is a late honest
 * message rather than a dead control.
 *
 * Complexity: O(1).
 *
 * @param {string} editingUnavailableReason the engine getter, "" when editable.
 * @param {string} withheldSentence the localised reason to show when it is not.
 * @returns {{available: boolean, reason: string}}
 */
export function reflowAvailability(editingUnavailableReason, withheldSentence) {
  const windowed = String(editingUnavailableReason ?? "") !== "";
  return { available: !windowed, reason: windowed ? withheldSentence : "" };
}

/**
 * The width feed: quantise, then debounce, then only if the bucket moved.
 *
 * The order is the design. Quantising FIRST is what makes the common case free;
 * debouncing first would still wake up once per gesture to discover nothing
 * changed, and would put a 150ms lag on the answer "nothing to do".
 *
 * Injected timers rather than `setTimeout` directly, so the coalescing is
 * answerable in `node` against a fake clock — the behaviour this module exists
 * to make cheap is precisely the behaviour a browser test cannot assert without
 * waiting on a wall clock, which `SKILL.md` §6 names as its own failure mode.
 *
 * @param {{
 *   onSettled: (bucketPx: number) => void,
 *   debounceMs?: number,
 *   quantum?: number,
 *   setTimer?: (fn: () => void, ms: number) => any,
 *   clearTimer?: (handle: any) => void,
 * }} deps
 * @returns {{observe: (px:number) => boolean, adopt: (px:number) => void,
 *            cancel: () => void, current: () => number, pending: () => boolean}}
 */
export function createWidthFeed({
  onSettled,
  debounceMs = REFLOW_DEBOUNCE_MS,
  quantum = REFLOW_QUANTUM_PX,
  setTimer = (fn, ms) => setTimeout(fn, ms),
  clearTimer = (handle) => clearTimeout(handle),
}) {
  /** The bucket the ENGINE is currently laid out at. -1 until the first pass. */
  let applied = -1;
  let timer = null;

  function cancel() {
    if (timer !== null) clearTimer(timer);
    timer = null;
  }

  return {
    /** Record that the engine now holds `px`. Called by the render pass, which
     *  is the only thing that actually talks to the setter. */
    adopt(px) {
      cancel();
      applied = quantiseReflowWidth(px, quantum);
    },
    /** Feed a live width. Returns true when a settled pass has been scheduled —
     *  which is exactly "this event was NOT free". */
    observe(px) {
      const bucket = quantiseReflowWidth(px, quantum);
      if (bucket === applied) {
        // The whole point: a resize inside the bucket the engine already holds
        // costs one division and one comparison, and nothing in the document is
        // touched. A pass scheduled by an earlier width is CANCELLED rather than
        // left standing — a reader who drags out and back has changed nothing,
        // and a timer still holding the furthest width would relayout to a size
        // the window no longer is. The LAST width wins, including when the last
        // width is the one already in effect.
        cancel();
        return false;
      }
      cancel();
      timer = setTimer(() => {
        timer = null;
        onSettled(bucket);
      }, debounceMs);
      return true;
    },
    cancel,
    current: () => applied,
    pending: () => timer !== null,
  };
}
