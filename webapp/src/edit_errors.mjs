// How a refused engine edit is described to the person who asked for it.
//
// Extracted from `main.js` rather than added to it: the module is 93% of the
// webapp with zero exports (`109` HF-085), and message policy is exactly the
// kind of pure decision that can be unit-tested once it is out of there.
//
// The engine's error names — `Unsupported`, `CrossParagraph`, `EditError` —
// are internal vocabulary and must never reach the status bar (`docs/67`,
// "Error/reporting UX"). But one generic sentence for every refusal is its own
// defect: a refused undo has nothing to do with the selection, and saying it
// does sends the user looking in the wrong place.

/** The fallback: a refused edit the caller has nothing more specific to say about. */
const GENERIC = "That edit isn't supported for this selection yet";

/** A refused history step. `apply_group` restores the pre-edit document and
 *  pushes the entry back before returning this, so "unchanged" is a promise the
 *  engine actually keeps (`109` HF-045) — not a hopeful phrasing. */
const HISTORY = "That history step can no longer be applied — the document is unchanged";

/** The engine prefixes a refused history step with `undo failed:` / `redo failed:`. */
const HISTORY_FAILURE = /^(undo|redo) failed:/;

/**
 * The sentence to show for a thrown engine error.
 *
 * @param {unknown} error the value `catch` received; any shape, including null.
 * @returns {string} a user-facing sentence, never an engine error name.
 */
export function editRefusalMessage(error) {
  const text = String(error?.message ?? error ?? "");
  return HISTORY_FAILURE.test(text) ? HISTORY : GENERIC;
}
