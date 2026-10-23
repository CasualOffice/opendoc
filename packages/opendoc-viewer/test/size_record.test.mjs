// The published size numbers are generated, never typed (SKILL §9.1).
//
// docs/176 §4.5's table is rendered from the committed `size.json` by
// `tools/measure-size.mjs`. This fails when the table was edited by hand, or
// when size.json changed and the table was not regenerated. It does not need a
// build, so it runs everywhere.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { docsMatchRecord } from "../tools/measure-size.mjs";

test("docs/176's viewer size table is size.json's rendering", () => {
  assert.ok(
    docsMatchRecord(),
    "docs/176 §4.5 does not match packages/opendoc-viewer/size.json: rebuild and run tools/measure-size.mjs",
  );
});

test("size.json records the build the package ships", () => {
  const record = JSON.parse(readFileSync(new URL("../size.json", import.meta.url), "utf8"));
  assert.equal(record.features, "wasm,core-fonts,pdf", "the record is of a different build than build.sh's default");
  for (const key of ["raw", "gzip", "brotli", "code", "data"]) {
    assert.ok(Number.isInteger(record.module[key]) && record.module[key] > 0, `module.${key}`);
  }
});
