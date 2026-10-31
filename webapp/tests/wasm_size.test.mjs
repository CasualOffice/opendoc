// The engine download the editor page waits on, as a budget.
//
// WHY THIS FILE EXISTS. The owner's report was "loading of the editor page is
// way too slow", and the measurement was that `editor.html` blocks on a 26 MB
// WebAssembly module. The cause was not a debug build — `wasm-pack build`
// prints "Finished `release` profile [optimized]" — it was that the workspace
// `[profile.release]` sets `codegen-units`, `lto` and `strip` but no
// `opt-level`, so the browser module was built at the default 3: optimise for
// SPEED, which buys throughput by emitting more code. Three settings now change
// that, and all three are for the browser build only:
//
//   * `webapp/build.sh` exports `CARGO_PROFILE_RELEASE_OPT_LEVEL=z`;
//   * the root `Cargo.toml` pins the layout, shaping, rasterization, model and
//     import crates back to `opt-level = 3`, so only the cold bulk is built for
//     size — `z` on everything costs 2.5x on a page render, which the editor
//     cannot pay (`docs/107` §4);
//   * `crates/casual-doc-wasm/Cargo.toml` asks `wasm-opt` for `-Oz` rather than
//     a bare `-O`, which in binaryen is `-Os`.
//
// Together: 26,070,655 B -> 24,297,780 B, with the CODE section — the part the
// browser has to compile — down 14.4% from 12,376,042 B to 10,594,266 B, for no
// measurable cost to a keystroke or a page render.
//
// Nothing else in the repository would notice if any of them were dropped.
// `build.sh` would still succeed, every Rust test would still pass, and the
// editor would quietly go back to shipping 1.8 MB more than it needs to — on
// every single load, because the module is too large for Chromium's per-entry
// disk-cache limit and is never cached. So this is a budget on the artifact, not
// a check that a line of shell still reads a certain way: it measures what
// `build.sh` actually produced.
//
// Two budgets rather than one, because they fail differently:
//
//   * TOTAL is what the visitor downloads.
//   * CODE is what the browser then has to compile, and it is the half
//     `opt-level` controls. Pinning it separately means a dropped `opt-level`
//     override reports as "the code section doubled" rather than as a vague
//     total, and it cannot be hidden by an unrelated reduction elsewhere.
//
// Both are ceilings with headroom, not byte-exact ratchets: a byte-exact value
// measured on one machine reddens `main` the first time a different `wasm-opt`
// build or an unrelated engine change moves it by a kilobyte, and a guard that
// fails for a reason that is not the defect gets edited rather than read. The
// headroom is sized so that the regression this exists to catch — any of the
// three settings reverted — is far outside it, and ordinary engine growth is
// not.
//
// The floors are there so the budget cannot be met by a build that did not
// happen. A missing, truncated or stubbed `pkg/` must fail loudly: "0 bytes is
// under budget" is exactly how a size guard becomes decoration.
//
// Driven RED before it was trusted (`SKILL.md` §4): the mutation and its output
// are in the commit message.
import assert from "node:assert/strict";
import { existsSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";

const WASM = fileURLToPath(new URL("../pkg/casual_doc_wasm_bg.wasm", import.meta.url));

/** What a visitor downloads for `editor.html`. Measured: 24,297,780 B (#805).
 *
 *  Both ceilings raised on 2026-10-10 for `docs/155` §19 (chart fonts, axis
 *  titles, number formats, combinations, trendlines and error bars, engine to
 *  page), measured both sides with `build.sh`: `main` at 9d4653f2 (#820) is
 *  25,241,354 B with a 11,373,143 B code section; the branch is 25,461,630 B
 *  with 11,585,101 B — +220,276 B, +211,958 B of it code. `build.sh` still
 *  exports opt-level `z`, so this is the engine growing, decided and recorded
 *  rather than absorbed; the next feature of this size meets the same
 *  decision. */
// 2026-10-11, docs/169: remote presence, refusal recovery, resource-preserving
// snapshots and Share add 249,476 B total / 235,115 B code to the §19 measurement.
// Binaryen 117, the existing -Oz flags and release opt-level z produce
// 25,711,106 B total / 11,820,216 B code. Snapshot resources account for only
// 6,454 B / 5,865 B versus the same collaboration build without the package.
// Retain bounded headroom; optimization settings and the non-stub floor stay armed.
// RM-18 / PR #835, measured in CI on 42a70e6b (Binaryen 117, existing -Oz
// and release z): 25,869,487 B total / 11,960,785 B code. Against the merged
// collaboration baseline this adds 158,381 B / 140,569 B for character patches,
// ligature parsing/shaping and CPU/PDF ink effects. This is 0.62% download growth,
// with all optimization settings retained. Keep about 130 KiB of headroom rather
// than making CI byte-exact; the code ceiling remains below even the smaller
// #805 opt-level-3 measurement (12,376,042 B). The non-stub floor is unchanged.
// `docs/109` HF-390..HF-396 (legacy VML normalised into the DrawingML model,
// ADR-072), measured both sides with `build.sh`'s settings on 2026-10-24:
// `main` at c7395504 (#869) is 25,961,105 B with a 12,049,343 B code section;
// with the VML work (#870 and its follow-up) 26,102,679 B with 12,188,963 B —
// +141,574 B, +139,620 B of it code (0.55%): the path and formula evaluator,
// the group/shape/picture translation, the retained-source side table through
// the model's derived impls, and the writer's verbatim path. Marking the
// once-per-pict functions `#[inline(never)]` recovered only 1,518 B, so this
// is the engine growing, decided and recorded. About 130 KiB of headroom is
// kept on both, and the code ceiling stays below the opt-level-3 measurement
// (12,376,042 B) so a dropped `z` still fails.
// `docs/109` HF-360 (host identity, ADR-070), measured both sides with
// `build.sh`'s settings on 2026-10-24: `main` at cd95c00b (#880) is 26,199,731 B
// with a 12,282,103 B code section; with HF-360 26,272,292 B with 12,349,976 B —
// +72,561 B, +67,873 B of it code (0.28%): `authorship::conform` and its walks,
// the `Profile`/`Participant` wire types and their serde, the participant list
// on the roster, and the people.xml save projection. The engine growing, decided
// and recorded; about 128 KiB of headroom is restored on both. The #805
// opt-level-3 reference above is stale for the CODE ceiling: THIS engine at
// opt-level 3 measured 14,144,617 B of code, so a dropped `z` still fails by
// 1.6 MB with the ceiling at 12,480,000.
// `editor.ai`'s private preview (`docs/181` §4.4, lane preview), measured both
// sides with `build.sh`'s wasm-pack settings on 2026-10-28: the base (72bcdab1, the
// registry families merged) is 26,388,318 B with a 12,461,432 B code section — the
// old ceilings had 11,682 B and 18,568 B left; with the assistant 26,883,062 B with
// 12,906,928 B — +494,744 B, +445,496 B of it code (1.9%). That is `casual-doc-ai`
// linked into the editor: the plan compiler (every edit's serde shape and lowering
// dispatch, the anchor resolver, the redline) and the five reads, which is the
// point of "one compiler": the editor runs the code the MCP server runs instead of
// a second, smaller lowering. Recorded for the owner's decision rather than
// absorbed; about 128 KiB of headroom is restored on both, and the code ceiling
// stays below the opt-level-3 measurement (14,144,617 B) so a dropped `z` fails.
// PDF import in the editor (`docs/182` P2, PR #899), measured with `build.sh`'s
// settings on 2026-10-29: 29,827,011 B total, 14,739,317 B code (+2,834,776 B /
// +1,729,551 B over the base). The owner chose to ship it now (2026-10-29) with the
// ceilings at the measured size plus the usual ~128 KiB headroom; the follow-up is a
// lazily loaded PDF module that returns the main module to the old ceilings
// (27,010,000 / 13,035,000), which this comment then retires.
// Re-measured by CI with reflow (P3) and the OCR plumbing merged: 32,437,447 B
// total, 17,222,892 B code. Ceilings at that plus ~128 KiB, under the same ship-now
// decision; the lazy PDF module returns the main module to the pre-PDF size.
// The lazy PDF module (`docs/182` §17a): the reader, the importer, reflow, OCR and
// the byte-preserving save's planner moved to `pkg-pdf/`, so this module is back
// to the pre-PDF engine plus the save's writing half (the overlay layout and
// `write_incremental_update`), the wire codec and `openPdfImport`. The pre-PDF
// base (75f91629) measured 27,015,500 B with `build.sh`'s settings, already 5,500 B
// over the old 27,010,000; its code section was about 13,033,000 B (13,009,766 B at
// 502ff112 plus that base's growth). Ceilings are that base plus ~128 KiB:
// 27,150,000 / 13,170,000. AN ESTIMATE, not a measurement: the lane that moved
// the planner could not run `build.sh` (owner rule, 2026-10-29), so CI's first
// measurement of this branch replaces both numbers.
// 27,150,000 -> 27,465,000 with the consolidated editing work (#917: table tools,
// notes settings and conversion, review display and Word-compatible protection
// hashing, AutoCorrect, selection language). MEASURED at 27,331,721 B; the
// ceiling is that plus ~128 KiB.
// 27,465,000 -> 28,110,000 with round 3 (#918: Markdown and HTML import with
// pulldown-cmark, content controls and forms, table layout and borders, shape
// effects, picture insert, find/view extras). MEASURED by CI at 27,978,452 B;
// the ceiling is that plus ~128 KiB.
const MAX_TOTAL_BYTES = 28_110_000;
/** What the browser then compiles. Measured: 10,594,266 B (#805); 11,585,101 B
 *  after §19; 12,349,976 B with HF-360, whose opt-level-3 build is 14,144,617 B,
 *  so a dropped `z` still lands far outside this. */
// 13,170,000 -> 13,465,000 for the same work: MEASURED at 13,331,393 B, plus ~128 KiB.
// 13,465,000 -> 14,020,000 for the same work: MEASURED by CI at 13,887,605 B, plus ~128 KiB.
const MAX_CODE_BYTES = 14_020_000;
/** The engine is actually in here: a stub or a truncated file must not pass. */
const MIN_CODE_BYTES = 3_000_000;

const STALE =
  "Run `./webapp/build.sh` first — this budget measures the module that build " +
  "produces, and `webapp/pkg` is not committed.";

/** The WebAssembly section table of `bytes`, as `{ [id]: totalBytes }`. */
function sections(bytes) {
  assert.deepEqual(
    [...bytes.subarray(0, 8)],
    [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    `not a version-1 WebAssembly module. ${STALE}`,
  );
  const sizes = {};
  let at = 8;
  while (at < bytes.length) {
    const id = bytes[at];
    at += 1;
    let size = 0;
    let shift = 0;
    for (;;) {
      const byte = bytes[at];
      at += 1;
      size |= (byte & 0x7f) << shift;
      shift += 7;
      if (!(byte & 0x80)) break;
    }
    sizes[id] = (sizes[id] ?? 0) + size;
    at += size;
  }
  assert.equal(at, bytes.length, "the module's section table does not cover the file");
  return sizes;
}

test("the editor's engine download stays inside its budget", () => {
  assert.ok(existsSync(WASM), `${WASM} is missing. ${STALE}`);
  const total = statSync(WASM).size;
  assert.ok(
    total <= MAX_TOTAL_BYTES,
    `the engine module is ${total.toLocaleString("en-US")} B, over the ` +
      `${MAX_TOTAL_BYTES.toLocaleString("en-US")} B budget. Either a size setting was ` +
      `dropped (the CARGO_PROFILE_RELEASE_OPT_LEVEL block in webapp/build.sh, the ` +
      `per-package opt-level overrides in the root Cargo.toml, the wasm-opt flags in ` +
      `crates/casual-doc-wasm/Cargo.toml), or the engine legitimately grew — in which ` +
      `case measure what grew and decide, do not just raise the number.`,
  );
});

test("the code the browser must compile stays inside its budget", () => {
  assert.ok(existsSync(WASM), `${WASM} is missing. ${STALE}`);
  const code = sections(readFileSync(WASM))[10] ?? 0;
  assert.ok(
    code >= MIN_CODE_BYTES,
    `the code section is only ${code.toLocaleString("en-US")} B — that is not this ` +
      `engine. ${STALE}`,
  );
  assert.ok(
    code <= MAX_CODE_BYTES,
    `the code section is ${code.toLocaleString("en-US")} B, over the ` +
      `${MAX_CODE_BYTES.toLocaleString("en-US")} B budget. At opt-level 3 it measures ` +
      `12,376,042 B, so first check that webapp/build.sh still exports ` +
      `CARGO_PROFILE_RELEASE_OPT_LEVEL=z.`,
  );
});

// ---- The lazily loaded PDF module (`docs/182` §17a) -------------------------
//
// What a visitor downloads the first time they open a PDF, and never for a
// DOCX: hayro and the reader, the fixed-layout importer, reflow, the PP-OCRv5
// runtime (never its models) and the save planner. Budgeted on its own so it
// cannot quietly absorb the 9 MB of faces the main module ships (it asks the
// main module for them, `bundledFace`) or grow back into the main module.
//
// Before the planner, reflow and the CI-merged P3 fonts, it measured
// 6,625,258 B. The ceiling adds an ESTIMATE for those (reflow and the CFF/Type1
// wrapping about 0.7 MB, the planner with the snapshot parser it needs about
// 0.5 MB) and headroom; CI's first measurement replaces it.
const PDF_WASM = fileURLToPath(new URL("../pkg-pdf/casual_doc_wasm_pdf_bg.wasm", import.meta.url));
const PDF_GLUE = fileURLToPath(new URL("../pkg-pdf/casual_doc_wasm_pdf.js", import.meta.url));
const MAIN_GLUE = fileURLToPath(new URL("../pkg/casual_doc_wasm.js", import.meta.url));
// MEASURED by CI at 9,359,261 B (the 8,500,000 estimate predated reflow, the P3
// fonts and the save planner in this module); the ceiling is that plus ~128 KiB.
const MAX_PDF_TOTAL_BYTES = 9_490_000;
const MIN_PDF_TOTAL_BYTES = 1_000_000;

/** Every name a wasm-bindgen glue file exports. */
function exportsOf(glue) {
  const source = readFileSync(glue, "utf8");
  const names = new Set();
  for (const match of source.matchAll(/export (?:async )?(?:function|class|const|let) ([A-Za-z0-9_$]+)/g)) names.add(match[1]);
  for (const match of source.matchAll(/export \{([^}]*)\}/g)) {
    for (const part of match[1].split(",")) {
      const name = part.trim().split(/\s+as\s+/).pop();
      if (name) names.add(name);
    }
  }
  return names;
}

/** Whether `bytes` holds the ASCII text `needle` (a crate path in a panic
 *  location, a type name in a message: what linking a crate leaves behind). */
function holds(bytes, needle) {
  return Buffer.from(bytes).includes(Buffer.from(needle, "latin1"));
}

test("the PDF module stays inside its own budget", () => {
  assert.ok(existsSync(PDF_WASM), `${PDF_WASM} is missing. ${STALE}`);
  const total = statSync(PDF_WASM).size;
  assert.ok(total >= MIN_PDF_TOTAL_BYTES, `the PDF module is only ${total} B — not this module. ${STALE}`);
  assert.ok(
    total <= MAX_PDF_TOTAL_BYTES,
    `the PDF module is ${total.toLocaleString("en-US")} B, over its ` +
      `${MAX_PDF_TOTAL_BYTES.toLocaleString("en-US")} B budget. If it now carries fonts, it ` +
      `stopped asking the main module for them (\`setFaceHost\`).`,
  );
});

test("the main module exports no PDF import entry point, and links no PDF reader", () => {
  assert.ok(existsSync(MAIN_GLUE) && existsSync(PDF_GLUE), STALE);
  const main = exportsOf(MAIN_GLUE);
  const pdf = exportsOf(PDF_GLUE);
  // The PDF module's own entry points: the staged import, OCR and the save plan.
  const importer = ["beginPdfImport", "beginPdfImportWithOcr", "PdfImportJob", "PdfOcrEngine", "PdfOcrModels", "planSave"];
  for (const name of importer) {
    assert.ok(pdf.has(name), `the PDF module should export ${name} (the check must be able to see it)`);
    assert.ok(!main.has(name), `the main module exports ${name}: the PDF importer is linked into the editor again`);
  }
  // What the main module does export for a PDF session: open the package, and
  // hand the save to the PDF module.
  for (const name of ["openPdfImport", "openPdfReflow", "setPdfSavePlanner", "bundledFace"]) {
    assert.ok(main.has(name), `the main module should export ${name}`);
  }
  // hayro leaves its crate paths in panic locations wherever it is linked; the
  // PDF module proves the needle is visible at all.
  assert.ok(holds(readFileSync(PDF_WASM), "hayro"), "the PDF module should link hayro");
  assert.ok(!holds(readFileSync(WASM), "hayro"), "the main module links hayro again");
  assert.ok(!holds(readFileSync(WASM), "casual-doc-pdf-save"), "the main module links the save planner");
});

// ---- Text recognition's kernels are vectorised (HF-414) ---------------------
//
// rten selects its WebAssembly SIMD kernels at compile time, so a PDF module
// built without `-C target-feature=+simd128` still works, only many times
// slower: on the live site (2026-10-31) a scanned page had not finished after
// ten minutes, and the owner reported that OCR "is still not there".
// `build.sh` builds this module with `simd128` and the main module without it.
//
// The check reads the artifact, not the build script: a function that keeps a
// value in a `v128` local declares it in its body's local declarations (value
// type 0x7b), which a scalar build never does. O(code section), decoding only
// each body's local declarations, never its instructions.

/** How many function bodies in `bytes` (a wasm module) declare a `v128` local. */
function v128Functions(bytes) {
  let p = 8;
  const leb = () => {
    let value = 0;
    let shift = 0;
    let byte;
    do {
      byte = bytes[p++];
      value += (byte & 0x7f) * 2 ** shift;
      shift += 7;
    } while (byte & 0x80);
    return value;
  };
  let count = 0;
  while (p < bytes.length) {
    const id = bytes[p++];
    const size = leb();
    const end = p + size;
    if (id === 10) {
      const bodies = leb();
      for (let i = 0; i < bodies; i += 1) {
        const bodyEnd = leb() + p;
        const groups = leb();
        let vector = false;
        for (let g = 0; g < groups; g += 1) {
          leb();
          if (bytes[p++] === 0x7b) vector = true;
        }
        if (vector) count += 1;
        p = bodyEnd;
      }
    }
    p = end;
  }
  return count;
}

test("the PDF module is built with WebAssembly SIMD, and the main module is not", () => {
  assert.ok(existsSync(PDF_WASM) && existsSync(WASM), STALE);
  const pdf = v128Functions(readFileSync(PDF_WASM));
  // Measured 563 with simd128, 0 in the scalar module the site served before.
  assert.ok(
    pdf >= 100,
    `only ${pdf} functions in the PDF module use v128: it was built without simd128 ` +
      "(webapp/build.sh's RUSTFLAGS for casual-doc-wasm-pdf), so OCR's kernels run scalar",
  );
  // The main module's build is a separate decision (RM-14); this keeps the flag
  // from leaking into it unnoticed.
  assert.equal(v128Functions(readFileSync(WASM)), 0, "the main module now uses SIMD: RM-14 decides that, not this flag");
});
