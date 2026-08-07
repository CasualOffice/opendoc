// Rich-clipboard bridge: building/parsing the `text/html` clipboard payload
// for copy/paste (docs/67-EDITOR-UX-GAP-ANALYSIS.md, "Native clipboard
// fidelity"). Scoped to paragraphs, run formatting, and hyperlinks — the
// P0 daily-editing surface; tables/lists/images remain plain text.
//
// The WASM engine (`doc.copyRichRuns`/`doc.pasteRichRuns`) is the single
// source of truth for the run shape: `{ text, bold?, italic?, underline?,
// strike?, sizeHalfPoints?, color?, highlight?, vertAlign?, font?, href?,
// paragraphBreak? }`. This module only builds/parses HTML around that shape
// — it never invents new fields.

const MARKER_PREFIX = "opendoc-clipboard-runs:";

export function escapeHtml(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function styleAttr(run) {
  const decls = [];
  if (run.color) decls.push(`color:${run.color}`);
  if (run.sizeHalfPoints) decls.push(`font-size:${run.sizeHalfPoints / 2}pt`);
  if (run.font) decls.push(`font-family:${run.font.replaceAll('"', "'")}`);
  if (run.highlight && run.highlight !== "none") {
    decls.push(`background-color:${run.highlight}`);
  }
  return decls.length ? ` style="${decls.join(";")}"` : "";
}

/** Builds a visible HTML fragment from `ClipboardRun[]` (the shape
 * `doc.copyRichRuns` produces). Every paragraph becomes a `<p>`; run
 * formatting nests `<b>/<i>/<u>/<s>/<sup>/<sub>`, a `<span style>` carries
 * color/size/font/highlight, and a `href` wraps the run in `<a>`. */
export function runsToHtml(runs) {
  const paragraphs = [[]];
  for (const run of runs) {
    if (run.paragraphBreak) {
      paragraphs.push([]);
    } else {
      paragraphs.at(-1).push(run);
    }
  }
  return paragraphs
    .map((runsInParagraph) => {
      const inner = runsInParagraph.map(runToHtml).join("");
      return `<p>${inner || "<br>"}</p>`;
    })
    .join("");
}

function runToHtml(run) {
  let html = escapeHtml(run.text).replaceAll("\n", "<br>");
  if (run.bold) html = `<b>${html}</b>`;
  if (run.italic) html = `<i>${html}</i>`;
  if (run.underline) html = `<u>${html}</u>`;
  if (run.strike) html = `<s>${html}</s>`;
  if (run.vertAlign === "super") html = `<sup>${html}</sup>`;
  if (run.vertAlign === "sub") html = `<sub>${html}</sub>`;
  const style = styleAttr(run);
  if (style) html = `<span${style}>${html}</span>`;
  if (run.href) html = `<a href="${escapeHtml(run.href)}">${html}</a>`;
  return html;
}

const BLOCK_TAGS = new Set(["P", "DIV", "LI", "H1", "H2", "H3", "H4", "H5", "H6"]);
const SKIP_TAGS = new Set(["SCRIPT", "STYLE"]);

/** Parses a browser-native DOM (from an external app's `text/html` paste —
 * Word, Docs, a browser selection) into `ClipboardRun[]`. Best-effort and
 * sanitizing: recognizes common formatting tags, `<a href>`, and inline CSS on
 * the `style` attribute (Google Docs / Word wrap every run in a styled `<span>`
 * with no `<b>/<i>/<u>` tags, so tag-only detection would flatten them). Tag
 * and inline-style signals are merged — either sets a format on. Anything else
 * (tables, images, scripts, styles, unknown elements) is either skipped or
 * flattened to its plain text — never silently dropped, only unstyled. */
export function htmlToRuns(root) {
  const runs = [];
  let sawParagraph = false;

  function pushText(text, format) {
    if (text) runs.push({ text, ...format });
  }

  function walk(node, format) {
    for (const child of node.childNodes) {
      if (child.nodeType === Node.TEXT_NODE) {
        pushText(child.textContent, format);
        continue;
      }
      if (child.nodeType !== Node.ELEMENT_NODE) continue;
      const tag = child.tagName;
      if (SKIP_TAGS.has(tag)) continue;
      if (tag === "BR") {
        pushText("\n", format);
        continue;
      }
      if (BLOCK_TAGS.has(tag)) {
        if (sawParagraph) runs.push({ paragraphBreak: true });
        sawParagraph = true;
      }
      // Start from the inherited format, then let this element's tag and its
      // inline style each turn formats on (either signal wins). Inline style
      // is read last so a Docs-style `<span style>` with no tags is honored,
      // and a deeper span's size/color overrides an ancestor's.
      const next = { ...format };
      if (tag === "B" || tag === "STRONG") next.bold = true;
      if (tag === "I" || tag === "EM") next.italic = true;
      if (tag === "U") next.underline = true;
      if (tag === "S" || tag === "STRIKE" || tag === "DEL") next.strike = true;
      if (tag === "SUP") next.vertAlign = "super";
      if (tag === "SUB") next.vertAlign = "sub";
      if (tag === "A") {
        const href = child.getAttribute("href");
        if (href) next.href = href;
      }
      applyInlineStyle(next, child.getAttribute("style"));
      walk(child, next);
    }
  }

  walk(root, {});
  return runs;
}

// `sizeHalfPoints` is `pt * 2` (matches main.js run-size handling). Clamp to a
// sane range — Word caps font size at 1638pt; the floor is 1pt.
const MIN_SIZE_HALF_POINTS = 2;
const MAX_SIZE_HALF_POINTS = 3276;

/** Mutates `format` with the run properties an element's inline `style`
 * declares, mapping CSS to the model's run shape. Only ever turns formats on
 * (never unsets an inherited one), so it composes with tag-based signals and
 * ancestor styles the same way "either wins" nesting does. */
function applyInlineStyle(format, style) {
  if (!style) return;
  const decls = parseDeclarations(style);

  const weight = decls["font-weight"];
  if (weight && (weight === "bold" || weight === "bolder" || Number(weight) >= 600)) {
    format.bold = true;
  }
  const fontStyle = decls["font-style"];
  if (fontStyle && (fontStyle.startsWith("italic") || fontStyle.startsWith("oblique"))) {
    format.italic = true;
  }
  const decoration = decls["text-decoration-line"] || decls["text-decoration"];
  if (decoration) {
    if (/\bunderline\b/.test(decoration)) format.underline = true;
    if (/\bline-through\b/.test(decoration)) format.strike = true;
  }
  const vAlign = decls["vertical-align"];
  if (vAlign === "super") format.vertAlign = "super";
  if (vAlign === "sub") format.vertAlign = "sub";

  const size = parseFontSize(decls["font-size"]);
  if (size !== undefined) format.sizeHalfPoints = size;

  const color = parseCssColor(decls.color);
  if (color) format.color = color;
  // background-color -> highlight. `w:highlight` is a NAMED enum (yellow, green,
  // …), not arbitrary hex — and the engine maps any unrecognized name to Yellow,
  // so sending a raw hex would turn EVERY pasted background (incl. subtle table/
  // paragraph shading) into a bright-yellow highlight. Snap the background to the
  // nearest classic highlighter color, and only when it is bright+saturated
  // enough to actually be a highlight; ordinary shading/near-white/gray is
  // dropped rather than mis-highlighted.
  const highlight = nearestHighlightName(parseCssColor(decls["background-color"]));
  if (highlight) format.highlight = highlight;
}

// The classic highlighter colors the engine recognizes as `w:highlight` names.
const HIGHLIGHT_PALETTE = [
  ["yellow", 0xff, 0xff, 0x00],
  ["green", 0x00, 0xff, 0x00],
  ["cyan", 0x00, 0xff, 0xff],
  ["magenta", 0xff, 0x00, 0xff],
  ["red", 0xff, 0x00, 0x00],
  ["blue", 0x00, 0x00, 0xff],
];

/** Maps a `#rrggbb` background color to the nearest named highlight color, or
 *  `null` when it is not a bright, saturated highlighter color (so ordinary
 *  shading — near-white/gray/dark or low-saturation fills — is dropped instead
 *  of becoming a phantom highlight). */
function nearestHighlightName(hex) {
  if (!hex) return null;
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/.exec(hex);
  if (!m) return null;
  const [r, g, b] = m.slice(1, 4).map((h) => parseInt(h, 16));
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  // Require a bright, saturated color — a real highlighter, not shading.
  if (max < 128 || max - min < 64) return null;
  let best = null;
  let bestDist = Infinity;
  for (const [name, pr, pg, pb] of HIGHLIGHT_PALETTE) {
    const dist = (r - pr) ** 2 + (g - pg) ** 2 + (b - pb) ** 2;
    if (dist < bestDist) {
      bestDist = dist;
      best = name;
    }
  }
  return best;
}

/** Splits a CSS declaration string into a `{ property: value }` map, with
 * properties lowercased and values trimmed+lowercased. */
function parseDeclarations(style) {
  const map = {};
  for (const decl of style.split(";")) {
    const idx = decl.indexOf(":");
    if (idx === -1) continue;
    const prop = decl.slice(0, idx).trim().toLowerCase();
    const value = decl.slice(idx + 1).trim().toLowerCase();
    if (prop && value) map[prop] = value;
  }
  return map;
}

/** Converts a CSS `font-size` (pt or px) to the model's `sizeHalfPoints`, or
 * `undefined` when absent/unparseable/zero. px is converted at 96dpi
 * (1px = 0.75pt). Clamped to a sane range. */
function parseFontSize(value) {
  if (!value) return undefined;
  const match = /^([\d.]+)\s*(pt|px)$/.exec(value);
  if (!match) return undefined;
  const num = Number(match[1]);
  if (!Number.isFinite(num) || num <= 0) return undefined;
  const pt = match[2] === "px" ? num * 0.75 : num;
  const halfPoints = Math.round(pt * 2);
  if (halfPoints < MIN_SIZE_HALF_POINTS) return MIN_SIZE_HALF_POINTS;
  if (halfPoints > MAX_SIZE_HALF_POINTS) return MAX_SIZE_HALF_POINTS;
  return halfPoints;
}

/** Parses a CSS color value (6-digit hex, 3-digit hex, or `rgb()`/`rgba()`) to
 * a lowercase `#rrggbb`, or `undefined` for anything else — including the
 * `transparent`/no-fill keywords Docs and Word emit on unstyled backgrounds. */
function parseCssColor(value) {
  if (!value || value === "transparent") return undefined;
  const hex6 = /^#([0-9a-f]{6})$/.exec(value);
  if (hex6) return `#${hex6[1]}`;
  const hex3 = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/.exec(value);
  if (hex3) return `#${hex3[1]}${hex3[1]}${hex3[2]}${hex3[2]}${hex3[3]}${hex3[3]}`;
  const rgb = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})/.exec(value);
  if (!rgb) return undefined;
  const [r, g, b] = rgb.slice(1, 4).map(Number);
  if ([r, g, b].some((n) => n > 255)) return undefined;
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}

function utf8ToBase64(text) {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
  }
  return btoa(binary);
}

function base64ToUtf8(base64) {
  const binary = atob(base64);
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

/** Wraps the exact JSON `doc.copyRichRuns` produced in a leading HTML
 * comment, so a same-origin internal paste can round-trip losslessly instead
 * of reinterpreting its own visible HTML. External apps see (and ignore) an
 * ordinary comment. */
export function embedMarker(runsJson) {
  return `<!--${MARKER_PREFIX}${utf8ToBase64(runsJson)}-->`;
}

/** Extracts the `runsJson` string `embedMarker` embedded, or `null` if `html`
 * has no (valid) leading marker — an external app's paste, or a stale/edited
 * clipboard payload. */
export function extractMarker(html) {
  const pattern = new RegExp(`^\\s*<!--${MARKER_PREFIX}([A-Za-z0-9+/=]+)-->`);
  const match = pattern.exec(html);
  if (!match) return null;
  try {
    return base64ToUtf8(match[1]);
  } catch {
    return null;
  }
}
