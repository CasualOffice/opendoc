// References ▸ Table of contents: generating one, and updating one.
//
// Both controls shipped DISABLED with a reason for as long as there was no
// engine operation behind them — the rule this repository keeps
// (`docs/63`: never a control that silently does nothing). The operations exist
// now (`insertTableOfContents`, `updateTableOfContents`), so the reasons come
// off and this module is what the buttons run.
//
// ## The competitive standard, and which one this follows
//
// * **Word** — References ▸ Table of Contents inserts one, and **Update Table**
//   opens a two-radio dialog: *Update page numbers only* / *Update entire
//   table*. The insert side offers **Show levels**, 1..9, defaulting to 3.
// * **ONLYOFFICE** — the same split, as two rows of one dropdown rather than a
//   modal, and the same 1..9 level choice in its settings dialog.
// * **Google Docs** — inserts with no options at all and updates with a refresh
//   button on the table itself; no level choice anywhere.
//
// **Followed: Word.** The two products that offer a level choice agree on the
// range and the default, and the update split is the first thing a user reaches
// for after editing a document — which is exactly why "update" cannot be one
// button that guesses. Word's radio pair is kept as a radio pair, and each mode
// ALSO gets its own command so the choice is reachable without the modal.
//
// ## Which table of contents is being updated
//
// `updateTableOfContents("")` means "the document's only contents field", and
// the engine refuses rather than guessing when there are none or several. So the
// empty id covers the ordinary document, and only a document with more than one
// pays for `fieldRangeSpans()` — which is **O(document)** and documented as not
// a keystroke accessor. It is called here on the update gesture itself, never
// from enablement: the ribbon's enablement reads `fieldRangeEntries()`, which is
// O(field ranges) and walks nothing, cached the way the stale-caption count is.
//
// The style-based path in `toc_navigation.mjs` stays exactly as it is and stays
// necessary: a hand-built table of contents has no field at all, so nothing here
// reports it, and following one of its entries is not a field question.
import { t } from "./i18n.mjs";

/** Word's default for "Show levels", and the engine's own default for `0`. */
export const DEFAULT_TOC_LEVELS = 3;

/** The deepest heading level a table of contents can list (`w:outlineLvl` is
 *  0..8, so nine levels). */
export const MAX_TOC_LEVELS = 9;

/** The two update modes the facade takes, in the order Word's dialog lists them. */
export const UPDATE_MODES = Object.freeze(["pageNumbers", "entire"]);

/**
 * Whether a field instruction is a table of contents.
 *
 * `TOC` is the field kind; `TOC \o "1-3" \z \u` is what we generate and
 * ` TOC \h ` is what Word writes. Matched on the first word so a `TOA` (table of
 * authorities) or a `RD` in the same document is not mistaken for one.
 *
 * O(length of the instruction).
 */
export function isTocInstruction(instruction) {
  return /^\s*TOC\b/i.test(String(instruction ?? ""));
}

/**
 * `fieldRangeSpans()`'s lines as `{ id, instruction, nodes }`.
 *
 * The wire shape is `id\tinstruction\tnode node …`; a range whose markers are
 * missing reports an empty node list rather than being dropped, so an entry with
 * no third column is kept with no nodes rather than skipped.
 *
 * O(total line length).
 */
export function parseFieldRangeSpans(lines) {
  return [...(lines ?? [])].map((line) => {
    const [id = "", instruction = "", nodes = ""] = String(line).split("\t");
    return { id, instruction, nodes: nodes ? nodes.split(" ").filter(Boolean) : [] };
  });
}

/**
 * The id of the contents field covering `node`, or `""`.
 *
 * This is the question `toc_navigation.mjs` could not ask before the spans
 * accessor existed — "is the caret inside the contents field" — and it is what
 * decides which table an update is aimed at when a document holds several.
 *
 * O(spans × nodes per span).
 */
export function tocFieldAt(spans, node) {
  if (!node) return "";
  for (const span of spans) {
    if (!isTocInstruction(span.instruction)) continue;
    if (span.nodes.includes(node)) return span.id;
  }
  return "";
}

/**
 * Builds the two dialogs and the contents-field cache.
 *
 * `io` is the whole contact with the application:
 *
 *   `getDoc()`           the open document, or null
 *   `caretNode()`        the caret's paragraph id, or null
 *   `applyEdit(fn)`      Promise<boolean>; one gated, undoable engine call
 *   `mutationBlocked()`  true (having said why) when the review mode refuses
 *   `status(text, kind)` the status line
 *   `registerModal(dialog, options)`
 *   `fallbackFocus()`
 */
export function createTocCommands(io) {
  const el = (id) => document.getElementById(id);
  /** How many contents FIELDS the document holds. Read by the ribbon on every
   *  keystroke, so it is a cached number and never a walk. */
  let fieldCount = 0;

  const fields = {
    get count() {
      return fieldCount;
    },

    /** One `fieldRangeEntries()` read — O(field ranges), no document walk. Called
     *  from the same deliberate interactions the stale-caption count is: opening
     *  a document, inserting or updating a table, and reaching the References
     *  tab. */
    refresh() {
      const doc = io.getDoc();
      if (!doc?.fieldRangeEntries) {
        fieldCount = 0;
        return;
      }
      try {
        fieldCount = doc
          .fieldRangeEntries()
          .filter((line) => isTocInstruction(String(line).split("\t")[1] ?? "")).length;
      } catch {
        // A build whose engine predates the binding: "no contents field" is the
        // honest state, and the button then says so rather than throwing.
        fieldCount = 0;
      }
    },
  };

  // ---- Insert -------------------------------------------------------------
  const insertDialog = el("tocDialog");
  const levelsInput = el("tocLevels");
  const insertModal = insertDialog
    ? io.registerModal(insertDialog, {
        initialFocus: () => levelsInput,
        fallbackFocus: io.fallbackFocus,
      })
    : null;

  /** The level the dialog is asking for, clamped to what the engine accepts.
   *  A blank or unreadable box is Word's default rather than a refusal: the
   *  question is "how deep", and 3 is the answer every product ships with. */
  function levels() {
    const raw = Number(levelsInput?.value);
    if (!Number.isFinite(raw)) return DEFAULT_TOC_LEVELS;
    return Math.min(MAX_TOC_LEVELS, Math.max(1, Math.round(raw)));
  }

  const insert = {
    open() {
      if (!insertModal || !io.getDoc() || io.mutationBlocked()) return;
      if (levelsInput && levelsInput.value === "") {
        levelsInput.value = String(DEFAULT_TOC_LEVELS);
      }
      insertModal.open();
    },

    close() {
      insertModal?.close();
    },

    /** Inserts at the caret, then re-reads the cache the insert invalidates.
     *
     *  The engine's refusals here are sentences a reader can act on — "the
     *  document has no headings at level 3 or above to list" is the whole
     *  explanation — so they are shown verbatim instead of being flattened into
     *  the generic "that edit isn't supported" `runEdit` writes for internal
     *  vocabulary (`edit_errors.mjs`). */
    async run() {
      const doc = io.getDoc();
      const node = io.caretNode();
      if (!doc || !node || io.mutationBlocked()) return;
      let refusal = "";
      const applied = await io.applyEdit(() => {
        try {
          return doc.insertTableOfContents(node, levels());
        } catch (error) {
          refusal = String(error?.message ?? error ?? "");
          throw error;
        }
      });
      fields.refresh();
      if (refusal) io.status(refusal, "warn");
      else if (applied) io.status(t("toc.inserted"));
      if (applied) insert.close();
    },
  };

  // ---- Update -------------------------------------------------------------
  const updateDialog = el("tocUpdateDialog");
  const updateModal = updateDialog
    ? io.registerModal(updateDialog, {
        initialFocus: () => updateDialog.querySelector('input[name="tocUpdateMode"]:checked'),
        fallbackFocus: io.fallbackFocus,
      })
    : null;

  const update = {
    open() {
      if (!updateModal || !io.getDoc() || io.mutationBlocked()) return;
      updateModal.open();
    },

    close() {
      updateModal?.close();
    },

    /** The mode the dialog's radios are on. */
    chosenMode() {
      const picked = updateDialog?.querySelector('input[name="tocUpdateMode"]:checked');
      return UPDATE_MODES.includes(picked?.value) ? picked.value : UPDATE_MODES[0];
    },

    /**
     * Updates one table of contents.
     *
     * With exactly one in the document the empty id says so and no walk happens
     * at all. With several, the caret decides — which is Word's rule, and the
     * question `fieldRangeSpans()` exists to answer. That call is O(document)
     * and runs HERE, on a gesture, never on the enablement path.
     */
    async run(mode) {
      const doc = io.getDoc();
      if (!doc || io.mutationBlocked()) return;
      let id = "";
      if (fields.count > 1) {
        id = tocFieldAt(parseFieldRangeSpans(doc.fieldRangeSpans()), io.caretNode());
        if (!id) {
          io.status(t("toc.whichTable"), "warn");
          return;
        }
      }
      let refusal = "";
      const applied = await io.applyEdit(() => {
        try {
          return doc.updateTableOfContents(id, mode);
        } catch (error) {
          refusal = String(error?.message ?? error ?? "");
          throw error;
        }
      });
      fields.refresh();
      if (refusal) io.status(refusal, "warn");
      else if (applied) {
        io.status(t(mode === "entire" ? "toc.rebuilt" : "toc.pageNumbersUpdated"));
      }
      if (applied) update.close();
    },
  };

  insertDialog?.querySelector("#tocForm")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void insert.run();
  });
  el("tocCancel")?.addEventListener("click", () => insert.close());
  el("tocClose")?.addEventListener("click", () => insert.close());
  updateDialog?.querySelector("#tocUpdateForm")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void update.run(update.chosenMode());
  });
  el("tocUpdateCancel")?.addEventListener("click", () => update.close());
  el("tocUpdateClose")?.addEventListener("click", () => update.close());

  return { fields, insert, update };
}
