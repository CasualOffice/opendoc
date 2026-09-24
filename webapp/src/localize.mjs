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
import { activeLocale, direction, has, t } from "./i18n.mjs";

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
    const key = element.dataset.i18n;
    // A key this locale has no answer for is LEFT ALONE. The English is
    // already in the markup beside the key — that is what it is there for —
    // so an untranslated string reads as English rather than as a dotted
    // identifier. It is also what makes translating incremental: routing a
    // surface and translating it are two different days' work, and between
    // them the product must still be readable.
    if (has(key)) setLabelText(element, t(key));
  }
  for (const [suffix, attribute] of Object.entries(ATTRIBUTE_KEYS)) {
    for (const element of root.querySelectorAll(`[data-i18n-${suffix}]`)) {
      const key = element.dataset[`i18n${cap(suffix)}`];
      if (has(key)) element.setAttribute(attribute, t(key));
    }
  }
}

/** A control's own tooltip, in the language now in force.
 *
 *  A disabled control has to say WHY, so the shell swaps its tooltip for the
 *  reason and must put the real one back afterwards. It used to put back a
 *  `dataset.enabledTitle` captured once at boot — which is before any
 *  catalogue has loaded, so the snapshot is always the English in the markup,
 *  and restoring it silently un-translated the control. Reading the key cannot
 *  go stale, because the key is what the catalogue is indexed by; the snapshot
 *  stays as the fallback for a control that carries no key. */
export function authoredTitle(button) {
  const key = button.dataset.i18nTitle;
  if (key && has(key)) return t(key);
  return button.dataset.enabledTitle ?? button.title;
}

/** Replaces an element's own words without touching what it CONTAINS.
 *
 *  `textContent = ...` is the obvious implementation and it is wrong here:
 *  half the labels in this editor are `<label>Initials<input …></label>`, and
 *  assigning textContent deletes the input. The element's first text node is
 *  its own words; anything else under it belongs to something else. Falls back
 *  to textContent for an element that has no text node yet, which is how an
 *  empty placeholder gets its first words. */
function setLabelText(element, text) {
  for (const node of element.childNodes) {
    if (node.nodeType === Node.TEXT_NODE && node.nodeValue.trim()) {
      node.nodeValue = text;
      return;
    }
  }
  if (element.childElementCount === 0) element.textContent = text;
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

/** Paints the document-state pill in the language now in force.
 *
 *  `status_policy.mjs` names the states and stays catalogue-free, so it can be
 *  unit-tested without one; it carries the key beside the English and this
 *  resolves it. The pill is reassigned every time the document's saved-ness
 *  changes, which is long after the boot sweep — so it showed "Opened" in
 *  every language until the key existed. */
export function paintDocumentState(badge, pill, textEl) {
  pill.dataset.state = badge.state;
  pill.querySelector(".ms").textContent = badge.icon;
  const words = badge.key && has(badge.key) ? t(badge.key) : badge.text;
  textEl.textContent = words;
  pill.title = words;
}
