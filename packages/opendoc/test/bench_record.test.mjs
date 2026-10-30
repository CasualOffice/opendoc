// The published benchmark table in docs/176 section 12.10 is rendered from the
// committed benchmarks/sdk/results.json, never typed (SKILL section 9).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { DOC, RESULTS, apply } from "../../../benchmarks/sdk/report.mjs";

test("docs/176's benchmark region is exactly what results.json renders to", () => {
  const doc = readFileSync(DOC, "utf8");
  const results = JSON.parse(readFileSync(RESULTS, "utf8"));
  assert.equal(apply(doc, results), doc, "run node benchmarks/sdk/report.mjs");
  assert.equal(results.schema, 1);
  assert.ok(results.speed.length > 0 && results.roundTrip.length > 0);
});
