// The localisation seam (docs/124; `109` HF-081).
//
// Every user-facing string in the editor resolves through here, and nothing in
// this module touches the DOM — so a unit test, a host page or a non-browser
// runtime can ask what a locale says without a document existing. The DOM side
// (walking `data-i18n` attributes) is a separate, thin applier; keeping the
// lookup pure is what lets the plural and fallback rules be tested directly
// rather than through a rendered page.
//
// No dependency. `Intl` is in every browser the project supports and it is the
// only thing that knows that Russian has three plural forms and Arabic six —
// which is what the hand-rolled `count === 1 ? "word" : "words"` ternaries
// scattered through `main.js` get wrong for most of the world.

/** The locale every other one falls back to, and the only one whose catalogue
 *  is generated from the source rather than translated from it. */
export const FALLBACK_LOCALE = "en";

/** Scripts written right to left. The UI mirrors for these; a DOCUMENT never
 *  does on their account — a left-to-right `.docx` opened by an Arabic-speaking
 *  user is still left-to-right, and that distinction is structural here rather
 *  than left to whoever writes the next stylesheet (docs/124 §3.5). */
const RTL_LANGUAGES = new Set(["ar", "fa", "he", "ps", "sd", "ug", "ur", "yi"]);

/** Loaded catalogues, by exact tag. */
const catalogues = new Map();

let active = FALLBACK_LOCALE;

/** `pt-BR` → `["pt-BR", "pt", "en"]`. Region-specific first, then the base
 *  language, then English: a Brazilian user with an incomplete `pt-BR` gets
 *  Portuguese rather than English, which is the useful degradation. */
export function fallbackChain(tag) {
  const chain = [];
  let current = String(tag || "").trim();
  while (current) {
    if (!chain.includes(current)) chain.push(current);
    const cut = current.lastIndexOf("-");
    if (cut < 1) break;
    current = current.slice(0, cut);
  }
  if (!chain.includes(FALLBACK_LOCALE)) chain.push(FALLBACK_LOCALE);
  return chain;
}

/** `"rtl"` or `"ltr"` for a tag, by its LANGUAGE subtag — `ar-EG` is as
 *  right-to-left as `ar`. */
export function direction(tag) {
  const language = String(tag || "")
    .split("-")[0]
    .toLowerCase();
  return RTL_LANGUAGES.has(language) ? "rtl" : "ltr";
}

/** Registers a catalogue. Called by the loader once a locale's JSON arrives,
 *  and directly by tests. */
export function setCatalogue(tag, entries) {
  catalogues.set(tag, entries ?? {});
}

export function hasCatalogue(tag) {
  return catalogues.has(tag);
}

/** Switches the active locale. Registering the catalogue is the caller's job,
 *  so that a locale can be made active the instant its JSON lands without this
 *  module knowing how it was fetched. */
export function setLocale(tag) {
  active = String(tag || FALLBACK_LOCALE).trim() || FALLBACK_LOCALE;
  return active;
}

export function activeLocale() {
  return active;
}

/** The first catalogue in the chain that has `key`, or null. */
function lookup(key) {
  for (const tag of fallbackChain(active)) {
    const entry = catalogues.get(tag)?.[key];
    if (typeof entry === "string") return entry;
  }
  return null;
}

/** A plural family, resolved ONE CATALOGUE AT A TIME.
 *
 *  The order matters and is easy to get backwards. Walking the categories
 *  across the whole chain first — `key.one` in every locale, then `key.other`
 *  in every locale — means a language that carries only `other` loses to
 *  English's `one`, and a Swahili user reads "1 word" while every other count
 *  reads Swahili. So each catalogue is asked for its own exact category and
 *  its own `other` before the next one is consulted: a locale that can answer
 *  at all answers in its own language. */
function lookupPlural(key, category) {
  for (const tag of fallbackChain(active)) {
    const entries = catalogues.get(tag);
    if (!entries) continue;
    const exact = entries[`${key}.${category}`];
    if (typeof exact === "string") return exact;
    const other = entries[`${key}.other`];
    if (typeof other === "string") return other;
  }
  return null;
}

/** Placeholders are `{named}`. A name with no value is left ALONE rather than
 *  replaced with "undefined": a visible `{count}` in the UI is a bug report,
 *  and `undefined` is a bug that ships. */
function interpolate(text, params) {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name) =>
    Object.hasOwn(params, name) ? String(params[name]) : whole,
  );
}

/**
 * The string for `key` in the active locale.
 *
 * With a `count` param the key is treated as a plural family: the catalogue
 * carries `key.one`, `key.other` and whichever other categories the language
 * needs, and `Intl.PluralRules` picks. A language that needs `few` and `many`
 * gets them without the call site knowing they exist, which is the whole point
 * — no call site can be written correctly for every language.
 *
 * An unknown key returns the key itself. That is deliberately ugly: it is
 * visible in a screenshot, and `no_unrouted_strings.test.mjs` plus
 * `locale_coverage.test.mjs` are what stop it reaching one.
 */
export function t(key, params) {
  let values = params;
  if (params && typeof params.count === "number") {
    // `{count}` is displayed as well as counted, and it is displayed to a
    // person — so it is formatted in their locale (`1,024` / `1 024` /
    // `١٬٠٢٤`) while SELECTION still runs on the raw number. Every call site
    // would otherwise have to remember to format it, and the one that forgot
    // is how the status bar came to print browser-locale numbers inside
    // editor-locale sentences.
    values = { ...params, count: n(params.count) };
    const category = new Intl.PluralRules(active).select(params.count);
    const plural = lookupPlural(key, category);
    if (plural !== null) return interpolate(plural, values);
  }
  const entry = lookup(key);
  return entry === null ? key : interpolate(entry, values);
}

/** True when the active chain can answer `key` — for a caller that wants to
 *  fall back to its own English literal rather than print a key. */
export function has(key) {
  return lookup(key) !== null;
}

/** A number in the active locale: `1,024` in `en`, `1 024` in `fr`,
 *  `١٬٠٢٤` in `ar`. The status bar's counts go through here. */
export function n(value, options) {
  return new Intl.NumberFormat(active, options).format(value);
}

/** A date in the active locale. */
export function d(value, options = { dateStyle: "medium" }) {
  return new Intl.DateTimeFormat(active, options).format(value);
}

/** "A, B and C" — and the languages that punctuate it differently. */
export function list(items, options = { style: "long", type: "conjunction" }) {
  return new Intl.ListFormat(active, options).format(items);
}

/** Test seam: forget every catalogue and go back to English. */
export function resetI18n() {
  catalogues.clear();
  active = FALLBACK_LOCALE;
}
