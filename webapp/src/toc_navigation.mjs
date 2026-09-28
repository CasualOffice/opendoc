// Making a table of contents navigable — including the ones the author's field
// never hyperlinked.
//
// ## What was actually broken
//
// Reported as "i checked TOC.. many of this points were unclickable on our
// place whereas UX of the same in ONLYOFFICE was way different nice".
//
// Driven in the real editor on two documents that differ in ONE switch:
//
//   * ` TOC \o "1-3" \h \z \u ` — Word wraps every entry in a
//     `w:hyperlink w:anchor="_TocN"`. Clicking an entry already jumps to the
//     heading, at 54 pages, for every entry. Nothing to fix.
//   * ` TOC \o "1-3" \z \u ` — no `\h`, so there is no hyperlink anywhere in
//     the table. Identical text, identical `TOCn` paragraph styles, identical
//     PAGEREF page numbers. Measured: the pointer stays `text` over every
//     entry, a click does nothing at all, and the status bar says nothing.
//
// That second shape is not exotic. It is what Word writes whenever `\h` is
// absent, what LibreOffice and most pre-2007 producers emit, and what a table
// built by hand looks like. Half a real corpus is "many of these points".
//
// ## The rule, and where it comes from
//
// Word will not follow a non-hyperlinked entry either — but Word is also the
// program that wrote the field, so in Word the two cases barely co-occur.
// ONLYOFFICE's contents entries navigate whatever the source document did,
// because their TOC is a field their own layer owns. That is the behaviour
// being matched here, stated once:
//
//   **A contents entry navigates to the heading it names, whether or not the
//   authored field carried `\h`.**
//
// The authored hyperlink still wins where there is one: it names a bookmark,
// which is exact, and it is what the round-trip preserves. This is the fallback
// for when there is no link to follow.
//
// ## How an entry is recognised, and why not by field range
//
// The engine reports `fieldRangeEntries()` — `id\tinstruction` for every
// paragraph-spanning complex field — so a host can tell a document HOLDS a
// table of contents. It does not report which paragraphs the range covers, so
// "is this paragraph inside the TOC field" is not a question that can be asked
// today (noted in the report; it is engine work).
//
// What can be asked, in one call, is the paragraph's style name. Word marks
// every generated entry with `TOC 1`..`TOC 9`, ODF/LibreOffice with
// `Contents 1`..`Contents 10`, and that marking is exactly as authoritative as
// the field range — it is how Word itself finds the entries to restyle. So the
// style is the signal.
//
// ## Complexity
//
// `buildHeadingIndex` is O(headings) over rows the engine already produced for
// the Outline panel, and the caller caches it per document version. Everything
// else here is a map lookup and some string trimming: **O(1) per click**, with
// no walk of the document per entry.

/** Word's `TOC 1`..`TOC 9` and ODF's `Contents 1`..`Contents 10`, plus the
 *  space-less spellings a style id (rather than a style NAME) uses. */
const TOC_STYLE = /^(?:toc|contents|tabledescontenidos|index)\s*([1-9]|10)$/;

/**
 * Whether paragraph style `name` marks a table-of-contents entry.
 *
 * Case- and space-insensitive, because the same style reaches us as `TOC 1`
 * (Word's style name), `TOC1` (its style id) and `Contents 1` (ODF).
 *
 * O(1).
 *
 * @param {string} name a paragraph style name or id
 * @returns {boolean}
 */
export function isTocEntryStyle(name) {
  if (typeof name !== "string" || name === "") return false;
  return TOC_STYLE.test(name.trim().toLowerCase().replace(/\s+/g, " "));
}

/**
 * The heading text an entry names, with the parts a table of contents adds
 * stripped: the tab (or dot leader) and the page number that follow it.
 *
 * `"Alpha chapter\t2"`, `"Alpha chapter......... 2"` and `"1.2 Beta chapter\t7"`
 * all reduce to the text side. Leading numbering is KEPT, because the heading it
 * points at usually carries the same numbering in its own text, and dropping it
 * would merge "1.1 Scope" and "2.1 Scope" onto one target.
 *
 * O(length of the entry).
 *
 * @param {string} text the entry paragraph's plain text
 * @returns {string}
 */
export function tocEntryLabel(text) {
  if (typeof text !== "string") return "";
  let label = text;
  const tab = label.indexOf("\t");
  if (tab >= 0) label = label.slice(0, tab);
  // A leader written as literal dots (or the older `. . .`) rather than as a tab
  // with a leader character, which is what a hand-built table looks like.
  label = label.replace(/[.…·_\-\s]{3,}\d*\s*$/u, "");
  // A trailing page number with no leader at all.
  label = label.replace(/\s+\d+\s*$/u, "");
  return label.trim();
}

/** The comparison key for a heading or an entry: case-folded, with runs of
 *  whitespace and the non-breaking space collapsed, so "Alpha  Chapter" and
 *  "Alpha chapter" are the same target. */
function key(text) {
  return String(text ?? "")
    .replace(/[   ]/gu, " ")
    .trim()
    .replace(/\s+/gu, " ")
    .toLowerCase();
}

/**
 * An index from heading text to the node that heading lives on, built from the
 * engine's `documentOutline()` rows (`"{level}\t{node}\t{text}"`).
 *
 * The FIRST heading with a given text wins, and the rows arrive in document
 * order, so a document that repeats a heading sends the reader to the first
 * one — which is where a table of contents' first entry for that text points.
 *
 * O(headings). Build it once per document version; a click must not rebuild it.
 *
 * @param {string[]} rows `documentOutline()` output
 * @returns {Map<string, string>} comparison key to node id
 */
export function buildHeadingIndex(rows) {
  const index = new Map();
  for (const row of rows ?? []) {
    const parts = String(row).split("\t");
    if (parts.length < 3) continue;
    const node = parts[1];
    const k = key(parts.slice(2).join("\t"));
    if (k !== "" && !index.has(k)) index.set(k, node);
  }
  return index;
}

/**
 * The node id the contents entry `text` names, or `null` when no heading
 * matches.
 *
 * Tries the whole label first, then the label with leading numbering removed —
 * a table generated with `\n` prints "Introduction" for a heading whose own text
 * is "1. Introduction", and vice versa — so both directions of that mismatch
 * resolve without either becoming ambiguous.
 *
 * O(1).
 *
 * @param {Map<string, string>} index from `buildHeadingIndex`
 * @param {string} text the entry paragraph's plain text
 * @returns {string | null}
 */
export function resolveTocTarget(index, text) {
  if (!index || index.size === 0) return null;
  const label = tocEntryLabel(text);
  if (label === "") return null;
  const exact = index.get(key(label));
  if (exact) return exact;
  const unnumbered = label.replace(/^\s*(?:\d+[.)]?)(?:\.\d+)*[.)]?\s+/u, "");
  if (unnumbered !== label) {
    const hit = index.get(key(unnumbered));
    if (hit) return hit;
  }
  return null;
}

/**
 * Whether `instructions` (the `fieldRangeEntries()` rows, `id\tinstruction`)
 * show the document holds a table of contents at all.
 *
 * The gate that keeps every other document from paying for this: no TOC field,
 * no style lookup on hover, no outline build, nothing.
 *
 * O(field ranges) — the engine reads its definition map and never walks the
 * body.
 *
 * @param {string[]} instructions
 * @returns {boolean}
 */
export function hasTableOfContents(instructions) {
  for (const row of instructions ?? []) {
    const instruction = String(row).split("\t").slice(1).join("\t");
    if (/(^|\s)TOC(\s|$)/i.test(instruction)) return true;
  }
  return false;
}

/**
 * The caching layer over the four pure helpers above, so a click stays O(1).
 *
 * Two caches, both keyed on the document revision the host supplies: whether
 * this document holds a TOC field at all (O(field ranges), and the gate that
 * makes every other document free), and the heading index (O(document) to
 * build — the same walk the Outline panel already does — and O(1) per use).
 *
 * @param {object} host
 * @param {() => object | null} host.doc the engine handle, or null
 * @param {() => string|number} host.revision a token that changes whenever the
 *   document changes OR is replaced, so both caches expire together. An edit
 *   counter alone is NOT enough: it restarts at zero on every open, and a cache
 *   keyed on it answers for the document before this one.
 */
export function createTocNavigator(host) {
  let index = null;
  let indexAt = null;
  let present = false;
  let presentAt = null;

  function hasToc() {
    const doc = host.doc();
    if (!doc) return false;
    if (presentAt !== host.revision()) {
      present = hasTableOfContents(doc.fieldRangeEntries());
      presentAt = host.revision();
    }
    return present;
  }

  return {
    /** The heading a contents entry at `node` names, as `{ node, label }`, or
     *  null when `node` is not a contents entry, or names no heading this
     *  document has. One style lookup and one text read, both bounded by the
     *  paragraph; returns immediately for a document with no TOC.
     *
     *  `offset` is the position within the entry the POINTER is at, and passing
     *  it narrows the answer to the entry's own content. Past the end of the
     *  line there is nothing to follow — Word's `\h` hyperlink stops at the page
     *  number too — and that blank strip to the right is the only place left to
     *  click if you want to put a caret in a contents entry and edit it. The
     *  ribbon command passes no offset, because a caret anywhere in the entry is
     *  enough for an explicit "go to the heading this entry names". */
    targetAt(node, offset = null) {
      const doc = host.doc();
      if (!node || !doc || !hasToc()) return null;
      if (!isTocEntryStyle(doc.paragraphStyleAt(node))) return null;
      const length = doc.paragraphLength(node);
      if (offset !== null && offset >= length) return null;
      const text = doc.copyText(node, 0, node, length);
      if (indexAt !== host.revision()) {
        index = buildHeadingIndex(doc.documentOutline());
        indexAt = host.revision();
      }
      const target = resolveTocTarget(index, text);
      return target ? { node: target, label: tocEntryLabel(text) } : null;
    },
  };
}
