// The five roles, and the properties that make them safe to hand a host.
//
// `capabilities.test.mjs` already holds the load-bearing DEFAULT — a framed
// page resolves to `embedded`, not `standalone`. This file holds what `docs/126`
// phase 1 adds on top: the five roles the owner asked for (`preview`,
// `readonly`, `commentor`, `edit`, `owner`) as presets over the SAME capability
// set, the mapping from a capability set onto the editor's existing three review
// modes, and the iframe sandbox tokens derived from it.
//
// Every assertion here is about a guarantee a host relies on, not about the
// shape of the table. Three of them are structural and would catch a whole class
// of future edit rather than one typo:
//
//   * the role chain is MONOTONE, so a "higher" role can never lose something a
//     lower one had — the classic permissions defect;
//   * no role that lacks `edit` resolves to an editable mode, for every role
//     there is and every role there ever will be;
//   * the legacy preset names really are the same sets as the roles they claim
//     to alias, rather than two tables that drifted.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  CAPABILITIES,
  LEGACY_PRESETS,
  PRESET_NAMES,
  ROLES,
  editingModeFor,
  hostCapabilities,
  resolveCapabilities,
  sandboxTokensFor,
} from "../src/capabilities.mjs";

/** The set a role grants, as a sorted array, for set comparisons. */
function granted(role) {
  return [...resolveCapabilities({ mode: role })].sort();
}

test("the five roles the owner asked for all exist and all resolve", () => {
  assert.deepEqual([...ROLES], ["preview", "readonly", "commentor", "edit", "owner"]);
  for (const role of ROLES) {
    assert.ok(PRESET_NAMES.includes(role), `${role} is named but not resolvable`);
  }
});

test("the role chain is monotone: a higher role never loses a lower one's grant", () => {
  // The defect this refuses: `commentor` gaining `download` while `readonly`
  // keeps it, or `edit` dropping `print`, so that "upgrading" a host's role
  // silently takes something away. Stated as set inclusion over the declared
  // order, so it holds for a role added later too.
  for (let i = 1; i < ROLES.length; i++) {
    const lower = resolveCapabilities({ mode: ROLES[i - 1] });
    const higher = resolveCapabilities({ mode: ROLES[i] });
    for (const capability of lower) {
      assert.ok(
        higher.has(capability),
        `${ROLES[i]} must grant everything ${ROLES[i - 1]} does, but it drops "${capability}"`,
      );
    }
    assert.ok(
      higher.size > lower.size,
      `${ROLES[i]} grants no more than ${ROLES[i - 1]} — two names for one role`,
    );
  }
});

test("a reader cannot change, keep, or replace the document", () => {
  for (const role of ["preview", "readonly"]) {
    const set = resolveCapabilities({ mode: role });
    for (const forbidden of ["edit", "comment", "save", "open", "new", "autosave", "download"]) {
      assert.equal(set.has(forbidden), false, `${role} must not grant "${forbidden}"`);
    }
  }
  // And the difference between the two is the one thing it claims to be.
  assert.deepEqual(granted("preview"), []);
  assert.deepEqual(granted("readonly"), ["print"]);
});

test("a commentor may annotate and suggest, but not edit outright", () => {
  const set = resolveCapabilities({ mode: "commentor" });
  assert.equal(set.has("comment"), true);
  assert.equal(set.has("edit"), false, "a commentor that can edit is an editor");
  assert.equal(set.has("open"), false);
  assert.equal(set.has("new"), false);
});

test("only the owner may reach for a different document or wear our name", () => {
  // `open` and `new` are the HF-109 defect itself: live inside someone else's
  // page they let a visitor replace the host's document. `branding` is the other
  // side of the same line — an embedded editor does not advertise us inside a
  // host's product.
  for (const role of ROLES) {
    const set = resolveCapabilities({ mode: role });
    const expected = role === "owner";
    for (const capability of ["open", "new", "branding"]) {
      assert.equal(set.has(capability), expected, `${role} / ${capability}`);
    }
  }
});

test("no role grants a capability that does not exist", () => {
  for (const role of ROLES) {
    for (const capability of resolveCapabilities({ mode: role })) {
      assert.ok(CAPABILITIES.includes(capability), `${role} grants unknown "${capability}"`);
    }
  }
});

test("a capability set without edit never resolves to an editable mode", () => {
  // The guarantee, stated over every preset there is rather than over the three
  // this phase happens to care about: if a preset cannot edit, the mode it maps
  // onto must not be the one where edits apply untracked.
  for (const name of PRESET_NAMES) {
    const set = resolveCapabilities({ mode: name });
    const mode = editingModeFor(set);
    if (!set.has("edit")) {
      assert.notEqual(mode, "editing", `${name} cannot edit, so it must not resolve to Editing`);
    }
    if (!set.has("edit") && !set.has("comment")) {
      assert.equal(mode, "viewing", `${name} can neither edit nor comment, so it must be Viewing`);
    }
  }
});

test("the five roles land on the three review modes that already exist", () => {
  assert.equal(editingModeFor(resolveCapabilities({ mode: "preview" })), "viewing");
  assert.equal(editingModeFor(resolveCapabilities({ mode: "readonly" })), "viewing");
  assert.equal(editingModeFor(resolveCapabilities({ mode: "commentor" })), "suggesting");
  assert.equal(editingModeFor(resolveCapabilities({ mode: "edit" })), "editing");
  assert.equal(editingModeFor(resolveCapabilities({ mode: "owner" })), "editing");
});

test("a mode is never decided from a missing or malformed capability set", () => {
  // A caller that has not resolved anything yet must not be handed an editable
  // editor by accident. Fail closed on every shape that is not a set.
  for (const nothing of [undefined, null, {}, new Set()]) {
    assert.equal(editingModeFor(nothing), "viewing");
  }
});

test("an unknown role is the framed default, never more than it", () => {
  // A typo in a host's URL is the case that matters: `?mode=readonly` misspelt
  // must not hand the visitor an editor that can replace the host's document.
  const typo = resolveCapabilities({ mode: "read-only", framed: true });
  assert.equal(typo.has("open"), false);
  assert.equal(typo.has("new"), false);
  assert.equal(typo.has("autosave"), false);
  assert.deepEqual([...typo].sort(), [...resolveCapabilities({ framed: true })].sort());
});

test("the legacy preset names are the same sets as the roles they alias", () => {
  for (const [legacy, role] of Object.entries(LEGACY_PRESETS)) {
    if (role === null) continue;
    assert.deepEqual(
      granted(legacy),
      granted(role),
      `"${legacy}" claims to be "${role}" but grants a different set`,
    );
  }
  // `embedded` is deliberately NOT an alias: it is `edit` minus `autosave`,
  // because it is what a page gets when it turns out to be framed and nobody
  // asked for a mode, and a framed page must not leave drafts in the host's
  // origin store.
  assert.equal(LEGACY_PRESETS.embedded, null);
  const difference = granted("edit").filter((c) => !granted("embedded").includes(c));
  assert.deepEqual(difference, ["autosave"]);
});

test("a framed page keeps no drafts unless the host says so", () => {
  // This was `autosaveAllowedHere()` in `main.js` — the second place that knew
  // about framing (`docs/126` phase 1). The rule it encoded has to survive the
  // move, including both halves of the `?autosave` override (`docs/112` O-2).
  assert.equal(resolveCapabilities({ framed: true }).has("autosave"), false);
  assert.equal(resolveCapabilities({ framed: false }).has("autosave"), true);
  assert.equal(resolveCapabilities({ framed: true, autosave: true }).has("autosave"), true);
  assert.equal(resolveCapabilities({ framed: false, autosave: false }).has("autosave"), false);
});

test("the autosave override can only move autosave", () => {
  // It is the one per-capability override in v1, so it must not be a way to
  // widen a role sideways.
  for (const role of ROLES) {
    for (const autosave of [true, false]) {
      const overridden = resolveCapabilities({ mode: role, autosave });
      const plain = resolveCapabilities({ mode: role });
      const moved = [
        ...new Set([...overridden, ...plain]),
      ].filter((c) => overridden.has(c) !== plain.has(c));
      assert.ok(
        moved.length === 0 || (moved.length === 1 && moved[0] === "autosave"),
        `?autosave=${autosave ? 1 : 0} on "${role}" also moved ${JSON.stringify(moved)}`,
      );
    }
  }
});

test("the sandbox withholds the tokens the capability set withholds", () => {
  // The browser-enforced layer: a frame without `allow-downloads` cannot start
  // a download however its chrome or its engine is persuaded.
  const reader = sandboxTokensFor(resolveCapabilities({ mode: "readonly" }));
  assert.equal(reader.includes("allow-downloads"), false, "a reader keeps no copy of the file");
  assert.equal(reader.includes("allow-modals"), true, "but it may open the print dialog");

  const preview = sandboxTokensFor(resolveCapabilities({ mode: "preview" }));
  assert.deepEqual([...preview], ["allow-scripts", "allow-same-origin"]);

  const editor = sandboxTokensFor(resolveCapabilities({ mode: "edit" }));
  assert.equal(editor.includes("allow-downloads"), true);

  // Never granted to anyone: these are how a framed page escapes its box.
  for (const role of ROLES) {
    const tokens = sandboxTokensFor(resolveCapabilities({ mode: role }));
    for (const forbidden of ["allow-top-navigation", "allow-popups", "allow-forms"]) {
      assert.equal(tokens.includes(forbidden), false, `${role} must not get ${forbidden}`);
    }
    // Without these two the editor cannot boot its own WebAssembly, and an
    // embed that cannot boot is not a permission decision.
    assert.ok(tokens.includes("allow-scripts") && tokens.includes("allow-same-origin"));
  }
});

test("the host inputs are read off a page without it having to be a browser", () => {
  // `hostCapabilities` is the only impure part, and it has to survive a
  // cross-origin parent — where touching `view.top` throws, and a throw means
  // we ARE framed.
  const framedThrower = {
    location: { search: "?mode=commentor" },
    get self() {
      return this;
    },
    get top() {
      throw new Error("cross-origin");
    },
  };
  assert.deepEqual([...hostCapabilities(framedThrower)].sort(), granted("commentor"));

  const topLevel = { location: { search: "" } };
  topLevel.self = topLevel;
  topLevel.top = topLevel;
  assert.deepEqual([...hostCapabilities(topLevel)].sort(), granted("owner"));

  // No location at all (a worker, a node harness): framed is unknown, so the
  // unframed default stands — and nothing throws.
  assert.ok(hostCapabilities({}) instanceof Set);
});
