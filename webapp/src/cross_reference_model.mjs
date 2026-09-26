// The vocabulary and the arithmetic behind Insert ▸ Caption and
// References ▸ Cross-reference (OO-005). No DOM, no engine, no catalogue: what
// lives here is the part of Word's two dialogs that is a RULE rather than a
// widget, so every rule is answerable in node.
//
// Three rules, and each one shipped wrong somewhere else before it was written
// down:
//
//   1. WHICH "Insert reference to" options a reference type offers. Word's list
//      changes with the type, and so does the WORDING of the same underlying
//      reference — `ParaNum` reads "Paragraph number" for a bookmark and
//      "Heading number" for a heading. ONLYOFFICE spells the same mapping in
//      `CrossReferenceDialog.js:289-350` (`refreshReferenceTypes`), and this is
//      that mapping, one table instead of a switch per call site.
//   2. WHEN "Include above/below" applies. ONLYOFFICE derives it in
//      `CrossReferenceDialog.js:436-447` (`onReferenceSelected`) and the rule is
//      not "always": a heading never offers it, a caption offers it only for a
//      page-number reference, and the others refuse it for the text and
//      above/below references themselves.
//   3. WHAT the caption will read as. Word prefills its Caption box with
//      "Figure 1" and the author types after it; ours takes only the author's
//      own text, so the composed string has to be shown somewhere or the label,
//      the number, the format and the chapter switch are all invisible until
//      after the insert. `captionPreview` composes it, and the sequence number
//      it is given comes from `nextCaptionNumber` counting the captions that
//      already precede the target — the same order the engine's SEQ field will
//      resolve to.
//
// DELIBERATE OMISSION: Word and ONLYOFFICE both offer "Numbered item" as a
// reference type (`CrossReferenceDialog.js:132`, `textParagraph`). The engine's
// `referenceTargets(kind)` takes "heading", "bookmark", "footnote", "endnote" or
// a caption label and has no numbered-item enumeration, so the type is absent
// rather than present-and-empty. It belongs here the day the engine can list
// numbered paragraphs; `REFERENCE_KINDS` is the one place that has to change.

/** The reference types the engine can enumerate targets for, in Word's order. */
export const REFERENCE_KINDS = Object.freeze(["heading", "bookmark", "footnote", "endnote"]);

/** The stand-in kind for "one of the document's caption labels". A caption
 *  label is document DATA ("Figure", "Table", "Abbildung"), so it cannot be a
 *  member of the list above — but every caption label offers the same options,
 *  which is why the mapping keys on this one value. */
export const CAPTION_KIND = "caption";

/** `referenceTo` values, in the order Word's combo lists them per type.
 *
 *  Mirrors `CrossReferenceDialog.js:289-350`. The engine's `referenceTo` names
 *  replace ONLYOFFICE's `c_oAscDocumentRefenceToType` numbers one for one,
 *  except for its note-only `NoteNumber`/`NoteNumberFormatted`: the engine's
 *  operation does not take those, so a footnote's number reference is
 *  `paragraphNumber` wearing a footnote's wording — which is the same thing
 *  ONLYOFFICE does for a heading, where `ParaNum` displays as "Heading number".
 *  The formatted variant ("Footnote number (formatted)", which carries the
 *  note's own superscript formatting) has no engine value at all and is absent
 *  rather than mapped onto something that would insert plain text. */
const REFERENCE_TO = Object.freeze({
  [CAPTION_KIND]: Object.freeze([
    "entireCaption",
    "labelAndNumber",
    "captionText",
    "pageNumber",
    "aboveBelow",
  ]),
  heading: Object.freeze([
    "paragraphText",
    "pageNumber",
    "paragraphNumber",
    "paragraphNumberNoContext",
    "paragraphNumberFullContext",
    "aboveBelow",
  ]),
  bookmark: Object.freeze([
    "paragraphText",
    "pageNumber",
    "paragraphNumber",
    "paragraphNumberNoContext",
    "paragraphNumberFullContext",
    "aboveBelow",
  ]),
  footnote: Object.freeze(["paragraphNumber", "pageNumber", "aboveBelow"]),
  endnote: Object.freeze(["paragraphNumber", "pageNumber", "aboveBelow"]),
});

/** The wording of one `referenceTo` value FOR ONE TYPE, as an i18n key suffix.
 *  Word does not call `paragraphText` the same thing for a heading ("Heading
 *  text"), a bookmark ("Bookmark text") and a caption ("Entire caption"); a
 *  single label per value would be wrong on two of the three. */
const REFERENCE_TO_NAMES = Object.freeze({
  [CAPTION_KIND]: Object.freeze({
    entireCaption: "entireCaption",
    labelAndNumber: "labelAndNumber",
    captionText: "captionText",
    pageNumber: "pageNumber",
    aboveBelow: "aboveBelow",
  }),
  heading: Object.freeze({
    paragraphText: "headingText",
    pageNumber: "pageNumber",
    paragraphNumber: "headingNumber",
    paragraphNumberNoContext: "headingNumberNoContext",
    paragraphNumberFullContext: "headingNumberFullContext",
    aboveBelow: "aboveBelow",
  }),
  bookmark: Object.freeze({
    paragraphText: "bookmarkText",
    pageNumber: "pageNumber",
    paragraphNumber: "paragraphNumber",
    paragraphNumberNoContext: "paragraphNumberNoContext",
    paragraphNumberFullContext: "paragraphNumberFullContext",
    aboveBelow: "aboveBelow",
  }),
  footnote: Object.freeze({
    paragraphNumber: "footnoteNumber",
    pageNumber: "pageNumber",
    aboveBelow: "aboveBelow",
  }),
  endnote: Object.freeze({
    paragraphNumber: "endnoteNumber",
    pageNumber: "pageNumber",
    aboveBelow: "aboveBelow",
  }),
});

/** Every distinct wording this mapping can ask for, so the catalogue's coverage
 *  is provable from the table rather than from reading the call site. */
export function referenceToNameKeys() {
  const keys = new Set();
  for (const names of Object.values(REFERENCE_TO_NAMES)) {
    for (const name of Object.values(names)) keys.add(name);
  }
  return [...keys].sort();
}

/** `[{ referenceTo, name }]` for `kind`, in Word's order. `name` is the wording
 *  suffix the caller turns into a label; an unknown kind yields nothing rather
 *  than throwing, because the kind can come from document data. */
export function referenceToOptions(kind) {
  const values = REFERENCE_TO[kind] ?? [];
  const names = REFERENCE_TO_NAMES[kind] ?? {};
  return values.map((referenceTo) => ({ referenceTo, name: names[referenceTo] }));
}

/** Whether Word offers "Include above/below" for this pair.
 *
 *  `CrossReferenceDialog.js:436-447`, inverted (theirs computes `disable`):
 *  never for a heading; for a caption only alongside a page number; otherwise
 *  for anything but the text and above/below references, which already say
 *  where the target is. */
export function aboveBelowEnabled(kind, referenceTo) {
  if (kind === "heading") return false;
  if (kind === CAPTION_KIND) return referenceTo === "pageNumber";
  if (!Object.hasOwn(REFERENCE_TO, kind)) return false;
  return referenceTo !== "paragraphText" && referenceTo !== "aboveBelow";
}

/** The numbering formats Word's Caption dialog offers, in its order
 *  (`CaptionDialog.js:259-267`). The names are the engine's. */
export const NUMBER_FORMATS = Object.freeze([
  "arabic",
  "lowerLetter",
  "upperLetter",
  "lowerRoman",
  "upperRoman",
]);

/** Word's five chapter separators (`CaptionDialog.js:315-321`), with the
 *  character each one writes. The engine takes the NAME; the preview needs the
 *  character, and deriving one from the other in two places is how a dialog
 *  comes to show "1-1" and write "1.1". */
export const SEPARATORS = Object.freeze(["hyphen", "period", "colon", "emDash", "enDash"]);

const SEPARATOR_CHARS = Object.freeze({
  hyphen: "-",
  period: ".",
  colon: ":",
  emDash: "—",
  enDash: "–",
});

/** The character a separator name writes, or the hyphen for an unknown name. */
export function separatorChar(name) {
  return SEPARATOR_CHARS[name] ?? SEPARATOR_CHARS.hyphen;
}

const ROMAN = Object.freeze([
  [1000, "M"],
  [900, "CM"],
  [500, "D"],
  [400, "CD"],
  [100, "C"],
  [90, "XC"],
  [50, "L"],
  [40, "XL"],
  [10, "X"],
  [9, "IX"],
  [5, "V"],
  [4, "IV"],
  [1, "I"],
]);

function roman(value) {
  let left = value;
  let out = "";
  for (const [size, glyph] of ROMAN) {
    while (left >= size) {
      out += glyph;
      left -= size;
    }
  }
  return out;
}

/** Word's `upperLetter` sequence: A…Z, then AA, AB — NOT base-26 arithmetic,
 *  which would produce "BA" for 27 because there is no zero digit. */
function letters(value) {
  const index = value - 1;
  const cycle = Math.floor(index / 26) + 1;
  return String.fromCharCode(65 + (index % 26)).repeat(cycle);
}

/** One sequence number rendered in a caption number format. Values below 1 —
 *  which cannot happen for a real SEQ result — render as "1" rather than as an
 *  empty string, so a preview never silently loses its number. */
export function formatSequenceNumber(value, format) {
  const n = Number.isFinite(value) && value >= 1 ? Math.floor(value) : 1;
  switch (format) {
    case "lowerRoman":
      return roman(n).toLowerCase();
    case "upperRoman":
      return roman(n);
    case "lowerLetter":
      return letters(n).toLowerCase();
    case "upperLetter":
      return letters(n);
    default:
      return String(n);
  }
}

/** Splits one tab-separated engine row into exactly `count` fields, keeping any
 *  further tabs inside the last one. The engine promises the text field has its
 *  tabs replaced by spaces; this does not depend on that promise. */
function fields(row, count) {
  const out = [];
  let at = 0;
  for (let i = 0; i < count - 1; i += 1) {
    const tab = row.indexOf("\t", at);
    if (tab === -1) {
      out.push(row.slice(at));
      at = row.length;
      continue;
    }
    out.push(row.slice(at, tab));
    at = tab + 1;
  }
  out.push(row.slice(at));
  return out;
}

/** `captionEntries()` rows — `"{node}\t{label}\t{number}\t{text}"` — as objects,
 *  in document order. A row with no node is dropped: it cannot be a target and
 *  it cannot be counted. */
export function parseCaptionEntries(rows) {
  const out = [];
  for (const row of rows ?? []) {
    const [node, label, number, text] = fields(String(row), 4);
    if (!node) continue;
    out.push({ node, label, number, text });
  }
  return out;
}

/** `referenceTargets(kind)` rows — `"{node}\t{text}"` — as objects, in document
 *  order. */
export function parseReferenceTargets(rows) {
  const out = [];
  for (const row of rows ?? []) {
    const [node, text] = fields(String(row), 2);
    if (!node) continue;
    out.push({ node, text });
  }
  return out;
}

/** The label list the combo offers: the document's own labels (the engine
 *  already unions Word's three built-ins into them) plus any label the author
 *  has added in this session but not yet used, de-duplicated and sorted the way
 *  ONLYOFFICE sorts its own (`CrossReferenceDialog.js:121-126`, case-insensitive).
 *
 *  Labels are document CONTENT, not chrome: "Figure" is literal text in the
 *  paragraph the engine writes, so nothing here is translated. */
export function mergeCaptionLabels(fromDocument, addedInSession = []) {
  const seen = new Map();
  for (const value of [...(fromDocument ?? []), ...(addedInSession ?? [])]) {
    const label = String(value ?? "").trim();
    if (!label) continue;
    const key = label.toLocaleLowerCase();
    if (!seen.has(key)) seen.set(key, label);
  }
  return [...seen.values()].sort((a, b) =>
    a.localeCompare(b, undefined, { sensitivity: "base" }),
  );
}

/** The sequence number a new caption for `label` will take, given the captions
 *  already in the document and where the new one goes.
 *
 *  `targetIndex` is how many EXISTING captions precede the insertion point in
 *  document order — `captionEntries()` is in document order, so that is an index
 *  into the array. Word's SEQ counts from the start of the document, so a
 *  caption inserted in the middle takes the next number at ITS position and
 *  renumbers everything after it; counting the whole document instead would
 *  preview "4" for what will render as "2". */
export function nextCaptionNumber(entries, label, targetIndex) {
  const upTo = Number.isFinite(targetIndex) ? targetIndex : (entries?.length ?? 0);
  let count = 0;
  for (let i = 0; i < (entries?.length ?? 0) && i < upTo; i += 1) {
    if (entries[i].label === label) count += 1;
  }
  return count + 1;
}

/** How many existing captions precede `node` in document order. Returns the
 *  total when the node is not one of them, which is the "append at the end"
 *  answer and the right default for a target with no caption after it. */
export function captionsBefore(entries, node) {
  const list = entries ?? [];
  for (let i = 0; i < list.length; i += 1) {
    if (list[i].node === node) return i;
  }
  return list.length;
}

/** The whole caption, as it will read once inserted.
 *
 *  Word's own Caption box holds "Figure 1" and the author types after it, so
 *  what they see is the composed string. Our input holds only the author's
 *  text, so this is what puts the rest of it back on screen — and it is
 *  deliberately a plain concatenation, because that is exactly what the engine
 *  writes: a literal label run, the SEQ field, then the author's text verbatim.
 *  No separator is invented between the number and the text; typing ": Wiring
 *  diagram" is how "Figure 1: Wiring diagram" happens, in Word too.
 *
 *  `chapter` is the chapter number's PLACEHOLDER when "Include chapter number"
 *  is on — the real value comes from a STYLEREF to the nearest heading, which
 *  only the engine can resolve. Passing "" means no chapter number. */
export function captionPreview({
  label = "",
  number = "1",
  text = "",
  excludeLabel = false,
  chapter = "",
  separator = "hyphen",
} = {}) {
  const numbered = chapter ? `${chapter}${separatorChar(separator)}${number}` : String(number);
  const head = excludeLabel || !label ? numbered : `${label} ${numbered}`;
  return `${head}${text ?? ""}`;
}
