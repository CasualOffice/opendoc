// `109` UX-006 and UX-007, the half that needs no browser.
//
// The invariant that matters is not "the table has a row for ⌘E". It is that the
// chord a surface ADVERTISES and the chord the dispatcher BINDS are the same
// declaration — because they were two unrelated tables, and that is how the
// editor came to advertise chords it had never bound and to bind one (⌘⇧E) it
// advertised nowhere. So the central test here takes each row's label, builds the
// keystroke that label describes, and checks the dispatcher routes it back to the
// same command. A label that cannot be typed, or that types something else, fails.
//
// The browser half — that each chord reaches real behaviour rather than merely
// resolving to an id — is `tests/e2e/keyboard-chords.spec.mjs`. Neither half is
// sufficient: this file cannot tell whether `paragraph.align.center` does
// anything, and that file cannot enumerate the table cheaply.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { APPLE_PLATFORM, STANDARD_PLATFORM, matchesShortcut, parseShortcut } from "../src/keyboard.mjs";
import { APP_SCOPE, EDITOR_SCOPE, KEYMAP, chordCommand, shortcutForCommand } from "../src/keymap.mjs";

const MAIN_JS = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const PLATFORMS = [APPLE_PLATFORM, STANDARD_PLATFORM];

/** The keystroke a declared chord describes, as the fields a `keydown` carries.
 *
 *  This is the inverse of the matcher, written independently OF the matcher on
 *  purpose: if both sides shared one helper the round-trip test below would be
 *  comparing a function with itself and would pass for any table at all.
 */
function keystrokeFor(spec, platform) {
  const wanted = parseShortcut(spec);
  const apple = platform === APPLE_PLATFORM;
  return {
    key: wanted.key,
    metaKey: apple ? wanted.mod : false,
    ctrlKey: apple ? wanted.control : wanted.mod || wanted.control,
    altKey: wanted.alt,
    shiftKey: wanted.shift,
  };
}

function rowsFor(platform) {
  return KEYMAP.filter((row) => !row.platform || row.platform === platform);
}

test("the table is not empty, or every test below is vacuous", () => {
  assert.ok(KEYMAP.length >= 25, `only ${KEYMAP.length} chords declared`);
});

test("every declared chord is a keystroke a keyboard can actually produce", () => {
  const unparseable = KEYMAP.filter((row) => !parseShortcut(row.chord)).map((row) => row.chord);
  assert.deepEqual(
    unparseable,
    [],
    "a spec that does not parse is a binding that can never fire and a label nobody can press",
  );
});

// THE central invariant. `⌃Space` was caught by this shape of thinking: it
// rendered as a bare "⌃" in the palette, which is a label no one can act on.
test("the chord a command advertises is a chord that runs it", () => {
  for (const platform of PLATFORMS) {
    for (const row of rowsFor(platform)) {
      // 1. What the surfaces print for this command must itself be typeable and
      //    must reach this command. This is the direction UX-007 failed in.
      const advertised = shortcutForCommand(row.command, platform);
      assert.ok(advertised, `${row.command} advertises nothing on ${platform}`);
      const advertisedEvent = keystrokeFor(advertised, platform);
      if (advertisedEvent.metaKey || advertisedEvent.ctrlKey || advertisedEvent.altKey) {
        assert.equal(
          chordCommand(advertisedEvent, platform, { inEditor: true }),
          row.command,
          `${advertised} is advertised for ${row.command} on ${platform} but does not run it`,
        );
      }

      // 2. Every row's OWN chord reaches its own command, including the alternate
      //    spellings that share a command (⌘Y as well as ⌘⇧Z for redo). This is
      //    the direction UX-006 failed in: declared and never wired.
      const own = keystrokeFor(row.chord, platform);
      // A row with no command modifier is label-only by construction — Enter
      // belongs to the text-input path, which has four other meanings for it.
      if (!own.metaKey && !own.ctrlKey && !own.altKey) continue;
      assert.equal(
        chordCommand(own, platform, { inEditor: true }),
        row.command,
        `${row.chord} on ${platform} does not run ${row.command}`,
      );
    }
  }
});

test("no command advertises a chord that belongs to the other keyboard", () => {
  for (const platform of PLATFORMS) {
    for (const row of KEYMAP) {
      if (!row.platform || row.platform === platform) continue;
      // The row exists but is not for this keymap. If it is the ONLY row for its
      // command, the command must advertise nothing here rather than promise a
      // chord that is dead on the keyboard in front of the user.
      const others = KEYMAP.filter(
        (other) => other.command === row.command && (!other.platform || other.platform === platform),
      );
      if (others.length) continue;
      assert.equal(
        shortcutForCommand(row.command, platform),
        undefined,
        `${row.command} advertises ${row.chord} on ${platform}, where it is not bound`,
      );
    }
  }
});

test("no two chords collide on the same keyboard", () => {
  for (const platform of PLATFORMS) {
    const seen = new Map();
    for (const row of rowsFor(platform)) {
      const previous = seen.get(row.chord);
      assert.equal(
        previous,
        undefined,
        `${row.chord} is claimed by both ${previous} and ${row.command} on ${platform}; ` +
          "one of them would silently never fire",
      );
      seen.set(row.chord, row.command);
    }
  }
});

// SKILL.md §10: a control that does nothing is worse than no control. A chord
// naming a command that does not exist would be exactly that, and it would be
// invisible — the dispatcher would find nothing and fall through in silence.
test("every chord names a command the editor actually defines", () => {
  const missing = [...new Set(KEYMAP.map((row) => row.command))].filter(
    (id) => !MAIN_JS.includes(`id: "${id}"`),
  );
  assert.deepEqual(
    missing,
    [],
    "a chord bound to a command id that no descriptor defines is a dead chord: it " +
      "would preventDefault nothing and do nothing, with no error anywhere",
  );
});

test("every row declares a scope the dispatcher understands", () => {
  const bad = KEYMAP.filter((row) => ![APP_SCOPE, EDITOR_SCOPE].includes(row.scope));
  assert.deepEqual(bad.map((row) => row.chord), []);
});

// Scope is what keeps ⌘B from emboldening the document while the font search box
// has focus. Without it the editor's chords would fire over every chrome input.
test("an editor chord is ignored outside the editor; an app chord is not", () => {
  const editorRow = KEYMAP.find((row) => row.scope === EDITOR_SCOPE && parseShortcut(row.chord).mod);
  const appRow = KEYMAP.find((row) => row.scope === APP_SCOPE);
  for (const platform of PLATFORMS) {
    assert.equal(
      chordCommand(keystrokeFor(editorRow.chord, platform), platform, { inEditor: false }),
      null,
      `${editorRow.chord} must not fire from a chrome control`,
    );
    assert.equal(
      chordCommand(keystrokeFor(appRow.chord, platform), platform, { inEditor: false }),
      appRow.command,
      `${appRow.chord} must fire wherever the editor has focus`,
    );
  }
});

test("ordinary typing reaches no chord at all", () => {
  for (const key of ["a", "Z", " ", "Enter", "Backspace", "ArrowLeft"]) {
    for (const platform of PLATFORMS) {
      assert.equal(
        chordCommand({ key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false }, platform),
        null,
        `"${key}" must not be a chord`,
      );
    }
  }
});

// Each modifier is matched exactly, or a chord steals its own Shift variant and
// the user gets an edit they did not ask for.
test("a chord does not fire with an extra modifier held", () => {
  const base = { key: "b", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };
  assert.equal(matchesShortcut("⌘B", base, APPLE_PLATFORM), true);
  assert.equal(matchesShortcut("⌘B", { ...base, shiftKey: true }, APPLE_PLATFORM), false);
  assert.equal(matchesShortcut("⌘B", { ...base, altKey: true }, APPLE_PLATFORM), false);
  assert.equal(matchesShortcut("⌘B", { ...base, ctrlKey: true }, APPLE_PLATFORM), false);
});

// On Apple, Command and Control are different physical keys. Collapsing them —
// which the LABEL table does, because a Windows keyboard calls both "Ctrl" — would
// make ⌃Space fire on ⌘Space and clear a paragraph's formatting on a Mac.
test("Command and Control are the same key off Apple and different on it", () => {
  const ctrlSpace = { key: " ", metaKey: false, ctrlKey: true, altKey: false, shiftKey: false };
  const cmdSpace = { key: " ", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };
  assert.equal(matchesShortcut("⌃Space", ctrlSpace, APPLE_PLATFORM), true);
  assert.equal(matchesShortcut("⌃Space", cmdSpace, APPLE_PLATFORM), false);
  assert.equal(matchesShortcut("⌃Space", ctrlSpace, STANDARD_PLATFORM), true);
  // The Super/Windows key is not a chord modifier anywhere in this editor.
  assert.equal(matchesShortcut("⌘B", { key: "b", metaKey: true, ctrlKey: false }, STANDARD_PLATFORM), false);
  assert.equal(matchesShortcut("⌘B", { key: "b", metaKey: false, ctrlKey: true }, STANDARD_PLATFORM), true);
});

test("a chord is case-insensitive in the letter, as the keyboard reports it", () => {
  for (const key of ["b", "B"]) {
    assert.equal(
      matchesShortcut("⌘B", { key, metaKey: true, ctrlKey: false, altKey: false, shiftKey: false }, APPLE_PLATFORM),
      true,
      key,
    );
  }
});

test("a platform-narrowed chord fires only on its platform", () => {
  const row = KEYMAP.find((r) => r.platform);
  assert.ok(row, "the table no longer exercises the platform field");
  assert.equal(chordCommand(keystrokeFor(row.chord, row.platform), row.platform, {}), row.command);
  const other = row.platform === APPLE_PLATFORM ? STANDARD_PLATFORM : APPLE_PLATFORM;
  assert.notEqual(chordCommand(keystrokeFor(row.chord, other), other, {}), row.command);
});

// The chords `109` UX-006 names as missing, asserted as PRESENT so the row cannot
// be closed by a table that quietly dropped half of them. Each is cited from
// ONLYOFFICE's own default keymap in `src/keymap.mjs`.
test("the chords the row was opened for are all bound", () => {
  const required = {
    "⌘L": "paragraph.align.start",
    "⌘E": "paragraph.align.center",
    "⌘R": "paragraph.align.end",
    "⌘J": "paragraph.align.justify",
    "⌘⇧L": "paragraph.list.bullet",
    "⌘M": "paragraph.indent.increase",
    "⌘⇧M": "paragraph.indent.decrease",
    "⌘]": "format.grow",
    "⌘[": "format.shrink",
    "⌘⇧X": "format.strike",
  };
  for (const [chord, command] of Object.entries(required)) {
    assert.equal(
      chordCommand(keystrokeFor(chord, APPLE_PLATFORM), APPLE_PLATFORM, { inEditor: true }),
      command,
      `${chord} is not bound to ${command}`,
    );
  }
});

// UX-007: ⌘⇧E worked and was advertised nowhere, because only descriptors carry
// labels and it was a hand-written keydown branch.
test("the review-mode cycle is both bound and advertised", () => {
  assert.equal(
    chordCommand(keystrokeFor("⌘⇧E", APPLE_PLATFORM), APPLE_PLATFORM, {}),
    "review.mode.cycle",
  );
  assert.equal(shortcutForCommand("review.mode.cycle", APPLE_PLATFORM), "⌘⇧E");
});

test("no command descriptor hard-codes a shortcut label any more", () => {
  const literals = MAIN_JS.match(/shortcut: "/g) ?? [];
  assert.deepEqual(
    literals,
    [],
    "a hand-written label is a second source of truth, which is the whole of " +
      "UX-006/UX-007 — stamp it from `shortcutForCommand` instead",
  );
});
