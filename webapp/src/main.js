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
  strike: document.getElementById("strike"),
};
const alignBtns = {
  start: document.getElementById("alignStart"),
  center: document.getElementById("alignCenter"),
  end: document.getElementById("alignEnd"),
  justify: document.getElementById("alignJustify"),
};
const superBtn = document.getElementById("superscript");
const subBtn = document.getElementById("subscript");
const fontSizeSel = document.getElementById("fontSize");
const textColorInput = document.getElementById("textColor");
const highlightSel = document.getElementById("highlight");
const lineSpacingSel = document.getElementById("lineSpacing");
const indentDecBtn = document.getElementById("indentDec");
const indentIncBtn = document.getElementById("indentInc");
const bulletListBtn = document.getElementById("bulletList");
const numberedListBtn = document.getElementById("numberedList");
const insertRowBtn = document.getElementById("insertRow");
const deleteRowBtn = document.getElementById("deleteRow");
const insertColumnBtn = document.getElementById("insertColumn");
const deleteColumnBtn = document.getElementById("deleteColumn");
const fontFamilySel = document.getElementById("fontFamily");
const paragraphStyleSel = document.getElementById("paragraphStyle");
const runControls = [superBtn, subBtn, fontSizeSel, textColorInput, highlightSel, fontFamilySel];
const paraControls = [
  ...Object.values(alignBtns),
  lineSpacingSel,
  indentDecBtn,
  indentIncBtn,
  bulletListBtn,
  numberedListBtn,
  paragraphStyleSel,
];
const saveBtn = document.getElementById("save");
const zoomInBtn = document.getElementById("zoomIn");
const zoomOutBtn = document.getElementById("zoomOut");
const tableGroup = document.getElementById("tableGroup");
const docTitleEl = document.getElementById("docTitle");
const titleDividerEl = document.getElementById("titleDivider");
const statsEl = document.getElementById("stats");
const statWords = document.getElementById("statWords");
const statParas = document.getElementById("statParas");
const statPages = document.getElementById("statPages");

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
/** Armed run formatting for typing at a collapsed caret (e.g. click Bold with no
 *  selection → next typed characters are bold). `null` when nothing is armed; else
 *  a subset of { bold, italic, underline, strike } → boolean. Cleared whenever the
 *  caret moves for any reason other than the typing that consumes it. */
let pendingFormat = null;
/** The open document's filename, for the Save download. */
let currentName = "document.docx";

function setStatus(text, kind = "") {
  statusEl.textContent = text;
  statusEl.className = `status ${kind}`;
}

/** Refreshes the footer word / paragraph / page counts from the engine. */
function updateStats() {
  if (!doc) {
    statsEl.hidden = true;
    return;
  }
  const s = doc.documentStats();
  const words = s.words;
  const paras = s.paragraphs;
  s.free();
  statWords.textContent = `${words.toLocaleString()} word${words === 1 ? "" : "s"}`;
  statParas.textContent = `${paras.toLocaleString()} paragraph${paras === 1 ? "" : "s"}`;
  statsEl.hidden = false;
  statPages.hidden = false;
  updatePageNumber();
}

/** Cheap current-page update (caret's page / total), for caret moves. */
function updatePageNumber() {
  if (!doc || !pages.length) return;
  let cur = 1;
  if (selection) {
    const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
    if (flat.length) cur = flat[0];
  }
  statPages.textContent = `Page ${cur} of ${pages.length}`;
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
    currentName = name;
    docTitleEl.textContent = name;
    docTitleEl.hidden = false;
    titleDividerEl.hidden = false;
    saveBtn.disabled = false;
    populateStyles();
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
  if (token === renderToken) {
    setStatus("");
    updateStats();
  }
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
  if (selection) {
    paintActiveCell(selection.focus); // under the caret/highlight
    paintSelection(selection);
  }
  updateToolbar();
  updatePageNumber();
}

/** Outlines the table cell the caret is in (nothing when not in a table), so the
 *  user always sees which cell they are editing. */
function paintActiveCell(focus) {
  const flat = doc.cellRect(focus.node); // [page, x, y, w, h] twips, or []
  if (flat.length >= 5) place(flat, "cell-outline");
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
  pendingFormat = null; // a click moves the caret → disarm typing format
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
// Triple-click selects the paragraph (the click's `detail` is the click count).
pagesEl.addEventListener("click", (e) => {
  if (e.detail !== 3) return;
  const page = pageFromEvent(e);
  if (!page) return;
  const a = anchorAt(page, e);
  if (!a) return;
  selection = {
    anchor: { node: a.node, offset: 0 },
    focus: { node: a.node, offset: doc.paragraphLength(a.node) },
  };
  drawSelection();
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
  updateStats(); // word/paragraph counts may have changed
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
  pendingFormat = null; // caret moved → disarm typing format
  const f = selection.focus;
  const c = doc.moveCaret(f.node, f.offset, dir);
  const to = { node: c.node, offset: c.offset };
  c.free();
  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  scrollCaretIntoView();
}

// ---- Formatting toolbar (run + paragraph properties) -------------------------

/** The current selection endpoints as `[sNode, sOff, eNode, eOff]`, or null. */
function selEndpoints() {
  if (!selection) return null;
  const { anchor, focus } = selection;
  return [anchor.node, anchor.offset, focus.node, focus.offset];
}

/** Runs a toolbar edit thunk `(sNode, sOff, eNode, eOff) => EditResult`,
 *  preserving the selection (formatting does not collapse it) and repainting
 *  only the dirty pages. */
async function runToolbarEdit(thunk) {
  const ends = selEndpoints();
  if (!ends) return;
  let res;
  try {
    res = thunk(...ends);
  } catch (err) {
    console.warn("edit ignored:", err?.message ?? err);
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

/** The uniform run-format state over the selection, or null if not a range. */
function selectionFormat() {
  if (!doc || !hasRange()) return null;
  const [sn, so, en, eo] = selEndpoints();
  const f = doc.selectionFormat(sn, so, en, eo);
  const state = { bold: f.bold, italic: f.italic, underline: f.underline, strike: f.strike };
  f.free();
  return state;
}

/** The run format the collapsed caret inherits (what new typing would carry). */
function caretFormatState() {
  if (!doc || !selection) return { bold: false, italic: false, underline: false, strike: false };
  const f = doc.caretFormat(selection.focus.node, selection.focus.offset);
  const state = { bold: f.bold, italic: f.italic, underline: f.underline, strike: f.strike };
  f.free();
  return state;
}

/** Toggles a run toggle (`bold`/`italic`/`underline`/`strike`). With a range it
 *  formats the selection; at a collapsed caret it arms the format for typing
 *  (premium editors: press Bold, then type — the text comes out bold). */
function toggleFormat(prop) {
  if (!hasRange()) {
    // Arm/disarm at the caret: flip the effective current value (pending overrides
    // the caret's inherited format), and reflect it in the toolbar.
    const current = pendingFormat?.[prop] ?? caretFormatState()[prop];
    pendingFormat = { ...(pendingFormat || {}), [prop]: !current };
    updateToolbar();
    return;
  }
  const state = selectionFormat();
  if (!state) return;
  runToolbarEdit((sn, so, en, eo) =>
    doc.formatSelection(
      sn,
      so,
      en,
      eo,
      prop === "bold" ? !state.bold : undefined,
      prop === "italic" ? !state.italic : undefined,
      prop === "underline" ? !state.underline : undefined,
      prop === "strike" ? !state.strike : undefined,
    ),
  );
}

/** "#rrggbb" → [r, g, b]. */
function hexToRgb(hex) {
  return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
}

/** Reflects the selection in the toolbar: active states + which controls are
 *  enabled (run controls need a text range; paragraph controls need a caret). */
function updateToolbar() {
  const hasSel = !!selection;
  const range = hasRange();
  const runState = selectionFormat();

  // B/I/U/S work with a range (format it) or a collapsed caret (arm it for typing),
  // so they are enabled whenever there is any selection. Pressed state: a range
  // reflects its uniform run format; a caret reflects the armed format (if any) over
  // the format new typing would inherit.
  const caretFmt = !range && hasSel ? caretFormatState() : null;
  for (const key of ["bold", "italic", "underline", "strike"]) {
    fmtButtons[key].disabled = !hasSel;
    const pressed = range
      ? runState && runState[key]
      : (pendingFormat?.[key] ?? (caretFmt ? caretFmt[key] : false));
    fmtButtons[key].setAttribute("aria-pressed", String(!!pressed));
  }
  for (const el of runControls) el.disabled = !range;
  for (const el of paraControls) el.disabled = !hasSel;

  const align = hasSel && doc ? doc.alignmentAt(selection.focus.node, selection.focus.offset) : "start";
  for (const [key, btn] of Object.entries(alignBtns)) {
    btn.setAttribute("aria-pressed", String(key === align));
  }

  // Reflect the current run styling (size / font / color / super-sub) of a range.
  let size = "";
  let font = "";
  let sup = false;
  let sub = false;
  if (range && doc) {
    const rs = doc.selectionRunStyle(
      selection.anchor.node,
      selection.anchor.offset,
      selection.focus.node,
      selection.focus.offset,
    );
    if (rs.sizePoints) size = String(rs.sizePoints);
    font = rs.font;
    if (rs.color) textColorInput.value = rs.color;
    sup = rs.superscript;
    sub = rs.subscript;
    rs.free();
  }
  fontSizeSel.value = size;
  fontFamilySel.value = font;
  superBtn.setAttribute("aria-pressed", String(sup));
  subBtn.setAttribute("aria-pressed", String(sub));

  // Reflect the current paragraph style + line spacing + list kind.
  paragraphStyleSel.value = hasSel && doc ? doc.paragraphStyleAt(selection.focus.node) : "";
  lineSpacingSel.value = hasSel && doc ? String(doc.lineSpacingAt(selection.focus.node) || "") : "";
  const listKind = hasSel && doc ? doc.listStyleAt(selection.focus.node) : "";
  bulletListBtn.setAttribute("aria-pressed", String(listKind === "bullet"));
  numberedListBtn.setAttribute("aria-pressed", String(listKind === "numbered"));

  // Table controls: a contextual group that appears only when the caret is inside
  // a table cell (Google-Docs style — no permanent clutter for non-table docs).
  const inTable = hasSel && doc ? doc.inTable(selection.focus.node) : false;
  tableGroup.hidden = !inTable;
}

/** Fills the paragraph-style dropdown from the open document's styles. */
function populateStyles() {
  const styles = doc ? doc.listStyles() : [];
  paragraphStyleSel.replaceChildren();
  for (const [value, label] of [["", "Style"], ...styles.map((s) => [s, s])]) {
    const opt = document.createElement("option");
    opt.value = value;
    opt.textContent = label;
    paragraphStyleSel.appendChild(opt);
  }
}

// mousedown (not click) so a button never steals the selection focus mid-edit.
function onButton(el, handler) {
  el.addEventListener("mousedown", (e) => {
    e.preventDefault();
    handler();
  });
}

for (const key of ["bold", "italic", "underline", "strike"]) {
  onButton(fmtButtons[key], () => toggleFormat(key));
}
onButton(superBtn, () => runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, "super")));
onButton(subBtn, () => runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, "sub")));
for (const [key, btn] of Object.entries(alignBtns)) {
  onButton(btn, () => runToolbarEdit((a, b, c, d) => doc.setAlignment(a, b, c, d, key)));
}
onButton(indentDecBtn, () => runToolbarEdit((a, b, c, d) => doc.adjustIndent(a, b, c, d, -360)));
onButton(indentIncBtn, () => runToolbarEdit((a, b, c, d) => doc.adjustIndent(a, b, c, d, 360)));
onButton(bulletListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "bullet")));
onButton(numberedListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "numbered")));
// Row ops act on the caret's paragraph (not a range) and move the caret to the
// new / surviving row, so they run through the edit path, not runToolbarEdit.
onButton(insertRowBtn, () => {
  if (selection) runEdit(() => doc.insertRow(selection.focus.node, true));
});
onButton(deleteRowBtn, () => {
  if (selection) runEdit(() => doc.deleteRow(selection.focus.node));
});
onButton(insertColumnBtn, () => {
  if (selection) runEdit(() => doc.insertColumn(selection.focus.node, true));
});
onButton(deleteColumnBtn, () => {
  if (selection) runEdit(() => doc.deleteColumn(selection.focus.node));
});

fontSizeSel.addEventListener("change", () => {
  const pt = Number(fontSizeSel.value);
  if (pt) runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, pt));
  fontSizeSel.value = "";
});
lineSpacingSel.addEventListener("change", () => {
  const percent = Number(lineSpacingSel.value);
  if (percent) runToolbarEdit((a, b, c, d) => doc.setLineSpacing(a, b, c, d, percent));
  lineSpacingSel.value = "";
});
highlightSel.addEventListener("change", () => {
  const name = highlightSel.value;
  runToolbarEdit((a, b, c, d) => doc.setHighlight(a, b, c, d, name));
  highlightSel.value = "none";
});
textColorInput.addEventListener("input", () => {
  const [r, g, b] = hexToRgb(textColorInput.value);
  runToolbarEdit((a, bo, c, d) => doc.setTextColor(a, bo, c, d, r, g, b));
});
fontFamilySel.addEventListener("change", () => {
  const family = fontFamilySel.value;
  if (family) runToolbarEdit((a, b, c, d) => doc.setFont(a, b, c, d, family));
  fontFamilySel.value = "";
});
paragraphStyleSel.addEventListener("change", () => {
  const name = paragraphStyleSel.value;
  runToolbarEdit((a, b, c, d) => doc.setParagraphStyle(a, b, c, d, name));
});

/** Serializes the edited document and downloads it as a .docx (user-initiated). */
function saveDocx() {
  if (!doc) return;
  try {
    const bytes = doc.exportDocx();
    const blob = new Blob([bytes], {
      type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = currentName.toLowerCase().endsWith(".docx") ? currentName : `${currentName}.docx`;
    a.click();
    URL.revokeObjectURL(url);
    setStatus(`Saved ${a.download}`);
  } catch (err) {
    console.error(err);
    setStatus(`Save failed: ${err?.message ?? err}`, "error");
  }
}
saveBtn.addEventListener("click", saveDocx);

/** The engine move direction for a navigation key, factoring in ⌘ (line/doc) and
 *  ⌥ (word) modifiers, or null if the key is not a navigation key. */
function navDirection(e) {
  const mod = e.metaKey || e.ctrlKey;
  switch (e.key) {
    case "ArrowLeft":
      return e.altKey ? "wordLeft" : mod ? "lineStart" : "left";
    case "ArrowRight":
      return e.altKey ? "wordRight" : mod ? "lineEnd" : "right";
    case "ArrowUp":
      return mod ? "docStart" : "up";
    case "ArrowDown":
      return mod ? "docEnd" : "down";
    case "Home":
      return "lineStart";
    case "End":
      return "lineEnd";
    default:
      return null;
  }
}

/** Moves the caret to an engine Caret result (⌘↑/↓, doc bounds). Shift extends. */
function navToPosition(caret, extend) {
  pendingFormat = null; // caret moved → disarm typing format
  const to = { node: caret.node, offset: caret.offset };
  caret.free();
  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  scrollCaretIntoView();
}

/** Selects the whole document (⌘A). */
function selectAll() {
  if (!doc) return;
  const a = doc.firstPosition();
  const b = doc.lastPosition();
  selection = {
    anchor: { node: a.node, offset: a.offset },
    focus: { node: b.node, offset: b.offset },
  };
  a.free();
  b.free();
  drawSelection();
}

/** Cut (⌘X): copy the selection to the clipboard, then delete it. */
async function cut() {
  if (!hasRange()) return;
  await copySelection();
  const { anchor, focus } = selection;
  await runEdit(() => doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset));
}

/** Paste (⌘V): insert clipboard text at the caret, replacing any selection and
 *  turning newlines into paragraph splits. */
async function paste() {
  if (!doc || !selection) return;
  let text;
  try {
    text = await navigator.clipboard.readText();
  } catch (err) {
    console.warn("paste failed:", err);
    return;
  }
  if (!text) return;
  if (hasRange()) {
    const { anchor, focus } = selection;
    await runEdit(() => doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset));
  }
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  for (let i = 0; i < lines.length; i++) {
    if (i > 0) await runEdit(() => doc.splitParagraph(selection.focus.node, selection.focus.offset));
    if (lines[i]) await runEdit(() => doc.insertText(selection.focus.node, selection.focus.offset, lines[i]));
  }
}

const FORMAT_KEYS = { b: "bold", i: "italic", u: "underline" };

document.addEventListener("keydown", async (e) => {
  if (!doc) return;
  // Don't hijack keys aimed at the chrome (file picker, zoom select).
  const tag = e.target?.tagName;
  if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;

  const mod = e.metaKey || e.ctrlKey;
  const key = e.key;
  const lower = key.toLowerCase();

  // Clipboard, select-all, history (⌘/Ctrl based).
  if (mod && lower === "c") {
    copySelection();
    return;
  }
  if (mod && lower === "x") {
    e.preventDefault();
    await cut();
    return;
  }
  if (mod && lower === "v") {
    e.preventDefault();
    await paste();
    return;
  }
  if (mod && lower === "a") {
    e.preventDefault();
    selectAll();
    return;
  }
  if (mod && lower === "z") {
    e.preventDefault();
    await runEdit(() => (e.shiftKey ? doc.redo() : doc.undo()));
    return;
  }
  if (mod && lower === "y") {
    e.preventDefault();
    await runEdit(() => doc.redo());
    return;
  }
  if (mod && FORMAT_KEYS[lower]) {
    e.preventDefault();
    toggleFormat(FORMAT_KEYS[lower]);
    return;
  }

  if (!selection) return;

  // Navigation (arrows + Home/End) with ⌘ (line/doc) and ⌥ (word) granularity —
  // handled before the generic `if (mod) return` so ⌘/⌥ + arrows work. Shift
  // extends the selection.
  const navDir = navDirection(e);
  if (navDir === "docStart" || navDir === "docEnd") {
    e.preventDefault();
    navToPosition(navDir === "docStart" ? doc.firstPosition() : doc.lastPosition(), e.shiftKey);
    return;
  }
  if (navDir) {
    e.preventDefault();
    navCaret(navDir, e.shiftKey);
    return;
  }

  // Tab / Shift+Tab indent / outdent the paragraph(s) the selection touches — the
  // word-processor convention (and how lists are demoted/promoted). Caught before
  // `if (mod) return` is irrelevant (Tab carries no ⌘), but before the browser can
  // move focus off the page.
  if (key === "Tab") {
    e.preventDefault();
    pendingFormat = null;
    await runToolbarEdit((a, b, c, d) => doc.adjustIndent(a, b, c, d, e.shiftKey ? -360 : 360));
    return;
  }

  if (mod) return; // leave other ⌘ shortcuts to the browser

  const { anchor, focus } = selection;
  const range = hasRange();

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
    if (range) {
      pendingFormat = null; // typing over a selection uses the selection's own runs
      await runEdit(() =>
        doc.replaceSelection(anchor.node, anchor.offset, focus.node, focus.offset, key),
      );
    } else if (pendingFormat) {
      const pf = pendingFormat; // armed format persists across consecutive typing
      await runEdit(() =>
        doc.insertStyledText(focus.node, focus.offset, key, pf.bold, pf.italic, pf.underline, pf.strike),
      );
    } else {
      await runEdit(() => doc.insertText(focus.node, focus.offset, key));
    }
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
function stepZoom(dir) {
  const i = zoomEl.selectedIndex + dir;
  if (i >= 0 && i < zoomEl.options.length) {
    zoomEl.selectedIndex = i;
    renderAll();
  }
}
zoomInBtn.addEventListener("click", () => stepZoom(1));
zoomOutBtn.addEventListener("click", () => stepZoom(-1));

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

// ---- Settings: theme + accent, persisted (OSS-customizable) ------------------
const settingsBtn = document.getElementById("settingsBtn");
const settingsPanel = document.getElementById("settingsPanel");
const themeSeg = document.getElementById("themeSeg");
const accentSwatches = document.getElementById("accentSwatches");
const accentCustom = document.getElementById("accentCustom");
const settingsReset = document.getElementById("settingsReset");

const DEFAULT_SETTINGS = { theme: "system", accent: "#e2622a" };
let settings = loadSettings();

function loadSettings() {
  try {
    return { ...DEFAULT_SETTINGS, ...JSON.parse(localStorage.getItem("opendoc.settings") || "{}") };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function saveSettings() {
  try {
    localStorage.setItem("opendoc.settings", JSON.stringify(settings));
  } catch {
    /* storage disabled — settings apply for the session only */
  }
}

/** Applies the current settings to the document root + reflects them in the panel. */
function applySettings() {
  const root = document.documentElement;
  if (settings.theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", settings.theme);
  root.style.setProperty("--accent", settings.accent);

  for (const b of themeSeg.querySelectorAll("button")) {
    b.setAttribute("aria-pressed", String(b.dataset.theme === settings.theme));
  }
  for (const b of accentSwatches.querySelectorAll(".acc[data-accent]")) {
    b.setAttribute(
      "aria-pressed",
      String(b.dataset.accent.toLowerCase() === settings.accent.toLowerCase()),
    );
  }
  accentCustom.value = settings.accent;
}

themeSeg.addEventListener("click", (e) => {
  const b = e.target.closest("button[data-theme]");
  if (!b) return;
  settings.theme = b.dataset.theme;
  saveSettings();
  applySettings();
});
accentSwatches.addEventListener("click", (e) => {
  const b = e.target.closest(".acc[data-accent]");
  if (!b) return;
  settings.accent = b.dataset.accent;
  saveSettings();
  applySettings();
});
accentCustom.addEventListener("input", () => {
  settings.accent = accentCustom.value;
  saveSettings();
  applySettings();
});
settingsReset.addEventListener("click", () => {
  settings = { ...DEFAULT_SETTINGS };
  saveSettings();
  applySettings();
});

function toggleSettings(open) {
  const show = open ?? settingsPanel.hidden;
  settingsPanel.hidden = !show;
  settingsBtn.setAttribute("aria-expanded", String(show));
}
settingsBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  toggleSettings();
});
document.addEventListener("click", (e) => {
  if (!settingsPanel.hidden && !settingsPanel.contains(e.target) && e.target !== settingsBtn) {
    toggleSettings(false);
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !settingsPanel.hidden) toggleSettings(false);
});
applySettings();

fileEl.disabled = true;
updateToolbar(); // start with the toolbar controls disabled (no selection yet)
boot();
