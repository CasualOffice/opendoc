// Shape Fill ▸ Gradient (`docs/109` RM-30).
//
// Competitive standard first. Word's Shape Fill ▸ Gradient offers Light and
// Dark variations of the shape's current colour in eight linear directions
// and from the centre; Google Docs' drawing fill has a Gradient tab with
// presets; ONLYOFFICE's shape panel has a Gradient fill type with linear and
// radial styles and a direction picker
// (`web-apps/apps/documenteditor/main/app/view/ShapeSettings.js`,
// `FILL_TYPE_GRAD`, :194). This is Word's Light variation: the shape's own colour
// fading to a tint of it, in Word's directions, written by the engine's
// `setShapeGradient` as a two-stop `a:gradFill`.
//
// Deliberately not here yet: the Dark variation and a stop editor (Word's
// More Gradients pane). The engine takes any two stops, so both are host work.
//
// Pure: no DOM, no engine.

/** Word's directions: the angle is `a:lin@ang` in degrees, clockwise from the
 *  positive x-axis; `null` is the radial From Center. */
export const GRADIENT_CHOICES = Object.freeze([
  Object.freeze({ id: "linearDown", angle: 90, key: "shape.gradient.linearDown" }),
  Object.freeze({ id: "linearUp", angle: 270, key: "shape.gradient.linearUp" }),
  Object.freeze({ id: "linearRight", angle: 0, key: "shape.gradient.linearRight" }),
  Object.freeze({ id: "linearLeft", angle: 180, key: "shape.gradient.linearLeft" }),
  Object.freeze({ id: "linearDiagonal", angle: 45, key: "shape.gradient.linearDiagonal" }),
  Object.freeze({ id: "fromCenter", angle: null, key: "shape.gradient.fromCenter" }),
]);

/** The colour a shape with no fill starts its gradient from: the editor's
 *  accent, which is what a new shape is filled with. */
export const DEFAULT_GRADIENT_BASE = "#3355c4";

/** `hex` mixed toward white by `amount` (0..1), as `#rrggbb`. O(1). */
export function tint(hex, amount) {
  const match = /^#?([0-9a-f]{6})$/i.exec(String(hex ?? ""));
  const value = match ? parseInt(match[1], 16) : parseInt(DEFAULT_GRADIENT_BASE.slice(1), 16);
  const channel = (shift) => {
    const c = (value >> shift) & 0xff;
    return Math.round(c + (255 - c) * amount)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${channel(16)}${channel(8)}${channel(0)}`;
}

/**
 * The Gradient submenu for a selected shape.
 *
 * @param {object} io
 * @param {string | null} io.fill        the shape's current fill colour, or null
 * @param {boolean} io.mutationEnabled
 * @param {string} io.mutationReason
 * @param {(key: string) => string} io.t
 * @param {(from: string, to: string, angle: number | undefined) => void} io.apply
 */
export function gradientCommands(io) {
  const base = /^#[0-9a-f]{6}$/i.test(io.fill ?? "") ? io.fill.toLowerCase() : DEFAULT_GRADIENT_BASE;
  const light = tint(base, 0.75);
  const gate = io.mutationEnabled ? { enabled: true } : { enabled: false, disabledReason: io.mutationReason };
  return {
    id: "object.fill.gradient",
    label: io.t("shape.gradient"),
    group: "gradient",
    submenu: GRADIENT_CHOICES.map((choice) => ({
      id: `object.fill.gradient.${choice.id}`,
      label: io.t(choice.key),
      group: "gradient",
      ...gate,
      run: () => io.apply(base, light, choice.angle ?? undefined),
    })),
  };
}
