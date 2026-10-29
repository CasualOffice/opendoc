/** The editable AutoCorrect lists, per language: the replace-as-you-type table
 *  and the first-letter exceptions (`docs/153` `text.autocorrect-replace`,
 *  `review.autocorrect-replace-as-you-type`, `text.autocapitalise`; `docs/109`
 *  RM-19).
 *
 *  THE SHAPE, from the two references. Word keeps one replace/with table per
 *  language and a first-letter exceptions list beside it (AutoCorrect ▸
 *  Exceptions). ONLYOFFICE keeps the exceptions per language and stores only the
 *  user's DIFFERENCE from the built-in list — what was added and what was
 *  removed (`web-apps/apps/common/main/lib/view/AutoCorrectDialog.js:333-341`,
 *  `settings-letter-exception-add-<lang>` / `-rem-<lang>`), so a later release
 *  that improves the built-in list reaches users who customised it, and Reset is
 *  "forget the difference". Both lists here are stored that way.
 *
 *  A language is its PRIMARY subtag (`en-GB` and `en-US` share `en`): what to
 *  replace and which abbreviations end no sentence are properties of the
 *  language, not of the region, and a table per region would make a British
 *  user's corrections vanish in a document tagged `en-US`.
 *
 *  Pure apart from the injected `readPref`/`writePref`; no DOM, no engine.
 *  Every read is O(size of one list).
 */

import { ABBREVIATIONS, SYMBOLS, TYPOS } from "./input_rules.mjs";

/** The built-in replace tables. Symbols for every language; the English typo
 *  list for English only, because "teh" is a typo in English and a word in
 *  German. */
const BUILT_IN_REPLACEMENTS = Object.freeze({
  en: Object.freeze([...SYMBOLS, ...TYPOS]),
});

/** The built-in first-letter exceptions: abbreviations whose full stop does not
 *  end a sentence. Lowercase, each ending in its full stop. */
const BUILT_IN_EXCEPTIONS = Object.freeze({
  en: ABBREVIATIONS,
  de: Object.freeze(["z.b.", "usw.", "bzw.", "ca.", "evtl.", "ggf.", "vgl.", "nr.", "dr.", "str.", "abs.", "bzgl."]),
  fr: Object.freeze(["etc.", "cf.", "env.", "mme.", "mlle.", "dr.", "p.", "vol.", "ex."]),
  es: Object.freeze(["etc.", "sr.", "sra.", "srta.", "dr.", "dra.", "pág.", "núm.", "ej."]),
  ru: Object.freeze(["т.е.", "т.д.", "т.п.", "др.", "г.", "ул.", "стр.", "см."]),
});

/** The languages offered in the dialog before the user has customised any. */
export const BUILT_IN_LANGUAGES = Object.freeze(["en", "de", "fr", "es", "ru"]);

/** The longest entry either list accepts, in characters. Word's own limit for
 *  a replace entry is 255; a bound keeps one stored preference small. */
export const MAX_ENTRY = 255;
/** The most entries one stored difference holds. */
export const MAX_ENTRIES = 1000;

const LANGUAGES_PREF = "opendoc.autocorrect.languages";
const replacePref = (language) => `opendoc.autocorrect.replace.${language}`;
const exceptionsPref = (language) => `opendoc.autocorrect.exceptions.${language}`;

/** `en-GB` → `en`; anything unusable → `en`, the editor's own fallback. */
export function languageKey(tag) {
  const primary = String(tag ?? "").trim().split(/[-_]/)[0].toLowerCase();
  return /^[a-z]{2,8}$/.test(primary) ? primary : "en";
}

function readJson(io, key, fallback) {
  try {
    const raw = io.readPref(key);
    const value = raw ? JSON.parse(raw) : null;
    return value && typeof value === "object" ? value : fallback;
  } catch {
    return fallback;
  }
}

/** Whether `text` may be stored as an entry: non-empty after trimming, within
 *  the bound, no line break (an entry is typed on one line). */
export function admissibleEntry(text) {
  const value = String(text ?? "");
  return value.trim().length > 0 && value.length <= MAX_ENTRY && !/[\r\n]/.test(value);
}

/**
 * The per-language lists over a preference store.
 *
 * @param {{readPref: (key: string) => string | null, writePref: (key: string, value: string | null) => void}} io
 */
export function createAutoCorrectTables(io) {
  /** language → { replace: Map, exceptions: Set }, rebuilt on change. */
  const cache = new Map();

  function difference(key) {
    const stored = readJson(io, key, {});
    return {
      add: Array.isArray(stored.add) ? stored.add.slice(0, MAX_ENTRIES) : [],
      remove: Array.isArray(stored.remove) ? stored.remove.slice(0, MAX_ENTRIES) : [],
    };
  }

  function remember(language) {
    const known = new Set(readJson(io, LANGUAGES_PREF, []));
    if (known.has(language)) return;
    known.add(language);
    io.writePref(LANGUAGES_PREF, JSON.stringify([...known].sort()));
  }

  function store(key, language, diff) {
    if (!diff.add.length && !diff.remove.length) io.writePref(key, "");
    else {
      io.writePref(key, JSON.stringify(diff));
      remember(language);
    }
    cache.delete(language);
  }

  function lists(tag) {
    const language = languageKey(tag);
    let entry = cache.get(language);
    if (entry) return entry;
    const replace = new Map(BUILT_IN_REPLACEMENTS[language] ?? SYMBOLS);
    const replaceDiff = difference(replacePref(language));
    for (const from of replaceDiff.remove) replace.delete(from);
    for (const pair of replaceDiff.add) {
      if (Array.isArray(pair) && admissibleEntry(pair[0]) && typeof pair[1] === "string") replace.set(pair[0], pair[1]);
    }
    const exceptions = new Set(BUILT_IN_EXCEPTIONS[language] ?? []);
    const exceptionDiff = difference(exceptionsPref(language));
    for (const word of exceptionDiff.remove) exceptions.delete(word);
    for (const word of exceptionDiff.add) if (admissibleEntry(word)) exceptions.add(String(word).toLowerCase());
    entry = { replace, exceptions };
    cache.set(language, entry);
    return entry;
  }

  return {
    /** The replace table in force for `tag`, as a `Map` (read-only by contract). */
    replacements: (tag) => lists(tag).replace,
    /** The first-letter exceptions in force for `tag`, lowercase. */
    exceptions: (tag) => lists(tag).exceptions,
    /** The languages the dialog offers: the built-in ones and every language the
     *  user has customised. */
    languages() {
      return [...new Set([...BUILT_IN_LANGUAGES, ...readJson(io, LANGUAGES_PREF, [])])].filter((l) => /^[a-z]{2,8}$/.test(l));
    },
    /** Adds or replaces the entry `from` → `to`. Returns whether it was stored. */
    setReplacement(tag, from, to) {
      if (!admissibleEntry(from) || String(to ?? "").length > MAX_ENTRY || /[\r\n]/.test(String(to ?? ""))) return false;
      const language = languageKey(tag);
      const diff = difference(replacePref(language));
      diff.add = diff.add.filter((pair) => pair[0] !== from).concat([[from, String(to)]]);
      diff.remove = diff.remove.filter((entry) => entry !== from);
      if (diff.add.length > MAX_ENTRIES) return false;
      store(replacePref(language), language, diff);
      return true;
    },
    /** Deletes the entry `from`, built in or added. */
    deleteReplacement(tag, from) {
      const language = languageKey(tag);
      const diff = difference(replacePref(language));
      diff.add = diff.add.filter((pair) => pair[0] !== from);
      const builtIn = new Map(BUILT_IN_REPLACEMENTS[language] ?? SYMBOLS);
      if (builtIn.has(from) && !diff.remove.includes(from)) diff.remove.push(from);
      store(replacePref(language), language, diff);
    },
    /** Adds a first-letter exception (stored lowercase). Returns whether it was stored. */
    addException(tag, word) {
      if (!admissibleEntry(word) || /\s/.test(String(word).trim())) return false;
      const value = String(word).trim().toLowerCase();
      const language = languageKey(tag);
      const diff = difference(exceptionsPref(language));
      diff.remove = diff.remove.filter((entry) => entry !== value);
      if (!(BUILT_IN_EXCEPTIONS[language] ?? []).includes(value) && !diff.add.includes(value)) diff.add.push(value);
      store(exceptionsPref(language), language, diff);
      return true;
    },
    /** Removes a first-letter exception, built in or added. */
    deleteException(tag, word) {
      const value = String(word).toLowerCase();
      const language = languageKey(tag);
      const diff = difference(exceptionsPref(language));
      diff.add = diff.add.filter((entry) => entry !== value);
      if ((BUILT_IN_EXCEPTIONS[language] ?? []).includes(value) && !diff.remove.includes(value)) diff.remove.push(value);
      store(exceptionsPref(language), language, diff);
    },
    /** Forgets the user's difference for both of `tag`'s lists. */
    reset(tag) {
      const language = languageKey(tag);
      io.writePref(replacePref(language), "");
      io.writePref(exceptionsPref(language), "");
      cache.delete(language);
    },
    /** Whether `tag`'s lists differ from the built-in ones (Reset is offered). */
    customised(tag) {
      const language = languageKey(tag);
      const r = difference(replacePref(language));
      const e = difference(exceptionsPref(language));
      return r.add.length + r.remove.length + e.add.length + e.remove.length > 0;
    },
  };
}
