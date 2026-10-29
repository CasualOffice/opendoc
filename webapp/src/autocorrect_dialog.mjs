// The AutoCorrect options dialog: every AutoCorrect switch in one place, the
// per-language replace table and the first-letter exceptions (`docs/153`
// `review.autocorrect-options`, `text.autocorrect-replace`,
// `review.autocorrect-replace-as-you-type`, `text.autocapitalise`; `docs/109`
// RM-19).
//
// THE SHAPE, FROM THE REFERENCES. Word's AutoCorrect dialog has a "Replace
// text as you type" switch over a Replace/With pair and the table below it,
// with Add (Replace, when the entry exists) and Delete; its first-letter
// exceptions are a second list behind an Exceptions button; the
// "AutoFormat As You Type" switches are a second tab. ONLYOFFICE's dialog
// (`web-apps/apps/common/main/lib/view/AutoCorrectDialog.js`) has the same
// lists, adds Reset to default (`:192-195`) and keys the exceptions by a
// language picker (`:344-383`).
//
// So one dialog, three groups: the as-you-type switches, the replace table, and
// the exceptions, both lists under ONE language picker, because both are
// properties of the language being typed. Changes apply at once and are
// remembered, as ONLYOFFICE's do; there is nothing to confirm, so the dialog has
// Close and not OK/Cancel.
//
// Keyboard and screen reader: the lists are native `<select size>` list boxes
// (arrow keys, type-ahead, announced as "list box, n items"), every control has
// a visible label, and every change is said in the dialog's live note.
import { t } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";
import { languageKey } from "./autocorrect_table.mjs";
import { languageName } from "./proof_languages.mjs";
import { selectionLanguage } from "./language_selection.mjs";

/**
 * @param {object} ed the editor context (docs/177): `doc`, `selection`,
 *   `settings`, `pagesEl`.
 * @param {ReturnType<import("./autocorrect.mjs").createAutoCorrect>} autoCorrect
 * @param {{locale?: () => string}} [options]
 */
export function createAutoCorrectDialog(ed, autoCorrect, { locale = () => "en" } = {}) {
  const el = (id) => document.getElementById(id);
  const dialog = el("autoCorrectDialog");
  const language = el("autoCorrectLanguage");
  const from = el("autoCorrectFrom");
  const to = el("autoCorrectTo");
  const add = el("autoCorrectAdd");
  const remove = el("autoCorrectDelete");
  const list = el("autoCorrectList");
  const exceptionInput = el("autoCorrectExceptionInput");
  const exceptionAdd = el("autoCorrectExceptionAdd");
  const exceptionRemove = el("autoCorrectExceptionDelete");
  const exceptionList = el("autoCorrectExceptionList");
  const reset = el("autoCorrectReset");
  const note = el("autoCorrectNote");
  const modal = dialog
    ? registerModal(dialog, { initialFocus: () => dialog.querySelector("input[type=checkbox]"), fallbackFocus: () => ed.pagesEl })
    : null;
  for (const button of dialog?.querySelectorAll("[data-autocorrect-dismiss]") ?? []) {
    button.addEventListener("click", () => modal?.close("done"));
  }
  const { tables } = autoCorrect;
  const say = (text) => {
    if (note) note.textContent = text;
  };
  const current = () => language?.value || "en";
  const nameOf = (key) => languageName(key, locale());

  function renderLanguages(selected) {
    if (!language) return;
    const keys = [...new Set([...tables.languages(), selected])];
    const named = keys.map((key) => ({ key, name: nameOf(key) })).sort((a, b) => a.name.localeCompare(b.name, locale()));
    language.replaceChildren(...named.map(({ key, name }) => {
      const option = document.createElement("option");
      option.value = key;
      option.textContent = name;
      return option;
    }));
    language.value = selected;
  }

  function renderLists() {
    const key = current();
    if (list) {
      const keep = list.value;
      list.replaceChildren(...[...tables.replacements(key)].sort(([a], [b]) => a.localeCompare(b)).map(([source, target]) => {
        const option = document.createElement("option");
        option.value = source;
        option.textContent = t("autocorrect.entry", { from: source, to: target });
        return option;
      }));
      if (keep) list.value = keep;
    }
    if (exceptionList) {
      exceptionList.replaceChildren(...[...tables.exceptions(key)].sort().map((word) => {
        const option = document.createElement("option");
        option.value = word;
        option.textContent = word;
        return option;
      }));
    }
    if (reset) reset.disabled = !tables.customised(key);
    reflectAddLabel();
  }

  /** "Add" for a new entry, "Replace" for one that exists, as Word's button reads. */
  function reflectAddLabel() {
    if (!add || !from) return;
    const exists = tables.replacements(current()).has(from.value);
    add.textContent = t(exists ? "autocorrect.replaceEntry" : "autocorrect.addEntry");
    add.disabled = !from.value.trim();
    if (remove) remove.disabled = !list?.value;
    if (exceptionAdd) exceptionAdd.disabled = !exceptionInput?.value.trim();
    if (exceptionRemove) exceptionRemove.disabled = !exceptionList?.value;
  }

  function open() {
    // The language picker starts on the language being typed at the caret, so
    // the list a reader sees is the list in force where they are writing.
    const at = ed.doc && ed.selection ? selectionLanguage(ed.doc, ed.selection).tag : "";
    const key = languageKey(at || ed.settings?.spellLanguage || "en");
    renderLanguages(key);
    if (from) from.value = "";
    if (to) to.value = "";
    if (exceptionInput) exceptionInput.value = "";
    say("");
    renderLists();
    modal?.open();
  }

  language?.addEventListener("change", () => {
    renderLists();
    say(t("autocorrect.languageShown", { language: nameOf(current()) }));
  });
  from?.addEventListener("input", reflectAddLabel);
  exceptionInput?.addEventListener("input", reflectAddLabel);
  list?.addEventListener("change", () => {
    const target = tables.replacements(current()).get(list.value);
    if (from) from.value = list.value;
    if (to && target !== undefined) to.value = target;
    reflectAddLabel();
  });
  exceptionList?.addEventListener("change", () => {
    if (exceptionInput) exceptionInput.value = exceptionList.value;
    reflectAddLabel();
  });

  function addEntry() {
    if (!from || !to) return;
    const source = from.value;
    if (!tables.setReplacement(current(), source, to.value)) {
      say(t("autocorrect.entryRefused"));
      return;
    }
    renderLists();
    if (list) list.value = source;
    say(t("autocorrect.entryAdded", { from: source, to: to.value }));
    reflectAddLabel();
  }
  add?.addEventListener("click", addEntry);
  // Enter in either field adds, as it does in Word's dialog.
  for (const input of [from, to]) {
    input?.addEventListener("keydown", (event) => {
      if (event.key !== "Enter") return;
      event.preventDefault();
      addEntry();
    });
  }
  remove?.addEventListener("click", () => {
    const source = list?.value;
    if (!source) return;
    tables.deleteReplacement(current(), source);
    renderLists();
    say(t("autocorrect.entryDeleted", { from: source }));
  });

  function addException() {
    const word = exceptionInput?.value ?? "";
    if (!tables.addException(current(), word)) {
      say(t("autocorrect.entryRefused"));
      return;
    }
    renderLists();
    say(t("autocorrect.exceptionAdded", { word: word.trim().toLowerCase() }));
  }
  exceptionAdd?.addEventListener("click", addException);
  exceptionInput?.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    addException();
  });
  exceptionRemove?.addEventListener("click", () => {
    const word = exceptionList?.value;
    if (!word) return;
    tables.deleteException(current(), word);
    renderLists();
    say(t("autocorrect.exceptionDeleted", { word }));
  });
  // Settings ▸ AutoCorrect opens this too: the dialog stacks over Settings.
  el("settingsAutoCorrectOptions")?.addEventListener("click", () => open());
  reset?.addEventListener("click", () => {
    tables.reset(current());
    renderLists();
    say(t("autocorrect.listsReset", { language: nameOf(current()) }));
  });

  return { open };
}
