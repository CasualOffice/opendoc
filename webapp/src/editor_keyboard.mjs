// The editor's keydown dispatch: object grammar, crop, navigation, editing keys
// and chords, in the order they claim a key. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { handleCropKey, objectNudge } from "./object_keys.mjs";
import { capabilityRefusal } from "./object_refusal.mjs";
import { lineDeletionDirection, navigationDirection, wordDeletionDirection } from "./keyboard.mjs";
import { t } from "./i18n.mjs";
import { orderedSelectionEnds } from "./caret_navigation.mjs";
import { smartQuoteForTyped } from "./text_rules.mjs";
import { typeArmedCharacter } from "./font_advanced.mjs";
import { cellContentsSelection } from "./table_tab.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createKeyboard(ed) {
  document.addEventListener("keydown", async (e) => {
    if (!ed.doc) return;
    // The canvas editor owns keystrokes only while its focus owner is active.
    // Chrome controls, popovers, and link chips keep normal browser semantics.
    if (ed.isInteractiveChromeTarget(e.target) || !ed.eventTargetsEditor(e)) return;

    const mod = e.metaKey || e.ctrlKey;
    const key = e.key;

    if (ed.composingText || e.isComposing || key === "Process") return;

    // The object interaction grammar (docs/85 §4) owns the keyboard while an
    // object is selected. Escape is the two-step exit (editing → selected → text);
    // Enter/Delete act on the object; a selected object swallows text keys so a
    // stale caret is never edited.
    if (key === "Escape" && ed.objectTextDrag.cancel()) return void e.preventDefault();
    if (ed.keyboardObjectMove.onKey(e)) return; // F2 / Shift+F2, "Move to where?" (UX-OB-02)
    if ((ed.objectResize.active() || ed.objectRotate.active() || ed.objectMoveDrag) && key === "Escape") {
      e.preventDefault();
      ed.objectResize.cancel();
    ed.objectRotate.cancel(); // Escape during a drag cancels it (docs/85 §4.2)
      ed.cancelObjectMove();
      return;
    }
    // Crop mode owns Enter, Escape and the arrows before the object grammar —
    // an arrow let through moved the PICTURE under the crop (HF-106).
    if (ed.objectCropSession && handleCropKey(e, ed.cropKeyIo)) return;
    // Esc leaves header/footer editing first: while that context is open it is the
    // thing Esc most obviously means, and Word closes the header on Esc too.
    if (ed.runningEditBand && key === "Escape") {
      e.preventDefault();
      ed.exitRunningEdit();
      return;
    }
    if (ed.objectSelection) {
      if (key === "Escape") {
        e.preventDefault();
        if (ed.objectSelection.mode === "editing") {
          ed.objectSelection = { ...ed.objectSelection, mode: "selected" };
        } else if (!ed.climbOutOfGroup()) {
          ed.objectSelection = null; // collapse to the surrounding-text caret
        }
        ed.clearObjectStatus();
        ed.drawSelection();
        return;
      }
      if (ed.objectSelection.mode === "selected") {
        if (key === "Enter") {
          e.preventDefault();
          ed.enterObjectEditMode(); // double-click's keyboard twin (§4.3)
          return;
        }
        // Tab / Shift+Tab move to the next / previous object (docs/85 §8d, Q1).
        // Word cycles floating objects with Tab once one is selected, and Word for
        // the web moves between graphics the same way — so the gesture belongs to
        // object-selected mode. From a text caret Tab keeps its indent /
        // list-demote / next-cell meaning, which the handler further down still
        // owns. This is the only way to reach an object without a pointer: nothing
        // else selects one, so a keyboard user could not reach an image or text
        // box at all.
        if (key === "Tab" && !mod) {
          e.preventDefault();
          ed.traverseObjects(e.shiftKey ? -1 : 1);
          return;
        }
        if (key === "Delete" || key === "Backspace") {
          e.preventDefault();
          if (ed.objectSelection.canDelete) ed.deleteSelectedObject(); // one undoable delete; gated
          else ed.setStatus(capabilityRefusal(ed.objectSelection, "canDelete", ed.SESSION.sentenceFor), "error");
          return;
        }
        // Arrow keys nudge a FLOATING object's position (Word/Docs): Shift a larger
        // step, Ctrl/Option Word's one-pixel step. An inline image has no position,
        // so its arrows still fall through to move the caret off it.
        const nudge = objectNudge(e);
        if (nudge && ed.objectSelection.canMove) {
          e.preventDefault();
          ed.nudgeSelectedObject(nudge);
          return;
        }
        // A text key on an INLINE object (an inserted picture is selected, HF-303)
        // drops the object and types at the caret after it — never a silent drop,
        // and never Word's replace-the-picture. A floating object still keeps them.
        if ([...key].length === 1 && !mod) {
          if (ed.objectSelection.anchored) return void e.preventDefault();
          ed.objectSelection = null; // HF-315; code points, so an emoji counts as one key
          ed.drawSelection();
        }
      }
    }

    // Space ticks the form checkbox the caret is in, as in Word — the keyboard
    // half of the click in `onPointerDown` (`docs/118-DOCUMENT-AUDIT-2026-09-22.md` §2). Collapsed caret and
    // no modifier only: Ctrl+Space is Clear Formatting (a keymap row now, claimed
    // by the dispatcher), and a Space over a selection is a replacement, not a
    // toggle.
    if (key === " " && !e.ctrlKey && !e.metaKey && !e.altKey && !ed.hasRange() && ed.selection) {
      if (ed.toggleFormCheckboxAt(ed.selection.focus.node, ed.selection.focus.offset)) {
        e.preventDefault();
        return;
      }
    }

    // Every ⌘/Ctrl chord this handler used to carry — clipboard, select-all,
    // history, B/I/U, the format painter, clear-formatting — is now a row in
    // `keymap.mjs` claimed by the one dispatcher, which runs before this listener
    // and stops the event. What stays here is the keyboard that is NOT a chord:
    // caret movement, Enter, Tab, Backspace, the object grammar and text input.
    if (!ed.selection) return;

    // Navigation uses an explicit macOS/Windows keymap. It runs before the
    // generic modifier guard so the supported Ctrl/Command/Option combinations
    // reach semantic engine moves. Shift extends every navigation intent.
    const navDir = navigationDirection(e, ed.EDITOR_KEYBOARD_PLATFORM);
    // Shift+Arrow EXTENDS a cell selection once one exists — the Docs and Word
    // behaviour — instead of collapsing it into a text range one character long.
    // With no cell selection this is not reached and Shift+Arrow is text as ever.
    if (navDir && e.shiftKey && ed.tableRange.get() && ed.tableRange.extendByStep(navDir)) {
      e.preventDefault();
      ed.syncSelectionToCellRange();
      ed.tableRange.announce();
      return;
    }
    if (navDir === "docStart" || navDir === "docEnd") {
      e.preventDefault();
      ed.navToPosition(navDir === "docStart" ? ed.doc.firstPosition() : ed.doc.lastPosition(), e.shiftKey);
      return;
    }
    if (navDir === "pageUp" || navDir === "pageDown") {
      e.preventDefault();
      ed.navByViewport(navDir, e.shiftKey);
      return;
    }
    if (navDir) {
      e.preventDefault();
      ed.navCaret(navDir, e.shiftKey);
      return;
    }

    // Tab / Shift+Tab indent / outdent the paragraph(s) the selection touches — the
    // word-processor convention (and how lists are demoted/promoted). Caught before
    // `if (mod) return` is irrelevant (Tab carries no ⌘), but before the browser can
    // move focus off the page.
    if (key === "Tab") {
      e.preventDefault();
      ed.pendingFormat = null;
      if (ed.doc.inTable(ed.selection.focus.node)) {
        const from = ed.selection.focus.node;
        let target = null;
        try {
          // ONLY the engine call is guarded, so a throw from `navToPosition` can
          // never be mistaken for "we are at the boundary" and append a row.
          target = ed.doc.moveTableCell(from, !e.shiftKey);
        } catch {
          // THE TABLE BOUNDARY (`docs/141` TBL-01). `moveTableCell` throws only
          // "no adjacent table cell" here — `inTable` has already answered the
          // other throw — so a forward Tab that fails is a Tab in the LAST cell,
          // and Word and Google Docs both append a row for it. The engine's own
          // caret lands in the new row's first cell (`insert_row`'s
          // `apply_action_caret`), which is exactly where the gesture means to go,
          // so nothing here moves the caret a second time.
          //
          // It used to be an empty `catch {}`, refusing with nothing at all.
          if (e.shiftKey) ed.setStatus(t("table.atFirstCell"));
          else if (await ed.runEdit(() => ed.doc.insertRow(from, true), { gate: true })) ed.setStatus(t("table.rowAppended"));
        }
        // Tab SELECTS the destination cell's contents, so typing replaces them (OL-03, docs/171).
        const cellContents = target && cellContentsSelection(ed.doc, target);
        if (cellContents) {
          ed.tableRange.clear();
          ed.selection = cellContents;
          ed.drawSelection();
          ed.focusEditorSurface();
          ed.scrollCaretIntoView();
        } else if (target) ed.navToPosition(target, false);
        return;
      }
      if (ed.reviewMode === "suggesting") {
        ed.setStatus("Indent and list structure changes cannot be tracked yet; switch to Editing", "error");
        return;
      }
      const listKind = ed.doc.listStyleAt(ed.selection.focus.node);
      if (listKind) {
        await ed.runEdit(() =>
          ed.doc.adjustListLevel(
            ed.selection.anchor.node,
            ed.selection.anchor.offset,
            ed.selection.focus.node,
            ed.selection.focus.offset,
            e.shiftKey ? -1 : 1,
          ),
        );
      } else {
        await ed.runToolbarEdit((a, b, c, d) => ed.doc.adjustIndent(a, b, c, d, e.shiftKey ? -360 : 360), { paragraphLevel: true });
      }
      return;
    }

    const { anchor, focus } = ed.selection;
    const range = ed.hasRange();
    const wordDelete = wordDeletionDirection(e, ed.EDITOR_KEYBOARD_PLATFORM);

    if (wordDelete) {
      e.preventDefault();
      if (ed.reviewMode === "suggesting") {
        const start = range
          ? (anchor.offset <= focus.offset ? anchor : focus)
          : wordDelete === "backward"
            ? (() => { const c = ed.doc.moveCaret(focus.node, focus.offset, "wordLeft"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })()
            : focus;
        const end = range
          ? (anchor.offset <= focus.offset ? focus : anchor)
          : wordDelete === "forward"
            ? (() => { const c = ed.doc.moveCaret(focus.node, focus.offset, "wordRight"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })()
            : focus;
        if (start.node === end.node && start.offset < end.offset) {
          await ed.runEdit(() => ed.doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
        } else if (start.node !== end.node) {
          await ed.suggestAcrossParagraphs(start, end, null);
        }
        return;
      }
      await ed.runEdit(() =>
        range
          ? ed.doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset)
          : wordDelete === "backward"
            ? ed.doc.deleteWordBackward(focus.node, focus.offset)
            : ed.doc.deleteWordForward(focus.node, focus.offset),
      );
      return;
    }

    // macOS ⌘Backspace deletes from the caret to the start of the line (⌘Delete to
    // the line end). The engine's `lineStart`/`lineEnd` caret move gives the same
    // boundary Home/End navigation uses, and the span is removed as one undoable
    // range delete. This must run before the `if (mod)` guard, which otherwise
    // swallows the ⌘ chord.
    const lineDelete = lineDeletionDirection(e, ed.EDITOR_KEYBOARD_PLATFORM);
    if (lineDelete) {
      e.preventDefault();
      const boundary = range
        ? null
        : (() => {
            const c = ed.doc.moveCaret(focus.node, focus.offset, lineDelete === "backward" ? "lineStart" : "lineEnd");
            const p = { node: c.node, offset: c.offset };
            c.free();
            return p;
          })();
      const start = range
        ? (anchor.offset <= focus.offset ? anchor : focus)
        : lineDelete === "backward" ? boundary : focus;
      const end = range
        ? (anchor.offset <= focus.offset ? focus : anchor)
        : lineDelete === "backward" ? focus : boundary;
      if (ed.reviewMode === "suggesting") {
        if (start.node === end.node && start.offset < end.offset) {
          await ed.runEdit(() => ed.doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
        } else if (start.node !== end.node) {
          // A no-op (the caret is already at the line boundary) is swallowed; a real
          // cross-paragraph span is now a tracked suggestion.
          await ed.suggestAcrossParagraphs(start, end, null);
        }
        return;
      }
      if (!(start.node === end.node && start.offset === end.offset)) {
        await ed.runEdit(() => ed.doc.deleteSelection(start.node, start.offset, end.node, end.offset));
      }
      return;
    }

    if (mod) {
      ed.breakTypingSession();
      return; // leave other ⌘/Ctrl shortcuts to the browser
    }

    if (key === "Backspace") {
      e.preventDefault();
      if (!range && focus.offset === 0) {
        const listKind = ed.doc.listStyleAt(focus.node);
        if (listKind) {
          const level = ed.doc.listLevelAt?.(focus.node) ?? 0;
          await ed.runToolbarEdit((a, b, c, d) =>
            level > 0
              ? ed.doc.adjustListLevel(a, b, c, d, -1)
              : ed.doc.toggleList(a, b, c, d, listKind), { paragraphLevel: true });
          return;
        }
      }
      if (ed.reviewMode === "suggesting") {
        const start = range ? (anchor.offset <= focus.offset ? anchor : focus) : (() => { const c = ed.doc.moveCaret(focus.node, focus.offset, "left"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })();
        const end = range ? (anchor.offset <= focus.offset ? focus : anchor) : focus;
        if (start.node === end.node && start.offset < end.offset) {
          await ed.runEdit(() => ed.doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
          return;
        }
        // Backspace at a paragraph start suggests deleting the PREVIOUS paragraph's
        // mark: that mark is the break being removed.
        if (!range && start.node !== end.node) {
          try {
            await ed.runEdit(() => ed.doc.suggestDeleteParagraphMark(start.node, undefined, new Date().toISOString()));
          } catch (error) {
            ed.setStatus(String(error?.message ?? error), "error");
          }
          return;
        }
        if (start.node !== end.node) await ed.suggestAcrossParagraphs(start, end, null);
        return;
      }
      await ed.runEdit(() => range ? ed.doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset) : ed.doc.deleteBackward(focus.node, focus.offset));
      return;
    }
    if (key === "Delete") {
      e.preventDefault();
      if (ed.reviewMode === "suggesting") {
        const start = range ? (anchor.offset <= focus.offset ? anchor : focus) : focus;
        const end = range ? (anchor.offset <= focus.offset ? focus : anchor) : (() => { const c = ed.doc.moveCaret(focus.node, focus.offset, "right"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })();
        if (start.node === end.node && start.offset < end.offset) {
          await ed.runEdit(() => ed.doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
          return;
        }
        // Delete at a paragraph end suggests deleting THIS paragraph's mark.
        if (!range && start.node !== end.node) {
          try {
            await ed.runEdit(() => ed.doc.suggestDeleteParagraphMark(start.node, undefined, new Date().toISOString()));
          } catch (error) {
            ed.setStatus(String(error?.message ?? error), "error");
          }
          return;
        }
        if (start.node !== end.node) await ed.suggestAcrossParagraphs(start, end, null);
        return;
      }
      await ed.runEdit(() => range ? ed.doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset) : ed.doc.deleteForward(focus.node, focus.offset));
      return;
    }
    if (key === "Enter") {
      e.preventDefault();
      // Shift+Enter is a SOFT line break: the text stays in one paragraph and so
      // keeps its list membership, style, numbering and spacing. It used to fall
      // through to the paragraph split below, which in a bulleted list produced a
      // second bullet and at the end of a heading dropped the next line into body
      // text — the opposite of what the gesture means in Word and Docs.
      //
      // Checked before the Suggesting gate and before the empty-list-item rule,
      // because neither is about this gesture: a line break inside an empty list
      // item must not exit the list.
      if (e.shiftKey && !mod) {
        await ed.insertLineBreakAtSelection();
        return;
      }
      if (ed.reviewMode === "suggesting") {
        // A tracked Enter. Over a selection the range goes first, as its own
        // suggestion, then the break — the order Word writes them in.
        if (range) {
          const [s, e] = orderedSelectionEnds(ed.selection, ed.doc);
          const ok = s.node === e.node
            ? await ed.runEdit(() => ed.doc.suggestDelete(s.node, s.offset, e.offset, undefined, new Date().toISOString())).then(() => true)
            : await ed.suggestAcrossParagraphs(s, e, null);
          if (!ok) return;
        }
        const at = ed.selection?.focus ?? focus;
        try {
          await ed.runEdit(() => ed.doc.suggestSplit(at.node, at.offset, undefined, new Date().toISOString()));
        } catch (error) {
          ed.setStatus(String(error?.message ?? error), "error");
        }
        return;
      }
      // Word/Docs convention: Enter on an empty list item exits the list instead
      // of creating another empty bullet/number. The current paragraph remains in
      // place, so the caret does not jump and Undo restores the list marker.
      if (!range) {
        const listKind = ed.doc.listStyleAt(focus.node);
        if (listKind && ed.doc.paragraphLength(focus.node) === 0) {
          const level = ed.doc.listLevelAt?.(focus.node) ?? 0;
          await ed.runToolbarEdit((a, b, c, d) =>
            level > 0
              ? ed.doc.adjustListLevel(a, b, c, d, -1)
              : ed.doc.toggleList(a, b, c, d, listKind), { paragraphLevel: true });
          return;
        }
      }
      const landed = await ed.runEdit(() =>
        ed.doc.insertPlainTextAs(
          anchor.node,
          anchor.offset,
          focus.node,
          focus.offset,
          "\n",
          "paragraphBreak",
        ),
      );
      // The break finished a word: AutoCorrect it on the paragraph it stayed in
      // (a split keeps the original node for the text before the caret), as its
      // own undo step after the break, which is Word's order (RM-19).
      if (landed && !range) await ed.autoCorrect.afterBreak(focus.node, focus.offset);
      return;
    }
    // A printable character (single key, no modifiers). Counted in CODE POINTS,
    // not UTF-16 units: an emoji arrives as a surrogate pair, so `key.length === 1`
    // silently discarded every astral character the user typed or an IME committed
    // — while the same character pasted or inserted from a picker went in fine.
    // Named keys ("Enter", "F1", "Tab") are still longer than one code point.
    if ([...key].length === 1) {
      e.preventDefault();
      const session = ed.typingSessionForKey();
      // Straight quotes become typographic ones, decided from what precedes the
      // insertion point — the start of the replaced range when there is a
      // selection, since that is what the quote will actually follow.
      const quoteNode = range ? (anchor.offset <= focus.offset ? anchor.node : focus.node) : focus.node;
      const quoteOffset = range ? Math.min(anchor.offset, focus.offset) : focus.offset;
      const typed = smartQuoteForTyped(key, {
        enabled: ed.autoCorrect.enabled("smartQuotes"),
        offset: quoteOffset,
        readPrefix: () => ed.doc.copyText(quoteNode, 0, quoteNode, quoteOffset),
      });
      if (range) {
        ed.pendingFormat = null; // typing over a selection uses the selection's own runs
        if (ed.reviewMode === "suggesting" && anchor.node !== focus.node) {
          const [s, e] = orderedSelectionEnds(ed.selection, ed.doc);
          await ed.suggestAcrossParagraphs(s, e, typed);
          return;
        }
        await ed.runEdit(
          () => ed.reviewMode === "suggesting"
            ? ed.doc.suggestReplace(
              anchor.node,
              Math.min(anchor.offset, focus.offset),
              Math.max(anchor.offset, focus.offset),
              typed,
              undefined,
              new Date().toISOString(),
              session,
            )
            : ed.doc.typeText(anchor.node, anchor.offset, focus.node, focus.offset, typed, session),
          { typing: true },
        );
      } else if (ed.pendingFormat) {
        const pf = ed.pendingFormat; // armed format persists across consecutive typing
        // An armed underline style or colour types through the character path,
        // which records both as a tracked insert in Suggesting mode (HF-255) —
        // the plain styled insert below has no field for either.
        const armedUnderline = pf.underlineStyle != null || pf.underlineColor != null;
        await ed.runEdit(
          () => pf.character || (armedUnderline && ed.reviewMode === "suggesting") ? typeArmedCharacter(ed.doc, focus, typed, pf, session, ed.reviewMode) : ed.reviewMode === "suggesting"
            ? ed.doc.suggestStyledInsert(
              focus.node,
              focus.offset,
              typed,
              pf.bold,
              pf.italic,
              pf.underline,
              pf.strike,
              pf.sizeHalfPoints,
              pf.color,
              pf.highlight,
              pf.vertAlign,
              pf.font,
              undefined,
              new Date().toISOString(),
              session,
            )
            : (pf.underlineStyle != null || pf.underlineColor != null)
              ? ed.doc.typeStyledTextWithUnderline(
                  focus.node,
                  focus.offset,
                  typed,
                  pf.bold,
                  pf.italic,
                  pf.underline,
                  pf.strike,
                  pf.sizeHalfPoints,
                  pf.color,
                  pf.highlight,
                  pf.vertAlign,
                  pf.font,
                  session,
                  pf.underlineStyle,
                  pf.underlineColor,
                )
              : ed.doc.typeStyledText(
                  focus.node,
                  focus.offset,
                  typed,
                  pf.bold,
                  pf.italic,
                  pf.underline,
                  pf.strike,
                  pf.sizeHalfPoints,
                  pf.color,
                  pf.highlight,
                  pf.vertAlign,
                  pf.font,
                  session,
                ),
          { typing: true },
        );
      } else {
        await ed.runEdit(
          () => ed.reviewMode === "suggesting"
            ? ed.doc.suggestInsert(
              focus.node,
              focus.offset,
              typed,
              undefined,
              new Date().toISOString(),
              session,
            )
            : ed.doc.typeText(focus.node, focus.offset, focus.node, focus.offset, typed, session),
          { typing: true },
        );
      }
      if (!range) await ed.autoCorrect.afterTyped(typed); // a list marker or a correction, as its own undo step (TA-05/06)
    }
  });

  /** The viewer's package-admission limit, mirroring `max_input_bytes` in
   *  `casual-doc-wasm` (64 MiB). Mirrored rather than imported because the engine
   *  applies it inside `open(bytes)`, i.e. only once the whole file has already
   *  been read; this is the pre-check that lets the editor say no first. */
}
