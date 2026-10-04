import assert from "node:assert/strict";
import test from "node:test";
import { collabCommands } from "../src/collab_chrome.mjs";

// `collab_transport.mjs` has had a `reconnect()` since the transport landed and
// nothing could reach it. These guards are about the ROW being honest: a control
// offered in a state where the pipe refuses to act is a dead control wearing an
// enabled label, which is the recurring defect `SKILL.md` §10 names.
const t = (key) => key;
const pipeIn = (name) => ({ state: () => ({ name }), reconnect() { this.called = true; } });
const row = (transport) => collabCommands({ transport, t })[0];

test("standalone: no shared document, so the row refuses and says which fact applies", () => {
  const r = row(() => null);
  assert.equal(r.enabled, false);
  assert.equal(r.disabledReason, "collab.standalone");
});

test("stopped is the ONE state the pipe acts in, so it is the one state the row is enabled in", () => {
  const r = row(() => pipeIn("stopped"));
  assert.equal(r.enabled, true);
  assert.equal(r.disabledReason, "");
});

test("connected and reconnecting each refuse with their OWN sentence, not a shared one", () => {
  assert.deepEqual(
    ["connected", "reconnecting"].map((n) => row(() => pipeIn(n)).disabledReason),
    ["collab.connected", "collab.reconnecting"],
    "a reader who reached for Reconnect while already sharing must not be told " +
      "the same thing as one whose connection is mid-retry",
  );
  for (const n of ["connected", "reconnecting"]) {
    assert.equal(row(() => pipeIn(n)).enabled, false, `${n} must not offer the row`);
  }
});

test("the transport is read per CALL, because the room is opened per document", () => {
  let pipe = pipeIn("connected");
  const transport = () => pipe;
  assert.equal(collabCommands({ transport, t })[0].enabled, false);
  pipe = pipeIn("stopped");
  assert.equal(
    collabCommands({ transport, t })[0].enabled,
    true,
    "a captured handle would be the PREVIOUS file's pipe after the next open",
  );
});

test("running the row reaches the pipe's own reconnect", () => {
  const pipe = pipeIn("stopped");
  row(() => pipe).run();
  assert.equal(pipe.called, true);
});

test("running it with no transport does not throw", () => {
  assert.doesNotThrow(() => row(() => null).run());
});
