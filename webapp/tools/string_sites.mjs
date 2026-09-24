// Where user-facing English still lives in the source (docs/124 §4).
//
// One scanner, used by the gate that ratchets the count down and by anything
// else that needs to know what has not been routed through the seam yet. It is
// deliberately a SCANNER and not a parser: a parser would be more precise and
// would also be a second implementation of JavaScript to maintain. What a
// ratchet needs is determinism — the same source must always produce the same
// number — and a bias towards counting a site it is unsure about, because a
// false positive costs somebody one allowlist entry with a reason and a false
// negative is a string that ships untranslated.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

/** Attributes whose value a person reads or hears. `alt` included: a screen
 *  reader says it. `data-*` excluded: those are machine-facing. */
const HUMAN_ATTRIBUTES = ["title", "aria-label", "placeholder", "alt", "aria-roledescription"];

/** Properties and helpers whose string argument reaches a person. */
const HUMAN_SINKS = [
  "textContent",
  "innerText",
  "ariaLabel",
  "placeholder",
  "title",
  "setStatus",
  "label",
  "reason",
  "disabledReason",
  "blurb",
  "description",
];

/** A value worth translating: it has two consecutive letters somewhere. This is
 *  what keeps `"—"`, `"1"`, `"#e2622a"`, `"px"` and every CSS fragment out. */
const TRANSLATABLE = /[A-Za-z]{2}/;

/** Values that are letters but are NOT prose, listed rather than pattern-matched
 *  so each exclusion is visible and arguable. */
const NOT_PROSE = new Set([
  "true",
  "false",
  "none",
  "auto",
  "block",
  "inline",
  "flex",
  "grid",
  "hidden",
  "button",
  "text",
  "checkbox",
  "radio",
  "menu",
  "menuitem",
  "dialog",
  "separator",
  "listbox",
  "combobox",
  "tabpanel",
  "region",
  "polite",
  "assertive",
  "off",
  "on",
]);

function isProse(value) {
  const text = value.trim();
  if (UNROUTABLE_TEXT.has(text)) return false;
  if (!TRANSLATABLE.test(text)) return false;
  if (NOT_PROSE.has(text.toLowerCase())) return false;
  // A bare identifier or dotted id — `file.export.pdf`, `chevron_right`,
  // `keyboard_arrow_up` — is a key or an icon name, not a sentence.
  if (/^[a-z][a-z0-9]*([._-][a-z0-9]+)*$/.test(text)) return false;
  // A url, a selector, a mime type, a format string.
  if (/^(https?:|\.\/|\/|#|[.#][A-Za-z-]+$|[a-z]+\/[a-z0-9+.-]+$)/.test(text)) return false;
  return true;
}

/** The strings that CANNOT go through the seam, each with the reason.
 *
 *  Promised by `docs/124` §4 and empty until something earned a place in it.
 *  An entry here is a claim that routing the string would be WRONG, not that
 *  routing it is inconvenient — so the list is expected to stay tiny, and a
 *  reviewer should push back on any addition that reads like the latter.
 */
const UNROUTABLE = [
  {
    text: "Loading engine…",
    reason:
      "The pre-boot placeholder. It is on screen before any script runs, which " +
      "is before a catalogue could exist, so English is the only thing it can " +
      "honestly say. It was routed once: the sweep then wrote it back over the " +
      "value the shell had cleared and the no-document editor claimed to be " +
      "loading forever.",
  },
];

const UNROUTABLE_TEXT = new Set(UNROUTABLE.map((entry) => entry.text));

/** The allowlist, for a guard that wants to print the reasons. */
export function unroutableStrings() {
  return UNROUTABLE.map((entry) => ({ ...entry }));
}

/** Markup sites: a human-readable attribute, or a text node, with no
 *  `data-i18n*` on the element that would route it through the seam. */
export function scanMarkup(source) {
  const sites = [];
  const tags = [...source.matchAll(/<([a-zA-Z][\w-]*)\b([^>]*)>/g)];
  for (const tag of tags) {
    const [whole, name, attributes] = tag;
    if (name === "script" || name === "style") continue;
    const routed = /\bdata-i18n(-[a-z]+)?=/.test(attributes);
    for (const attribute of HUMAN_ATTRIBUTES) {
      const match = attributes.match(new RegExp(`\\b${attribute}="([^"]*)"`));
      if (!match || !isProse(match[1])) continue;
      if (routed && new RegExp(`data-i18n-${attribute.replace("aria-", "")}=`).test(attributes)) {
        continue;
      }
      sites.push({ kind: "attribute", attribute, text: match[1], at: lineOf(source, tag.index) });
    }
    if (routed && /\bdata-i18n=/.test(attributes)) continue;
    // The text immediately inside this tag, up to the next tag.
    const after = source.slice(tag.index + whole.length);
    const text = after.slice(0, after.search(/[<]/) === -1 ? 0 : after.search(/[<]/));
    if (isProse(text)) {
      sites.push({ kind: "text", element: name, text: text.trim(), at: lineOf(source, tag.index) });
    }
  }
  return sites;
}

/** Script sites: a string literal handed to something a person reads. */
export function scanScript(source) {
  const sites = [];
  const sinks = HUMAN_SINKS.join("|");
  const patterns = [
    // `x.textContent = "…"`, `title: "…"`, `setStatus("…")`
    new RegExp(`\\b(${sinks})\\s*(?:=|:)\\s*(["'\`])((?:(?!\\2)[^\\\\]|\\\\.)*)\\2`, "g"),
    new RegExp(`\\b(${sinks})\\s*\\(\\s*(["'\`])((?:(?!\\2)[^\\\\]|\\\\.)*)\\2`, "g"),
    // `setAttribute("aria-label", "…")`
    /setAttribute\(\s*["'](?:aria-label|title|placeholder|alt)["']\s*,\s*(["'`])((?:(?!\1)[^\\]|\\.)*)\1/g,
  ];
  for (const pattern of patterns) {
    for (const match of source.matchAll(pattern)) {
      const text = match[3] ?? match[2];
      if (!isProse(text)) continue;
      // Already routed: `title: t("…")` never matches, because the literal is
      // not adjacent to the sink.
      sites.push({ kind: "script", sink: match[1] ?? "setAttribute", text, at: lineOf(source, match.index) });
    }
  }
  return sites;
}

function lineOf(source, index) {
  return source.slice(0, index).split("\n").length;
}

/** Every unrouted site under `root`, by file, in a stable order. */
export function scanTree(root) {
  const counts = new Map();
  const markup = join(root, "editor.html");
  counts.set("editor.html", scanMarkup(readFileSync(markup, "utf8")));
  const sources = readdirSync(join(root, "src"))
    .filter((name) => name.endsWith(".mjs") || name.endsWith(".js"))
    .sort();
  for (const name of sources) {
    const sites = scanScript(readFileSync(join(root, "src", name), "utf8"));
    if (sites.length) counts.set(`src/${name}`, sites);
  }
  return counts;
}

export function totalSites(counts) {
  return [...counts.values()].reduce((sum, sites) => sum + sites.length, 0);
}
