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

/** The HOST's overrides, by exact tag, with `"*"` for every locale.
 *
 *  `docs/126` phase 3 asks for host string overrides and says they must "layer
 *  on the existing seam rather than introducing a second one". This is that
 *  layer, and it is deliberately the SAME shape as a catalogue — flat
 *  `key → string`, plural families as `key.category` — resolved by the same two
 *  functions below. So a host renaming the product is not a different mechanism
 *  from translating the product; it is the same lookup asked one layer earlier,
 *  and every surface that was already localised is already overridable.
 *
 *  WHY A LAYER AND NOT A CATALOGUE. Registering a host's strings through
 *  `setCatalogue` would work for one locale and then fight the fallback chain:
 *  a host override written for `de` would be shadowed by our own `de` when the
 *  chain reached it first, and an override meant for every locale has no tag to
 *  live under at all. It would also make a host's product name look like a
 *  translation, so `locale_coverage.test.mjs`'s orphan gate — no catalogue may
 *  carry a key English lacks — would be refusing the host's own words. A
 *  separate layer consulted FIRST answers both: `"*"` is "in every language",
 *  and an exact tag narrows it.
 *
 *  Resolution order is `"*"`-then-chain per tag, so a host that overrode one
 *  locale keeps their generic override everywhere else, and an override never
 *  falls back into OUR string for a locale the host did answer. */
const overrides = new Map();

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

/** Every-locale overrides live under this tag. Not a valid BCP-47 tag by
 *  construction, so it can never collide with a locale a host also overrode. */
export const EVERY_LOCALE = "*";

/**
 * Installs a host's string overrides, replacing any previous set.
 *
 * `{ "*": {key: string}, "de": {…} }`. Called once at boot by the loader, and
 * directly by tests. `null` clears them, which is what a host that stops
 * white-labelling gets rather than a stale product name.
 *
 * Complexity: O(locales the host overrode). Nothing here walks a document, and
 * the maps are read by reference rather than copied per lookup.
 *
 * @param {Record<string, Record<string, string>>|null} byLocale
 */
export function setOverrides(byLocale) {
  overrides.clear();
  for (const [tag, entries] of Object.entries(byLocale ?? {})) {
    if (entries && typeof entries === "object") overrides.set(tag, entries);
  }
}

/** Whether any host override is installed — for a caller that wants to say so
 *  rather than guess, and for a guard that must prove the condition it created. */
export function hasOverrides() {
  return overrides.size > 0;
}

/** The order host overrides are consulted in.
 *
 *  `fallbackChain` with `"*"` inserted immediately BEFORE the English fallback.
 *  The reasoning is that `"*"` means "in every language", so it must outrank a
 *  FALLBACK language — a German reader gets the host's every-language word, not
 *  the host's English one — while the reader's OWN language still outranks it,
 *  because a host who wrote a German override meant it for German readers.
 *  For an English reader `fallbackChain` is just `["en"]`, so `"*"` lands last
 *  and their own language wins, which is the same rule. */
function overrideChain() {
  const chain = fallbackChain(active);
  const tail = chain.lastIndexOf(FALLBACK_LOCALE);
  if (tail <= 0) return [...chain, EVERY_LOCALE];
  return [...chain.slice(0, tail), EVERY_LOCALE, ...chain.slice(tail)];
}

/** The first catalogue in the chain that has `key`, or null.
 *
 *  Host overrides are consulted before our own catalogues, entirely — not
 *  interleaved per tag. A host that renamed the product renamed it in every
 *  language we ship, and an override that lost to one of the nineteen
 *  catalogues would white-label the editor in eighteen languages and not the
 *  nineteenth, which is the kind of almost-working a host discovers from a
 *  screenshot. */
function lookup(key) {
  for (const tag of overrideChain()) {
    const entry = overrides.get(tag)?.[key];
    if (typeof entry === "string") return entry;
  }
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
  // Host overrides first, and by the same one-layer-at-a-time rule: a host that
  // supplied only `other` for their own word answers in their own words rather
  // than losing `one` to ours, which would read as two different products in
  // one sentence.
  for (const tag of overrideChain()) {
    const entries = overrides.get(tag);
    if (!entries) continue;
    const exact = entries[`${key}.${category}`];
    if (typeof exact === "string") return exact;
    const other = entries[`${key}.other`];
    if (typeof other === "string") return other;
  }
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

/** Test seam: forget every catalogue and every host override, and go back to
 *  English. Overrides are cleared here too: a test that installed a white-label
 *  and did not clear it would white-label every test after it, and a shared
 *  module's leaked state is the hardest kind of green-for-the-wrong-reason. */
export function resetI18n() {
  catalogues.clear();
  overrides.clear();
  active = FALLBACK_LOCALE;
}
