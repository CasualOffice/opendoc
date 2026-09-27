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

/** The byte ranges of `<pre><code>…</code></pre>` blocks.
 *
 *  Source code, not prose. The site pages embed extracted Rust, JavaScript and
 *  shell — `webapp/tools/build-embed-docs.mjs` generates most of it FROM the code
 *  it documents — and `npm pack` or `let mut package = DocxPackage::open(…)` is
 *  not translated into eighteen languages. Left counted, the embedding guide
 *  alone would carry about eighty sites of debt that can never legitimately come
 *  down, which is a ratchet nobody can turn.
 *
 *  This is a STRUCTURAL exemption, not an allowlist entry, and that is the point:
 *  it cannot quietly cover real prose, because prose is not marked up as
 *  preformatted code. `<pre>` alone is not enough — the `<code>` child is
 *  required — so a `<pre>` holding a poem, a transcript or a wrapped paragraph
 *  keeps counting, and `no_unrouted_strings.test.mjs` proves both directions. */
/** The byte ranges of `<script>` and `<style>` BODIES.
 *
 *  `scanMarkup` has always skipped the `<script>` and `<style>` OPENING TAGS by
 *  name, which is not the same thing, and the difference is a real defect: the
 *  scanner walks tag by tag and captures the text after each tag up to the next
 *  `<`, so a script body containing anything tag-SHAPED is read as markup. The
 *  embedding guide's own description says "…the `<opendoc-editor>` custom
 *  element…", and putting that sentence inside an `ld+json` block made the scanner
 *  match `<opendoc-editor>` as an element and count the rest of the JSON line as
 *  translatable prose:
 *
 *      { kind: "text", element: "opendoc-editor",
 *        text: "custom element, and a capability contract." }
 *
 *  The ratchet itself found it, one site over a ceiling. It is also a reminder that
 *  a measurement taken on a case which does not exercise the bug proves nothing: a
 *  first check of "does the scanner read `ld+json` bodies?" used a block with no
 *  tag-shaped text in it and answered, wrongly, no.
 *
 *  A script or style body is code by definition — there is no HTML element in there
 *  to route, and the text is JavaScript, CSS or JSON. `editor.html`'s inline
 *  scripts fall under this too, and `no_unrouted_strings.test.mjs` publishes how
 *  many sites it suppresses per file, so it cannot become a hiding place. */
function scriptBodyRanges(source) {
  const ranges = [];
  for (const open of source.matchAll(/<(script|style)\b[^>]*>/gi)) {
    const bodyStart = open.index + open[0].length;
    const close = source.toLowerCase().indexOf(`</${open[1].toLowerCase()}>`, bodyStart);
    if (close === -1) continue;
    ranges.push([bodyStart, close]);
  }
  return ranges;
}

function codeBlockRanges(source) {
  const ranges = [];
  for (const open of source.matchAll(/<pre\b[^>]*>/g)) {
    const bodyStart = open.index + open[0].length;
    if (!/^\s*<code\b/.test(source.slice(bodyStart, bodyStart + 40))) continue;
    const close = source.indexOf("</pre>", bodyStart);
    if (close === -1) continue;
    // From the END of the `<pre …>` tag, so the `<pre>`'s own attributes are still
    // scanned: `<pre title="Shell commands">` carries a caption a person reads,
    // and only the code between the tags is exempt.
    ranges.push([bodyStart, close + "</pre>".length]);
  }
  return ranges;
}

/** Markup sites: a human-readable attribute, or a text node, with no
 *  `data-i18n*` on the element that would route it through the seam.
 *
 *  `exemptCode: false` turns OFF the `<pre><code>` and script/style-body
 *  exemptions, so a guard can measure exactly how many sites they suppress by
 *  scanning the same source both ways. It exists for that measurement and for
 *  nothing else: a caller that wants the count uses the default. */
export function scanMarkup(source, { exemptCode = true } = {}) {
  const sites = [];
  const skip = exemptCode
    ? [...codeBlockRanges(source), ...scriptBodyRanges(source)]
    : [];
  const inCode = (index) => skip.some(([from, to]) => index >= from && index < to);
  const tags = [...source.matchAll(/<([a-zA-Z][\w-]*)\b([^>]*)>/g)];
  for (const tag of tags) {
    const [whole, name, attributes] = tag;
    if (name === "script" || name === "style") continue;
    // Inside a `<pre><code>` block, or inside a script/style body. Both are code,
    // and in both cases it is the CONTENT that has to be skipped rather than the
    // element: the highlighting spans in a code panel carry the code's own text,
    // and a script body containing tag-shaped text is otherwise read as markup.
    if (inCode(tag.index)) continue;
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

/** Every unrouted site under `root`, by file, in a stable order.
 *
 *  Markup: `editor.html`, plus the SITE — every `*.page.html` template and every
 *  shared partial under `_partials/`. Those were invisible to this scanner until
 *  `109` HF-190: it read `editor.html` and `src/*.{js,mjs}` and nothing else, so
 *  the public pages carried about five hundred unrouted strings against an
 *  `editor.html` ceiling of sixteen and no gate could tell.
 *
 *  The TEMPLATES, deliberately, not the generated `*.html`. Each generated page
 *  inlines the header and footer partials, so scanning both would charge the same
 *  shared strings once per page and a single edit to a partial would move four
 *  ceilings at once. A template plus a partial is counted exactly where it is
 *  authored, which is also where it has to be fixed. `build-site.py --check`
 *  already guarantees the generated pages are nothing but the templates. */
export function scanTree(root) {
  const counts = new Map();
  counts.set("editor.html", scanMarkup(readFileSync(join(root, "editor.html"), "utf8")));
  for (const name of readdirSync(root)
    .filter((entry) => entry.endsWith(".page.html"))
    .sort()) {
    counts.set(name, scanMarkup(readFileSync(join(root, name), "utf8")));
  }
  for (const name of readdirSync(join(root, "_partials"))
    .filter((entry) => entry.endsWith(".html"))
    .sort()) {
    const sites = scanMarkup(readFileSync(join(root, "_partials", name), "utf8"));
    if (sites.length) counts.set(`_partials/${name}`, sites);
  }
  const sources = readdirSync(join(root, "src"))
    .filter((name) => name.endsWith(".mjs") || name.endsWith(".js"))
    .sort();
  for (const name of sources) {
    const sites = scanScript(readFileSync(join(root, "src", name), "utf8"));
    if (sites.length) counts.set(`src/${name}`, sites);
  }
  return counts;
}

/** How many sites the code exemptions suppress in a source.
 *
 *  Measured by scanning the same source with the exemptions on and off, which is
 *  the only way to get it right — see `scanMarkup`'s `exemptCode` note. A guard
 *  pins this per file, so wrapping real prose in `<pre><code>` or hiding it in a
 *  script body moves a published number instead of moving nothing. */
export function exemptedSites(source) {
  return scanMarkup(source, { exemptCode: false }).length - scanMarkup(source).length;
}

export function totalSites(counts) {
  return [...counts.values()].reduce((sum, sites) => sum + sites.length, 0);
}
