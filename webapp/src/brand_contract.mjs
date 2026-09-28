// The white-label CONTRACT: what a host may set, what happens when they set
// something unreadable, and the stylesheet their choices produce.
//
// This is the half of `tools/build-brand.mjs` that is pure — no file system, no
// DOM — and it moved here for one reason. The SDK configuration playground
// (`webapp/playground.html`, `docs/126`) lets somebody pick an accent and see the
// live editor take it, and it must show **the generator's own refusal** when the
// colour fails AA: the same pair, the same measured ratio, the same floor, the
// same nearest passing value. The only way to publish a message the CLI really
// emits is to run the code the CLI runs, and a browser cannot import a module
// whose first line is `node:fs`.
//
// So: the contract and the arithmetic live here, and the generator keeps the file
// reading, the catalogues, the `editor.html` regions and the command line. Every
// name below is re-exported by `tools/build-brand.mjs`, so its callers did not
// move, and there is exactly ONE statement of which tokens are overridable and
// one of what a refusal says. A second copy for the browser would be the
// two-tables defect (`109` UX-006) applied to a permission-shaped API.
//
// §AA — WHAT HAPPENS WHEN A HOST'S COLOUR FAILS AA, AND WHY THIS CHOICE
//
// It is REFUSED, naming the pair, the measured ratio, the floor, and the nearest
// value in the same hue that would pass. Not corrected, and not warned about. The
// three candidates and why the other two are worse:
//
//   * CORRECT IT SILENTLY — ship the host's brand in a colour the host did not
//     choose. `src/contrast.mjs` already refuses to do this one layer down, for
//     the same reason, in as many words: "Nudging a document's colour until it
//     passes produces a preview that is a lie about the style." A product
//     shipped in an accent nobody approved is that lie with a bigger blast
//     radius, and the host discovers it from a screenshot.
//   * WARN AND SHIP IT — this is precisely ONLYOFFICE's failure mode, recorded
//     in `docs/125` §1.1: a custom loader logo is "not blocked — it is *nagged*.
//     It applies, then raises a 'paid feature' modal. Worse than refusing,
//     because the integrator ships it and then discovers the modal." A warning
//     in a build log is a warning nobody reads twice.
//   * REFUSE — the failure lands on the person who can still fix it, at the
//     moment it is free to fix, with the number and a value that works. It costs
//     a host one edit; it costs their readers nothing.
//
// The floors are WCAG's own split, so a brand colour is not refused for being a
// hairline: text pairs clear 4.5:1 (SC 1.4.3), edges and icons clear 3:1
// (SC 1.4.11). The tables live in `src/contrast.mjs` beside the arithmetic and
// are shared with `tests/style_tokens.test.mjs` — one statement of which token
// is text, not two.
//
// And the host's palette is measured AS IT WILL RENDER: the editor's own tokens
// with the host's written over them, per theme, with `var()` and `color-mix()`
// resolved. Measuring the host's five values in isolation would pass a brand
// colour that is unreadable only against OUR surface, which is the only place it
// will ever be painted.
import { auditPalette } from "./contrast.mjs";

/** The product's own identity: the name the catalogues were written with, and
 *  the mark `editor.html` shipped with.
 *
 *  Here rather than in `brand.json` because `brand.json` is the HOST's file —
 *  every field in it is a delta — and because this name is the needle the string
 *  overrides search the catalogues for. A test proves the catalogues really
 *  contain it, so a rename of the product cannot leave this constant behind
 *  silently substituting nothing. */
export const PRODUCT = Object.freeze({
  name: "OpenDoc",
  mark: "./opendoc-mark.svg",
  touchIcon: "./apple-touch-icon.png",
  tabTitle: "document+product",
});

/** Tab-title policies a host may choose.
 *
 *  `document+product` is the platform convention and today's behaviour: the
 *  document name first, because a tab strip truncates from the right. `document`
 *  drops our name from the HOST's browser tab, which is the single most visible
 *  place our brand leaks into someone else's product (`docs/125` §2 F4:
 *  "the host page's own browser tab title carries our brand"). */
export const TAB_TITLE_POLICIES = Object.freeze(["document+product", "document"]);

/** Tokens a host may NOT override, with the reason, because the reason is the
 *  point and a bare refusal would read as arbitrary.
 *
 *  The paper layer is theme-INVARIANT by design: the sheet is white in both
 *  themes, so everything painted onto the raster has a fixed contrast partner
 *  (`src/style.css` §paper). A host who darkened `--paper` would put the dark
 *  theme's markers on a dark sheet at roughly 2:1 and no existing guard would
 *  see it, because the paper layer is in neither role list. A host who wants a
 *  dark READING experience wants the chrome dark and the page white, which is
 *  what Word, Docs and ONLYOFFICE all do. */
export const LOCKED_TOKEN_PREFIXES = Object.freeze(["--paper"]);

/** Token groups a host may override, by prefix or exact name. Everything else is
 *  refused — including geometry (`--radius`, `--space-*`, `--h-*`) and z-order,
 *  which are structure rather than brand. `docs/63` calls the radii "a
 *  considered flat redesign" and says to propose visual changes rather than make
 *  them; a white-label seam that let a host round every corner would be making
 *  them on the owner's behalf, for every deployment. */
export const OVERRIDABLE = Object.freeze([
  "--accent",
  "--accent-strong",
  "--accent-ink",
  "--accent-soft",
  "--accent-line",
  "--accent-text",
  "--bg",
  "--bg-2",
  "--surface",
  "--surface-alt",
  "--ink",
  "--muted",
  "--faint",
  "--line",
  "--line-strong",
  "--primary-bg",
  "--primary-bg-hover",
  "--primary-ink",
]);

/** Tokens that live in the theme-INDEPENDENT block and therefore belong in
 *  `theme.tokens`, not in `theme.light` / `theme.dark`. Declaring one per theme
 *  is refused rather than quietly accepted: the accent fill is one colour in both
 *  themes on purpose, and a host who set it twice would be surprised by which
 *  one won. */
export const INDEPENDENT_ONLY = Object.freeze([
  "--accent",
  "--accent-strong",
  "--accent-ink",
  "--accent-soft",
  "--accent-line",
]);

const INDEPENDENT_SET = new Set(INDEPENDENT_ONLY);

/** JSON has no comments, and this is a file a host reads and edits, so a key
 *  beginning with `//` is a note rather than a setting. Dropped everywhere a map
 *  is walked, which is the only way a comment inside `theme.light` does not read
 *  as a token called `//`. */
const isNote = (key) => key.startsWith("//");

/** A refusal a host can act on. Carries every reason at once rather than the
 *  first: a build that fails five times for five colours is five builds. */
export class BrandRefusal extends Error {
  constructor(reasons) {
    super(`brand.json was refused:\n  - ${reasons.join("\n  - ")}`);
    this.name = "BrandRefusal";
    this.reasons = reasons;
  }
}

/** A path a host may reference: relative, same-origin, no scheme, no `..` that
 *  climbs out of the served directory, no protocol-relative `//host`. */
function refuseRemote(field, value, reasons) {
  if (/^[a-z][a-z0-9+.-]*:/i.test(value) || value.startsWith("//")) {
    reasons.push(
      `${field} "${value}" is an absolute or protocol-relative URL. Put the file next to the ` +
        "editor and name it with a relative path: the chrome must paint with the network off, " +
        "which is what tests/chrome_fonts.test.mjs exists to keep true.",
    );
    return false;
  }
  if (!value.startsWith("./") && !value.startsWith("../")) {
    reasons.push(`${field} "${value}" must start with "./" so it is unambiguously relative.`);
    return false;
  }
  return true;
}

/**
 * Validates a host's configuration and returns it normalized, or throws.
 *
 * Every refusal is collected before any is thrown. Complexity: O(tokens).
 *
 * @param {object} raw the parsed `brand.json`.
 * @param {Map<string, Record<string, string>>} themes the editor's own palettes.
 */
export function normalize(raw, themes) {
  const reasons = [];
  if (raw?.version !== 1) {
    reasons.push(
      `version must be 1; found ${JSON.stringify(raw?.version)}. An unknown major is refused ` +
        "rather than coerced (docs/125 §4 rule 1) — failing closed is the house rule.",
    );
  }
  const name = raw?.name ?? null;
  if (name !== null && (typeof name !== "string" || !name.trim())) {
    reasons.push('name must be a non-empty string, or null to keep the product\'s own name.');
  }
  // `null` keeps ours; a path uses theirs; `false` is "no mark anywhere", which is
  // a real configuration and not the same as an invisible image — a host
  // embedding an editor inside their own header already has a logo on screen.
  const mark = raw?.mark ?? null;
  if (mark !== null && mark !== false) {
    if (typeof mark !== "string" || !mark.trim()) {
      reasons.push("mark must be a relative path, false for no mark, or null to keep ours.");
    } else refuseRemote("mark", mark, reasons);
  }
  const tabTitle = raw?.tabTitle ?? null;
  if (tabTitle !== null && !TAB_TITLE_POLICIES.includes(tabTitle)) {
    reasons.push(`tabTitle must be one of ${TAB_TITLE_POLICIES.join(", ")}, or null.`);
  }

  const font = raw?.font ?? null;
  if (font !== null) {
    if (!font.family || typeof font.family !== "string") reasons.push("font.family is required.");
    if (!font.src || typeof font.src !== "string") reasons.push("font.src is required.");
    else refuseRemote("font.src", font.src, reasons);
  }

  const groups = { tokens: raw?.theme?.tokens ?? {}, light: raw?.theme?.light ?? {}, dark: raw?.theme?.dark ?? {} };
  for (const [group, entries] of Object.entries(groups)) {
    for (const [token, value] of Object.entries(entries)) {
      if (isNote(token)) continue;
      if (LOCKED_TOKEN_PREFIXES.some((prefix) => token.startsWith(prefix))) {
        reasons.push(
          `theme.${group} may not set ${token}. The paper layer is the same in both themes on ` +
            "purpose, so everything drawn onto the sheet has a fixed contrast partner; a host " +
            "who moves it puts the dark theme's markers on a dark page.",
        );
        continue;
      }
      if (!OVERRIDABLE.includes(token)) {
        reasons.push(
          `theme.${group} may not set ${token}. Overridable tokens are brand and palette only: ` +
            `${OVERRIDABLE.join(", ")}. Geometry and z-order are structure, not brand (docs/63).`,
        );
        continue;
      }
      if (group === "tokens" && !INDEPENDENT_SET.has(token)) {
        reasons.push(
          `${token} is per-theme, so it belongs in theme.light and theme.dark rather than ` +
            "theme.tokens — a single value would paint the dark theme with the light one's.",
        );
      }
      if (group !== "tokens" && INDEPENDENT_SET.has(token)) {
        reasons.push(
          `${token} is the same in both themes by design, so it belongs in theme.tokens rather ` +
            `than theme.${group}.`,
        );
      }
      if (typeof value !== "string" || !value.trim()) {
        reasons.push(`theme.${group}.${token} must be a non-empty CSS colour.`);
      }
    }
  }

  const strings = raw?.strings ?? {};
  for (const [tag, entries] of Object.entries(strings)) {
    if (isNote(tag)) continue;
    if (!entries || typeof entries !== "object") {
      reasons.push(`strings["${tag}"] must be an object of key → string.`);
      continue;
    }
    for (const [key, value] of Object.entries(entries)) {
      if (isNote(key)) continue;
      if (typeof value !== "string") reasons.push(`strings["${tag}"]["${key}"] must be a string.`);
    }
  }

  if (reasons.length) throw new BrandRefusal(reasons);
  const clean = (map) => Object.fromEntries(Object.entries(map).filter(([k]) => !isNote(k)));
  return {
    version: 1,
    name,
    mark,
    tabTitle,
    theme: { tokens: clean(groups.tokens), light: clean(groups.light), dark: clean(groups.dark) },
    font,
    strings: Object.fromEntries(
      Object.entries(strings)
        .filter(([tag]) => !isNote(tag))
        .map(([tag, entries]) => [tag, clean(entries)]),
    ),
  };
}

/**
 * The three palettes a host's overrides produce, and every AA failure in them.
 *
 * @returns {{palettes: Map<string, Record<string,string>>, failures: string[]}}
 */
export function auditBrand(config, themes) {
  const palettes = new Map();
  const failures = [];
  for (const [themeName, base] of themes) {
    const perTheme = themeName === "light" ? config.theme.light : config.theme.dark;
    const palette = { ...base, ...config.theme.tokens, ...perTheme };
    palettes.set(themeName, palette);
    const { failures: bad, unreadable } = auditPalette(palette);
    for (const value of unreadable) {
      failures.push(
        `${themeName}: ${value} cannot be read as a colour. Use #rgb, #rrggbb, or a ` +
          "color-mix(in srgb, …) of them — a value this cannot measure is a value nothing checks.",
      );
    }
    for (const f of bad) {
      failures.push(
        `${themeName}: ${f.role} (${f.value}) on ${f.on} (${f.ground}) measures ` +
          `${f.ratio.toFixed(2)}:1 and must clear ${f.floor}:1` +
          (f.suggestion ? `. ${f.suggestion} would pass, on the same hue.` : "."),
      );
    }
  }
  return { palettes, failures };
}

/** The banner every generated artifact opens with. Here rather than in the
 *  generator because `brandCss` writes it and `brandCss` is here. */
export const BANNER = (file) =>
  `GENERATED by webapp/tools/build-brand.mjs from webapp/brand.json — do not edit ${file}.`;

/** The palette declarations for one theme, as CSS text. */
function block(tokens) {
  return Object.entries(tokens)
    .map(([token, value]) => `  ${token}: ${value};`)
    .join("\n");
}

/** `src/brand.css`.
 *
 *  A STYLESHEET rather than a script, because the browser's own cascade applies
 *  it before the first paint with no ordering to get wrong and no flash to
 *  explain. It declares the host's tokens in the same three entry points
 *  `style.css` uses — light, system dark and explicit dark — because a host who
 *  overrode only `:root` would white-label the light theme and leave the dark one
 *  ours, and because a reader who picked Dark on a light OS only ever sees the
 *  explicit block (`docs/104` HF-092, three patches shipped in one block and not
 *  the other). */
export function brandCss(config, marks) {
  const lines = [`/* ${BANNER("this file")} */`, ""];
  lines.push("/* The brand mark, as a token so the element that paints it needs no id. */");
  lines.push(":root {");
  lines.push(`  --brand-mark: url("${marks.mark}");`);
  // The one marker the RUNTIME reads. `applySettings()` writes an INLINE
  // `--accent` on `:root` from `localStorage`, and an inline declaration beats
  // any stylesheet a host ships — `docs/125` §2 F4, still true when this landed.
  // Rather than adding a second configuration channel for "did the host pin the
  // accent", the answer travels in the stylesheet that pinned it: the runtime
  // reads this token and stops writing. One mechanism, and it means an embed can
  // be white-labelled by swapping one static file.
  if (config.theme.tokens["--accent"]) {
    lines.push("  /* Read by applySettings(): the host owns the accent, so the visitor's");
    lines.push("     stored preference must not be written inline over this. */");
    lines.push("  --brand-accent-pinned: 1;");
  }
  for (const [token, value] of Object.entries(config.theme.tokens)) {
    if (token === "--accent") continue;
    lines.push(`  ${token}: ${value};`);
  }
  if (config.theme.tokens["--accent"]) lines.push(`  --accent: ${config.theme.tokens["--accent"]};`);
  lines.push("}");
  if (config.font) {
    lines.push("");
    lines.push("/* The host's own face, self-hosted beside the editor. Never fetched.");
    lines.push("   `--font-stack` keeps the product's fallbacks behind it, so a face that");
    lines.push("   fails to load leaves readable chrome rather than a system serif. */");
    lines.push("@font-face {");
    lines.push(`  font-family: "${config.font.family}";`);
    lines.push(`  src: url("${marks.font}") format("${marks.fontFormat}");`);
    lines.push("  font-display: swap;");
    lines.push("}");
    lines.push(":root {");
    lines.push(`  --brand-font: "${config.font.family}";`);
    lines.push("}");
  }
  const light = block(config.theme.light);
  const dark = block(config.theme.dark);
  if (light) {
    lines.push("");
    lines.push(":root,");
    lines.push(':root[data-theme="light"] {');
    lines.push(light);
    lines.push("}");
  }
  if (dark) {
    lines.push("");
    lines.push("@media (prefers-color-scheme: dark) {");
    lines.push("  :root:not([data-theme]) {");
    lines.push(dark.replace(/^ {2}/gm, "    "));
    lines.push("  }");
    lines.push("}");
    lines.push("");
    lines.push(':root[data-theme="dark"] {');
    lines.push(dark);
    lines.push("}");
  }
  return `${lines.join("\n")}\n`;
}

/** Where the mark is referenced from, which is two different places with two
 *  different bases: `editor.html` resolves against the page, `src/brand.css`
 *  resolves against itself. Exported so the guard can assert both. */
export function markPaths(config) {
  const showMark = config.mark !== false;
  const pageMark = showMark ? (config.mark ?? PRODUCT.mark) : null;
  return {
    showMark,
    pageMark,
    // `url()` in a stylesheet resolves against the STYLESHEET, and this one lives
    // in `src/`, so a path the host wrote relative to the page needs one level up.
    mark: pageMark === null ? "data:," : pageMark.replace(/^\.\//, "../"),
    font: config.font ? config.font.src.replace(/^\.\//, "../") : null,
    fontFormat: config.font?.src.endsWith(".woff") ? "woff" : "woff2",
  };
}
