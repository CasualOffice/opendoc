// The language of the selected text: read it in the status bar, set it from a
// dialog (`docs/153` `review.proofing-language-selection` and
// `text.language-of-selection`; `docs/168` batch 27; `docs/109` RM-42).
//
// THE SHAPE, FROM THE REFERENCES.
//
//   * Word: the status bar names the language of the text at the caret, and a
//     click on it opens Review ▸ Language ▸ Set Proofing Language — a searchable
//     list of languages, the ones with proofing tools marked, applied to the
//     selection.
//   * ONLYOFFICE: the same pair. The status bar's language button carries the
//     selection's language and opens `Common.Views.LanguageDialog`; the engine
//     call is `asc_docs_api.put_TextPrLang`, a text-property change
//     (`sdkjs/word/api.js:3569-3579`).
//   * Google Docs has only a document language (File ▸ Language), no per-run
//     one, so it contributes nothing here.
//
// So: Word's status-bar item and Word's dialog, reached from the status bar,
// the Review menu and the palette. The write is `setCharacterFormat` with
// `{"language": tag}` (`suggestCharacterFormat` in Suggesting mode): one undo
// step through the existing `FormatText` operation, tracked like any other
// character change, and saved as `w:lang`.
//
// WHAT A CARET MEANS. With nothing selected, Word and ONLYOFFICE apply a
// character property to the word the caret is in; at a caret between words it
// arms the property for what is typed next. This does the same, so "click in a
// word, set French" re-checks that word in French.
//
// WHERE WE DIFFER. Word's dialog also offers "Detect language automatically"
// and "Do not check spelling or grammar". Detection needs a language model
// this editor does not ship; "do not check" is `w:noProof`, a separate
// property, and is not offered here rather than offered half-done.
//
// SPELL CHECK honours the language because the checker reads it per run
// (`languageSpans`, `spell_check.mjs`), and this dialog says, per language,
// whether a word list exists — a language with none is marked, never silently
// unchecked.
import { t } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";
import { orderedSelectionEnds } from "./caret_navigation.mjs";
import { languageName } from "./proof_languages.mjs";
import { dictionaryForLanguage } from "./spelling.mjs";

/** The languages the dialog lists, as BCP 47 tags. Curated rather than every
 *  tag `Intl` knows: Word's list is the languages it has proofing tools or a
 *  keyboard for, about this length, and a list of 7,000 names is a list
 *  nobody can scan. Each name is the platform's own (`Intl.DisplayNames`), in
 *  the reader's language. A document's own tags are added when they are not
 *  here, so text Word tagged is never shown as unknown. */
export const LANGUAGE_CHOICES = Object.freeze([
  "en-US", "en-GB", "en-AU", "en-CA", "en-IN", "en-IE", "en-NZ", "en-ZA",
  "de-DE", "de-AT", "de-CH", "fr-FR", "fr-CA", "fr-BE", "fr-CH",
  "es-ES", "es-MX", "es-419", "it-IT", "pt-PT", "pt-BR", "nl-NL", "nl-BE",
  "da-DK", "sv-SE", "nb-NO", "nn-NO", "fi-FI", "is-IS",
  "pl-PL", "cs-CZ", "sk-SK", "sl-SI", "hr-HR", "sr-Latn-RS", "sr-Cyrl-RS", "bs-BA",
  "hu-HU", "ro-RO", "bg-BG", "el-GR", "ru-RU", "uk-UA", "be-BY",
  "lt-LT", "lv-LV", "et-EE", "tr-TR", "az-Latn-AZ", "ka-GE", "hy-AM",
  "ar-SA", "ar-EG", "he-IL", "fa-IR", "ur-PK",
  "hi-IN", "bn-IN", "ta-IN", "te-IN", "mr-IN", "gu-IN", "kn-IN", "ml-IN", "pa-IN",
  "th-TH", "vi-VN", "id-ID", "ms-MY", "fil-PH",
  "zh-CN", "zh-TW", "zh-HK", "ja-JP", "ko-KR",
  "sw-KE", "af-ZA", "ca-ES", "eu-ES", "gl-ES", "cy-GB", "ga-IE", "mt-MT", "sq-AL", "mk-MK",
]);

/**
 * The language of the span holding `offset`, from `languageSpans` output
 * (`[{start, end, language}]`, byte offsets). A caret belongs to the text
 * BEFORE it — what it continues — except at the start of the paragraph, where
 * it belongs to the first span. `""` when no span declares one.
 *
 * @param {{start: number, end: number, language: string}[]} spans
 * @param {number} offset
 * @returns {string}
 */
export function spanLanguageAt(spans, offset) {
  if (!Array.isArray(spans) || !spans.length) return "";
  const hit = offset <= 0 ? spans[0] : spans.find((span) => span.start < offset && offset <= span.end);
  return String(hit?.language ?? spans.at(-1)?.language ?? "");
}

/**
 * The selection's language as `{tag, mixed}`: `tag` is `""` when the text
 * declares none, and `mixed` is true when a range spans more than one.
 *
 * Complexity: O(runs in the caret's paragraph) for a caret or a one-paragraph
 * range (`languageSpans`); O(selection) for a range across paragraphs
 * (`selectionCharacterFormat`), which the status bar only asks once the
 * selection has settled.
 */
export function selectionLanguage(doc, selection) {
  if (!doc || !selection) return { tag: "", mixed: false };
  const [start, end] = orderedSelectionEnds(selection, doc);
  const spansOf = (node) => {
    try {
      return JSON.parse(doc.languageSpans(node));
    } catch {
      return [];
    }
  };
  if (start.node === end.node) {
    const spans = spansOf(start.node);
    if (start.offset === end.offset) return { tag: spanLanguageAt(spans, start.offset), mixed: false };
    const covered = spans.filter((span) => span.end > start.offset && span.start < end.offset);
    const tags = new Set(covered.map((span) => span.language));
    return tags.size > 1 ? { tag: "", mixed: true } : { tag: [...tags][0] ?? "", mixed: false };
  }
  try {
    const state = JSON.parse(doc.selectionCharacterFormat(start.node, start.offset, end.node, end.offset));
    if (state.mixed?.includes("language")) return { tag: "", mixed: true };
    return { tag: String(state.values?.language ?? ""), mixed: false };
  } catch {
    return { tag: "", mixed: false };
  }
}

/** The choices to list: the curated set, plus `extra` tags (the document's
 *  own) it lacks, sorted by name in the reader's language. */
export function languageChoices(extra = [], locale = "en") {
  const tags = [...new Set([...LANGUAGE_CHOICES, ...extra.filter(Boolean)])];
  const named = tags.map((tag) => ({ tag, name: languageName(tag, locale) }));
  return named.sort((a, b) => a.name.localeCompare(b.name, locale));
}

/** Whether `query` finds `choice`: by name or by tag, case-insensitively. */
export function choiceMatches(choice, query) {
  const q = String(query ?? "").trim().toLocaleLowerCase();
  return !q || choice.name.toLocaleLowerCase().includes(q) || choice.tag.toLowerCase().includes(q);
}

/** How long the selection must sit still before a cross-paragraph range's
 *  language is read (it is O(selection)); a caret is read at once. */
const SETTLE_MS = 120;

/**
 * Wires the status-bar language item and the Set Language dialog.
 *
 * `ed` is the editor context (docs/177); this reads `doc`, `selection`,
 * `reviewMode`, `pendingFormat` (and sets it), `settings`, `runEdit`,
 * `setStatus`, `updateToolbar`, `blockMutationInViewing` and
 * `focusEditorSurface`, and calls `locale()` for the reader's language.
 */
export function createLanguageSelection(ed, { locale = () => "en" } = {}) {
  const status = document.getElementById("textLanguageStatus");
  const statusLabel = document.getElementById("textLanguageStatusLabel");
  const dialog = document.getElementById("textLanguageDialog");
  const form = document.getElementById("textLanguageForm");
  const search = document.getElementById("textLanguageSearch");
  const list = document.getElementById("textLanguageList");
  const note = document.getElementById("textLanguageNote");
  const modal = dialog
    ? registerModal(dialog, { initialFocus: () => search, fallbackFocus: () => ed.pagesEl })
    : null;
  for (const button of dialog?.querySelectorAll("[data-text-language-dismiss]") ?? []) {
    button.addEventListener("click", () => modal?.close("cancel"));
  }

  const defaultLanguage = () => ed.settings?.spellLanguage || "en-US";
  /** The name a tag is shown by; `""` reads as the default language. */
  const nameOf = (tag) => languageName(tag || defaultLanguage(), locale());

  let choices = [];
  let current = { tag: "", mixed: false };

  /** Whether `tag` is checked for spelling, said in a sentence. */
  function proofingNote(tag) {
    const checked = dictionaryForLanguage(tag || defaultLanguage()) !== null;
    return t(checked ? "language.checked" : "language.notChecked", { language: nameOf(tag) });
  }

  function renderList() {
    if (!list) return;
    const query = search?.value ?? "";
    list.replaceChildren();
    const none = document.createElement("option");
    none.value = "";
    none.textContent = t("language.none", { language: nameOf("") });
    list.append(none);
    for (const choice of choices) {
      if (!choiceMatches(choice, query)) continue;
      const option = document.createElement("option");
      option.value = choice.tag;
      const unchecked = dictionaryForLanguage(choice.tag) === null;
      // The mark is part of the option's text, so a screen reader reads it
      // with the name rather than a reader having to see a glyph.
      option.textContent = unchecked ? choice.name : t("language.withSpelling", { language: choice.name });
      list.append(option);
    }
    const wanted = current.mixed ? null : current.tag;
    list.value = wanted !== null && [...list.options].some((o) => o.value === wanted) ? wanted : (list.options[1]?.value ?? "");
    if (note) note.textContent = proofingNote(list.value);
  }

  function open() {
    if (!ed.doc || !ed.selection) {
      ed.setStatus(t("command.needsDocument"), "error");
      return;
    }
    if (ed.blockMutationInViewing()) return;
    current = selectionLanguage(ed.doc, ed.selection);
    choices = languageChoices(current.tag ? [current.tag] : [], locale());
    if (search) search.value = "";
    renderList();
    modal?.open();
  }

  search?.addEventListener("input", renderList);
  // Arrow keys in the search box move through the list, as Word's dialog does,
  // so a reader can type a few letters and pick without a Tab.
  search?.addEventListener("keydown", (event) => {
    if (!list || (event.key !== "ArrowDown" && event.key !== "ArrowUp")) return;
    event.preventDefault();
    const step = event.key === "ArrowDown" ? 1 : -1;
    const next = Math.min(list.options.length - 1, Math.max(0, list.selectedIndex + step));
    list.selectedIndex = next;
    if (note) note.textContent = proofingNote(list.value);
  });
  list?.addEventListener("change", () => {
    if (note) note.textContent = proofingNote(list.value);
  });
  list?.addEventListener("dblclick", () => form?.requestSubmit());

  form?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const tag = list?.value ?? "";
    modal?.close("apply");
    await apply(tag);
  });

  /** Sets `tag` (`""` clears) on the selection, the word at a caret, or — at a
   *  caret between words — on what is typed next. One undo step. */
  async function apply(tag) {
    const doc = ed.doc;
    if (!doc || !ed.selection || ed.blockMutationInViewing()) return false;
    const json = JSON.stringify({ language: tag || null });
    const suggesting = ed.reviewMode === "suggesting";
    const write = (sn, so, en, eo) => (suggesting
      ? doc.suggestCharacterFormat(sn, so, en, eo, json, new Date().toISOString())
      : doc.setCharacterFormat(sn, so, en, eo, json));
    const [start, end] = orderedSelectionEnds(ed.selection, doc);
    let landed;
    if (start.node !== end.node || start.offset !== end.offset) {
      landed = await ed.runEdit(() => write(start.node, start.offset, end.node, end.offset), { keepSelection: true });
    } else {
      const word = Array.from(doc.wordAt(start.node, start.offset) ?? []);
      if (word.length === 2 && word[0] < word[1]) {
        landed = await ed.runEdit(() => write(start.node, word[0], start.node, word[1]), { keepSelection: true });
      } else {
        const pending = ed.pendingFormat ?? {};
        ed.pendingFormat = { ...pending, character: { ...pending.character, language: tag || null } };
        ed.updateToolbar();
        landed = true;
      }
    }
    if (landed) {
      ed.setStatus(tag ? t("language.applied", { language: nameOf(tag) }) : t("language.cleared", { language: nameOf("") }));
      reflectNow();
    }
    return landed;
  }

  // ---- The status bar ------------------------------------------------------
  let timer = 0;

  function reflectNow() {
    timer = 0;
    if (!status) return;
    const doc = ed.doc;
    status.hidden = !doc;
    if (!doc || !ed.selection) return;
    const pendingTag = ed.pendingFormat?.character?.language;
    const state = pendingTag !== undefined ? { tag: pendingTag ?? "", mixed: false } : selectionLanguage(doc, ed.selection);
    const label = state.mixed ? t("language.mixed") : nameOf(state.tag);
    if (statusLabel) statusLabel.textContent = label;
    const tip = t("language.statusTip", { language: label });
    status.title = tip;
    status.setAttribute("aria-label", tip);
  }

  /** Called on every selection change. A caret or a one-paragraph range is read
   *  now (O(paragraph)); a range across paragraphs once it has settled. */
  function reflect() {
    if (timer) clearTimeout(timer);
    timer = 0;
    const selection = ed.selection;
    if (selection && selection.anchor.node !== selection.focus.node) timer = setTimeout(reflectNow, SETTLE_MS);
    else reflectNow();
  }

  status?.addEventListener("click", () => open());

  return { open, apply, reflect };
}
