// Shape Outline ▸ Dashes and ▸ Arrows (`docs/109` RM-30).
//
// Competitive standard first. Word's Shape Outline menu has Weight, Dashes
// (eight patterns, Solid first) and Arrows (a gallery of start/end
// combinations); a picture's border has Weight and Dashes. ONLYOFFICE carries
// the dash as the shape panel's line-type picker
// (`web-apps/apps/documenteditor/main/app/view/ShapeSettings.js:1635`,
// `cmbBorderType` writing `asc_putPrstDash`) and the arrows as begin / end
// style pickers in the advanced settings (`ImageSettingsAdvanced.js:1037`,
// `btnBeginStyle`, and `:1079`, `btnEndStyle`).
// The names below are Word's, and each maps to the DrawingML token the engine
// stores (`a:prstDash`, `a:headEnd` / `a:tailEnd`) through
// `setShapeOutlineStyle`, which keeps the colour and the weight.
//
// Deliberately: Arrows is offered for every shape, not only for lines. Word
// greys it on a closed shape because a closed path has no ends to decorate;
// here the engine has no "is this path open" answer yet, so the rows stay live
// and an arrowhead on a closed shape is stored and drawn at the path's start
// and end, which is what DrawingML says it means.
//
// Pure: no DOM, no engine. `io` is the reflection read and the gated write.

/** Word's Dashes, in its order, each with the model's token. */
export const DASH_CHOICES = Object.freeze([
  Object.freeze({ id: "solid", token: "solid", key: "shape.dash.solid" }),
  Object.freeze({ id: "roundDot", token: "systemDot", key: "shape.dash.roundDot" }),
  Object.freeze({ id: "squareDot", token: "systemDash", key: "shape.dash.squareDot" }),
  Object.freeze({ id: "dash", token: "dash", key: "shape.dash.dash" }),
  Object.freeze({ id: "dashDot", token: "dashDot", key: "shape.dash.dashDot" }),
  Object.freeze({ id: "longDash", token: "largeDash", key: "shape.dash.longDash" }),
  Object.freeze({ id: "longDashDot", token: "largeDashDot", key: "shape.dash.longDashDot" }),
  Object.freeze({ id: "longDashDotDot", token: "largeDashDotDot", key: "shape.dash.longDashDotDot" }),
]);

/** Word's Arrows gallery as start / end pairs, each with the model's tokens. */
export const ARROW_CHOICES = Object.freeze([
  Object.freeze({ id: "none", head: "none", tail: "none", key: "shape.arrows.none" }),
  Object.freeze({ id: "end", head: "none", tail: "triangle", key: "shape.arrows.end" }),
  Object.freeze({ id: "start", head: "triangle", tail: "none", key: "shape.arrows.start" }),
  Object.freeze({ id: "both", head: "triangle", tail: "triangle", key: "shape.arrows.both" }),
  Object.freeze({ id: "openEnd", head: "none", tail: "arrow", key: "shape.arrows.openEnd" }),
  Object.freeze({ id: "stealthEnd", head: "none", tail: "stealth", key: "shape.arrows.stealthEnd" }),
  Object.freeze({ id: "diamondEnd", head: "none", tail: "diamond", key: "shape.arrows.diamondEnd" }),
  Object.freeze({ id: "ovalEnd", head: "none", tail: "oval", key: "shape.arrows.ovalEnd" }),
]);

/**
 * The Dashes and Arrows submenus for a selected shape or picture.
 *
 * @param {object} io
 * @param {{dash: string, head: string, tail: string} | null} io.style  the current outline, read once
 * @param {boolean} io.picture          a picture's border has no Arrows (Word)
 * @param {boolean} io.mutationEnabled
 * @param {string} io.mutationReason
 * @param {(key: string) => string} io.t
 * @param {(dash?: string, head?: string, tail?: string) => void} io.apply
 * @returns {object[]}
 */
export function outlineStyleCommands(io) {
  const style = io.style ?? { dash: "", head: "", tail: "" };
  const gate = io.mutationEnabled ? { enabled: true } : { enabled: false, disabledReason: io.mutationReason };
  const out = [
    {
      id: "object.outline.dashes",
      label: io.t("shape.dashes"),
      group: "style",
      submenu: DASH_CHOICES.map((choice) => ({
        id: `object.outline.dash.${choice.id}`,
        label: io.t(choice.key),
        group: "dash",
        shortcut: style.dash === choice.token ? "✓" : "",
        ...gate,
        run: () => io.apply(choice.token, undefined, undefined),
      })),
    },
  ];
  if (!io.picture) {
    out.push({
      id: "object.outline.arrows",
      label: io.t("shape.arrows"),
      group: "style",
      submenu: ARROW_CHOICES.map((choice) => ({
        id: `object.outline.arrows.${choice.id}`,
        label: io.t(choice.key),
        group: "arrows",
        shortcut: style.head === choice.head && style.tail === choice.tail ? "✓" : "",
        ...gate,
        run: () => io.apply(undefined, choice.head, choice.tail),
      })),
    });
  }
  return out;
}

/** The current outline as `{dash, head, tail}` tokens, from a `shapeFormat`
 *  read, or null when the object has no outline. Frees the wasm value. */
export function readOutlineStyle(format) {
  if (!format) return null;
  try {
    if (!format.outline) return null;
    return { dash: format.outlineDash ?? "solid", head: format.headEnd ?? "none", tail: format.tailEnd ?? "none" };
  } finally {
    format.free?.();
  }
}
