// The object properties panel — Word's "Size and Position…", Docs' "All image
// options".
//
// Lifted out of `main.js` unchanged in behaviour, to pay under the line ratchet
// for the arrange work that surrounds it (`tests/module_seams.test.mjs`), and
// because a 170-line panel that reads and writes engine geometry is exactly the
// kind of thing that should not be interleaved with pointer handling. Its whole
// contact with the application is `io`:
//
//   `doc()`              the live engine handle, or null
//   `selection()`        the live object selection, or null
//   `runEdit(fn, opts)`  the one gated, undoable edit path
//   `setStatus(text, k)` the status line
//   `openShapeFill()`    open the shape-fill picker (the bar owns the button)
//   `openShapeOutline()`
//   `t(key)`             the catalogue
//
// Cost: O(1) per reflect — a fixed number of engine reads for ONE object. It
// runs on every repaint while the panel is open, so it must stay that way.
import { OBJECT_LABELS } from "./object_traversal.mjs";
import { TWIPS_PER_INCH } from "./units.mjs";
import { readTextBoxBody, writeTextBoxBody } from "./text_box_body.mjs";

/** Builds the panel. Nothing is created until `ensure()` is first called, so a
 *  session that never selects an object never pays for it. */
export function createObjectInspector(io) {
  let objectInspectorEl = null;

  /** The object the inspector's fields currently describe. It is what tells a
   *  repaint of the SAME object (where a field the user is typing in must be left
   *  alone) apart from a move to a DIFFERENT one (where leaving the old numbers is
   *  the data-loss defect: Apply then wrote one object's geometry onto another). */
  let objectInspectorNode = null;

  /** Fills the inspector from the model's live geometry for the selected object.
   *
   *  This runs on every repaint while the panel is open (docs/104 HF-057). It used
   *  to run only on the opening call, so a drag-resize left the panel showing the
   *  pre-drag numbers and the next Apply — or a nudge — put the object back where
   *  it had been; and selecting a second object left the first one's size, alt text
   *  and wrap in the fields, aimed at the new object.
   *
   *  A field the user is mid-edit in is not overwritten, because typing "3.2" and
   *  having the caret jump is its own defect — but that courtesy stops at the
   *  object boundary: when the panel switches objects every field is rewritten,
   *  focused or not, because a stale value there is a wrong write, not a nuisance. */
  function reflectObjectInspector() {
    if (!objectInspectorEl || !io.doc() || !io.selection()) return;
    const rect = io.doc().objectRect(io.selection().node);
    if (rect.length < 5) return;
    const sameObject = objectInspectorNode === io.selection().node;
    objectInspectorNode = io.selection().node;
    // `value` writes are skipped for the focused control only while the panel is
    // still describing the same object.
    const setValue = (element, value) => {
      if (!element) return;
      if (sameObject && element === document.activeElement) return;
      element.value = value;
    };
    const inches = (twips) => String(Math.round(twips / TWIPS_PER_INCH * 100) / 100);
    setValue(objectInspectorEl.querySelector("[data-object-prop=width]"), inches(rect[3]));
    setValue(objectInspectorEl.querySelector("[data-object-prop=height]"), inches(rect[4]));
    setValue(objectInspectorEl.querySelector("[data-object-prop=left]"), inches(rect[1]));
    setValue(objectInspectorEl.querySelector("[data-object-prop=top]"), inches(rect[2]));
    // The typed half of the rotation handle. A grip can express any angle but
    // cannot express EXACTLY 37, and `docs/105` UX-004 asks for every capability
    // on at least two surfaces — the Rotate menu's four fixed choices are a
    // different capability, not a second surface for this one.
    const rotationField = objectInspectorEl.querySelector("[data-object-inspector-rotation]");
    rotationField.hidden = !io.selection().canRotate;
    if (io.selection().canRotate) {
      const frame = io.doc().objectFrame?.(io.selection().node) ?? [];
      setValue(
        rotationField.querySelector("[data-object-prop=rotation]"),
        String(Math.round((frame[5] ?? 0) / 1000)),
      );
    }
    objectInspectorEl.querySelector("[data-object-inspector-kind]").textContent = OBJECT_LABELS[io.selection().kind] ?? "Object";
    const altField = objectInspectorEl.querySelector("[data-object-inspector-alt]");
    altField.hidden = !io.selection().canAltText;
    if (io.selection().canAltText) setValue(altField.querySelector("input"), io.doc().objectDescr(io.selection().node) ?? "");
    const wrapField = objectInspectorEl.querySelector("[data-object-inspector-wrap]");
    wrapField.hidden = !io.selection().canWrap;
    if (io.selection().canWrap) setValue(wrapField.querySelector("select"), io.doc().objectWrap(io.selection().ref.root) || "square");
    // A picture's Appearance is its border — Word's Format Picture ▸ Line — and
    // the button says so (`docs/109` HF-254).
    const appearance = objectInspectorEl.querySelector("[data-object-inspector-appearance]");
    const kind = io.selection().kind;
    appearance.hidden = (kind !== "shape" && kind !== "image") || (!io.selection().canFill && !io.selection().canStroke);
    appearance.querySelector("[data-object-inspector-fill]").hidden = !io.selection().canFill || kind !== "shape";
    const stroke = appearance.querySelector("[data-object-inspector-stroke]");
    stroke.hidden = !io.selection().canStroke;
    stroke.textContent = kind === "image" ? io.t("object.pictureBorder") : "Shape outline";
    const bodyField = objectInspectorEl.querySelector("[data-object-inspector-textbox]");
    bodyField.hidden = io.selection().kind !== "textbox";
    // The record's shape — camelCase, defaults omitted — is `text_box_body.mjs`'s
    // to know (HF-253: reading snake_case showed every text box as default).
    const body = io.selection().kind === "textbox" ? readTextBoxBody(io.doc().textBoxBodyProperties?.(io.selection().node)) : null;
    if (body) {
      for (const side of ["left", "top", "right", "bottom"]) setValue(bodyField.querySelector(`[data-object-textbox-inset=${side}]`), String(body.insets[side]));
      setValue(bodyField.querySelector("[data-object-textbox-anchor]"), body.verticalAnchor);
      setValue(bodyField.querySelector("[data-object-textbox-h-overflow]"), body.horizontalOverflow);
      setValue(bodyField.querySelector("[data-object-textbox-v-overflow]"), body.verticalOverflow);
      setValue(bodyField.querySelector("[data-object-textbox-autofit]"), body.autoFit);
    }
  }

  function toggleObjectInspector(open) {
    if (!objectInspectorEl) return;
    const show = open ?? objectInspectorEl.hidden;
    if (show) {
      // Opening always re-reads the model, even for the object the fields already
      // name: what the panel held may pre-date a drag, an undo or an engine edit.
      objectInspectorNode = null;
      reflectObjectInspector();
    } else {
      objectInspectorNode = null;
    }
    objectInspectorEl.hidden = !show;
  }

  /** Fail-closed precondition for every Apply button in the inspector: the numbers
   *  in the fields must belong to the object that is selected now. `reflectObject-
   *  Inspector` keeps that true on every repaint, so this only fires if some path
   *  changed the selection without one — in which case applying would write the
   *  previous object's geometry onto this one (docs/104 HF-057). It re-reads the
   *  model and refuses, rather than silently doing the wrong write or nothing. */
  function objectInspectorMatchesSelection() {
    if (!io.selection()) return false;
    if (objectInspectorNode === io.selection().node) return true;
    reflectObjectInspector();
    io.setStatus("Object properties now show the selected object — check the values, then apply", "error");
    return false;
  }

  function ensureObjectInspector() {
    if (objectInspectorEl) return objectInspectorEl;
    objectInspectorEl = document.createElement("aside");
    objectInspectorEl.className = "object-inspector side-panel";
    objectInspectorEl.hidden = true;
    objectInspectorEl.setAttribute("aria-label", "Object properties");
    objectInspectorEl.innerHTML = `
      <header class="panel-head properties-panel-head"><div class="properties-panel-heading"><span><strong class="panel-title">Object properties</strong><small data-object-inspector-kind></small></span></div><button type="button" class="panel-close" aria-label="Close object properties"><span class="ms" aria-hidden="true">close</span></button></header>
      <div class="panel-body properties-panel-body"><p class="properties-panel-intro">Exact model geometry. Changes apply as one undoable resize.</p><fieldset class="dialog-group property-section"><legend>Position</legend><label class="dialog-field">Left<span class="number-control"><input data-object-prop="left" type="number" step="0.01" /><span>in</span></span></label><label class="dialog-field">Top<span class="number-control"><input data-object-prop="top" type="number" step="0.01" /><span>in</span></span></label></fieldset><fieldset class="dialog-group property-section"><legend>Size</legend><label class="dialog-field">Width<span class="number-control"><input data-object-prop="width" type="number" min="0.1" step="0.01" /><span>in</span></span></label><label class="dialog-field">Height<span class="number-control"><input data-object-prop="height" type="number" min="0.1" step="0.01" /><span>in</span></span></label><button type="button" class="dialog-button dialog-button-primary" data-object-inspector-apply>Apply geometry</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-rotation hidden><legend>Rotation</legend><label class="dialog-field">Angle<span class="number-control"><input data-object-prop="rotation" type="number" min="0" max="360" step="1" /><span>&deg;</span></span></label><button type="button" class="dialog-button" data-object-inspector-rotation-apply>Apply rotation</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-wrap hidden><legend>Text wrapping</legend><label class="dialog-field">Wrap<select data-object-inspector-wrap-select><option value="square">Square</option><option value="tight">Tight</option><option value="through">Through</option><option value="topAndBottom">Top &amp; bottom</option><option value="behind">Behind text</option><option value="front">In front of text</option></select></label><button type="button" class="dialog-button" data-object-inspector-wrap-apply>Apply wrap</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-textbox hidden><legend>Text box body</legend><div class="property-grid-2"><label class="dialog-field">Left inset<span class="number-control"><input data-object-textbox-inset="left" type="number" min="0" step="0.01" /><span>in</span></span></label><label class="dialog-field">Right inset<span class="number-control"><input data-object-textbox-inset="right" type="number" min="0" step="0.01" /><span>in</span></span></label><label class="dialog-field">Top inset<span class="number-control"><input data-object-textbox-inset="top" type="number" min="0" step="0.01" /><span>in</span></span></label><label class="dialog-field">Bottom inset<span class="number-control"><input data-object-textbox-inset="bottom" type="number" min="0" step="0.01" /><span>in</span></span></label></div><label class="dialog-field">Vertical alignment<select data-object-textbox-anchor><option value="top">Top</option><option value="center">Center</option><option value="bottom">Bottom</option></select></label><label class="dialog-field">Horizontal overflow<select data-object-textbox-h-overflow><option value="overflow">Overflow</option><option value="clip">Clip</option></select></label><label class="dialog-field">Vertical overflow<select data-object-textbox-v-overflow><option value="overflow">Overflow</option><option value="clip">Clip</option><option value="ellipsis">Ellipsis</option></select></label><label class="dialog-field">Autofit<select data-object-textbox-autofit><option value="none">Fixed shape</option><option value="shape">Grow shape to fit</option><option value="normal">Scale text</option></select></label><button type="button" class="dialog-button" data-object-inspector-textbox-apply>Apply text box body</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-alt hidden><legend>Accessibility</legend><label class="dialog-field">Description<input data-object-inspector-alt-input type="text" maxlength="255" placeholder="Describe this object" /></label><button type="button" class="dialog-button" data-object-inspector-alt-apply>Apply description</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-appearance hidden><legend>Appearance</legend><button type="button" class="dialog-button" data-object-inspector-fill>Shape fill</button><button type="button" class="dialog-button" data-object-inspector-stroke>Shape outline</button></fieldset></div>`;
    objectInspectorEl.querySelector(".panel-close").addEventListener("click", () => toggleObjectInspector(false));
    objectInspectorEl.querySelector("[data-object-inspector-apply]").addEventListener("click", () => {
      if (!io.doc() || !io.selection()?.canResize || !objectInspectorMatchesSelection()) return;
      const rect = io.doc().objectRect(io.selection().node);
      const width = Number(objectInspectorEl.querySelector("[data-object-prop=width]").value);
      const height = Number(objectInspectorEl.querySelector("[data-object-prop=height]").value);
      const left = Number(objectInspectorEl.querySelector("[data-object-prop=left]").value);
      const top = Number(objectInspectorEl.querySelector("[data-object-prop=top]").value);
      if (rect.length < 5 || ![left, top, width, height].every(Number.isFinite) || width <= 0 || height <= 0) return;
      // An in-line object's position is the text's, not the panel's: send the
      // origin it already has. The Left/Top fields show it rounded to 0.01in,
      // and sending that back moved the origin by the rounding, which the engine
      // refuses for an in-line object — so Apply could never resize one
      // (`docs/109` UX-OB-05).
      const origin = io.selection().anchored ? [left * TWIPS_PER_INCH, top * TWIPS_PER_INCH] : [rect[1], rect[2]];
      io.runEdit(() => io.doc().resizeObject(io.selection().ref.root, origin[0] * 635, origin[1] * 635, width * TWIPS_PER_INCH * 635, height * TWIPS_PER_INCH * 635), { gate: true });
    });
    objectInspectorEl.querySelector("[data-object-inspector-rotation-apply]").addEventListener("click", () => {
      if (!io.doc() || !io.selection()?.canRotate || !objectInspectorMatchesSelection()) return;
      const degrees = Number(objectInspectorEl.querySelector("[data-object-prop=rotation]").value);
      if (!Number.isFinite(degrees)) return;
      // The SAME engine call the handle drag makes, so the two halves cannot
      // drift: a typed 30 and a dragged 30 are one value and one undo step.
      io.runEdit(() => io.doc().setObjectRotation(io.selection().node, ((degrees % 360) + 360) % 360), { gate: true });
    });
    objectInspectorEl.querySelector("[data-object-inspector-wrap-apply]").addEventListener("click", () => {
      if (!io.doc() || !io.selection()?.canWrap || !objectInspectorMatchesSelection()) return;
      const mode = objectInspectorEl.querySelector("[data-object-inspector-wrap-select]").value;
      io.runEdit(() => io.doc().setObjectWrap(io.selection().ref.root, mode), { gate: true });
    });
    objectInspectorEl.querySelector("[data-object-inspector-textbox-apply]").addEventListener("click", () => {
      if (!io.doc() || io.selection()?.kind !== "textbox" || typeof io.doc().setTextBoxBodyProperties !== "function") return;
      if (!objectInspectorMatchesSelection()) return;
      const field = (selector) => objectInspectorEl.querySelector(selector).value;
      const record = writeTextBoxBody(io.doc().textBoxBodyProperties(io.selection().node), {
        insets: Object.fromEntries(["left", "top", "right", "bottom"].map((side) => [side, field(`[data-object-textbox-inset=${side}]`)])),
        verticalAnchor: field("[data-object-textbox-anchor]"),
        horizontalOverflow: field("[data-object-textbox-h-overflow]"),
        verticalOverflow: field("[data-object-textbox-v-overflow]"),
        autoFit: field("[data-object-textbox-autofit]"),
      });
      // An inset that is not a number of inches is refused here, and SAYS so,
      // rather than reaching the engine as a record it would refuse.
      if (record === null) return io.setStatus(io.t("object.textBoxBody.invalid"), "error");
      io.runEdit(() => io.doc().setTextBoxBodyProperties(io.selection().node, record), { gate: true });
    });
    objectInspectorEl.querySelector("[data-object-inspector-alt-apply]").addEventListener("click", () => {
      if (!io.doc() || !io.selection()?.canAltText || !objectInspectorMatchesSelection()) return;
      const value = objectInspectorEl.querySelector("[data-object-inspector-alt-input]").value.trim();
      io.runEdit(() => io.doc().setObjectDescr(io.selection().node, value || null), { gate: true });
    });
    objectInspectorEl.querySelector("[data-object-inspector-fill]").addEventListener("click", () => io.openShapeFill());
    objectInspectorEl.querySelector("[data-object-inspector-stroke]").addEventListener("click", () => io.openShapeOutline());
    // Mount inside `.workarea`, not on `<body>`. As a body child it had to be
    // `position: fixed`, which made it float OVER the canvas and over the footer
    // instead of taking space in the row — so the page never shifted aside for it
    // the way it does for the outline/pages/review panels, and the status bar was
    // covered. `.workarea` is a flex row and `.side-panel` is already a flex item,
    // so joining it gives the shift and the footer clearance for free.
    (document.querySelector(".workarea") ?? document.body).appendChild(objectInspectorEl);
    return objectInspectorEl;
  }
  return {
    /** Creates (once) and returns the panel element. */
    ensure: ensureObjectInspector,
    /** Re-reads the model into the fields. Safe to call on every repaint. */
    reflect: reflectObjectInspector,
    /** Shows or hides the panel; `undefined` toggles. */
    toggle: toggleObjectInspector,
    /** Whether the panel is on screen — what tells a repaint to re-read it. */
    isOpen: () => !!objectInspectorEl && !objectInspectorEl.hidden,
    /**
     * Opens the panel and focuses the control the caller names, so Layout ▸
     * Wrap text lands on the wrap control rather than on the first field. A
     * no-op without a selected object; the button is disabled then.
     */
    openAt(selector) {
      const selection = io.selection();
      if (!selection || selection.mode !== "selected") return;
      ensureObjectInspector();
      toggleObjectInspector(true);
      queueMicrotask(() =>
        objectInspectorEl?.querySelector(selector)?.focus({ preventScroll: true }),
      );
    },
  };
}
