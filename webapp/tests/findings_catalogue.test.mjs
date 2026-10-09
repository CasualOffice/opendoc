// A compatibility finding is named in words a reader shares (`109` FID-AT-05).
//
// The owner opened `sample.docx` and the findings dialog said
// `cNvPr/@name ×2`, `fontScheme/@name ×1`, `docx.rsid ×165` under a headline of
// 183 things "kept in the file, not shown or editable here" — XML vocabulary,
// and a number that was mostly Word's revision-save bookkeeping. These are the
// guarantees that replace it, each asserted as a guarantee rather than as a
// mechanism:
//
//   * every feature that report carried, and every frequent feature of the
//     committed corpus, reads as words — not as its id;
//   * an id nobody has catalogued still says which part and what kind of thing,
//     because the engine's vocabulary is open-ended;
//   * Word's own bookkeeping is a DECLARED list, pinned here, and it is kept out
//     of the headline count without being dropped;
//   * every word is answered by every one of the nineteen catalogues, and not
//     with an English copy.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  FEATURE_CATALOGUE,
  catalogueEntry,
  catalogueKeys,
  describeFinding,
  isBookkeeping,
} from "../src/findings_catalogue.mjs";
import { findingTotals, groupFindings } from "../src/compat_findings.mjs";
import { compatibilityOccurrenceCount } from "../src/format_io.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale, t } from "../src/i18n.mjs";

const LOCALES = join(dirname(fileURLToPath(import.meta.url)), "..", "locales");

setCatalogue("en", EN_STRINGS);
setLocale("en");

/** One report entry, in the shape `importReportJson` carries. */
const finding = (feature, occurrences, partName, modelOutcome = "omitted", retentionOutcome = "preserved") => ({
  feature,
  occurrences,
  location: { partName, namespace: null, localName: null, attributeName: null },
  modelOutcome,
  retentionOutcome,
});

/** `sample.docx`'s import report, entry for entry, as the owner saw it. */
const SAMPLE = [
  finding("cNvPr/@name", 2, "word/document.xml", "degraded"),
  finding("fontScheme/@name", 1, "word/theme/theme1.xml", "degraded"),
  finding("theme/@name", 1, "word/theme/theme1.xml", "degraded"),
  finding("HyperlinksChanged", 1, null),
  finding("customXml/item1.xml", 1, "customXml/item1.xml"),
  finding("customXml/itemProps1.xml", 1, "customXml/itemProps1.xml"),
  finding("decimalSymbol", 1, "word/settings.xml"),
  finding("defaultImageDpi", 1, "word/settings.xml"),
  finding("doNotAutoCompressPictures", 1, "word/settings.xml"),
  finding("docId", 1, "word/settings.xml"),
  finding("docProps/thumbnail.jpeg", 1, "docProps/thumbnail.jpeg"),
  finding("docx.rsid", 165, null),
  finding("graphicFrameLocks", 2, "word/document.xml"),
  finding("listSeparator", 1, "word/settings.xml"),
  finding("mathPr", 1, "word/settings.xml"),
  finding("objectDefaults", 1, "word/theme/theme1.xml"),
  finding("savePreviewPicture", 1, "word/settings.xml"),
  finding("shapeDefaults", 1, "word/settings.xml"),
  finding("useFELayout", 1, "word/settings.xml"),
  finding("word/stylesWithEffects.xml", 1, "word/stylesWithEffects.xml"),
  finding("word/webSettings.xml", 1, "word/webSettings.xml"),
];
const SAMPLE_REPORT = JSON.stringify({ entries: SAMPLE });

/** The committed corpus's findings by document frequency, measured with
 *  `cargo run -p casual-doc-import --example corpus_report` over the 43
 *  committed `.docx` (39 import). Every feature it raised is here. */
const CORPUS = [
  ["cNvPr/@name", "word/document.xml"],
  ["formProt", "word/document.xml"],
  ["drawing", "word/document.xml"],
  ["graphicFrameLocks", "word/document.xml"],
  ["customXml/item1.xml", "customXml/item1.xml"],
  ["shape", "word/header1.xml"],
  ["textpath", "word/header1.xml"],
  ["fill", "word/header1.xml"],
  ["path", "word/header1.xml"],
  ["shapetype", "word/header1.xml"],
  ["picLocks", "word/document.xml"],
  ["textbox", "word/document.xml"],
  ["chart.trendline", "word/charts/chart1.xml"],
  ["word/charts/colors1.xml", "word/charts/colors1.xml"],
  ["word/charts/style1.xml", "word/charts/style1.xml"],
  ["word/embeddings/Microsoft_Excel_Worksheet1.xlsx", "word/embeddings/Microsoft_Excel_Worksheet1.xlsx"],
];

/** What the row says first, in English. */
const words = (entry) => t(describeFinding(entry).words);
const where = (entry) => {
  const place = describeFinding(entry).where;
  return place ? t(place.key, place.params) : null;
};

test("every feature sample.docx reported is named in words, not by its XML id", () => {
  const unnamed = SAMPLE.filter((entry) => !describeFinding(entry).known).map((entry) => entry.feature);
  assert.deepEqual(unnamed, [], "these fell through to the fallback");
  for (const entry of SAMPLE) {
    const text = words(entry);
    assert.notEqual(text, entry.feature);
    assert.ok(!text.startsWith("findings."), `${entry.feature} rendered its catalogue key: ${text}`);
    assert.ok(!/[/@]|\.xml\b/.test(text), `${entry.feature} reads as markup: ${text}`);
  }
  // The words Word's own interface uses, where it has them.
  assert.equal(words(SAMPLE[0]), "Object name shown in Word's Selection Pane");
  assert.equal(words(finding("decimalSymbol", 1, "word/settings.xml")), "Decimal symbol used in calculations");
  assert.equal(words(finding("theme/@name", 1, "word/theme/theme1.xml")), "Document theme name");
  assert.match(words(finding("graphicFrameLocks", 1, "word/document.xml")), /Lock aspect ratio/);
});

test("the most frequent features of the committed corpus are named in words", () => {
  const unnamed = CORPUS.filter(([feature, part]) => !describeFinding(finding(feature, 1, part)).known);
  assert.deepEqual(unnamed, []);
});

test("a numbered part and a family of ids answer one entry", () => {
  assert.equal(catalogueEntry("customXml/item7.xml").key, catalogueEntry("customXml/item1.xml").key);
  assert.equal(catalogueEntry("customXml/itemProps12.xml").key, "findings.feature.customXmlProperties");
  assert.equal(catalogueEntry("docProps/thumbnail.png").key, "findings.feature.thumbnail");
  assert.equal(catalogueEntry("documentProtection/@hashValue").key, "findings.feature.restrictEditingPassword");
  assert.equal(catalogueEntry("chart.dLbls").key, "findings.feature.chartDetail");
  assert.equal(catalogueEntry("chart.trendline").key, "findings.feature.chartTrendline", "an exact entry beats its prefix");
});

test("Word's own bookkeeping is exactly this declared list", () => {
  // Adding an id here is a decision that a reader would not miss it, and it
  // takes that id out of the headline count. Make it here, visibly.
  const declared = Object.entries(FEATURE_CATALOGUE)
    .filter(([, value]) => value.bookkeeping)
    .map(([id]) => id)
    .sort();
  assert.deepEqual(declared, [
    "HyperlinksChanged",
    "docId",
    "docProps/thumbnail.*",
    "docx.export.stale.styles_with_effects",
    "docx.export.stale.thumbnail",
    "docx.rsid",
    "p/@paraId",
    "p/@textId",
    "savePreviewPicture",
    "tr/@paraId",
    "tr/@textId",
    "word/stylesWithEffects.xml",
    "word/webSettings.xml",
  ]);
});

test("revision-save ids are bookkeeping; the theme's name and the document's content are not", () => {
  assert.equal(isBookkeeping(finding("docx.rsid", 165, null)), true);
  assert.equal(isBookkeeping(finding("docProps/thumbnail.jpeg", 1, "docProps/thumbnail.jpeg")), true);
  for (const feature of ["theme/@name", "cNvPr/@name", "customXml/item1.xml", "decimalSymbol", "formProt"]) {
    assert.equal(isBookkeeping(finding(feature, 1, null)), false, feature);
  }
  // An id nobody has looked at is never presumed harmless.
  assert.equal(isBookkeeping(finding("rsidSomethingNew", 1, "word/settings.xml")), false);
});

test("the headline count excludes bookkeeping, and nothing is dropped from the dialog", () => {
  assert.deepEqual(findingTotals(SAMPLE_REPORT), { headline: 16, bookkeeping: 171, entries: 21 });
  const groups = groupFindings(SAMPLE_REPORT);
  const preserved = groups.find((group) => group.id === "preserved");
  assert.equal(preserved.total, 12, "183 kept, of which 171 are Word's own bookkeeping");
  assert.equal(preserved.bookkeeping.total, 171);
  assert.ok(preserved.bookkeeping.entries.some((entry) => entry.feature === "docx.rsid"));
  assert.ok(!preserved.entries.some((entry) => entry.feature === "docx.rsid"));
  // Every occurrence the engine counted is in exactly one place.
  const listed = groups.reduce((sum, group) => sum + group.total + group.bookkeeping.total, 0);
  assert.equal(listed, compatibilityOccurrenceCount(SAMPLE_REPORT));
  const rows = groups.reduce((sum, group) => sum + group.entries.length + group.bookkeeping.entries.length, 0);
  assert.equal(rows, SAMPLE.length);
});

test("bookkeeping stays under the heading that says what happened to it", () => {
  // After an edited save the stale thumbnail is NOT kept. It is still
  // bookkeeping, and it is still listed under "not kept" — the catalogue names
  // it, the engine's outcome files it.
  const report = JSON.stringify({
    entries: [
      finding("docx.export.stale.thumbnail", 1, "docProps/thumbnail.jpeg", "omitted", "not_retained"),
      finding("docx.rsid", 40, null, "omitted", "not_retained"),
    ],
  });
  const groups = groupFindings(report);
  assert.deepEqual(groups.map((group) => [group.id, group.total, group.bookkeeping.total]), [["lost", 0, 41]]);
  assert.deepEqual(findingTotals(report), { headline: 0, bookkeeping: 41, entries: 2 });
});

test("an id nobody catalogued still says which part it is in and what kind of thing it is", () => {
  const theme = finding("extLst", 1, "word/theme/theme1.xml");
  assert.equal(words(theme), "Other content or formatting");
  assert.equal(where(theme), "in the theme");

  const header = finding("someFutureElement", 3, "word/header2.xml");
  assert.equal(where(header), "in a header");
  assert.equal(where(finding("x", 1, "word/footer1.xml")), "in a footer");
  assert.equal(where(finding("x", 1, "word/footnotes.xml")), "in the footnotes");
  assert.equal(where(finding("x", 1, "word/comments.xml")), "in the comments");
  assert.equal(where(finding("x", 1, "word/numbering.xml")), "in the list definitions");
  assert.equal(where(finding("x", 1, "word/styles.xml")), "in the styles");

  const attribute = finding("p/@w14:somethingNew", 1, "word/document.xml");
  assert.equal(words(attribute), "A property of an item");
  assert.equal(where(attribute), "in the document body");

  const setting = finding("doNotUseHTMLParagraphAutoSpacing", 1, "word/settings.xml");
  assert.equal(words(setting), "A document setting");
  assert.equal(where(setting), "in the document settings");

  assert.equal(words(finding("Template2", 1, "docProps/app.xml")), "A document property");

  // A part this table does not know is named as itself, never left blank.
  assert.equal(where(finding("x", 1, "word/zz.xml")), "in the file part word/zz.xml");
  // A whole part already names itself on the row; an unknown one still says
  // what kind of thing it is, and its area when there is one.
  assert.equal(words(finding("word/foo.xml", 1, "word/foo.xml")), "A separate part of the file");
  assert.equal(where(finding("word/foo.xml", 1, "word/foo.xml")), null);
  assert.equal(where(finding("word/diagrams/data1.xml", 1, "word/diagrams/data1.xml")), "in a SmartArt graphic");
  // A class from another format says which format.
  const odf = finding("odf.style.unresolved", 2, null);
  assert.equal(words(odf), "Another kind of finding");
  assert.equal(where(odf), "in the OpenDocument file");

  // No fallback ever prints the id as if it were the words.
  for (const entry of [theme, header, attribute, setting, odf]) assert.notEqual(words(entry), entry.feature);
});

test("a catalogued finding still says where it is, and a whole part does not repeat itself", () => {
  assert.equal(where(SAMPLE[0]), "in the document body");
  assert.equal(where(finding("fontScheme/@name", 1, "word/theme/theme1.xml")), "in the theme");
  assert.equal(where(finding("decimalSymbol", 1, "word/settings.xml")), "in the document settings");
  assert.equal(where(finding("customXml/item1.xml", 1, "customXml/item1.xml")), null);
  assert.equal(where(finding("docx.rsid", 165, null)), null);
});

test("every catalogue key is answered by all nineteen locales, in their own words", () => {
  const keys = catalogueKeys();
  const extra = ["findings.featureId", "findings.bookkeeping.label", "findings.bookkeeping.note", "findings.bookkeeping.only"];
  const tags = readdirSync(LOCALES)
    .filter((name) => name.endsWith(".json"))
    .map((name) => name.replace(/\.json$/, ""));
  assert.equal(tags.length, 19);
  const gaps = [];
  const copies = [];
  for (const tag of tags) {
    const catalogue = JSON.parse(readFileSync(join(LOCALES, `${tag}.json`), "utf8"));
    for (const key of [...keys, ...extra]) {
      const value = catalogue[key];
      if (typeof value !== "string" || value.trim() === "") gaps.push(`${tag}: ${key}`);
      else if (tag !== "en" && value === EN_STRINGS[key]) copies.push(`${tag}: ${key}`);
    }
  }
  assert.deepEqual(gaps, []);
  assert.deepEqual(copies, [], "a translated catalogue answered with the English sentence");
});
