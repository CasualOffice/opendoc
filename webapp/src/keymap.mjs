// The editor's whole keyboard map: ONE declaration per binding, and the same
// declaration the palette, the menus, the compact toolbar's tooltips and the
// shortcut reference all read their label from.
//
// `109` UX-006 and UX-007 are one defect with two symptoms. Before this file:
//
//   * 25 hand-written `shortcut:` labels sat on command descriptors, and the
//     bindings themselves were `if (mod && lower === "x")` branches in FOUR
//     separate `document.addEventListener("keydown")` handlers. The two tables
//     were unrelated, so the editor advertised chords it had never bound and
//     bound one (⌘⇧E, review-mode cycle) that it advertised nowhere.
//   * Every chord Word users reach for on a paragraph — alignment, indent,
//     bullets, grow/shrink — was simply absent.
//
// The established answer is not new: a **key-binding registry over a command
// table**, dispatched by id. ONLYOFFICE has exactly that, in two layers — a
// closed typed shortcut set with a default keymap
// (`sdkjs/word/Editor/Shortcuts.js`, `Asc.c_oAscDocumentShortcutType` +
// `c_oAscDefaultShortcuts`) and a declarative delegator over it
// (`common/main/lib/util/Shortcuts.js`, used at
// `documenteditor/main/app/controller/LeftMenu.js:147-158`). Two details of
// theirs are worth copying and are copied here: one keymap key carries BOTH
// platform spellings (`"command+f,ctrl+f"`), and a chord may be declared per
// platform where the platforms genuinely differ (`DocumentHolder.js:147-148`,
// `isMac ? 'command+alt+a' : 'alt+h'`).
//
// One thing of theirs is deliberately NOT copied: `Shortcuts.js:96-107` suspends
// every chord while a modal is open. This editor already has that, and better —
// `modal.mjs` swallows application chords in the capture phase but lets an
// allowlist of text-editing chords through to the dialog's own fields, and a
// surface opened by a chord stays closable by it (`toggleChord`). Their version
// suspends everything, including ⌘C inside their own dialogs. So the dispatcher
// here needs no modal check of its own: it runs after that lock.
//
// Their per-type "unlocked" flag — a chord that still works on a protected
// document — still has no meaning here, and the reason has CHANGED: this editor
// now has document protection as well as review modes (ADR-052 enforces
// `w:documentProtection` at the operation, ADR-059 installs and lifts it, and
// Review ▸ Restrict Editing reaches both). What makes a second gate unnecessary is
// that the enforcement is at the OPERATION: every chord routes to a command, every
// command that writes routes to the one choke point, and the choke point refuses
// with its own sentence. A flag here would put the reason for a refusal in two
// places and would have to be kept in step with
// `casual_doc_edit::protection::exempt_from_protection` by hand.

import { APPLE_PLATFORM, STANDARD_PLATFORM, formatShortcut, matchesShortcut } from "./keyboard.mjs";

/** Where a chord is allowed to fire.
 *
 *  `app` chords work wherever the editor has focus, including from a chrome
 *  control, because they are about the application: open the palette, save,
 *  print, find. `editor` chords only fire while the editing surface owns the
 *  keyboard, because they act on the caret — ⌘B while the font search box has
 *  focus must type a "b", not embolden the document behind it.
 */
export const APP_SCOPE = "app";
export const EDITOR_SCOPE = "editor";

/**
 * Every chord the editor binds, as `{ chord, command, scope, platform? }`.
 *
 * `chord` is Apple notation because that is what `formatShortcut` already
 * renders and what the descriptors already declared; `⌘` resolves to Command on
 * Apple and Control everywhere else, so one row covers both keyboards.
 * `platform` narrows a row to one keymap, and is only used where the platforms
 * genuinely disagree rather than as a convenience.
 *
 * `command` is a command id from `editorCommands()`. That is the whole contract,
 * and it is what stops this file from becoming a third place where behaviour
 * lives: a chord cannot do anything a menu, the palette and the shortcut
 * reference do not already offer, and `keymap.test.mjs` fails if an id here is
 * not a real command. SKILL §10 — a chord that exists and does nothing is a dead
 * control, so a row whose command does not exist is a build failure, not a
 * silent no-op.
 *
 * Sources for the chords that were missing, read from ONLYOFFICE's own default
 * keymap rather than from documentation
 * (`reference/sdkjs/word/Editor/Shortcuts.js`), and cross-checked against Word:
 *
 *   ⌘L / ⌘E / ⌘R / ⌘J   `:162, :159, :161, :160`  LeftPara/CenterPara/RightPara/JustifyPara
 *   ⌘⇧L                 `:155`                    ApplyListBullet
 *   ⌘M / ⌘⇧M            `:165-166`                Indent / UnIndent
 *   ⌘] / ⌘[             `:157-158`                Increase/DecreaseFontSize
 *   ⌘⇧X                 `:214`                    Strikeout
 *   ⌘⌥D                 `:186`                    InsertEndnoteNow
 *   ⌃Space              `:156`                     ResetChar (clear formatting)
 */
export const KEYMAP = [
  // ---- Application ---------------------------------------------------------
  // ⌘⇧P is the command palette, NOT Insert Page Number, which is what
  // ONLYOFFICE puts here (`Shortcuts.js:167`, InsertPageNumber). A deliberate
  // divergence, decided by the owner: ⌘K had to carry the Word / Google Docs /
  // Pages standard "insert or edit hyperlink", which moved the palette off ⌘K
  // onto the VS Code convention (docs/67 audit row 8). The owner settled it as
  // "fine for now", so it is a choice and not an oversight — but "for now" is
  // not "forever", and a future Insert Page Number chord must be found
  // elsewhere rather than by taking this one back silently.
  { chord: "⌘⇧P", command: "help.commands", scope: APP_SCOPE },
  { chord: "⌘S", command: "file.save", scope: APP_SCOPE },
  { chord: "⌘P", command: "file.print", scope: APP_SCOPE },
  { chord: "⌘F", command: "edit.find", scope: APP_SCOPE },
  // Find and REPLACE, with the replacement field ready. Ctrl+H is the chord in
  // Word, Google Docs and ONLYOFFICE alike, and it did nothing here. Split by
  // platform because the platforms genuinely differ: on a Mac ⌘H is the
  // operating system's Hide, which a page never receives, so the Apple row is
  // Google Docs' ⌘⇧H (Word for Mac uses ⌃H, which is a text field's
  // delete-backward on macOS and is left to it).
  { chord: "⌘H", command: "edit.replace", scope: APP_SCOPE, platform: STANDARD_PLATFORM },
  { chord: "⌘⇧H", command: "edit.replace", scope: APP_SCOPE, platform: APPLE_PLATFORM },
  // Google Docs' own chord for revision history (Ctrl+Alt+Shift+H), which is
  // ⌘⌥⇧H on an Apple keyboard. Taken from the competition rather than invented:
  // a chord nobody else uses is a chord nobody reaches for, and this one is free
  // in both keymaps. `APP_SCOPE`, because opening a timeline is an application
  // action and must work from a chrome control as well as from the canvas.
  { chord: "⌘⌥⇧H", command: "file.versionHistory", scope: APP_SCOPE },

  // ---- Editing -------------------------------------------------------------
  { chord: "⌘Z", command: "edit.undo", scope: EDITOR_SCOPE },
  { chord: "⌘⇧Z", command: "edit.redo", scope: EDITOR_SCOPE },
  { chord: "⌘Y", command: "edit.redo", scope: EDITOR_SCOPE },
  { chord: "⌘X", command: "edit.cut", scope: EDITOR_SCOPE },
  { chord: "⌘C", command: "edit.copy", scope: EDITOR_SCOPE },
  { chord: "⌘V", command: "edit.paste", scope: EDITOR_SCOPE },
  { chord: "⌘⇧V", command: "edit.pasteText", scope: EDITOR_SCOPE },
  { chord: "⌘A", command: "edit.selectAll", scope: EDITOR_SCOPE },

  // ---- Character formatting ------------------------------------------------
  { chord: "⌘B", command: "format.bold", scope: EDITOR_SCOPE },
  { chord: "⌘I", command: "format.italic", scope: EDITOR_SCOPE },
  { chord: "⌘U", command: "format.underline", scope: EDITOR_SCOPE },
  { chord: "⌘⇧X", command: "format.strike", scope: EDITOR_SCOPE },
  { chord: "⌘]", command: "format.grow", scope: EDITOR_SCOPE },
  { chord: "⌘[", command: "format.shrink", scope: EDITOR_SCOPE },
  { chord: "⌘⇧C", command: "format.painter", scope: EDITOR_SCOPE },
  // Clear direct formatting. STANDARD keyboards only, and that asymmetry is the
  // reason `platform` exists: macOS reserves Control+Space for switching input
  // source, so binding it there would break text entry for anyone typing a
  // language that needs the switcher. Word's own Mac build leaves it alone too.
  { chord: "⌃Space", command: "format.clear", scope: EDITOR_SCOPE, platform: STANDARD_PLATFORM },

  // ---- Paragraph formatting — the Word chords the editor had none of -------
  { chord: "⌘L", command: "paragraph.align.start", scope: EDITOR_SCOPE },
  { chord: "⌘E", command: "paragraph.align.center", scope: EDITOR_SCOPE },
  { chord: "⌘R", command: "paragraph.align.end", scope: EDITOR_SCOPE },
  { chord: "⌘J", command: "paragraph.align.justify", scope: EDITOR_SCOPE },
  { chord: "⌘⇧L", command: "paragraph.list.bullet", scope: EDITOR_SCOPE },
  { chord: "⌘M", command: "paragraph.indent.increase", scope: EDITOR_SCOPE },
  { chord: "⌘⇧M", command: "paragraph.indent.decrease", scope: EDITOR_SCOPE },
  // Line spacing had no chord at all, while Word has bound these three since
  // Word 97 and Docs binds the same three. The command ids are the ones the
  // Home popover's own preset rows generate, so a preset added to the markup
  // keeps its chord's meaning rather than a literal drifting away from it.
  { chord: "⌘1", command: "paragraph.spacing.100", scope: EDITOR_SCOPE },
  { chord: "⌘5", command: "paragraph.spacing.150", scope: EDITOR_SCOPE },
  { chord: "⌘2", command: "paragraph.spacing.200", scope: EDITOR_SCOPE },
  // Heading styles. Ctrl+Alt+1/2/3 is Word's (⌘⌥1/2/3 on its Mac build) and the
  // first three of Google Docs' Ctrl+Alt+1..6; Ctrl+Alt+0 is Docs' "Normal text".
  // Measured free before it was taken: no row here uses ⌘⌥ with a digit, and
  // `matchesShortcut` compares Alt exactly, so these cannot collide with the
  // ⌘1 / ⌘5 / ⌘2 line-spacing rows above. Where Ctrl+Alt is AltGr (German,
  // Polish, French and other layouts) the keystroke TYPES a character — AltGr+0
  // is "}" in German — and since matching reads `event.key` off Apple, the
  // character wins and the chord does not fire there: typing is never stolen,
  // and the palette and the Styles box remain the route on those layouts.
  { chord: "⌘⌥1", command: "paragraph.heading.1", scope: EDITOR_SCOPE },
  { chord: "⌘⌥2", command: "paragraph.heading.2", scope: EDITOR_SCOPE },
  { chord: "⌘⌥3", command: "paragraph.heading.3", scope: EDITOR_SCOPE },
  { chord: "⌘⌥0", command: "paragraph.normal", scope: EDITOR_SCOPE },

  // ---- Insert --------------------------------------------------------------
  { chord: "⌘K", command: "insert.link", scope: EDITOR_SCOPE },
  { chord: "⌘⌥D", command: "insert.endnote", scope: EDITOR_SCOPE },
  // Label only, and it cannot be otherwise: `chordCommand` ignores any keystroke
  // with no command modifier, so Shift+Enter never reaches the dispatcher. Enter
  // belongs to the text-input path, which has to decide between a paragraph, a
  // line break, a table cell and a form field. The row is here so the chord is
  // still advertised from the one table rather than from a literal.
  { chord: "⇧⏎", command: "insert.lineBreak", scope: EDITOR_SCOPE },
  // ⌘⏎ / Ctrl+Enter — THE page-break chord, in Word, in Google Docs and in
  // ONLYOFFICE (`Shortcuts.js`, `InsertPageBreak`). `104` HF-127 recorded it as
  // "inert — the chord is swallowed before the Enter branch", and that diagnosis
  // was right about the symptom and could have led to the wrong fix.
  //
  // The swallowing is real: `main.js`'s editor key handler has an
  // `if (mod) { breakTypingSession(); return; }` guard that leaves every ⌘/Ctrl
  // chord to the browser, and it sits ABOVE the `key === "Enter"` branch — so a
  // second path for Ctrl+Enter inside that handler would have to be written
  // above the guard, as ⌘Backspace's line-delete already is, and the editor
  // would then have two places that decide what a chord means. That is the shape
  // this whole file exists to remove.
  //
  // The real cause is simpler: nothing CLAIMED the chord. The dispatcher
  // (`document.addEventListener("keydown")` over `chordCommand`) is registered
  // before the editor's own handler and calls `stopImmediatePropagation` on every
  // chord it owns, so a row here is all that was missing — ⌘⌥⏎ has reached
  // `review.acceptNext` through the same path all along, which is the proof that
  // an Enter chord carrying a command modifier arrives here fine.
  { chord: "⌘⏎", command: "layout.break.page", scope: EDITOR_SCOPE },

  // ---- Table sizing from the keyboard --------------------------------------
  // The keyboard half of the table chrome layer (`docs/141` D-1 §4.1.6): every
  // pointer gesture the chrome adds has to be reachable without a pointer, or the
  // new affordances are mouse-only capability.
  //
  // ⌥⇧ + arrow was MEASURED free before it was taken: of the 36 chords this table
  // declared, exactly two used Alt at all (⌥H's platform twin ⌘⌥A, and ⌘⌥D /
  // ⌘⌥M / ⌘⌥⏎ / ⌘⌥⌫, all of which also carry ⌘). Nothing here or in
  // `navigationDirection` claims Alt+Shift+Arrow, and Word uses the same family
  // of chords for table sizing.
  //
  // Right/Left move the caret's column boundary; Down/Up grow and shrink its row.
  // The axis mapping is the physical one — a horizontal arrow moves a vertical
  // border — which is the only mapping a user does not have to learn.
  { chord: "⌥⇧→", command: "table.column.grow", scope: EDITOR_SCOPE },
  { chord: "⌥⇧←", command: "table.column.shrink", scope: EDITOR_SCOPE },
  { chord: "⌥⇧↓", command: "table.row.grow", scope: EDITOR_SCOPE },
  { chord: "⌥⇧↑", command: "table.row.shrink", scope: EDITOR_SCOPE },

  // ---- View ----------------------------------------------------------------
  { chord: "⌘=", command: "view.zoomIn", scope: APP_SCOPE },
  { chord: "⌘-", command: "view.zoomOut", scope: APP_SCOPE },
  // Show/Hide formatting marks. ⌘8 is Word for Mac's own chord for it, and
  // ONLYOFFICE registers theirs as `shortcutHints.ShowAll`
  // (`Toolbar.js:806-808`), so both references advertise one.
  //
  // NOT Word for Windows' Ctrl+Shift+8, and the reason is `event.key` rather than
  // taste: Shift+8 reports `"*"` on a US layout, so `⌘⇧8` would parse to a
  // keystroke no keyboard produces — a chord advertised on four surfaces and dead
  // on all of them, which is UX-007 exactly. `⌘` already resolves to Control off
  // Apple, so this is ⌘8 on a Mac and Ctrl+8 elsewhere, both live.
  { chord: "⌘8", command: "view.formattingMarks", scope: APP_SCOPE },
  // Move between the window's regions — tab strip, open side panes, document,
  // status bar — the way F6 / Shift+F6 does in Word and in every Windows
  // application, and the only keyboard route from the document OUT to the chrome
  // that does not mean tabbing through every control on the way. No modifier, so
  // `chordCommand` admits function keys explicitly (an F-key never types).
  { chord: "F6", command: "view.region.next", scope: APP_SCOPE },
  { chord: "⇧F6", command: "view.region.previous", scope: APP_SCOPE },

  // ---- Review --------------------------------------------------------------
  // `review.comment`, NOT `comment.add`: the latter is the annotate surface's own
  // id for the same capability and is not in the command registry, so binding it
  // here produced a dead chord AND a palette row with no hint. The source-text
  // guard missed it because the string exists in the file; the registry guard
  // that replaced it does not.
  { chord: "⌘⌥M", command: "review.comment", scope: APP_SCOPE },
  { chord: "⌘⇧E", command: "review.mode.cycle", scope: APP_SCOPE },
  { chord: "⌘⌥⏎", command: "review.acceptNext", scope: EDITOR_SCOPE },
  { chord: "⌘⌥⌫", command: "review.rejectNext", scope: EDITOR_SCOPE },
];

/** The chord to advertise for `commandId`, or `undefined`.
 *
 *  The label side of the single table. Command descriptors no longer carry a
 *  hand-written `shortcut:` — they are stamped from here, which is what makes a
 *  label that disagrees with its binding unrepresentable rather than merely
 *  discouraged. Where two chords run one command (⌘⇧Z and ⌘Y both redo) the
 *  FIRST wins the label, because a menu row shows one hint and Word shows the
 *  primary.
 */
export function shortcutForCommand(commandId, platform, keymap = KEYMAP) {
  // A row narrowed to the OTHER keymap must not be advertised here: on a Mac,
  // "Clear direct formatting" printing ⌃Space would be UX-007 in reverse — a
  // chord promised on a surface and dead on the keyboard in front of the user.
  // Driving the palette on Apple is how that was caught.
  return keymap.find(
    (row) => row.command === commandId && (!row.platform || row.platform === platform),
  )?.chord;
}

/** A control's tooltip: its name, then the chord its command is bound to here.
 *
 *  For the controls whose chord DIFFERS by platform and so cannot be written
 *  into markup: Replace is ⌘H off Apple and ⌘⇧H on it, because ⌘H on a Mac is
 *  the operating system's Hide. A title authored as "Replace (⌘F)" was a chord
 *  that never opened Replace; one authored as "(⌘H)" would advertise Hide to
 *  every Mac. Read from the same table the dispatcher matches against, so the
 *  tooltip cannot name a chord that is not bound. */
export function chordTitle(label, commandId, platform, keymap = KEYMAP) {
  const chord = shortcutForCommand(commandId, platform, keymap);
  return chord ? `${label} (${formatShortcut(chord, platform)})` : label;
}

/** The command a keystroke asks for, or `null`.
 *
 *  `inEditor` says whether the editing surface currently owns the keyboard; an
 *  `editor`-scope chord is ignored when it does not, so ⌘B in the font search
 *  box types a "b". O(keymap) — about thirty rows, on a keystroke that already
 *  carries a modifier, so it never runs on ordinary typing.
 */
export function chordCommand(event, platform, { inEditor = true } = {}, keymap = KEYMAP) {
  // Ordinary typing carries no chord modifier. Checking this first keeps the
  // per-keystroke cost of the whole mechanism at one boolean for the common case
  // (`docs/107` §4: per-interaction work is O(1) and small). A FUNCTION key is the
  // one exception: F6 is a chord with no modifier, and no F-key ever types.
  if (!event.metaKey && !event.ctrlKey && !event.altKey && !isFunctionKey(event.key)) return null;
  for (const row of keymap) {
    if (row.platform && row.platform !== platform) continue;
    if (row.scope === EDITOR_SCOPE && !inEditor) continue;
    if (matchesShortcut(row.chord, event, platform)) return row.command;
  }
  return null;
}

/** `F1`…`F24`. A string test, not a regex, because it runs on every keystroke
 *  that carries no modifier. */
function isFunctionKey(key) {
  return typeof key === "string" && key.length >= 2 && key.length <= 3 && key[0] === "F" && key[1] >= "1" && key[1] <= "9";
}
