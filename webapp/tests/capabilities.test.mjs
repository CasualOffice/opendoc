// A framed editor must not be a standalone editor.
//
// `docs/125` §2 F3: today File ▸ New and File ▸ Open are live inside someone
// else's page, so a visitor can replace the host's document from within the
// host's own chrome. opencalc measured and fixed the identical defect
// (`docs/104` §HF-109) — a framed editor there resolved to `standalone`.
//
// The load-bearing rule is the DEFAULT: a page that is not the top window was
// put there by someone else, and the safe reading of that is `embedded`. A host
// should not have to discover a URL parameter to avoid the defect.
import { test } from "node:test";
import assert from "node:assert/strict";
import { CAPABILITIES, PRESET_NAMES, resolveCapabilities } from "../src/capabilities.mjs";

test("a framed editor defaults to embedded, not standalone", () => {
  const framed = resolveCapabilities({ framed: true });
  assert.equal(framed.has("open"), false, "a framed editor must not replace the host's document");
  assert.equal(framed.has("new"), false, "nor start a different one");
  // It is embedded to be USED, so the reason it was embedded still works.
  assert.equal(framed.has("edit"), true);
  assert.equal(framed.has("comment"), true);
  assert.equal(framed.has("save"), true);
});

test("an unframed page keeps everything", () => {
  const top = resolveCapabilities({ framed: false });
  for (const capability of CAPABILITIES) {
    assert.ok(top.has(capability), `a standalone editor should keep ${capability}`);
  }
});

test("a host can ask for a preset explicitly, in either framing", () => {
  for (const framed of [true, false]) {
    assert.equal(resolveCapabilities({ mode: "viewer", framed }).has("edit"), false);
    assert.equal(resolveCapabilities({ mode: "standalone", framed }).has("open"), true);
    assert.equal(resolveCapabilities({ mode: "embedded", framed }).has("open"), false);
  }
  // Case and padding are a host's typing, not a different intent.
  assert.equal(resolveCapabilities({ mode: " EMBEDDED " }).has("open"), false);
});

test("a viewer can read and print, and do nothing else", () => {
  const viewer = resolveCapabilities({ mode: "viewer" });
  assert.deepEqual([...viewer].sort(), ["print"]);
});

test("an unrecognised mode falls back to the framing default, never to more", () => {
  // A typo in a host's URL must not hand the visitor a standalone editor inside
  // someone else's page, and must not leave the editor with nothing either.
  const typo = resolveCapabilities({ mode: "embeded", framed: true });
  assert.deepEqual([...typo].sort(), [...resolveCapabilities({ framed: true })].sort());
  assert.equal(typo.has("open"), false);

  const unframed = resolveCapabilities({ mode: "nonsense", framed: false });
  assert.equal(unframed.has("open"), true);
});

test("every preset grants only capabilities that exist", () => {
  // A preset naming a capability the set does not define is a typo that would
  // silently grant nothing, and would read as a deliberate denial forever.
  for (const name of PRESET_NAMES) {
    for (const capability of resolveCapabilities({ mode: name })) {
      assert.ok(CAPABILITIES.includes(capability), `${name} grants unknown "${capability}"`);
    }
  }
});

test("no argument at all is the standalone answer", () => {
  assert.equal(resolveCapabilities().has("open"), true);
});
