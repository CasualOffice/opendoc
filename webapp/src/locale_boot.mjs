// Choosing a locale, loading it, and keeping the picker honest (docs/124 §5).
//
// The seam (`i18n.mjs`) answers what a locale says and the applier
// (`localize.mjs`) puts it on the page; this is the part that decides WHICH
// locale, fetches it, and survives the fetch failing. It takes its
// collaborators rather than reaching for them so the whole sequence can be
// driven without a document, a settings store or a network.
import { setCatalogue, setLocale, t } from "./i18n.mjs";
import { applyDocumentLocale, fetchCatalogue, localizeTree } from "./localize.mjs";
import { LOCALES, bestLocale } from "./locales.mjs";
import { EN_STRINGS } from "./en_strings.mjs";

/** Locales whose FULL catalogue — the keys the markup declares, not just the
 *  script strings compiled in — has been loaded. */
const loaded = new Set();

/**
 * The locale the user asked for, in precedence order: an explicit `?lang=`
 * (so a bug report can name one), then their saved choice, then what the
 * browser prefers.
 */
export function preferredLocale({ search, saved, browser }) {
  const requested = new URLSearchParams(search ?? "").get("lang");
  return bestLocale([requested, saved, ...(browser ?? [])].filter(Boolean));
}

/**
 * Switches the editor's language, fetching the catalogue if it is not already
 * here. Returns the locale actually in force — English when a catalogue cannot
 * be fetched, because an editor in the wrong language beats one that failed to
 * open.
 */
export async function useLocale(tag, { onLocalised, fetcher = fetchCatalogue } = {}) {
  if (!loaded.has(tag)) {
    const entries = await fetcher(tag);
    if (entries) {
      // English MERGES: `EN_STRINGS` is compiled in so no script-side string
      // ever waits on a request, and the generated catalogue adds the keys the
      // markup declares. Every other locale is whole in its own file.
      setCatalogue(tag, tag === "en" ? { ...EN_STRINGS, ...entries } : entries);
      loaded.add(tag);
    } else if (tag !== "en") {
      return useLocale("en", { onLocalised, fetcher });
    }
  }
  setLocale(tag);
  applyDocumentLocale();
  // Only relabel from a catalogue that actually has the markup's keys. Without
  // this, a failed request would replace every label in the editor with a
  // dotted key — and English's fallback is already sitting in the markup.
  if (loaded.has(tag)) localizeTree();
  onLocalised?.(tag);
  return tag;
}

/**
 * Fills the language picker. Every language names itself, because a picker
 * listing languages in a language you cannot read is a picker you cannot use
 * to escape — and the automatic entry says WHICH language it would pick,
 * because "System default" alone tells the user nothing about what they are
 * choosing.
 */
export function buildLanguagePicker(select, { saved, browser } = {}) {
  if (!select) return;
  const automatic = document.createElement("option");
  automatic.value = "";
  const preferred = LOCALES.find((locale) => locale.tag === bestLocale(browser ?? []));
  automatic.textContent = t("settings.language.systemDefault", {
    name: preferred?.endonym ?? "English",
  });
  select.replaceChildren(automatic);
  for (const locale of LOCALES) {
    const option = document.createElement("option");
    option.value = locale.tag;
    option.textContent = locale.endonym;
    option.lang = locale.tag;
    select.appendChild(option);
  }
  select.value = saved ?? "";
}

/**
 * Wires the whole locale lifecycle to a page: register English, pick a locale,
 * fetch and apply it, fill the picker, and keep both in step when the user
 * chooses a different language.
 *
 * It lives here rather than in the shell because every line of it is about
 * locales, and because the sequence has an order that matters — the picker is
 * built BEFORE the catalogue lands so the dialog is never empty, and rebuilt
 * after so its automatic entry is named in the language now in force.
 */
export function startLocalisation({ select, settings, saveSettings, onLocalised }) {
  setCatalogue("en", EN_STRINGS);
  const sources = () => ({
    search: location.search,
    saved: settings.language,
    browser: navigator.languages ?? [],
  });
  const apply = (tag) => useLocale(tag, { onLocalised });
  select?.addEventListener("change", async () => {
    settings.language = select.value;
    saveSettings();
    await apply(preferredLocale(sources()));
    buildLanguagePicker(select, sources());
  });
  buildLanguagePicker(select, sources());
  return apply(preferredLocale(sources())).then((tag) => {
    buildLanguagePicker(select, sources());
    return tag;
  });
}

/** Test seam: forget which catalogues have been fetched. */
export function resetLoadedCatalogues() {
  loaded.clear();
}
