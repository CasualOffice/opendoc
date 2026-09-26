// "Does this document hold any floating object?" — answered in O(1), because it
// is asked on a per-interaction path (`109` HF-183).
//
// THE DEFECT. `editorCommands()` offers `object.selectNext` /
// `object.selectPrevious` enabled only when there is something to select, and it
// answered that by calling the engine's `objectOrder()` and `JSON.parse`-ing the
// result — the whole paint order, to learn whether it is empty. `editorCommands()`
// is rebuilt on EVERY keystroke typed into the command palette (`renderCommands`
// → `buildCommands`) and, since the keymap dispatcher landed (`109` UX-006), on
// every chord press as well. So a yes/no question cost O(objects) per keystroke,
// against `docs/107` §4's rule that per-interaction work is O(1) in document
// size.
//
// THE KNOWN PATTERN, named before the code (SKILL.md §8): memoization with
// explicit invalidation at the single mutation choke point. Not a time-to-live,
// not a revision comparison that still has to ask the engine something — the
// editor already funnels every landed edit through one function
// (`noteDocumentEdited`), which is where `clearFindParagraphCache` is invalidated
// for exactly the same reason. This is that, for one boolean.
//
// WHY A CACHE RATHER THAN A CHEAPER ENGINE CALL. The facade exposes
// `objectOrder`, `objectDescendants`, `objectAt`, `objectRect` … and nothing that
// answers "is there at least one". A `hasObjects()` / `objectCount()` on the wasm
// side would make this O(1) with no host state at all, and would be the better
// fix; it is engine work, and this module is deliberately shaped so that
// adopting it later is a one-line change to `read`.
//
// INVALIDATION IS THE WHOLE RISK, so it is stated plainly: the answer is
// remembered until `forget()` is called, and `forget()` must be called wherever
// the document's object set can change — every landed edit, and every document
// swap. A missed invalidation shows up as "Select next object" greyed out
// immediately after inserting the first picture, which `object-command-reach`
// drives in a browser.

/** A memoized `documentHasObjects`.
 *
 *  `read` is called at most once per invalidation window and must return the
 *  engine's object order as a JSON string (or throw / return something
 *  unparseable, which is read as "no objects" — the same fail-closed behaviour
 *  the inline version had).
 *
 *  Cost: `has()` is O(1) after the first call in a window. The first call is
 *  O(objects), which is the unavoidable cost of asking the engine at all; it is
 *  paid once per edit rather than once per keystroke. */
export function createObjectPresence(read) {
  let cached = null;
  return {
    /** Whether the document holds at least one floating object. O(1) except on
     *  the first call after `forget()`. */
    has() {
      if (cached !== null) return cached;
      try {
        const objects = JSON.parse(read());
        cached = Array.isArray(objects) && objects.length > 0;
      } catch {
        cached = false;
      }
      return cached;
    },
    /** Drops the remembered answer. Call on every landed edit and every
     *  document swap; calling it too often costs one engine call, missing a call
     *  ships a stale answer, so err towards this. */
    forget() {
      cached = null;
    },
    /** Whether an answer is currently remembered — for the guard that proves the
     *  work does not grow with the document, and for nothing else. */
    isCached() {
      return cached !== null;
    },
  };
}
