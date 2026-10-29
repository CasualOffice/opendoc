// Review ▸ Display for review (`docs/153` review.display-mode-simple-markup,
// review.display-mode-original, review.display-mode-final; `docs/109` RM-31).
//
// The mode is DERIVED from what the canvas holds — the Original copy, the
// markup layout, one flag for Simple vs No markup — so these drive the picker
// against a fake editor context and assert what each mode makes the overlay
// paint and which engine switch it throws. The canvas itself is
// `review-display.spec.mjs`'s.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

// `popover_manager.mjs` listens on `document` at import, and the picker reads its
// markup by id; neither exists in node, so both are stood in for.
const elements = new Map();
globalThis.document = {
  getElementById: (id) => elements.get(id) ?? null,
  addEventListener() {},
};

const { DISPLAY_MODES, overlayFor, createReviewDisplay } = await import("../src/review_display.mjs");
const { commandContract } = await import("../src/host_contract.mjs");

test("each mode paints what Word's does, and only All markup paints the inline markers", () => {
  // Mutation: `revisionMarkers: mode !== "none"` -> Simple markup paints the
  // inline markers it exists to replace, and the first deepEqual goes red.
  assert.deepEqual(overlayFor("simple"), { revisionMarkers: false, changeBars: true, commentMarkers: true });
  assert.deepEqual(overlayFor("all"), { revisionMarkers: true, changeBars: false, commentMarkers: true });
  assert.deepEqual(overlayFor("none"), { revisionMarkers: false, changeBars: false, commentMarkers: false });
  assert.deepEqual(overlayFor("original"), { revisionMarkers: false, changeBars: false, commentMarkers: true });
});

test("the four modes are Word's, in Word's order, and each is a host-contract view command", () => {
  assert.deepEqual(DISPLAY_MODES.map((row) => row.mode), ["simple", "all", "none", "original"]);
  for (const row of DISPLAY_MODES) {
    const contract = commandContract(row.command);
    assert.ok(contract, `${row.command} is in the host contract`);
    assert.equal(contract.requires, null, `${row.command} is a view and requires nothing`);
  }
  // The popover's rows name the same four commands, so the markup cannot drift.
  const editor = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
  for (const row of DISPLAY_MODES) {
    assert.match(editor, new RegExp(`data-display-mode="${row.mode}" data-command="${row.command.replaceAll(".", "\\.")}"`));
  }
});

/** A fake editor context: a document, the markup switch, and a recorder. */
function fakeEditor() {
  const calls = [];
  const ed = {
    doc: { id: "live" },
    showingChanges: true,
    versionPreviewHome: null,
    async setShowingChanges(on) {
      calls.push(["markup", on]);
      ed.showingChanges = on;
    },
    async showVersionPreview(previewDoc) {
      calls.push(["preview", previewDoc ? "copy" : null]);
    },
    drawSelection() {},
    announceReview(text) {
      calls.push(["announce", text]);
    },
    setStatus(text, kind) {
      calls.push(["status", text, kind]);
    },
  };
  return { ed, calls };
}

test("Simple and No markup take the markup layout off; All markup puts it back; the mode follows", async () => {
  const { ed, calls } = fakeEditor();
  const display = createReviewDisplay(ed);
  assert.equal(display.mode(), "all", "markup on reads as All markup");
  await display.setMode("simple");
  assert.equal(display.mode(), "simple");
  assert.deepEqual(calls.filter(([kind]) => kind === "markup"), [["markup", false]]);
  await display.setMode("none");
  assert.equal(display.mode(), "none");
  assert.deepEqual(display.overlay(), overlayFor("none"));
  await display.setMode("all");
  assert.equal(display.mode(), "all");
  // Something ELSE turning the markup on — Suggesting, the View menu's toggle —
  // is All markup by derivation, with no second state to fall out of step.
  ed.showingChanges = false;
  assert.equal(display.mode(), "none", "the No-markup choice is remembered while markup is off");
  ed.showingChanges = true;
  assert.equal(display.mode(), "all");
});

test("Original is refused while a version preview is already up, and nothing is swapped", async () => {
  const { ed, calls } = fakeEditor();
  ed.versionPreviewHome = { doc: ed.doc };
  const display = createReviewDisplay(ed);
  await display.setMode("original");
  assert.equal(display.mode(), "all");
  assert.equal(calls.some(([kind]) => kind === "preview"), false);
  assert.equal(calls.at(-1)[0], "status");
});

test("the palette rows are the four modes, live only with a document", () => {
  const { ed } = fakeEditor();
  const display = createReviewDisplay(ed);
  assert.deepEqual(
    display.commands().map((row) => row.id),
    DISPLAY_MODES.map((row) => row.command),
  );
  assert.ok(display.commands().every((row) => row.enabled));
  ed.doc = null;
  assert.ok(display.commands().every((row) => !row.enabled && row.disabledReason));
});
