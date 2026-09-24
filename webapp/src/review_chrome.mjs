// The DOM half of the review chrome: the small factories every comment card is
// built from, and the right-margin comment affordance.
//
// These came out of `main.js` under the line ratchet (`module_seams`), and they
// belong out here on their own account: none of them reads a single piece of
// application state. They take a node or a label and return a node, which is
// what makes the margin affordance's placement assertable without a browser —
// the arithmetic lives in `review_layout.mjs`, and this file only applies it.
import { commentAffordanceSpot } from "./review_layout.mjs";

/** A text action on a comment card ("Resolve", "Reply", "Delete"). */
export function reviewCardButton(label, action, danger = false, ariaLabel = "") {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `review-margin-action${danger ? " danger" : ""}`;
  button.textContent = label;
  // A descriptive accessible name where the visible verb alone ("Accept",
  // "Reply") lacks context for a screen reader (REVIEW-GAP-023).
  if (ariaLabel) {
    button.setAttribute("aria-label", ariaLabel);
    button.title = ariaLabel;
  }
  button.addEventListener("click", async (event) => {
    event.stopPropagation();
    await action();
  });
  return button;
}

/** An icon-only action on a comment card. The glyph is `aria-hidden`, so the
 *  label is the accessible name and the tooltip both. */
export function reviewIconButton(icon, label, action) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "review-margin-icon-action";
  button.setAttribute("aria-label", label);
  button.title = label;
  const glyph = document.createElement("span");
  glyph.className = "ms";
  glyph.setAttribute("aria-hidden", "true");
  glyph.textContent = icon;
  button.appendChild(glyph);
  button.addEventListener("click", async (event) => {
    event.stopPropagation();
    await action();
  });
  return button;
}

/** Makes a textarea grow with its content (modern Docs/Word composer): height
 *  tracks the scroll height from a min of one row up to a cap, after which it
 *  scrolls. Returns a `resize()` the caller can invoke after programmatic value
 *  changes. */
export function autoGrowTextarea(textarea, { min = 34, max = 180 } = {}) {
  const resize = () => {
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(max, Math.max(min, textarea.scrollHeight))}px`;
  };
  textarea.addEventListener("input", resize);
  requestAnimationFrame(resize);
  return resize;
}

/** Shared "modern comment" composer key handling: Enter submits, Shift+Enter
 *  inserts a newline, Escape cancels. Kept identical across the top-level comment
 *  box and every reply composer so the interaction never differs by surface (Q5).
 *  Always stops propagation so a keystroke in the composer never reaches the
 *  canvas editor or the card's expand/collapse handler. */
export function attachComposerKeys(textarea, { onSubmit, onCancel }) {
  textarea.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter" && !event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      onSubmit?.();
    } else if (event.key === "Escape") {
      event.preventDefault();
      onCancel?.();
    }
  });
}

/** Drives the right-margin comment affordance — Google Docs' margin button.
 *
 *  Why it exists: with the comment column closed, the right margin of the work
 *  area was empty and offered nothing. The durable entry points were both on the
 *  far side of the window (the rail's Comments toggle, the Review band's
 *  Comments pane), so the one place a reader looks when they want to say
 *  something about a sentence — beside the sentence — was the one place with no
 *  affordance at all. Docs puts an Add-comment button in that margin, on the
 *  line the caret is in; Word puts its Comments button in the top right and
 *  marks the text instead. This chrome already renders comment cards in that
 *  margin, so Docs' affordance is the one that belongs in it.
 *
 *  It is NOT a second way to comment. The button is declared on the existing
 *  `review.comment` row of main.js's `REVIEW_SURFACE`, so the same wiring pass
 *  gives it the same handler and the same enablement rule as the Review band's
 *  button; this controller only decides WHERE it goes and WHETHER the margin can
 *  hold it.
 *
 *  `rect()` answers with the caret's line — or a selection's first line — in
 *  client coordinates, or null. The offsets are read from the live viewport at
 *  call time so nothing here has to be told when the window resizes.
 *
 *  O(1): one rect from the engine per state change, and nothing at all per
 *  scroll frame beyond re-applying a stored number.
 */
export function createCommentAffordance({ button, viewport, rect }) {
  // Band coordinates, like the cards (`mountReviewWindow`): `bandOffset` moves
  // under a compressed document on every scroll, so the stored position has it
  // taken out and the live value added back when the position is applied.
  let placed = null;
  const apply = (bandOffset = 0) => {
    button.hidden = !placed;
    if (!placed) return;
    button.style.left = `${placed.left}px`;
    button.style.top = `${placed.top + bandOffset}px`;
  };
  return {
    /** Recomputes the position. `allowed` is the caller's own policy — a
     *  document is open, the column is closed, and the layout is the margin
     *  shape rather than the narrow-width bottom sheet. */
    sync(allowed, bandOffset = 0) {
      const box = viewport.getBoundingClientRect();
      placed = allowed
        ? commentAffordanceSpot({
          rect: rect(),
          viewportLeft: box.left,
          viewportTop: box.top,
          // `clientWidth`, not the bounding box: the box includes the
          // viewport's own scrollbar, and a button positioned under it is a
          // button nobody can press.
          viewportWidth: viewport.clientWidth,
          scrollLeft: viewport.scrollLeft,
          scrollTop: viewport.scrollTop,
          bandOffset,
        })
        : null;
      apply(bandOffset);
    },
    /** Re-applies the stored position for the current band offset. This is the
     *  whole of the scroll path: the button is positioned in the viewport's
     *  scrolled content, so it rides the scroll natively and only a compressed
     *  band moves it. */
    ride(bandOffset = 0) {
      apply(bandOffset);
    },
  };
}
