// The deck viewer's decisions, driven without a browser.
//
// `slides.mjs` is deliberately free of DOM work so these can run under
// `node --test`: the arithmetic, the clamping, the key mapping and the order the
// bitmap's getters are read in are all testable here, and only the wiring needs
// a browser. The browser specs then cover what only a browser can.

import assert from "node:assert/strict";
import test from "node:test";

import { createViewer } from "../src/slides.mjs";

/** A bitmap that enforces the boundary's real ownership rule.
 *
 *  `SlideBitmap::rgba` takes `self` in Rust, so reading it MOVES the value and
 *  frees the handle — every getter after that hits a dropped pointer and
 *  wasm-bindgen throws "null pointer passed to rust". This stub reproduces that
 *  exactly, which is what turns the ordering from a comment into a contract. */
function movingBitmap(width, height) {
  let moved = false;
  return {
    get widthPx() {
      if (moved) throw new Error("null pointer passed to rust");
      return width;
    },
    get heightPx() {
      if (moved) throw new Error("null pointer passed to rust");
      return height;
    },
    get rgba() {
      if (moved) throw new Error("null pointer passed to rust");
      moved = true;
      return new Uint8Array(width * height * 4).fill(255);
    },
  };
}

/** A facade stub: a three-slide 16:9 deck, one of them hidden. */
function stubFacade({ names = ["One", "Two", "Three"], hidden = [false, false, true] } = {}) {
  const rendered = [];
  return {
    rendered,
    open() {
      return {
        slideCount: names.length,
        slideWidthEmu: 12192000,
        slideHeightEmu: 6858000,
        slideName: (index) => names[index] ?? "",
        slideHidden: (index) => hidden[index] ?? false,
        fidelityReport: JSON.stringify({
          findings: [{ feature: "a:gradFill", occurrences: 2, part: "ppt/slides/slide1.xml" }],
        }),
        renderSlide(index, dpi) {
          rendered.push({ index, dpi });
          return movingBitmap(16, 9);
        },
        save: () => new Uint8Array([1, 2, 3]),
      };
    },
  };
}

/** A canvas stub that records what was written to it. */
function stubCanvas() {
  const calls = [];
  return {
    width: 0,
    height: 0,
    style: {},
    calls,
    getContext() {
      return { putImageData: (data) => calls.push({ w: data.width, h: data.height }) };
    },
  };
}

// `ImageData` is a DOM type; the viewer only constructs it, so a shape-compatible
// stand-in is enough and keeps these tests out of a browser.
globalThis.ImageData ??= class {
  constructor(data, width, height) {
    this.data = data;
    this.width = width;
    this.height = height;
  }
};

function viewerWith(facade = stubFacade(), devicePixelRatio = 1) {
  const viewer = createViewer({ facade, elements: {}, devicePixelRatio });
  assert.deepEqual(viewer.open(new Uint8Array([0])), { ok: true });
  return viewer;
}

test("a thumbnail reads the bitmap's dimensions BEFORE its moved pixels", () => {
  // The regression this exists for: `new ImageData(bitmap.rgba, bitmap.widthPx,
  // bitmap.heightPx)` evaluates left to right, so `rgba` freed the handle and the
  // next getter threw. Nothing on the page caught it — the first thumbnail threw,
  // the loop stopped, and the deck never painted at all.
  const viewer = viewerWith();
  const canvas = stubCanvas();
  assert.equal(viewer.paintThumbnail(canvas, 0), true);
  assert.deepEqual(canvas.calls, [{ w: 16, h: 9 }]);
  assert.equal(canvas.width, 16, "the backing store was sized from the bitmap");
});

test("the slide canvas reads its dimensions before its pixels too", () => {
  const viewer = viewerWith();
  const canvas = stubCanvas();
  assert.equal(viewer.paint(canvas, 960), true);
  assert.deepEqual(canvas.calls, [{ w: 16, h: 9 }]);
});

test("the CSS box keeps the deck's own aspect ratio", () => {
  const viewer = viewerWith();
  // 12192000 x 6858000 EMU is 16:9, so 960 CSS pixels wide is 540 tall. A viewer
  // that assumed 4:3 would letterbox every widescreen deck.
  assert.deepEqual(viewer.cssSize(960), { width: 960, height: 540 });
});

test("the dpi asked for follows the device ratio", () => {
  // 12192000 EMU is 13.333 inches. 960 CSS pixels across that is 72 dpi at ratio
  // 1 and 144 at ratio 2 — the difference between a sharp deck and a blurred one
  // on a retina display, which no "a bitmap appeared" assertion can see.
  const one = viewerWith(stubFacade(), 1);
  const two = viewerWith(stubFacade(), 2);
  assert.equal(Math.round(one.dpiFor(960)), 72);
  assert.equal(Math.round(two.dpiFor(960)), 144);
});

test("the sorter reports presentation order, hidden slides included", () => {
  const viewer = viewerWith();
  assert.deepEqual(
    viewer.slides().map((slide) => [slide.name, slide.hidden]),
    [
      ["One", false],
      ["Two", false],
      ["Three", true],
    ],
  );
});

test("navigation clamps at both ends rather than erroring", () => {
  const viewer = viewerWith();
  assert.equal(viewer.goTo(-5), 0, "before the first slide is the first slide");
  assert.equal(viewer.goTo(99), 2, "past the last is the last");
});

test("the key map answers only for navigation keys", () => {
  const viewer = viewerWith();
  viewer.goTo(1);
  assert.equal(viewer.indexForKey("ArrowRight"), 2);
  assert.equal(viewer.indexForKey("ArrowLeft"), 0);
  assert.equal(viewer.indexForKey("Home"), 0);
  assert.equal(viewer.indexForKey("End"), 2);
  assert.equal(viewer.indexForKey(" "), 2, "space pages forward, as a reader expects");
  // Anything else must be null so the handler does not call `preventDefault` and
  // swallow a shortcut the browser or the page owns.
  assert.equal(viewer.indexForKey("a"), null);
  assert.equal(viewer.indexForKey("Tab"), null);
});

test("the key map clamps at the ends instead of wrapping", () => {
  const viewer = viewerWith();
  viewer.goTo(2);
  assert.equal(viewer.indexForKey("ArrowRight"), 2, "the last slide does not wrap to the first");
  viewer.goTo(0);
  assert.equal(viewer.indexForKey("ArrowLeft"), 0);
});

test("a refused deck reports the engine's own reason", () => {
  const viewer = createViewer({
    facade: {
      open() {
        throw new Error("open: the package has no p:sldSz");
      },
    },
    elements: {},
  });
  const result = viewer.open(new Uint8Array([0]));
  assert.equal(result.ok, false);
  // The engine's reason, not a generic apology: "the package has no p:sldSz" is
  // actionable and "could not open file" is not.
  assert.match(result.message, /p:sldSz/);
  assert.equal(viewer.slideCount(), 0, "and nothing is left half-open");
});

test("the fidelity report's findings reach the page", () => {
  const viewer = viewerWith();
  assert.deepEqual(viewer.findings(), [
    { feature: "a:gradFill", occurrences: 2, part: "ppt/slides/slide1.xml" },
  ]);
});

test("a malformed report loses the report, not the deck", () => {
  const facade = {
    open: () => ({
      slideCount: 1,
      slideWidthEmu: 100,
      slideHeightEmu: 100,
      slideName: () => "One",
      slideHidden: () => false,
      fidelityReport: "{not json",
      renderSlide: () => movingBitmap(4, 4),
    }),
  };
  const viewer = createViewer({ facade, elements: {} });
  viewer.open(new Uint8Array([0]));
  // A bug one layer down must not take the slides with it: they still render.
  assert.deepEqual(viewer.findings(), []);
  assert.equal(viewer.paint(stubCanvas(), 100), true);
});

test("nothing renders before a deck is open", () => {
  const viewer = createViewer({ facade: stubFacade(), elements: {} });
  assert.equal(viewer.slideCount(), 0);
  assert.equal(viewer.paint(stubCanvas(), 960), false);
  assert.equal(viewer.paintThumbnail(stubCanvas(), 0), false);
  assert.equal(viewer.indexForKey("ArrowRight"), null, "and a keystroke is inert");
  assert.deepEqual(viewer.slides(), []);
});

test("a thumbnail renders at the fixed low density, not the device's", () => {
  // Thumbnails are navigation decoration, not reading: a 40-slide deck
  // re-rendered at retina density on every open is a visible stall for a strip
  // nobody reads text in.
  const facade = stubFacade();
  const viewer = viewerWith(facade, 2);
  viewer.paintThumbnail(stubCanvas(), 0);
  const thumbnail = facade.rendered.at(-1);
  assert.equal(thumbnail.index, 0);
  assert.ok(thumbnail.dpi < 72, `a thumbnail must not ask for display density: ${thumbnail.dpi}`);
});
