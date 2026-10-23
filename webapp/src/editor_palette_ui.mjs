// The command palette's surface: the filtered list, the selection in it,
// running a command by row or by id, and the keyboard that opens it. The
// registry it lists is `editor_palette.mjs`. Moved out of `main.js`
// unchanged (docs/177); it reaches the rest of the editor only through `ed`.
import { escapeHtml } from "./text_rules.mjs";
import { formatShortcut } from "./keyboard.mjs";
import { chordCommand } from "./keymap.mjs";
import { registerModal } from "./modal.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createPaletteUi(ed) {
  let cmdMatches = [];
  let cmdSel = 0;

  function renderCommands(query) {
    const q = query.trim().toLowerCase();
    const all = ed.buildCommands();
    cmdMatches = q
      ? all.filter((c) => `${c.label} ${c.group} ${c.kw} ${c.shortcut ?? ""}`.toLowerCase().includes(q))
      : all;
    cmdSel = cmdMatches.findIndex((command) => command.enabled !== false);
    ed.cmdList.replaceChildren();
    if (!cmdMatches.length) {
      const empty = document.createElement("div");
      empty.className = "cmd-empty";
      empty.textContent = "No matching commands";
      ed.cmdList.appendChild(empty);
      ed.cmdInput.setAttribute("aria-expanded", "false");
      ed.cmdInput.removeAttribute("aria-activedescendant");
      return;
    }
    ed.cmdInput.setAttribute("aria-expanded", "true");
    cmdMatches.forEach((c, i) => {
      const item = document.createElement("button");
      item.type = "button";
      item.className = `cmd-item${i === cmdSel ? " sel" : ""}`;
      item.setAttribute("role", "option");
      // A stable id per option is what `aria-activedescendant` points at; the
      // option itself must stay out of the tab order because focus never leaves
      // the query input.
      item.id = `cmdOption-${i}`;
      // The command id, on the row, for the same reason the context menu carries
      // it (`data-command-id`) and the ribbon buttons carry `data-command`: it
      // makes "is this capability on more than one surface?" a question the DOM can
      // answer, so a parity guard compares id sets instead of matching labels that
      // are context sensitive and carry their shortcut in the same box.
      item.dataset.commandId = c.id;
      item.tabIndex = -1;
      item.setAttribute("aria-selected", String(i === cmdSel));
      item.disabled = c.enabled === false;
      if (c.enabled === false && c.disabledReason) item.title = c.disabledReason;
      // The hint column shows the disabled reason when unavailable, else the
      // command's keyboard shortcut when it has one (so the palette teaches the
      // shortcut), else its group.
      const hint = c.enabled === false ? c.disabledReason : (formatShortcut(c.shortcut) || c.group);
      // The shortcut, separately from the hint TEXT. The hint box holds a chord, a
      // group name or a refusal reason depending on state, so a test reading it
      // cannot tell "has a chord" from "has a group" — and a reachability guard
      // that counts a group name as a keyboard surface passes for every command
      // there is, which is the green-but-worthless guard this repo keeps catching.
      if (c.shortcut) item.dataset.commandShortcut = formatShortcut(c.shortcut);
      item.innerHTML = `<span>${escapeHtml(c.label)}</span><span class="cmd-hint">${escapeHtml(hint)}</span>`;
      item.addEventListener("mousemove", () => setCmdSel(i));
      item.addEventListener("click", () => runCommand(i));
      ed.cmdList.appendChild(item);
    });
    reflectCmdSel();
  }

  function setCmdSel(i) {
    if (i < 0 || cmdMatches[i]?.enabled === false) return;
    cmdSel = i;
    reflectCmdSel();
  }

  /** Publishes the current selection to both channels: the `.sel` class the eye
   *  reads, and the `aria-selected` / `aria-activedescendant` pair a screen reader
   *  reads. Arrowing used to move only the class, so the palette said nothing
   *  while the highlight travelled and Enter ran whatever happened to be under
   *  it. The option's own text carries the hint column, so a disabled command
   *  announces its reason with its name. */
  function reflectCmdSel() {
    const items = ed.cmdList.querySelectorAll(".cmd-item");
    items.forEach((el, k) => {
      el.classList.toggle("sel", k === cmdSel);
      el.setAttribute("aria-selected", String(k === cmdSel));
    });
    const active = items[cmdSel];
    if (active) {
      ed.cmdInput.setAttribute("aria-activedescendant", active.id);
      active.scrollIntoView({ block: "nearest" });
    } else {
      ed.cmdInput.removeAttribute("aria-activedescendant");
    }
  }

  function moveCmdSelection(direction) {
    if (!cmdMatches.some((command) => command.enabled !== false)) return;
    let index = cmdSel;
    for (let count = 0; count < cmdMatches.length; count++) {
      index = (index + direction + cmdMatches.length) % cmdMatches.length;
      if (cmdMatches[index].enabled !== false) {
        setCmdSel(index);
        return;
      }
    }
  }

  function runCommand(i) {
    const cmd = cmdMatches[i];
    if (!cmd || cmd.enabled === false) return;
    closeCmd();
    // From the File page's PANE there is no modal for `closeCmd` to close, so
    // the command ran with the page still over the document. No-op when closed.
    ed.closeFilePage();
    cmd.run();
  }

  // The palette is modal too — it takes the keyboard and dims the app behind it —
  // so it joins the same stack rather than keeping a fourth dismissal contract.
  // It brings its own Arrow/Enter handling on #cmdInput; the primitive only owns
  // Escape, Tab containment and the shortcut lock.
  const cmdModal = registerModal(ed.cmdPalette, {
    initialFocus: () => ed.cmdInput,
    fallbackFocus: () => ed.pagesEl,
    // A shortcut that opens a surface must also close it; the lock would
    // otherwise swallow the second ⌘⇧P and strand the palette open.
    toggleChord: (event) => (event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "p",
    onClose: () => ed.searchTrigger?.setAttribute("aria-expanded", "false"),
  });

  function openCmd() {
    ed.closeAppMenu();
    ed.cmdInput.value = "";
    renderCommands("");
    ed.searchTrigger?.setAttribute("aria-expanded", "true");
    cmdModal.open();
  }
  function closeCmd() {
    cmdModal.close();
  }

  ed.cmdInput.addEventListener("input", () => renderCommands(ed.cmdInput.value));
  ed.cmdInput.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveCmdSelection(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      moveCmdSelection(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      runCommand(cmdSel);
    }
  });
  /** Runs the command `id` names, or says why it cannot run.
   *
   *  A chord that exists and does nothing is a dead control (SKILL.md §10), so a
   *  shortcut whose command is unavailable refuses OUT LOUD with the same reason
   *  the ribbon button and the menu row give — and since `109` UX-017 that reason
   *  is perceivable at phone widths and to a screen reader too. Returns false only
   *  when no such command exists in the current state, which is how a chord whose
   *  command needs a document (⌘S) still falls through to the browser when nothing
   *  is open, exactly as it did before.
   *
   *  Cost: one `editorCommands()` build per chord press. That is the same build the
   *  command palette already performs on EVERY keystroke typed into its input
   *  (`renderCommands` → `buildCommands`), so this is an accepted cost on an
   *  existing path rather than a new one — but it is not O(1): `documentHasObjects`
   *  inside the build calls `objectOrder()` and parses it, which is O(objects).
   *  That is pre-existing and worth its own row; it is not introduced here.
   */
  function runCommandById(id) {
    const command = ed.editorCommands({ surface: "palette" }).find((c) => c.id === id);
    if (!command) return false;
    if (command.enabled === false) {
      ed.setStatus(command.disabledReason ?? "That command is not available right now", "error");
      return true;
    }
    command.run();
    return true;
  }

  // THE dispatcher: the one place a keyboard chord becomes a command (`109`
  // UX-006). It replaces four separate `keydown` handlers that each carried their
  // own hand-written if-chain, which is why the editor could advertise a chord it
  // had never bound and bind one it advertised nowhere.
  //
  // No modal check here, and none is needed: `modal.mjs` already swallows
  // application chords in the capture phase while a dialog is open, with an
  // allowlist for text-editing chords and a `toggleChord` escape hatch. That is
  // ONLYOFFICE's `common/main/lib/util/Shortcuts.js:96-107` suspend/resume, with
  // the two refinements their version lacks.
  //
  // `stopImmediatePropagation` once a chord is claimed, because a chord the table
  // owns must not also be read by anything else — that is what made ⌘⌥⏎ need it by
  // hand, or it would both decide a tracked change and insert a paragraph. Every
  // other document-level `keydown` listener in this file is Escape-only or
  // popover-only, so nothing legitimate is cut off.
  document.addEventListener("keydown", (e) => {
    const id = chordCommand(e, ed.EDITOR_KEYBOARD_PLATFORM, {
      inEditor: !ed.isInteractiveChromeTarget(e.target) && ed.eventTargetsEditor(e),
    });
    if (!id) return;
    // The palette is the one command that toggles: its own chord has to close it
    // as well as open it.
    if (id === "help.commands" && cmdModal.isOpen) {
      e.preventDefault();
      e.stopImmediatePropagation();
      closeCmd();
      return;
    }
    if (!runCommandById(id)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
  });
  // Visible entry point for the palette (doc 69 §1.4.1): the shortcut already
  // worked, it just had no on-screen affordance to discover it.
  ed.searchTrigger?.addEventListener("click", () => openCmd());
  return { openCmd, renderCommands };
}
