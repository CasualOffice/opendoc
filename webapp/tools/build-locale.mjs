#!/usr/bin/env node
// Generates `webapp/locales/en.json` from the source (docs/124 §3.1).
//
// English is DERIVED, never hand-edited — the rule `webapp/dict/glossary.txt`
// already lives under, for the same reason: an artifact somebody can edit by
// hand drifts from its source, and then the catalogue and the product disagree
// about what the product says. `locale_artifact.test.mjs` fails on a stale
// file, so the drift is a build failure rather than a discovery.
//
// Two sources, because strings are declared in two places:
//   * `editor.html` — `data-i18n*` attributes, whose English is the markup
//     beside them (the text content, or the attribute the key names);
//   * `webapp/src/**` — `t("key")` calls whose English is registered in
//     `EN_STRINGS`, the one place a script-side string is written down.
//
// Three sources since `109` HF-198, because the public SITE went through the
// same seam: the `*.page.html` templates and the shared `_partials/*.html` are
// markup exactly like `editor.html` is, and their keys are extracted by the same
// function. The TEMPLATES and not the generated `*.html` — a generated page
// inlines the partials, so extracting from both would read the same key twice
// and make one edit to a partial look like six. `build-site.py --check`
// guarantees the generated pages are nothing but the templates.
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

/** The named and numeric entities that can appear in this markup, decoded.
 *
 *  The applier writes TEXT, not HTML — it has to, or a translation could
 *  inject markup — so an entity left in the catalogue reaches the screen
 *  literally. "Comments &amp; suggestions" is what the sidebar header showed
 *  the first time this ran. Decoding at EXTRACTION means the catalogue holds
 *  what a person reads and a translator sees a sentence rather than an escape.
 */
function decodeEntities(text) {
  return text
    .replace(/&#x([0-9a-f]+);/gi, (_, hex) => String.fromCodePoint(parseInt(hex, 16)))
    .replace(/&#(\d+);/g, (_, dec) => String.fromCodePoint(Number(dec)))
    .replace(/&times;/g, "\u00d7")
    .replace(/&nbsp;/g, "\u00a0")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&apos;/g, "'")
    // LAST, so a double-escaped "&amp;lt;" does not become "<".
    .replace(/&amp;/g, "&");
}

/** `data-i18n-<suffix>` -> the attribute carrying its English. */
const ATTRIBUTE_KEYS = { title: "title", label: "aria-label", placeholder: "placeholder", alt: "alt" };

/** Every `data-i18n*` in the markup, with the English sitting beside it. */
export function keysFromMarkup(source) {
  const found = new Map();
  for (const tag of source.matchAll(/<([a-zA-Z][\w-]*)\b([^>]*)>/g)) {
    const [whole, , attributes] = tag;
    for (const [suffix, attribute] of Object.entries(ATTRIBUTE_KEYS)) {
      const key = attributes.match(new RegExp(`\\bdata-i18n-${suffix}="([^"]+)"`))?.[1];
      if (!key) continue;
      // `(?<![-\\w])` and not `\\b`: `\\btitle="` also matches INSIDE
      // `data-i18n-title="…"`, because `-` is a word boundary — so the
      // extractor read the KEY as the English and wrote `foo.title` as the
      // translation of `foo.title`.
      const english = attributes.match(new RegExp(`(?<![-\\w])${attribute}="([^"]*)"`))?.[1];
      if (english === undefined) throw new Error(`${key}: data-i18n-${suffix} with no ${attribute}`);
      found.set(key, decodeEntities(english));
    }
    const textKey = attributes.match(/\bdata-i18n="([^"]+)"/)?.[1];
    if (!textKey) continue;
    const after = source.slice(tag.index + whole.length);
    const cut = after.search(/</);
    const english = (cut === -1 ? after : after.slice(0, cut)).trim();
    if (!english) throw new Error(`${textKey}: data-i18n on an element with no text`);
    found.set(textKey, decodeEntities(english));
  }
  return found;
}

/** The script-side catalogue. A `t("key")` whose key is not here is a build
 *  failure, which is what stops a call site from inventing a key that no
 *  translator will ever see. */
export async function scriptStrings() {
  const { EN_STRINGS } = await import(join(WEBAPP, "src/en_strings.mjs"));
  return new Map(Object.entries(EN_STRINGS));
}

/** Comments out, so a `t("key")` written in prose to EXPLAIN the seam is not
 *  mistaken for a call site — which is exactly what `en_strings.mjs`'s own
 *  header did the first time this ran. */
function withoutComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
}

/** Every `t("…")` key actually called in the source. `i18n.mjs` is skipped: it
 *  DEFINES `t`, so its own signature and doc comments are not call sites. */
export function keysCalled(root = join(WEBAPP, "src")) {
  const keys = new Set();
  for (const name of readdirSync(root)) {
    if (name === "i18n.mjs") continue;
    if (!name.endsWith(".mjs") && !name.endsWith(".js")) continue;
    const source = withoutComments(readFileSync(join(root, name), "utf8"));
    for (const call of source.matchAll(/\bt\(\s*["'`]([^"'`]+)["'`]/g)) keys.add(call[1]);
  }
  return keys;
}

/** A key is answered by a catalogue either directly or as a PLURAL FAMILY —
 *  `status.words` is declared by `status.words.one` and `status.words.other`,
 *  and no call site ever names a category, because which categories exist is
 *  the language's business and not the caller's. */
export function isDeclared(key, catalogue) {
  if (key in catalogue) return true;
  const prefix = `${key}.`;
  return Object.keys(catalogue).some((declared) => declared.startsWith(prefix));
}

/** The site's markup sources, in a stable order: every page template, then
 *  every shared partial. Sorted so the catalogue is byte-identical whatever the
 *  filesystem hands back. */
export function siteSources(root = WEBAPP) {
  const pages = readdirSync(root)
    .filter((name) => name.endsWith(".page.html"))
    .sort();
  const partials = readdirSync(join(root, "_partials"))
    .filter((name) => name.endsWith(".html"))
    .sort()
    .map((name) => join("_partials", name));
  return [...pages, ...partials];
}

/** Every key the SITE declares, with the English beside it.
 *
 *  A key may legitimately appear in more than one file — the header partial is
 *  one file, but nothing stops two pages naming `site.docs.import`. What may
 *  NOT happen is the same key carrying two different sentences: the catalogue
 *  holds one value per key, so the second would silently win and one of the two
 *  pages would be translated into the other's words. That is a build failure
 *  here rather than a discovery on a screenshot.
 */
export function keysFromSite(root = WEBAPP) {
  const found = new Map();
  const clashes = [];
  for (const source of siteSources(root)) {
    for (const [key, english] of keysFromMarkup(readFileSync(join(root, source), "utf8"))) {
      const existing = found.get(key);
      if (existing !== undefined && existing !== english) {
        clashes.push(`${key}: ${JSON.stringify(existing)} vs ${JSON.stringify(english)} (${source})`);
      }
      found.set(key, english);
    }
  }
  if (clashes.length) throw new Error(`site keys with two different English strings:\n  ${clashes.join("\n  ")}`);
  return found;
}

export async function buildCatalogue() {
  const markup = new Map([
    ...keysFromMarkup(readFileSync(join(WEBAPP, "editor.html"), "utf8")),
    // `slides.html` is the deck viewer (docs/156 Tier 3). Listed explicitly
    // rather than globbed, like `editor.html` beside it: a glob over `*.html`
    // would also read the GENERATED site pages, and those inline their partials,
    // so one edit to a partial would look like six separate keys.
    ...keysFromMarkup(readFileSync(join(WEBAPP, "slides.html"), "utf8")),
    ...keysFromSite(),
  ]);
  const script = await scriptStrings();
  const clash = [...markup.keys()].filter((key) => script.has(key));
  if (clash.length) throw new Error(`keys declared twice: ${clash.join(", ")}`);
  const merged = [...markup, ...script].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  // The same `@@` header every translated catalogue carries, so English is not
  // a special shape the coverage gate has to make an exception for. English is
  // reviewed by construction: it IS the source.
  return {
    "@@direction": "ltr",
    "@@locale": "en",
    "@@reviewed": true,
    ...Object.fromEntries(merged),
  };
}

export function serialise(catalogue) {
  return `${JSON.stringify(catalogue, null, 2)}\n`;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const catalogue = await buildCatalogue();
  mkdirSync(join(WEBAPP, "locales"), { recursive: true });
  writeFileSync(join(WEBAPP, "locales/en.json"), serialise(catalogue));
  const called = keysCalled();
  const missing = [...called].filter((key) => !isDeclared(key, catalogue));
  process.stdout.write(
    `build-locale: wrote en.json, ${Object.keys(catalogue).length} keys` +
      (missing.length ? `; ${missing.length} called but undeclared: ${missing.join(", ")}` : "") +
      "\n",
  );
  if (missing.length) process.exitCode = 1;
}
