/** WCAG relative luminance and contrast, and the one rule for "may this colour
 *  be painted as text here".
 *
 *  This exists because the Styles gallery draws each card's label in the style it
 *  represents, colour included — Word's behaviour — but Word's gallery sits on a
 *  permanently light background and ours follows the theme. A document's own
 *  colours are absolute: Word's built-in Heading 1 is #2F5496, and painting that
 *  onto dark chrome gives roughly 1.7:1, i.e. an invisible label. The document is
 *  not wrong and neither is the theme; the pairing is, and only the pairing can
 *  be judged.
 *
 *  Kept as pure arithmetic in its own module so it can be unit-tested directly.
 *  The DOM half — resolving a CSS colour string to bytes — belongs to the caller,
 *  because only the caller knows what is actually behind the text. */

/** WCAG 2.1 relative luminance of an 8-bit sRGB colour. */
export function relativeLuminance({ r, g, b }) {
  const channel = (value) => {
    const v = value / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** WCAG contrast ratio between two opaque colours, 1:1 to 21:1. Symmetric. */
export function contrastRatio(a, b) {
  const [hi, lo] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Composites a possibly-translucent colour over an opaque one. */
export function composite(fg, bg) {
  const a = fg.a === undefined ? 1 : fg.a;
  return {
    r: fg.r * a + bg.r * (1 - a),
    g: fg.g * a + bg.g * (1 - a),
    b: fg.b * a + bg.b * (1 - a),
    a: 1,
  };
}

/** The floor a style-card preview colour has to clear to be used as authored.
 *
 *  Deliberately below the 4.5:1 body-text floor. A gallery card is a 30px
 *  decorative preview of a style, not prose, and holding it to the prose bar
 *  would discard nearly every authored colour on a dark theme — which throws away
 *  the preview this feature exists to give. 3:1 is WCAG's own large-text and
 *  non-text-contrast floor: enough that the label is unmistakably readable, loose
 *  enough that a mid-tone brand colour still previews as itself. */
export const PREVIEW_CONTRAST_FLOOR = 3;

/** Whether `ink` may be painted on `background` for a style preview.
 *
 *  The answer is a plain boolean rather than a "corrected" colour on purpose.
 *  Nudging a document's colour until it passes produces a preview that is a lie
 *  about the style — a heading previewed in a blue the document does not contain.
 *  Falling back to the theme's own ink at least says "this style's colour is not
 *  shown", which is honest, and the weight, size, slant and family previews all
 *  still carry the style's identity. */
export function previewInkIsLegible(ink, background) {
  if (!ink || !background) return false;
  return contrastRatio(composite(ink, background), background) >= PREVIEW_CONTRAST_FLOOR;
}
