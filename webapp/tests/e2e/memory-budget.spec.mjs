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
import { makeLargeDocx } from "./large-docx.mjs";

// The reduction target for the standard shipped document (sample.docx, ~14
// pages) at CI's devicePixelRatio of 1. The tab must stay well under 150 MB —
// down from the >400 MB measured before this work (at dpr2, unvirtualized).
const STANDARD_BUDGET = 150 * 1024 * 1024;

// The structural virtualization guard: a 49-page document must stay under
// 200 MB. This is only green once page canvases are virtualized, and stays green
// because tab memory is then bounded by the viewport, not the page count.
const LARGE_BUDGET = 200 * 1024 * 1024;

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

// Sum the three real memory consumers after a deterministic collection.
async function measureMemory(page) {
  const client = await page.context().newCDPSession(page);
  await client.send("HeapProfiler.enable");
  await client.send("HeapProfiler.collectGarbage");
  await client.detach();
  return page.evaluate(() => {
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
    };
  });
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
