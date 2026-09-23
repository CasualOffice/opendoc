// The DOM half of the localisation seam (docs/124 §3.3).
//
// `i18n.mjs` answers "what does this locale say"; this applies the answer to a
// document. They are apart because the answer is the part with rules worth
// testing — plural categories, the fallback chain — and a rule tested through
// a rendered page is a rule tested badly.
//
// Markup declares its own strings: `data-i18n` for text, `data-i18n-title`,
// `data-i18n-label` (for `aria-label`) and `data-i18n-placeholder` for the
// attributes a person reads. The ENGLISH text stays in the markup beside them,
// as the extractor's source and as the fallback, so the file stays readable
// and a catalogue that fails to load degrades to English rather than to a
// screen of key names.
import { activeLocale, direction, t } from "./i18n.mjs";

/** Attribute suffix -> the DOM attribute it sets. */
const ATTRIBUTE_KEYS = Object.freeze({
  title: "title",
  label: "aria-label",
  placeholder: "placeholder",
  alt: "alt",
});

/** Relabels `root` (default: the whole document) in the active locale. Safe to
 *  run repeatedly — it is how a locale change is applied. */
export function localizeTree(root = document) {
  for (const element of root.querySelectorAll("[data-i18n]")) {
    element.textContent = t(element.dataset.i18n);
  }
  for (const [suffix, attribute] of Object.entries(ATTRIBUTE_KEYS)) {
    for (const element of root.querySelectorAll(`[data-i18n-${suffix}]`)) {
      element.setAttribute(attribute, t(element.dataset[`i18n${cap(suffix)}`]));
    }
  }
}

function cap(value) {
  return value[0].toUpperCase() + value.slice(1);
}

/** Puts the active locale on the document element, which is what a screen
 *  reader's pronunciation, the browser's own spell checker and every `:lang()`
 *  rule read. The chrome mirrors for a right-to-left locale; the DOCUMENT does
 *  not — `dir` on the page surfaces belongs to the document's own `w:bidi`,
 *  and a left-to-right `.docx` stays left-to-right however the UI reads
 *  (docs/124 §3.5). */
export function applyDocumentLocale(documentElement = document.documentElement) {
  documentElement.lang = activeLocale();
  documentElement.dir = direction(activeLocale());
}

/** Fetches a catalogue next to the app. Returns null rather than throwing when
 *  a locale is absent or malformed: an editor that refuses to open because a
 *  translation file 404'd is worse than an editor in English. */
export async function fetchCatalogue(tag, base = "./locales") {
  try {
    const response = await fetch(`${base}/${tag}.json`, { cache: "no-cache" });
    if (!response.ok) return null;
    const entries = await response.json();
    return entries && typeof entries === "object" ? entries : null;
  } catch {
    return null;
  }
}
