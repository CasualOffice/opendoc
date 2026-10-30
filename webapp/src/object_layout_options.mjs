// Layout ▸ More layout options (`docs/153` object.advanced-layout-options).
//
// Competitive standard first. Word's Layout dialog has, on its Text Wrapping
// tab, "Distance from text" (top, bottom, left, right) and, on its Position
// tab, "Move object with text" and "Allow overlap". ONLYOFFICE's picture and
// shape advanced settings carry the same: the four distances on the wrapping
// tab and two switches on the position tab (`ImageSettingsAdvanced.js`,
// `textMove` / `textOverlap`). Google Docs has "Margin from text" under Image
// options. Word's dialog opens from Layout Options ▸ "See more…" and from the
// picture's right-click menu; here it opens from the object's right-click menu
// and the command palette.
//
// The engine side is `setObjectPosition`, which already took the distances
// and now takes `allowOverlap` and `moveWithText` too. "Move object with text"
// converts the vertical offset so the object stays where it is (Word's
// checkbox changes what the object follows, not where it sits); the engine
// does that conversion because only layout knows where the paragraph starts.
//
// The RULES are pure — reading the engine's position answer into the form,
// and the form back into the smallest payload that says what changed — so they
// are tested in node (`tests/object_layout_options.test.mjs`). The dialog is
// the markup in `editor.html` (`#objectLayoutDialog`), adopted by id.

/** EMU per point. */
export const EMU_PER_PT = 12_700;

/** Word's largest distance from text, 22 inches, in points. */
export const MAX_DISTANCE_PT = 1584;

/**
 * What the dialog shows, from an `objectPosition` answer.
 *
 * @param {string} json the raw `objectPosition(node)` answer
 * @returns {{available: boolean, reasonKey?: string, top?: number, bottom?: number,
 *            left?: number, right?: number, moveWithText?: boolean,
 *            allowOverlap?: boolean}}
 */
export function readLayoutOptions(json) {
  let read;
  try {
    read = JSON.parse(json || "null");
  } catch {
    read = null;
  }
  if (!read) return { available: false, reasonKey: "object.wrap.notAnObject" };
  if (read.groupChild) return { available: false, reasonKey: "object.wrap.groupChild" };
  if (!read.floating) return { available: false, reasonKey: "object.layoutOptions.inline" };
  const distances = read.wrapDistances ?? {};
  const pt = (emu) => Math.round((Number(emu) || 0) / EMU_PER_PT * 100) / 100;
  return {
    available: true,
    top: pt(distances.topEmu),
    bottom: pt(distances.bottomEmu),
    // `start`/`end` are the schema's names for `distL`/`distR`, which are
    // physical left and right whatever the text direction.
    left: pt(distances.startEmu),
    right: pt(distances.endEmu),
    moveWithText: read.moveWithText !== false,
    allowOverlap: read.allowOverlap !== false,
  };
}

/**
 * The `setObjectPosition` payload for what the reader changed, or `null` when
 * nothing changed (Apply on an untouched dialog is not an undo entry), or an
 * `{error}` naming the field that is out of range.
 *
 * Only the changed members are sent: an omitted field is left alone by the
 * engine, so sending the four distances when only "Allow overlap" changed would
 * restate them for nothing.
 *
 * @param {ReturnType<typeof readLayoutOptions>} before
 * @param {{top: string|number, bottom: string|number, left: string|number,
 *          right: string|number, moveWithText: boolean, allowOverlap: boolean}} form
 */
export function layoutOptionsPayload(before, form) {
  const out = {};
  const sides = ["top", "bottom", "left", "right"];
  const values = {};
  for (const side of sides) {
    const value = Number(form[side]);
    if (!Number.isFinite(value) || value < 0 || value > MAX_DISTANCE_PT) {
      return { error: side };
    }
    values[side] = value;
  }
  if (sides.some((side) => Math.abs(values[side] - (before[side] ?? 0)) > 1e-9)) {
    const emu = (points) => Math.round(points * EMU_PER_PT);
    out.wrapDistances = {
      topEmu: emu(values.top),
      bottomEmu: emu(values.bottom),
      startEmu: emu(values.left),
      endEmu: emu(values.right),
    };
  }
  if (!!form.moveWithText !== !!before.moveWithText) out.moveWithText = !!form.moveWithText;
  if (!!form.allowOverlap !== !!before.allowOverlap) out.allowOverlap = !!form.allowOverlap;
  return Object.keys(out).length ? out : null;
}

/**
 * The palette / right-click row that opens the dialog.
 *
 * @param {{available: boolean, reasonKey?: string}} availability from
 *        `readLayoutOptions`, read once per menu build
 * @param {{mutationEnabled: boolean, mutationReason: string,
 *          t: (key: string) => string, open: () => void}} io
 */
export function layoutOptionsCommand(availability, io) {
  const enabled = io.mutationEnabled && availability.available;
  const reason = !io.mutationEnabled
    ? io.mutationReason
    : availability.reasonKey
      ? io.t(availability.reasonKey)
      : "";
  return {
    id: "object.layoutOptions",
    label: io.t("object.layoutOptions"),
    group: "arrange",
    icon: "tune",
    enabled,
    disabledReason: enabled ? "" : reason,
    run: () => io.open(),
  };
}

/**
 * Adopts `#objectLayoutDialog` and returns `{open}`.
 *
 * @param {object} io
 * @param {(element: Element, options: object) => object} io.registerModal
 * @param {() => Element} io.fallbackFocus
 * @param {() => string|null} io.node        the selected object's node
 * @param {(node: string) => string} io.read  `objectPosition(node)`
 * @param {(node: string, payload: object) => Promise<boolean>} io.apply one gated edit
 * @param {(key: string) => string} io.t
 */
export function createLayoutOptionsDialog(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("objectLayoutDialog");
  if (!dialog) return { open() {} };
  const form = el("objectLayoutForm");
  const fields = {
    top: el("objectLayoutTop"),
    bottom: el("objectLayoutBottom"),
    left: el("objectLayoutLeft"),
    right: el("objectLayoutRight"),
  };
  const move = el("objectLayoutMove");
  const overlap = el("objectLayoutOverlap");
  const note = el("objectLayoutNote");
  let before = null;
  let node = null;

  const modal = io.registerModal(dialog, {
    initialFocus: () => fields.top,
    fallbackFocus: io.fallbackFocus,
  });

  async function commit(event) {
    event?.preventDefault();
    if (!before || !node) return;
    const payload = layoutOptionsPayload(before, {
      top: fields.top.value,
      bottom: fields.bottom.value,
      left: fields.left.value,
      right: fields.right.value,
      moveWithText: move.checked,
      allowOverlap: overlap.checked,
    });
    if (payload?.error) {
      note.textContent = io.t("object.layoutOptions.badDistance");
      fields[payload.error]?.focus();
      return;
    }
    if (payload && !(await io.apply(node, payload))) return;
    modal.close();
  }

  form?.addEventListener("submit", commit);
  el("objectLayoutCancel")?.addEventListener("click", () => modal.close());
  el("objectLayoutClose")?.addEventListener("click", () => modal.close());

  return {
    /** Opens the dialog on the selected object. O(document): one position read. */
    open() {
      node = io.node();
      if (!node) return;
      before = readLayoutOptions(io.read(node));
      if (!before.available) return;
      for (const side of Object.keys(fields)) fields[side].value = String(before[side]);
      move.checked = before.moveWithText;
      overlap.checked = before.allowOverlap;
      note.textContent = "";
      modal.open();
    },
  };
}
