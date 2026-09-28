// Choosing a locale, loading it, and keeping the picker honest (docs/124 §5).
//
// The seam (`i18n.mjs`) answers what a locale says and the applier
// (`localize.mjs`) puts it on the page; this is the part that decides WHICH
// locale, fetches it, and survives the fetch failing. It takes its
// collaborators rather than reaching for them so the whole sequence can be
// driven without a document, a settings store or a network.
import { setCatalogue, setLocale, t } from "./i18n.mjs";
import { hasOverrides, setOverrides } from "./i18n.mjs";
import { BRAND_STRINGS } from "./brand.mjs";
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
 * Fills the FOOTER picker — the same list, in the shape the status bar's other
 * control (zoom) already uses.
 *
 * It exists because the alternative was worse than it looked. This control used
 * to open the Settings dialog, which is the one thing the status bar's own
 * rationale argues against: a person who cannot read the interface had to
 * navigate an interface they cannot read in order to escape it. Word and Google
 * Docs both put language here because it is status; status that answers with a
 * modal in the wrong language is not status, it is a redirect.
 *
 * Rebuilt on every open rather than once, so the automatic row names the
 * language it would pick in the language now in force. Twenty rows: O(1) in the
 * size of the document, which is what a per-interaction budget asks of a click.
 *
 * The tick marks the SAVED CHOICE, which is what the dialog's `<select>` shows
 * too — one setting with one answer, rather than two surfaces that can be read
 * against each other. So with nothing saved the automatic row is the ticked one,
 * and it names the language it resolves to; the language actually in force is
 * never in doubt because the control the list hangs off is already spelling it
 * out in its own script.
 */
export function buildLanguageMenu(list, { saved, browser, choose } = {}) {
  if (!list) return;
  const row = (tag, text) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "menu-item language-option";
    button.setAttribute("role", "menuitemradio");
    button.setAttribute("aria-checked", String((saved ?? "") === tag));
    button.dataset.lang = tag;
    const check = document.createElement("span");
    check.className = "menu-check";
    check.setAttribute("aria-hidden", "true");
    const label = document.createElement("span");
    label.className = "menu-item-label";
    label.textContent = text;
    // The endonym is written in its own language, so the row says so. Without
    // `lang` the browser picks a fallback font from the INTERFACE's language,
    // and a reader hunting for their own script can be handed tofu or the wrong
    // Han variant — precisely the row they have to be able to recognise.
    if (tag) label.lang = tag;
    button.append(check, label);
    button.addEventListener("click", () => choose?.(tag));
    return button;
  };
  const preferred = LOCALES.find((locale) => locale.tag === bestLocale(browser ?? []));
  list.replaceChildren(
    row("", t("settings.language.systemDefault", { name: preferred?.endonym ?? "English" })),
    ...LOCALES.map((locale) => row(locale.tag, locale.endonym)),
  );
}

/** Arrow keys walk a menu; that is what `role="menu"` promises, and with twenty
 *  rows Tab alone is not an answer. Home and End go to the ends, and the ends
 *  wrap, because a list this long is circular to anyone using it in earnest. */
function menuArrowKeys(list) {
  list?.addEventListener("keydown", (event) => {
    const step = { ArrowDown: 1, ArrowUp: -1, Home: 0, End: 0 }[event.key];
    if (step === undefined) return;
    const rows = [...list.querySelectorAll(".language-option")];
    if (rows.length === 0) return;
    const at = rows.indexOf(document.activeElement);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? rows.length - 1
          : (at + step + rows.length) % rows.length;
    event.preventDefault();
    rows[next]?.focus();
  });
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
export function startLocalisation({
  select,
  settings,
  saveSettings,
  onLocalised,
  registerPopover,
}) {
  // THE HOST'S WORDS GO IN FIRST, and synchronously.
  //
  // `docs/126` phase 3 asks for string overrides that "layer on the existing
  // seam rather than introducing a second one", which is what `setOverrides` is:
  // the same lookup the nineteen catalogues resolve through, consulted one layer
  // earlier. Installed here rather than in `main.js` because this is the module
  // that owns what the editor reads, and because `main.js` had no lines.
  //
  // BEFORE the English catalogue and before the first `localizeTree`, so a
  // white-labelled build never paints our product name and then replaces it. That
  // ordering is the whole difference between a white-label and a flash of someone
  // else's brand: `has()` answers for an override, so the sweep below relabels
  // the markup the moment it runs, without waiting for any request.
  setOverrides(BRAND_STRINGS);
  setCatalogue("en", EN_STRINGS);
  if (hasOverrides()) localizeTree();
  const sources = () => ({
    search: location.search,
    saved: settings.language,
    browser: navigator.languages ?? [],
  });
  /** The status bar names the language in ITSELF, so a reader who cannot read
   *  the rest of the chrome can still see which language they are in and get
   *  out of it. */
  const nameInStatus = (tag) => {
    const label = document.getElementById("languageStatusLabel");
    if (!label) return;
    const locale = LOCALES.find((candidate) => candidate.tag === tag);
    label.textContent = locale?.endonym ?? tag;
    label.lang = tag;
  };
  const apply = (tag) =>
    useLocale(tag, { onLocalised }).then((inForce) => {
      nameInStatus(inForce);
      return inForce;
    });
  /** ONE language setting, whichever surface chose it (docs/124 §5). Both the
   *  dialog's `<select>` and the footer's menu end here, and both are rebuilt
   *  afterwards, so a change made on one is visible on the other. */
  const choose = async (value) => {
    settings.language = value;
    saveSettings();
    // The CHOICE wins, not the URL. `?lang=` is there so a bug report can name
    // a locale, and on load it should; but once a person has picked one from
    // the control in front of them, re-deriving the answer put the URL's locale
    // straight back and the picker appeared to do nothing.
    await apply(value || preferredLocale({ ...sources(), search: "" }));
    buildLanguagePicker(select, sources());
  };
  select?.addEventListener("change", () => choose(select.value));
  // The footer control shows the LIST, in place, on the same manager the zoom
  // presets use — so it dismisses, restores focus and stays mutually exclusive
  // with every other popover by one implementation rather than two. It is
  // CHROME, not a document command: `needsDocument: false` keeps it live while
  // the engine is still loading, which is exactly when someone stuck in a
  // language they cannot read wants it.
  const menuList = document.getElementById("languageMenuList");
  const languagePopover = registerPopover?.(
    document.getElementById("languageStatus"),
    document.getElementById("languageMenu"),
    () => buildLanguageMenu(menuList, { ...sources(), choose: pick }),
    { needsDocument: false },
  );
  /** Choosing from the footer closes the menu first — through the manager, so
   *  `aria-expanded` and the keyboard's place go back with it — and only then
   *  relabels, because relabelling rebuilds the rows under the pointer. */
  function pick(value) {
    languagePopover?.close();
    void choose(value);
  }
  menuArrowKeys(menuList);
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
