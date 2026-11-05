// The undo snackbar (`docs/109` UX-065, PROPOSAL D-11, audit REVIEW-06/12).
//
// Two guarantees, each driven against a stand-in DOM so they can fail here:
//
// * A BLIND offer (Accept all's: the editor's Undo, which undoes the LAST step)
//   never undoes something else. After any later edit it is withdrawn, and its
//   button refuses if the document's version moved under it. A BOUND offer
//   (Resolve's, which names its comment) survives edits.
// * When the offer goes away holding focus, focus goes back to the document,
//   never to <body>.
//
// The browser half — typing after Accept all really withdraws it, through the
// edit choke point — is `e2e/review-layout.spec.mjs`.
import { test } from "node:test";
import assert from "node:assert/strict";

function fakeDom() {
  const listeners = new Map();
  const make = (id) => ({
    id,
    hidden: true,
    textContent: "",
    dataset: {},
    children: [],
    addEventListener(type, fn) {
      listeners.set(`${id}:${type}`, fn);
    },
    contains(el) {
      return el === this || this.children.includes(el);
    },
  });
  const bar = make("undoSnackbar");
  const text = make("undoSnackbarText");
  const undo = make("undoSnackbarUndo");
  bar.children.push(text, undo);
  const body = make("body");
  const doc = {
    body,
    activeElement: body,
    getElementById: (id) => ({ undoSnackbar: bar, undoSnackbarText: text, undoSnackbarUndo: undo })[id] ?? null,
  };
  const click = () => listeners.get("undoSnackbarUndo:click")();
  const key = (k) => listeners.get("undoSnackbar:keydown")({ key: k });
  return { doc, bar, undo, click, key };
}

const { showUndoSnackbar, hideUndoSnackbar, withdrawUndoSnackbarOnEdit } = await import("../src/undo_snackbar.mjs");

test("a blind Undo is withdrawn by the next edit and never undoes that edit", () => {
  const dom = fakeDom();
  let version = 7;
  let undone = 0;
  showUndoSnackbar("All changes accepted", () => undone++, { version: () => version }, dom.doc);
  assert.equal(dom.bar.hidden, false);
  // The person types: the edit choke point reports it.
  version = 8;
  withdrawUndoSnackbarOnEdit({}, dom.doc);
  assert.equal(dom.bar.hidden, true, "the offer is withdrawn by the edit after it");
  hideUndoSnackbar(dom.doc);
});

test("a blind Undo's button refuses when the version moved without the choke point seeing it", () => {
  const dom = fakeDom();
  let version = 1;
  let undone = 0;
  showUndoSnackbar("All changes rejected", () => undone++, { version: () => version }, dom.doc);
  version = 2;
  dom.click();
  assert.equal(undone, 0, "Undo would have undone the later step, not the decision");
  assert.equal(dom.bar.hidden, true);
  // And with nothing in between it does undo.
  showUndoSnackbar("All changes rejected", () => undone++, { version: () => version }, dom.doc);
  dom.click();
  assert.equal(undone, 1);
});

test("an unreadable revision withdraws a blind offer; a bound offer survives edits but not another document", () => {
  const dom = fakeDom();
  showUndoSnackbar("x", () => {}, { version: () => 1 }, dom.doc);
  withdrawUndoSnackbarOnEdit({ unreadable: true }, dom.doc);
  assert.equal(dom.bar.hidden, true);
  let reopened = 0;
  showUndoSnackbar("Comment resolved", () => reopened++, { survivesEdits: true }, dom.doc);
  withdrawUndoSnackbarOnEdit({}, dom.doc);
  withdrawUndoSnackbarOnEdit({ unreadable: true }, dom.doc);
  assert.equal(dom.bar.hidden, false, "Resolve's Undo names its comment, so an edit does not make it wrong");
  dom.click();
  assert.equal(reopened, 1);
  showUndoSnackbar("Comment resolved", () => reopened++, { survivesEdits: true }, dom.doc);
  withdrawUndoSnackbarOnEdit({ replaced: true }, dom.doc);
  assert.equal(dom.bar.hidden, true, "another document withdraws every offer");
});

test("hiding while the button holds focus puts focus in the document, never on <body>", () => {
  for (const close of ["click", "escape", "edit"]) {
    const dom = fakeDom();
    let version = 1;
    const focused = [];
    showUndoSnackbar("x", () => {}, { version: () => version, focusBack: () => focused.push("editor") }, dom.doc);
    dom.doc.activeElement = dom.undo;
    if (close === "click") dom.click();
    if (close === "escape") dom.key("Escape");
    if (close === "edit") {
      version = 2;
      withdrawUndoSnackbarOnEdit({}, dom.doc);
    }
    assert.equal(dom.bar.hidden, true, close);
    assert.deepEqual(focused, ["editor"], `${close}: focus was returned to the document`);
  }
  // Focus elsewhere is left alone.
  const dom = fakeDom();
  const focused = [];
  showUndoSnackbar("x", () => {}, { version: () => 1, focusBack: () => focused.push("editor") }, dom.doc);
  hideUndoSnackbar(dom.doc);
  assert.deepEqual(focused, []);
});

test("the edit choke point withdraws it: every landed edit, and opening another document", async () => {
  const dom = fakeDom();
  globalThis.document = dom.doc;
  const { createDirtyTracking } = await import("../src/editor_dirty.mjs");
  const ed = {
    currentRevision: 3,
    documentEpoch: 1,
    clearFindParagraphCache() {},
    objectPresence: { forget() {} },
    setDocumentState() {},
    noteDraftDirty() {},
    spellChecker: { noteEdited() {} },
    remotePresence: { sync() {} },
    reviewTracking: { follow() {} },
    SESSION: {},
  };
  const dirty = createDirtyTracking(ed);
  const version = () => `${ed.documentEpoch}:${ed.currentRevision}`;
  showUndoSnackbar("All changes accepted", () => {}, { version }, dom.doc);
  dirty.noteDocumentEdited(4);
  assert.equal(dom.bar.hidden, true, "a keystroke after Accept all withdraws its Undo");
  showUndoSnackbar("Comment resolved", () => {}, { survivesEdits: true }, dom.doc);
  dirty.noteDocumentEdited(5);
  assert.equal(dom.bar.hidden, false, "a bound offer survives the edit");
  dirty.resetDirtyTracking("other.docx");
  assert.equal(dom.bar.hidden, true, "opening another document withdraws it");
  delete globalThis.document;
});
