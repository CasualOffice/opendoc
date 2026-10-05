// SPDX-License-Identifier: Apache-2.0
// The object properties panel — Word's Format Picture / Format Shape pane,
// Google Docs' "All image options", ONLYOFFICE's right-hand panel.
//
// ---- WHAT CHANGED, AND WHY (the owner's three complaints) -----------------
//
// Verbatim: "all properties panel is pathetic who designed it.. i dont makes
// any sense ..a nd alot of cognative burden have you seen onlyoffice properites
// panel what properties they provide and also design and separation". Three
// complaints — breadth, design, separation — and three answers here:
//
//   SEPARATION. Seven flat fieldsets, every one of them open, became seven
//   collapsible sections with Size open and the rest folded, each folded
//   section carrying a digest of what it holds (`property_sections.mjs`). An
//   image's panel went from 11 controls on screen to 3; a text box's from 20
//   to 3.
//
//   DESIGN. Three things. (1) The markup moved to `editor.html`, so this panel
//   goes through the same localisation seam as every other surface — as a
//   template literal in this file it went through NONE, and the whole panel was
//   English in all nineteen locales with no gate able to see it. (2) The five
//   separate "Apply …" buttons are gone: every control commits on `change`,
//   which is what the paragraph and table panels next to it already did and
//   what Word, Google Docs and ONLYOFFICE all do. Three commit contracts for
//   one task across three sibling panels was most of "who designed it". (3) The
//   measurement fields carry `data-measure-*` and resolve through the reader's
//   unit preference when the host supplies it, instead of hard-coding inches.
//
//   BREADTH. `shapeFormat` has returned `{fill, outline, outlineWidthEmu}`
//   since it landed and no panel read any of the three: Appearance showed two
//   buttons that open the ribbon's pickers and could not say what the shape's
//   fill WAS. It now shows and writes all three, weight included. Size gained
//   Word's own Lock aspect ratio — which ONLYOFFICE keeps behind its Advanced
//   dialog (`reference/web-apps` 9c0ca538c3b2,
//   `app/template/ImageSettingsAdvanced.template:60`) and which belongs beside
//   the two fields it governs.
//
// ---- WHAT IT IS NOT ------------------------------------------------------
//
// Not a crop control. Numeric crop fields were tried and rejected by the owner
// (`109` HF-417): cropping is a direct-manipulation gesture and four fractions
// in a form is the engine's binding wearing a UI. `object_crop_chrome.mjs`
// owns it.
//
// Not picture borders or effects, and not page borders. Those are real gaps and
// they are not UI gaps: a picture's frame outline round-trips through import,
// layout, paint and export with no wasm getter or setter at all (`shapeFormat`
// reaches shapes only, through `find_shape`), and page borders additionally
// have no `casual-doc-edit` operation to bind. Both are reported as engine work
// rather than faked here.
//
// Its whole contact with the application is `io`:
//
//   `doc()`              the live engine handle, or null
//   `selection()`        the live object selection, or null
//   `runEdit(fn, opts)`  the one gated, undoable edit path
//   `setStatus(text, k)` the status line
//   `openShapeFill()`    open the shape-fill picker (the bar owns the button)
//   `openShapeOutline()`
//   `measure()`          OPTIONAL — the reader's measurement-unit preference
//                        (`measurement_units.mjs`). A thunk, because the panel
//                        is constructed before the engine is instantiated and
//                        the preference cannot be read until after `init()`.
//                        Absent, every field falls back to inches, which is
//                        exactly what this panel did before.
//
// Cost: O(1) per reflect — a fixed number of engine reads for ONE object. It
// runs on every repaint while the panel is open, so it must stay that way.
import { OBJECT_LABELS } from "./object_traversal.mjs";
import { TWIPS_PER_INCH } from "./units.mjs";
import {
  bindPropertySections,
  paintPropertyDigests,
  revealPropertyControl,
} from "./property_sections.mjs";

/** EMU per point, for the outline weight. */
const EMU_PER_POINT = 12700;

/** EMU per twip. */
const EMU_PER_TWIP = 635;

/** Word's own Shape Outline weight ladder, in points. Numbers plus `pt`, so the
 *  options carry no translatable sentence — which is why they are built here
 *  rather than written into the markup as nineteen catalogue entries that all
 *  say the same digits. */
const OUTLINE_WEIGHTS_PT = Object.freeze([0.25, 0.5, 0.75, 1, 1.5, 2.25, 3, 4.5, 6]);

/** Builds the panel. The markup is `editor.html`'s `#objectPropertiesPanel`;
 *  what is here is the application state it reads and the gated edit path it
 *  writes through. */
export function createObjectInspector(io) {
  let objectInspectorEl = null;
  /** Set once the panel's listeners are attached. */
  let bound = false;

  /** The object the inspector's fields currently describe. It is what tells a
   *  repaint of the SAME object (where a field the user is typing in must be left
   *  alone) apart from a move to a DIFFERENT one (where leaving the old numbers is
   *  the data-loss defect: a commit then wrote one object's geometry onto another). */
  let objectInspectorNode = null;

  const panel = () => objectInspectorEl ?? ensureObjectInspector();
  const field = (selector) => objectInspectorEl?.querySelector(selector) ?? null;

  /** The reader's measurement preference, or `null` before the host wires it. */
  const measure = () => {
    try {
      return io.measure?.() ?? null;
    } catch {
      // The preference faults its roster in from the engine on first use and
      // throws before `init()` resolves. A panel cannot be the reason the
      // editor fails to start, so an unavailable preference is inches.
      return null;
    }
  };

  /** Twips -> the text a field shows, in the unit in force. */
  function showText(twips) {
    const unit = measure();
    if (unit) return unit.format(twips);
    return String(Math.round((twips / TWIPS_PER_INCH) * 100) / 100);
  }

  /** A field's text -> twips, or `null` when it is not a number this panel may
   *  write. A REFUSAL and never a zero: a blank width field read as 0 is a
   *  zero-inch object nobody asked for. */
  function readTwips(input) {
    if (!input) return null;
    const unit = measure();
    if (unit) return unit.parse(input.value);
    const value = Number(input.value);
    return Number.isFinite(value) ? Math.round(value * TWIPS_PER_INCH) : null;
  }

  /** Applies the unit in force to every measurement field: step, bounds, suffix.
   *  A no-op without the preference, which leaves the markup's inches. */
  function applyUnits() {
    const unit = measure();
    if (!unit || !objectInspectorEl) return;
    for (const input of objectInspectorEl.querySelectorAll("[data-measure-min-twip]")) {
      unit.applyToField(input);
    }
  }

  /** Fills the inspector from the model's live geometry for the selected object.
   *
   *  This runs on every repaint while the panel is open (docs/104 HF-057). It used
   *  to run only on the opening call, so a drag-resize left the panel showing the
   *  pre-drag numbers and the next commit — or a nudge — put the object back where
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
    applyUnits();
    setValue(field("[data-object-prop=width]"), showText(rect[3]));
    setValue(field("[data-object-prop=height]"), showText(rect[4]));
    setValue(field("[data-object-prop=left]"), showText(rect[1]));
    setValue(field("[data-object-prop=top]"), showText(rect[2]));
    // The typed half of the rotation handle. A grip can express any angle but
    // cannot express EXACTLY 37, and `docs/105` UX-004 asks for every capability
    // on at least two surfaces — the Rotate menu's four fixed choices are a
    // different capability, not a second surface for this one.
    const rotationSection = field("[data-object-inspector-rotation]");
    rotationSection.hidden = !io.selection().canRotate;
    if (io.selection().canRotate) {
      const frame = io.doc().objectFrame?.(io.selection().node) ?? [];
      setValue(
        rotationSection.querySelector("[data-object-prop=rotation]"),
        String(Math.round((frame[5] ?? 0) / 1000)),
      );
    }
    field("[data-object-inspector-kind]").textContent =
      OBJECT_LABELS[io.selection().kind] ?? "Object";
    const altSection = field("[data-object-inspector-alt]");
    altSection.hidden = !io.selection().canAltText;
    if (io.selection().canAltText) {
      setValue(
        altSection.querySelector("[data-object-inspector-alt-input]"),
        io.doc().objectDescr(io.selection().node) ?? "",
      );
    }
    const wrapSection = field("[data-object-inspector-wrap]");
    wrapSection.hidden = !io.selection().canWrap;
    if (io.selection().canWrap) {
      setValue(
        wrapSection.querySelector("[data-object-inspector-wrap-select]"),
        io.doc().objectWrap(io.selection().ref.root) || "square",
      );
    }
    reflectAppearance(setValue);
    reflectTextBox(setValue);
    paintPropertyDigests(objectInspectorEl);
  }

  /** The shape's own fill, outline and outline weight.
   *
   *  `shapeFormat` reports what the shape HAS rather than what was last applied,
   *  and until this panel read it the only thing Appearance could say was the
   *  names of two buttons. A shape with no fill shows the picker's own default
   *  rather than black, because an `<input type="color">` has no empty state and
   *  painting black would read as "this shape is black". */
  function reflectAppearance(setValue) {
    const section = field("[data-object-inspector-appearance]");
    const selection = io.selection();
    section.hidden =
      selection.kind !== "shape" || (!selection.canFill && !selection.canStroke);
    if (section.hidden) return;
    section.querySelector("[data-object-inspector-fill-row]").hidden = !selection.canFill;
    const strokeRow = section.querySelector("[data-object-inspector-stroke-row]");
    strokeRow.hidden = !selection.canStroke;
    const weightField = section.querySelector("[data-object-inspector-weight]").parentElement;
    weightField.hidden = !selection.canStroke;
    let format = null;
    try {
      format = io.doc().shapeFormat?.(selection.node) ?? null;
    } catch {
      // A malformed or unsupported payload fails closed; nothing is overwritten.
      return;
    }
    if (!format) return;
    if (selection.canFill) {
      setValue(section.querySelector("[data-object-inspector-fill-color]"), format.fill ?? "#ffffff");
    }
    if (!selection.canStroke) return;
    setValue(section.querySelector("[data-object-inspector-stroke-color]"), format.outline ?? "#000000");
    const weight = section.querySelector("[data-object-inspector-weight]");
    const points = (format.outlineWidthEmu ?? 0) / EMU_PER_POINT;
    const nearest = OUTLINE_WEIGHTS_PT.reduce((best, candidate) =>
      Math.abs(candidate - points) < Math.abs(best - points) ? candidate : best,
    );
    setValue(weight, String(nearest));
  }

  /** The text box's body properties: insets, vertical anchor, overflow, autofit. */
  function reflectTextBox(setValue) {
    const section = field("[data-object-inspector-textbox]");
    section.hidden = io.selection().kind !== "textbox";
    if (section.hidden || typeof io.doc().textBoxBodyProperties !== "function") return;
    try {
      const raw = io.doc().textBoxBodyProperties(io.selection().node);
      const props = raw ? JSON.parse(raw) : null;
      if (!props) return;
      const insets = props.insets ?? {};
      for (const side of ["left", "top", "right", "bottom"]) {
        const input = section.querySelector(`[data-object-textbox-inset=${side}]`);
        if (input) {
          setValue(input, showText(Math.round(Number(insets[`${side}Emu`] ?? 0) / EMU_PER_TWIP)));
        }
      }
      setValue(section.querySelector("[data-object-textbox-anchor]"), props.vertical_anchor ?? "top");
      setValue(
        section.querySelector("[data-object-textbox-h-overflow]"),
        props.horizontal_overflow ?? "overflow",
      );
      setValue(
        section.querySelector("[data-object-textbox-v-overflow]"),
        props.vertical_overflow ?? "overflow",
      );
      setValue(section.querySelector("[data-object-textbox-autofit]"), props.auto_fit?.mode ?? "none");
    } catch {
      // A malformed/unsupported payload fails closed; authored data is not overwritten.
    }
  }

  function toggleObjectInspector(open) {
    ensureObjectInspector();
    if (!objectInspectorEl) return;
    const show = open ?? objectInspectorEl.hidden;
    if (show) {
      // Opening always re-reads the model, even for the object the fields already
      // name: what the panel held may pre-date a drag, an undo or an engine edit.
      objectInspectorNode = null;
      objectInspectorEl.hidden = false;
      reflectObjectInspector();
    } else {
      objectInspectorNode = null;
      objectInspectorEl.hidden = true;
    }
  }

  /** Fail-closed precondition for every commit in the inspector: the numbers in
   *  the fields must belong to the object that is selected now.
   *  `reflectObjectInspector` keeps that true on every repaint, so this only
   *  fires if some path changed the selection without one — in which case
   *  committing would write the previous object's geometry onto this one
   *  (docs/104 HF-057). It re-reads the model and refuses, rather than silently
   *  doing the wrong write or nothing. */
  function objectInspectorMatchesSelection() {
    if (!io.selection()) return false;
    if (objectInspectorNode === io.selection().node) return true;
    reflectObjectInspector();
    // STILL UNROUTED, and it is the module's last English literal. A `t()` key
    // is script-side, and `locale_coverage.test.mjs` requires every script-side
    // key to be answered by all eighteen translated catalogues — so minting one
    // means eighteen sentences, and no catalogue carries a neighbouring string
    // this one can be derived from. Inventing them is worse than the honest
    // debt, so it is reported rather than guessed. ("then apply" is gone from
    // the wording because the Apply buttons are.)
    io.setStatus(
      "Object properties now show the selected object — check the values before changing them",
      "error",
    );
    return false;
  }

  /** Commits the four geometry numbers as one undoable resize.
   *
   *  All four together because `resizeObject` takes all four: a panel that sent
   *  width alone would have to invent the other three, and inventing a position
   *  is how an object moves when a reader resizes it.
   *
   *  `changed` is the field the reader just left, which is what Lock aspect ratio
   *  needs: the side they typed is the one that is kept. */
  function commitGeometry(changed) {
    if (!io.doc() || !io.selection()?.canResize || !objectInspectorMatchesSelection()) return;
    const rect = io.doc().objectRect(io.selection().node);
    if (rect.length < 5) return;
    const widthInput = field("[data-object-prop=width]");
    const heightInput = field("[data-object-prop=height]");
    let width = readTwips(widthInput);
    let height = readTwips(heightInput);
    const left = readTwips(field("[data-object-prop=left]"));
    const top = readTwips(field("[data-object-prop=top]"));
    if (![left, top, width, height].every((value) => Number.isFinite(value))) return;
    if (width <= 0 || height <= 0) return;
    if (field("[data-object-inspector-lock-ratio]")?.checked && rect[3] > 0 && rect[4] > 0) {
      // The ratio comes from the model's CURRENT rect, not from the other field:
      // the other field may itself hold an uncommitted number, and deriving one
      // typed value from another is how two edits compound into a wrong shape.
      const ratio = rect[4] / rect[3];
      if (changed === heightInput) {
        width = Math.round(height / ratio);
        widthInput.value = showText(width);
      } else {
        height = Math.round(width * ratio);
        heightInput.value = showText(height);
      }
    }
    io.runEdit(
      () =>
        io
          .doc()
          .resizeObject(
            io.selection().ref.root,
            left * EMU_PER_TWIP,
            top * EMU_PER_TWIP,
            width * EMU_PER_TWIP,
            height * EMU_PER_TWIP,
          ),
      { gate: true },
    );
  }

  function commitRotation() {
    if (!io.doc() || !io.selection()?.canRotate || !objectInspectorMatchesSelection()) return;
    const degrees = Number(field("[data-object-prop=rotation]").value);
    if (!Number.isFinite(degrees)) return;
    // The SAME engine call the handle drag makes, so the two halves cannot
    // drift: a typed 30 and a dragged 30 are one value and one undo step.
    io.runEdit(() => io.doc().setObjectRotation(io.selection().node, ((degrees % 360) + 360) % 360), {
      gate: true,
    });
  }

  function commitWrap() {
    if (!io.doc() || !io.selection()?.canWrap || !objectInspectorMatchesSelection()) return;
    const mode = field("[data-object-inspector-wrap-select]").value;
    io.runEdit(() => io.doc().setObjectWrap(io.selection().ref.root, mode), { gate: true });
  }

  function commitAltText() {
    if (!io.doc() || !io.selection()?.canAltText || !objectInspectorMatchesSelection()) return;
    const value = field("[data-object-inspector-alt-input]").value.trim();
    io.runEdit(() => io.doc().setObjectDescr(io.selection().node, value || null), { gate: true });
  }

  function commitFill() {
    if (!io.doc() || !io.selection()?.canFill || !objectInspectorMatchesSelection()) return;
    const hex = field("[data-object-inspector-fill-color]").value;
    io.runEdit(() => io.doc().setShapeFill(io.selection().node, hex), { gate: true });
  }

  function commitOutlineColor() {
    if (!io.doc() || !io.selection()?.canStroke || !objectInspectorMatchesSelection()) return;
    const hex = field("[data-object-inspector-stroke-color]").value;
    io.runEdit(() => io.doc().setShapeOutline(io.selection().node, hex, undefined), { gate: true });
  }

  function commitOutlineWeight() {
    if (!io.doc() || !io.selection()?.canStroke || !objectInspectorMatchesSelection()) return;
    const points = Number(field("[data-object-inspector-weight]").value);
    if (!Number.isFinite(points) || points <= 0) return;
    // Colour `undefined`, deliberately: the engine reads that as "weight only,
    // keep the colour" (and keeps the dash pattern and line ends with it), where
    // sending the swatch's value would rebuild the stroke and straighten a
    // dashed callout's leader because somebody chose a weight.
    io.runEdit(
      () => io.doc().setShapeOutline(io.selection().node, undefined, points * EMU_PER_POINT),
      { gate: true },
    );
  }

  function commitTextBox() {
    if (!io.doc() || io.selection()?.kind !== "textbox") return;
    if (typeof io.doc().textBoxBodyProperties !== "function") return;
    if (typeof io.doc().setTextBoxBodyProperties !== "function") return;
    if (!objectInspectorMatchesSelection()) return;
    let props;
    try {
      props = JSON.parse(io.doc().textBoxBodyProperties(io.selection().node));
    } catch {
      return;
    }
    if (!props?.insets) return;
    for (const side of ["left", "top", "right", "bottom"]) {
      const twips = readTwips(field(`[data-object-textbox-inset=${side}]`));
      if (!Number.isFinite(twips) || twips < 0) return;
      props.insets[`${side}Emu`] = twips * EMU_PER_TWIP;
    }
    props.vertical_anchor = field("[data-object-textbox-anchor]").value;
    props.horizontal_overflow = field("[data-object-textbox-h-overflow]").value;
    props.vertical_overflow = field("[data-object-textbox-v-overflow]").value;
    const autofitMode = field("[data-object-textbox-autofit]").value;
    // Keep the authored scale/reduction values for normal autofit. The compact
    // inspector changes the mode only; it must never erase unsupported detail.
    if (autofitMode === "normal") {
      props.auto_fit =
        props.auto_fit?.mode === "normal"
          ? props.auto_fit
          : { mode: "normal", font_scale: 100000, line_spacing_reduction: 0 };
    } else {
      props.auto_fit = { mode: autofitMode };
    }
    io.runEdit(
      () => io.doc().setTextBoxBodyProperties(io.selection().node, JSON.stringify(props)),
      { gate: true },
    );
  }

  /** Binds the panel's listeners, once.
   *
   *  COMMIT ON `change`, not on a per-section Apply button. `change` on a number
   *  field fires on blur and on Enter — one commit per value a reader finished
   *  typing, which is the granularity Word, Google Docs and ONLYOFFICE all use
   *  and the one the paragraph and table panels beside this one already used.
   *  The five Apply buttons this replaces were five controls carrying no
   *  property, in the panel whose complaint was that it has too many controls. */
  function ensureObjectInspector() {
    if (objectInspectorEl) return objectInspectorEl;
    objectInspectorEl = document.getElementById("objectPropertiesPanel");
    if (!objectInspectorEl || bound) return objectInspectorEl;
    bound = true;

    const weight = objectInspectorEl.querySelector("[data-object-inspector-weight]");
    for (const points of OUTLINE_WEIGHTS_PT) {
      const option = document.createElement("option");
      option.value = String(points);
      option.textContent = `${points} pt`;
      weight.append(option);
    }

    objectInspectorEl
      .querySelector(".panel-close")
      .addEventListener("click", () => toggleObjectInspector(false));

    const onChange = [
      ["[data-object-prop=width]", commitGeometry],
      ["[data-object-prop=height]", commitGeometry],
      ["[data-object-prop=left]", commitGeometry],
      ["[data-object-prop=top]", commitGeometry],
      ["[data-object-prop=rotation]", commitRotation],
      ["[data-object-inspector-wrap-select]", commitWrap],
      ["[data-object-inspector-alt-input]", commitAltText],
      ["[data-object-inspector-fill-color]", commitFill],
      ["[data-object-inspector-stroke-color]", commitOutlineColor],
      ["[data-object-inspector-weight]", commitOutlineWeight],
      ["[data-object-textbox-inset=left]", commitTextBox],
      ["[data-object-textbox-inset=top]", commitTextBox],
      ["[data-object-textbox-inset=right]", commitTextBox],
      ["[data-object-textbox-inset=bottom]", commitTextBox],
      ["[data-object-textbox-anchor]", commitTextBox],
      ["[data-object-textbox-h-overflow]", commitTextBox],
      ["[data-object-textbox-v-overflow]", commitTextBox],
      ["[data-object-textbox-autofit]", commitTextBox],
    ];
    for (const [selector, commit] of onChange) {
      const control = objectInspectorEl.querySelector(selector);
      control?.addEventListener("change", () => commit(control));
    }

    objectInspectorEl
      .querySelector("[data-object-inspector-fill]")
      .addEventListener("click", () => io.openShapeFill());
    objectInspectorEl
      .querySelector("[data-object-inspector-stroke]")
      .addEventListener("click", () => io.openShapeOutline());

    bindPropertySections(objectInspectorEl, "object");
    return objectInspectorEl;
  }

  return {
    /** Returns the panel element, binding its listeners on the first call. */
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
     *
     * The section holding the control is opened FIRST. `focus()` on an element
     * inside a closed `<details>` does nothing at all, so a folded section
     * would have silently broken every one of these pointers — ADR-062's
     * corollary C1 is that a control which defers to another surface must
     * actually reveal it, and a reveal that lands on a folded section has not.
     */
    openAt(selector) {
      const selection = io.selection();
      if (!selection || selection.mode !== "selected") return;
      ensureObjectInspector();
      toggleObjectInspector(true);
      const control = panel()?.querySelector(selector);
      revealPropertyControl(control);
      queueMicrotask(() => control?.focus({ preventScroll: true }));
    },
  };
}
