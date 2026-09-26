// The References tab's caption machinery, wired in one place.
//
// Insert caption, Cross-reference and Update caption numbers are three controls
// over one engine surface, and the third one exists because of a decision in the
// second: inserting a caption renumbers the later captions of the same label in
// the SAME undoable action, but DELETING a caption paragraph deliberately does
// not — renumbering on Backspace would make a keystroke O(document), which the
// editing budget forbids (SKILL.md §8, `docs/107` §4). So a document can be
// "dirty rather than silently stale", and `staleCaptionNumbers()` is how the
// product notices.
//
// That notice is itself a document walk, which is exactly why it is CACHED here
// rather than called from the ribbon's enablement path: `updateToolbar()` runs on
// every keystroke and every caret move, so asking the engine there would make a
// keystroke O(document) — the same defect `documentOutline` shipped with, and the
// reason `main-thread-budget.spec.mjs` exists. The cache is refreshed on the
// interactions that can change the answer and that a user performs
// deliberately: opening a document, inserting or updating captions, and
// switching to the References tab — which is the only way to reach the button.
import { t } from "./i18n.mjs";
import { createCaptionDialog } from "./caption_dialog.mjs";
import { createCrossReferenceDialog } from "./cross_reference_dialog.mjs";

/** The References rows Word and ONLYOFFICE put on the picture, table and
 *  equation right-click menus (`DocumentHolderExt.js:46` — Insert caption, at the
 *  top, ahead of their own separator).
 *
 *  Derived from whichever rows of the References surface declare `contextMenu`,
 *  so the ribbon button, the palette row and both menus cannot disagree about the
 *  command's label or about whether it is available — the restated-row defect
 *  `docs/105` UX-004 keeps finding. Deliberately NOT on the prose menu: neither
 *  Word nor ONLYOFFICE offers a caption where there is nothing to caption.
 *
 *  `enabled` and `reason` are the surface's own two rules, passed in so this
 *  answers with exactly what the ribbon would answer. */
export function objectMenuRows(surface, enabled, reason) {
  return surface
    .filter((entry) => entry.contextMenu)
    .map((entry) => ({
      id: entry.command,
      label: entry.label,
      group: "caption",
      enabled: enabled(entry),
      disabledReason: reason(entry),
      run: entry.run,
    }));
}

/**
 * Builds both dialogs and the caption-numbering controller.
 *
 * `io` is the application state all three need:
 *
 *   `getDoc()`          the open document, or null
 *   `applyEdit(fn)`     Promise<boolean>; runs `fn` through the gated, tracked,
 *                       undoable edit path (one Undo, fail-closed in Viewing and
 *                       in Suggesting)
 *   `mutationBlocked()` true (having said why) when the review mode refuses
 *   `targetNode()`      the block a caption attaches to
 *   `caret()`           `{ node, offset }`, or null
 *   `status(text, kind)` the status line
 *   `registerModal(dialog, options)`
 *   `fallbackFocus()`
 */
export function createReferenceCommands(io) {
  /** How many captions currently show a number the document disagrees with.
   *  Read by the ribbon; never computed by it. */
  let staleCount = 0;

  const numbering = {
    get count() {
      return staleCount;
    },

    /** ONE document walk. Called only from the deliberate interactions listed in
     *  this module's header, never from `updateToolbar`. */
    refresh() {
      const doc = io.getDoc();
      if (!doc?.staleCaptionNumbers) {
        staleCount = 0;
        return;
      }
      try {
        staleCount = doc.staleCaptionNumbers().length;
      } catch {
        // A build whose engine predates the binding, or a document the walk
        // cannot answer for: "nothing to fix" is the honest state, and the
        // button says so rather than offering an action that would throw.
        staleCount = 0;
      }
    },

    /** Rewrites every caption's number in one undoable action. The engine
     *  refuses with a readable message when nothing is stale, which is a state
     *  the disabled button should have prevented — surfacing it rather than
     *  swallowing it is what makes the cache's staleness visible. */
    async update() {
      const doc = io.getDoc();
      if (!doc || io.mutationBlocked()) return;
      const applied = await io.applyEdit(() => doc.updateCaptionNumbers());
      numbering.refresh();
      if (applied) io.status(t("caption.numbersUpdated"));
    },
  };

  const shared = {
    registerModal: io.registerModal,
    getDoc: io.getDoc,
    status: io.status,
    fallbackFocus: io.fallbackFocus,
    mutationBlocked: io.mutationBlocked,
  };

  const caption = createCaptionDialog({
    ...shared,
    targetNode: io.targetNode,
    insert: (o) =>
      io.applyEdit(() =>
        io
          .getDoc()
          .insertCaption(
            o.targetNode,
            o.label,
            o.text,
            o.position,
            o.excludeLabel,
            o.numberFormat,
            o.chapterLevel,
            o.separator,
          ),
      ),
    // An insert renumbers the later captions of the same label, so what is
    // "stale" changes with it.
    inserted: () => numbering.refresh(),
  });

  const crossReference = createCrossReferenceDialog({
    ...shared,
    caret: io.caret,
    // Detected from the binding's arity, so the include-above/below switch comes
    // alive the moment the engine grows its sixth parameter.
    aboveBelowSupported: () => (io.getDoc()?.insertCrossReference?.length ?? 0) >= 6,
    insert: (o) =>
      io.applyEdit(() =>
        io
          .getDoc()
          .insertCrossReference(
            o.caretNode,
            o.caretOffset,
            o.targetNode,
            o.referenceTo,
            o.hyperlink,
            o.includeAboveBelow,
          ),
      ),
  });

  return { caption, crossReference, numbering };
}
