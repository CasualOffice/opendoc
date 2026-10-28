// `editor.ai`'s decisions that need no browser (`docs/178` §9, `docs/181` §4.4):
// which access level each grant gets, which host refusal code an engine code
// becomes, and how a redline block is drawn.
import assert from "node:assert/strict";
import test from "node:test";

import { ROLES, resolveCapabilities } from "../src/capabilities.mjs";
import { REFUSAL_CODES } from "../src/host_contract.mjs";
import {
  AI_ACCESS,
  aiAccessFor,
  hostRefusalCode,
  redlineSegments,
  refusalKey,
} from "../src/ai_assistant.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

test("each role's grant gives an assistant exactly `docs/178` §9's row", () => {
  const level = (role, direct = false) => aiAccessFor(resolveCapabilities({ mode: role, framed: true }), { direct });
  assert.deepEqual(
    Object.fromEntries(ROLES.map((role) => [role, level(role).access])),
    { preview: "none", readonly: "read", commentor: "comment", edit: "suggest", owner: "review" },
  );
  // Direct edits only with the host's explicit grant, and only where `edit` is granted.
  for (const role of ROLES) assert.equal(level(role).direct, false, `${role} without the grant`);
  assert.equal(level("edit", true).direct, true);
  assert.equal(level("owner", true).direct, true);
  for (const role of ["preview", "readonly", "commentor"]) {
    assert.equal(level(role, true).direct, false, `${role} cannot be granted direct edits`);
  }
  // Decided by the grant, not the role name: withholding `edit` from `edit` is `comment`.
  assert.equal(aiAccessFor(resolveCapabilities({ mode: "edit", withhold: "-edit" })).access, "comment");
  for (const role of ROLES) assert.ok(AI_ACCESS.includes(level(role).access));
});

test("every refusal maps to a contract code and to a sentence the catalogue has", () => {
  for (const code of [
    "capability-withheld",
    "document-protected",
    "document.protected-forms-only",
    "anchor-not-found",
    "anchor-ambiguous",
    "stale-revision",
    "ai.preview-stale",
    "ai.preview-gone",
    "no-preview",
    "no-document",
    "invalid-argument",
    "unknown-edit",
    "engine-refused",
    "something-new",
  ]) {
    assert.ok(REFUSAL_CODES.includes(hostRefusalCode(code)), `${code} -> ${hostRefusalCode(code)}`);
    assert.ok(EN_STRINGS[refusalKey(code)], `${code} -> ${refusalKey(code)} has no English`);
  }
  assert.equal(hostRefusalCode("capability-withheld"), "capability-withheld");
  assert.equal(refusalKey("capability-withheld"), "ai.refusal.access");
  assert.equal(refusalKey("ai.preview-stale"), "ai.refusal.stale");
});

test("a suggestion's redline keeps its marks; a direct edit shows what it was and becomes", () => {
  const suggestion = {
    before: "The notice period is thirty (30) days.",
    after: "The notice period is sixty (60) days.",
    afterRuns: [
      { text: "The notice period is " },
      { text: "sixty (60)", marks: ["inserted"] },
      { text: "thirty (30)", marks: ["deleted"] },
      { text: " days." },
    ],
  };
  assert.deepEqual(redlineSegments(suggestion), [
    { text: "The notice period is ", mark: null },
    { text: "sixty (60)", mark: "ins" },
    { text: "thirty (30)", mark: "del" },
    { text: " days.", mark: null },
  ]);
  const direct = { before: "Old text.", after: "New text.", afterRuns: [{ text: "New text." }] };
  assert.deepEqual(redlineSegments(direct), [
    { text: "Old text.", mark: "del" },
    { text: "New text.", mark: "ins" },
  ]);
  // A new paragraph has no `before`: all of it is inserted, marked or not.
  assert.deepEqual(redlineSegments({ before: null, after: "Added.", afterRuns: [{ text: "Added." }] }), [
    { text: "Added.", mark: "ins" },
  ]);
  // Unchanged text is not dressed up as a change.
  assert.deepEqual(redlineSegments({ before: "Same.", after: "Same.", afterRuns: [{ text: "Same." }] }), [
    { text: "Same.", mark: null },
  ]);
});
