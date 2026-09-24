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
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

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
      found.set(key, english);
    }
    const textKey = attributes.match(/\bdata-i18n="([^"]+)"/)?.[1];
    if (!textKey) continue;
    const after = source.slice(tag.index + whole.length);
    const cut = after.search(/</);
    const english = (cut === -1 ? after : after.slice(0, cut)).trim();
    if (!english) throw new Error(`${textKey}: data-i18n on an element with no text`);
    found.set(textKey, english);
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

export async function buildCatalogue() {
  const markup = keysFromMarkup(readFileSync(join(WEBAPP, "editor.html"), "utf8"));
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
