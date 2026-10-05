// SPDX-License-Identifier: Apache-2.0
// The rights surface's decisions, and the access indicator's.
//
// Every guard here creates its own condition. The two that matter most are the
// ones about ABSENCE — a viewer must see nothing, and a reader with no room must
// see a reason — because absence is the kind of property a test passes by
// accident: an empty array is what a broken builder returns too. So each of them
// also asserts the OTHER side of the branch in the same test, and the mutation
// log in the commit records that deleting the rule reddens them.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import {
  CUSTOM_ROLE_KEY,
  ROLES,
  membershipRows,
  namesForRole,
  rightsSurface,
  roleKey,
  roleOf,
  rolesWithin,
} from "../src/session_rights.mjs";
import { ACCESS_LEVEL_KEYS, ACCESS_SOURCE_KEYS, accessState } from "../src/access_badge.mjs";
import { PARTICIPANT_CAPABILITIES, capabilityForCommand } from "../src/session_access.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const source = (...parts) => readFileSync(join(ROOT, ...parts), "utf8");

/** A participant grant of the shape `session_access.mjs` builds. */
const grant = (names) => Object.freeze({ shared: true, names: Object.freeze(names) });
const standalone = Object.freeze({ shared: false, names: Object.freeze([]) });

const OWNER = ["comment", "edit", "manageAccess", "manageProtection", "review", "suggest"];
const EDITOR = ["comment", "edit", "manageAccess", "review", "suggest"];

// ---- The role vocabulary is the ENGINE's, not a second one -----------------

test("every role mirrors the engine preset it is named after", () => {
  // DERIVED from `access.rs` rather than from a list here, because the whole
  // point of the role table is that it is a named subset of the engine's own
  // capability vocabulary. A preset that gains a capability in Rust and not here
  // is a role the dialog offers and the relay refuses.
  const access = source("crates", "casual-doc-edit", "src", "access.rs");

  /** The `with_*` calls a preset's body reaches, following `Self::x()` chains. */
  const expand = (name, seen = new Set()) => {
    if (seen.has(name)) return [];
    seen.add(name);
    const body = access.match(
      new RegExp(`pub const fn ${name}\\(\\) -> Self \\{([\\s\\S]*?)\\n    \\}`),
    );
    assert.ok(body, `access.rs has no \`${name}()\` preset`);
    const own = [...body[1].matchAll(/with_(\w+)\(\)/g)].map((m) => m[1]);
    const chained = [...body[1].matchAll(/Self::(\w+)\(\)/g)].flatMap((m) => expand(m[1], seen));
    return [...chained, ...own];
  };

  /** `with_manage_access` → `manageAccess`, the facade's name for it. */
  const camel = (snake) => snake.replace(/_([a-z])/g, (_, c) => c.toUpperCase());

  for (const role of ROLES) {
    const expected = [...new Set(expand(role.value).map(camel))].sort();
    assert.deepEqual(
      [...role.names],
      expected,
      `role \`${role.value}\` does not match \`Capabilities::${role.value}()\``,
    );
    for (const name of role.names) {
      assert.ok(
        PARTICIPANT_CAPABILITIES.includes(name),
        `role \`${role.value}\` names \`${name}\`, which the facade will refuse`,
      );
    }
  }
  // The scan can see what it looks for: `viewer()` really is the empty set, so an
  // expander that returned nothing for every preset would not pass the others.
  assert.deepEqual([...ROLES[0].names], []);
  assert.ok(ROLES.some((role) => role.names.length > 0));
});

test("every role label resolves to a declared sentence", () => {
  for (const role of ROLES) assert.ok(EN_STRINGS[role.labelKey], `${role.labelKey} is undeclared`);
  assert.ok(EN_STRINGS[CUSTOM_ROLE_KEY]);
});

test("a grant no role matches is named Custom rather than rounded to the nearest", () => {
  // `143` §10 exists so a host can narrow a role without inventing a role
  // vocabulary, so `comment + manageProtection` is a real grant. Rounding it to a
  // role would tell a reader they hold something they do not.
  const odd = ["comment", "manageProtection"];
  assert.equal(roleOf(odd), null);
  assert.equal(roleKey(odd), CUSTOM_ROLE_KEY);
  // ...and the exact sets still resolve, so the guard is about the gap and not
  // about `roleOf` failing everywhere.
  assert.equal(roleOf(OWNER)?.value, "owner");
  assert.equal(roleOf([])?.value, "viewer");
  assert.equal(roleOf(["comment"])?.value, "commenter");
  // Order and duplicates do not change the answer: the wire sorts, and a caller
  // that did not must not get "Custom" for an owner.
  assert.equal(roleOf(["edit", "comment", "comment", "manageAccess", "review", "suggest"])?.value, "editor");
});

test("a reviewer is not a suggester, which is the asymmetry that IS the role", () => {
  // `access.rs`'s own note: a reviewer resolves suggestions and does not author
  // them. A role table that made `reviewer` a superset would hand out `suggest`
  // with a name that does not say so.
  const reviewer = ROLES.find((role) => role.value === "reviewer");
  const suggester = ROLES.find((role) => role.value === "suggester");
  assert.ok(reviewer.names.includes("review"));
  assert.ok(!reviewer.names.includes("suggest"));
  assert.ok(suggester.names.includes("suggest"));
  assert.ok(!suggester.names.includes("review"));
});

// ---- The two delegation bounds ---------------------------------------------

test("a surface offers only the roles inside BOTH the target's ceiling and the actor's own", () => {
  // The same pair `refuse_access_change` applies, which is why this exists: a
  // surface that offered a role the relay refuses is the dead control at a
  // distance — it looks live and the refusal arrives from the network.
  const offered = (ceiling, actor) => rolesWithin(ceiling, actor).map((role) => role.value);

  // A commenter's ceiling admits a viewer and a commenter and nothing above.
  assert.deepEqual(offered(["comment"], OWNER), ["viewer", "commenter"]);
  // An EDITOR cannot appoint an owner even when the host granted the target one,
  // because the editor holds no `manageProtection` itself.
  assert.deepEqual(offered(OWNER, EDITOR), [
    "viewer",
    "commenter",
    "suggester",
    "reviewer",
    "editor",
  ]);
  // An owner facing an owner may offer everything.
  assert.deepEqual(offered(OWNER, OWNER), [
    "viewer",
    "commenter",
    "suggester",
    "reviewer",
    "editor",
    "owner",
  ]);
  // A viewer's ceiling admits exactly one row, which is still a row: it says
  // truthfully that this person is a viewer and cannot be anything else.
  assert.deepEqual(offered([], OWNER), ["viewer"]);
  // ...and the order is the vocabulary's, ascending, not the intersection's.
  assert.deepEqual(
    offered(OWNER, OWNER),
    ROLES.map((role) => role.value),
  );
});

test("a role value no table has yields null and never an empty grant", () => {
  // `null` and not `[]`: an empty array is `viewer`, a real and very different
  // answer, and a typo that silently demoted somebody to a viewer is the worst
  // failure this module could have.
  assert.equal(namesForRole("nonsense"), null);
  assert.equal(namesForRole(""), null);
  assert.deepEqual([...namesForRole("viewer")], []);
  assert.deepEqual([...namesForRole("editor")], EDITOR);
});

// ---- Who is offered the surface at all ------------------------------------

test("a viewer in a room is offered nothing at all, and an editor is offered it", () => {
  // THE OWNER'S DECISION: "a full rights-changing dialog for owner and editor —
  // not viewer... not greyed, not present." Both sides in one test, so an
  // implementation that returned `present: false` for everybody would fail.
  for (const names of [[], ["comment"], ["comment", "suggest"], ["comment", "review"]]) {
    assert.deepEqual(
      rightsSurface(grant(names)),
      { present: false, reasonKey: null },
      `a participant holding ${JSON.stringify(names)} was offered the rights surface`,
    );
  }
  for (const names of [EDITOR, OWNER]) {
    assert.deepEqual(rightsSurface(grant(names)), { present: true, reasonKey: null });
  }
});

test("a document with no room offers the command DISABLED WITH A REASON, not absent", () => {
  // The absent thing is a SESSION, not a permission — `collab.reconnect`'s shape,
  // and the whole of what makes the standalone state legible rather than silent.
  const surface = rightsSurface(standalone);
  assert.equal(surface.present, true);
  assert.equal(surface.reasonKey, "rights.standalone");
  assert.ok(EN_STRINGS[surface.reasonKey], "the standalone reason has no sentence");
  // `null` is the same case: `main.js` asks before a document is open.
  assert.deepEqual(rightsSurface(null), surface);
});

test("the gated command resolves to the capability that authorises it", () => {
  assert.equal(capabilityForCommand("review.manageAccess"), "manageAccess");
  // And not to the one a study claimed: `manageProtection` is the DOCUMENT's own
  // policy and travels with the file.
  assert.equal(capabilityForCommand("review.restrictEditing"), "manageProtection");
  assert.notEqual(
    capabilityForCommand("review.manageAccess"),
    capabilityForCommand("review.restrictEditing"),
  );
});

// ---- The membership rows ---------------------------------------------------

test("a membership row carries what somebody is and what they may become", () => {
  const rows = membershipRows(
    [
      { participant: 3, capabilities: EDITOR, ceiling: OWNER },
      { participant: 1, capabilities: [], ceiling: ["comment"] },
    ],
    OWNER,
  );
  // Sorted by participant number, so the list does not reorder itself between
  // two `welcome` frames that happened to arrive in a different order.
  assert.deepEqual(
    rows.map((row) => row.participant),
    [1, 3],
  );
  assert.equal(rows[0].role, "viewer");
  assert.deepEqual(
    rows[0].options.map((role) => role.value),
    ["viewer", "commenter"],
  );
  assert.equal(rows[1].role, "editor");
  assert.equal(rows[1].roleKey, "rights.role.editor");
  assert.deepEqual(
    rows[1].options.map((role) => role.value),
    ["viewer", "commenter", "suggester", "reviewer", "editor", "owner"],
  );
});

test("a participant number that is not one is dropped rather than rendered", () => {
  // The number is what a `SetAccess` NAMES, so a row built from a bad one is a
  // control that would address the wrong person or nobody. Dropped, not defaulted
  // to zero, which is a real participant.
  const rows = membershipRows(
    [
      { participant: -1, capabilities: [], ceiling: [] },
      { participant: "x", capabilities: [], ceiling: [] },
      { participant: 1.5, capabilities: [], ceiling: [] },
      { participant: 2, capabilities: [], ceiling: ["comment"] },
    ],
    OWNER,
  );
  assert.deepEqual(
    rows.map((row) => row.participant),
    [2],
  );
  // Non-arrays are the no-room case and answer empty rather than throwing.
  assert.deepEqual(membershipRows(null, OWNER), []);
  assert.deepEqual(membershipRows(undefined, []), []);
});

test("a custom grant has no selected role, so a picker cannot guess one", () => {
  const [row] = membershipRows(
    [{ participant: 1, capabilities: ["comment", "manageProtection"], ceiling: OWNER }],
    OWNER,
  );
  assert.equal(row.role, null);
  assert.equal(row.roleKey, CUSTOM_ROLE_KEY);
  // The options are still the full set the bounds admit: a custom grant is
  // something to be moved OUT of, not a reason to refuse to move it.
  assert.equal(row.options.length, ROLES.length);
});

// ---- The persistent access indicator --------------------------------------

test("the badge reports the narrowest authority and names which one decided", () => {
  // Each branch in isolation, and the ORDER between them, because the order is
  // the rule: a read-only guest in a protected document is told about their own
  // access rather than about the file.
  const engine = accessState({ readOnlyReason: "too large to edit in windows" });
  assert.equal(engine.levelKey, ACCESS_LEVEL_KEYS.read);
  assert.equal(engine.sourceKey, ACCESS_SOURCE_KEYS.engine);
  // The engine's own sentence is carried VERBATIM — it is more specific than
  // anything this module could say.
  assert.equal(engine.detail, "too large to edit in windows");

  // A room outranks the document's own policy, because the participant's grant is
  // the more specific statement about this reader.
  const guest = accessState({
    grant: grant(["comment"]),
    protection: { active: true, value: "readOnly" },
  });
  assert.equal(guest.levelKey, "rights.role.commenter");
  assert.equal(guest.sourceKey, ACCESS_SOURCE_KEYS.shared);

  // The document's policy outranks the container, which is EXACTLY the
  // combination that used to open in the full editing chrome with nothing on
  // screen: a host grant saying yes and a file saying no.
  const protectedFile = accessState({
    protection: { active: true, value: "comments" },
    capabilities: { has: () => true },
  });
  assert.equal(protectedFile.levelKey, ACCESS_LEVEL_KEYS.comment);
  assert.equal(protectedFile.sourceKey, ACCESS_SOURCE_KEYS.document);
  assert.equal(
    accessState({ protection: { active: true, value: "trackedChanges" } }).levelKey,
    ACCESS_LEVEL_KEYS.suggest,
  );
  // THE ORDERING, with the condition CREATED rather than inherited. The case above
  // cannot see the order at all: a container granting everything grades to `full`,
  // which falls through, so checking it first would change nothing — and a first
  // draft of this guard passed with the two branches swapped.
  //
  // The condition that distinguishes them is a container that restricts and a
  // document that restricts MORE: `comment` from the host, `readOnly` from the
  // file. The narrowest true answer is read-only, attributed to the document.
  // Checking the container first would tell the reader they may comment on a
  // document that forbids it — an overstatement, attributed to the wrong
  // authority, which is the pair of failures the order exists to prevent.
  const both = accessState({
    protection: { active: true, value: "readOnly" },
    capabilities: { has: (name) => name === "comment" },
  });
  assert.equal(both.levelKey, ACCESS_LEVEL_KEYS.read);
  assert.equal(both.sourceKey, ACCESS_SOURCE_KEYS.document);
  // An unenforced restriction restricts nothing, so it must not be reported.
  assert.equal(
    accessState({ protection: { active: false, value: "readOnly" } }).sourceKey,
    ACCESS_SOURCE_KEYS.local,
  );

  // THE SECOND AXIS. `w:documentProtection` carries two independent restrictions
  // and a document can enforce `w:formatting` while its `w:edit` restricts
  // nothing — Word's pure formatting restriction, now enforced at the operation.
  // Such a reader may TYPE, so neither `full` nor `read` is true of them, and
  // both wrong answers are asserted against here rather than only the right one:
  // `full` is the lie this badge exists to stop, and `read` would stop a reader
  // typing in a document they can type in.
  const formattingOnly = accessState({
    protection: { active: true, value: "off", formatting: true },
  });
  assert.equal(formattingOnly.levelKey, ACCESS_LEVEL_KEYS.noFormatting);
  assert.equal(formattingOnly.sourceKey, ACCESS_SOURCE_KEYS.document);
  assert.notEqual(formattingOnly.levelKey, ACCESS_LEVEL_KEYS.full);
  assert.notEqual(formattingOnly.levelKey, ACCESS_LEVEL_KEYS.read);
  // And an editing level ABSORBS the formatting axis rather than being
  // reclassified by it: a reader told the document is read-only does not also
  // need to be told its formatting is.
  assert.equal(
    accessState({ protection: { active: true, value: "readOnly", formatting: true } }).levelKey,
    ACCESS_LEVEL_KEYS.read,
  );

  // The container's grant, read from the CAPABILITY and not the role name — a
  // host may withhold a capability from a named preset.
  const embedded = accessState({ capabilities: { has: (name) => name === "comment" } });
  assert.equal(embedded.levelKey, ACCESS_LEVEL_KEYS.comment);
  assert.equal(embedded.sourceKey, ACCESS_SOURCE_KEYS.host);
  assert.equal(
    accessState({ capabilities: { has: () => false } }).levelKey,
    ACCESS_LEVEL_KEYS.read,
  );
});

test("the badge says something even when nothing restricts anything", () => {
  // An empty indicator is indistinguishable from a broken one, and the owner
  // asked for this state to be LEGIBLE rather than invisible. Standalone is the
  // case `Capabilities::local` describes: the reader is the only authority.
  const alone = accessState({});
  assert.equal(alone.levelKey, ACCESS_LEVEL_KEYS.full);
  assert.equal(alone.sourceKey, ACCESS_SOURCE_KEYS.local);
  assert.equal(alone.detail, "");
  // Every field, always, so a caller destructuring cannot find `undefined` where
  // a sentence belongs.
  for (const state of [alone, accessState({ grant: grant(OWNER) })]) {
    assert.deepEqual(Object.keys(state).sort(), ["detail", "levelKey", "sourceKey"]);
  }
});

test("every sentence the badge can name is declared", () => {
  for (const key of [...Object.values(ACCESS_LEVEL_KEYS), ...Object.values(ACCESS_SOURCE_KEYS)]) {
    assert.ok(EN_STRINGS[key], `${key} is undeclared`);
  }
  // And it can name a role, because in a room it does.
  assert.ok(EN_STRINGS[accessState({ grant: grant(OWNER) }).levelKey]);
  assert.ok(EN_STRINGS["rights.applied"]);
  assert.ok(EN_STRINGS["rights.participant"]);
  assert.ok(EN_STRINGS["rights.notSent"]);
});

test("a container that said nothing is not reported as a container that said no", () => {
  // The distinction this module got WRONG first time round, so it is a guard
  // rather than a comment. Fail-closed is the rule for a boundary and this is a
  // statement: "Read only — set when this document was opened" names an authority
  // that did not speak, and a reader told they may not edit does not try.
  assert.equal(accessState({ capabilities: null }).sourceKey, ACCESS_SOURCE_KEYS.local);
  assert.equal(accessState({ capabilities: {} }).sourceKey, ACCESS_SOURCE_KEYS.local);
  assert.equal(accessState({ capabilities: { has: 7 } }).sourceKey, ACCESS_SOURCE_KEYS.local);
  // ...and a container that IS there and withholds `edit` is a different fact,
  // and is reported. Both sides, so an implementation that ignored the container
  // entirely would fail.
  assert.equal(
    accessState({ capabilities: { has: () => false } }).sourceKey,
    ACCESS_SOURCE_KEYS.host,
  );
  assert.equal(
    accessState({ capabilities: { has: () => false } }).levelKey,
    ACCESS_LEVEL_KEYS.read,
  );
});
