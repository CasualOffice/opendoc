// The complexity guard for `109` HF-183 — a DOUBLING test, not a stopwatch.
//
// `docs/107` §4 says per-interaction work is O(1) in document size, and SKILL.md
// §8 says to guard the complexity rather than a millisecond figure: "build
// documents of n and 2n and assert the work roughly doubles. A timing threshold
// is flaky and cannot tell a slow constant from a quadratic."
//
// So "work" here is measured, not timed: every engine call is counted and every
// byte of paint order the host asks for is added up. A document of 2n objects
// must not make the editor do more work per interaction than a document of n.
// Before the cache the ratio was exactly 2 (the payload doubles, and it is
// fetched once per keystroke); with it, the ratio is 1 and the number of engine
// calls per interaction window is 1 whatever n is.
import assert from "node:assert/strict";
import test from "node:test";

import { createObjectPresence } from "../src/object_presence.mjs";

/** An engine stand-in holding `n` floating objects, counting what is asked of
 *  it. `bytes` is the real cost being guarded: `objectOrder()` serializes the
 *  whole paint order and the host then parses it, so the work is proportional to
 *  the payload, not to the number of calls alone. */
function engineWith(n) {
  const payload = JSON.stringify(
    Array.from({ length: n }, (_, i) => ({ ref: `obj${i}`, kind: "picture" })),
  );
  const meter = { calls: 0, bytes: 0 };
  return {
    meter,
    read: () => {
      meter.calls += 1;
      meter.bytes += payload.length;
      return payload;
    },
  };
}

/** The work one interaction window costs: `keystrokes` questions asked between
 *  two edits, which is what typing into the command palette does. */
function workFor(objects, keystrokes) {
  const engine = engineWith(objects);
  const presence = createObjectPresence(engine.read);
  for (let i = 0; i < keystrokes; i += 1) assert.equal(presence.has(), objects > 0);
  return engine.meter;
}

test("the work of one interaction window does not grow with the document", () => {
  const n = 500;
  // n and 2n, and one keystroke against many, because two ratios are needed:
  // the payload doubles with the document no matter how the host behaves, so a
  // document-size ratio alone cannot tell a per-keystroke fetch from a cached
  // one. What separates them is whether the work grows with the NUMBER of
  // questions asked between two edits — which is what typing into the command
  // palette, or holding a chord down, actually does.
  for (const objects of [n, n * 2]) {
    const once = workFor(objects, 1);
    const many = workFor(objects, 200);
    assert.equal(
      many.bytes,
      once.bytes,
      `${objects} objects: 200 questions cost ${many.bytes} bytes against ${once.bytes} for one`,
    );
    assert.equal(many.calls, 1, `${objects} objects: 200 questions cost ${many.calls} engine calls`);
  }

  // And the shape of the remaining cost, stated rather than implied: what one
  // interaction window costs is exactly one document-sized read, so doubling the
  // document doubles that single read and nothing else. A quadratic or a
  // per-keystroke scan cannot satisfy both this and the loop above.
  const small = workFor(n, 200);
  const big = workFor(n * 2, 200);
  const ratio = big.bytes / small.bytes;
  assert.ok(
    ratio > 1.8 && ratio < 2.2,
    `one read each, so the ratio is the size ratio: got ${ratio}`,
  );
});

test("an edit is what makes it ask again, and it asks exactly once more", () => {
  const engine = engineWith(3);
  const presence = createObjectPresence(engine.read);
  assert.equal(presence.has(), true);
  assert.equal(engine.meter.calls, 1);

  // Stale-answer risk is the whole cost of a cache, so the invalidation is
  // asserted directly rather than inferred from behaviour.
  presence.forget();
  assert.equal(presence.isCached(), false);
  assert.equal(presence.has(), true);
  assert.equal(presence.has(), true);
  assert.equal(engine.meter.calls, 2, "one call per invalidation window, not per question");
});

test("the answer follows the document rather than the first thing it saw", () => {
  let objects = 0;
  const presence = createObjectPresence(() => JSON.stringify(Array.from({ length: objects })));
  assert.equal(presence.has(), false);
  // An insert that did not invalidate would leave "Select next object" greyed out
  // with a picture on the page — the user-visible face of a missed invalidation.
  objects = 1;
  assert.equal(presence.has(), false, "still cached, which is why forget() is mandatory");
  presence.forget();
  assert.equal(presence.has(), true);
  objects = 0;
  presence.forget();
  assert.equal(presence.has(), false);
});

test("an engine that throws or answers nonsense means no objects, not a crash", () => {
  // Fail closed, exactly as the inline version did: a greyed command with a
  // reason beats a thrown exception inside a command build that the palette,
  // the menu bar and the chord dispatcher all depend on.
  const throwing = createObjectPresence(() => {
    throw new Error("null pointer passed to rust");
  });
  assert.equal(throwing.has(), false);
  assert.equal(createObjectPresence(() => "not json").has(), false);
  assert.equal(createObjectPresence(() => "{}").has(), false, "an object is not a paint order");
});
