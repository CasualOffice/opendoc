// The chip that appears above a selected image, shape or text box.
//
// Google Docs puts exactly this under a selected image — the wrap modes as one
// row, then the object's own actions — and Word's Layout Options flyout is the
// same control with a button in front of it. This host had the chip already;
// what it did not have was the DEFAULT case. An inserted picture is INLINE, an
// inline object reports `canWrap: false` (correctly: it has no anchor to set a
// wrap on), and the chip was keyed on that bit — so the object every user meets
// first showed no wrap control at all, and "In line" did not exist as a mode
// anywhere in the product. With `setObjectAnchorKind` in the facade the
// conversion is a real node rewrite, so in-line becomes the first choice in the
// row and the chip is offered for every top-level object.
//
// Lifted out of `main.js` with the arrange work, both to pay under the line
// ratchet and because the bar is now six controls rather than three. Its whole
// contact with the application is `io` — the state it renders, and the verbs it
// calls. It reads NOTHING from the document itself: the caller gathers the
// arrange state once per repaint (`objectPosition` is O(document) with one
// walk) and hands it in, so a bar render can never become a per-object query in
// a loop.
//
// Cost: O(controls) per render — a fixed ~20 nodes for ONE object.
import { objectBarPosition } from "./object_guides.mjs";
import { OBJECT_LABELS } from "./object_traversal.mjs";
import { WRAP_CHOICES, activeWrapChoice, wrapAvailability } from "./object_arrange.mjs";

/** Builds one bar action button (icon + label). `pointerdown` is prevented so
 *  clicking it never deselects the object — the canvas pointerdown deselect is
 *  what every control on this bar has to dodge. */
export function objectBarButton(icon, label, title, onClick, danger = false) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = `object-bar-btn${danger ? " danger" : ""}`;
  btn.title = title;
  btn.setAttribute("aria-label", title);
  const glyph = document.createElement("span");
  glyph.className = "ms";
  glyph.setAttribute("aria-hidden", "true");
  glyph.textContent = icon;
  btn.appendChild(glyph);
  const text = document.createElement("span");
  text.textContent = label;
  btn.appendChild(text);
  btn.addEventListener("pointerdown", (event) => event.preventDefault()); // keep selection
  btn.addEventListener("click", onClick);
  return btn;
}

export function createObjectBar(io) {
  let barEl = null;

  /** The wrap row: in-line first, then Word's six wrapping modes.
   *
   *  Offered for every top-level object, disabled WITH THE REASON for a group's
   *  child (whose position its parent decides). Never hidden: a control that
   *  disappears cannot be told from a bug, and the reason is the whole point. */
  function wrapRow(state) {
    const availability = wrapAvailability(state.read);
    const active = activeWrapChoice(state.read, state.wrap);
    const row = document.createElement("div");
    row.className = "object-wrap-menu";
    row.setAttribute("role", "group");
    row.setAttribute("aria-label", io.t("object.wrap.label"));
    for (const choice of WRAP_CHOICES) {
      const label = io.t(choice.key);
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "object-wrap-btn";
      btn.dataset.wrap = choice.value;
      btn.textContent = label;
      btn.setAttribute("aria-pressed", String(choice.value === active));
      if (!availability.available) {
        const reason = io.t(availability.reasonKey);
        btn.disabled = true;
        btn.title = reason;
        // The reason has to reach a screen reader and a touch user too, and
        // `title` reaches neither. It goes in the accessible name.
        btn.setAttribute("aria-label", `${label} — ${reason}`);
      }
      btn.addEventListener("pointerdown", (event) => event.preventDefault());
      btn.addEventListener("click", () => io.setWrap(choice.value));
      row.appendChild(btn);
    }
    return row;
  }

  /** Shows/positions the bar above the selected object. */
  function update() {
    io.ensureInspector();
    if (!barEl) {
      barEl = document.createElement("div");
      barEl.className = "object-context-bar";
      barEl.hidden = true;
      document.body.appendChild(barEl);
    }
    const selection = io.selection();
    if (!selection || selection.mode !== "selected") {
      barEl.hidden = true;
      io.toggleInspector(false);
      return;
    }
    const rect = io.doc().objectRect(selection.node); // [page, x, y, w, h]
    const page = rect.length >= 5 ? io.pages()[rect[0] - 1] : null;
    if (!page) {
      barEl.hidden = true;
      return;
    }
    barEl.replaceChildren();
    const strong = document.createElement("strong");
    strong.textContent = OBJECT_LABELS[selection.kind] ?? "Object";
    barEl.appendChild(strong);
    // While a crop is live the bar described the gesture it is NOT offering — it
    // read "Drag handles to resize" over black crop grips. It says what they do.
    const cropping = io.croppingNode() === selection.node;
    const state = io.arrangeState();
    if (cropping) {
      const hint = document.createElement("small");
      hint.textContent = io.t("object.cropHint");
      barEl.appendChild(hint);
    } else {
      if (state) barEl.appendChild(wrapRow(state));
      const hint = document.createElement("small");
      hint.textContent = selection.canMove
        ? (selection.canResize ? "Drag to move · handles to resize" : "Drag to move")
        : (selection.canResize ? "Drag handles to resize" : "");
      if (hint.textContent) barEl.appendChild(hint);
    }

    const divider = document.createElement("span");
    divider.className = "object-bar-divider";
    const actions = document.createElement("div");
    actions.className = "object-bar-actions";
    // Position / Arrange / Rotate are Word's Arrange group, on the object's own
    // chip because that is where Docs puts the same decisions. Their buttons are
    // module-level in the caller so their popovers stay registered across the
    // bar's re-renders — the same reason Fill and Outline are.
    if (state && !cropping) {
      actions.appendChild(io.positionButton());
      actions.appendChild(io.arrangeButton());
      actions.appendChild(io.rotateButton());
    }
    if (selection.canAltText) {
      actions.appendChild(objectBarButton("description", "Alt text", "Edit alt text", io.openAltText));
    }
    if (selection.canResize) {
      actions.appendChild(
        objectBarButton("tune", "Properties", "Open object properties", () => io.toggleInspector(true)),
      );
    }
    if (selection.kind === "shape" && (selection.canFill || selection.canStroke)) {
      // Word's Shape Format tab reduces to its two live controls.
      if (selection.canFill) actions.appendChild(io.fillButton());
      if (selection.canStroke) actions.appendChild(io.outlineButton());
      io.reflectShapeSwatches();
    }
    if (selection.canCrop) {
      // Crop is a picture-only operation; a text box has no source rectangle.
      const cropBtn = objectBarButton(
        "crop",
        cropping ? "Apply" : "Crop",
        cropping ? "Apply crop (Enter)" : "Crop image",
        io.enterCrop,
      );
      if (cropping) cropBtn.classList.add("is-active");
      actions.appendChild(cropBtn);
    }
    if (selection.canDelete) {
      actions.appendChild(objectBarButton("delete", "Delete", "Delete object", io.deleteObject, true));
    }
    if (actions.childElementCount > 0) {
      barEl.appendChild(divider);
      barEl.appendChild(actions);
    }
    barEl.hidden = false;
    reposition();
    // The panel is a live view of the selected object, so it is re-read here —
    // this runs on every repaint, which is what makes a drag-resize, a nudge and
    // a change of selection all show up in the fields (docs/104 HF-057).
    if (io.inspectorOpen()) io.reflectInspector();
  }

  /** Puts the bar just above the object it acts on, and takes it off screen when
   *  that object is not on screen.
   *
   *  The bar is body-level with fixed viewport coordinates, so nothing about a
   *  scroll moves it on its own (docs/104 HF-058). Separated from `update` so a
   *  scroll only re-measures; it never rebuilds the bar's contents. */
  function reposition() {
    if (!barEl || !io.doc()) return;
    const selection = io.selection();
    const rect = selection?.mode === "selected" ? io.doc().objectRect(selection.node) : [];
    const page = rect.length >= 5 ? io.pages()[rect[0] - 1] : null;
    if (!page) {
      barEl.hidden = true;
      return;
    }
    const { rect: pageRect, sx, sy } = io.scaleOf(page);
    const top = pageRect.top + rect[2] * sy;
    barEl.hidden = false; // must be visible to be measured
    const at = objectBarPosition(
      { left: pageRect.left + rect[1] * sx, top, bottom: top + rect[4] * sy },
      io.viewportRect(),
      barEl.offsetHeight,
    );
    if (!at) {
      barEl.hidden = true; // the object is scrolled out of the page view
      return;
    }
    // Keep the bar inside the window. `objectBarPosition` aligns its left edge
    // with the object's, which ran a wide bar off the right-hand side — where it
    // wrapped to a second row, and every extra row of bar is a row of the
    // OBJECT the user cannot reach, because the bar falls back to overlapping
    // the object when there is no room above it.
    const view = io.viewportRect();
    const maxLeft = Math.max(8, view.right - barEl.offsetWidth - 8);
    barEl.style.left = `${Math.min(at.left, maxLeft)}px`;
    barEl.style.top = `${at.top}px`;
  }

  return { update, reposition, element: () => barEl };
}
