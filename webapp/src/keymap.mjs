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
// Their per-type "unlocked" flag — a chord that still works on a
// protected document — has no meaning here yet: this editor has review modes,
// not document protection, and the modes are enforced by the commands
// themselves, which report their own refusal. Inventing a second gate would put
// the reason for a refusal in two places.

import { STANDARD_PLATFORM, matchesShortcut } from "./keyboard.mjs";

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

  // ---- Insert --------------------------------------------------------------
  { chord: "⌘K", command: "insert.link", scope: EDITOR_SCOPE },
  { chord: "⌘⌥D", command: "insert.endnote", scope: EDITOR_SCOPE },
  // Label only, and it cannot be otherwise: `chordCommand` ignores any keystroke
  // with no command modifier, so Shift+Enter never reaches the dispatcher. Enter
  // belongs to the text-input path, which has to decide between a paragraph, a
  // line break, a table cell and a form field. The row is here so the chord is
  // still advertised from the one table rather than from a literal.
  { chord: "⇧⏎", command: "insert.lineBreak", scope: EDITOR_SCOPE },

  // ---- View ----------------------------------------------------------------
  { chord: "⌘=", command: "view.zoomIn", scope: APP_SCOPE },
  { chord: "⌘-", command: "view.zoomOut", scope: APP_SCOPE },

  // ---- Review --------------------------------------------------------------
  { chord: "⌘⌥M", command: "comment.add", scope: APP_SCOPE },
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
  // (`docs/107` §4: per-interaction work is O(1) and small).
  if (!event.metaKey && !event.ctrlKey && !event.altKey) return null;
  for (const row of keymap) {
    if (row.platform && row.platform !== platform) continue;
    if (row.scope === EDITOR_SCOPE && !inEditor) continue;
    if (matchesShortcut(row.chord, event, platform)) return row.command;
  }
  return null;
}
