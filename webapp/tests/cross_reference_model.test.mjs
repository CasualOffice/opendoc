// The rules behind Insert caption and Cross-reference (OO-005).
//
// Word's two dialogs are mostly a table and some arithmetic, and ONLYOFFICE
// keeps both inside a Backbone view (`CrossReferenceDialog.js`,
// `CaptionDialog.js`) where nothing can reach them. Ours are in a module with no
// DOM, no engine and no catalogue, which is what lets every one of them be
// driven here — including the ones that are easy to get subtly wrong and
// impossible to notice: that the same reference is WORDED differently per type,
// that Word never offers above/below for a heading, that "AA" follows "Z", and
// that a caption inserted in the middle of a document takes the number at ITS
// position rather than the next one overall.
import assert from "node:assert/strict";
import test from "node:test";

const {
  CAPTION_KIND,
  NUMBER_FORMATS,
  REFERENCE_KINDS,
  SEPARATORS,
  aboveBelowEnabled,
  captionPreview,
  captionsBefore,
  formatSequenceNumber,
  mergeCaptionLabels,
  nextCaptionNumber,
  parseCaptionEntries,
  parseReferenceTargets,
  referenceToNameKeys,
  referenceToOptions,
  separatorChar,
} = await import("../src/cross_reference_model.mjs");

const { EN_STRINGS } = await import("../src/en_strings.mjs");

/** Exactly the `referenceTo` values the engine's `insertCrossReference`
 *  documents. Written out here rather than imported, so a value the mapping
 *  invents — or a rename on one side only — fails instead of reaching the engine
 *  and being refused at runtime, where the user sees it. */
const ENGINE_REFERENCE_TO = new Set([
  "entireCaption",
  "labelAndNumber",
  "captionText",
  "pageNumber",
  "aboveBelow",
  "paragraphText",
  "paragraphNumber",
  "paragraphNumberNoContext",
  "paragraphNumberFullContext",
]);

const values = (kind) => referenceToOptions(kind).map((option) => option.referenceTo);
const names = (kind) => referenceToOptions(kind).map((option) => option.name);

// ---- The option mapping, per reference type --------------------------------

test("a caption offers Word's five caption references, in Word's order", () => {
  // `CrossReferenceDialog.js:292-298`: Text, OnlyLabelAndNumber,
  // OnlyCaptionText, PageNum, AboveBelow.
  assert.deepEqual(values(CAPTION_KIND), [
    "entireCaption",
    "labelAndNumber",
    "captionText",
    "pageNumber",
    "aboveBelow",
  ]);
});

test("a heading offers six, and a note offers three", () => {
  // `:317-324` and `:335-341`. A note has no text and no paragraph numbering to
  // refer to, so its list is the short one.
  assert.deepEqual(values("heading"), [
    "paragraphText",
    "pageNumber",
    "paragraphNumber",
    "paragraphNumberNoContext",
    "paragraphNumberFullContext",
    "aboveBelow",
  ]);
  assert.deepEqual(values("footnote"), ["paragraphNumber", "pageNumber", "aboveBelow"]);
  assert.deepEqual(values("endnote"), ["paragraphNumber", "pageNumber", "aboveBelow"]);
});

test("ONE reference value is worded differently per type, as Word words it", () => {
  // This is the assertion that a single label per value would fail. ONLYOFFICE
  // shows `ParaNum` as "Paragraph number" for a bookmark (`:327`) and as
  // "Heading number" for a heading (`:320`); the note lists reuse it again for
  // the note's own number.
  assert.equal(names("heading")[2], "headingNumber");
  assert.equal(names("bookmark")[2], "paragraphNumber");
  assert.equal(names("footnote")[0], "footnoteNumber");
  assert.equal(names("endnote")[0], "endnoteNumber");
  // …and `paragraphText` likewise: "Heading text", "Bookmark text", "Entire
  // caption" are one engine value seen from three reference types.
  assert.equal(names("heading")[0], "headingText");
  assert.equal(names("bookmark")[0], "bookmarkText");
  assert.equal(names(CAPTION_KIND)[0], "entireCaption");
});

test("every value the mapping can produce is one the engine accepts", () => {
  const unknown = [];
  for (const kind of [...REFERENCE_KINDS, CAPTION_KIND]) {
    for (const value of values(kind)) {
      if (!ENGINE_REFERENCE_TO.has(value)) unknown.push(`${kind}: ${value}`);
    }
  }
  assert.deepEqual(unknown, []);
});

test("every option has a name, and every name has an English string", () => {
  const missing = [];
  for (const kind of [...REFERENCE_KINDS, CAPTION_KIND]) {
    for (const { referenceTo, name } of referenceToOptions(kind)) {
      if (!name) missing.push(`${kind}/${referenceTo} has no wording`);
    }
  }
  for (const name of referenceToNameKeys()) {
    const key = `crossRefDialog.refTo.${name}`;
    if (!Object.hasOwn(EN_STRINGS, key)) missing.push(`${key} is not in EN_STRINGS`);
  }
  assert.deepEqual(missing, []);
});

test("an unknown reference type offers nothing rather than throwing", () => {
  // The type list carries one row per caption LABEL, which is document data, so
  // a kind this table has never seen is reachable from a document rather than
  // from a bug.
  assert.deepEqual(referenceToOptions("Abbildung"), []);
  assert.deepEqual(referenceToOptions(undefined), []);
});

test("every reference kind has a 'For which …' heading string", () => {
  const missing = [];
  for (const kind of [...REFERENCE_KINDS, "caption"]) {
    const key = `crossRefDialog.forWhich.${kind}`;
    if (!Object.hasOwn(EN_STRINGS, key)) missing.push(key);
  }
  for (const kind of REFERENCE_KINDS) {
    const key = `crossRefDialog.type.${kind}`;
    if (!Object.hasOwn(EN_STRINGS, key)) missing.push(key);
  }
  assert.deepEqual(missing, []);
});

// ---- "Include above/below" --------------------------------------------------

test("Word never offers above/below for a heading", () => {
  // `CrossReferenceDialog.js:441`: `disable = type === 1 || …`, and `:442` even
  // clears the box. It is the one type where the switch is unconditionally off.
  for (const value of values("heading")) {
    assert.equal(aboveBelowEnabled("heading", value), false, value);
  }
});

test("a caption offers above/below only alongside a page number", () => {
  // `:441`: `(type==5 && refType!==PageNum)` disables it.
  assert.equal(aboveBelowEnabled(CAPTION_KIND, "pageNumber"), true);
  for (const value of ["entireCaption", "labelAndNumber", "captionText", "aboveBelow"]) {
    assert.equal(aboveBelowEnabled(CAPTION_KIND, value), false, value);
  }
});

test("a bookmark or note offers it for everything but the text and above/below", () => {
  // `:441`: `(type<5) && (refType==Text || refType==AboveBelow)` disables it —
  // both already say where the target is.
  assert.equal(aboveBelowEnabled("bookmark", "pageNumber"), true);
  assert.equal(aboveBelowEnabled("bookmark", "paragraphNumberFullContext"), true);
  assert.equal(aboveBelowEnabled("bookmark", "paragraphText"), false);
  assert.equal(aboveBelowEnabled("bookmark", "aboveBelow"), false);
  assert.equal(aboveBelowEnabled("footnote", "paragraphNumber"), true);
  assert.equal(aboveBelowEnabled("endnote", "aboveBelow"), false);
});

test("an unknown kind does not offer it", () => {
  assert.equal(aboveBelowEnabled("Abbildung", "pageNumber"), false);
});

// ---- Number formats and separators -----------------------------------------

test("the five caption number formats are Word's five", () => {
  // `CaptionDialog.js:259-267`, in its order.
  assert.deepEqual(NUMBER_FORMATS, [
    "arabic",
    "lowerLetter",
    "upperLetter",
    "lowerRoman",
    "upperRoman",
  ]);
});

test("arabic, roman and letter sequences render the way Word renders them", () => {
  assert.equal(formatSequenceNumber(1, "arabic"), "1");
  assert.equal(formatSequenceNumber(14, "arabic"), "14");
  assert.equal(formatSequenceNumber(4, "upperRoman"), "IV");
  assert.equal(formatSequenceNumber(9, "lowerRoman"), "ix");
  assert.equal(formatSequenceNumber(1944, "upperRoman"), "MCMXLIV");
  assert.equal(formatSequenceNumber(1, "upperLetter"), "A");
  assert.equal(formatSequenceNumber(26, "upperLetter"), "Z");
  assert.equal(formatSequenceNumber(2, "lowerLetter"), "b");
});

test("the letter sequence is Word's A…Z, AA, AB — not base-26 arithmetic", () => {
  // Base-26 with no zero digit gives "BA" for 27, which is what a naive
  // implementation produces and what Word does not do.
  assert.equal(formatSequenceNumber(27, "upperLetter"), "AA");
  assert.equal(formatSequenceNumber(28, "upperLetter"), "BB");
  assert.equal(formatSequenceNumber(53, "lowerLetter"), "aaa");
});

test("a number below one still renders as a number", () => {
  // Cannot happen for a real SEQ result; a preview that silently lost its
  // number would be worse than one that reads "1".
  assert.equal(formatSequenceNumber(0, "arabic"), "1");
  assert.equal(formatSequenceNumber(Number.NaN, "upperRoman"), "I");
  assert.equal(formatSequenceNumber(undefined, "lowerLetter"), "a");
});

test("each separator name writes the character Word writes", () => {
  // `CaptionDialog.js:315-321`. The engine takes the NAME and the preview needs
  // the CHARACTER; deriving one from the other twice is how a dialog comes to
  // show "1-1" and write "1.1".
  assert.deepEqual(SEPARATORS, ["hyphen", "period", "colon", "emDash", "enDash"]);
  assert.deepEqual(
    SEPARATORS.map(separatorChar),
    ["-", ".", ":", "—", "–"],
  );
  assert.equal(separatorChar("nonsense"), "-");
});

// ---- The caption's sequence number -----------------------------------------

const ENTRIES = parseCaptionEntries([
  "n1\tFigure\t1\t: first figure",
  "n2\tTable\t1\t: a table",
  "n3\tFigure\t2\t: second figure",
  "n4\tFigure\t3\t: third figure",
]);

test("a caption at the end takes the next number for its own label", () => {
  assert.equal(nextCaptionNumber(ENTRIES, "Figure", ENTRIES.length), 4);
  assert.equal(nextCaptionNumber(ENTRIES, "Table", ENTRIES.length), 2);
});

test("a caption in the MIDDLE takes the number at its position, not overall", () => {
  // Word's SEQ counts from the start of the document and renumbers everything
  // after the insertion, so a figure inserted after "n1" renders as 2 — a
  // preview that counted the whole document would promise 4.
  assert.equal(nextCaptionNumber(ENTRIES, "Figure", 1), 2);
  assert.equal(nextCaptionNumber(ENTRIES, "Figure", 0), 1);
});

test("a label the document has never used starts at one", () => {
  assert.equal(nextCaptionNumber(ENTRIES, "Abbildung", ENTRIES.length), 1);
  assert.equal(nextCaptionNumber([], "Figure", 0), 1);
});

test("the insertion position is how many captions precede the target", () => {
  assert.equal(captionsBefore(ENTRIES, "n1"), 0);
  assert.equal(captionsBefore(ENTRIES, "n3"), 2);
  // A block that is not itself a caption appends at the end, which is the right
  // default for a target with no caption after it.
  assert.equal(captionsBefore(ENTRIES, "somewhere-else"), 4);
  assert.equal(captionsBefore([], "n1"), 0);
});

// ---- The composed caption --------------------------------------------------

test("the preview reads as the caption will read", () => {
  assert.equal(
    captionPreview({ label: "Figure", number: "1", text: ": Wiring diagram" }),
    "Figure 1: Wiring diagram",
  );
});

test("the author's text is appended VERBATIM, with no separator invented", () => {
  // The engine writes a literal label run, the SEQ field, then the author's text
  // exactly as typed — which is also what Word does, where ": " is something the
  // author types. A preview that helpfully inserted ": " would describe a
  // document we do not write.
  assert.equal(captionPreview({ label: "Table", number: "2", text: "" }), "Table 2");
  assert.equal(captionPreview({ label: "Table", number: "2", text: " — totals" }), "Table 2 — totals");
  assert.equal(captionPreview({ label: "Table", number: "2" }), "Table 2");
});

test("excluding the label keeps the number", () => {
  assert.equal(
    captionPreview({ label: "Figure", number: "4", text: ": detail", excludeLabel: true }),
    "4: detail",
  );
});

test("a chapter number goes in front, joined by the chosen separator", () => {
  assert.equal(
    captionPreview({ label: "Table", number: "3", chapter: "#", separator: "period" }),
    "Table #.3",
  );
  assert.equal(
    captionPreview({ label: "Table", number: "3", chapter: "2", separator: "emDash" }),
    "Table 2—3",
  );
  // "" means the switch is off, and then no separator appears at all.
  assert.equal(
    captionPreview({ label: "Table", number: "3", chapter: "", separator: "period" }),
    "Table 3",
  );
});

test("no label yields the number alone rather than a leading space", () => {
  assert.equal(captionPreview({ label: "", number: "1", text: ": x" }), "1: x");
  assert.equal(captionPreview(), "1");
});

// ---- Parsing the engine's rows ---------------------------------------------

test("caption entries keep tabs inside the text field", () => {
  const [entry] = parseCaptionEntries(["n9\tFigure\tIV\tone\ttwo"]);
  assert.deepEqual(entry, { node: "n9", label: "Figure", number: "IV", text: "one\ttwo" });
});

test("a row with no node is dropped: it can be neither counted nor targeted", () => {
  assert.deepEqual(parseCaptionEntries(["\tFigure\t1\tx"]), []);
  assert.deepEqual(parseReferenceTargets(["\tA heading"]), []);
  assert.deepEqual(parseReferenceTargets(undefined), []);
  assert.deepEqual(parseCaptionEntries(undefined), []);
});

test("reference targets keep their text whole", () => {
  assert.deepEqual(parseReferenceTargets(["n1\t1.2 Scope and\tpurpose"]), [
    { node: "n1", text: "1.2 Scope and\tpurpose" },
  ]);
});

test("a short row does not throw, it just has empty fields", () => {
  assert.deepEqual(parseCaptionEntries(["n1\tFigure"]), [
    { node: "n1", label: "Figure", number: "", text: "" },
  ]);
});

// ---- The label list --------------------------------------------------------

test("the label list is the document's plus the session's, sorted and deduped", () => {
  assert.deepEqual(
    mergeCaptionLabels(["Table", "Figure", "Equation"], ["Illustration"]),
    ["Equation", "Figure", "Illustration", "Table"],
  );
});

test("a label is deduped case-insensitively, keeping the first spelling", () => {
  // ONLYOFFICE sorts case-insensitively (`CrossReferenceDialog.js:121-126`), and
  // "Figure" and "figure" are one label in a document, not two.
  assert.deepEqual(mergeCaptionLabels(["Figure"], ["figure", "FIGURE"]), ["Figure"]);
});

test("blank and whitespace-only labels never reach the combo", () => {
  assert.deepEqual(mergeCaptionLabels(["Figure", "", "   ", null, undefined], []), ["Figure"]);
  assert.deepEqual(mergeCaptionLabels(["  Table  "], []), ["Table"]);
});

test("nothing at all is an empty list, not a crash", () => {
  assert.deepEqual(mergeCaptionLabels(undefined, undefined), []);
  assert.deepEqual(mergeCaptionLabels([]), []);
});
