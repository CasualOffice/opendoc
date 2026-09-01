// The editor's one modal contract (docs/104 theme T-04).
//
// Nine dialogs used to hand-roll their own dismissal, and they diverged: Split
// cell ignored Escape, backdrop clicks and Enter; the style-name prompt lost
// Escape as soon as you tabbed off its input; only six of the nine trapped Tab
// at all, so the rest let the keyboard walk out into the ribbon behind their own
// dimmed backdrop; `syncModalLock` knew about exactly two of them, so ⌘F opened
// Find *behind* whichever modal was on screen. Every one of those is the same
// defect — a rule that enumerates its subjects is one omission from being wrong.
//
// So the rule lives here instead, once, and every modal gets it by registering:
//
//   * opening moves focus INTO the dialog and remembers what had it;
//   * Escape, a backdrop click, and close/cancel are ONE path — they close and
//     put focus back where it came from;
//   * Tab cycles inside the dialog and cannot leave it, including recovering
//     focus that has already escaped;
//   * application shortcuts do not fire while a modal is open, but text-editing
//     chords still reach the dialog's own fields;
//   * the topmost modal owns the keyboard, so nesting is well-defined.
//
// The primitive ADOPTS existing markup rather than generating it. The dialogs
// are authored in editor.html with their real labels, `aria-labelledby` and
// `aria-describedby` wiring; regenerating that from JS would trade a solved
// accessibility problem for an unsolved one.

/** Open modals, innermost last. The array is the single source of truth for
 *  "is a modal open" — nothing else may answer that question. */
const stack = [];

/** Chords that belong to the focused text field rather than to the
 *  application. Everything else with a modifier is refused while a modal is
 *  open: an application command that fires behind a blocking dialog is the
 *  HF-063 defect, and refusing an unknown chord is the safe direction — the
 *  user loses one keystroke, instead of losing sight of where their keystrokes
 *  are going. Undo/redo are included because a dialog's fields are editable. */
const TEXT_EDITING_CHORDS = new Set(["a", "c", "v", "x", "z", "y"]);

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not(:disabled)",
  "input:not(:disabled):not([type='hidden'])",
  "select:not(:disabled)",
  "textarea:not(:disabled)",
  "[tabindex]:not([tabindex='-1'])",
].join(", ");

let listenersInstalled = false;
let onFirstOpen = null;

/** Elements already registered, so a double registration is loud. */
const registered = new WeakSet();

/** Hook run when the stack goes empty → non-empty. The editor uses it to put
 *  away chrome that must never paint over a modal (docs/104 HF-089: a pinned
 *  tracked-change card floated above the scrim with live Accept/Reject
 *  buttons). One hook beats teaching every open path about every popover. */
export function setModalHooks({ firstOpen } = {}) {
  onFirstOpen = typeof firstOpen === "function" ? firstOpen : null;
}

/** True while any registered modal is open. The predicate every global
 *  shortcut and outside-click handler should consult. */
export function modalIsOpen() {
  return stack.length > 0;
}

/** The modal that currently owns the keyboard, or null. */
function top() {
  return stack.length ? stack[stack.length - 1] : null;
}

/** Visible, enabled, focusable descendants in DOM order. `getClientRects` is
 *  the cheap "is it actually on screen" test — contextual ribbon panels hide
 *  whole groups, and focusing something invisible is how the editor used to
 *  appear frozen after Split cell closed (HF-062). */
function focusable(element) {
  return [...element.querySelectorAll(FOCUSABLE_SELECTOR)].filter(
    (node) => node.getClientRects().length > 0,
  );
}

function focusFirst(entry) {
  const explicit = entry.options.initialFocus?.();
  const target = explicit && explicit.getClientRects().length > 0 ? explicit : focusable(entry.element)[0];
  target?.focus({ preventScroll: true });
  return target ?? null;
}

/** Tab containment. Unlike the helper this replaces, it also recovers focus
 *  that is already outside — a trap that only fires at the first and last
 *  element has nothing to say once focus has left, which is exactly when it
 *  matters. */
function trapTab(event, entry) {
  const items = focusable(entry.element);
  if (items.length === 0) {
    // Nothing to focus inside, but the dialog is still modal: keep the keyboard
    // from wandering into the inert chrome behind it.
    event.preventDefault();
    return;
  }
  const first = items[0];
  const last = items[items.length - 1];
  const active = document.activeElement;
  if (!entry.element.contains(active)) {
    event.preventDefault();
    (event.shiftKey ? last : first).focus({ preventScroll: true });
    return;
  }
  if (event.shiftKey && active === first) {
    event.preventDefault();
    last.focus({ preventScroll: true });
  } else if (!event.shiftKey && active === last) {
    event.preventDefault();
    first.focus({ preventScroll: true });
  }
}

function installListeners() {
  if (listenersInstalled) return;
  listenersInstalled = true;

  // Capture phase, on the document, so this runs before any handler the editor
  // registered — including the ones that used to open Find or the palette from
  // inside a modal. Capture also means ordering does not depend on which module
  // happened to load first.
  document.addEventListener(
    "keydown",
    (event) => {
      const entry = top();
      if (!entry) return;

      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
        entry.controller.close("escape");
        return;
      }

      if (event.key === "Tab") {
        trapTab(event, entry);
        return;
      }

      if (event.key === "Enter" && entry.options.defaultAction) {
        // Only for dialogs with no <form> of their own: a form already turns
        // Enter into submit, and firing both would apply twice. Enter on a
        // control that has its own meaning (a button, a multi-line field, a
        // native select) belongs to that control.
        const target = event.target;
        const tag = target instanceof Element ? target.tagName : "";
        if (!["BUTTON", "TEXTAREA", "SELECT", "A"].includes(tag) && !event.isComposing) {
          event.preventDefault();
          entry.options.defaultAction();
        }
        return;
      }

      // A surface opened by a chord stays toggleable by that chord — otherwise
      // the lock below swallows the second press and strands it open.
      if (entry.options.toggleChord?.(event)) {
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
        entry.controller.close("toggle");
        return;
      }

      // Application shortcuts are inert while a modal is open. Text-editing
      // chords still reach the dialog's fields; everything else is swallowed
      // AND default-prevented, because merely stopping propagation would hand
      // ⌘S / ⌘P / ⌘F straight to the browser instead of to the editor.
      const modifier = event.metaKey || event.ctrlKey;
      if (modifier && !event.altKey) {
        const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
        if (!TEXT_EDITING_CHORDS.has(key)) {
          event.preventDefault();
          event.stopPropagation();
          event.stopImmediatePropagation();
        }
      }
    },
    true,
  );

  // Last line of the trap: anything that moves focus out of the topmost modal
  // without going through Tab (a programmatic `.focus()`, a repaint that
  // detaches the focused node) is pulled back. Without this the dialog can
  // still be left claiming `aria-modal` while the keyboard is somewhere else.
  document.addEventListener("focusin", (event) => {
    const entry = top();
    if (!entry || entry.closing) return;
    if (entry.element.contains(event.target)) return;
    focusFirst(entry);
  });
}

/** Registers `element` (an existing `.dialog-overlay`-style node) as a modal and
 *  returns its controller. One registration per element; a second throws rather
 *  than quietly double-binding its listeners.
 *
 *  Options:
 *    initialFocus()  — the control to focus on open; defaults to the first
 *                      focusable descendant.
 *    fallbackFocus() — where focus goes when the opener has gone away (the
 *                      object context bar is rebuilt on every repaint, so its
 *                      buttons detach while a dialog is open).
 *    defaultAction() — what Enter does, for dialogs with no <form>.
 *    toggleChord(e)  — the chord that opened this surface, so pressing it again
 *                      closes it instead of being swallowed by the lock.
 *    onOpen()        — run after the element is shown and focused.
 *    onClose(reason) — run after the element is hidden, before focus is
 *                      restored. Reasons: "escape", "backdrop", or whatever the
 *                      caller passes to close().
 */
export function registerModal(element, options = {}) {
  installListeners();
  if (registered.has(element)) {
    throw new Error(`modal #${element.id || "(anonymous)"} is already registered`);
  }
  registered.add(element);

  const entry = {
    element,
    options,
    returnFocus: null,
    closing: false,
    controller: null,
    backdropArmed: false,
  };

  // Backdrop dismissal requires press AND release on the backdrop. Closing on
  // mousedown alone throws the dialog away when a drag that started inside a
  // text field happens to end on the scrim — a real way to lose typed input.
  element.addEventListener("mousedown", (event) => {
    entry.backdropArmed = event.target === element;
  });
  element.addEventListener("click", (event) => {
    const armed = entry.backdropArmed;
    entry.backdropArmed = false;
    if (armed && event.target === element) entry.controller.close("backdrop");
  });

  const controller = {
    get isOpen() {
      return stack.includes(entry);
    },

    /** Shows the dialog, remembers the opener, and moves focus in. */
    open() {
      if (controller.isOpen) return controller;
      const wasEmpty = stack.length === 0;
      entry.returnFocus =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
      entry.closing = false;
      element.hidden = false;
      stack.push(entry);
      document.body.classList.add("modal-open");
      if (wasEmpty) onFirstOpen?.();
      // Deferred one microtask: the dialog's own open path often fills fields
      // immediately after calling this, and focusing a control it is about to
      // rewrite loses the caret position.
      queueMicrotask(() => {
        if (!controller.isOpen) return;
        focusFirst(entry);
        options.onOpen?.();
      });
      return controller;
    },

    /** Hides the dialog and returns focus to whatever opened it. */
    close(reason = "close") {
      const index = stack.indexOf(entry);
      if (index === -1) return controller;
      entry.closing = true;
      stack.splice(index, 1);
      element.hidden = true;
      if (stack.length === 0) document.body.classList.remove("modal-open");
      options.onClose?.(reason);
      const opener = entry.returnFocus;
      entry.returnFocus = null;
      // The opener can have been removed or hidden while the dialog was up, in
      // which case focusing it silently drops the keyboard onto <body> and the
      // editor looks frozen. Fall back deliberately rather than hope.
      if (opener && opener.isConnected && opener.getClientRects().length > 0) {
        opener.focus({ preventScroll: true });
      } else {
        options.fallbackFocus?.()?.focus({ preventScroll: true });
      }
      entry.closing = false;
      return controller;
    },
  };

  entry.controller = controller;
  return controller;
}
