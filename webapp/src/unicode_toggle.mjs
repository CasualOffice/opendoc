// Alt+X: a typed code point becomes its character, and a character becomes its
// code point (`docs/153` text.unicode-code-toggle).
//
// Word's rule, which ONLYOFFICE binds to the same chord
// (`sdkjs/word/Editor/Shortcuts.js:311` Alt+X; `:228` Alt+Cmd+X and
// Ctrl+Alt+X on a Mac, `ReplaceUnicodeToSymbol`):
//
//   * with the caret after up to six hexadecimal digits — optionally written
//     `U+2603` — the digits become the character they name;
//   * otherwise the character before the caret becomes its code, four or more
//     upper-case hex digits (`é` → `00E9`), so pressing Alt+X again turns it
//     back;
//   * with text selected, a selection that is a code becomes its character and
//     a selection that is one character becomes its code.
//
// A code that names nothing printable — a surrogate, a value past U+10FFFF, a
// control character — is not converted: Word converts the character before the
// caret instead, and so does this, which is why typing "a" and pressing Alt+X
// gives "0061" rather than a line feed.
//
// Pure: text in, the edit out. The caller reads the paragraph and applies it.
// Complexity: O(1) — at most the last eight UTF-16 units are examined.

/** The longest code Word reads before the caret. */
const MAX_HEX_DIGITS = 6;

/** Whether `cp` is a character worth inserting: a scalar value (not a
 *  surrogate, not past U+10FFFF) that is not a C0/C1 control. */
export function isPrintableCodePoint(cp) {
  if (!Number.isInteger(cp) || cp < 0x20 || cp > 0x10ffff) return false;
  if (cp >= 0x7f && cp <= 0x9f) return false;
  if (cp >= 0xd800 && cp <= 0xdfff) return false;
  // Noncharacters: U+FDD0..U+FDEF and the last two of every plane.
  if ((cp >= 0xfdd0 && cp <= 0xfdef) || (cp & 0xfffe) === 0xfffe) return false;
  return true;
}

/** The highest code a run of digits BEFORE THE CARET is read as. Planes 4-16
 *  are unassigned or private use, so "caf00E9" reads as "caf" + U+00E9 rather
 *  than as U+F00E9: a letter a-f before a code is the common case, and a
 *  private-use character typed by hex is not. A SELECTED code is taken whole. */
const MAX_TYPED_CODE = 0x3ffff;

/** `cp` as Word writes it: upper-case hex, at least four digits. */
export function formatCodePoint(cp) {
  return cp.toString(16).toUpperCase().padStart(4, "0");
}

/** A whole string that is a code (`2603`, `U+2603`, `u+1F600`) → its code
 *  point, or null. */
export function parseCode(text) {
  const match = /^(?:[Uu]\+)?([0-9A-Fa-f]{1,6})$/.exec(text);
  if (!match) return null;
  const cp = Number.parseInt(match[1], 16);
  return isPrintableCodePoint(cp) ? cp : null;
}

/**
 * The Alt+X edit for a collapsed caret.
 *
 * @param {string} before the paragraph's text before the caret
 * @returns {{ remove: number, insert: string } | null} how many UTF-16 units
 *   before the caret to replace, and with what; null when there is nothing to
 *   toggle (the caret is at the start of the paragraph).
 */
export function toggleBeforeCaret(before) {
  if (!before) return null;
  // The hex digits immediately before the caret, longest first, and an
  // optional `U+` in front of them.
  const tail = /(?:[Uu]\+)?([0-9A-Fa-f]{1,6})$/.exec(before.slice(-(MAX_HEX_DIGITS + 2)));
  if (tail) {
    const digits = tail[1];
    // Shrink from the left until the value names a character: "café1F600"
    // offers "1F600", and "12FFFFF" never names anything past U+10FFFF.
    for (let start = 0; start < digits.length; start++) {
      const cp = Number.parseInt(digits.slice(start), 16);
      if (cp <= MAX_TYPED_CODE && isPrintableCodePoint(cp)) {
        const prefixed = start === 0 && tail[0].length > digits.length;
        return { remove: digits.length - start + (prefixed ? 2 : 0), insert: String.fromCodePoint(cp) };
      }
    }
  }
  const cp = before.codePointAt(before.length - 1);
  const last = cp >= 0xdc00 && cp <= 0xdfff && before.length >= 2 ? before.codePointAt(before.length - 2) : cp;
  const width = last > 0xffff ? 2 : 1;
  return { remove: width, insert: formatCodePoint(last) };
}

/**
 * The Alt+X edit for a selection: its replacement text, or null when the
 * selection is neither a code nor a single character (Word does nothing then).
 *
 * @param {string} selected
 * @returns {string | null}
 */
export function toggleSelection(selected) {
  if (!selected || selected.includes("\n")) return null;
  const cp = parseCode(selected.trim());
  if (cp !== null) return String.fromCodePoint(cp);
  const chars = [...selected];
  if (chars.length === 1) return formatCodePoint(chars[0].codePointAt(0));
  return null;
}
