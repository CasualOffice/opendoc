// The field vocabulary: which field kinds the editor offers, what each one is
// called, and what display text the HOST has to compute for it.
//
// Extracted from `main.js` for the ratchet (`module_seams.test.mjs`) when the
// Insert band gained its direct Page-number and Date buttons, and it is the
// right seam independently of that: this is data plus one pure formatter, so
// "does every kind the engine accepts have a label and a keyword set?" and "what
// does a date field cache?" are answerable in node, with no DOM and no engine.
//
// PAGE and NUMPAGES recompute at pagination and carry no cached text; the
// clock/context kinds (date/time/filename/author) cache an already-formatted
// string, because the engine reads no clock and no filesystem (see the
// `insertField` binding in `crates/casual-doc-wasm`). That split is the whole
// reason a host-side formatter exists at all, so it lives next to the table that
// declares it rather than in the 17k-line entry script.

/** Every field kind the editor offers, in the order the picker lists them.
 *  `note` is the picker's one-line explanation; `kw` feeds palette search. */
export const FIELD_KINDS = [
  { kind: "page", label: "Page number", icon: "tag", kw: "page number current", note: "Current page number" },
  { kind: "numpages", label: "Number of pages", icon: "tag", kw: "number of pages count total", note: "Total page count" },
  { kind: "date", label: "Date", icon: "calendar_today", kw: "date today", note: "Today’s date" },
  { kind: "time", label: "Time", icon: "schedule", kw: "time clock now", note: "Current time" },
  { kind: "filename", label: "File name", icon: "description", kw: "file name filename document", note: "This document’s file name" },
  { kind: "author", label: "Author", icon: "person", kw: "author name creator", note: "The active author" },
];

const LABELS = new Map(FIELD_KINDS.map((f) => [f.kind, f.label]));

/** The human name of a field kind, or `"field"` for a kind this build does not
 *  know — the status strip says "Inserted field" rather than "Inserted
 *  undefined" if the engine ever accepts a kind the picker has not learned. */
export function fieldLabel(kind) {
  return LABELS.get(kind) ?? "field";
}

/** The already-formatted display text a cached field kind shows. PAGE/NUMPAGES
 *  recompute at pagination and take no cached text (`undefined` → the binding's
 *  `None`). Date and time are formatted with the locale-default medium
 *  `Intl.DateTimeFormat`; `fileName` is the editor's current document name and
 *  `authorName` the active review author, both passed in because they are host
 *  state and this module holds none. O(1). */
export function fieldResultText(kind, { fileName = "", authorName = "" } = {}) {
  switch (kind) {
    case "date":
      return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date());
    case "time":
      return new Intl.DateTimeFormat(undefined, { timeStyle: "medium" }).format(new Date());
    case "filename":
      return fileName;
    case "author":
      return authorName.trim() || "You";
    default:
      return undefined; // page / numpages: engine recomputes, no cached text
  }
}
