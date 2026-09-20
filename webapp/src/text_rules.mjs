// Rules over plain text: what Change case produces, which quote a keystroke
// becomes, and how an engine byte offset lines up with a JavaScript string.
//
// Every one of these is pure — no DOM, no engine handle, no module state — and
// every one of them is where the interesting bugs are. They were nevertheless
// only reachable through `main.js` (`109` HF-085), so the cases that actually
// break them (an apostrophe after "café", a title-cased "don't", a sentence
// ending in `?"`) could only be exercised through a full browser run.
//
// The impure halves stay with their callers: reading the paragraph text out of
// the engine, and deciding which offsets to ask about.

/**
 * `text` with Word's Change-case rule `mode` applied.
 *
 * Locale-aware throughout (`toLocaleUpperCase`), because the Turkish dotted/
 * dotless i and the German ß are not ASCII case-mappings. An unknown mode
 * returns the text unchanged rather than guessing.
 *
 * - `title` capitalises each word and lowercases its tail. An apostrophe is
 *   part of the word, not a boundary, so "o'brien" becomes "O'brien" and
 *   "don't" stays "Don't" — Word's Capitalize Each Word behaves the same way,
 *   and treating the apostrophe as a boundary would produce "Don'T".
 * - `sentence` lowercases everything first, then re-capitalises the first
 *   letter and each letter that follows terminal punctuation — including
 *   punctuation inside a closing quote or bracket (`?"`, `.)`).
 * - `toggle` swaps the case of each character, leaving caseless ones alone.
 */
export function transformCase(text, mode) {
  switch (mode) {
    case "upper":
      return text.toLocaleUpperCase();
    case "lower":
      return text.toLocaleLowerCase();
    case "title":
      return text.replace(/\p{L}[\p{L}'’]*/gu, (w) => w[0].toLocaleUpperCase() + w.slice(1).toLocaleLowerCase());
    case "sentence": {
      const lowered = text.toLocaleLowerCase();
      return lowered.replace(/(^\s*\p{L})|([.!?]["')\]]?\s+\p{L})/gu, (m) => m.toLocaleUpperCase());
    }
    case "toggle":
      return [...text].map((ch) => {
        const up = ch.toLocaleUpperCase();
        const lo = ch.toLocaleLowerCase();
        return ch === lo && ch !== up ? up : lo;
      }).join("");
    default:
      return text;
  }
}

/** Characters after which a quote is an OPENING quote. */
export const QUOTE_OPENERS = new Set(["(", "[", "{", "‘", "“", "—", "–", "-", "/"]);

/**
 * The curly quote a straight `key` becomes, given the character before it.
 *
 * `previous` is the preceding CHARACTER, or `""` at the start of a paragraph —
 * which opens, as does whitespace and any of `QUOTE_OPENERS`. Anything else
 * (a letter, a digit, a closing bracket) closes, which is what makes the
 * apostrophe in "don't" a right single quote rather than an opening one.
 *
 * Anything that is not a straight quote comes back untouched, so the caller
 * can route every keystroke through this without a second test.
 */
export function smartQuoteChar(key, previous) {
  if (key !== '"' && key !== "'") return key;
  const opening = previous === "" || /\s/u.test(previous) || QUOTE_OPENERS.has(previous);
  if (key === '"') return opening ? "“" : "”";
  return opening ? "‘" : "’";
}

const UTF8 = new TextEncoder();

/**
 * An engine UTF-8 BYTE offset → the JavaScript string index at the same place.
 *
 * The engine counts bytes and JavaScript counts UTF-16 code units, so the two
 * only agree while the text is ASCII. Walking by code point (not by unit) is
 * what keeps an astral character — an emoji, most CJK extensions — from
 * splitting a surrogate pair. An offset past the end clamps to the end.
 */
export function byteOffsetToStringIndex(text, byteOffset) {
  if (byteOffset <= 0) return 0;
  let bytes = 0;
  for (let i = 0; i < text.length; ) {
    if (bytes >= byteOffset) return i;
    const cp = text.codePointAt(i);
    const width = UTF8.encode(String.fromCodePoint(cp)).length;
    bytes += width;
    i += cp > 0xffff ? 2 : 1;
  }
  return text.length;
}

/**
 * Whether `text[start, end)` is bounded by non-word characters on both sides —
 * the "Whole word" find option.
 *
 * A word character is a Unicode letter, a Unicode digit or `_`, so "naïve"
 * and "Ω" are words; the boundary at the start or end of the paragraph counts
 * as non-word, which is why the missing neighbours read as `""`.
 */
export function isWholeWordAt(text, start, end) {
  const word = /[\p{L}\p{N}_]/u;
  return !word.test(text[start - 1] || "") && !word.test(text[end] || "");
}

/**
 * HTML-escapes a string for interpolation into markup.
 *
 * Pure and DOM-free, so it belongs beside the other text rules rather than in
 * the 18k-line module. Moved to bring `main.js` back under its ratchet after a
 * merge: two branches each lowered the ceiling from a shared base, and the
 * merge summed their additions, so the file ended two lines above a ceiling
 * neither branch had measured against.
 *
 * @param {unknown} text any value; stringified first.
 * @returns {string} the text with `& < > "` replaced by their entities.
 */
export function escapeHtml(text) {
  return String(text)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
