// What a typed link target means, before anything is written (audit
// DIALOGS-11 and DIALOGS-14).
//
// The link popover takes one field, as Google Docs' does: a web address, an
// email address, or a place in this document. This decides which of those the
// text is, and refuses what none of them is, so the field can be marked invalid
// and the primary action disabled BEFORE the reader presses it, rather than
// writing a link that cannot be followed. The schemes are the ones a link may
// follow (`link_targets.mjs` EXTERNAL_LINK_SCHEMES): a `javascript:` or `file:`
// target typed here would be written into the document and then refused on
// every click, so it is refused once, here, with the reason.
//
// Pure: no DOM, no engine, so `link_popover_model.test.mjs` holds every rule.
import { EXTERNAL_LINK_SCHEMES } from "./link_targets.mjs";

/** A bare domain: labels joined by dots, ending in a two-or-more letter TLD,
 *  with an optional port and path. "example.com/docs" and "localhost:8080" are
 *  addresses; "hello world" and "notes" are not. */
const BARE_HOST = /^(?:localhost|(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,})(?::\d{1,5})?(?:[/?#]\S*)?$/i;
const EMAIL = /^[^\s@/:]+@[^\s@/:]+\.[a-z]{2,}$/i;
const SCHEME = /^([a-z][a-z0-9+.-]*):/i;

/**
 * Resolves what the reader typed into the target a link will carry.
 *
 * @param {string} text the field's value.
 * @param {{bookmarks?: string[]}} [context] the document's bookmark names, so
 *   "#name" can be checked against what exists.
 * @returns {{ok: true, target: string, kind: "web"|"email"|"place"} |
 *           {ok: false, why: "empty"|"invalid"|"scheme"|"noBookmark"}}
 *
 * Complexity: O(length of `text` + bookmarks).
 */
export function resolveLinkInput(text, { bookmarks = [] } = {}) {
  const value = String(text ?? "").trim();
  if (!value) return { ok: false, why: "empty" };
  if (value.startsWith("#")) {
    const name = value.slice(1);
    if (!name) return { ok: false, why: "invalid" };
    return bookmarks.includes(name) ? { ok: true, target: value, kind: "place" } : { ok: false, why: "noBookmark" };
  }
  if (/\s/.test(value)) return { ok: false, why: "invalid" };
  const scheme = SCHEME.exec(value)?.[1]?.toLowerCase();
  // "localhost:8080" parses as scheme "localhost": a port is not a scheme.
  if (scheme && !/^[a-z0-9.-]+:\d/i.test(value)) {
    if (!EXTERNAL_LINK_SCHEMES.has(`${scheme}:`)) return { ok: false, why: "scheme" };
    let parsed;
    try {
      parsed = new URL(value);
    } catch {
      return { ok: false, why: "invalid" };
    }
    if (scheme === "mailto") return parsed.pathname.includes("@") ? { ok: true, target: value, kind: "email" } : { ok: false, why: "invalid" };
    return parsed.hostname ? { ok: true, target: value, kind: "web" } : { ok: false, why: "invalid" };
  }
  if (EMAIL.test(value)) return { ok: true, target: `mailto:${value}`, kind: "email" };
  if (BARE_HOST.test(value)) return { ok: true, target: `https://${value}`, kind: "web" };
  return { ok: false, why: "invalid" };
}

/** The bookmarks whose name contains `query`, case-insensitively, at most
 *  `limit` of them, in document order. An empty query lists the first ones, so
 *  the places are discoverable without typing. */
export function matchPlaces(bookmarks, query, limit = 6) {
  const needle = String(query ?? "").trim().replace(/^#/, "").toLowerCase();
  return bookmarks.filter((name) => name.toLowerCase().includes(needle)).slice(0, limit);
}

/** The row the keyboard lands on after ArrowDown or ArrowUp in a list of
 *  `count` rows, from `index` (-1: none yet). Down from none is the first row,
 *  Up from none the last, and both wrap, as the APG combobox's listbox does. */
export function stepActiveOption(index, count, key) {
  if (count <= 0) return -1;
  if (key === "ArrowDown") return index < 0 || index >= count - 1 ? 0 : index + 1;
  if (key === "ArrowUp") return index <= 0 ? count - 1 : index - 1;
  return index;
}
