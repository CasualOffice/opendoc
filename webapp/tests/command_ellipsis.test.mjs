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

/** The object literal that contains `index`: from the `{` before it to its
 *  matching `}`, skipping quoted strings. Labelled commands are object
 *  literals, one line or several. */
function objectAround(text, index) {
  let start = index;
  for (let depth = 0; start > 0; start -= 1) {
    const c = text[start];
    if (c === "}") depth += 1;
    else if (c === "{") {
      if (depth === 0) break;
      depth -= 1;
    }
  }
  let depth = 0;
  let quote = null;
  for (let i = start; i < text.length; i += 1) {
    const c = text[i];
    if (quote) {
      if (c === "\\") i += 1;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'" || c === "`") quote = c;
    else if (c === "{") depth += 1;
    else if (c === "}" && --depth === 0) return text.slice(start, i + 1);
  }
  return text.slice(start);
}

/** Every labelled command with the function its run/activate handler calls. */
function openers() {
  const out = [];
  for (const name of readdirSync(SRC).filter((n) => n.endsWith(".mjs"))) {
    const text = readFileSync(join(SRC, name), "utf8");
    for (const m of text.matchAll(/\b(?:id|command):\s*"([a-z][\w.]*)",\s*(?:\/\/[^\n]*\n\s*)*label:/g)) {
      const body = objectAround(text, m.index);
      const call = /\b(?:run|activate):\s*(?:async\s*)?\([^)]*\)\s*=>\s*(?:\{\s*)?(?:void\s+)?(?:await\s+)?([\w.]+)\(/.exec(body)?.[1];
      if (call) out.push({ id: m[1], call, file: name });
    }
  }
  return out;
}

// Declaring a command is opt-in, so the first test cannot see a dialog command
// nobody declared, nor a pane command declared as a picker: Indentation… and
// Paragraph spacing… were declared pickers while opening the Paragraph
// properties pane `paragraph.properties` opens as a pane, and "Add or edit link"
// opened the link popover undeclared, beside "Add link…" declared a dialog.
// Commands that run the SAME function open the same surface, so they must be
// declared the same way, or not at all. Mutation (kept in the commit message):
// declaring layout.indent a picker again, or dropping insert.link from the
// table, turns this red.
test("commands that open the same surface are declared to open the same kind", () => {
  const byCall = new Map();
  for (const entry of openers()) byCall.set(entry.call, [...(byCall.get(entry.call) ?? []), entry]);
  const split = [];
  for (const [call, entries] of byCall) {
    const kinds = new Set(entries.map(({ id }) => COMMAND_OPENS.get(id) ?? "undeclared"));
    if (kinds.size > 1) split.push(`${call}: ${entries.map(({ id }) => `${id}=${COMMAND_OPENS.get(id) ?? "undeclared"}`).join(", ")}`);
  }
  assert.deepEqual(split, []);
});

test("the opener scan sees the routes it is meant to compare", () => {
  const calls = new Map(openers().map(({ id, call }) => [id, call]));
  assert.equal(calls.get("layout.indent"), calls.get("layout.paragraph"));
  assert.equal(calls.get("insert.link"), calls.get("link.add"));
});

test("three dots are never typed for an ellipsis", () => {
  const dots = labels().filter(({ label }) => /\.\.\.\s*$/.test(label));
  assert.deepEqual(dots.map(({ id, label }) => `${id} "${label}"`), []);
});
