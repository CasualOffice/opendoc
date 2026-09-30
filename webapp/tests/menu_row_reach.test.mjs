// The application-menu row reach contract, enforced across the browser suite.
//
// Eighteen bands in the menu bar fold into submenu flyouts, so a row a spec
// names may be one level in. A folded row is IN THE DOM and `hidden`, which is
// the trap: a raw `#appMenuPopover .app-menu-item[data-command="…"]` locator
// RESOLVES. Playwright then waits for an element the locator can see and the
// reader cannot, so the failure is not "no such element" — it is a sixty-second
// timeout, and inside `command-activation-contract.spec.mjs` a four-minute test
// timeout on five tests at once.
//
// That is exactly how `main` went red: promoting bands into submenus broke
// specs that had never mentioned submenus, and the number of specs one
// promotion can break is not bounded by the promotion. Patching them one CI
// failure at a time is how the class returns on the next promotion — so this
// test fails the build if any spec reaches for a row directly again.
//
// Use `revealMenuRow(page, commandId)` from `fixtures.mjs`, or the helpers built
// on it (`menuCommandRow`, `runAppMenuCommand`). Enumeration is deliberately
// still allowed: `$$eval` over `#appMenuPopover .app-menu-item` reads the whole
// band INCLUDING its folded rows, which is what the sweeps want.
import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const E2E = join(dirname(fileURLToPath(import.meta.url)), "e2e");

/** A raw row locator, followed by the things that need the row REVEALED first.
 *  Allows whitespace and line breaks between the two, because prettier puts the
 *  `.click()` on its own line as soon as the locator is long. */
const ROW = String.raw`#appMenuPopover[^"'\`]*\.app-menu-item\[data-command=`;
const BANNED = [
  // page.locator(`#appMenuPopover .app-menu-item[data-command="${id}"]`).click()
  new RegExp(ROW + String.raw`[^;]*?\)\s*\.click\(`, "s"),
  // expect(page.locator('#appMenuPopover .app-menu-item[data-command="x"]')).toBeVisible()
  new RegExp(ROW + String.raw`[^;]*?\)\s*\)?\s*\.(?:toBeVisible|toBeEnabled)\(`, "s"),
];

test("no spec reaches an application-menu row without revealing it", () => {
  const offenders = [];
  for (const name of readdirSync(E2E)) {
    if (!name.endsWith(".spec.mjs")) continue;
    // Normalise line endings: a Windows checkout must read the same as a POSIX
    // one, or this guard passes on one platform and fails on the other.
    const text = readFileSync(join(E2E, name), "utf8").replace(/\r\n/g, "\n");
    for (const statement of text.split(";")) {
      if (BANNED.some((pattern) => pattern.test(statement))) {
        offenders.push(`${name}  ${statement.trim().replace(/\s+/g, " ").slice(0, 160)}`);
      }
    }
  }
  assert.deepEqual(
    offenders,
    [],
    "these reach an application-menu row directly, which breaks the moment its " +
      "band folds into a submenu; use revealMenuRow(page, commandId) from " +
      "fixtures.mjs:\n  " +
      offenders.join("\n  "),
  );
});

test("the helper every spec is pointed at still exists", () => {
  // A contract that names a replacement must keep the replacement real: without
  // this, deleting `revealMenuRow` would leave a guard telling everyone to use
  // something that is gone, and the guard itself would still pass.
  const fixtures = readFileSync(join(E2E, "fixtures.mjs"), "utf8");
  assert.match(
    fixtures,
    /export async function revealMenuRow\(page, commandId\)/,
    "fixtures.mjs must export revealMenuRow(page, commandId)",
  );
});
