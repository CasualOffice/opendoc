// Drawing a style's NAME in that style, legibly.
//
// The menu exists so a reader can see what applying a style does, which means
// the row has to be painted in the style — and a document's colours are
// absolute while the chrome's are not. Word's built-in Heading 1 is #2F5496,
// which sits at about 1.7:1 on this product's dark surface: the label was
// painted, correctly, in a colour nobody could see. Word never has to solve
// this because its gallery is always light.
//
// So the authored colour is kept on the element and the pairing is re-decided
// whenever the surface changes — including the "System" theme flipping under
// us, which changes no attribute and so fires nothing else.
//
// `stylePreviewOf` is injected: it is the one engine call here, and taking it
// as an argument is what lets the legibility decision be reasoned about (and
// the module loaded) without an engine behind it.

import { previewInkIsLegible } from "./contrast.mjs";
import { previewPx } from "./style_picker.mjs";

/** Resolves any CSS color string to sRGB bytes.
 *
 *  Via canvas rather than a regex: the browser hands back `rgb()`, `color(srgb
 *  ...)`, `color-mix(...)` and `oklab(...)` depending on how the value was
 *  authored and whether a transition is in flight, and a regex that assumes one
 *  form reads another's components as RGB. Canvas understands every CSS color
 *  syntax, including ones that do not exist yet. */
let colorProbeContext = null;
export function resolveColor(value) {
  if (!value) return null;
  if (!colorProbeContext) {
    colorProbeContext = document
      .createElement("canvas")
      .getContext("2d", { willReadFrequently: true });
  }
  const ctx = colorProbeContext;
  // An unparseable value leaves fillStyle untouched, so a sentinel distinguishes
  // "transparent black" from "the browser rejected this string".
  ctx.fillStyle = "#ff00ff";
  ctx.fillStyle = value;
  if (ctx.fillStyle === "#ff00ff" && !/^#ff00ff$|magenta/i.test(value.trim()))
    return null;
  ctx.clearRect(0, 0, 1, 1);
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
  return { r, g, b, a: a / 255 };
}

/** The background a gallery card's label is actually painted on: the first
 *  opaque ancestor, since the card itself is usually transparent. */
function effectiveBackground(element) {
  for (
    let node = element;
    node && node.nodeType === 1;
    node = node.parentElement
  ) {
    const color = resolveColor(getComputedStyle(node).backgroundColor);
    if (color && color.a === 1) return color;
  }
  return resolveColor(getComputedStyle(document.body).backgroundColor);
}

/** Paints the authored preview color if it is legible here, otherwise falls back
 *  to the card's theme ink. */
export function applyPreviewInk(label) {
  const authored = label.dataset.previewColor;
  if (!authored) {
    label.style.color = "";
    return;
  }
  const ink = resolveColor(authored);
  const background = effectiveBackground(label);
  label.style.color = previewInkIsLegible(ink, background) ? authored : "";
}

// On the "System" setting no attribute changes when the OS flips to dark, so
// nothing above would fire — but the palette underneath the cards has changed
// completely. Without this the gallery keeps light-theme decisions on a dark
// surface, which is the exact bug, just reached by the more common route.
window
  .matchMedia("(prefers-color-scheme: dark)")
  .addEventListener("change", () => refreshStylePreviews());

/** Re-decides every card's preview color. The cards are built once per document,
 *  but the surface under them changes with the theme, and a decision made against
 *  the light palette is not valid against the dark one. */
export function refreshStylePreviews() {
  for (const label of document.querySelectorAll(
    ".style-option-name[data-preview-color]",
  )) {
    applyPreviewInk(label);
  }
}

/** Draws an option's label IN the style it represents, from the engine's resolved
 *  preview (family/size/weight/slant/underline/colour) — which is the whole reason
 *  the list is a product popup rather than a native `<select>`: an OS popup renders
 *  every row in the system font, so "Heading 1" would be a word rather than a
 *  picture of what applying it does. Degrades to the plain label when no preview is
 *  available; the size is clamped so a 28pt Title stays a menu row. */
export function applyStylePreview(label, name, stylePreviewOf) {
  let preview;
  try {
    preview = stylePreviewOf?.(name);
  } catch {
    return; // unknown style — keep the plain label (graceful degrade)
  }
  if (!preview) return;
  const family = preview.fontFamily;
  if (family)
    label.style.fontFamily = `"${family.replace(/"/g, "")}", sans-serif`;
  const px = previewPx(preview.sizePoints);
  if (px) label.style.fontSize = `${px}px`;
  label.style.fontWeight = preview.bold ? "700" : "450";
  label.style.fontStyle = preview.italic ? "italic" : "normal";
  label.style.textDecoration = preview.underline ? "underline" : "none";
  // Explicit RGB colors preview as authored — but only when they are actually
  // legible on the card. Automatic/theme colors resolve to an empty string and
  // inherit the card's theme-aware ink.
  //
  // A document's colors are absolute and the chrome's are not. Word's built-in
  // Heading 1 is #2F5496, which sits at about 1.7:1 on the dark theme's surface:
  // the label was painted, correctly, in a color nobody could see. Word never has
  // to solve this because its gallery is always light. Ours follows the theme, so
  // the pairing has to be judged every time it changes — hence the authored value
  // is kept on the element and re-decided in `refreshStylePreviews`.
  label.dataset.previewColor = preview.color || "";
  applyPreviewInk(label);
  const align = preview.alignment;
  label.style.textAlign =
    align === "start" ? "left" : align === "end" ? "right" : align;
}
