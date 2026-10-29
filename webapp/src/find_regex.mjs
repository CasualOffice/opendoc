// Find & Replace with a regular expression (`docs/153` shell.find-regex).
//
// Word's "Use wildcards", Google Docs' "Match using regular expressions" and
// ONLYOFFICE's "Match using regular expressions"
// (`web-apps/apps/common/main/lib/view/SearchPanel.js`, `textMatchUsingRegExp`)
// all take a pattern and a replacement that can name the groups it captured.
// The dialect here is the browser's own `RegExp` with the `u` flag, the one a
// person typing into a web page expects: `\d`, `\b`, lookbehind, named groups,
// and `$1`, `$<name>`, `$&`, `$$` in the replacement.
//
// Pure: the engine's text (`searchableText()`, `[[node, text], …]` in the
// literal scan's order, offsets in UTF-8 bytes) comes in as data and model
// ranges go out, so every rule runs in node. The caller does the engine calls.
//
// A pattern never spans paragraphs, as the literal search never does: each
// paragraph is matched on its own, which is also what Word and Docs do.
//
// Complexity: O(text) per scan plus O(match length) per match for the offset
// conversion, bounded by `cap` matches.

import { stringIndexToByteOffset } from "./text_rules.mjs";

/** The flags every compiled pattern carries: global (to scan), Unicode. */
const BASE_FLAGS = "gu";

/** Letters, digits and the connector: what a whole-word boundary is between.
 *  The same class `isWholeWordAt` reads for the literal search. */
const WORD_CHAR = "[\\p{L}\\p{N}_]";

/**
 * Compiles a query as a pattern, or says why it cannot.
 *
 * Returns `{ regex }` or `{ error }`, never throws: an invalid pattern is a
 * state the Find panel shows inline while the person is still typing it, not an
 * exception. `wholeWord` wraps the pattern in the same letter/digit boundary
 * the literal option uses, so the two checkboxes compose as they look.
 *
 * @param {string} query
 * @param {{ matchCase?: boolean, wholeWord?: boolean }} [options]
 * @returns {{ regex: RegExp } | { error: string }}
 */
export function compileFindPattern(query, { matchCase = false, wholeWord = false } = {}) {
  if (!query) return { error: "" };
  const source = wholeWord ? `(?<!${WORD_CHAR})(?:${query})(?!${WORD_CHAR})` : query;
  try {
    return { regex: new RegExp(source, BASE_FLAGS + (matchCase ? "" : "i")) };
  } catch (err) {
    // The engine's own sentence ("Invalid regular expression: /(/: Unterminated
    // group") names the problem better than anything generic could; only the
    // prefix every one of them repeats is dropped.
    const message = String(err?.message ?? err).replace(/^Invalid regular expression:\s*/, "");
    return { error: message };
  }
}

/**
 * Every match of `regex` over the engine's paragraphs, in document order, as
 * model ranges `{ startNode, startOffset, endNode, endOffset, text, index }`
 * (`text` is the paragraph, `index` the UTF-16 start, both kept so a
 * replacement can be expanded with the paragraph as context).
 *
 * An empty match (`a*` between two letters) is skipped and the scan moves on
 * one code point: Word and Docs never select nothing, and a zero-width range
 * cannot be replaced visibly.
 *
 * @param {Array<[string, string]>} paragraphs  `searchableText()` parsed
 * @param {RegExp} regex  a `g`-flagged pattern from `compileFindPattern`
 * @param {number} [cap]  the most matches to return
 */
export function regexMatches(paragraphs, regex, cap = Infinity) {
  const matches = [];
  for (const [node, text] of paragraphs) {
    if (!text) continue;
    regex.lastIndex = 0;
    let found;
    while ((found = regex.exec(text)) !== null) {
      if (found[0].length === 0) {
        const cp = text.codePointAt(regex.lastIndex);
        regex.lastIndex += cp !== undefined && cp > 0xffff ? 2 : 1;
        if (regex.lastIndex > text.length) break;
        continue;
      }
      const start = found.index;
      const end = start + found[0].length;
      matches.push({
        startNode: node,
        startOffset: stringIndexToByteOffset(text, start),
        endNode: node,
        endOffset: stringIndexToByteOffset(text, end),
        text,
        index: start,
      });
      if (matches.length >= cap) return matches;
    }
  }
  return matches;
}

/**
 * The text one match is replaced with: `replacement` with `$1`, `$<name>`,
 * `$&`, `` $` ``, `$'` and `$$` expanded exactly as `String.prototype.replace`
 * expands them, with the whole paragraph as context so a lookbehind or an
 * anchor still sees what it saw when the match was found.
 *
 * The match is re-found with a STICKY copy of the pattern at its own index, so
 * the expansion is the browser's, not a re-implementation of it.
 *
 * @param {RegExp} regex
 * @param {{ text: string, index: number }} match  from `regexMatches`
 * @param {string} replacement
 */
export function expandReplacement(regex, match, replacement) {
  const sticky = new RegExp(regex.source, regex.flags.replace("g", "") + "y");
  sticky.lastIndex = match.index;
  const found = sticky.exec(match.text);
  if (!found) return replacement;
  sticky.lastIndex = match.index;
  const replaced = match.text.replace(sticky, replacement);
  const tail = match.text.length - (match.index + found[0].length);
  return replaced.slice(match.index, replaced.length - tail);
}
