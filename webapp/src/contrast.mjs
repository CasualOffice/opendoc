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

/** The floor a style preview colour has to clear to be used as authored.
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

// ---- The palette contract, shared ------------------------------------------
// `docs/126` phase 3 lets a HOST supply the palette, and "any host theme must
// still pass the contrast sweep" is the phase's own condition. That check is the
// one `tests/style_tokens.test.mjs` already performs on our palette — which
// role is text, which is an edge, and what each has to clear — so the tables
// move here rather than being written a second time for the host's palette. Two
// tables saying which token is text is exactly the `shortcut:`-labels drift
// (`109` UX-006) in a new costume, and this repository already carries FIVE
// copies of the WCAG arithmetic (`tests/e2e/contrast-audit.mjs` says so in its
// own header); this adds none and gives two callers one.
//
// Still pure arithmetic and pure data: the generator reads it in node, the
// stylesheet guard reads it in node, and the editor reads the two functions
// above in a browser.

/** The AA floor for text: WCAG 2.1 SC 1.4.3 at normal size and weight. */
export const TEXT_CONTRAST_FLOOR = 4.5;

/** The floor for a border, rule, edge or icon: SC 1.4.11 non-text contrast,
 *  which is also 1.4.3's large-text floor. A hairline held to the prose bar
 *  would be refused for being a hairline. */
export const UI_CONTRAST_FLOOR = 3;

/** Roles painted directly as text on `--surface`. Held to 4.5:1.
 *
 *  These are the numbers three shipped defects were actually about: review
 *  deletion text measured 2.38:1 on the dark surface, the mode pill 2.62:1, and
 *  `--faint` 3.29:1 in light (`docs/104` T-08 / T-17). */
export const TEXT_ROLES = Object.freeze([
  "--ink",
  "--muted",
  "--faint",
  "--success",
  "--warning",
  "--danger",
  "--info",
  "--review-move",
]);

/** Roles painted as a border, rule or edge straight onto `--surface`. 3:1.
 *
 *  Only the ones a rule sets directly; a `-line` token that is always mixed into
 *  another colour first (`--warning-line`, through `color-mix`) is read at its
 *  mixed value and not at this one. */
export const UI_ROLES = Object.freeze([
  "--success-line",
  "--danger-line",
  "--info-line",
  "--review-format-line",
]);

/** Pairs that are not "role on `--surface`" and so cannot be expressed by the
 *  two lists above, but still carry text and still have to be legible.
 *
 *  `--accent` is the FILL — the same value in both themes, by design
 *  (`style.css` §accent) — and `--accent-ink` is the text painted ON it. A host
 *  brand colour is exactly the thing most likely to break this pair, and
 *  NOTHING checked it before this phase: `style_tokens.test.mjs` holds eight
 *  text roles and four edges against `--surface`, and the accent is in neither
 *  list. So a host could supply a pale brand yellow, keep every existing role
 *  passing, and ship white text on it at 1.3:1. */
export const PAIRED_ROLES = Object.freeze([
  Object.freeze({ ink: "--accent-ink", on: "--accent", floor: TEXT_CONTRAST_FLOOR }),
  Object.freeze({ ink: "--primary-ink", on: "--primary-bg", floor: TEXT_CONTRAST_FLOOR }),
  // The accent as a foreground on the theme's own panel. Per-theme, and the
  // reason dark redefines it as a mix towards white rather than reusing the fill.
  Object.freeze({ ink: "--accent-text", on: "--surface", floor: TEXT_CONTRAST_FLOOR }),
]);

// NOT in the table, and stated rather than left as an omission — absence from a
// list is an overstatement by omission (`docs/99` §9.3):
//
//   * `--accent-soft` (13% towards transparent) and `--accent-line` (42%) are
//     TINTS. Measured, `--accent-line` sits at 1.97:1 on the light surface and
//     1.39:1 on dark. Holding them to SC 1.4.11 would red the palette we ship,
//     and greening that by loosening the floor is exactly the "inflate a
//     tolerance" move this repository forbids. The honest reading is that 1.4.11
//     governs a boundary that is the SOLE indicator of a component or its state,
//     and neither of these is: they are a wash behind a control that also
//     carries its own border, label and `aria-pressed`. Asserting a floor for
//     them would be asserting something about the design nobody has verified.
//     If a reviewer establishes that one of them IS a sole indicator somewhere,
//     it belongs here and the palette has a real defect to fix.
//   * `--accent-strong` (82% towards `--ink`) is a hover fill, always under
//     `--accent-ink`, and moves in the same direction as the pair above it.
//   * The `--paper-*` layer is checked nowhere here because it is not a theme:
//     the sheet is white in both themes ON PURPOSE, so a host may not move it at
//     all. The white-label generator refuses those tokens outright rather than
//     measuring them (see `tools/build-brand.mjs`).

/** `#abc` / `#aabbcc` → `{r, g, b}`, or null for anything else.
 *
 *  Deliberately refuses `color-mix()`, `rgb()` and named colours rather than
 *  guessing: a palette value this cannot read must be REPORTED, because a
 *  checker that silently skips what it cannot parse is a checker that passes on
 *  the one value somebody got wrong. Callers assert on the refusals. */
export function parseHex(value) {
  const text = String(value ?? "").trim();
  if (!/^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(text)) return null;
  let digits = text.slice(1);
  if (digits.length === 3) digits = [...digits].map((c) => c + c).join("");
  return {
    r: Number.parseInt(digits.slice(0, 2), 16),
    g: Number.parseInt(digits.slice(2, 4), 16),
    b: Number.parseInt(digits.slice(4, 6), 16),
  };
}

/** `{r, g, b}` → `#rrggbb`, lower case, which is the spelling the palette uses. */
export function toHex({ r, g, b }) {
  const byte = (v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0");
  return `#${byte(r)}${byte(g)}${byte(b)}`;
}

/**
 * Resolves a palette value to opaque-or-translucent sRGB, following the two
 * indirections the palette actually uses.
 *
 * The palette is not a table of literals: `--accent-text` is `var(--accent)` in
 * light and `color-mix(in srgb, var(--accent) 58%, #ffffff)` in dark, and the
 * `-soft`/`-line` family mixes towards `transparent`. A checker that cannot
 * follow those reports the accent family as unreadable and therefore never
 * checks the one family a host is most likely to break — so the two forms are
 * resolved here rather than declared out of scope.
 *
 * Supported, and nothing else: `#rgb`/`#rrggbb`, `transparent`, `var(--name)`
 * (recursively, with a depth bound so a cycle cannot hang a build), and
 * `color-mix(in srgb, <value> <p>%, <value>)`. Anything else returns null and the
 * caller reports it, because guessing is how a checker passes on the value
 * somebody got wrong.
 *
 * Complexity: O(depth), bounded at 8.
 *
 * @param {string|undefined} value
 * @param {Record<string, string>} palette
 * @param {number} [depth]
 * @returns {{r:number,g:number,b:number,a:number}|null}
 */
export function resolveColour(value, palette, depth = 0) {
  if (depth > 8) return null;
  const text = String(value ?? "").trim();
  if (!text) return null;
  if (text === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  const hex = parseHex(text);
  if (hex) return { ...hex, a: 1 };
  const variable = text.match(/^var\(\s*(--[\w-]+)\s*\)$/);
  if (variable) return resolveColour(palette[variable[1]], palette, depth + 1);
  const mix = text.match(/^color-mix\(\s*in\s+srgb\s*,\s*(.+?)\s+([\d.]+)%\s*,\s*(.+?)\s*\)$/);
  if (!mix) return null;
  const first = resolveColour(mix[1], palette, depth + 1);
  const second = resolveColour(mix[3], palette, depth + 1);
  if (!first || !second) return null;
  const weight = Number.parseFloat(mix[2]) / 100;
  if (!Number.isFinite(weight)) return null;
  // sRGB mixing, premultiplied by alpha, which is what `color-mix(in srgb, …)`
  // specifies and what makes `X 42%, transparent` come out as X at 0.42 alpha.
  const a = first.a * weight + second.a * (1 - weight);
  if (a === 0) return { r: 0, g: 0, b: 0, a: 0 };
  const channel = (k) => (first[k] * first.a * weight + second[k] * second.a * (1 - weight)) / a;
  return { r: channel("r"), g: channel("g"), b: channel("b"), a };
}

/**
 * Every role in a palette that fails its floor.
 *
 * `palette` is a plain `name → value` map — one theme's worth. Returns a list of
 * `{ role, on, value, ground, ratio, floor, suggestion }`, empty when the
 * palette passes, plus a list of values that could not be read at all.
 *
 * `suggestion` is the nearest value in the SAME HUE that clears the floor,
 * found by walking lightness towards the far end. It is offered so a refusal can
 * say what would work — never applied. Nudging a host's brand colour until it
 * passes ships a product in a colour the host did not choose and did not agree
 * to, which is the same lie `previewInkIsLegible` refuses to tell about a
 * document's own colours, one layer up.
 *
 * Complexity: O(roles) — about twenty — with a bounded 256-step search per
 * failure. Runs in a generator and in a unit test, never per interaction.
 *
 * @param {Record<string, string>} palette
 * @returns {{failures: object[], unreadable: string[]}}
 */
export function auditPalette(palette) {
  const failures = [];
  const unreadable = [];
  const read = (name) => {
    const value = palette[name];
    if (value === undefined) return null;
    const parsed = resolveColour(value, palette);
    if (!parsed) {
      unreadable.push(`${name}: ${value}`);
      return null;
    }
    return parsed;
  };
  const check = (inkName, groundName, floor) => {
    const ink = read(inkName);
    const raw = read(groundName);
    if (!ink || !raw) return;
    // A translucent GROUND is measured where it actually sits, which for every
    // pair in these tables is the theme's own panel. `--surface` itself is
    // opaque in all three palettes; this only matters if a host makes one
    // translucent, and then the honest reading is "over the surface it replaces".
    const ground = raw.a === 1 ? raw : composite(raw, { r: 255, g: 255, b: 255, a: 1 });
    const ratio = contrastRatio(composite(ink, ground), ground);
    if (ratio >= floor) return;
    failures.push({
      role: inkName,
      on: groundName,
      value: palette[inkName],
      ground: palette[groundName],
      ratio,
      floor,
      suggestion: nearestLegible(composite(ink, ground), ground, floor),
    });
  };
  for (const role of TEXT_ROLES) check(role, "--surface", TEXT_CONTRAST_FLOOR);
  for (const role of UI_ROLES) check(role, "--surface", UI_CONTRAST_FLOOR);
  for (const pair of PAIRED_ROLES) check(pair.ink, pair.on, pair.floor);
  return { failures, unreadable: [...new Set(unreadable)] };
}

/** The nearest value to `ink` that clears `floor` against `ground`, kept on the
 *  same hue by scaling the channels towards black or towards white — whichever
 *  direction the ground is not. Null when even the extreme fails, which happens
 *  on a mid-grey ground and is worth saying rather than suggesting `#000000`
 *  that also fails. */
function nearestLegible(ink, ground, floor) {
  const towardsWhite = relativeLuminance(ground) < 0.5;
  for (let step = 1; step <= 256; step += 1) {
    const t = step / 256;
    const candidate = {
      r: ink.r + (towardsWhite ? (255 - ink.r) * t : -ink.r * t),
      g: ink.g + (towardsWhite ? (255 - ink.g) * t : -ink.g * t),
      b: ink.b + (towardsWhite ? (255 - ink.b) * t : -ink.b * t),
    };
    if (contrastRatio(candidate, ground) >= floor) return toHex(candidate);
  }
  return null;
}
