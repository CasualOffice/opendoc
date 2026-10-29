// Review ▸ Display for review — Word's four views of a reviewed document, in one
// picker (`docs/153` `review.display-mode-simple-markup`,
// `review.display-mode-original`, `review.display-mode-final`; `docs/109` RM-31).
//
// ---- THE SHAPE, FROM THE REFERENCES ------------------------------------------
//
//   * Word 365: Review ▸ Tracking ▸ "Display for Review" is one dropdown of four —
//     Simple Markup (its default), All Markup, No Markup, Original. In Simple
//     Markup the text reads as if every change were accepted and a red bar in the
//     left margin marks each changed line; clicking a bar switches to All Markup.
//   * ONLYOFFICE: Collaboration ▸ Display mode, `Common.Views.ReviewChanges`
//     `txtMarkupSimple` / `txtMarkup` / `txtFinal` / `txtOriginal` — the same
//     four, by the same names (`web-apps/apps/common/main/lib/view/ReviewChanges.js`).
//
// So: ONE dropdown on the Review band's Tracking group, four `menuitemradio`
// rows in Word's order with Word's names, the same four as a Review-menu flyout
// and as palette rows. One mechanism behind all three faces: `setMode`.
//
// ---- WHAT EACH MODE IS, IN THIS ENGINE ---------------------------------------
//
//   * All markup — the engine's markup layout (`setShowChanges(true)`, docs/93):
//     struck deletions and coloured insertions painted into the page, with the
//     overlay's revision markers on top. Unchanged.
//   * Simple markup — the EDITING layout, which already reads as accepted
//     (insertions plain, deletions zero-width), with the inline revision markers
//     suppressed and a change bar beside each changed line instead. The bars are
//     the overlay's own margin-bar geometry (`paragraphRevisionRects`, Word's
//     "changed lines" bar for a paragraph-format change), so a bar and a
//     paragraph-format bar can never sit in two different places.
//   * No markup — the editing layout and nothing on top of it: no revision
//     markers, no bars, no comment highlights. Word's No Markup hides balloons
//     too; the comment column is the reader's own panel toggle and stays theirs.
//   * Original — every change REJECTED, view-only. Not a paint style: it is
//     Reject All, which the engine implements once, applied to a THROWAWAY COPY
//     (`casual-doc-wasm` `review_display.rs`) and shown through the version
//     preview's read-only swap — the editor's existing fail-closed choke point,
//     so typing, paste, toolbars and the host bridge all refuse by saying it is
//     the original. The reader's document is never touched; any other mode puts
//     it back.
//
// The mode is DERIVED, never stored twice: the copy being on the canvas is
// Original, the markup layout being on is All markup, and one flag here tells
// Simple from No markup. So Suggesting turning the markup on (review UX Q1), the
// View menu's `view.showChanges` toggle and a version preview leaving can never
// leave this picker saying something the canvas does not show.
import { authoredTitle } from "./localize.mjs";
import { t } from "./i18n.mjs";
import { onButton, registerPopover } from "./popover_manager.mjs";

/** The four modes, in Word's order. `command` is the host-contract id. */
export const DISPLAY_MODES = Object.freeze([
  Object.freeze({ mode: "simple", command: "review.displayMode.simpleMarkup", labelKey: "reviewDisplay.simpleMarkup" }),
  Object.freeze({ mode: "all", command: "review.displayMode.allMarkup", labelKey: "reviewDisplay.allMarkup" }),
  Object.freeze({ mode: "none", command: "review.displayMode.noMarkup", labelKey: "reviewDisplay.noMarkup" }),
  Object.freeze({ mode: "original", command: "review.displayMode.original", labelKey: "reviewDisplay.original" }),
]);

/** Which marks the overlay paints in `mode`. Pure, so the four answers are
 *  checked in node. */
export function overlayFor(mode) {
  return {
    revisionMarkers: mode === "all",
    changeBars: mode === "simple",
    commentMarkers: mode !== "none",
  };
}

/**
 * Installs the picker on the editor context `ed` and returns its surface.
 *
 * @param {object} ed the editor context (`docs/177`).
 */
export function createReviewDisplay(ed) {
  /** `true` for No markup, `false` for Simple markup, while the markup layout is off. */
  let hidden = false;
  /** The throwaway Original copy while it is on the canvas, else null. */
  let original = null;

  function mode() {
    if (original && ed.doc === original) return "original";
    if (ed.showingChanges) return "all";
    return hidden ? "none" : "simple";
  }

  /** Builds the Original copy: the bytes a save would write, opened afresh, every
   *  change rejected. O(document), once. Returns null after saying why. */
  async function buildOriginal() {
    let copy = null;
    try {
      // Imported when first asked for, as the version panel's parse is: the
      // engine is already loaded by then, and nothing at boot pays for it.
      const { open } = await import("../pkg/casual_doc_wasm.js");
      copy = open(ed.takeDraftSnapshot().bytes);
      copy.showAsOriginal();
      return copy;
    } catch (error) {
      copy?.free?.();
      ed.setStatus(String(error?.message ?? error), "error");
      return null;
    }
  }

  /** Leaves the Original copy, if it is up, and frees it. */
  async function leaveOriginal() {
    if (!original) return;
    const copy = original;
    original = null;
    if (ed.doc === copy) await ed.showVersionPreview(null);
    copy.free?.();
  }

  async function setMode(next) {
    if (!ed.doc || !DISPLAY_MODES.some((row) => row.mode === next)) return;
    if (next === mode()) {
      reflect();
      return;
    }
    if (next === "original") {
      // From the LIVE document, whatever mode it is in: the copy is built from
      // the document the reader is editing, not from a preview of something else.
      if (ed.versionPreviewHome) {
        ed.setStatus(t("reviewDisplay.previewOpen"), "error");
        return;
      }
      const copy = await buildOriginal();
      if (!copy) return;
      original = copy;
      await ed.showVersionPreview(copy, { reason: t("reviewDisplay.originalReadOnly") });
    } else {
      await leaveOriginal();
      // Only an off choice is remembered: All markup leaves it, so markup turned
      // off again by something else returns to the reader's own off mode.
      if (next !== "all") hidden = next === "none";
      await ed.setShowingChanges(next === "all");
      ed.drawSelection();
    }
    reflect();
    ed.announceReview(t(DISPLAY_MODES.find((row) => row.mode === next).labelKey));
  }

  const button = document.getElementById("reviewDisplayBtn");
  const menu = document.getElementById("reviewDisplayMenu");
  if (button && menu) {
    const popover = registerPopover(button, menu, () => reflect());
    for (const row of menu.querySelectorAll("[data-display-mode]")) {
      onButton(row, () => {
        // Closes through the manager, which keeps `aria-expanded` honest and
        // returns focus to the trigger, BEFORE the canvas changes under it.
        popover.close();
        void setMode(row.dataset.displayMode);
      });
    }
  }

  /** Syncs the trigger and the rows to the derived mode. O(4). */
  function reflect() {
    const current = mode();
    if (button) {
      button.disabled = !ed.doc;
      // The authored title, through the catalogue when it has arrived: chrome
      // reflected at boot must not paint a raw key (chrome-raw-keys, SKILL §11).
      button.title = ed.doc ? authoredTitle(button) : t("command.needsDocument");
    }
    for (const row of menu?.querySelectorAll("[data-display-mode]") ?? []) {
      row.setAttribute("aria-checked", String(row.dataset.displayMode === current));
    }
  }

  /**
   * Paints Simple markup's change bars on the overlay: one bar per changed
   * range per page, in the page margin beside the text column. Clicking one
   * shows All markup, which is Word's gesture. The bars carry no focus — the
   * picker is the keyboard and screen-reader path to the same switch — and
   * `pointerdown` does not swallow the event, so a click still places the caret.
   *
   * O(revisions), on the overlay pass that already walks them.
   */
  function paintChangeBars(revisions) {
    for (const revision of revisions) {
      const range = ed.revisionRange(revision);
      if (!range) continue;
      const rects = ed.paragraphRevisionRects({
        ...revision,
        kind: "paragraph_format",
        anchor: { node: range.startNode, start: range.startOffset, end: range.endOffset },
      });
      for (let i = 0; i + 4 < rects.length; i += 5) {
        const bar = ed.place(rects.slice(i, i + 5), "review-change-bar");
        if (!bar) continue;
        bar.title = t("reviewDisplay.changeBar");
        bar.addEventListener("pointerdown", () => void setMode("all"));
      }
    }
  }

  return {
    mode,
    setMode,
    reflect,
    paintChangeBars,
    overlay: () => overlayFor(mode()),
    /** The palette's four rows, live whenever a document is open. */
    commands: () =>
      DISPLAY_MODES.map((row) => ({
        id: row.command,
        label: t(row.labelKey),
        group: "Review",
        kw: "display for review markup simple all no original final tracked changes view redline",
        enabled: !!ed.doc,
        disabledReason: ed.doc ? "" : t("command.needsDocument"),
        run: () => void setMode(row.mode),
      })),
  };
}
