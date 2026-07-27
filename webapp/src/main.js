// OpenDoc WASM viewer — P1G-001 harness.
//
// Loads the `casual-doc-wasm` module, opens a user-selected `.docx` fully
// client-side, and blits each rendered page onto a canvas. This is the
// browser-first surface the viewer→editor is built and fine-tuned on (docs 56/57);
// no server, deployable as static files (e.g. GitHub Pages).

import init, { open } from "../pkg/casual_doc_wasm.js";

// Network-fetched fallback faces, keyed by a script bucket. The browser WASM
// build ships only bundled Latin faces, so CJK / complex-script runs render as
// `.notdef` tofu (▯) until a covering face is registered — the "browser =
// network-fetched fonts" half of the font-provisioning strategy. CORS-enabled
// raw TTF/OTF (skrifa reads OpenType, not woff2). CJK OTFs are large (~16 MB);
// fetched once, then cached by the browser and in `fontCache`.
const CJK = "https://cdn.jsdelivr.net/gh/googlefonts/noto-cjk@main/Sans/OTF";
const NOTO = "https://cdn.jsdelivr.net/gh/notofonts/notofonts.github.io@main/fonts";
const FALLBACK_FONTS = {
  jp: { url: `${CJK}/Japanese/NotoSansCJKjp-Regular.otf`, scripts: ["Hani", "Hira", "Kana"] },
  kr: { url: `${CJK}/Korean/NotoSansCJKkr-Regular.otf`, scripts: ["Hani", "Hang"] },
  sc: { url: `${CJK}/SimplifiedChinese/NotoSansCJKsc-Regular.otf`, scripts: ["Hani"] },
  arabic: { url: `${NOTO}/NotoSansArabic/hinted/ttf/NotoSansArabic-Regular.ttf`, scripts: ["Arab"] },
  devanagari: { url: `${NOTO}/NotoSansDevanagari/hinted/ttf/NotoSansDevanagari-Regular.ttf`, scripts: ["Deva"] },
  hebrew: { url: `${NOTO}/NotoSansHebrew/hinted/ttf/NotoSansHebrew-Regular.ttf`, scripts: ["Hebr"] },
  thai: { url: `${NOTO}/NotoSansThai/hinted/ttf/NotoSansThai-Regular.ttf`, scripts: ["Thai"] },
};

/** Which fallback bucket (if any) covers a code point. */
function fontKeyFor(cp) {
  if ((cp >= 0x3040 && cp <= 0x30ff) || (cp >= 0x31f0 && cp <= 0x31ff)) return "jp"; // kana
  if ((cp >= 0xac00 && cp <= 0xd7a3) || (cp >= 0x1100 && cp <= 0x11ff) || (cp >= 0x3130 && cp <= 0x318f)) return "kr"; // hangul
  if ((cp >= 0x4e00 && cp <= 0x9fff) || (cp >= 0x3400 && cp <= 0x4dbf) || (cp >= 0xf900 && cp <= 0xfaff)) return "sc"; // han
  if (cp >= 0x0600 && cp <= 0x06ff) return "arabic";
  if (cp >= 0x0900 && cp <= 0x097f) return "devanagari";
  if (cp >= 0x0590 && cp <= 0x05ff) return "hebrew";
  if (cp >= 0x0e00 && cp <= 0x0e7f) return "thai";
  return null;
}

/** url → Uint8Array of already-fetched font bytes (persists across documents). */
const fontCache = new Map();

const statusEl = document.getElementById("status");
const fileEl = document.getElementById("file");
const zoomEl = document.getElementById("zoom");
const pagesEl = document.getElementById("pages");
const dropEl = document.getElementById("drop");
const viewportEl = document.getElementById("viewport");
const fmtButtons = {
  bold: document.getElementById("bold"),
  italic: document.getElementById("italic"),
  underline: document.getElementById("underline"),
};

// The engine `render_page(i, dpi)` rasterizes at `dpi` device px per inch
// (device_px = twip / 1440 * dpi). We render at 96·zoom·devicePixelRatio for a
// crisp result on HiDPI screens, then down-scale via CSS to logical pixels.
const BASE_DPI = 96;

/** The currently open document handle (or null). Kept so a zoom change re-renders. */
let doc = null;
/** Monotonic token so a slow render from a previous file/zoom is discarded. */
let renderToken = 0;
/** Per-page DOM: { pageNumber (1-based), canvas, overlay, twipPerPx }. */
let pages = [];
/** Current selection as model anchors, or null. `focus` trails the pointer. */
let selection = null; // { anchor: {node, offset}, focus: {node, offset} }
let dragging = false;

function setStatus(text, kind = "") {
  statusEl.textContent = text;
  statusEl.className = `status ${kind}`;
}

async function boot() {
  try {
    await init();
    setStatus("Ready — open a .docx");
    fileEl.disabled = false;
  } catch (err) {
    console.error(err);
    setStatus("Failed to load the WASM engine", "error");
  }
}

async function openBytes(bytes, name) {
  try {
    setStatus(`Opening ${name}…`);
    // A previous document's memory is freed when it is dropped; replace it.
    if (doc) doc.free();
    doc = open(bytes);
    selection = null;
    dropEl.hidden = true;
    await provisionFonts(name);
    await renderAll();
  } catch (err) {
    console.error(err);
    setStatus(`Could not open ${name}: ${err.message ?? err}`, "error");
  }
}

// If the freshly-opened document has code points the bundled faces can't cover
// (CJK / complex scripts), fetch the covering Noto face(s) and register them so
// pagination + render pick them up — replacing tofu with real glyphs.
async function provisionFonts(name) {
  if (!doc) return;
  const missing = doc.missingCoverage();
  if (missing.length === 0) return;

  const keys = new Set();
  for (const cp of missing) {
    const key = fontKeyFor(cp);
    if (key) keys.add(key);
  }
  // JP and KR already include Han, so the separate SC fetch is redundant then.
  if (keys.has("jp") || keys.has("kr")) keys.delete("sc");
  if (keys.size === 0) return; // uncovered scripts we have no font for

  setStatus(`Fetching fonts for ${name} (${[...keys].join(", ")})…`);
  for (const key of keys) {
    const { url, scripts } = FALLBACK_FONTS[key];
    try {
      let bytes = fontCache.get(url);
      if (!bytes) {
        const res = await fetch(url);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        bytes = new Uint8Array(await res.arrayBuffer());
        fontCache.set(url, bytes);
      }
      doc.registerFallbackFont(bytes, scripts); // registers + re-paginates
    } catch (err) {
      console.warn(`font ${key} (${url}) failed:`, err);
      setStatus(`Could not load the ${key} font — some text may show as ▯`, "error");
    }
  }
}

async function renderAll() {
  if (!doc) return;
  const token = ++renderToken;
  const zoom = Number(zoomEl.value);
  const dpr = window.devicePixelRatio || 1;
  const dpi = BASE_DPI * zoom * dpr;
  const count = doc.pageCount;

  pagesEl.replaceChildren();
  pages = [];
  setStatus(`Rendering ${count} page${count === 1 ? "" : "s"} at ${Math.round(zoom * 100)}%…`);

  for (let i = 0; i < count; i++) {
    // Yield so a burst of pages does not freeze the tab; abort if superseded.
    if (i > 0 && i % 4 === 0) await new Promise((r) => requestAnimationFrame(r));
    if (token !== renderToken) return;

    const bmp = doc.renderPage(i, dpi);
    const wrap = document.createElement("div");
    wrap.className = "page-wrap";

    const canvas = document.createElement("canvas");
    canvas.className = "page";
    canvas.width = bmp.widthPx;
    canvas.height = bmp.heightPx;
    // Logical CSS size = device pixels / dpr, so the page appears at `zoom` scale.
    canvas.style.width = `${bmp.widthPx / dpr}px`;
    canvas.style.height = `${bmp.heightPx / dpr}px`;

    const ctx = canvas.getContext("2d");
    // The surface is fully opaque, so tiny-skia's premultiplied RGBA equals the
    // straight-alpha RGBA `ImageData` expects — a direct blit is correct.
    const image = new ImageData(bmp.rgba, bmp.widthPx, bmp.heightPx);
    ctx.putImageData(image, 0, 0);

    // A transparent overlay above the canvas holds the caret/selection we draw
    // ourselves from engine geometry — so the highlight matches the raster
    // exactly (doc 58: custom engine-driven selection, no overlay-vs-glyph drift).
    const overlay = document.createElement("div");
    overlay.className = "overlay";

    // The page box in twips — the domain of hit-testing and selection geometry.
    // Scale to CSS px is derived from the canvas's *actual* on-screen rect
    // (below), so alignment holds under any zoom, DPR, or CSS max-width scaling.
    const size = doc.pageSize(i);
    const wTwip = size.widthTwip;
    const hTwip = size.heightTwip;
    size.free();

    wrap.append(canvas, overlay);
    pagesEl.appendChild(wrap);
    pages.push({ pageNumber: i + 1, canvas, overlay, wTwip, hTwip });
  }

  drawSelection(); // re-place any existing selection at the new zoom
  if (token === renderToken) setStatus(`${count} page${count === 1 ? "" : "s"} · ${Math.round(zoom * 100)}%`);
}

// ---- Selection & copy (doc 58 pipeline: hit-test → selection → draw → copy) ---

/** twip → CSS px scale for a page, from the canvas's live on-screen size, so it
 *  tracks the render under any zoom / DPR / CSS scaling. */
function scaleOf(page) {
  const rect = page.canvas.getBoundingClientRect();
  return { rect, sx: rect.width / page.wTwip, sy: rect.height / page.hTwip };
}

/** A pointer event on a page's overlay/canvas → that page's local twip point. */
function pointToTwip(page, event) {
  const { rect, sx, sy } = scaleOf(page);
  return {
    x: Math.round((event.clientX - rect.left) / sx),
    y: Math.round((event.clientY - rect.top) / sy),
  };
}

/** Resolve a pointer event to a model anchor, or null if it misses content. */
function anchorAt(page, event) {
  const { x, y } = pointToTwip(page, event);
  const hit = doc.hitTest(page.pageNumber, x, y);
  if (!hit) return null;
  const anchor = { node: hit.node, offset: hit.offset };
  hit.free(); // release the WASM-owned payload; we copied out its fields
  return anchor;
}

/** Clears every page's caret/selection layer. */
function clearOverlays() {
  for (const p of pages) p.overlay.replaceChildren();
}

/** Draws the current selection from engine geometry: a highlight for a real
 *  range, else a caret at the focus (so a click — or a range with no visible
 *  rects — always shows a cursor). */
function drawSelection() {
  if (!doc) return;
  clearOverlays();
  if (selection) paintSelection(selection);
  updateToolbar();
}

/** Paints the caret or highlight for `sel` from engine geometry. */
function paintSelection({ anchor, focus }) {
  const collapsed = anchor.node === focus.node && anchor.offset === focus.offset;
  if (!collapsed) {
    const rects = doc.selectionRects(anchor.node, anchor.offset, focus.node, focus.offset);
    if (rects.length >= 5) {
      for (let i = 0; i < rects.length; i += 5) place(rects.slice(i, i + 5), "highlight");
      return;
    }
    // No visible rects (e.g. a tiny drag within one caret slot) → fall to caret.
  }
  place(doc.caretRect(focus.node, focus.offset), "caret");
}

/** Places one flat `[page, x, y, w, h]` twip rect as a `kind` box on its page,
 *  converting twips → CSS px with that page's live scale. */
function place(flat, kind) {
  if (flat.length < 5) return;
  const [pageNumber, x, y, w, h] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return;
  const { sx, sy } = scaleOf(page);
  const el = document.createElement("div");
  el.className = kind;
  el.style.left = `${x * sx}px`;
  el.style.top = `${y * sy}px`;
  el.style.width = `${Math.max(w * sx, kind === "caret" ? 2 : 0)}px`;
  el.style.height = `${h * sy}px`;
  page.overlay.appendChild(el);
}

function onPointerDown(page, event) {
  if (event.button !== 0) return;
  const anchor = anchorAt(page, event);
  if (!anchor) return;
  dragging = true;
  // Shift+Click extends the current selection to the click (keeps the anchor).
  selection =
    event.shiftKey && selection
      ? { anchor: selection.anchor, focus: anchor }
      : { anchor, focus: anchor };
  drawSelection();
  event.preventDefault();
}

function onPointerMove(page, event) {
  if (!dragging) return;
  const focus = anchorAt(page, event);
  if (!focus) return;
  selection = { anchor: selection.anchor, focus };
  drawSelection();
}

function onPointerUp() {
  dragging = false;
}

/** Double-click selects the word under the pointer. */
function selectWord(page, event) {
  const a = anchorAt(page, event);
  if (!a) return;
  const bounds = doc.wordAt(a.node, a.offset); // [start, end] or []
  if (bounds.length === 2) {
    selection = {
      anchor: { node: a.node, offset: bounds[0] },
      focus: { node: a.node, offset: bounds[1] },
    };
    drawSelection();
  }
}

async function copySelection() {
  if (!selection) return;
  const { anchor, focus } = selection;
  const text = doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset);
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    const n = text.length;
    setStatus(`Copied ${n} character${n === 1 ? "" : "s"}`);
  } catch (err) {
    console.warn("clipboard write failed:", err);
  }
}

// Delegated pointer handling: resolve which page the event is over.
function pageFromEvent(event) {
  const wrap = event.target.closest?.(".page-wrap");
  if (!wrap) return null;
  const idx = [...pagesEl.children].indexOf(wrap);
  return pages[idx] ?? null;
}
pagesEl.addEventListener("pointerdown", (e) => {
  const page = pageFromEvent(e);
  if (page) onPointerDown(page, e);
});
pagesEl.addEventListener("pointermove", (e) => {
  const page = pageFromEvent(e);
  if (page) onPointerMove(page, e);
});
pagesEl.addEventListener("dblclick", (e) => {
  const page = pageFromEvent(e);
  if (page) selectWord(page, e);
});
window.addEventListener("pointerup", onPointerUp);

// ---- Editing (keys → semantic edits through the WASM choke point) ------------

/** Device DPI the pages are rastered at (HiDPI-crisp). */
function currentDpi() {
  const dpr = window.devicePixelRatio || 1;
  return BASE_DPI * Number(zoomEl.value) * dpr;
}

/** Whether the selection currently spans any text (a real range vs a caret). */
function hasRange() {
  return (
    selection &&
    (selection.anchor.node !== selection.focus.node || selection.anchor.offset !== selection.focus.offset)
  );
}

/** Re-raster a single page in place, reusing its canvas — the incremental repaint
 *  that keeps editing latency to one page, not the whole document. */
function repaintPage(i) {
  const page = pages[i];
  if (!page) return;
  const dpr = window.devicePixelRatio || 1;
  const bmp = doc.renderPage(i, currentDpi());
  const canvas = page.canvas;
  canvas.width = bmp.widthPx;
  canvas.height = bmp.heightPx;
  canvas.style.width = `${bmp.widthPx / dpr}px`;
  canvas.style.height = `${bmp.heightPx / dpr}px`;
  canvas.getContext("2d").putImageData(new ImageData(bmp.rgba, bmp.widthPx, bmp.heightPx), 0, 0);
}

/** Scroll the caret into view only if it is off-screen (no jitter while typing). */
function scrollCaretIntoView() {
  const caret = pagesEl.querySelector(".overlay .caret");
  if (caret) caret.scrollIntoView({ block: "nearest", inline: "nearest" });
}

/** Apply an EditResult: place the caret, repaint only the dirty pages (or rebuild
 *  on a page-count change), redraw the caret, and keep it in view. */
async function applyEditResult(res) {
  const node = res.node;
  const offset = res.offset;
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  selection = { anchor: { node, offset }, focus: { node, offset } };
  if (newCount !== pages.length) {
    await renderAll(); // structural change (page added/removed): rebuild the list
  } else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scrollCaretIntoView();
}

/** Runs an edit thunk and applies its result; unsupported edits are ignored. */
async function runEdit(thunk) {
  let res;
  try {
    res = thunk();
  } catch (err) {
    console.warn("edit ignored:", err?.message ?? err);
    return;
  }
  await applyEditResult(res);
}

/** Move the caret by arrow key. Shift extends (moves the focus); plain collapses. */
function navCaret(dir, extend) {
  if (!selection) return;
  const f = selection.focus;
  const c = doc.moveCaret(f.node, f.offset, dir);
  const to = { node: c.node, offset: c.offset };
  c.free();
  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  scrollCaretIntoView();
}

// ---- Formatting (bold / italic / underline over the selection) ---------------

/** The selection as a same-paragraph ordered range `{node,start,end}`, or null.
 *  (Cross-paragraph formatting is a later slice.) */
function orderedSel() {
  if (!hasRange()) return null;
  const { anchor, focus } = selection;
  if (anchor.node !== focus.node) return null;
  const [start, end] =
    anchor.offset < focus.offset ? [anchor.offset, focus.offset] : [focus.offset, anchor.offset];
  return { node: anchor.node, start, end };
}

/** Applies a run-property delta to the selection, keeping the selection (format
 *  does not collapse it) and repainting only the dirty pages. */
async function formatSelection(delta) {
  const s = orderedSel();
  if (!s) return;
  let res;
  try {
    res = doc.formatText(s.node, s.start, s.end, delta.bold, delta.italic, delta.underline, delta.strike);
  } catch (err) {
    console.warn("format ignored:", err?.message ?? err);
    return;
  }
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  if (newCount !== pages.length) await renderAll();
  else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
}

/** Toggles one toggle (`"bold"`|`"italic"`|`"underline"`) over the selection. */
function toggleFormat(prop) {
  const s = orderedSel();
  if (!s) return;
  const f = doc.formatAt(s.node, s.start, s.end);
  const target = !f[prop];
  f.free();
  formatSelection({ [prop]: target });
}

/** Reflects the selection's format in the toolbar (active + enabled state). */
function updateToolbar() {
  const s = doc ? orderedSel() : null;
  const state = { bold: false, italic: false, underline: false };
  if (s) {
    const f = doc.formatAt(s.node, s.start, s.end);
    state.bold = f.bold;
    state.italic = f.italic;
    state.underline = f.underline;
    f.free();
  }
  for (const key of ["bold", "italic", "underline"]) {
    const btn = fmtButtons[key];
    btn.disabled = !s;
    btn.setAttribute("aria-pressed", String(state[key]));
  }
}

for (const key of ["bold", "italic", "underline"]) {
  // mousedown (not click) so the button never steals focus mid-selection.
  fmtButtons[key].addEventListener("mousedown", (e) => {
    e.preventDefault();
    toggleFormat(key);
  });
}

const ARROWS = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };
const FORMAT_KEYS = { b: "bold", i: "italic", u: "underline" };

document.addEventListener("keydown", async (e) => {
  if (!doc) return;
  // Don't hijack keys aimed at the chrome (file picker, zoom select).
  const tag = e.target?.tagName;
  if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;

  const mod = e.metaKey || e.ctrlKey;
  const key = e.key;

  if (mod && key.toLowerCase() === "c") {
    copySelection();
    return;
  }
  if (mod && key.toLowerCase() === "z") {
    e.preventDefault();
    await runEdit(() => (e.shiftKey ? doc.redo() : doc.undo()));
    return;
  }
  if (mod && key.toLowerCase() === "y") {
    e.preventDefault();
    await runEdit(() => doc.redo());
    return;
  }
  if (mod && FORMAT_KEYS[key.toLowerCase()]) {
    e.preventDefault();
    toggleFormat(FORMAT_KEYS[key.toLowerCase()]);
    return;
  }
  if (mod) return; // leave other shortcuts to the browser

  if (!selection) return;
  const { anchor, focus } = selection;
  const range = hasRange();

  // Arrow navigation — always preventDefault so the page doesn't also scroll.
  if (ARROWS[key]) {
    e.preventDefault();
    navCaret(ARROWS[key], e.shiftKey);
    return;
  }
  if (key === "Backspace") {
    e.preventDefault();
    await runEdit(() =>
      range
        ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset)
        : doc.deleteBackward(focus.node, focus.offset),
    );
    return;
  }
  if (key === "Delete") {
    e.preventDefault();
    await runEdit(() =>
      range
        ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset)
        : doc.deleteForward(focus.node, focus.offset),
    );
    return;
  }
  if (key === "Enter") {
    e.preventDefault();
    if (range) {
      // Replace the selection with a break: delete it, then split at the caret.
      await runEdit(() => doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset));
    }
    await runEdit(() => doc.splitParagraph(selection.focus.node, selection.focus.offset));
    return;
  }
  // A printable character (single key, no modifiers).
  if (key.length === 1) {
    e.preventDefault();
    await runEdit(() =>
      range
        ? doc.replaceSelection(anchor.node, anchor.offset, focus.node, focus.offset, key)
        : doc.insertText(focus.node, focus.offset, key),
    );
  }
});

async function handleFile(file) {
  if (!file) return;
  if (!file.name.toLowerCase().endsWith(".docx")) {
    setStatus("Please choose a .docx file", "error");
    return;
  }
  const buf = await file.arrayBuffer();
  await openBytes(new Uint8Array(buf), file.name);
}

fileEl.addEventListener("change", (e) => handleFile(e.target.files[0]));
zoomEl.addEventListener("change", () => renderAll());

// Drag-and-drop anywhere over the viewport.
for (const type of ["dragover", "drop"]) {
  viewportEl.addEventListener(type, (e) => e.preventDefault());
}
viewportEl.addEventListener("dragover", () => viewportEl.classList.add("dragging"));
viewportEl.addEventListener("dragleave", () => viewportEl.classList.remove("dragging"));
viewportEl.addEventListener("drop", (e) => {
  viewportEl.classList.remove("dragging");
  handleFile(e.dataTransfer?.files?.[0]);
});

fileEl.disabled = true;
boot();
