// The ruler's page index belongs to the document it was built for (`docs/109`
// HF-400).
//
// The owner's report was "Could not open …: page index 13 out of range (0..4)":
// the ruler still said page 13 of the PREVIOUS document, and the open path's
// fit-zoom asked the new four-page document for it. `builtForPage()` must not
// hand out an index computed for a document that is no longer the open one, and
// `syncToCaret()` must not treat an old document's scale as this one's.
import { test } from "node:test";
import assert from "node:assert/strict";

/** Just enough DOM for `build()`: elements that hold style, dataset and children. */
function fakeElement() {
  return {
    style: { setProperty() {} },
    dataset: {},
    hidden: false,
    children: [],
    appendChild(child) {
      this.children.push(child);
      return child;
    },
    replaceChildren(...kids) {
      this.children = kids;
    },
    addEventListener() {},
    querySelectorAll: () => [],
  };
}
globalThis.document = { createElement: () => fakeElement() };

const { createRuler } = await import("../src/ruler.mjs");

/** An engine handle with `pages` Letter pages, whose caret sits on `caretPage` (1-based). */
function fakeDoc(pages, caretPage) {
  const asked = [];
  return {
    asked,
    pageCount: pages,
    caretRect: () => [caretPage, 0, 0, 0, 0],
    // `updateMarkers` asks for the caret paragraph's indents and tabs; this test
    // is not about them.
    paragraphIndent: () => ({ startTwip: 0, endTwip: 0, firstLineTwip: 0, hangingTwip: 0, free() {} }),
    paragraphTabs: () => [],
    pageRulerGeometry(index) {
      asked.push(index);
      if (index >= pages) throw new Error(`page index ${index} out of range (0..${pages})`);
      return { widthTwip: 12240, marginStartTwip: 1440, marginEndTwip: 1440, free() {} };
    },
  };
}

function pagesOf(n) {
  return Array.from({ length: n }, (_, i) => ({ pageNumber: i + 1 }));
}

test("an index built for one document is not reported once another is open", () => {
  let doc = fakeDoc(16, 16);
  let pages = pagesOf(16);
  const band = () => ({ widths: pages.map(() => 816) });
  const ruler = createRuler({
    getDoc: () => doc,
    getSelection: () => ({ focus: { node: 1, offset: 0 } }),
    getPages: () => pages,
    getBandModel: band,
    runToolbarEdit() {},
    twipsPerInch: 1440,
    labels: {},
  });

  ruler.build();
  assert.equal(ruler.builtForPage(), 15, "precondition: the ruler describes page 16 of the long document");

  // The open path replaces the document before the first render replaces the
  // page set: the ruler's index is now an index into a document that is gone.
  doc = fakeDoc(4, 1);
  assert.equal(ruler.builtForPage(), -1, "a stale index is discarded, not handed to the new document");

  // Once the new page set is published, the caret's page on the NEW document is
  // what the ruler describes.
  pages = pagesOf(4);
  ruler.syncToCaret();
  assert.equal(ruler.builtForPage(), 0);
  assert.deepEqual(doc.asked, [0], "the new document was asked only for a page it has");
});
