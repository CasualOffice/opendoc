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
 * sanitizing: recognizes common formatting tags, `<a href>`, and a simple
 * inline `color` style; anything else (tables, images, scripts, styles,
 * unknown elements) is either skipped or flattened to its plain text — never
 * silently dropped, only unstyled. */
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
      const color = parseInlineColor(child.getAttribute("style"));
      if (color) next.color = color;
      walk(child, next);
    }
  }

  walk(root, {});
  return runs;
}

function parseInlineColor(style) {
  if (!style) return undefined;
  const hex = /color\s*:\s*(#[0-9a-fA-F]{6})\b/.exec(style);
  if (hex) return hex[1].toLowerCase();
  const rgb = /color\s*:\s*rgb\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*\)/.exec(style);
  if (!rgb) return undefined;
  const [r, g, b] = rgb.slice(1, 4).map(Number);
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
