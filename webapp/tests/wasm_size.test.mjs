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
const MAX_TOTAL_BYTES = 25_700_000;
/** What the browser then compiles. Measured: 10,594,266 B (#805); 11,585,101 B
 *  after §19. opt-level 3 measured 12,376,042 B on the smaller #805 engine, so
 *  a dropped `z` still lands far outside this. */
const MAX_CODE_BYTES = 11_800_000;
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
