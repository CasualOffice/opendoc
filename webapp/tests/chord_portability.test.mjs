// No spec may hardcode a platform modifier for a chord the keymap declares.
//
// This guard exists because a keymap rewrite (`109` UX-006/UX-007, #623) turned
// nine spec lines into keystrokes that reach nothing, and `main` went red with
// "undo is broken" — which it was not.
//
// `keymap.mjs` declares every chord in Apple notation and `matchesShortcut`
// resolves `⌘` against the platform the editor is running on: Command on Apple,
// Control everywhere else, and EXACTLY one of them. Before #623 the same chords
// were `if (e.metaKey || e.ctrlKey)` branches, so both modifiers worked on both
// platforms and a spec could hardcode either and still pass anywhere. That
// looseness is gone, deliberately — Command and Control are different physical
// keys on a Mac, and Word and ONLYOFFICE both bind ⌘ there — so a hardcoded
// modifier now names a keystroke that fires on ONE platform and silently does
// nothing on the other:
//
//   * `"Control+z"`          — dead on macOS. Three specs had it, and they are
//                              what reddened `main`: the undo never ran, the
//                              line numbers stayed painted, the shape stayed
//                              red, and the palette never opened.
//   * `"Meta+Shift+KeyP"`    — dead on Linux/Windows. Six more sites had it, in
//                              `object-command-reach` and `object-edit-keeps-view`,
//                              which is the same defect pointed at CI instead of
//                              at the developer's machine.
//
// Both halves are invisible to a single-platform run, which is why neither the
// Linux CI suite nor a macOS sweep could catch the pair. A source-level guard can,
// and this is it. SKILL §10: fix the class and fail the build if it returns.
//
// The fixture exports `MOD` (Meta on darwin, Control elsewhere) and `WORD_MOD`
// for the word-navigation modifier. Use those.
import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const E2E = join(HERE, "e2e");

/** A `keyboard.press`/`down`/`up` argument that is a STRING LITERAL naming a
 *  platform modifier. The modifier must be a whole token — `Alt+ArrowLeft` is a
 *  hit, and a key named `"Control"` on its own (a bare modifier keydown, which is
 *  platform-neutral) is not. */
const HARDCODED_CHORD =
  /keyboard\.(?:press|down|up)\(\s*(["'])((?:[^"']*\+)?(?:Meta|Control|Alt)\+[^"']*)\1/g;

/** Spec files allowed to name a modifier outright, and the exact chords they may
 *  name. A chord belongs here only when it is genuinely ONE platform's chord
 *  rather than the platform's spelling of a shared one — and the file must gate
 *  itself on the platform, which the second assertion below enforces, so an entry
 *  here cannot smuggle in a chord that simply never runs. */
const PLATFORM_ONLY = new Map([
  [
    "keyboard-clipboard-parity.spec.mjs",
    {
      chords: new Set(["Meta+Backspace"]),
      reason:
        "⌘Backspace deletes to line start on macOS only — `lineDeletionDirection` " +
        "returns null on the standard keymap, so there is no chord to spell there",
    },
  ],
]);

function specFiles() {
  return readdirSync(E2E)
    .filter((name) => name.endsWith(".spec.mjs"))
    .sort();
}

/** Every hardcoded chord in `name`, as `{ chord, line }`. */
function hardcodedChords(name) {
  const text = readFileSync(join(E2E, name), "utf8");
  const found = [];
  for (const match of text.matchAll(HARDCODED_CHORD)) {
    const line = text.slice(0, match.index).split("\n").length;
    found.push({ chord: match[2], line });
  }
  return found;
}

test("no e2e spec hardcodes a platform modifier for a keymap chord", () => {
  const offenders = [];
  for (const name of specFiles()) {
    const allowed = PLATFORM_ONLY.get(name)?.chords ?? new Set();
    for (const { chord, line } of hardcodedChords(name)) {
      if (allowed.has(chord)) continue;
      offenders.push(`${name}:${line}  "${chord}"`);
    }
  }
  assert.deepEqual(
    offenders,
    [],
    "these chords fire on one platform and silently do nothing on the other — " +
      "use `MOD` / `WORD_MOD` from fixtures.mjs, which resolve to the modifier " +
      "`matchesShortcut` expects on the host running the suite:\n  " +
      offenders.join("\n  "),
  );
});

test("a spec allowed to name a modifier gates itself on the platform", () => {
  const ungated = [];
  for (const [name, { reason }] of PLATFORM_ONLY) {
    const text = readFileSync(join(E2E, name), "utf8");
    if (!/test\.skip\(\s*process\.platform/.test(text)) {
      ungated.push(`${name} — allowed for: ${reason}`);
    }
  }
  assert.deepEqual(
    ungated,
    [],
    "a platform-only chord must be skipped on the platforms that do not have it, " +
      "or the allowlist entry just hides a keystroke that reaches nothing:\n  " +
      ungated.join("\n  "),
  );
});

test("the allowlist names real files and real chords", () => {
  // An entry that no longer matches anything is a stale exemption, and a stale
  // exemption is how a ban quietly stops covering the thing it was written for.
  const stale = [];
  const names = new Set(specFiles());
  for (const [name, { chords }] of PLATFORM_ONLY) {
    if (!names.has(name)) {
      stale.push(`${name} is allowlisted but is not a spec file`);
      continue;
    }
    const present = new Set(hardcodedChords(name).map(({ chord }) => chord));
    for (const chord of chords) {
      if (!present.has(chord)) stale.push(`${name} no longer presses "${chord}"`);
    }
  }
  assert.deepEqual(stale, [], stale.join("\n  "));
});
