// Shape Format ▸ Shape Effects ▸ Shadow (`docs/153` object.shape-shadow).
//
// Competitive standard first. Word's Shadow gallery leads with "No Shadow",
// then the nine OUTER presets — the eight offsets around the shape and
// "Offset: Center" — and ends with "Shadow Options…", a pane with colour,
// transparency, size, blur, angle and distance. Word's offset presets are
// all black at 60% transparency, 3 pt away and blurred 4 pt, differing only
// in angle; "Offset: Center" has no distance and a 5 pt blur. ONLYOFFICE's
// shape settings carry a shadow toggle (`ShapeSettings.js`, `btnShadowShape`)
// with the same preset-then-options shape. Google Docs drawings offer a drop
// shadow with colour, transparency, angle, distance and blur.
//
// Deliberately not here: Word's INNER and PERSPECTIVE shadows, and its Size
// field. The model carries an outer shadow (`a:outerShdw`); a perspective
// shadow's skew and scale are kept on import and written back, but not
// painted, so offering them would offer something the page cannot show.
//
// The rules (presets, the menu rows, reading the engine's answer, building a
// request) are pure and tested in node (`tests/shape_shadow.test.mjs`). The
// options dialog adopts `#shapeShadowDialog` in `editor.html` by id.

/** Word's outer presets, in its gallery order, as engine requests. */
export const SHADOW_PRESETS = Object.freeze([
  Object.freeze({ id: "bottomRight", key: "shape.shadow.bottomRight", angle: 45, distancePt: 3, blurPt: 4 }),
  Object.freeze({ id: "bottom", key: "shape.shadow.bottom", angle: 90, distancePt: 3, blurPt: 4 }),
  Object.freeze({ id: "bottomLeft", key: "shape.shadow.bottomLeft", angle: 135, distancePt: 3, blurPt: 4 }),
  Object.freeze({ id: "right", key: "shape.shadow.right", angle: 0, distancePt: 3, blurPt: 4 }),
  Object.freeze({ id: "center", key: "shape.shadow.center", angle: 0, distancePt: 0, blurPt: 5 }),
  Object.freeze({ id: "left", key: "shape.shadow.left", angle: 180, distancePt: 3, blurPt: 4 }),
  Object.freeze({ id: "topRight", key: "shape.shadow.topRight", angle: 315, distancePt: 3, blurPt: 4 }),
  Object.freeze({ id: "top", key: "shape.shadow.top", angle: 270, distancePt: 3, blurPt: 4 }),
  Object.freeze({ id: "topLeft", key: "shape.shadow.topLeft", angle: 225, distancePt: 3, blurPt: 4 }),
]);

/** Word's preset colour and transparency, shared by every preset. */
export const PRESET_COLOR = "#000000";
export const PRESET_TRANSPARENCY = 0.6;

/** The engine request for one preset. Colour and transparency are Word's, so a
 *  preset chosen over a recoloured shadow resets it, as Word's gallery does. */
export function presetRequest(preset) {
  return {
    color: PRESET_COLOR,
    transparency: PRESET_TRANSPARENCY,
    angle: preset.angle,
    distancePt: preset.distancePt,
    blurPt: preset.blurPt,
  };
}

/**
 * The shadow `shapeShadow` answered: `undefined` when the node cannot carry
 * one (a picture, a chart, nothing selected), `null` when it has none, else
 * `{color, transparency, angle, distancePt, blurPt}`.
 *
 * @param {string} json
 */
export function readShadow(json) {
  if (!json) return undefined;
  try {
    const parsed = JSON.parse(json);
    return parsed && "shadow" in parsed ? parsed.shadow : undefined;
  } catch {
    return undefined;
  }
}

/** The preset a shadow IS, so the gallery can check it, or `null`. O(9). */
export function activePreset(shadow) {
  if (!shadow) return null;
  const near = (a, b) => Math.abs(Number(a) - Number(b)) < 0.01;
  return (
    SHADOW_PRESETS.find(
      (preset) =>
        near(shadow.angle, preset.angle) &&
        near(shadow.distancePt, preset.distancePt) &&
        near(shadow.blurPt, preset.blurPt) &&
        String(shadow.color).toLowerCase() === PRESET_COLOR &&
        near(shadow.transparency, PRESET_TRANSPARENCY),
    ) ?? null
  );
}

/**
 * The Shadow submenu for a selected shape or text box: No shadow, the nine
 * presets (the current one checked), and Shadow options….
 *
 * @param {object} io
 * @param {ReturnType<typeof readShadow>} io.shadow   read once per menu build
 * @param {boolean} io.mutationEnabled
 * @param {string} io.mutationReason
 * @param {(key: string) => string} io.t
 * @param {(request: object|null) => void} io.apply  `null` removes the shadow
 * @param {() => void} io.openOptions
 */
export function shadowCommands(io) {
  const gate = io.mutationEnabled ? { enabled: true } : { enabled: false, disabledReason: io.mutationReason };
  const active = activePreset(io.shadow);
  return {
    id: "object.shadow",
    label: io.t("shape.shadow"),
    group: "arrange",
    icon: "format",
    submenu: [
      {
        id: "object.shadow.none",
        label: io.t("shape.shadow.none"),
        group: "reset",
        shortcut: io.shadow === null ? "✓" : "",
        ...gate,
        run: () => io.apply(null),
      },
      ...SHADOW_PRESETS.map((preset) => ({
        id: `object.shadow.${preset.id}`,
        label: io.t(preset.key),
        group: "outer",
        shortcut: active === preset ? "✓" : "",
        ...gate,
        run: () => io.apply(presetRequest(preset)),
      })),
      {
        id: "object.shadow.options",
        label: io.t("shape.shadow.options"),
        group: "options",
        ...gate,
        run: () => io.openOptions(),
      },
    ],
  };
}

/** The sentence each refused options field says, by field. */
export const BAD_FIELD_KEYS = Object.freeze({
  color: "shape.shadow.bad.color",
  transparency: "shape.shadow.bad.transparency",
  angle: "shape.shadow.bad.angle",
  distance: "shape.shadow.bad.distance",
  blur: "shape.shadow.bad.blur",
});

/**
 * The options form read back into an engine request, or `{error}` naming the
 * first field out of Word's range. Transparency is a percentage in the form
 * and a fraction in the request.
 *
 * @param {{color: string, transparency: string|number, angle: string|number,
 *          distance: string|number, blur: string|number}} form
 */
export function optionsRequest(form) {
  const number = (value) => (value === "" || value === null || value === undefined ? NaN : Number(value));
  const transparency = number(form.transparency);
  const angle = number(form.angle);
  const distance = number(form.distance);
  const blur = number(form.blur);
  if (!/^#[0-9a-f]{6}$/i.test(String(form.color ?? ""))) return { error: "color" };
  if (!(transparency >= 0 && transparency <= 100)) return { error: "transparency" };
  if (!Number.isFinite(angle)) return { error: "angle" };
  if (!(distance >= 0 && distance <= 200)) return { error: "distance" };
  if (!(blur >= 0 && blur <= 100)) return { error: "blur" };
  return {
    color: String(form.color).toLowerCase(),
    transparency: Math.round(transparency) / 100,
    angle: ((angle % 360) + 360) % 360,
    distancePt: distance,
    blurPt: blur,
  };
}

/**
 * Adopts `#shapeShadowDialog` and returns `{open}`.
 *
 * @param {object} io
 * @param {(element: Element, options: object) => object} io.registerModal
 * @param {() => Element} io.fallbackFocus
 * @param {() => string|null} io.node
 * @param {(node: string) => string} io.read     `shapeShadow(node)`
 * @param {(node: string, request: object) => Promise<boolean>} io.apply
 * @param {(key: string) => string} io.t
 */
export function createShadowOptionsDialog(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("shapeShadowDialog");
  if (!dialog) return { open() {} };
  const form = el("shapeShadowForm");
  const fields = {
    color: el("shapeShadowColor"),
    transparency: el("shapeShadowTransparency"),
    angle: el("shapeShadowAngle"),
    distance: el("shapeShadowDistance"),
    blur: el("shapeShadowBlur"),
  };
  const note = el("shapeShadowNote");
  let node = null;

  const modal = io.registerModal(dialog, {
    initialFocus: () => fields.color,
    fallbackFocus: io.fallbackFocus,
  });

  async function commit(event) {
    event?.preventDefault();
    if (!node) return;
    const request = optionsRequest({
      color: fields.color.value,
      transparency: fields.transparency.value,
      angle: fields.angle.value,
      distance: fields.distance.value,
      blur: fields.blur.value,
    });
    if (request.error) {
      note.textContent = io.t(BAD_FIELD_KEYS[request.error]);
      fields[request.error]?.focus();
      return;
    }
    if (!(await io.apply(node, request))) return;
    modal.close();
  }

  form?.addEventListener("submit", commit);
  el("shapeShadowCancel")?.addEventListener("click", () => modal.close());
  el("shapeShadowClose")?.addEventListener("click", () => modal.close());

  return {
    /** Opens on the selected shape, filled from its shadow or Word's preset. */
    open() {
      node = io.node();
      if (!node) return;
      const current = readShadow(io.read(node));
      if (current === undefined) return;
      const start = current ?? { color: PRESET_COLOR, transparency: PRESET_TRANSPARENCY, ...SHADOW_PRESETS[0] };
      fields.color.value = start.color;
      fields.transparency.value = String(Math.round(start.transparency * 100));
      fields.angle.value = String(Math.round(start.angle));
      fields.distance.value = String(start.distancePt);
      fields.blur.value = String(start.blurPt);
      note.textContent = "";
      modal.open();
    },
  };
}
