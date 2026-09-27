// "Ask the user for one line of text" — once, for every dialog that does it.
//
// Two surfaces need exactly this: Home ▸ Styles ▸ "Create a style from the
// selection", and version history's "Name this version" (`docs/139` §8.3).
// Before this module the first one was ~50 lines of `main.js` and the second
// would have been a second copy of them, which is the "prefer one mechanism over
// two" rule (SKILL §8) applied to the smallest possible case — and the case where
// the duplication would have been hardest to see, because the copy would have
// looked correct.
//
// `window.prompt` is not an option here, and not only because it is ugly. The
// owner's ranked list of interactions that are NOT Word/Docs standard has
// "hyperlink = `window.prompt`" at number one; a browser prompt cannot be
// labelled, cannot be localised, freezes the wasm engine's event loop, and is
// increasingly refused outright by browsers, so a cancelled prompt and a
// suppressed prompt are indistinguishable.
//
// The subtle part, and the reason this is a module rather than a copied block:
// the modal primitive knows nothing about our promise, so Escape, the backdrop
// and ✕ close the dialog without passing through our own close path. The result
// is therefore STAGED and read back in `onClose`, where every route out
// converges. Getting that wrong leaves the promise pending forever, which is a
// dialog the user has dismissed and a command that never finishes.

/**
 * Wires one name dialog and returns the function that opens it.
 *
 * Element ids are passed in rather than hard-coded, because the whole point is
 * that there is more than one of these cards; the SHAPE is fixed (a title, one
 * input, Cancel and a primary confirm) and `dialog-i18n-keys.spec.mjs` already
 * holds every dialog to it.
 *
 * @param {object} deps
 * @param {(element: Element, options: object) => object} deps.registerModal the
 *        editor's modal contract.
 * @param {() => Element} deps.fallbackFocus where focus goes when whatever opened
 *        the card is gone by the time it closes.
 * @param {{dialog: string, input: string, confirm: string, cancel: string,
 *          close: string}} deps.ids the card's element ids.
 * @returns {{prompt: (initial?: string) => Promise<string|null>}} `prompt`
 *          resolves to the trimmed text, or to `null` for every form of
 *          dismissal. Bounding and sanitising the text is the CALLER's job: this
 *          module does not know whether it is naming a style or a version, and
 *          the two have different rules.
 */
export function createNamePrompt({ registerModal, fallbackFocus, ids }) {
  const dialog = document.getElementById(ids.dialog);
  const input = document.getElementById(ids.input);
  if (!dialog || !input) return { prompt: () => Promise.resolve(null) };
  const confirmBtn = document.getElementById(ids.confirm);
  const cancelBtn = document.getElementById(ids.cancel);
  const closeBtn = document.getElementById(ids.close);
  let resolveAsked = null;
  /** The value the dialog will resolve with when it closes — staged, never
   *  resolved directly, so a dismissal that skipped our close path still
   *  settles the promise. */
  let staged = null;

  const controller = registerModal(dialog, {
    initialFocus: () => input,
    fallbackFocus,
    defaultAction: () => finish(input.value.trim()),
    onClose: () => {
      const resolve = resolveAsked;
      const result = staged;
      resolveAsked = null;
      staged = null;
      if (resolve) resolve(result);
    },
  });

  function finish(result) {
    staged = result;
    controller.close();
  }

  confirmBtn?.addEventListener("click", () => finish(input.value.trim()));
  cancelBtn?.addEventListener("click", () => finish(null));
  closeBtn?.addEventListener("click", () => finish(null));

  return {
    /** Opens the card and resolves to the entered text, or null if dismissed.
     *  `initial` pre-fills it, which is what makes the same card a RENAME:
     *  a rename that starts empty makes the user retype what they already had.
     *  A single in-flight prompt at a time. O(1). */
    prompt(initial = "") {
      return new Promise((resolve) => {
        resolveAsked = resolve;
        staged = null;
        input.value = initial;
        controller.open();
      });
    },
  };
}
