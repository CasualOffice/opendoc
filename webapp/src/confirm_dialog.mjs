// The application's single yes/no question.
//
// `window.alert` / `confirm` / `prompt` are barred in this editor, and not as a
// style preference: they cannot be styled or labelled, they freeze the wasm
// engine's event loop while they are up, and browsers increasingly refuse them
// outright — so a "confirmation" the user never saw would silently read as
// cancelled. This card goes through the same modal contract as every other
// dialog, which means Escape and the backdrop already answer it, and answer it
// as "no".
//
// Extracted from `main.js` to pay for the version history panel under the line
// ratchet (`module_seams.test.mjs`): the file was AT its ceiling with zero slack,
// and the honest ways to go green are to put the new code in a module or to take
// something else out. This is the something else, and it is the right thing to
// take: a confirmation card decides nothing that needs the application — it is a
// promise, four elements and one modal controller — and it is now the second
// caller of the same shape `name_dialog.mjs` uses, so the two dialogs that stage
// a result and read it back in `onClose` do it once rather than twice.
//
// The staging is the subtle part and the reason both live in modules now. The
// modal primitive knows nothing about our promise, so Escape, the backdrop and ✕
// all close the dialog without passing through our own close path. The answer is
// therefore staged in a variable and read back in `onClose`, where EVERY route
// out converges — a dismissal that skipped the close path used to leave the
// promise pending forever.

/**
 * Wires the confirmation card and returns the one function that asks with it.
 *
 * @param {object} deps
 * @param {(element: Element, options: object) => object} deps.registerModal the
 *        editor's modal contract (focus trap, Escape, backdrop, chord lock).
 * @param {() => Element} deps.fallbackFocus where focus goes when the element
 *        that opened the card is gone by the time it closes.
 * @returns {{ask: (options: object) => Promise<boolean>, isOpen: () => boolean}}
 */
export function createConfirmDialog({ registerModal, fallbackFocus }) {
  const dialog = document.getElementById("confirmDialog");
  const titleEl = document.getElementById("confirmTitle");
  const descriptionEl = document.getElementById("confirmDescription");
  const noteEl = document.getElementById("confirmNote");
  const iconEl = document.getElementById("confirmIcon");
  const acceptBtn = document.getElementById("confirmAccept");
  const cancelBtn = document.getElementById("confirmCancel");
  const closeBtn = document.getElementById("confirmClose");
  let resolveAsked = null;
  let answer = false;

  const controller = registerModal(dialog, {
    // Cancel holds focus, so Enter and Space answer "no". Every question this
    // card asks is asked because the alternative destroys something; the safe
    // answer is the one a reflexive keypress lands on.
    initialFocus: () => cancelBtn,
    fallbackFocus,
    defaultAction: () => finish(true),
    onClose: () => {
      const resolve = resolveAsked;
      const value = answer;
      resolveAsked = null;
      answer = false;
      // Escape, the backdrop and ✕ never set an answer, so they resolve false —
      // dismissal is a refusal, not a silent yes.
      if (resolve) resolve(value);
    },
  });

  function finish(value) {
    answer = value;
    controller.close();
  }

  acceptBtn.addEventListener("click", () => finish(true));
  cancelBtn.addEventListener("click", () => finish(false));
  closeBtn.addEventListener("click", () => finish(false));

  return {
    /** Asks one yes/no question and resolves to the user's answer. Resolves
     *  false for every form of dismissal. A second call while one is open
     *  resolves the first as refused rather than stacking two questions.
     *  O(1). */
    ask({ title, message, confirmLabel = "OK", cancelLabel = "Cancel", note = "", icon = "help" }) {
      if (controller.isOpen) finish(false);
      titleEl.textContent = title;
      descriptionEl.textContent = message;
      noteEl.textContent = note;
      iconEl.textContent = icon;
      acceptBtn.textContent = confirmLabel;
      cancelBtn.textContent = cancelLabel;
      return new Promise((resolve) => {
        resolveAsked = resolve;
        answer = false;
        controller.open();
      });
    },
    isOpen: () => controller.isOpen,
  };
}
