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

/**
 * `runs` — a `copyRichRuns` payload — with Change-case `mode` applied to its
 * text, every run's formatting untouched.
 *
 * Moved out of `main.js` to pay for the folding chrome's wiring (`109` FOLD-004),
 * and it belongs here anyway: the hard part is not the case mapping but the
 * RE-SLICING, which is pure and was previously only reachable through a browser.
 *
 * Why the whole selection is cased as one string and then re-sliced, rather
 * than each run cased on its own: `sentence` and `title` are **not local
 * rules**. A selection split as `["Hello. wor", "ld"]` by a bold boundary cases
 * per-run to `"Hello. Wor" + "Ld"` — the second run's first character looks
 * like the start of a word to a rule that cannot see the run before it. Joining
 * first makes the boundary invisible to the rule, which is the only way the
 * answer can be independent of where the formatting happens to change.
 *
 * The re-slice is valid only while the transform preserves LENGTH, which every
 * mode does for almost every input and `ß → SS` (and the final sigma, and the
 * Turkish dotted capital I) does not. Where the length moves, the per-run
 * fallback is taken: the boundary rule degrades, and that is stated rather than
 * silently producing text misaligned with its formatting — which is what a
 * re-slice over a changed length does, shifting every later run's characters
 * into the wrong format for the rest of the selection.
 *
 * A `paragraphBreak` run carries no `text` and counts as exactly one character
 * (the `\n` the join writes), so the offsets stay in step across paragraphs.
 *
 * Returns fresh run objects; `runs` is not mutated. `O(characters)`.
 */
export function recaseRichRuns(runs, mode) {
  const full = runs.map((r) => (r.paragraphBreak ? "\n" : String(r.text ?? ""))).join("");
  const transformed = transformCase(full, mode);
  const out = runs.map((r) => ({ ...r }));
  if (transformed.length !== full.length) {
    for (const r of out) if (!r.paragraphBreak && r.text != null) r.text = transformCase(String(r.text), mode);
    return out;
  }
  let i = 0;
  for (const r of out) {
    const len = r.paragraphBreak ? 1 : String(r.text ?? "").length;
    if (!r.paragraphBreak && r.text != null) r.text = transformed.slice(i, i + len);
    i += len;
  }
  return out;
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
 * The inverse: a JavaScript string index → the engine UTF-8 BYTE offset at the
 * same place.
 *
 * Kept beside its counterpart deliberately. The spell checker tokenizes on the
 * JS string and then has to hand the engine a range, so both directions are now
 * in use; two conversions of the same mapping written in two files is how they
 * drift. An index past the end clamps to the end.
 */
export function stringIndexToByteOffset(text, index) {
  if (index <= 0) return 0;
  return UTF8.encode(text.slice(0, Math.min(index, text.length))).length;
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

/**
 * The typographic quote a typed `"` or `'` becomes, at a caret in a document.
 *
 * `smartQuoteChar` above answers the RULE — opening or closing, given the
 * preceding character. This answers the whole question a keystroke asks, and it
 * is here rather than in `main.js` because the only hard part of it is a
 * text-and-offset trap, which is what this module is for:
 *
 * `offset` is an ENGINE offset, a UTF-8 BYTE index, so `offset - 1` names the
 * preceding character only while that character is ASCII. After "Müller", "café"
 * or any Cyrillic or CJK word it lands INSIDE a multi-byte character, the
 * engine's clamp snaps it forward past `offset`, the read returns "", and an
 * empty prefix reads as start-of-paragraph — so every apostrophe typed after a
 * non-ASCII letter came out as an OPENING quote (`docs/104` HF-055). The fix is
 * to never synthesize an engine offset in JavaScript: read the whole prefix from
 * 0, which is a boundary by definition, and take its last code point.
 *
 * `readPrefix` is injected, so the rule is answerable in node with a plain
 * function and this module stays free of the engine. A reader that throws — an
 * engine that cannot resolve the position — yields the literal key rather than a
 * guess.
 *
 * Complexity: O(paragraph) in the prefix read, which is the engine's cost and is
 * paid once per typed quote character, not per keystroke.
 *
 * @param {string} key the character typed.
 * @param {{enabled: boolean, offset: number, readPrefix: () => string}} io
 * @returns {string}
 */
export function smartQuoteForTyped(key, { enabled, offset, readPrefix }) {
  if (!enabled || (key !== '"' && key !== "'")) return key;
  if (offset <= 0) return smartQuoteChar(key, "");
  let previous = "";
  try {
    previous = [...String(readPrefix() ?? "")].at(-1) ?? "";
  } catch {
    return key;
  }
  return smartQuoteChar(key, previous);
}
