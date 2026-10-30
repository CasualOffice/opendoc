// Content controls: Word's Developer ▸ Controls, on the Insert band
// (`docs/109` RM-34; `docs/153` page.contentControls, shell.content-controls,
// shell.form-fields).
//
// THE SHAPE, FROM THE REFERENCES.
//
//   * Word: Developer ▸ Controls is seven insert buttons (Rich Text, Plain Text,
//     Picture, Check Box, Combo Box, Drop-Down List, Date Picker) and a
//     Properties button; right-clicking a control offers Remove Content Control
//     and Properties. A drop-down, combo box or date picker opens its chooser
//     from the control (Alt+Down from the keyboard); a picture control opens a
//     file chooser when clicked; typing over a control's placeholder replaces it.
//   * ONLYOFFICE: Insert ▸ Content Controls is ONE dropdown holding the same
//     kinds plus "Remove content control" and "Settings"
//     (`apps/documenteditor/main/app/view/Toolbar.js`, `btnContentControls`),
//     and its settings dialog carries a placeholder field Word's does not.
//
// So: one dropdown on the Insert band (ONLYOFFICE's economy, which the band's
// width needs), Word's seven kinds in Word's order, Word's Properties dialog
// with ONLYOFFICE's placeholder field, and Word's fill gestures. The two form
// commands — Protect form and Form roles — sit at the foot of the same
// dropdown, as ONLYOFFICE's Forms tab keeps them beside the fields.
//
// Everything that is a decision rather than a widget is in
// `content_control_model.mjs`, which a unit test drives in node.
import { t, activeLocale } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";
import { onButton, registerPopover } from "./popover_manager.mjs";
import {
  CONTROL_KINDS,
  DATE_FORMATS,
  clickOpensChooser,
  dateInputFromIso,
  fillModeOf,
  formatWordDate,
  isoFromDateInput,
  propertiesPatch,
} from "./content_control_model.mjs";

/** EMU per CSS pixel at 96 dpi. */
const EMU_PER_PX = 9525;
/** The widest a filled picture is placed: six inches, Word's body width. */
const MAX_PICTURE_EMU = 6 * 914400;
/** A new picture control's placeholder, 1.5 inches square. */
const PLACEHOLDER_EMU = 1371600;

/** The labels, read when a surface builds (after the catalogue lands). */
function kindLabel(row) {
  switch (row.kind) {
    case "richText":
      return t("controls.kind.richText");
    case "plainText":
      return t("controls.kind.plainText");
    case "picture":
      return t("controls.kind.picture");
    case "checkbox":
      return t("controls.kind.checkbox");
    case "comboBox":
      return t("controls.kind.comboBox");
    case "dropDownList":
      return t("controls.kind.dropDownList");
    default:
      return t("controls.kind.date");
  }
}

/** The prompt an empty control of `kind` is inserted holding. */
function placeholderFor(kind) {
  if (kind === "comboBox" || kind === "dropDownList") return t("controls.placeholder.choose");
  if (kind === "date") return t("controls.placeholder.date");
  if (kind === "richText" || kind === "plainText") return t("controls.placeholder.text");
  return "";
}

/** A light placeholder picture, drawn once: Word's picture control shows a
 *  placeholder until a picture is chosen. */
async function placeholderPicture() {
  const canvas = document.createElement("canvas");
  canvas.width = 144;
  canvas.height = 144;
  const g = canvas.getContext("2d");
  if (g) {
    g.fillStyle = "#eef0f4";
    g.fillRect(0, 0, 144, 144);
    g.strokeStyle = "#9aa3b2";
    g.lineWidth = 4;
    g.strokeRect(2, 2, 140, 140);
    g.beginPath();
    g.moveTo(24, 112);
    g.lineTo(60, 64);
    g.lineTo(84, 92);
    g.lineTo(100, 76);
    g.lineTo(120, 112);
    g.closePath();
    g.fillStyle = "#9aa3b2";
    g.fill();
    g.beginPath();
    g.arc(100, 44, 12, 0, Math.PI * 2);
    g.fill();
  }
  const blob = await new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
  return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
}

/** A chosen image file's bytes and size in EMU, scaled to fit the body. */
async function pictureFromFile(file) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let width = 1;
  let height = 1;
  try {
    const bitmap = await createImageBitmap(file);
    width = bitmap.width || 1;
    height = bitmap.height || 1;
    bitmap.close?.();
  } catch {
    // An image the browser cannot decode is still the engine's to refuse.
  }
  let w = width * EMU_PER_PX;
  let h = height * EMU_PER_PX;
  if (w > MAX_PICTURE_EMU) {
    h = Math.round((h * MAX_PICTURE_EMU) / w);
    w = MAX_PICTURE_EMU;
  }
  return { bytes, width: w, height: h, mime: file.type || "image/png" };
}

/**
 * Wires the content-control surfaces.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc
 * @param {() => {anchor: {node: string, offset: number}, focus: {node: string, offset: number}}|null} io.getSelection
 * @param {(node: string, start: number, end?: number) => void} io.select puts the selection there and shows it
 * @param {(thunk: () => unknown, options?: object) => Promise<boolean>} io.runEdit
 * @param {() => unknown} io.revision the document revision, which keys the caret cache
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {() => Array<{name: string, color: string}>} io.roles the form's roles
 * @param {() => void} io.protectForm toggles forms protection
 * @param {() => void} io.openRoles opens the roles dialog
 * @param {() => Element|null} io.fallbackFocus
 */
export function createContentControls(io) {
  const el = (id) => document.getElementById(id);
  const button = el("insertControlsBtn");
  const menu = el("insertControlsMenu");

  /** The control at the caret, cached per document revision and caret. */
  let cache = { key: "", control: null };
  function atCaret() {
    const doc = io.getDoc();
    const selection = io.getSelection();
    if (!doc || !selection || typeof doc.contentControlAt !== "function") return null;
    const { node, offset } = selection.focus;
    const key = `${io.revision()}:${node}:${offset}`;
    if (cache.key === key) return cache.control;
    let control = null;
    try {
      const json = doc.contentControlAt(node, offset);
      control = json ? JSON.parse(json) : null;
    } catch {
      control = null;
    }
    cache = { key, control };
    return control;
  }
  const forget = () => {
    cache = { key: "", control: null };
  };

  /** The one insert path: every surface runs this. */
  async function insert(kind) {
    const doc = io.getDoc();
    const selection = io.getSelection();
    if (!doc || !selection) {
      io.setStatus(t("command.needsDocument"), "error");
      return false;
    }
    const { anchor, focus } = selection;
    if (anchor.node !== focus.node) {
      io.setStatus(t("controls.oneParagraph"), "error");
      return false;
    }
    const start = Math.min(anchor.offset, focus.offset);
    const end = Math.max(anchor.offset, focus.offset);
    let picture = null;
    if (kind === "picture") {
      picture = await placeholderPicture();
      if (!picture) {
        io.setStatus(t("controls.pictureFailed"), "error");
        return false;
      }
    }
    const landed = await io.runEdit(
      () =>
        kind === "picture"
          ? doc.insertPictureContentControl(anchor.node, end, picture, PLACEHOLDER_EMU, PLACEHOLDER_EMU, "image/png")
          : doc.insertContentControl(anchor.node, start, end, kind, placeholderFor(kind)),
      { gate: true },
    );
    forget();
    if (landed) io.setStatus(t("controls.inserted"));
    return landed;
  }

  // ---- Properties ---------------------------------------------------------------
  const props = {
    dialog: el("controlPropsDialog"),
    title: el("controlPropsTitle"),
    tag: el("controlPropsTag"),
    useColor: el("controlPropsUseColor"),
    color: el("controlPropsColor"),
    placeholderRow: el("controlPropsPlaceholderRow"),
    placeholder: el("controlPropsPlaceholder"),
    role: el("controlPropsRole"),
    temporary: el("controlPropsTemporary"),
    lockControl: el("controlPropsLockControl"),
    lockContent: el("controlPropsLockContent"),
    listRow: el("controlPropsListRow"),
    list: el("controlPropsList"),
    addItem: el("controlPropsAddItem"),
    dateRow: el("controlPropsDateRow"),
    dateFormat: el("controlPropsDateFormat"),
    datePreview: el("controlPropsDatePreview"),
    ok: el("controlPropsOk"),
    cancel: el("controlPropsCancel"),
    close: el("controlPropsClose"),
    kindLabel: el("controlPropsKind"),
  };
  let editing = null;
  const propsModal = props.dialog
    ? registerModal(props.dialog, {
      initialFocus: () => props.title,
      fallbackFocus: io.fallbackFocus,
      defaultAction: () => void applyProperties(),
    })
    : null;
  const formats = el("controlPropsDateFormats");
  if (formats && !formats.children.length) {
    for (const format of DATE_FORMATS) {
      const option = document.createElement("option");
      option.value = format;
      formats.append(option);
    }
  }

  function itemRow(item = { display: "", value: "" }) {
    const row = document.createElement("li");
    row.className = "control-item-row";
    const display = document.createElement("input");
    display.type = "text";
    display.maxLength = 255;
    display.value = item.display ?? "";
    display.className = "control-item-display";
    display.setAttribute("aria-label", t("controls.props.itemDisplay"));
    const value = document.createElement("input");
    value.type = "text";
    value.maxLength = 255;
    value.value = item.value ?? "";
    value.className = "control-item-value";
    value.setAttribute("aria-label", t("controls.props.itemValue"));
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "dialog-button control-item-remove";
    remove.textContent = t("controls.props.removeItem");
    remove.setAttribute("aria-label", t("controls.props.removeItem"));
    remove.addEventListener("click", () => {
      const next = row.nextElementSibling?.querySelector("input") ?? props.addItem;
      row.remove();
      next?.focus();
    });
    row.append(display, value, remove);
    return row;
  }

  function reflectDatePreview() {
    if (!props.datePreview || !props.dateFormat) return;
    props.datePreview.textContent = t("controls.props.datePreview", {
      example: formatWordDate(new Date(), props.dateFormat.value || "M/d/yyyy", activeLocale()),
    });
  }

  function openProperties() {
    const control = atCaret();
    if (!control) {
      io.setStatus(t("controls.notInControl"), "error");
      return;
    }
    if (!propsModal) return;
    editing = control;
    if (props.kindLabel) props.kindLabel.textContent = kindLabel({ kind: control.kind }) ?? control.kind;
    props.title.value = control.title ?? "";
    props.tag.value = control.tag ?? "";
    props.useColor.checked = !!control.color && control.color !== "auto";
    props.color.value = props.useColor.checked ? `#${control.color.toLowerCase()}` : "#3355c4";
    props.color.disabled = !props.useColor.checked;
    const mode = fillModeOf(control);
    props.placeholderRow.hidden = !(mode === "text" || mode === "list" || mode === "combo" || mode === "date");
    props.placeholder.value = control.showingPlaceholder ? control.text : "";
    props.placeholder.disabled = !control.showingPlaceholder;
    props.temporary.checked = !!control.temporary;
    props.lockControl.checked = control.lock === "sdtLocked" || control.lock === "sdtContentLocked";
    props.lockContent.checked = control.lock === "contentLocked" || control.lock === "sdtContentLocked";
    props.role.replaceChildren();
    const anyone = document.createElement("option");
    anyone.value = "";
    anyone.textContent = t("formRoles.anyone");
    props.role.append(anyone);
    for (const role of io.roles()) {
      const option = document.createElement("option");
      option.value = role.name;
      option.textContent = role.name;
      props.role.append(option);
    }
    props.role.value = control.role ?? "";
    props.listRow.hidden = !(control.kind === "dropDownList" || control.kind === "comboBox");
    props.list.replaceChildren(...(control.items ?? []).map((item) => itemRow(item)));
    props.dateRow.hidden = control.kind !== "date";
    props.dateFormat.value = control.dateFormat ?? "M/d/yyyy";
    reflectDatePreview();
    propsModal.open();
  }

  async function applyProperties() {
    const control = editing;
    const doc = io.getDoc();
    if (!control || !doc) return;
    const items = [...props.list.querySelectorAll(".control-item-row")].map((row) => ({
      display: row.querySelector(".control-item-display")?.value ?? "",
      value: row.querySelector(".control-item-value")?.value ?? "",
    }));
    const patch = propertiesPatch(control, {
      title: props.title.value,
      tag: props.tag.value,
      useColor: props.useColor.checked,
      color: props.color.value,
      placeholder: props.placeholder.disabled ? "" : props.placeholder.value,
      temporary: props.temporary.checked,
      lockControl: props.lockControl.checked,
      lockContent: props.lockContent.checked,
      role: props.role.value,
      items,
      dateFormat: props.dateFormat.value,
    });
    const landed = await io.runEdit(() => doc.setContentControlProperties(control.id, JSON.stringify(patch)), { gate: true });
    forget();
    if (landed) {
      propsModal.close("apply");
      io.setStatus(t("controls.propertiesApplied"));
    }
  }

  props.ok?.addEventListener("click", () => void applyProperties());
  props.cancel?.addEventListener("click", () => propsModal?.close("cancel"));
  props.close?.addEventListener("click", () => propsModal?.close("cancel"));
  props.useColor?.addEventListener("change", () => {
    props.color.disabled = !props.useColor.checked;
  });
  props.addItem?.addEventListener("click", () => {
    const row = itemRow();
    props.list.append(row);
    row.querySelector("input")?.focus();
  });
  props.dateFormat?.addEventListener("input", reflectDatePreview);

  async function remove() {
    const control = atCaret();
    const doc = io.getDoc();
    if (!control || !doc) {
      io.setStatus(t("controls.notInControl"), "error");
      return false;
    }
    const landed = await io.runEdit(() => doc.removeContentControl(control.id), { gate: true });
    forget();
    if (landed) io.setStatus(t("controls.removed"));
    return landed;
  }

  // ---- Filling --------------------------------------------------------------------
  const fill = {
    dialog: el("controlFillDialog"),
    heading: el("controlFillTitle"),
    list: el("controlFillList"),
    textRow: el("controlFillTextRow"),
    text: el("controlFillText"),
    dateRow: el("controlFillDateRow"),
    date: el("controlFillDate"),
    ok: el("controlFillOk"),
    cancel: el("controlFillCancel"),
  };
  let filling = null;
  const fillModal = fill.dialog
    ? registerModal(fill.dialog, {
      initialFocus: () =>
        fill.list.querySelector('[aria-selected="true"]') ??
        fill.list.querySelector("[role=option]") ??
        (fill.dateRow.hidden ? fill.text : fill.date),
      fallbackFocus: io.fallbackFocus,
      defaultAction: () => void applyFill(),
    })
    : null;
  const filePicker = document.createElement("input");
  filePicker.type = "file";
  filePicker.accept = "image/png,image/jpeg,image/gif,image/bmp,image/webp";
  filePicker.hidden = true;
  document.body.append(filePicker);
  filePicker.addEventListener("change", async () => {
    const file = filePicker.files?.[0];
    const control = filling;
    filePicker.value = "";
    const doc = io.getDoc();
    if (!file || !control || !doc) return;
    const picture = await pictureFromFile(file);
    const landed = await io.runEdit(
      () => doc.fillPictureContentControl(control.id, picture.bytes, picture.width, picture.height, picture.mime),
    );
    forget();
    if (landed) io.setStatus(t("controls.filled"));
  });

  function chooseOption(option) {
    for (const node of fill.list.querySelectorAll("[role=option]")) {
      const on = node === option;
      node.setAttribute("aria-selected", String(on));
      node.tabIndex = on ? 0 : -1;
    }
    if (filling?.kind === "comboBox" && option) fill.text.value = option.dataset.display ?? "";
  }

  fill.list?.addEventListener("keydown", (event) => {
    const options = [...fill.list.querySelectorAll("[role=option]")];
    const index = options.indexOf(document.activeElement);
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (!step || !options.length) return;
    event.preventDefault();
    const next = options[Math.min(options.length - 1, Math.max(0, index + step))];
    chooseOption(next);
    next.focus();
  });

  /** Opens the chooser for `control` — the list, the date, or the file. */
  function openFillFor(control) {
    const mode = fillModeOf(control);
    if (!control.fillable) {
      io.setStatus(t("controls.notFillable"), "error");
      return true;
    }
    if (mode === "picture") {
      filling = control;
      filePicker.click();
      return true;
    }
    if (mode !== "list" && mode !== "combo" && mode !== "date") return false;
    if (!fillModal) return false;
    const entries = (control.items ?? []).filter((item) => item.value || item.display !== control.text || !control.showingPlaceholder);
    if (mode === "list" && !entries.length) {
      io.setStatus(t("controls.noEntries"), "error");
      return true;
    }
    filling = control;
    fill.heading.textContent = control.title || kindLabel({ kind: control.kind });
    fill.list.hidden = mode === "date";
    fill.textRow.hidden = mode !== "combo";
    fill.dateRow.hidden = mode !== "date";
    fill.list.replaceChildren();
    // Word's default entry is the prompt itself, with an empty value; it is what
    // the control shows before anything is chosen, not a choice.
    for (const item of entries) {
      const option = document.createElement("li");
      option.setAttribute("role", "option");
      option.className = "control-fill-option";
      option.textContent = item.display;
      option.dataset.display = item.display;
      option.tabIndex = -1;
      option.setAttribute("aria-selected", String(!control.showingPlaceholder && item.display === control.text));
      option.addEventListener("click", () => {
        chooseOption(option);
        if (mode === "list") void applyFill();
      });
      fill.list.append(option);
    }
    const chosen = fill.list.querySelector('[aria-selected="true"]') ?? fill.list.querySelector("[role=option]");
    if (chosen) chosen.tabIndex = 0;
    fill.text.value = control.showingPlaceholder ? "" : control.text;
    fill.date.value = dateInputFromIso(control.fullDate);
    fillModal.open();
    return true;
  }

  async function applyFill() {
    const control = filling;
    const doc = io.getDoc();
    if (!control || !doc) return;
    const mode = fillModeOf(control);
    let value = null;
    if (mode === "date") {
      const iso = isoFromDateInput(fill.date.value);
      if (!iso) {
        io.setStatus(t("controls.chooseDate"), "error");
        return;
      }
      value = { text: formatWordDate(new Date(iso), control.dateFormat || "M/d/yyyy", activeLocale()), fullDate: iso };
    } else {
      const option = fill.list.querySelector('[aria-selected="true"]');
      const text = mode === "combo" ? fill.text.value.trim() : option?.dataset.display ?? "";
      if (!text) {
        io.setStatus(t("controls.chooseItem"), "error");
        return;
      }
      value = { text };
    }
    const landed = await io.runEdit(() => doc.fillContentControl(control.id, JSON.stringify(value)));
    forget();
    if (landed) fillModal.close("apply");
  }

  fill.ok?.addEventListener("click", () => void applyFill());
  fill.cancel?.addEventListener("click", () => fillModal?.close("cancel"));

  /** Alt+Down and the menu's "Choose a value": the chooser for the control at
   *  the caret. Returns whether there was one to open. */
  function openFill() {
    const control = atCaret();
    if (!control) {
      io.setStatus(t("controls.notInControl"), "error");
      return false;
    }
    if (openFillFor(control)) return true;
    io.setStatus(t("controls.typeToFill"));
    return false;
  }

  /** A press inside a list, date or picture control opens its chooser, as in
   *  Word, after the caret has moved there. Returns whether it did. */
  function pressAt(anchor) {
    const doc = io.getDoc();
    if (!doc || !anchor || typeof doc.contentControlAt !== "function") return false;
    let control = null;
    try {
      const json = doc.contentControlAt(anchor.node, anchor.offset);
      control = json ? JSON.parse(json) : null;
    } catch {
      control = null;
    }
    if (!control || control.scope !== "inline" || !clickOpensChooser(control)) return false;
    io.select(anchor.node, anchor.offset);
    return openFillFor(control);
  }

  /**
   * Typing over a control that is still showing its placeholder replaces the
   * placeholder, as in Word: the first keystroke becomes the content and the
   * placeholder flag is cleared, as one undoable step. Returns whether it took
   * the keystroke.
   */
  async function typeIntoPlaceholder(text) {
    const control = atCaret();
    const doc = io.getDoc();
    if (!control || !doc || !control.showingPlaceholder || control.scope !== "inline") return false;
    const mode = fillModeOf(control);
    if (mode !== "text" && mode !== "combo" && mode !== "date") return false;
    if (!control.fillable) {
      io.setStatus(t("controls.notFillable"), "error");
      return true;
    }
    const landed = await io.runEdit(() => doc.fillContentControl(control.id, JSON.stringify({ text })));
    forget();
    if (landed) io.select(control.paragraph, control.start + new TextEncoder().encode(text).length);
    return true;
  }

  // ---- The Insert band's dropdown -----------------------------------------------
  const rowState = () => {
    const control = atCaret();
    for (const row of menu?.querySelectorAll("[data-command]") ?? []) {
      const id = row.dataset.command;
      const needsControl = id === "controls.properties" || id === "controls.remove" || id === "controls.fill";
      row.disabled = !io.getDoc() || (needsControl && !control);
      row.title = row.disabled ? (io.getDoc() ? t("controls.notInControl") : t("command.needsDocument")) : "";
    }
  };
  if (button && menu) {
    const popover = registerPopover(button, menu, rowState);
    const actions = new Map([
      ...CONTROL_KINDS.map((row) => [row.command, () => void insert(row.kind)]),
      ["controls.properties", openProperties],
      ["controls.remove", () => void remove()],
      ["controls.fill", openFill],
      ["review.protectForm", () => io.protectForm()],
      ["forms.roles", () => io.openRoles()],
    ]);
    for (const row of menu.querySelectorAll("[data-command]")) {
      const action = actions.get(row.dataset.command);
      if (!action) continue;
      onButton(row, () => {
        popover.close();
        // A dialog opened from here records the band button as its opener, so
        // Escape comes back to it (the dialog contract); the caret is kept.
        button.focus({ preventScroll: true });
        action();
      });
    }
  }

  return { atCaret, forget, insert, openProperties, remove, openFill, pressAt, typeIntoPlaceholder, kindLabel };
}

/**
 * The content-control rows for `LAYOUT_SURFACE`: one per kind, and the three
 * that act on the control at the caret. `label` is a getter, read after the
 * catalogue lands (`break_commands.mjs` says why). The band button's click is
 * the dropdown's own, so every row is `ownsClick`.
 *
 * @param {() => ReturnType<typeof createContentControls>} controls
 */
export function contentControlSurfaceRows(controls) {
  const band = () => [document.getElementById("insertControlsBtn")].filter(Boolean);
  return [
    ...CONTROL_KINDS.map((row) => ({
      command: row.command,
      group: "Insert",
      get label() {
        return t("controls.insertCommand", { kind: kindLabel(row) });
      },
      kw: row.kw,
      buttons: band,
      requires: "caret",
      ownsClick: true,
      run: () => void controls().insert(row.kind),
    })),
    { command: "controls.properties", group: "Insert", get label() { return t("controls.propertiesCommand"); }, kw: "content control properties title tag lock placeholder list entries date format settings", buttons: () => [], requires: "control", run: () => controls().openProperties() },
    { command: "controls.remove", group: "Insert", get label() { return t("controls.removeCommand"); }, kw: "remove content control unwrap keep contents delete", buttons: () => [], requires: "control", run: () => void controls().remove() },
    { command: "controls.fill", group: "Insert", get label() { return t("controls.fillCommand"); }, kw: "choose value drop down list date picker picture fill form field select item", buttons: () => [], requires: "control", run: () => controls().openFill() },
  ];
}
