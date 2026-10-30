// The examples the package ships, and that `webapp/sdk.page.html` publishes
// verbatim, are RUN here: a quoted example that does not work is a false claim.
// They import the package by its own name (`@casualoffice/opendoc/node`), which
// Node resolves through the package's self-reference, so this is the import a
// host writes. Needs `./build.sh` first, as every test in this directory does.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { Document } from "../src/node.js";

const examples = fileURLToPath(new URL("../examples/", import.meta.url));
const sample = fileURLToPath(new URL("../../../sample.docx", import.meta.url));

function run(script, args) {
  const out = mkdtempSync(join(tmpdir(), "opendoc-example-"));
  try {
    const stdout = execFileSync(process.execPath, [join(examples, script), ...args(out)], {
      encoding: "utf8",
    });
    return { out, stdout };
  } catch (error) {
    rmSync(out, { recursive: true, force: true });
    throw error;
  }
}

test("read-modify-convert.mjs opens, edits and saves four formats with their reports", () => {
  const { out, stdout } = run("read-modify-convert.mjs", (dir) => [sample, dir]);
  try {
    assert.match(stdout, /^docx: \d+ paragraphs/m);
    for (const ext of ["docx", "odt", "pdf", "md"]) {
      assert.ok(statSync(join(out, `output.${ext}`)).size > 0, `output.${ext} was written`);
      assert.match(stdout, new RegExp(`^output\\.${ext}: `, "m"));
    }
    const saved = Document.open(readFileSync(join(out, "output.docx")));
    assert.ok(saved.paragraphs.some((p) => p.text === "Reviewed by the OpenDoc SDK."));
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
});

test("build-report.mjs, a docx-js program, writes the DOCX, the PDF and a page image", () => {
  const { out } = run("build-report.mjs", (dir) => [dir]);
  try {
    for (const name of ["report.docx", "report.pdf", "report-page-1.png"]) {
      assert.ok(statSync(join(out, name)).size > 0, `${name} was written`);
    }
    const built = Document.open(readFileSync(join(out, "report.docx")));
    assert.equal(built.paragraphs[0].text, "Quarterly report");
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
});
