/** What typing one character should turn into: list auto-format and
 *  AutoCorrect (`docs/174` TA-05, TA-06; `docs/109` OO-016, RM-19).
 *
 *  Word does all of these as you type, Google Docs most, ONLYOFFICE the list
 *  triggers, the dashes, hyperlinks and sentence capitalisation. Each rule here
 *  answers one question about the text before the caret, AFTER the character
 *  was typed, and returns the edit to make or `null`. The host applies the edit
 *  as its OWN undo step, so an immediate Undo gives back exactly what was typed
 *  — the contract all three products share.
 *
 *  Pure: strings in, a description out. Offsets are JS string indices; the host
 *  converts them to the engine's UTF-8 byte offsets. Every function is O(length
 *  of the text before the caret in this paragraph), never O(document).
 *
 *  Deliberate differences from Word, recorded here so they read as decisions:
 *  - Only `1.`/`1)` start a numbered list and `-`/`*` a bulleted one. Word also
 *    takes `a.`, `i.` and `>`; the engine's numbered list is decimal, and
 *    starting a lettered list as a decimal one would be wrong, not partial.
 *  - Fractions and ordinals (1/2, 1st) are not converted: they need a run-level
 *    superscript or a glyph choice, and are not in ONLYOFFICE either.
 *  - ` - ` becomes an en dash when the word AFTER it is finished, Word's timing,
 *    not on the second space, ONLYOFFICE's (`RunAutoCorrect.js:612-688`): until
 *    a word follows, the hyphen may still be a list marker or a minus sign.
 */

/** A typed list marker at the very start of a paragraph, completed by the space
 *  just typed. `prefix` is the paragraph's text before the caret.
 *
 *  @param {string} prefix
 *  @returns {{kind: "numbered" | "bullet", marker: string} | null}
 */
export function listTriggerFor(prefix) {
  if (prefix === "1. " || prefix === "1) ") return { kind: "numbered", marker: prefix };
  if (prefix === "- " || prefix === "* ") return { kind: "bullet", marker: prefix };
  return null;
}

/** Characters that END a word, and so trigger a correction of the word before. */
const BOUNDARY = /[\s.,;:!?)]/;

/** Symbols typed as letters in brackets, completed by their closing bracket.
 *  The language-neutral half of the default replace table. */
export const SYMBOLS = Object.freeze([
  ["(c)", "©"],
  ["(r)", "®"],
  ["(tm)", "™"],
]);

/** Common English misspellings and the word meant, in lowercase. A short,
 *  high-confidence list: a correction the user did not want is worse than one
 *  not made. `i` → `I` is here, as it is in Word's English table, rather than a
 *  capitalisation rule: it is a property of English, not of sentences. */
export const TYPOS = new Map([
  ["i", "I"],
  ["teh", "the"],
  ["adn", "and"],
  ["taht", "that"],
  ["hte", "the"],
  ["recieve", "receive"],
  ["recieved", "received"],
  ["seperate", "separate"],
  ["occured", "occurred"],
  ["definately", "definitely"],
  ["wich", "which"],
  ["becuase", "because"],
  ["untill", "until"],
  ["thier", "their"],
  ["alot", "a lot"],
  ["accomodate", "accommodate"],
  ["acheive", "achieve"],
  ["beleive", "believe"],
  ["goverment", "government"],
  ["tommorow", "tomorrow"],
  ["wierd", "weird"],
]);

/** The replace table used when the host passes none: the symbols and the
 *  English typos. */
const DEFAULT_TABLE = new Map([...SYMBOLS, ...TYPOS]);

/** Words after which a full stop does not end a sentence (English). The
 *  default first-letter exceptions list; the host passes the user's own. */
export const ABBREVIATIONS = Object.freeze(["e.g.", "i.e.", "etc.", "vs.", "mr.", "mrs.", "ms.", "dr.", "prof.", "st.", "no.", "fig.", "cf."]);
const DEFAULT_EXCEPTIONS = new Set(ABBREVIATIONS);

const LETTER = /\p{L}/u;

/** Keeps the capitalisation of the word typed: "Teh" → "The", "TEH" → "THE". */
function matchCase(typed, replacement) {
  if (typed === typed.toUpperCase() && typed.length > 1) return replacement.toUpperCase();
  if (typed[0] === typed[0].toUpperCase()) return replacement[0].toUpperCase() + replacement.slice(1);
  return replacement;
}

/** How the word starting at `start` sits in the paragraph:
 *  `"paragraph"` when only space precedes it, `"sentence"` after a
 *  sentence-ending mark that is not an exception's full stop, otherwise `null`. */
function sentencePosition(prefix, start, exceptions) {
  const before = prefix.slice(0, start);
  const trimmed = before.trimEnd();
  if (trimmed === "") return "paragraph";
  if (trimmed.length === before.length) return null; // no space between: mid-word
  const last = trimmed.at(-1);
  if (last === "!" || last === "?") return "sentence";
  if (last !== ".") return null;
  const previousWord = trimmed.slice(trimmed.search(/\S+$/)).toLowerCase();
  if (exceptions.has(previousWord)) return null;
  // "1.5", "a.m." and initials ("J. Smith") are not sentence ends.
  return !/^(\S\.)+$/.test(previousWord) && !/\d\.$/.test(previousWord) ? "sentence" : null;
}

/** The replacement the table holds for `word`, or `null`. An entry written in
 *  lowercase matches any case and is re-cased to match ("Teh" → "The"); any
 *  other entry matches exactly as written. */
function tableLookup(table, word) {
  if (table.has(word)) return table.get(word);
  const lower = word.toLowerCase();
  if (lower !== word && table.has(lower)) return matchCase(word, table.get(lower));
  return null;
}

/** The correction for the word just completed, or `null`.
 *
 *  `prefix` is the paragraph's text before the caret; its LAST character is the
 *  one just typed. Returns the range `[start, end)` of `prefix` to replace and
 *  its replacement.
 *
 *  Options, each a separate switch in the AutoCorrect dialog:
 *  - `replace` (and its two halves `typos`, `symbols`): the replace-as-you-type
 *    `table` (a `Map` from what is typed to what replaces it);
 *  - `dashes`: `word--word` → em dash, ` -- ` and ` - ` → en dash;
 *  - `capitalize`: the first letter of a sentence, with `exceptions` (lowercase
 *    words ending in a full stop that do not end a sentence);
 *  - `capitalizeStart`: the first letter of a PARAGRAPH, which the host sets
 *    from the "table cells" switch when the paragraph is in a table cell, as
 *    ONLYOFFICE does (`RunAutoCorrect.js:785-789`).
 *
 *  @param {string} prefix
 *  @param {{replace?: boolean, typos?: boolean, symbols?: boolean, dashes?: boolean,
 *    capitalize?: boolean, capitalizeStart?: boolean, table?: Map<string, string>,
 *    exceptions?: Set<string>}} [options]
 *  @returns {{start: number, end: number, replacement: string} | null}
 */
export function autocorrectFor(prefix, options = {}) {
  const {
    replace = true,
    typos = true,
    symbols = true,
    dashes = true,
    capitalize = true,
    capitalizeStart = capitalize,
    table = DEFAULT_TABLE,
    exceptions = DEFAULT_EXCEPTIONS,
  } = options;
  if (!prefix) return null;
  const typed = prefix.at(-1);

  // Entries that END in what was just typed complete on it, the way Word's
  // "(c)" completes on its closing bracket.
  if (replace && symbols && !LETTER.test(typed)) {
    const lower = prefix.toLowerCase();
    for (const [from, to] of table) {
      if (!from || from.at(-1) !== typed || LETTER.test(from.at(-1))) continue;
      const hit = prefix.endsWith(from) || (from === from.toLowerCase() && lower.endsWith(from));
      if (!hit) continue;
      const start = prefix.length - from.length;
      // An entry beginning with a letter must begin a word ("teh" is not inside "steh").
      if (LETTER.test(from[0]) && start > 0 && !/\s/.test(prefix[start - 1])) continue;
      return { start, end: prefix.length, replacement: to };
    }
  }
  if (!BOUNDARY.test(typed)) return null;
  const body = prefix.slice(0, -1);
  const start = body.search(/\S+$/);
  if (start < 0) return null;
  const word = body.slice(start);

  if (dashes) {
    // "--" alone between spaces is an en dash ("a -- b" → "a – b"), and
    // between two words an em dash ("word--word" → "word—word").
    if (typed === " ") {
      if (word === "--" && start > 0 && /\s/.test(body[start - 1])) {
        return { start, end: body.length, replacement: "–" };
      }
      const joined = /^([^-\s]+)--([^-\s]+)$/.exec(word);
      if (joined) return { start, end: body.length, replacement: `${joined[1]}—${joined[2]}` };
    }
    // " - " between two words is an en dash, once the second word is finished.
    if (/\S - $/.test(body.slice(0, start)) && word !== "-") {
      return { start: start - 2, end: start - 1, replacement: "–" };
    }
  }

  // The word itself, without punctuation it carries ("teh," → "teh").
  const letters = /^\p{L}+/u.exec(word)?.[0] ?? "";
  let corrected = word;
  if (replace && typos && letters.length === word.length) {
    corrected = tableLookup(table, word) ?? word;
  }
  if (letters.length === word.length && /^\p{Ll}/u.test(corrected) && !/\p{Lu}/u.test(corrected.slice(1))) {
    // A bare scheme is the start of an address, not a sentence (ONLYOFFICE,
    // `RunAutoCorrect.js:793`).
    const where = /^(www|https?|ftp|mailto)$/i.test(corrected) ? null : sentencePosition(body, start, exceptions);
    if ((where === "paragraph" && capitalizeStart) || (where === "sentence" && capitalize)) {
      corrected = corrected[0].toLocaleUpperCase() + corrected.slice(1);
    }
  }
  return corrected === word ? null : { start, end: body.length, replacement: corrected };
}

/** Trailing characters that end a sentence rather than an address. */
const URL_TAIL = /[.,;:!?'"”’»]+$/;

/** The address just finished, as a link to make, or `null`.
 *
 *  Fires when WHITESPACE is typed after something that is plainly an address:
 *  a `http://`, `https://`, `ftp://` or `file://` URL, `mailto:`, `www.` and a
 *  domain, or an e-mail address — the set ONLYOFFICE recognises
 *  (`RunAutoCorrect.js:709`, `rx_allowedProtocols` or `www.` or `@`). Trailing
 *  sentence punctuation stays outside the link, and a closing bracket does
 *  unless the address opened one ("(see https://x.org/a_(b))").
 *
 *  Returns the link's range in `prefix` and its target: a `www.` address is
 *  linked as `http://…` and an e-mail address as `mailto:…`, which is what Word
 *  writes.
 *
 *  @param {string} prefix
 *  @returns {{start: number, end: number, target: string} | null}
 */
export function hyperlinkFor(prefix) {
  if (!prefix || !/\s/.test(prefix.at(-1))) return null;
  const body = prefix.slice(0, -1);
  let start = body.search(/\S+$/);
  if (start < 0) return null;
  let text = body.slice(start);
  // An address opened by a bracket or quote starts after it.
  const opener = /^[("'“‘«[]+/.exec(text)?.[0] ?? "";
  start += opener.length;
  text = text.slice(opener.length);
  text = text.replace(URL_TAIL, "");
  while (text.endsWith(")") && (text.match(/\(/g)?.length ?? 0) < (text.match(/\)/g)?.length ?? 0)) {
    text = text.slice(0, -1).replace(URL_TAIL, "");
  }
  if (!text) return null;
  let target = null;
  if (/^(https?|ftp|file):\/\/[^\s/?#]+\S*$/i.test(text)) target = text;
  else if (/^mailto:[^\s@]+@[^\s@]+\.[^\s@]+$/i.test(text)) target = text;
  else if (/^www\.[^\s.]+\.[^\s]+$/i.test(text)) target = `http://${text}`;
  else if (/^[^\s@()<>,;:"]+@[^\s@()<>,;:"]+\.\p{L}{2,}$/u.test(text)) target = `mailto:${text}`;
  if (!target) return null;
  return { start, end: start + text.length, target };
}
