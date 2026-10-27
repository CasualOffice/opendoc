// Editor ↔ agent parity over the capability registry (`docs/181` §3, G3).
//
// `generated/commands.json` is the Rust registry (`crates/casual-doc-command`)
// as data, held current by `opendoc-command-gen --check` in `build.sh` and by
// the crate's `tests/generated.rs`. This file holds it to the editor's host
// contract (`src/host_contract.mjs`), in both directions:
//
//   1. Every id a command names as its editor twin (`uiCommands`) and every id
//      the registry lists as not exposed (`uiNotExposed`) is a real contract id:
//      an exact row, or a member or prefix of a family row.
//   2. Every contract id that can CHANGE THE DOCUMENT is covered exactly once:
//      named by a command, or listed as not exposed with a reason.
//
// THE "CHANGES THE DOCUMENT" RULE, from the contract's own data. A row's
// `requires` is what the host must grant for it to run (`REQUIREMENTS`):
//
//   mutate   "may this page change the document at all" (formatting, inserts,
//            layout, structure); resolved through `editingModeFor`
//   edit     the edit grant: accepting/rejecting, protection, direction
//   comment  the comment grant: commenting, resolving, deleting comments
//
// Those three are the requirements a document change needs, so a row requiring
// one of them is in scope. Everything else (`null`, `save`, `download`,
// `print`, `open`, `new`, `autosave`) is navigation, view, file or session
// state and is out of scope. The rule is deliberately the broad one: the few
// rows it sweeps in that change the editor rather than the document
// (`review.mode.editing`, `review.mode.suggesting`) are listed as not exposed
// with that reason, so the rule never needs an exception list of its own.
//
// Units of coverage: every in-scope exact row; every in-scope family PREFIX
// (`style.`), covered only by an entry equal to the prefix; and every member
// of an in-scope family the webapp enumerates statically (`TABLE_MENU_LABELS`
// for `table.`), which resolves to its family row through `commandContract`.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import { COMMAND_CONTRACT, COMMAND_FAMILIES, commandContract } from "../src/host_contract.mjs";
import { TABLE_MENU_LABELS } from "../src/command_taxonomy.mjs";

const registry = JSON.parse(readFileSync(new URL("../generated/commands.json", import.meta.url), "utf8"));

/** The requirements a document change needs (see the header). */
const CHANGES_DOCUMENT = new Set(["mutate", "edit", "comment"]);

/** Static enumerations of family members, keyed by family prefix. */
const ENUMERATED_MEMBERS = new Map([["table.", [...TABLE_MENU_LABELS.keys()]]]);

const twins = registry.commands.flatMap((command) => command.uiCommands.map((id) => ({ id, by: command.id })));
const notExposed = registry.uiNotExposed;

function isFamilyPrefix(id) {
  return COMMAND_FAMILIES.some((row) => row.prefix === id);
}

/** Every contract id that can change the document, with what it is. */
function unitsInScope() {
  const units = [];
  for (const row of COMMAND_CONTRACT) {
    if (CHANGES_DOCUMENT.has(row.requires)) units.push({ id: row.id, kind: "exact" });
  }
  for (const row of COMMAND_FAMILIES) {
    if (!CHANGES_DOCUMENT.has(row.requires)) continue;
    units.push({ id: row.prefix, kind: "family" });
    for (const member of ENUMERATED_MEMBERS.get(row.prefix) ?? []) {
      const resolved = commandContract(member);
      // An exact row wins over its family (`table.select.row` is an exact row
      // with no requirement); only members that resolve to this family count.
      if (resolved && resolved.family === row.prefix && CHANGES_DOCUMENT.has(resolved.requires)) {
        units.push({ id: member, kind: "member" });
      }
    }
  }
  return units;
}

test("the registry snapshot is the generated one, with its vocabulary version", () => {
  assert.match(registry.generatedBy, /opendoc-command-gen/);
  assert.match(registry.vocabularyVersion, /^\d+\.\d+$/);
  assert.ok(registry.commands.length > 0);
  assert.equal(notExposed.length, registry.uiNotExposedCeiling, "the ratchet ceiling travels with the list");
});

test("every editor twin a command names is a real host-contract id", () => {
  for (const { id, by } of twins) {
    assert.ok(
      commandContract(id) !== null || isFamilyPrefix(id),
      `\`${by}\` names \`${id}\` as its editor twin, but the host contract has no such command or family`,
    );
  }
});

test("every editor id listed as not exposed is a real host-contract id that can change the document", () => {
  for (const { id, reason } of notExposed) {
    const row = commandContract(id);
    assert.ok(row !== null || isFamilyPrefix(id), `\`${id}\` is listed as not exposed but is not in the host contract`);
    const requires = row ? row.requires : COMMAND_FAMILIES.find((family) => family.prefix === id).requires;
    assert.ok(
      CHANGES_DOCUMENT.has(requires),
      `\`${id}\` is listed as not exposed but requires \`${requires}\`, so it cannot change the document; drop it`,
    );
    assert.ok(reason.length > 20, `\`${id}\` needs a reason a user can read`);
  }
});

test("every host-contract id that can change the document is named by a command or listed as not exposed, once", () => {
  const units = unitsInScope();
  assert.ok(units.length > 100, `the scope rule found ${units.length} ids; the contract has well over a hundred`);
  const named = new Map(twins.map(({ id, by }) => [id, by]));
  const excluded = new Set(notExposed.map(({ id }) => id));
  const missing = [];
  for (const unit of units) {
    const isNamed = named.has(unit.id);
    const isExcluded = excluded.has(unit.id);
    assert.ok(
      !(isNamed && isExcluded),
      `\`${unit.id}\` is named by \`${named.get(unit.id)}\` and also listed as not exposed; remove it from UI_NOT_EXPOSED`,
    );
    if (!isNamed && !isExcluded) missing.push(`${unit.id} (${unit.kind})`);
  }
  assert.deepEqual(
    missing,
    [],
    "these editor commands can change the document but no registry command is their agent twin and " +
      "crates/casual-doc-command/src/registry.rs UI_NOT_EXPOSED does not list them with a reason " +
      "(docs/181 G3); add a command, or a not-exposed entry, then regenerate",
  );
  const scope = new Set(units.map((unit) => unit.id));
  for (const { id } of notExposed) {
    assert.ok(scope.has(id), `\`${id}\` is listed as not exposed but is not an id the scope rule covers`);
  }
});
