// The README's capability statements are generated (`tools/build-readme.mjs`),
// because the hand-written ones fell weeks behind `main`. This holds the three
// things that make that true: every region exists, every committed region is
// what the code produces today, and the generator's inputs really move its
// output (a generator that ignored docs/153 would pass the first two).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  TARGETS,
  build,
  descriptors,
  formatRows,
  mcpTools,
  parityAreas,
  workspaceCrates,
} from "../tools/build-readme.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (p) => readFileSync(join(REPO, p), "utf8");

test("every generated region is present and non-empty", () => {
  for (const { file, regions } of TARGETS) {
    const text = read(file);
    for (const name of Object.keys(regions)) {
      const m = new RegExp(`<!-- @generated ${name} -->\\n([\\s\\S]*?)<!-- @end ${name} -->`).exec(text);
      assert.ok(m, `${file}: region ${name} is missing`);
      assert.ok(m[1].trim().length > 0, `${file}: region ${name} is empty`);
    }
  }
});

test("--check passes: the committed regions are what the code produces", () => {
  assert.deepEqual(build({ check: true }), [], "run node webapp/tools/build-readme.mjs and commit the result");
});

test("the feature list follows docs/153: a row that stops shipping leaves it", () => {
  const text = read("docs/153-ONLYOFFICE-CAPABILITY-PARITY-MATRIX.md");
  const before = parityAreas(text);
  const mutated = text.replace("| Bold | Parity |", "| Bold | Gap |");
  assert.notEqual(mutated, text, "the Bold row moved; pick another Parity row for this test");
  const after = parityAreas(mutated);
  const shipped = (areas) => areas.flatMap((a) => a.shipped);
  assert.ok(shipped(before).includes("Bold"));
  assert.ok(!shipped(after).includes("Bold"));
});

test("a Partial row must say what is missing", () => {
  const text = read("docs/153-ONLYOFFICE-CAPABILITY-PARITY-MATRIX.md");
  const mutated = text.replace(/(\| Drop cap \| Partial \| [^|]*?) — missing: [^|]*/, "$1");
  assert.notEqual(mutated, text);
  assert.throws(() => parityAreas(mutated), /states no missing part/);
});

test("the format table is the registry's descriptors", () => {
  const all = descriptors();
  for (const id of ["DOCX", "ODT", "TEXT", "RTF", "PDF"]) assert.ok(all.has(id), `no descriptor for ${id}`);
  const rows = new Map(formatRows(all).map((r) => [r.id, r]));
  assert.equal(rows.get("RTF").save, false, "RTF has no writer; the table must not offer it");
  assert.equal(rows.get("DOTX").open, false);
  assert.equal(rows.get("PDF").open, true, "PDF opens through enable_pdf_import and the editor's PDF module");
});

test("the agent surface is the MCP catalogue", () => {
  const names = mcpTools().map((t) => t.name);
  assert.ok(names.includes("convert_pdf_to_flowing"));
  assert.equal(new Set(names).size, names.length);
});

test("the crate list is the workspace", () => {
  const crates = workspaceCrates();
  assert.ok(crates.includes("casual-doc-pdf-import") && crates.includes("opendoc-relay"));
});
