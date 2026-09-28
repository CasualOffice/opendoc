// The public site's half of the localisation seam (docs/124; `109` HF-198).
//
// THE EDITOR WAS LOCALISED AND THE SITE WAS NOT. Every string in `editor.html`
// resolves through `t()`/`data-i18n` and nineteen catalogues; the marketing and
// documentation pages carried about five hundred English literals with nowhere
// to route them, which `no_unrouted_strings.test.mjs` had to exempt as
// "SEAMLESS" — declared measurements instead of ratchets, because deleting
// English was the only way to lower one. This module is the seam that exemption
// was waiting for, and it is deliberately NOT a second mechanism:
//
//   * the markup declares its strings with the same `data-i18n`,
//     `data-i18n-title`, `data-i18n-label` and `data-i18n-placeholder`
//     attributes the editor's chrome uses, with the English beside the key;
//   * `tools/build-locale.mjs` extracts them into the same `locales/en.json`
//     the editor is built from, so a site string and a ribbon string are one
//     kind of thing to a translator;
//   * `i18n.mjs` resolves them and `localize.mjs` applies them, unchanged.
//
// WHAT THIS IS INSTEAD OF, and why. Three shapes were available.
//
//   1. A page per locale (`/de/index.html`, …). It is the best thing for a
//      crawler, and it multiplies the site by nineteen: 5 templates + 12
//      reference pages become 323 files, every one of which has to be rebuilt
//      and re-checked whenever a sentence changes, and `build-seo.mjs` has to
//      grow `hreflang` for all of them. That is a real option for the day the
//      translations exist and matter for search; it is a lot of machinery to
//      carry for pages that are, today, entirely untranslated.
//   2. Resolving at build time into one page. That is just (1) with the choice
//      taken away from the reader.
//   3. This: English in the served HTML, and a swap at runtime for a reader who
//      wants another language.
//
// What (3) costs, exactly: ZERO extra bytes and ZERO extra requests for an
// English reader — the page they are served is the page they read, including
// with scripts off — plus this module (about 4 KB) and, only when a different
// language is actually chosen or detected, one `locales/<tag>.json` fetch. What
// it costs in return is that a crawler sees English; the canonical page is the
// English one and `hreflang` is not claimed, which is honest rather than
// convenient. Nothing here forecloses (1): the catalogue is the same artifact a
// per-locale build would consume.

import { setCatalogue, setLocale, t } from "./i18n.mjs";
import { applyDocumentLocale, fetchCatalogue, localizeTree } from "./localize.mjs";
import { LOCALES, bestLocale } from "./locales.mjs";
import { loadPrefObject, savePrefObject } from "./prefs.mjs";

/** The editor's settings object, shared on purpose.
 *
 *  A reader who sets the site to Japanese and then opens the editor expects the
 *  editor to be in Japanese. One key, one choice, both directions — the site is
 *  not a different product from the thing it is advertising. */
const SETTINGS_KEY = "opendoc.settings";

/** `locales/` as an absolute URL, derived from this module's own location.
 *
 *  The footer partial is inlined into pages at the site root AND into the
 *  generated `reference/*.html`, which sit one directory down; a relative
 *  `./locales` would resolve against the page and 404 for half the site. */
const CATALOGUES = new URL("../locales", import.meta.url).href;

/** Catalogues already fetched, by tag, so switching back and forth is free. */
const loaded = new Set([/* English is the markup; it is never fetched */ "en"]);

/**
 * The locale to show, in precedence order: an explicit `?lang=` (so a bug
 * report or a screenshot can name one), then the saved choice, then what the
 * browser asks for.
 *
 * Exported and pure so the precedence is testable without a page.
 */
export function preferredLocale({ search, saved, browser } = {}) {
  const requested = new URLSearchParams(search ?? "").get("lang");
  return bestLocale([requested, saved, ...(browser ?? [])].filter(Boolean));
}

/**
 * Puts `tag` in force: fetch its catalogue if needed, then relabel the page.
 *
 * Returns the locale actually in force, which is English when a catalogue
 * cannot be fetched — a page in the wrong language beats a page of dotted key
 * names, and the English is sitting in the markup for exactly this case.
 *
 * Complexity: O(elements carrying a key). Nothing here is per-word.
 */
export async function useLocale(tag, { fetcher = fetchCatalogue } = {}) {
  if (!loaded.has(tag)) {
    const entries = await fetcher(tag, CATALOGUES);
    if (!entries) return useLocale("en", { fetcher });
    setCatalogue(tag, entries);
    loaded.add(tag);
  }
  setLocale(tag);
  applyDocumentLocale();
  // English has no catalogue here, so `has()` answers false for every key and
  // `localizeTree` leaves the markup exactly as authored. That is the whole
  // reason the English text stays beside the key.
  localizeTree();
  return tag;
}

/**
 * Fills the language control and returns it, or null when the page has none.
 *
 * Every language names itself. A picker that lists "German" is unusable by the
 * person who most needs it, which is why `locales.mjs` holds endonyms and why
 * they are appended here rather than translated: a proper noun has no
 * translation, and putting nineteen of them in nineteen catalogues would give
 * three hundred and sixty-one chances to get one wrong.
 *
 * The automatic entry says WHICH language it would pick, because "Automatic"
 * alone tells a reader nothing about what they are choosing.
 */
export function buildLanguagePicker(select, { saved, browser } = {}) {
  if (!select) return null;
  const automatic = select.querySelector('option[value=""]');
  for (const option of [...select.options]) if (option !== automatic) option.remove();
  if (automatic) {
    const preferred = LOCALES.find((locale) => locale.tag === bestLocale(browser ?? []));
    // `localizeTree` has already put this option's own words in the active
    // language; the endonym is appended after, as a proper noun.
    const words = t("site.header.languageAutomatic");
    automatic.textContent = preferred ? words + " \u2014 " + preferred.endonym : words;
  }
  for (const locale of LOCALES) {
    const option = document.createElement("option");
    option.value = locale.tag;
    option.textContent = locale.endonym;
    option.lang = locale.tag;
    select.appendChild(option);
  }
  select.value = saved ?? "";
  select.closest(".site-lang")?.removeAttribute("hidden");
  return select;
}

/**
 * Wires the whole thing to a page: pick a locale, apply it, fill the picker,
 * and remember what the reader chooses.
 *
 * Takes its collaborators so the sequence can be driven in a test without a
 * browser, a network or a storage backend.
 */
export async function startSiteLocalisation({
  select,
  view = globalThis,
  search = view.location?.search ?? "",
  browser = view.navigator?.languages ?? [],
  fetcher = fetchCatalogue,
} = {}) {
  const settings = loadPrefObject(SETTINGS_KEY, { language: "" }, view);
  const sources = () => ({ search, saved: settings.language, browser });
  const inForce = await useLocale(preferredLocale(sources()), { fetcher });
  buildLanguagePicker(select, sources());
  select?.addEventListener("change", async () => {
    settings.language = select.value;
    savePrefObject(SETTINGS_KEY, settings, view);
    // The CHOICE outranks the URL from here on. Re-deriving with `?lang=` still
    // in hand would put the URL's locale straight back and the picker would
    // appear to do nothing — the same defect the editor's picker had.
    await useLocale(select.value || preferredLocale({ ...sources(), search: "" }), { fetcher });
    buildLanguagePicker(select, sources());
  });
  return inForce;
}

/** Test seam: forget which catalogues have been fetched. */
export function resetSiteLocales() {
  loaded.clear();
  loaded.add("en");
}

// Auto-start only in a real page. Importing this module from a test must not
// reach for a document.
if (typeof document !== "undefined" && document.getElementById) {
  const start = () =>
    startSiteLocalisation({ select: document.getElementById("siteLanguage") }).catch(() => {
      /* A page stuck in English is a page. Nothing here may stop it rendering. */
    });
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
}
