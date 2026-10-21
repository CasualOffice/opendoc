// The toolbar that appears beside a selected image, shape, text box or chart.
//
// Google Docs puts exactly this under a selected image — the wrap modes as one
// row of icons, then the object's own actions — and Word's Layout Options flyout
// is the same control with a button in front of it. The bar offers "In line" as
// the first wrap mode for every top-level object, because an inserted picture is
// INLINE and the object every user meets first must not be the one without a
// wrap control (`setObjectAnchorKind` makes the conversion a real node rewrite).
//
// `docs/172` is the design. In short, what changed and why:
//
//   * PLACEMENT is the shared anchored-popover mechanism (`object_anchor.mjs`):
//     below the object, as Docs places its image toolbar, flipped above when
//     there is no room below, centred on the object and kept inside the page
//     view, clear of every grip, and hidden when the object is out of view. It
//     used to sit above the object, left-aligned, 1,220px wide over a 16px
//     picture, and pinned to the top of the view when the object scrolled up —
//     covering the heading the reader was looking at.
//   * It is ONE TOOLBAR (WAI-ARIA): one tab stop, arrow keys inside, Escape back
//     to where the keyboard was, and Alt+F10 to reach it from the document, the
//     chord editors with a contextual toolbar use (TinyMCE, CKEditor). It had no
//     role, no name and no keyboard path at all.
//   * It hides while a pointer gesture is in flight on the pages, as Word's and
//     Docs' do, so it never sits where the object used to be during a drag.
//   * Every control is an icon with its name on the tooltip AND the accessible
//     name, localised. A disabled control stays focusable (`aria-disabled`) so
//     its reason can be read; it does nothing when pressed.
//
// Its whole contact with the application is `io` — the state it renders, and the
// verbs it calls. It reads NOTHING from the document itself: the caller gathers
// the arrange state once per repaint (`objectPosition` is O(document) with one
// walk) and hands it in, so a bar render can never become a per-object query in
// a loop.
//
// Cost: O(controls) per render — a fixed ~20 nodes for ONE object; a scroll is
// one `objectFrame` read and one placement.
import { applyAnchoredPlacement, bindToolbarKeys, focusToolbarControl, suspendDuringPointerGesture, toolbarControls } from "./object_anchor.mjs";
import { GRIP_REACH } from "./object_guides.mjs";
// Own line (anti-conflict): the rotation grip's reach and the turned bounds.
import { ROTATE_GRIP_REACH, rotatedBounds } from "./object_rotate.mjs";
import { OBJECT_LABELS } from "./object_traversal.mjs";
import { WRAP_CHOICES, activeWrapChoice, wrapAvailability } from "./object_arrange.mjs";

/** The chord that moves the keyboard from the document into the bar. */
export const OBJECT_BAR_CHORD = { key: "F10", altKey: true };

/** Keeps a press on a bar control from deselecting the object: the canvas
 *  pointerdown deselect is what every control on this bar has to dodge. */
function keepSelection(btn) {
  btn.addEventListener("pointerdown", (event) => event.preventDefault());
}

/** Builds one bar action button (icon + label). `pointerdown` is prevented so
 *  clicking it never deselects the object. */
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
  keepSelection(btn);
  btn.addEventListener("click", onClick);
  return btn;
}

/** An icon-only bar action: the name rides on the tooltip AND the accessible
 *  name, the pattern Position, Arrange and Rotate already used. */
function objectBarIconButton(icon, title, onClick, danger = false) {
  const btn = objectBarButton(icon, "", title, onClick, danger);
  btn.classList.add("object-bar-icon");
  btn.lastElementChild.remove(); // the empty label span
  return btn;
}

/** Names the shared outline button for what it is on THIS object: Word calls a
 *  picture's outline its Picture Border, and the menu, the bar and the
 *  inspector must all say the same word (`docs/109` HF-254). */
function nameOutlineButton(btn, kind, t) {
  const picture = kind === "image";
  const title = picture ? t("object.pictureBorder") : "Shape outline";
  btn.title = title;
  btn.setAttribute("aria-label", title);
  const label = btn.querySelector(".ms + span");
  if (label) label.textContent = picture ? t("object.pictureBorder.short") : "Outline";
  return btn;
}

/** The bar's accessible name for one object kind. */
function barLabel(kind, t) {
  const key = `object.bar.${OBJECT_LABELS[kind] ? kind : "object"}`;
  return t(key);
}

/** Whether a keydown is the chord that enters the bar. */
export function isObjectBarChord(event) {
  return event.key === OBJECT_BAR_CHORD.key && event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey;
}

export function createObjectBar(io) {
  let barEl = null;
  /** Where the keyboard was when Alt+F10 brought it into the bar, so Escape can
   *  hand it back (WAI-ARIA: a toolbar entered by a shortcut returns focus). */
  let returnFocus = null;
  const dir = () => (document.documentElement.dir === "rtl" ? "rtl" : "ltr");

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
      const glyph = document.createElement("span");
      glyph.className = "ms";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = choice.icon;
      btn.appendChild(glyph);
      btn.title = label;
      btn.setAttribute("aria-label", label);
      btn.setAttribute("aria-pressed", String(choice.value === active));
      if (!availability.available) {
        const reason = io.t(availability.reasonKey);
        // `aria-disabled`, not `disabled`: a disabled button cannot take focus,
        // so a keyboard user walking the toolbar could never reach the reason.
        btn.setAttribute("aria-disabled", "true");
        const named = `${label} — ${reason}`;
        btn.title = named;
        btn.setAttribute("aria-label", named);
      }
      keepSelection(btn);
      btn.addEventListener("click", () => {
        if (btn.getAttribute("aria-disabled") === "true") return;
        io.setWrap(choice.value);
      });
      row.appendChild(btn);
    }
    return row;
  }

  function ensureBar() {
    if (barEl) return barEl;
    barEl = document.createElement("div");
    barEl.className = "object-context-bar";
    barEl.hidden = true;
    document.body.appendChild(barEl);
    bindToolbarKeys(barEl, {
      dir,
      onEscape: () => {
        const target = returnFocus;
        returnFocus = null;
        if (target?.isConnected) target.focus();
        else io.returnFocus?.();
      },
    });
    suspendDuringPointerGesture(barEl, () => io.pagesElement?.() ?? document.getElementById("pages"), () => reposition());
    // Alt+F10 from the document puts the keyboard on the bar's first control.
    // Capture phase, so the editor's own keydown handling never sees the chord.
    window.addEventListener(
      "keydown",
      (event) => {
        if (!isObjectBarChord(event) || !barEl || barEl.hidden) return;
        if (barEl.contains(document.activeElement)) return;
        event.preventDefault();
        event.stopPropagation();
        returnFocus = document.activeElement;
        focusToolbarControl(barEl, toolbarControls(barEl)[0]);
      },
      true,
    );
    return barEl;
  }

  /** Shows/positions the bar beside the selected object. */
  function update() {
    io.ensureInspector();
    ensureBar();
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
    // Rebuilding the controls must not drop the keyboard: a wrap chip pressed
    // with Enter re-renders the bar, and focus would fall to <body>.
    const focusedWrap = barEl.contains(document.activeElement) ? document.activeElement : null;
    const focusKey = focusedWrap?.dataset.wrap ?? focusedWrap?.getAttribute("aria-label") ?? null;

    barEl.replaceChildren();
    barEl.setAttribute("aria-label", barLabel(selection.kind, io.t));
    barEl.dataset.objectKind = selection.kind;
    // While a crop is live the bar says what the black grips do.
    const cropping = io.croppingNode() === selection.node;
    const state = io.arrangeState();
    if (cropping) {
      const hint = document.createElement("small");
      hint.textContent = io.t("object.cropHint");
      barEl.appendChild(hint);
    } else if (state) {
      barEl.appendChild(wrapRow(state));
    }

    const actions = document.createElement("div");
    actions.className = "object-bar-actions";
    // Position / Arrange / Rotate are Word's Arrange group, on the object's own
    // bar because that is where Docs puts the same decisions. Their buttons are
    // module-level in the caller so their popovers stay registered across the
    // bar's re-renders — the same reason Fill and Outline are.
    if (state && !cropping) {
      actions.appendChild(io.positionButton());
      actions.appendChild(io.arrangeButton());
      actions.appendChild(io.rotateButton());
    }
    if (selection.kind === "chart") {
      // Word's Chart Design ▸ Edit Data, on the bar because that is where Docs
      // puts a selected object's own actions.
      actions.appendChild(objectBarIconButton("table_chart", io.t("chart.editDataTitle"), io.openChartData));
    }
    if (selection.canAltText) {
      actions.appendChild(objectBarIconButton("description", io.t("object.bar.altText"), io.openAltText));
    }
    // Word's Shape Format tab reduces to its two live controls; its Picture
    // Format tab's Picture Border is the outline half, on a picture.
    const formats = selection.kind === "shape" || selection.kind === "image";
    if (formats && (selection.canFill || selection.canStroke)) {
      if (selection.canFill && selection.kind === "shape") actions.appendChild(io.fillButton());
      if (selection.canStroke) actions.appendChild(nameOutlineButton(io.outlineButton(), selection.kind, io.t));
      io.reflectShapeSwatches();
    }
    if (selection.canCrop) {
      // Crop is a picture-only operation; a text box has no source rectangle.
      const cropBtn = objectBarIconButton(
        cropping ? "check" : "crop",
        io.t(cropping ? "object.bar.cropApply" : "object.bar.crop"),
        io.enterCrop,
      );
      if (cropping) cropBtn.classList.add("is-active");
      actions.appendChild(cropBtn);
    }
    // Word's Change Picture / Docs' Replace image (`docs/109` HF-252).
    if (!cropping && io.canChangePicture?.(selection)) {
      actions.appendChild(objectBarIconButton("add_photo_alternate", io.t("object.changePicture"), io.changePicture));
    }
    if (selection.canResize) {
      actions.appendChild(
        objectBarIconButton("tune", io.t("object.bar.properties"), () => io.toggleInspector(true)),
      );
    }
    if (selection.canDelete) {
      actions.appendChild(objectBarIconButton("delete", io.t("object.bar.delete"), io.deleteObject, true));
    }
    if (actions.childElementCount > 0) {
      if (barEl.childElementCount > 0) {
        const divider = document.createElement("span");
        divider.className = "object-bar-divider";
        divider.setAttribute("aria-hidden", "true");
        barEl.appendChild(divider);
      }
      barEl.appendChild(actions);
    }
    // One tab stop: the first control, or the one the keyboard was on.
    const controls = toolbarControlsOf(barEl);
    const keep =
      focusKey &&
      controls.find((control) => control.dataset.wrap === focusKey || control.getAttribute("aria-label") === focusKey);
    for (const control of controls) control.tabIndex = -1;
    const stop = keep ?? controls[0];
    if (stop) stop.tabIndex = 0;
    reposition();
    if (keep && focusedWrap) keep.focus();
    // The panel is a live view of the selected object, so it is re-read here —
    // this runs on every repaint, which is what makes a drag-resize, a nudge and
    // a change of selection all show up in the fields (docs/104 HF-057).
    if (io.inspectorOpen()) io.reflectInspector();
  }

  /** All the bar's controls, visible or not yet laid out (the bar may still be
   *  hidden when the tab stop is assigned). */
  function toolbarControlsOf(el) {
    return [...el.querySelectorAll("button")];
  }

  /** Places the bar against the object it acts on, and takes it off screen when
   *  that object is not on screen.
   *
   *  The bar is body-level with fixed viewport coordinates, so nothing about a
   *  scroll moves it on its own (docs/104 HF-058). Separated from `update` so a
   *  scroll only re-measures; it never rebuilds the bar's contents. */
  function reposition() {
    if (!barEl || !io.doc()) return;
    const selection = io.selection();
    // `objectFrame`, not `objectRect`: the same page-local rectangle plus the
    // angle, in the SAME one call. The angle must not be bought with
    // `arrangeState()` here — that is O(document) with two walks, and this runs
    // on every scroll and every window resize (`docs/107` §4).
    const frame = selection?.mode === "selected" ? io.doc().objectFrame?.(selection.node) ?? [] : [];
    const rect = frame.slice(0, 5);
    const page = rect.length >= 5 ? io.pages()[rect[0] - 1] : null;
    if (!page) {
      barEl.hidden = true;
      return;
    }
    const { rect: pageRect, sx, sy } = io.scaleOf(page);
    // The bar clears the object's TURNED outline, not its frame: a rotated
    // picture's corners stick out past the box the model stores.
    const turned = rotatedBounds({ x: rect[1], y: rect[2], w: rect[3], h: rect[4] }, (frame[5] ?? 0) / 1000);
    const left = pageRect.left + turned.x * sx;
    const top = pageRect.top + turned.y * sy;
    const anchor = { left, top, right: left + turned.w * sx, bottom: top + turned.h * sy };
    applyAnchoredPlacement(barEl, anchor, io.viewportRect(), {
      prefer: "below",
      dir: dir(),
      // A rotatable object carries a grip ABOVE it, further out than any resize
      // grip reaches; a bar inside a grip's target means `elementFromPoint`
      // answers with the bar and the grip cannot be grabbed.
      clearance: { above: selection.canRotate ? ROTATE_GRIP_REACH : GRIP_REACH, below: GRIP_REACH },
    });
  }

  return { update, reposition, element: () => barEl };
}
