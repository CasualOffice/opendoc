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
import { APP_SCOPE, EDITOR_SCOPE, KEYMAP, chordCommand, chordTitle, shortcutForCommand } from "../src/keymap.mjs";
import { QUICK_STYLES } from "../src/quick_styles.mjs";
import { REGION_COMMAND_IDS } from "../src/region_focus.mjs";

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
// naming a command the REGISTRY does not return is exactly that, and it is
// invisible — the dispatcher finds nothing and falls through in silence.
//
// This test used to grep `main.js` for `id: "<command>"` and it was GREEN while
// ⌘⌥M was dead: `comment.add` is the annotate surface's own id for adding a
// comment, the string is in the file, and `editorCommands()` does not return it.
// So the chord did nothing and the palette row showed its group where its hint
// belonged. Existence in the source is not membership in the registry, and only
// the second one is what the dispatcher looks a command up in. The registry is
// not importable (`main.js` has no exports, HF-085), so what is checked here is
// the set of ids `editorCommands` itself BUILDS — the array literals inside that
// one function — rather than any `id:` anywhere in the file.
const EDITOR_COMMANDS_SOURCE = (() => {
  const start = MAIN_JS.indexOf("function editorCommands(");
  assert.ok(start > 0, "editorCommands() has moved or been renamed");
  // Anchored on the `cmds.filter(` that ends the builder rather than on the whole
  // `return` line: the return is now wrapped — `SESSION.narrow(cmds.filter(…))`
  // applies the room's grant to every surface at once — and an anchor that
  // included the word `return` silently matched NOTHING, which made `end` fall
  // before `start` and took the whole scan with it.
  const returned = MAIN_JS.indexOf("cmds.filter(", start);
  assert.ok(returned > start, "editorCommands() no longer ends by filtering `cmds`");
  const end = MAIN_JS.indexOf("\n}", returned);
  assert.ok(end > start, "the end of editorCommands() could not be found");
  return MAIN_JS.slice(start, end);
})();

// The TABLE commands are not literals inside `editorCommands()` — that function
// pushes `tableToolCommands(...)` wholesale, so the tree in `table_commands.mjs`
// is genuinely in the registry while being invisible to the scan above. The push
// site is asserted, so this extension cannot outlive the wiring it stands for: if
// `editorCommands` stops composing that tree, the assertion fails here rather
// than letting four table chords quietly go dead.
const TABLE_COMMAND_IDS = (() => {
  assert.match(
    EDITOR_COMMANDS_SOURCE,
    /flattenCommandTree\(tableToolCommands\(/,
    "editorCommands() no longer composes tableToolCommands(); the table ids below " +
      "are no longer in the registry and every table chord is dead",
  );
  const source = readFileSync(new URL("../src/table_commands.mjs", import.meta.url), "utf8");
  return [...source.matchAll(/(?:id: |tableMutation\()"([\w.]+)"/g)].map((match) => match[1]);
})();

// The LINE-SPACING preset commands are generated from the popover's own markup —
// `id: \`paragraph.spacing.${percent}\`` over every `.spacing-line` button in
// `editor.html` — so they are genuinely in the registry while being invisible to
// a scan for literal ids, exactly like the table tree above. Both halves are
// asserted: the generator is still in `editorCommands`, and the percentages come
// from the markup rather than from a list here, so a preset removed from the
// popover takes its chord's target with it and this test says so.
const SPACING_COMMAND_IDS = (() => {
  const spacing = readFileSync(new URL("../src/spacing_menu.mjs", import.meta.url), "utf8");
  assert.match(
    spacing,
    /id: `paragraph\.spacing\.\$\{percent\}`/,
    "spacing_menu.mjs no longer generates the line-spacing preset commands; the " +
      "⌘1 / ⌘5 / ⌘2 chords are dead",
  );
  assert.match(
    EDITOR_COMMANDS_SOURCE,
    /spacingMenuControl\.commands\(/,
    "editorCommands() no longer composes the spacing command rows, so they are " +
      "not in the registry and the ⌘1 / ⌘5 / ⌘2 chords are dead",
  );
  const markup = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
  const ids = [...markup.matchAll(/spacing-line"[^>]*data-percent="(\d+)"/g)].map(
    (match) => `paragraph.spacing.${match[1]}`,
  );
  assert.ok(ids.length >= 4, `only ${ids.length} line-spacing presets found in the markup`);
  return ids;
})();

// The BREAK commands reach the registry through `LAYOUT_SURFACE`, which
// `editorCommands()` maps wholesale into palette rows with `id: entry.command` —
// so, like the two above, they are genuinely registered and invisible to a scan
// for literal ids. ⌘⏎ is bound to one of them, which is the whole of `104`
// HF-127's fix, so a broken link here would put that chord straight back where
// it was: present in the table and inert at the keyboard.
//
// BOTH links are asserted, because either one breaking is enough to kill the
// chord: `editorCommands` must still map `LAYOUT_SURFACE`, and `LAYOUT_SURFACE`
// must still spread the break rows into itself. The ids themselves come from
// `break_commands.mjs` rather than being listed here, so a break removed from
// that table takes its chord's target with it and this test says so.
const BREAK_COMMAND_IDS = (() => {
  assert.match(
    EDITOR_COMMANDS_SOURCE,
    /\.\.\.LAYOUT_SURFACE\.filter\(\(entry\) => entry\.label\)\.map\(/,
    "editorCommands() no longer maps LAYOUT_SURFACE into palette rows, so the " +
      "break commands are not in the registry and ⌘⏎ is dead",
  );
  assert.match(
    MAIN_JS,
    /\.\.\.breakSurfaceRows\(/,
    "LAYOUT_SURFACE no longer spreads the break rows, so ⌘⏎ has nothing to run",
  );
  const source = readFileSync(new URL("../src/break_commands.mjs", import.meta.url), "utf8");
  const ids = [...source.matchAll(/command: "([\w.]+)"/g)].map((match) => match[1]);
  const sections = [...source.matchAll(/command: `layout\.break\.section\.\$\{start\}`/g)];
  assert.ok(
    ids.length >= 2 && sections.length === 1,
    `break_commands.mjs declares ${ids.length} literal break ids and ` +
      `${sections.length} generated section families; the scan has drifted`,
  );
  const starts = /SECTION_STARTS = Object\.freeze\(\[([^\]]+)\]\)/.exec(source);
  assert.ok(starts, "SECTION_STARTS has moved, so the four section ids cannot be derived");
  return [
    ...ids,
    ...[...starts[1].matchAll(/"(\w+)"/g)].map((match) => `layout.break.section.${match[1]}`),
  ];
})();

// `formatting_marks.mjs` generates its command rows from its own table, the way
// `break_commands.mjs` and the spacing and table modules do, so the scan over
// `main.js` cannot see them: `main.js` carries one `...formattingMarks.commands()`
// spread instead of six literals. Derived from the module's own frozen
// declaration rather than re-listed here, because a second copy of the ids is
// exactly the drift this file exists to catch. Read as SOURCE and not imported,
// for the same reason the three lists above are: the factory reaches for
// `document`, which does not exist under `node --test`.
const FORMATTING_MARK_COMMAND_IDS = (() => {
  const source = readFileSync(new URL("../src/formatting_marks.mjs", import.meta.url), "utf8");
  const marks = /FORMATTING_MARKS = Object\.freeze\(\[([\s\S]*?)\]\);/.exec(source);
  assert.ok(marks, "FORMATTING_MARKS has moved, so the switch ids cannot be derived");
  const keys = [...marks[1].matchAll(/key: "(\w+)"/g)].map((match) => match[1]);
  assert.ok(keys.length >= 5, `only ${keys.length} marks found; the scan has drifted`);
  return ["view.formattingMarks", ...keys.map((key) => `view.formattingMarks.${key}`)];
})();

// The heading chords' four commands are generated by `quick_styles.mjs` and the
// two region commands by `region_focus.mjs`, each spread into the registry by one
// call — so, like the families above, they are registered and invisible to a scan
// for literal ids. Both spreads are asserted, so a registry that stops composing
// either one fails HERE rather than leaving Ctrl+Alt+1 or F6 bound to nothing.
const QUICK_STYLE_COMMAND_IDS = (() => {
  assert.match(
    EDITOR_COMMANDS_SOURCE,
    /quickStyleCommands\(/,
    "editorCommands() no longer composes the heading commands; Ctrl+Alt+1/2/3/0 are dead",
  );
  return QUICK_STYLES.map((row) => row.id);
})();
const REGION_IDS = (() => {
  assert.match(
    EDITOR_COMMANDS_SOURCE,
    /regionCycle\.commands\(\)/,
    "editorCommands() no longer composes the region commands; F6 and Shift+F6 are dead",
  );
  return [...REGION_COMMAND_IDS];
})();

test("every chord names a command the registry actually returns", () => {
  const registered = new Set([
    ...[...EDITOR_COMMANDS_SOURCE.matchAll(/id: "([\w.]+)"/g)].map((match) => match[1]),
    ...TABLE_COMMAND_IDS,
    ...SPACING_COMMAND_IDS,
    ...BREAK_COMMAND_IDS,
    ...FORMATTING_MARK_COMMAND_IDS,
    ...QUICK_STYLE_COMMAND_IDS,
    ...REGION_IDS,
  ]);
  assert.ok(registered.size > 80, `only ${registered.size} commands found; the scan has drifted`);
  const missing = [...new Set(KEYMAP.map((row) => row.command))].filter((id) => !registered.has(id));
  assert.deepEqual(
    missing,
    [],
    "a chord bound to an id the command registry does not return is a dead chord: " +
      "it would preventDefault nothing and do nothing, with no error anywhere, and " +
      "its palette row would print its group where the hint belongs",
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

// Ctrl+H — Find and Replace in Word, Google Docs and ONLYOFFICE — did nothing.
// The Apple row is ⌘⇧H, not ⌘H: on a Mac ⌘H is the system's Hide, which a page
// never receives, so binding it there would advertise a chord that hides the
// browser instead.
test("Ctrl+H opens Replace on a PC, ⌘⇧H on a Mac, and ⌘H is not claimed on a Mac", () => {
  const ctrlH = { key: "h", metaKey: false, ctrlKey: true, altKey: false, shiftKey: false };
  assert.equal(chordCommand(ctrlH, STANDARD_PLATFORM, { inEditor: true }), "edit.replace");
  // APP scope: it must work from the Find field and the ribbon, not only the page.
  assert.equal(chordCommand(ctrlH, STANDARD_PLATFORM, { inEditor: false }), "edit.replace");
  const cmdShiftH = { key: "H", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true };
  assert.equal(chordCommand(cmdShiftH, APPLE_PLATFORM, {}), "edit.replace");
  const cmdH = { key: "h", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };
  assert.equal(chordCommand(cmdH, APPLE_PLATFORM, {}), null);
  assert.equal(shortcutForCommand("edit.replace", STANDARD_PLATFORM), "⌘H");
  assert.equal(shortcutForCommand("edit.replace", APPLE_PLATFORM), "⌘⇧H");
  // The Replace control's tooltip is read from the same row, per platform.
  assert.equal(chordTitle("Replace", "edit.replace", STANDARD_PLATFORM), "Replace (Ctrl+H)");
  assert.equal(chordTitle("Replace", "edit.replace", APPLE_PLATFORM), "Replace (⌘⇧H)");
});

// Word's Ctrl+Alt+1/2/3 and Google Docs' Ctrl+Alt+0. They must not be mistaken
// for — or steal — the ⌘1 / ⌘2 / ⌘5 line-spacing chords, which differ only by Alt.
test("the heading chords apply Heading 1/2/3 and Normal, distinct from line spacing", () => {
  const expected = {
    "⌘⌥1": "paragraph.heading.1",
    "⌘⌥2": "paragraph.heading.2",
    "⌘⌥3": "paragraph.heading.3",
    "⌘⌥0": "paragraph.normal",
    "⌘1": "paragraph.spacing.100",
    "⌘2": "paragraph.spacing.200",
  };
  for (const platform of PLATFORMS) {
    for (const [chord, command] of Object.entries(expected)) {
      assert.equal(
        chordCommand(keystrokeFor(chord, platform), platform, { inEditor: true }),
        command,
        `${chord} on ${platform}`,
      );
    }
  }
  // Editor scope: in the font box, Ctrl+Alt+1 is the field's business.
  assert.equal(
    chordCommand(keystrokeFor("⌘⌥1", STANDARD_PLATFORM), STANDARD_PLATFORM, { inEditor: false }),
    null,
  );
});

// A Mac rewrites the character Option produces (⌥1 is "¡"), so the chord has to
// be read from the physical key there — and ONLY there: Ctrl+Alt is AltGr on
// German, Polish and French keyboards, where the same keystroke types a character
// that must reach the document.
test("⌘⌥1 fires on a Mac whose key reports '¡'; AltGr+0 typing '}' is never a chord", () => {
  const mac = { key: "¡", code: "Digit1", metaKey: true, ctrlKey: false, altKey: true, shiftKey: false };
  assert.equal(chordCommand(mac, APPLE_PLATFORM, { inEditor: true }), "paragraph.heading.1");
  // And the same rescue lifts ⌘⌥M (Option+M is "µ"), which was dead the same way.
  const macM = { key: "µ", code: "KeyM", metaKey: true, ctrlKey: false, altKey: true, shiftKey: false };
  assert.equal(chordCommand(macM, APPLE_PLATFORM, { inEditor: true }), "review.comment");
  const altGr = { key: "}", code: "Digit0", metaKey: false, ctrlKey: true, altKey: true, shiftKey: false };
  assert.equal(chordCommand(altGr, STANDARD_PLATFORM, { inEditor: true }), null);
  // Option alone is how a Mac TYPES those characters; it must never be a chord.
  const typing = { key: "¡", code: "Digit1", metaKey: false, ctrlKey: false, altKey: true, shiftKey: false };
  assert.equal(chordCommand(typing, APPLE_PLATFORM, { inEditor: true }), null);
});

// F6 / Shift+F6 carry no modifier, which the dispatcher used to refuse outright
// before looking at the table. A function key never types, so it is admitted —
// and nothing else that carries no modifier is.
test("F6 and Shift+F6 move between regions, and no typed key does", () => {
  for (const platform of PLATFORMS) {
    const f6 = { key: "F6", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false };
    assert.equal(chordCommand(f6, platform, { inEditor: true }), "view.region.next");
    assert.equal(chordCommand(f6, platform, { inEditor: false }), "view.region.next");
    assert.equal(chordCommand({ ...f6, shiftKey: true }, platform, {}), "view.region.previous");
    for (const key of ["F", "f", "6", "Fn"]) {
      assert.equal(chordCommand({ ...f6, key }, platform, {}), null, `"${key}" must not be a chord`);
    }
  }
  assert.equal(shortcutForCommand("view.region.next", STANDARD_PLATFORM), "F6");
  assert.equal(shortcutForCommand("view.region.previous", STANDARD_PLATFORM), "⇧F6");
});
