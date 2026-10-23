// The Viewer SDK's download, as a budget on the artefact `build.sh` produced.
//
// Why a budget and what it defends (docs/176 §4.5, §10 Q2): the viewer exists
// to be the light product. Its weight is the engine code a browser compiles and
// the font bytes in the data section, which barely compress. The owner decided
// that only a small core font set lives in the module and the host serves the
// rest; nothing else in the build would notice if the full 9.3 MB bundle came
// back (a dropped `core-fonts` feature, a `FAMILIES` change in the layout
// crate), or if an editing crate were linked in again. Each would still build,
// pass every Rust test and render correctly, and every viewer host would quietly
// download megabytes more on every load.
//
// Ceilings with headroom, not byte-exact ratchets: a different wasm-opt build or
// ordinary engine growth must not redden main (SKILL guard rule: assert the
// guarantee, not the circumstance). The headroom is sized so that the failures
// this exists for are far outside it: re-embedding the font bundle adds about
// 7.7 MB to the data section, and linking `casual-doc-wasm`'s editing code adds
// several MB of code.
//
// Floors, so a stub or a truncated build cannot pass as "small".
//
// Driven RED before it was trusted (SKILL §4): the mutation and its output are
// in the commit message.
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";

import { measure, wasmSections } from "../tools/measure-size.mjs";

const WASM = new URL("../pkg/opendoc_viewer_bg.wasm", import.meta.url);
const MB = 1024 * 1024;

/** Measured 2026-10-24 (size.json): 11,172,891 B raw, 5,493,044 B gzip, code
 *  5,013,430 B, data 6,148,897 B. */
const BUDGET = Object.freeze({
  raw: 12.5 * MB,
  gzip: 6.25 * MB,
  code: 5.75 * MB,
  data: 7 * MB,
});

test("the viewer module was built", () => {
  assert.ok(existsSync(WASM), "pkg/opendoc_viewer_bg.wasm is missing: run packages/opendoc-viewer/build.sh");
});

test("the viewer module stays inside its download budget", () => {
  const bytes = readFileSync(WASM);
  const { raw, gzip } = measure(bytes, { brotli: false });
  const sections = wasmSections(bytes);
  const code = sections[10] ?? 0;
  const data = sections[11] ?? 0;
  const report = `module is ${raw.toLocaleString("en-US")} B raw, ${gzip.toLocaleString("en-US")} B gzip; ` +
    `code section is ${code.toLocaleString("en-US")} B, data section is ${data.toLocaleString("en-US")} B`;
  console.log(report);

  // Not a stub: the engine and the core face are really in it.
  assert.ok(raw > 4 * MB, `implausibly small module: ${report}`);
  assert.ok(code > 2 * MB, `implausibly small code section: ${report}`);
  assert.ok(data > 1.5 * MB, `the core font set is not in the data section: ${report}`);

  assert.ok(
    data <= BUDGET.data,
    `data section over ${BUDGET.data} B: the module embeds more than the core font set ` +
      `(docs/176 §10, Q2: the host serves every other face): ${report}`,
  );
  assert.ok(
    code <= BUDGET.code,
    `code section over ${BUDGET.code} B: did an editing crate get linked into the viewer? ${report}`,
  );
  assert.ok(raw <= BUDGET.raw, `module over ${BUDGET.raw} B raw: ${report}`);
  assert.ok(gzip <= BUDGET.gzip, `module over ${BUDGET.gzip} B gzip: ${report}`);
});
