// How an e2e asserts that a proofing mark is ABSENT without the assertion
// passing for the wrong reason (`docs/109` HF-417).
//
// "There is no squiggle on X" is true in three states that are not the feature
// working, and `text-language.spec.mjs` showed every one of them:
//
//   1. the checker has not run yet (a fresh open, a reload, a word list still in
//      flight) — so nothing is marked anywhere;
//   2. the overlay is between repaints — it is rebuilt from nothing on every
//      repaint, so for a moment it holds no marks at all;
//   3. the caret is inside X — `paint` never marks the word the caret is in
//      (`spell_check.mjs`, the word being typed is not flagged), and a
//      right-click or a double-click on the squiggle is exactly what puts the
//      caret there.
//
// A bare `expect(mark).toHaveCount(0)` auto-retries until the FIRST moment the
// count is zero, so it takes whichever of those comes first. The helpers here
// close all three: the caret is moved off the word by the caller (each spec
// says how), and the absence is read in the SAME overlay snapshot as a
// positive control — a mark that must be painted — so "zero" can only be read
// from an overlay the checker has painted. `tests/proofing_absence_guard.test.mjs`
// fails the build if a spec asserts a zero mark count any other way.
import { expect } from "./fixtures.mjs";

/** A spelling mark on `word`. */
export const spellMark = (word) => `.overlay .spell-error[data-spell-word="${word}"]`;
/** A grammar mark for `rule`. */
export const grammarMarkFor = (rule) => `.overlay .grammar-error[data-grammar-rule="${rule}"]`;
/** Every spelling mark / every grammar mark. */
export const ANY_SPELL_MARK = ".overlay .spell-error";
export const ANY_GRAMMAR_MARK = ".overlay .grammar-error";

/**
 * Waits until, in one overlay snapshot, `absent` matches nothing while
 * `control` matches at least one mark.
 *
 * `control` must be a mark the checker paints in the state under test — the
 * fixture's grammar line (`doubled-word`, on every page of `makeSpellingDocx`),
 * or another page's deliberate typo. If the control is never painted the wait
 * fails, which is the point: an absence nobody could have seen is not evidence.
 */
export async function expectMarkAbsent(page, absent, control, message = "") {
  const read = () =>
    page.evaluate(
      ([gone, seen]) => ({
        absent: document.querySelectorAll(gone).length,
        control: document.querySelectorAll(seen).length,
      }),
      [absent, control],
    );
  await expect
    .poll(
      async () => {
        const counts = await read();
        return counts.absent === 0 && counts.control > 0
          ? "absent from a painted overlay"
          : `absent=${counts.absent} control=${counts.control}`;
      },
      {
        message: message || `${absent} must be absent while ${control} is painted`,
        timeout: 20_000,
      },
    )
    .toBe("absent from a painted overlay");
}

/**
 * For a document nothing in which CAN be marked — every paragraph in a language
 * with no word list, where grammar (English-only) has nothing to say either —
 * there is no positive control. The checker's own status line is the evidence
 * it ran: it is written only once the scan has split the window by language.
 * The overlay is read after it, once.
 */
export async function expectNoMarksWithCheckerStatus(page, statusText) {
  await expect(page.locator("#status")).toHaveText(statusText, { timeout: 20_000 });
  const counts = await page.evaluate(
    ([spell, grammar]) => [document.querySelectorAll(spell).length, document.querySelectorAll(grammar).length],
    [ANY_SPELL_MARK, ANY_GRAMMAR_MARK],
  );
  expect(counts, `after "${statusText}" nothing may be marked`).toEqual([0, 0]);
}

/**
 * For the one state where NO mark can be painted — spelling and grammar both
 * off — there is no positive control. The checker's own state is the
 * evidence instead: both Review-band switches must report off before the
 * overlay's emptiness means anything.
 */
export async function expectNoMarksWithProofingOff(page) {
  await expect(page.locator("#reviewSpellCheckBtn")).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator("#reviewGrammarCheckBtn")).toHaveAttribute("aria-pressed", "false");
  const counts = await page.evaluate(
    ([spell, grammar]) => [document.querySelectorAll(spell).length, document.querySelectorAll(grammar).length],
    [ANY_SPELL_MARK, ANY_GRAMMAR_MARK],
  );
  expect(counts, "with both proofing switches off nothing may be marked").toEqual([0, 0]);
}
