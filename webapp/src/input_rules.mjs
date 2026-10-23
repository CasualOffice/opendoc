/** What typing one character should turn into: list auto-format and
 *  AutoCorrect (`docs/174` TA-05, TA-06; `docs/109` OO-016).
 *
 *  Word does all of these as you type, Google Docs most, ONLYOFFICE the list
 *  triggers, the dash and sentence capitalisation. Each rule here answers one
 *  question about the text before the caret, AFTER the character was typed, and
 *  returns the edit to make or `null`. The host applies the edit as its OWN undo
 *  step, so an immediate Undo gives back exactly what was typed — the contract
 *  all three products share.
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

/** Symbols typed as letters in brackets, completed by their closing bracket. */
const SYMBOLS = [
  ["(c)", "©"],
  ["(r)", "®"],
  ["(tm)", "™"],
];

/** Common misspellings and the word meant, in lowercase. A short, high-confidence
 *  list: a correction the user did not want is worse than one not made. */
export const TYPOS = new Map([
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

/** Words after which a full stop does not end a sentence. */
const ABBREVIATIONS = new Set(["e.g.", "i.e.", "etc.", "vs.", "mr.", "mrs.", "ms.", "dr.", "prof.", "st.", "no.", "fig.", "cf."]);

/** Keeps the capitalisation of the word typed: "Teh" → "The", "TEH" → "THE". */
function matchCase(typed, replacement) {
  if (typed === typed.toUpperCase() && typed.length > 1) return replacement.toUpperCase();
  if (typed[0] === typed[0].toUpperCase()) return replacement[0].toUpperCase() + replacement.slice(1);
  return replacement;
}

/** True when the word starting at `start` begins a sentence: nothing but space
 *  before it in the paragraph, or a sentence-ending mark that is not an
 *  abbreviation's full stop. */
function startsSentence(prefix, start) {
  const before = prefix.slice(0, start);
  const trimmed = before.trimEnd();
  if (trimmed === "") return true;
  if (trimmed.length === before.length) return false; // no space between: mid-word
  const last = trimmed.at(-1);
  if (last === "!" || last === "?") return true;
  if (last !== ".") return false;
  const previousWord = trimmed.slice(trimmed.search(/\S+$/)).toLowerCase();
  if (ABBREVIATIONS.has(previousWord)) return false;
  // "1.5", "a.m." and initials ("J. Smith") are not sentence ends.
  return !/^(\S\.)+$/.test(previousWord) && !/\d\.$/.test(previousWord);
}

/** The correction for the word just completed, or `null`.
 *
 *  `prefix` is the paragraph's text before the caret; its LAST character is the
 *  one just typed. Returns the range `[start, end)` of `prefix` to replace and
 *  its replacement. `options.capitalize` and the other switches let the host
 *  turn rules off one by one.
 *
 *  @param {string} prefix
 *  @param {{capitalize?: boolean, typos?: boolean, symbols?: boolean, dashes?: boolean}} [options]
 *  @returns {{start: number, end: number, replacement: string} | null}
 */
export function autocorrectFor(prefix, { capitalize = true, typos = true, symbols = true, dashes = true } = {}) {
  if (!prefix) return null;
  const typed = prefix.at(-1);
  // Symbols complete on their own closing bracket, the way Word does it.
  if (symbols && typed === ")") {
    const lower = prefix.toLowerCase();
    for (const [code, symbol] of SYMBOLS) {
      if (lower.endsWith(code)) return { start: prefix.length - code.length, end: prefix.length, replacement: symbol };
    }
    return null;
  }
  if (!BOUNDARY.test(typed)) return null;
  const body = prefix.slice(0, -1);
  const start = body.search(/\S+$/);
  if (start < 0) return null;
  const word = body.slice(start);

  // Dashes: "--" alone between spaces is an en dash ("a -- b" → "a – b"), and
  // between two words an em dash ("word--word" → "word—word").
  if (dashes && typed === " ") {
    if (word === "--" && start > 0 && /\s/.test(body[start - 1])) {
      return { start, end: body.length, replacement: "–" };
    }
    const joined = /^([^-\s]+)--([^-\s]+)$/.exec(word);
    if (joined) return { start, end: body.length, replacement: `${joined[1]}—${joined[2]}` };
  }

  // The word itself, without punctuation it carries ("teh," → "teh").
  const letters = /^[A-Za-z]+/.exec(word)?.[0] ?? "";
  let corrected = word;
  if (typos && letters.length === word.length) {
    const fix = TYPOS.get(word.toLowerCase());
    if (fix) corrected = matchCase(word, fix);
  }
  if (capitalize && letters.length === word.length) {
    // "i" alone is always "I"; anything else only at a sentence start.
    if (corrected === "i") corrected = "I";
    else if (/^[a-z]/.test(corrected) && !/[A-Z]/.test(corrected.slice(1)) && startsSentence(body, start)) {
      corrected = corrected[0].toUpperCase() + corrected.slice(1);
    }
  }
  return corrected === word ? null : { start, end: body.length, replacement: corrected };
}
