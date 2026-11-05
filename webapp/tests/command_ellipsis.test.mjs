// The ellipsis taxonomy (audit DIALOGS-16, `docs/184` D-13): a command label
// ends in "…" exactly when the command opens a dialog or a picker, per
// `COMMAND_OPENS` in `command_taxonomy.mjs`; a pane, and a command that acts at
// once, never has one. Read from the source, every surface that declares a
// label: the palette registry, the Insert/Layout/References surfaces, the
// context menus and the object and table command tables.
//
// Mutation (kept in the commit message): putting the "…" back on "Paragraph
// properties…" or taking it off "Page setup…" turns this red.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { COMMAND_OPENS, labelTakesEllipsis } from "../src/command_taxonomy.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

const SRC = join(dirname(fileURLToPath(import.meta.url)), "..", "src");

/** Every `{ id|command: "x", label: "y" | t("key") }` pair in the source. */
function labels() {
  const out = [];
  for (const name of readdirSync(SRC).filter((n) => n.endsWith(".mjs"))) {
    const text = readFileSync(join(SRC, name), "utf8");
    for (const m of text.matchAll(/\b(?:id|command):\s*"([a-z][\w.]*)",\s*(?:\/\/[^\n]*\n\s*)*label:\s*(?:"([^"]*)"|t\("([^"]+)"\))/g)) {
      const label = m[2] ?? EN_STRINGS[m[3]];
      if (label !== undefined) out.push({ id: m[1], label, file: name });
    }
  }
  return out;
}

test("the source declares enough labels for this to mean something", () => {
  assert.ok(labels().length > 150, `found ${labels().length} labels`);
});

test("a command that opens a dialog or a picker ends in an ellipsis, and nothing else does", () => {
  const wrong = [];
  for (const { id, label, file } of labels()) {
    const has = label.trimEnd().endsWith("…");
    if (labelTakesEllipsis(id) && !has) wrong.push(`${file}: ${id} "${label}" opens a ${COMMAND_OPENS.get(id)} and needs "…"`);
    if (!labelTakesEllipsis(id) && has) wrong.push(`${file}: ${id} "${label}" ${COMMAND_OPENS.get(id) === "pane" ? "opens a pane" : "is not declared to open a dialog"} and must not end in "…"`);
  }
  assert.deepEqual(wrong, []);
});

test("every command the taxonomy names is labelled somewhere", () => {
  const ids = new Set(labels().map((l) => l.id));
  const stale = [...COMMAND_OPENS.keys()].filter((id) => !ids.has(id));
  assert.deepEqual(stale, [], "COMMAND_OPENS names commands no surface labels");
});

test("three dots are never typed for an ellipsis", () => {
  const dots = labels().filter(({ label }) => /\.\.\.\s*$/.test(label));
  assert.deepEqual(dots.map(({ id, label }) => `${id} "${label}"`), []);
});
