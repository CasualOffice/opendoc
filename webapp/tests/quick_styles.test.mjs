// Which stored style "Heading 2" means in the open document — the one rule
// behind Ctrl+Alt+1/2/3 and Ctrl+Alt+0.
//
// `setParagraphStyle` looks a style up by EXACT stored name, and Word stores its
// heading as `heading 2` while other producers write `Heading 2`, so a chord that
// asked for the literal "Heading 2" found nothing in a Word-authored file.
import { test } from "node:test";
import assert from "node:assert/strict";

import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale } from "../src/i18n.mjs";
import { QUICK_STYLES, quickStyleCommands, storedStyleFor } from "../src/quick_styles.mjs";

setCatalogue("en", { ...EN_STRINGS });
setLocale("en");

test("a heading resolves to the document's own spelling of it", () => {
  assert.equal(storedStyleFor("Heading 2", ["Normal", "heading 1", "heading 2"]), "heading 2");
  assert.equal(storedStyleFor("Heading 2", ["Normal", "Heading 2"]), "Heading 2");
  assert.equal(storedStyleFor("Heading 3", ["Normal", "heading 1"]), null);
  // A custom style that merely mentions a heading is not the built-in.
  assert.equal(storedStyleFor("Heading 1", ["My heading 1"]), null);
});

test("the commands apply the stored name, refuse a missing heading, and clear to Normal", () => {
  const applied = [];
  const commands = quickStyleCommands({
    styles: () => ["Body Text", "heading 1"],
    hasCaret: () => true,
    apply: (name) => applied.push(name),
  });
  const byId = new Map(commands.map((command) => [command.id, command]));
  assert.deepEqual([...byId.keys()], QUICK_STYLES.map((row) => row.id));

  byId.get("paragraph.heading.1").run();
  assert.deepEqual(applied, ["heading 1"]);

  const missing = byId.get("paragraph.heading.2");
  assert.equal(missing.enabled, false);
  assert.equal(missing.disabledReason, "This document has no Heading 2 style");

  // No style is NAMED Normal here, so Normal clears to the default paragraph
  // style — which is what Normal is — rather than being refused.
  const normal = byId.get("paragraph.normal");
  assert.equal(normal.enabled, true);
  normal.run();
  assert.deepEqual(applied, ["heading 1", ""]);
  assert.equal(byId.get("paragraph.heading.3").label, "Apply Heading 3");
});

test("with no paragraph to style, every heading command says so", () => {
  const commands = quickStyleCommands({ styles: () => ["heading 1"], hasCaret: () => false, apply: () => {} });
  for (const command of commands) {
    assert.equal(command.enabled, false);
    assert.equal(command.disabledReason, "Place the caret in a paragraph");
  }
});
