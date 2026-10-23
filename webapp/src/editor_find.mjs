// Find and replace: the panel's controls, the match scan over the engine's
// paragraph text, Replace / Replace all, and the Find session that refines as
// you type (TA-03, TA-04). Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { matchWithinScope, positionComparator } from "./find_scope.mjs";
import { createFindSession } from "./find_session.mjs";
import { byteOffsetToStringIndex, isWholeWordAt } from "./text_rules.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createFind(ed) {
  const findBtn = document.getElementById("findBtn");
  const findPanel = document.getElementById("findPanel");
  const findInput = document.getElementById("findInput");
  const replaceInput = document.getElementById("replaceInput");
  const findStatus = document.getElementById("findStatus");
  const findCase = document.getElementById("findCase");
  const findWholeWord = document.getElementById("findWholeWord");
  const findSelection = document.getElementById("findSelection");
  let findScope = null;
  const findSession = createFindSession({
    getDoc: () => ed.doc,
    getSelection: () => ed.selection,
    isImplicit: () => ed.caretIsImplicit(),
    place: (flat, kind) => ed.place(flat, kind),
    revisionKey: () => `${ed.documentEpoch}:${ed.currentRevision}`,
    active: () => !findPanel.hidden,
    repaint: () => ed.paintOverlayLayer(),
    // The highlight only: the status line keeps what the last action said ("Replaced 4").
    rescan: () => findSession.show(scanAllMatches(findInput.value, findCase.checked, findWholeWord.checked)),
  });

  // ---- Find / replace ---------------------------------------------------------
  const FIND_SCAN_CAP = 5000;

  function setFindStatus(text, miss = false) {
    findStatus.textContent = text;
    findStatus.classList.toggle("miss", miss);
  }

  const findParagraphTextCache = new Map();

  function paragraphTextForFind(node) {
    if (!findParagraphTextCache.has(node)) {
      const length = ed.doc.paragraphLength(node);
      findParagraphTextCache.set(node, ed.doc.copyText(node, 0, node, length));
    }
    return findParagraphTextCache.get(node);
  }

  /** Whether an engine match stands alone as a word — the "Whole word" option.
   *  The paragraph text comes from the engine; the boundary rule itself is
   *  `isWholeWordAt`, which is pure and unit-tested. */
  function isWholeWordMatch(match) {
    const text = paragraphTextForFind(match.startNode);
    return isWholeWordAt(
      text,
      byteOffsetToStringIndex(text, match.startOffset),
      byteOffsetToStringIndex(text, match.endOffset),
    );
  }

  function clearFindParagraphCache() {
    findParagraphTextCache.clear();
  }

  // Capture the current selection as an ordered scope for "find in selection".
  // selectionEdge(...false) is the document-order start, (...true) the end. We
  // copy the values out and free the WASM handles immediately. Returns null when
  // there is no non-empty range to scope to.
  function captureFindScope() {
    if (!ed.selection || !ed.hasRange()) return null;
    const { anchor, focus } = ed.selection;
    const s = ed.doc.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, false);
    const e = ed.doc.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, true);
    const scope = {
      startNode: s.node,
      startOffset: s.offset,
      endNode: e.node,
      endOffset: e.offset,
    };
    s.free();
    e.free();
    return scope;
  }

  // True iff position (aNode:aOff) <= position (bNode:bOff) in document order.
  // Same node compares offsets with no WASM call; otherwise selectionEdge(...false)
  // returns whichever endpoint is earlier, and since the two nodes differ the
  // returned node uniquely identifies which one that is.

  // A find match spans a single paragraph, so match.startNode === match.endNode.
  // Accept iff [match.startOffset .. match.endOffset] on that node lies within the
  // ordered scope [scopeStart .. scopeEnd]. Boundary-node matches (the common
  // single-paragraph case and the first/last paragraph of a multi-paragraph
  // scope) resolve with zero extra WASM calls; only genuinely-interior nodes need
  // the two order tests.
  function matchInFindSelection(match) {
    if (!findSelection.checked) return true;
    if (!findScope) return false;
    return matchWithinScope(match, findScope, positionComparator(ed.doc));
  }

  /** Scans every match in document order, starting from the top, up to
   * FIND_SCAN_CAP — bounded like every other pagination/parse loop in this
   * codebase, since findText wraps around and would otherwise loop forever.
   * Once every match has been visited once, the engine's wrap fallback can
   * re-surface *any* earlier match (not necessarily the first one), so
   * termination checks membership in every key seen so far, not just the
   * first — comparing only to the first match's key under-counts (it can
   * oscillate between two later matches forever without ever revisiting the
   * exact first one). */
  function scanAllMatches(query, matchCase, wholeWord = false) {
    const matches = [];
    if (!ed.doc || !query) return matches;
    const first = ed.doc.firstPosition();
    let node = first.node;
    let offset = first.offset;
    first.free();
    const seen = new Set();
    for (let i = 0; i < FIND_SCAN_CAP; i++) {
      const match = ed.doc.findText(query, node, offset, true, matchCase);
      if (!match.found) {
        match.free();
        break;
      }
      const key = `${match.startNode}:${match.startOffset}`;
      if (seen.has(key)) {
        match.free();
        break;
      }
      seen.add(key);
      const candidate = {
        startNode: match.startNode,
        startOffset: match.startOffset,
        endNode: match.endNode,
        endOffset: match.endOffset,
      };
      if ((!wholeWord || isWholeWordMatch(candidate)) && matchInFindSelection(candidate)) {
        matches.push(candidate);
      }
      node = match.endNode;
      offset = match.endOffset;
      match.free();
    }
    return matches;
  }

  /** Publishes the match list to the highlight and states "n of N". An untouched
   *  load-time caret is not a place the user searched from, so a document whose
   *  first characters are the query reports "N matches", not "1 of N". */
  function updateFindStatus(matches = scanAllMatches(findInput.value, findCase.checked, findWholeWord.checked)) {
    const idx = findSession.show(matches);
    if (!findInput.value) setFindStatus("");
    else if (!matches.length) setFindStatus("No match", true);
    else setFindStatus(matches.length === 1 ? "1 match" : idx >= 0 ? `${idx + 1} of ${matches.length}` : `${matches.length} matches`);
  }

  function selectedPlainText() {
    if (!ed.selection) return "";
    const { anchor, focus } = ed.selection;
    if (anchor.node === focus.node && anchor.offset === focus.offset) return "";
    return ed.doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset);
  }

  function queryMatchesSelection() {
    const query = findInput.value;
    if (!query) return false;
    const selected = selectedPlainText();
    if (!selected.includes("\n")) {
      return findCase.checked
        ? selected === query
        : selected.toLocaleLowerCase() === query.toLocaleLowerCase();
    }
    return false;
  }

  function selectTextMatch(match, matches) {
    if (!match || match.found === false) {
      setFindStatus("No match", true);
      return false;
    }
    ed.selection = {
      anchor: { node: match.startNode, offset: match.startOffset },
      focus: { node: match.endNode, offset: match.endOffset },
    };
    updateFindStatus(matches); // before the repaint, so it paints this match as current
    ed.drawSelection();
    // Deliberately does NOT call focusEditorSurface(): this runs on every
    // keystroke while live-searching (findInput's "input" listener), and
    // stealing focus back to the canvas mid-typing sent subsequent keystrokes
    // to the document instead of the find box. Focus returns to the canvas
    // only when the panel actually closes (closeFind).
    ed.scrollFindMatchIntoView();
    return true;
  }

  /** One Find step. `incremental` (the query was typed) re-searches from the
   *  session's anchor and never advances; otherwise Next/Previous step from the
   *  current match, or from the caret (`find_session.mjs`, TA-03). */
  function findFromSelection(forward, incremental = false) {
    const matches = ed.doc && findInput.value ? scanAllMatches(findInput.value, findCase.checked, findWholeWord.checked) : [];
    if (!matches.length) {
      updateFindStatus(matches);
      return false;
    }
    return selectTextMatch(matches[findSession.pick(matches, forward, incremental)], matches);
  }

  async function replaceCurrentMatch() {
    if (!ed.doc || !findInput.value) return;
    if (ed.blockUntrackedInSuggesting()) return;
    if (!queryMatchesSelection()) {
      findFromSelection(true);
      return;
    }
    const { anchor, focus } = ed.selection;
    await ed.runEdit(() =>
      ed.doc.replaceSelection(anchor.node, anchor.offset, focus.node, focus.offset, replaceInput.value),
    );
    findFromSelection(true);
  }

  /** Replaces every match in document order. Each iteration re-finds from just
   * past the previous replacement (not from the top), so a replacement text
   * that itself contains the query (e.g. "cat" -> "cats") can never re-match
   * what was just inserted and loop forever; FIND_SCAN_CAP is a bounded
   * backstop regardless. */
  async function replaceAllMatches() {
    if (!ed.doc || !findInput.value) return;
    if (ed.blockUntrackedInSuggesting()) return;
    const replacement = replaceInput.value;
    // Collect every match (honoring case / whole-word / selection scope) in
    // document order, then replace them ALL as ONE undoable action — Word and
    // Google Docs undo a Replace All in a single step. The ranges are passed in
    // DESCENDING document order (reverse of the scan) so applying each replacement
    // never shifts an earlier, not-yet-applied match's offsets.
    const matches = scanAllMatches(findInput.value, findCase.checked, findWholeWord.checked);
    if (!matches.length) {
      setFindStatus("No match", true);
      return;
    }
    const ordered = matches.slice().reverse();
    await ed.runEdit(() =>
      ed.doc.replaceRanges(
        ordered.map((m) => m.startNode),
        ordered.map((m) => m.startOffset),
        ordered.map((m) => m.endNode),
        ordered.map((m) => m.endOffset),
        replacement,
      ),
    );
    setFindStatus(`Replaced ${matches.length}`);
  }

  function openFind({ replace = false } = {}) {
    if (!ed.doc) return;
    findPanel.hidden = false;
    if (findSelection.checked) findSelection.dispatchEvent(new Event("change"));
    const selected = selectedPlainText();
    if (selected && !selected.includes("\n") && selected.length <= 80) findInput.value = selected;
    updateFindStatus();
    // ⌘H lands on the replacement once there is a query to replace (VS Code's rule; Word's Replace tab).
    const field = replace && findInput.value ? replaceInput : findInput;
    field.focus();
    field.select();
  }

  function closeFind() {
    findPanel.hidden = true;
    findSession.clear();
    ed.focusEditorSurface();
  }

  findInput.addEventListener("input", () => {
    if (findInput.value) findFromSelection(true, true); // refine from the anchor, never advance (TA-03)
    else updateFindStatus();
  });
  findInput.addEventListener("focus", () => findSession.anchorHere());
  findCase.addEventListener("change", () => updateFindStatus());
  findWholeWord.addEventListener("change", () => updateFindStatus());
  findSelection.addEventListener("change", () => {
    if (findSelection.checked) {
      findScope = captureFindScope();
    } else {
      findScope = null;
    }
    updateFindStatus();
  });
  findInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      findFromSelection(!e.shiftKey);
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeFind();
    }
  });
  replaceInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      replaceCurrentMatch();
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeFind();
    }
  });
  findCase.addEventListener("change", () => findFromSelection(true, true));
  // The find bar's five buttons, looked up where they are USED: one listener each,
  // and five names in the widest scope in the product for them.
  document.getElementById("findPrev").addEventListener("click", () => findFromSelection(false));
  document.getElementById("findNext").addEventListener("click", () => findFromSelection(true));
  document.getElementById("replaceOne").addEventListener("click", replaceCurrentMatch);
  document.getElementById("replaceAll").addEventListener("click", replaceAllMatches);
  document.getElementById("findClose").addEventListener("click", closeFind);
  findBtn.addEventListener("click", () => openFind());
  return { clearFindParagraphCache, findBtn, findSession, openFind };
}
