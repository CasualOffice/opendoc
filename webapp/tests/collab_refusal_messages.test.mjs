// What a collaboration refusal tells the reader, held to what it really did.
//
// RC-13 (`109`): `ODC-7006` said "unsent changes were lost — reload to continue"
// while the engine kept every one of them (`docs/169`), and reloading was the one
// thing that would have lost them. The fix is structural (`session_access.mjs`):
// the code's sentence says only WHY, and the consequence clause is chosen from
// the outcome the engine returned. This file holds the chain:
//
//   1. `collab_refusal_outcomes.json` is the engine's behaviour — the Rust test
//      `every_collaboration_refusal_does_what_the_fixture_says` drives every row
//      through the real `WasmDocument` and fails on any difference;
//   2. for every row, the consequence `refusalConsequence` chooses asserts only
//      facts the row has (`CONSEQUENCE_CLAIMS` ⊆ the row's `after`);
//   3. no cause sentence claims an outcome at all, in English;
//   4. every key the announcement can reach is answered by every catalogue.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const {
  COLLABORATION_REFUSAL_KEYS,
  COLLAB_CONSEQUENCE_KEYS,
  CONSEQUENCE_CLAIMS,
  refusalAnnouncement,
  refusalConsequence,
} = await import("../src/session_access.mjs");
const { EN_STRINGS } = await import("../src/en_strings.mjs");

const fixture = JSON.parse(readFileSync(join(WEBAPP, "tests", "collab_refusal_outcomes.json"), "utf8"));
const outcomeOf = (row) => ({ ...row.outcome, code: row.code });
const english = (key, values = {}) =>
  String(EN_STRINGS[key] ?? key).replace(/\{(\w+)\}/g, (_, name) => String(values[name] ?? `{${name}}`));

test("every row of the fixture names a routed code, and every routed code has a row", () => {
  const codes = new Set(fixture.scenarios.map((row) => row.code));
  assert.deepEqual([...codes].sort(), Object.keys(COLLABORATION_REFUSAL_KEYS).sort());
});

test("the consequence chosen for each real outcome claims only what that outcome did", () => {
  for (const row of fixture.scenarios) {
    for (const canRecover of [true, false]) {
      const consequence = refusalConsequence(outcomeOf(row), { stopped: row.after.stopped, canRecover });
      const where = `${row.code} by ${row.arrives}${canRecover ? "" : " (no snapshotFor)"}`;
      assert.notEqual(consequence, null, `${where}: a refusal the reader is never told about`);
      if (consequence === "cause") continue;
      if (consequence === "silent") {
        // Silence is a claim too: "nothing to tell you" is true only of work that
        // is still here and still going to be sent.
        assert.equal(row.outcome.retryable, true, `${where}: silent about a refusal nothing retries`);
        assert.equal(row.after.kept && row.after.pending && !row.after.stopped, true, where);
        continue;
      }
      const claims = CONSEQUENCE_CLAIMS[consequence];
      assert.ok(claims, `${where}: ${consequence} has no recorded claims`);
      for (const [fact, value] of Object.entries(claims)) {
        assert.equal(
          row.after[fact],
          value,
          `${where}: the "${consequence}" sentence says ${fact} is ${value}, and the engine did otherwise`,
        );
      }
    }
  }
});

test("the too-far-behind refusal tells the reader their work is kept, not lost (RC-13)", () => {
  const row = fixture.scenarios.find((r) => r.code === "ODC-7006");
  const said = refusalAnnouncement(outcomeOf(row), english, { stopped: row.after.stopped, canRecover: true });
  assert.match(said, /kept on this device/);
  assert.doesNotMatch(said, /lost|reload/i);
});

/** Words that state what became of the reader's work. A CAUSE sentence must use
 *  none of them: the consequence clause, chosen from the outcome, is the only
 *  thing allowed to say what happened. */
const OUTCOME_WORDS = /\b(lost|lose|reload|ignored|again|kept|keep|stays?|save|saved|copy your|not applied|not shared|being sent|retry|try)\b/i;

test("no cause sentence claims an outcome", () => {
  for (const [code, key] of Object.entries(COLLABORATION_REFUSAL_KEYS)) {
    if (key.startsWith("session.")) continue;
    assert.doesNotMatch(
      EN_STRINGS[key],
      OUTCOME_WORDS,
      `${code} (${key}) says what happened to the reader's work; only the consequence clause may`,
    );
  }
});

test("every key an announcement can reach is answered by every catalogue", () => {
  const keys = [
    ...Object.values(COLLABORATION_REFUSAL_KEYS),
    ...Object.values(COLLAB_CONSEQUENCE_KEYS),
    "collab.refusal",
    "collab.recoveryNeeded",
    "collab.recoveryFailed",
  ];
  for (const name of readdirSync(join(WEBAPP, "locales")).filter((f) => f.endsWith(".json"))) {
    const catalogue = JSON.parse(readFileSync(join(WEBAPP, "locales", name), "utf8"));
    for (const key of keys) {
      assert.ok(catalogue[key], `${name} cannot answer ${key}`);
      if (name !== "en.json" && !key.startsWith("session.") && key !== "collab.refusal") {
        assert.notEqual(catalogue[key], EN_STRINGS[key], `${name} ships ${key} in English`);
      }
    }
    assert.match(catalogue["collab.refusal"], /\{cause\}/, `${name}: the template lost its cause`);
    assert.match(catalogue["collab.refusal"], /\{consequence\}/, `${name}: the template lost its consequence`);
  }
});

test("a refusal about access says its cause alone, and a routine rebase says nothing", () => {
  assert.equal(
    refusalAnnouncement({ kind: "refused", code: "ODC-7011", retryable: false }, english),
    EN_STRINGS["session.noAccessChange"],
  );
  assert.equal(refusalAnnouncement({ kind: "refused", code: "ODC-7009", retryable: true }, english), "");
  assert.equal(refusalAnnouncement({ kind: "refused", code: "nope" }, english), "");
  assert.equal(refusalAnnouncement(null, english), "");
});

test("a thrown refusal that stopped the session is described as a stop", () => {
  const row = fixture.scenarios.find((r) => r.code === "ODC-7003" && r.arrives === "refused");
  assert.equal(refusalConsequence(outcomeOf(row), { stopped: true }), "stopped");
  assert.equal(refusalConsequence(outcomeOf(row), { stopped: false }), "notApplied");
});

test("recovery says the work is kept, and claims a load only when the host can supply one", async () => {
  const { createRecovery } = await import("../src/collab_recovery.mjs");
  const said = [];
  const refusing = { collabRecover: () => { throw new Error("refused: no"); } };
  const outcome = { kind: "recoveryRequired", code: "ODC-7006", revision: 9, retryable: false, terminal: false };
  const make = (host) =>
    createRecovery({
      document: () => refusing,
      provider: () => host,
      room: () => "room",
      transport: () => null,
      recovered: async () => {},
      status: (text) => said.push(text),
      t: english,
    });
  make({}).receive(outcome);
  assert.match(said.at(-1), /cannot supply the latest shared copy/, "no snapshotFor: nothing is loading");
  await make({ snapshotFor: async () => ({ revision: 9, bytes: new Uint8Array([1]) }) }).receive(outcome);
  assert.match(said.at(-2), /kept on this device while the latest shared copy is loaded/);
  assert.match(said.at(-1), /could not be loaded\. Your changes are still kept/);
  for (const text of said) assert.doesNotMatch(text, /lost|reload/i);
});
