// The comment card's quote (RC-12), its editor state (RC-11) and the composer's
// submit keys (RC-16), held without a browser. The in-browser half — the card,
// the real keyboard, and where focus lands (RC-15) — is
// `e2e/review-comment-card.spec.mjs`.
import assert from "node:assert/strict";
import test from "node:test";

const { commentQuoteText, createCommentQuotes, createCommentEditState, QUOTE_LIMIT } = await import(
  "../src/review_comment_card.mjs"
);
const { composerSubmits } = await import("../src/review_chrome.mjs");
const { reviewCardSignature } = await import("../src/review_layout.mjs");

/** A document whose paragraph text the test controls; `copyText` slices it the
 *  way the engine's does for a single-node range. */
function documentWith(text) {
  const doc = {
    text,
    calls: 0,
    copyText(startNode, start, endNode, end) {
      doc.calls += 1;
      assert.equal(startNode, endNode);
      return doc.text.slice(start, end);
    },
  };
  return doc;
}

test("Enter and Ctrl/Cmd+Enter submit a comment; Shift+Enter, Alt+Enter and an IME commit do not", () => {
  assert.equal(composerSubmits({ key: "Enter" }), true);
  assert.equal(composerSubmits({ key: "Enter", ctrlKey: true }), true, "Word's Post comment");
  assert.equal(composerSubmits({ key: "Enter", metaKey: true }), true, "Word's Post comment on a Mac");
  assert.equal(composerSubmits({ key: "Enter", shiftKey: true }), false, "a newline");
  assert.equal(composerSubmits({ key: "Enter", altKey: true }), false);
  assert.equal(composerSubmits({ key: "Enter", isComposing: true }), false, "commits the IME, not the comment");
  assert.equal(composerSubmits({ key: "a" }), false);
});

test("a quote collapses whitespace and is cut at the limit without splitting a character", () => {
  assert.equal(commentQuoteText("  two\n\nlines\t here "), "two lines here");
  const long = "😀".repeat(QUOTE_LIMIT + 5);
  const cut = commentQuoteText(long);
  assert.ok(cut.endsWith("…"));
  assert.equal(Array.from(cut).length, QUOTE_LIMIT + 1, "code points, then the ellipsis");
  assert.ok(!/[\uD800-\uDBFF]…$/.test(cut), "no lone surrogate before the ellipsis");
});

test("a card quotes its live text, and keeps it — marked deleted — once the text is gone", () => {
  const quotes = createCommentQuotes();
  const doc = documentWith("The quick brown fox");
  const comment = { id: "7", anchor: { node: "p1", start: 4, end: 9 } };
  assert.deepEqual({ ...quotes.read(doc, comment) }, { text: "quick", deleted: false });
  // The commented text is deleted: the engine's anchor collapses to a point.
  doc.text = "The  brown fox";
  const collapsed = { ...comment, anchor: { node: "p1", start: 4, end: 4 } };
  assert.deepEqual({ ...quotes.read(doc, collapsed) }, { text: "quick", deleted: true });
  // A new edit that gives the anchor text again is quoted live again.
  doc.text = "The slow brown fox";
  assert.deepEqual({ ...quotes.read(doc, { ...comment, anchor: { node: "p1", start: 4, end: 8 } }) }, {
    text: "slow",
    deleted: false,
  });
});

test("priming at open remembers a quote whose card was never drawn", () => {
  const quotes = createCommentQuotes();
  const doc = documentWith("alpha beta");
  quotes.prime(doc, [{ id: "1", anchor: { node: "p", start: 0, end: 5 } }]);
  doc.text = "";
  assert.deepEqual({ ...quotes.read(doc, { id: "1", anchor: { node: "p", start: 0, end: 0 } }) }, {
    text: "alpha",
    deleted: true,
  });
});

test("replies and never-anchored comments have no quote, and the memory is bounded and clearable", () => {
  const quotes = createCommentQuotes();
  const doc = documentWith("abc");
  assert.equal(quotes.read(doc, { id: "r", parentParaId: "X", anchor: { node: "p", start: 0, end: 3 } }), null);
  assert.equal(quotes.read(doc, { id: "point", anchor: { node: "p", start: 1, end: 1 } }), null);
  for (let i = 0; i < 50; i += 1) quotes.read(doc, { id: "same", anchor: { node: "p", start: 0, end: 3 } });
  assert.equal(quotes.size(), 1, "one entry per comment, however often its card is drawn");
  quotes.clear();
  assert.equal(quotes.size(), 0);
  assert.equal(quotes.read(doc, { id: "same", anchor: { node: "p", start: 0, end: 0 } }), null);
});

test("a read costs one copyText over the anchor and nothing else", () => {
  const quotes = createCommentQuotes();
  const doc = documentWith("x".repeat(1000));
  quotes.read(doc, { id: "1", anchor: { node: "p", start: 0, end: 4 } });
  assert.equal(doc.calls, 1);
});

test("deleting the commented text changes the card's signature, so a cached card is rebuilt", () => {
  const data = { id: "7", text: "note", anchor: { node: "p1", start: 4, end: 4 } };
  const live = reviewCardSignature({ type: "comment", data, quote: { text: "quick", deleted: false } }, []);
  const gone = reviewCardSignature({ type: "comment", data, quote: { text: "quick", deleted: true } }, []);
  assert.notEqual(live, gone);
});

test("the comment being edited survives a rebuild, and its card's signature says so", () => {
  const edit = createCommentEditState();
  const root = { id: "7", paraId: "A", text: "note" };
  const reply = { id: "8", parentParaId: "A", text: "re" };
  const item = { type: "comment", data: root };
  const before = reviewCardSignature(item, [root, reply], {});
  edit.begin("7", "note");
  edit.update("7", "note, fixed");
  assert.equal(edit.draftFor("7"), "note, fixed", "the draft outlives the card that typed it");
  assert.equal(edit.draftFor("8"), null);
  assert.notEqual(reviewCardSignature(item, [root, reply], { editingId: edit.id() }), before);
  edit.begin("8", "re");
  assert.notEqual(
    reviewCardSignature(item, [root, reply], { editingId: edit.id() }),
    before,
    "editing a reply rebuilds the thread that holds it",
  );
  edit.end();
  assert.equal(reviewCardSignature(item, [root, reply], { editingId: edit.id() }), before);
});
