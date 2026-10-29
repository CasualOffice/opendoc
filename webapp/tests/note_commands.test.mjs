// Footnotes and endnotes (`docs/153` page.note.settings, page.note.goto,
// page.note.convert; `docs/109` RM-24): the parts of `note_commands.mjs` that
// need no browser — the JSON the options dialog hands the engine, the kinds'
// choices, and that the menu, the contract and the command table name the same
// six commands. The gestures are `note-commands.spec.mjs`'s.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

globalThis.document = { getElementById: () => null, addEventListener() {} };

const { NOTE_COMMANDS, NOTE_POSITIONS, NOTE_RESTARTS, notePropertiesJson, noteSurfaceRows } = await import(
  "../src/note_commands.mjs"
);
const { commandContract } = await import("../src/host_contract.mjs");

test("the dialog hands the engine the model's NoteProperties, with the start clamped to its domain", () => {
  // Mutation: send `start` under its own name -> the engine's
  // `deny_unknown_fields` refuses it, and this shape assertion goes red first.
  assert.deepEqual(
    JSON.parse(notePropertiesJson({ position: "pageBottom", numberFormat: "lowerRoman", start: "3", restart: "eachPage" })),
    { position: "pageBottom", numberFormat: "lowerRoman", numberStart: 3, numberRestart: "eachPage" },
  );
  assert.equal(JSON.parse(notePropertiesJson({ position: "x", numberFormat: "decimal", start: "99999", restart: "continuous" })).numberStart, 32767);
  assert.equal(JSON.parse(notePropertiesJson({ position: "x", numberFormat: "decimal", start: "-4", restart: "continuous" })).numberStart, 0);
  assert.equal(JSON.parse(notePropertiesJson({ position: "x", numberFormat: "decimal", start: "abc", restart: "continuous" })).numberStart, 1);
  // A format the dialog has no row for travels as the value it read.
  assert.deepEqual(
    JSON.parse(notePropertiesJson({ position: "x", numberFormat: { other: "chicago" }, start: 1, restart: "continuous" })).numberFormat,
    { other: "chicago" },
  );
});

test("each kind offers only the locations and restarts Word offers it", () => {
  assert.deepEqual(NOTE_POSITIONS.footnote, ["pageBottom", "beneathText"]);
  assert.deepEqual(NOTE_POSITIONS.endnote, ["sectionEnd", "documentEnd"]);
  assert.ok(NOTE_RESTARTS.footnote.includes("eachPage"));
  assert.ok(!NOTE_RESTARTS.endnote.includes("eachPage"), "Word restarts endnotes per section, never per page");
});

test("the menu, the host contract and the command table name the same six commands", () => {
  const editor = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
  const start = editor.indexOf('id="refNotesMenu"');
  // The menu's own closing tag, not the divider's inside it.
  const menu = editor.slice(start, editor.indexOf("\n    </div>", start));
  const rows = [...menu.matchAll(/data-command="([^"]+)"/g)].map((match) => match[1]);
  assert.deepEqual(rows, NOTE_COMMANDS.map((row) => row.command));
  for (const row of NOTE_COMMANDS) {
    const contract = commandContract(row.command);
    assert.ok(contract, `${row.command} is in the host contract`);
    // Going to a note changes nothing; the dialog and Convert change the document.
    assert.equal(contract.requires, row.kind ? null : "mutate", row.command);
  }
  assert.deepEqual(noteSurfaceRows({}).map((row) => row.command), NOTE_COMMANDS.map((row) => row.command));
});
