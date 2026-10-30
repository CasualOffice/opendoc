// The clipboard: cut, paste (plain, rich, tracked, structured HTML), Paste as
// text, composed text, and the Paste options affordance. Moved out of `main.js`
// unchanged (docs/177); it reaches the rest of the editor only through the
// context `ed`.
import { extractMarker, htmlToRuns, htmlToStructured } from "./clipboard.mjs";
import { modalIsOpen } from "./modal.mjs";
import { onButton } from "./popover_manager.mjs";
import { textToRuns } from "./paste_picture.mjs";
import { externalPasteLossMessage, pasteLossMessage, structureFamilies } from "./paste_loss.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createClipboard(ed) {
  function editorClipboardEvent(event) {
    return ed.doc && ed.selection && !ed.isInteractiveChromeTarget(event.target) && ed.eventTargetsEditor(event);
  }

  function editorTextInputEvent(event) {
    return ed.doc && ed.selection && !ed.isInteractiveChromeTarget(event.target) && ed.eventTargetsEditor(event);
  }

  /** Cut (⌘X): copy the selection to the clipboard, then delete it. */
  async function cut(event = null) {
    if (!ed.hasRange()) return;
    // Cut is a mutation (copy + delete); in read-only Viewing mode it is blocked
    // before touching the clipboard so it never partially executes as a copy.
    if (ed.blockMutationInViewing()) return;
    const copied = await ed.copySelection(event);
    if (!copied) return;
    const { anchor, focus } = ed.selection;
    if (ed.reviewMode === "suggesting" && anchor.node !== focus.node) {
      ed.setStatus("Cross-paragraph cuts cannot be tracked yet; switch to Editing to cut", "error");
      return;
    }
    await ed.runEdit(() => ed.reviewMode === "suggesting" && anchor.node === focus.node
      ? ed.doc.suggestDelete(anchor.node, Math.min(anchor.offset, focus.offset), Math.max(anchor.offset, focus.offset), undefined, new Date().toISOString())
      : ed.doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset));
  }

  async function pasteText(text, actionKind = "paste") {
    if (!ed.doc || !ed.selection) return;
    if (!text) return;
    const { anchor, focus } = ed.selection;
    // A soft keyboard or an IME typing over a content control's placeholder
    // replaces it, as a desktop keystroke does (`content_controls.mjs`).
    if (actionKind === "typing" && !ed.hasRange() && !text.includes("\n") && (await ed.contentControls.typeIntoPlaceholder(text))) return;
    const sameParagraph = anchor.node === focus.node && !text.includes("\n");
    if (ed.reviewMode === "suggesting" && sameParagraph) {
      const start = Math.min(anchor.offset, focus.offset);
      const end = Math.max(anchor.offset, focus.offset);
      await ed.runEdit(() => end > start
        ? ed.doc.suggestReplace(anchor.node, start, end, text, undefined, new Date().toISOString())
        : ed.doc.suggestInsert(anchor.node, start, text, undefined, new Date().toISOString()));
      return;
    }
    if (ed.reviewMode === "suggesting") {
      ed.setStatus("Multi-paragraph paste cannot be tracked yet; switch to Editing to paste it", "error");
      return;
    }
    const landed = await ed.runEdit(() => ed.doc.insertPlainTextAs(anchor.node, anchor.offset, focus.node, focus.offset, text, actionKind));
    // A pasted lone address becomes a link, as its own undo step (RM-19).
    if (landed && actionKind === "paste") await ed.autoCorrect.afterPaste(text);
  }

  async function commitComposedText(text) {
    if (!text) return;
    ed.pendingFormat = null;
    await pasteText(text, "typing");
  }

  /**
   * Suggesting-mode rich paste at a collapsed caret, single paragraph only
   * (REVIEW-GAP-008): inserts each clipboard run as its own tracked
   * `suggestStyledInsert`, chained under one gesture (`typingSessionForKey`)
   * so the whole paste is one review card and one Undo step — the same
   * paragraph-snapshot coalescing real adjacent keystrokes already use (docs
   * 82 §3; see `suggest_insert`'s `continuing_group` in casual-doc-wasm).
   * This is what lets a rich paste keep its bold/italic/color/etc. per run
   * instead of flattening to one plain-text tracked insertion.
   *
   * Returns whether it handled the paste. It does not: multi-paragraph
   * content (`paragraphBreak` runs — REVIEW-GAP-009's structural-tracking
   * backlog), or a paste that also needs to replace an existing selection (no
   * tracked multi-run *replacement* group exists yet — the model's
   * `RevisionGroupKind::Replacement` requires exactly one deletion plus one
   * insertion). Both remain the existing flattened-plain-text fallback in
   * `pasteRichRunsJson`. A run's `href` (hyperlink) is not carried into the
   * tracked insertion either — the same as the existing flattened fallback,
   * so this is not a regression, just an unchanged, explicit limitation.
   */
  async function pasteTrackedRichRuns(runs) {
    if (!ed.doc || !ed.selection || ed.hasRange()) return false;
    if (!Array.isArray(runs) || runs.some((run) => run.paragraphBreak)) return false;
    const insertable = runs.filter((run) => !run.paragraphBreak && run.text);
    if (!insertable.length) return false;

    ed.breakTypingSession();
    const session = ed.typingSessionForKey();
    let node = ed.selection.focus.node;
    let offset = ed.selection.focus.offset;
    for (const run of insertable) {
      const applied = await ed.runEdit(
        () => ed.doc.suggestStyledInsert(
          node,
          offset,
          run.text,
          run.bold,
          run.italic,
          run.underline,
          run.strike,
          run.sizeHalfPoints,
          run.color,
          run.highlight,
          run.vertAlign,
          run.font,
          undefined,
          new Date().toISOString(),
          session,
        ),
        { typing: true },
      );
      // A refused run has already reported itself; continuing would paste the
      // remainder at an offset the document never reached.
      if (!applied) break;
      // Never synthesize the next insertion point. The engine's offsets are UTF-8
      // bytes and `run.text.length` is UTF-16 code units, so `offset += length`
      // drifted on every curly quote, em dash or accent — the rest of the paste
      // then landed inside an earlier run, or was refused outright. The caret in
      // the EditResult `applyEditResult` just installed is the engine's own answer
      // for where that run ended, so read it back instead (docs/104 T-09).
      node = ed.selection.focus.node;
      offset = ed.selection.focus.offset;
    }
    // Close the gesture explicitly so a later, unrelated keystroke cannot
    // merge into this paste's group (mirrors the pause-based boundary real
    // typing uses; nothing else calls `typingSessionForKey` between here and
    // the next real key).
    ed.breakTypingSession();
    return true;
  }

  /** Replaces the selection with a rich-run clipboard fragment, as one
   * undoable action (`doc.pasteRichRuns` — the paste counterpart of
   * `copyRichRuns`). `runsJson` must be a JSON array in the shape
   * `copyRichRuns` produces. */
  async function pasteRichRunsJson(runsJson) {
    if (!ed.doc || !ed.selection) return;
    // Retain the rich fragment so the paste-options chip can re-apply it as
    // "Merge formatting" (emphasis kept, font/size/color dropped). Consumed and
    // cleared by `offerPasteOptions`.
    lastRichPasteJson = runsJson;
    if (ed.reviewMode === "suggesting") {
      let runs = null;
      try {
        runs = JSON.parse(runsJson);
      } catch { /* malformed payload; runs stays null */ }
      if (await pasteTrackedRichRuns(runs)) return;
      // Falls back to a flattened, single-format tracked replace/insert for
      // the cases `pasteTrackedRichRuns` intentionally does not cover yet
      // (see its doc comment): an existing selection to replace, or
      // multi-paragraph content.
      const text = Array.isArray(runs)
        ? runs.map((run) => run.paragraphBreak ? "\n" : String(run.text ?? "")).join("")
        : "";
      if (text) {
        await pasteText(text, "paste");
        return;
      }
      // No plain-text fallback could be derived (malformed clipboard payload, or
      // an entirely non-text rich fragment) — `doc.pasteRichRuns` has no tracked
      // representation, so this must fail closed rather than silently apply
      // untracked (REVIEW-GAP-004).
      ed.blockUntrackedInSuggesting();
      return;
    }
    const { anchor, focus } = ed.selection;
    await ed.runEdit(() =>
      ed.doc.pasteRichRuns(anchor.node, anchor.offset, focus.node, focus.offset, runsJson),
    );
  }

  /** Tries to paste `html` as a rich fragment: the internal round-trip marker
   * if present (an OpenDoc-to-OpenDoc copy, lossless), else a best-effort
   * sanitized parse of the DOM (an external app's paste — Word, Docs, a
   * browser selection). Returns whether anything was pasted, so the caller can
   * fall back to plain text when `html` carries no usable content. */
  async function pasteHtml(html) {
    if (!html) return false;
    const internal = extractMarker(html);
    if (internal) {
      let parsed = null;
      try {
        parsed = JSON.parse(internal);
      } catch { /* not JSON — treat as no usable internal payload below */ }
      // A structured fragment (`{ blocks, runs }`) reconstructs tables/lists in
      // Editing mode; Suggesting mode has no tracked representation for structural
      // paste (GAP-009), so it uses the flat runs. If the engine declines the
      // structured paste (a range selection, or a caret inside a table cell), fall
      // back to the flat runs too.
      if (parsed && !Array.isArray(parsed) && Array.isArray(parsed.blocks)) {
        const fragment = JSON.stringify({ blocks: parsed.blocks });
        const insert = (a, f) => ed.doc.pasteStructured(a.node, a.offset, f.node, f.offset, fragment);
        if (ed.reviewMode !== "suggesting" && (await runStructuredPaste(insert))) {
          return true;
        }
        await pasteRichRunsJson(JSON.stringify(parsed.runs ?? []));
        return true;
      }
      await pasteRichRunsJson(internal);
      return true;
    }
    const parsed = new DOMParser().parseFromString(html, "text/html");
    // A table, list or heading pastes as REAL structure (one undo step) only at a body caret outside Suggesting
    // (GAP-009); what the flat fallback flattens, and any picture (never fetched), is SAID (TA-11).
    const structured = htmlToStructured(parsed.body);
    const said = (flattened) => {
      const lost = externalPasteLossMessage({ flattened, pictures: Boolean(parsed.body.querySelector("img")) });
      if (lost) ed.setStatus(lost, "error");
      return true;
    };
    if (ed.reviewMode !== "suggesting" && structured) {
      const fragment = JSON.stringify(structured);
      const insert = (a, f) => ed.doc.pasteExternalStructured(a.node, a.offset, f.node, f.offset, fragment);
      if (await runStructuredPaste(insert)) return said([]);
    }
    const runs = htmlToRuns(parsed.body);
    if (!runs.length) return false;
    await pasteRichRunsJson(JSON.stringify(runs));
    return said(structureFamilies(structured));
  }

  /** Editing-mode paste of whole-block structure (tables and list paragraphs) at
   * the caret, as one undoable action. `insert(anchor, focus)` names the engine
   * call: `pasteStructured` for an internal OpenDoc copy, `pasteExternalStructured`
   * for foreign HTML parsed by `htmlToStructured`. Returns true when applied; false
   * when the engine declines (a range selection, or a caret that is not a top-level
   * body paragraph), so the caller falls back to the flat rich-run paste. Calls the
   * engine directly (not through `runEdit`, which swallows the decline) so the
   * fallback can see it. ONE helper rather than the two that used to sit here: they
   * differed only in which engine method they named, and two copies of a paste path
   * is where the loss report below gets wired into one and forgotten in the other. */
  async function runStructuredPaste(insert) {
    if (!ed.doc || !ed.selection) return false;
    const { anchor, focus } = ed.selection;
    ed.breakTypingSession();
    let res;
    try {
      res = insert(anchor, focus);
    } catch {
      return false;
    }
    // Five families of reference cannot be duplicated inside one document, and are
    // REPORTED because `AGENTS.md` forbids silent data loss. READ before
    // `applyEditResult`, which calls `res.free()` — reading a freed wasm object
    // throws "null pointer passed to rust" and failed all four structured-paste
    // specs on the first draft. SAID after, so the paint's own status line cannot
    // overwrite it, and as `"error"`: the paste succeeded, but "your comment anchor
    // did not come across" is must-notice, and only that kind reaches the toast and
    // the assertive region (`status_policy.mjs`).
    const loss = pasteLossMessage(res.pasteLoss);
    await ed.applyEditResult(res);
    if (loss) ed.setStatus(loss, "error");
    return true;
  }

  /** Paste (⌘V): insert clipboard content at the caret, replacing any
   *  selection. Rich HTML (internal or external) wins when present; plain text
   *  with newline-as-paragraph-split remains the fallback. */
  async function paste(event = null) {
    if (!ed.doc || !ed.selection) return;
    // Read-only Viewing mode blocks paste up front (it still calls
    // preventDefault below) so no clipboard read or insertion is attempted.
    if (ed.reviewMode === "viewing") {
      if (event?.clipboardData) event.preventDefault();
      ed.blockMutationInViewing();
      return;
    }
    if (event?.clipboardData) {
      event.preventDefault();
      // A pasted image (a clipboard file item of an image type) is inserted as a
      // picture, matching Word/Docs — before the text/HTML fallback.
      const imageItem = [...(event.clipboardData.items ?? [])].find(
        (it) => it.kind === "file" && it.type.startsWith("image/"),
      );
      if (imageItem) {
        const file = imageItem.getAsFile();
        if (file) await ed.insertImageFromBlob(file);
        return;
      }
      const html = event.clipboardData.getData("text/html");
      const plain = event.clipboardData.getData("text/plain");
      if (await pasteHtml(html)) {
        offerPasteOptions(plain);
        return;
      }
      await pasteText(plain);
      return;
    }
    try {
      let plain = "";
      try { plain = await navigator.clipboard.readText(); } catch { /* html-only or image-only clipboard */ }
      if (navigator.clipboard.read) {
        const items = await navigator.clipboard.read();
        for (const item of items) {
          if (!item.types.includes("text/html")) continue;
          const html = await (await item.getType("text/html")).text();
          if (await pasteHtml(html)) {
            offerPasteOptions(plain);
            return;
          }
        }
        // Then an image. ⌘V is intercepted as a keydown, so this async path — not
        // the native ClipboardEvent one — is what a screenshot paste actually
        // reaches, and it had no image branch at all: ⌘V over a copied image did
        // nothing and said nothing (docs/104 HF-059). HTML is tried first because
        // a fragment copied from a web page carries BOTH an image and its markup,
        // and Word and Docs paste the markup in that case; a screenshot carries
        // only the bitmap. The insertable-format decision stays where it already
        // lives, in `insertImageFromBlob`, so both paste paths refuse alike.
        for (const item of items) {
          const imageType = item.types.find((type) => type.startsWith("image/"));
          if (!imageType) continue;
          await ed.insertImageFromBlob(await item.getType(imageType));
          return;
        }
      }
      if (plain) {
        await pasteText(plain);
        return;
      }
      // Nothing on the clipboard this editor can place. Saying so is the point:
      // a command that looks live and quietly does nothing is indistinguishable
      // from a broken editor.
      ed.setStatus("There is nothing on the clipboard to paste here", "error");
    } catch (err) {
      console.warn("paste failed:", err);
      ed.setStatus("Clipboard paste was blocked by the browser", "error");
    }
  }

  /** Paste as plain text (⌘/Ctrl+Shift+V): drops all formatting, keeping only the
   *  clipboard's text through the existing `pasteText` path. */
  async function pasteAsText() {
    if (!ed.doc || !ed.selection) return;
    if (ed.reviewMode === "viewing") {
      ed.blockMutationInViewing();
      return;
    }
    try {
      const text = await navigator.clipboard.readText();
      if (text) {
        await pasteText(text);
        return;
      }
      // ⌘⇧V over an image-only clipboard has nothing to keep the text of. Say so
      // rather than returning silently (docs/104 HF-059). The status kind is
      // "error" — the two clipboard failures here used to pass "err", which is not
      // a class the stylesheet defines, so they were reported in the plain style.
      ed.setStatus("There is no text on the clipboard to paste", "error");
    } catch (err) {
      console.warn("paste text failed:", err);
      ed.setStatus("Clipboard paste was blocked by the browser", "error");
    }
  }

  // ---- Paste options affordance (Q3) ------------------------------------------
  // After a rich paste, a small chip near the caret lets the user switch that
  // paste to text-only (undo the rich insertion, re-paste as plain text).
  const pasteOptionsEl = document.getElementById("pasteOptions");
  const pasteOptionsMergeBtn = document.getElementById("pasteOptionsMerge");
  let pasteOptionsPlain = null;
  let pasteOptionsRuns = null;
  // The document revision the chip's own paste produced. Both chip actions work by
  // undoing that paste and redoing it differently, so they are only safe while the
  // paste is still the change an undo would remove. Captured when the chip is
  // offered; `pasteOptionsStillOnTop` is the fail-closed check.
  let pasteOptionsRevision = null;
  // The rich-run JSON of the most recent paste, captured by `pasteRichRunsJson`
  // and drained by `offerPasteOptions` for the "Merge formatting" option.
  let lastRichPasteJson = null;

  /** Whether the chip is currently offered. Read from `applyEditResult`, which is
   *  defined earlier in the file but only ever runs from an event or `boot()`,
   *  by which time this section has evaluated. */
  function pasteOptionsShowing() {
    return !pasteOptionsEl.hidden;
  }

  function hidePasteOptions() {
    pasteOptionsEl.hidden = true;
    pasteOptionsPlain = null;
    pasteOptionsRuns = null;
    pasteOptionsRevision = null;
  }

  /** True while the chip's paste is still the change an undo would remove. An
   *  unreadable revision counts as "moved on": the chip may not gamble with the
   *  user's previous edit on a number it could not read. */
  function pasteOptionsStillOnTop() {
    return (
      pasteOptionsRevision !== null && !ed.revisionUnreadable && ed.currentRevision === pasteOptionsRevision
    );
  }
  /** Shows the chip only when the plain text differs from what a rich paste
   *  produced would matter — i.e. there is text to fall back to. "Merge
   *  formatting" additionally needs the rich-run payload the paste applied, so
   *  its button is only shown when that payload is available. */
  function offerPasteOptions(plain) {
    const runsJson = lastRichPasteJson;
    lastRichPasteJson = null;
    if (!plain) return hidePasteOptions();
    pasteOptionsPlain = plain;
    pasteOptionsRuns = runsJson;
    pasteOptionsRevision = ed.revisionUnreadable ? null : ed.currentRevision;
    pasteOptionsMergeBtn.hidden = !runsJson;
    pasteOptionsEl.hidden = false;
    requestAnimationFrame(positionPasteOptions);
  }
  function positionPasteOptions() {
    if (pasteOptionsEl.hidden) return;
    const caret = ed.pagesEl.querySelector(".overlay .caret");
    const w = pasteOptionsEl.offsetWidth;
    const h = pasteOptionsEl.offsetHeight;
    const vp = ed.viewportEl.getBoundingClientRect();
    let x;
    let y;
    if (caret) {
      const r = caret.getBoundingClientRect();
      x = r.left;
      y = r.bottom + 6;
    } else {
      x = vp.left + 16;
      y = vp.bottom - h - 16;
    }
    x = Math.max(vp.left + 8, Math.min(x, vp.right - w - 8));
    y = Math.max(vp.top + 8, Math.min(y, vp.bottom - h - 8));
    pasteOptionsEl.style.left = `${Math.round(x)}px`;
    pasteOptionsEl.style.top = `${Math.round(y)}px`;
  }
  async function switchPasteToTextOnly() {
    const text = pasteOptionsPlain;
    const onTop = pasteOptionsStillOnTop();
    hidePasteOptions();
    if (!text || !ed.doc) return;
    if (!onTop) return refusePasteOptions();
    if (ed.doc.canUndo) await ed.runEdit(() => ed.doc.undo());
    await pasteText(text);
    ed.focusEditorSurface();
  }
  /** Says no rather than undoing whatever is on top of the stack instead. */
  function refusePasteOptions() {
    ed.setStatus("The paste is no longer the most recent change; paste options can't be applied", "error");
    ed.focusEditorSurface();
  }

  /** "Merge formatting": undo the rich paste and re-insert it with each run's
   *  properties reduced to the emphasis flags only (bold/italic/underline/
   *  strike/vertAlign). Dropping font, size, color, and highlight lets the text
   *  inherit the destination paragraph's formatting — the Word/Docs behavior. */
  async function switchPasteToMergeFormatting() {
    const runsJson = pasteOptionsRuns;
    const onTop = pasteOptionsStillOnTop();
    hidePasteOptions();
    if (!runsJson || !ed.doc) return;
    if (!onTop) return refusePasteOptions();
    let runs = null;
    try {
      runs = JSON.parse(runsJson);
    } catch {
      /* malformed retained payload; nothing to merge */
    }
    if (!Array.isArray(runs)) return;
    const merged = runs.map((run) =>
      run.paragraphBreak
        ? { paragraphBreak: true }
        : {
            text: run.text,
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            strike: run.strike,
            vertAlign: run.vertAlign,
          },
    );
    if (ed.doc.canUndo) await ed.runEdit(() => ed.doc.undo());
    await pasteRichRunsJson(JSON.stringify(merged));
    ed.focusEditorSurface();
  }
  /** Word's Paste Options ▸ Picture: undo the paste and put the same content in
   *  as one picture of how it looks (`paste_picture.mjs`). The rich runs when the
   *  paste kept them, else the plain text — never a third reading of the
   *  clipboard, which may have changed since. */
  async function switchPasteToPicture() {
    const runsJson = pasteOptionsRuns;
    const text = pasteOptionsPlain;
    const onTop = pasteOptionsStillOnTop();
    hidePasteOptions();
    if ((!runsJson && !text) || !ed.doc) return;
    if (!onTop) return refusePasteOptions();
    let runs = null;
    try {
      runs = runsJson ? JSON.parse(runsJson) : null;
    } catch {
      runs = null;
    }
    await ed.objectInsert.pasteRunsAsPicture(Array.isArray(runs) ? runs : textToRuns(text), {
      beforeInsert: async () => {
        if (ed.doc.canUndo) await ed.runEdit(() => ed.doc.undo());
      },
    });
  }
  onButton(document.getElementById("pasteOptionsTextOnly"), () => void switchPasteToTextOnly());
  onButton(document.getElementById("pasteOptionsPicture"), () => void switchPasteToPicture());
  onButton(pasteOptionsMergeBtn, () => void switchPasteToMergeFormatting());
  onButton(document.getElementById("pasteOptionsClose"), hidePasteOptions);
  pasteOptionsEl.addEventListener("mousedown", (e) => {
    if (e.target.tagName !== "BUTTON") e.preventDefault();
  });
  ed.viewportEl.addEventListener("scroll", hidePasteOptions, { passive: true });
  document.addEventListener("pointerdown", (e) => {
    if (!pasteOptionsEl.hidden && !pasteOptionsEl.contains(e.target)) hidePasteOptions();
  });
  document.addEventListener("keydown", (e) => {
    if (pasteOptionsEl.hidden) return;
    if (e.key === "Escape") return hidePasteOptions();
    const editingKey = [...e.key].length === 1
      || ["Backspace", "Delete", "Enter", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(e.key);
    if (editingKey && !e.metaKey && !e.ctrlKey && !e.altKey) hidePasteOptions();
  }, true);

  document.addEventListener("copy", (e) => {
    if (editorClipboardEvent(e) && ed.hasRange()) ed.copySelection(e);
  });
  document.addEventListener("cut", (e) => {
    if (editorClipboardEvent(e) && ed.hasRange()) cut(e);
  });
  document.addEventListener("paste", (e) => {
    if (editorClipboardEvent(e)) paste(e);
  });
  // `#pages` stays in the tab order (it is the skip-link target and the document's
  // landmark), but it cannot accept text. Hand focus to the proxy when it lands
  // there, so reaching the document by keyboard gives a real text-input surface.
  ed.pagesEl.addEventListener("focus", () => {
    if (modalIsOpen()) return;
    if (!ed.editorTextInputEl) return;
    if (document.activeElement === ed.editorTextInputEl) return;
    ed.editorTextInputEl.focus({ preventScroll: true });
    ed.positionEditorTextInput();
  });

  return {
    commitComposedText, cut, editorTextInputEvent, hidePasteOptions, paste, pasteAsText,
    pasteOptionsShowing, pasteRichRunsJson, pasteText,
  };
}
