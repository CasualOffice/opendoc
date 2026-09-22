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

/** The engine's marker for a refusal it has ALREADY written as a sentence for
 *  the reader.
 *
 *  Most engine errors are internal vocabulary and must be translated here. But
 *  some refusals are decisions only the engine can explain — a document whose
 *  `w:documentProtection` permits nothing but its form fields is refused for a
 *  reason that has no equivalent on this side, and "that edit isn't supported
 *  for this selection yet" is actively wrong about it: the selection is fine,
 *  the DOCUMENT is locked, and the reader is left hunting a selection that
 *  will never work.
 *
 *  So the engine marks those and they pass through verbatim. A prefix rather
 *  than a list of remembered sentences, because a list is a second place to
 *  update and the next such refusal would silently fall back to the generic
 *  one — the defect this is fixing. */
const EXPLAINED = /^refused: /;

/** Viewing mode: a choice the reader made and can unmake, so the sentence says
 *  how. It is only true when the DOCUMENT is not itself read-only. */
const VIEWING = "Viewing mode is read-only; switch to Editing to change the document";

/**
 * What to say when a mutation is refused BEFORE it reaches the engine.
 *
 * The host blocks every mutation in Viewing mode at one choke point, and tells
 * the reader to switch to Editing. For a document the engine will not let
 * anyone edit — one too large to lay out whole (`docs/113` §8.3) — that is an
 * instruction they cannot follow: the mode buttons are disabled, and switching
 * is not a thing that exists. Same policy function as the engine-refusal case
 * below, so the two can never drift into saying different things about the
 * same document.
 *
 * @param {{editingUnavailableReason?: string}} [context]
 * @returns {string}
 */
export function mutationBlockedMessage(context = {}) {
  return String(context.editingUnavailableReason ?? "") || VIEWING;
}

/**
 * The sentence to show for a thrown engine error.
 *
 * @param {unknown} error the value `catch` received; any shape, including null.
 * @param {{editingUnavailableReason?: string}} [context] what the HOST already
 *        knows about the document, independent of this particular throw.
 *        `editingUnavailableReason` is the engine's own getter (`docs/113`
 *        §8.3): a non-empty string means no edit can ever apply to this
 *        document, and it is already a user-facing sentence naming the size,
 *        the limit and what to do about it.
 * @returns {string} a user-facing sentence, never an engine error name.
 */
export function editRefusalMessage(error, context = {}) {
  // Checked FIRST, and from context rather than from the thrown value. When a
  // document cannot be edited at all, nothing about the selection is wrong and
  // nothing about a history step is either — "that edit isn't supported for
  // this selection yet" sends the reader to inspect a selection that is fine,
  // and to try again with a different one, forever. This is the whole reason
  // the getter exists (`SKILL.md` §10: say why, never refuse blankly).
  const unavailable = String(context.editingUnavailableReason ?? "");
  if (unavailable) return unavailable;
  const text = String(error?.message ?? error ?? "");
  if (EXPLAINED.test(text)) return text.replace(EXPLAINED, "");
  return HISTORY_FAILURE.test(text) ? HISTORY : GENERIC;
}
