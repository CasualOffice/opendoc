// Tab-memory budget guard (memory-reduction work).
//
// The editor previously kept a live full-size <canvas> for EVERY page plus RGBA
// buffers that were never freed, so a 14-page document at Retina dpr2 used
// >400 MB and memory grew with page count. Page-canvas virtualization + eager
// PageBitmap.free() + a DPR cap make the peak small and page-count-independent.
//
// This spec measures the three real consumers — the WASM linear memory, the
// live canvas pixels, and the JS heap — after a forced GC, and asserts they stay
// under budget for the standard shipped document and for a large (49-page) one.
import { test, expect } from "@playwright/test";
import { makeLargeDocx, makeReviewDocx } from "./large-docx.mjs";

// The reduction target for the standard shipped document (sample.docx, ~14
// pages) at CI's devicePixelRatio of 1. The tab must stay well under 150 MB —
// down from the >400 MB measured before this work (at dpr2, unvirtualized).
const STANDARD_BUDGET = 150 * 1024 * 1024;

// The structural virtualization guard: a 49-page document must stay under
// 200 MB. This is only green once page canvases are virtualized, and stays green
// because tab memory is then bounded by the viewport, not the page count.
const LARGE_BUDGET = 200 * 1024 * 1024;

// A comment- and tracked-change-heavy document, opened with the "Show changes"
// markup view auto-enabled, must stay under 200 MB of measured content the same
// way a plain large document does — the review chrome (markup layout + comment
// sidebar) must not multiply tab memory.
const REVIEW_BUDGET = 200 * 1024 * 1024;

// The regression guard for the O(n²) reply-DOM blowup. A comment card used to
// treat every other top-level comment (all sharing a null threading key) as its
// own reply, so a document with N top-level comments built N² reply subtrees —
// ~500 K DOM nodes for a few-hundred-comment document, gigabytes of tab memory.
// With the reply-matching fixed, the node count is linear in the review size and
// stays far below this bound; before the fix this fixture blows past it (~170 K).
const REVIEW_DOM_NODE_BUDGET = 60_000;

// The live raster canvas count must be bounded by the viewport, never the page
// count, at any devicePixelRatio — the core virtualization invariant. A handful
// of pages are ever live (visible band + one screenful of over-scan each way).
const VIEWPORT_CANVAS_BOUND = 10;

// A review fixture dense in both tracked changes (two revisions per body
// paragraph) and comments (one per body paragraph, none threaded — the null-key
// case the O(n²) bug tripped on). Opening it auto-enables the markup view.
function reviewFixtureBytes(pageCount = 24) {
  return makeReviewDocx(pageCount, { paragraphsPerPage: 6, commentEveryN: 1 });
}

// Injected before any app script so it can wrap the WASM instantiation the
// wasm-bindgen loader performs, collecting every module's exported `memory`.
function installWasmMemoryHook() {
  window.__wasmMems = [];
  const remember = (result) => {
    try {
      const inst = result && (result.instance || (result.exports ? result : null));
      const mem = inst && inst.exports && inst.exports.memory;
      if (mem && mem.buffer && !window.__wasmMems.includes(mem)) window.__wasmMems.push(mem);
    } catch {
      /* ignore */
    }
    return result;
  };
  const wrap = (fn) =>
    function (...args) {
      return Promise.resolve(fn.apply(this, args)).then(remember);
    };
  WebAssembly.instantiate = wrap(WebAssembly.instantiate);
  if (typeof WebAssembly.instantiateStreaming === "function") {
    WebAssembly.instantiateStreaming = wrap(WebAssembly.instantiateStreaming);
  }
}

// The editor is ready once the WASM engine has booted, the document has opened,
// its first render has settled (status cleared, not an error), and pages exist.
async function waitForEditorReady(page, minWraps = 1) {
  await page.waitForFunction(
    (min) => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        status.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length >= min &&
        document.querySelectorAll("canvas.page").length > 0
      );
    },
    minWraps,
    { timeout: 45_000 },
  );
}

// Sum the three real memory consumers after a deterministic collection. Also
// reads the renderer's live DOM node count (including detached-but-retained
// nodes) via CDP — the signal that catches a DOM leak the JS-heap number can
// miss, since a huge detached tree can dwarf `usedJSHeapSize` in native memory.
async function measureMemory(page) {
  const client = await page.context().newCDPSession(page);
  await client.send("HeapProfiler.enable");
  await client.send("HeapProfiler.collectGarbage");
  let domNodes = 0;
  try {
    const counters = await client.send("Memory.getDOMCounters");
    domNodes = counters.nodes ?? 0;
  } catch {
    /* Memory domain unavailable — leave 0; the assertions that use it skip. */
  }
  await client.detach();
  const measured = await page.evaluate(() => {
    const wasm = (window.__wasmMems || []).reduce((sum, m) => sum + m.buffer.byteLength, 0);
    const canvas = [...document.querySelectorAll("canvas")].reduce(
      (sum, c) => sum + c.width * c.height * 4,
      0,
    );
    const js = performance.memory ? performance.memory.usedJSHeapSize : 0;
    return {
      wasm,
      canvas,
      js,
      total: wasm + canvas + js,
      canvasCount: document.querySelectorAll("canvas.page").length,
      pageCount: document.querySelectorAll(".page-wrap").length,
      showingChanges: document.body.classList.contains("showing-changes"),
      devicePixelRatio: window.devicePixelRatio,
    };
  });
  return { ...measured, domNodes };
}

const mb = (n) => `${(n / 1024 / 1024).toFixed(1)} MB`;

test("the standard document stays within the tab memory budget", async ({ page }) => {
  await page.addInitScript(installWasmMemoryHook);
  await page.goto("/editor.html");
  await waitForEditorReady(page);

  const m = await measureMemory(page);
  console.log(
    `standard doc — total ${mb(m.total)} (wasm ${mb(m.wasm)}, canvas ${mb(m.canvas)}, ` +
      `js ${mb(m.js)}); pages=${m.pageCount}, liveCanvases=${m.canvasCount}`,
  );
  expect(m.total).toBeLessThan(STANDARD_BUDGET);
});

test("a 49-page document stays memory-bounded via page virtualization", async ({ page }) => {
  await page.addInitScript(installWasmMemoryHook);
  await page.goto("/editor.html");
  await waitForEditorReady(page);

  // Open a deterministic 49-page document through the real file-open path.
  const bytes = makeLargeDocx(49);
  await page.setInputFiles("#file", {
    name: "large.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from(bytes),
  });
  await waitForEditorReady(page, 45);

  const m = await measureMemory(page);
  console.log(
    `49-page doc — total ${mb(m.total)} (wasm ${mb(m.wasm)}, canvas ${mb(m.canvas)}, ` +
      `js ${mb(m.js)}); pages=${m.pageCount}, liveCanvases=${m.canvasCount}`,
  );
  // The whole point of virtualization: only a handful of pages are ever live.
  expect(m.canvasCount).toBeLessThan(m.pageCount);
  expect(m.total).toBeLessThan(LARGE_BUDGET);
});

// Open a comment-/tracked-change-heavy document through the real file-open path,
// assert every review memory invariant, and return the measurement for logging.
// `expectedPages` lets the caller wait until most pages have paginated.
async function openReviewDocAndMeasure(page, { pageCount = 24 } = {}) {
  await page.setInputFiles("#file", {
    name: "review.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from(reviewFixtureBytes(pageCount)),
  });
  await waitForEditorReady(page, pageCount - 4);
  const m = await measureMemory(page);
  // The markup view auto-enables for a document that carries tracked changes, so
  // this measurement genuinely exercises the review render path.
  expect(m.showingChanges).toBe(true);
  // Content stays bounded — the review chrome does not multiply tab memory.
  expect(m.total).toBeLessThan(REVIEW_BUDGET);
  // The DOM stays linear in the review size: the O(n²) reply-DOM regression blows
  // far past this bound (~170 K nodes for this fixture); the fix keeps it ~10 K.
  if (m.domNodes > 0) expect(m.domNodes).toBeLessThan(REVIEW_DOM_NODE_BUDGET);
  // Canvas virtualization holds even with the second (markup) layout active, at
  // any devicePixelRatio: only a viewport-bounded handful of canvases are live.
  expect(m.canvasCount).toBeLessThan(m.pageCount);
  expect(m.canvasCount).toBeLessThanOrEqual(VIEWPORT_CANVAS_BOUND);
  return m;
}

test("a comment-heavy review document stays memory- and DOM-bounded", async ({ page }) => {
  await page.addInitScript(installWasmMemoryHook);
  await page.goto("/editor.html");
  await waitForEditorReady(page);

  const m = await openReviewDocAndMeasure(page);
  console.log(
    `review doc (dpr ${m.devicePixelRatio}) — total ${mb(m.total)} (wasm ${mb(m.wasm)}, ` +
      `canvas ${mb(m.canvas)}, js ${mb(m.js)}); pages=${m.pageCount}, liveCanvases=${m.canvasCount}, ` +
      `domNodes=${m.domNodes}, showingChanges=${m.showingChanges}`,
  );
});

// The same review document at Retina devicePixelRatio 2 — the real-world case the
// dpr-1 CI default never exercised. The backing-store DPR cap keeps the canvas
// pixels bounded and the live-canvas count is independent of dpr, so every
// invariant above must still hold.
test.describe("at Retina devicePixelRatio 2", () => {
  test.use({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 });

  test("a comment-heavy review document stays bounded at dpr 2", async ({ page }) => {
    await page.addInitScript(installWasmMemoryHook);
    await page.goto("/editor.html");
    await waitForEditorReady(page);

    const m = await openReviewDocAndMeasure(page);
    // Confirm the deviceScaleFactor actually took effect, so this is a real dpr-2
    // measurement (and not silently the dpr-1 path again).
    expect(m.devicePixelRatio).toBeGreaterThan(1);
    console.log(
      `review doc (dpr ${m.devicePixelRatio}) — total ${mb(m.total)} (wasm ${mb(m.wasm)}, ` +
        `canvas ${mb(m.canvas)}, js ${mb(m.js)}); pages=${m.pageCount}, liveCanvases=${m.canvasCount}, ` +
        `domNodes=${m.domNodes}, showingChanges=${m.showingChanges}`,
    );
  });
});
