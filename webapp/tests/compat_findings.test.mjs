// How the compatibility findings are grouped: by what HAPPENED to each
// construct, most serious first — the question a reader deciding whether to
// save over the original is asking. The engine reports two outcome axes per
// entry (`modelOutcome`, `retentionOutcome`); these are folded into five plain
// answers, and a combination nobody planned for lands in "other" rather than
// vanishing.
import { test } from "node:test";
import assert from "node:assert/strict";

import { FINDING_KINDS, findingKind, findingTotals, groupFindings } from "../src/compat_findings.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

const entry = (feature, modelOutcome, retentionOutcome, occurrences = 1) => ({
  feature,
  occurrences,
  location: { partName: null, namespace: null, localName: feature, attributeName: null },
  modelOutcome,
  retentionOutcome,
});

test("each outcome pair lands in the group that says what happened to it", () => {
  assert.equal(findingKind(entry("a", "omitted", "not_retained")), "lost");
  // Approximated AND not kept is, to a reader, not kept.
  assert.equal(findingKind(entry("a", "degraded", "not_retained")), "lost");
  assert.equal(findingKind(entry("a", "omitted", "blocked")), "refused");
  assert.equal(findingKind(entry("a", "omitted", "rejected")), "refused");
  assert.equal(findingKind(entry("a", "degraded", "preserved")), "approximated");
  assert.equal(findingKind(entry("a", "omitted", "preserved")), "preserved");
  // "The original is kept" is only claimed when retention says so.
  assert.equal(findingKind(entry("a", "degraded", "not_applicable")), "other");
  assert.equal(findingKind(entry("a", "mapped", "not_applicable")), "other");
});

test("groups come most-serious first, carry their totals, and leave nothing out", () => {
  const report = JSON.stringify({
    entries: [
      entry("customXml/item1.xml", "omitted", "preserved", 3),
      entry("cNvPr/@name", "degraded", "preserved", 2),
      entry("w:del", "omitted", "not_retained", 5),
      entry("future", "mapped", "not_applicable", 1),
    ],
  });
  const groups = groupFindings(report);
  assert.deepEqual(
    groups.map((group) => [group.id, group.total]),
    [
      ["lost", 5],
      ["approximated", 2],
      ["preserved", 3],
      ["other", 1],
    ],
  );
  const counted = groups.reduce((sum, group) => sum + group.total, 0);
  assert.equal(counted, 11, "every occurrence the chip counts is in exactly one group");
});

test("no report, or an empty one, is no groups — not an error", () => {
  assert.deepEqual(groupFindings(""), []);
  assert.deepEqual(groupFindings(JSON.stringify({ entries: [] })), []);
  assert.deepEqual(findingTotals(""), { headline: 0, bookkeeping: 0, entries: 0 });
});

test("a report with no entries array is a defect, never a clean document", () => {
  // The chip's count. "0 findings" for a malformed report would claim a clean
  // import — the worst answer available (`format_io.mjs`'s `importFindingCount`).
  assert.throws(() => findingTotals(JSON.stringify({ findings: [] })), /no entries array/);
});

test("the chip counts content, opens on bookkeeping alone, and every entry is in one group", () => {
  const report = JSON.stringify({
    entries: [
      entry("docx.rsid", "omitted", "preserved", 120),
      entry("docProps/thumbnail.jpeg", "omitted", "preserved", 1),
      entry("formProt", "omitted", "preserved", 2),
    ],
  });
  assert.deepEqual(findingTotals(report), { headline: 2, bookkeeping: 121, entries: 3 });
  const [kept] = groupFindings(report);
  assert.equal(kept.id, "preserved");
  assert.deepEqual(kept.entries.map((e) => e.feature), ["formProt"]);
  assert.deepEqual(kept.bookkeeping.entries.map((e) => e.feature), ["docx.rsid", "docProps/thumbnail.jpeg"]);

  // Bookkeeping alone: nothing for the chip to say, and still something to open.
  const onlyBookkeeping = JSON.stringify({ entries: [entry("docx.rsid", "omitted", "preserved", 120)] });
  assert.deepEqual(findingTotals(onlyBookkeeping), { headline: 0, bookkeeping: 120, entries: 1 });
  assert.equal(groupFindings(onlyBookkeeping).length, 1, "the group is kept for the entry it holds");
});

test("every group has a sentence in the catalogue", () => {
  for (const kind of FINDING_KINDS) assert.ok(kind.key in EN_STRINGS, `${kind.key} is not declared`);
});
