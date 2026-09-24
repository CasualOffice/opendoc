// The shipped locales, and how each one names itself (docs/124 §5).
//
// The name is the ENDONYM — "Deutsch", not "German". A language picker that
// lists languages in the language you cannot currently read is a picker you
// cannot use to escape, which is the one job it has. Endonyms are proper nouns
// in their own language and are never translated, which is why they live in a
// table here rather than in a catalogue.
export const LOCALES = Object.freeze([
  Object.freeze({ tag: "en", endonym: "English" }),
  Object.freeze({ tag: "ar", endonym: "العربية" }),
  Object.freeze({ tag: "de", endonym: "Deutsch" }),
  Object.freeze({ tag: "es", endonym: "Español" }),
  Object.freeze({ tag: "fr", endonym: "Français" }),
  Object.freeze({ tag: "hi", endonym: "हिन्दी" }),
  Object.freeze({ tag: "id", endonym: "Bahasa Indonesia" }),
  Object.freeze({ tag: "it", endonym: "Italiano" }),
  Object.freeze({ tag: "ja", endonym: "日本語" }),
  Object.freeze({ tag: "ko", endonym: "한국어" }),
  Object.freeze({ tag: "nl", endonym: "Nederlands" }),
  Object.freeze({ tag: "pl", endonym: "Polski" }),
  Object.freeze({ tag: "pt-BR", endonym: "Português (Brasil)" }),
  Object.freeze({ tag: "ru", endonym: "Русский" }),
  Object.freeze({ tag: "tr", endonym: "Türkçe" }),
  Object.freeze({ tag: "uk", endonym: "Українська" }),
  Object.freeze({ tag: "vi", endonym: "Tiếng Việt" }),
  Object.freeze({ tag: "zh-Hans", endonym: "简体中文" }),
  Object.freeze({ tag: "zh-Hant", endonym: "繁體中文" }),
]);

const TAGS = new Set(LOCALES.map((locale) => locale.tag));

/**
 * The best shipped locale for a list of preferences, longest match first.
 *
 * A browser asking for `de-AT` gets `de`; one asking for `zh-TW` gets
 * `zh-Hant`, because a Taiwanese user given Simplified has been given the
 * wrong writing system rather than a near miss. Unknown preferences fall
 * through to the next one and finally to English.
 */
export function bestLocale(preferences, shipped = TAGS) {
  for (const raw of preferences ?? []) {
    const wanted = String(raw || "").trim();
    if (!wanted) continue;
    if (shipped.has(wanted)) return wanted;
    const script = SCRIPT_ALIASES[wanted.toLowerCase()];
    if (script && shipped.has(script)) return script;
    let base = wanted;
    while (base.includes("-")) {
      base = base.slice(0, base.lastIndexOf("-"));
      if (shipped.has(base)) return base;
      const alias = SCRIPT_ALIASES[base.toLowerCase()];
      if (alias && shipped.has(alias)) return alias;
    }
  }
  return "en";
}

/** Region tags whose script is not guessable from the language subtag. `zh-TW`
 *  and `zh-HK` are Traditional; `zh-CN` and `zh-SG` are Simplified. Truncating
 *  `zh-TW` to `zh` would hand a Taiwanese user Simplified characters. */
const SCRIPT_ALIASES = Object.freeze({
  "zh-tw": "zh-Hant",
  "zh-hk": "zh-Hant",
  "zh-mo": "zh-Hant",
  "zh-hant": "zh-Hant",
  "zh-cn": "zh-Hans",
  "zh-sg": "zh-Hans",
  "zh-hans": "zh-Hans",
  zh: "zh-Hans",
  "pt-pt": "pt-BR",
  pt: "pt-BR",
});

export function isShipped(tag) {
  return TAGS.has(tag);
}
