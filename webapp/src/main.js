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
const spacingBtn = document.getElementById("spacingBtn");
const spacingMenu = document.getElementById("spacingMenu");
const spaceBeforeInput = document.getElementById("spaceBefore");
const spaceAfterInput = document.getElementById("spaceAfter");
const paraOptsBtn = document.getElementById("paraOptsBtn");
const paraOptsMenu = document.getElementById("paraOptsMenu");
const paraShade = document.getElementById("paraShade");
const paraShadeNone = document.getElementById("paraShadeNone");
const pgKeepNext = document.getElementById("pgKeepNext");
const pgKeepLines = document.getElementById("pgKeepLines");
const pgBreakBefore = document.getElementById("pgBreakBefore");
const indentLeftInput = document.getElementById("indentLeft");
const indentRightInput = document.getElementById("indentRight");
const indentSpecialSel = document.getElementById("indentSpecial");
const indentSpecialByInput = document.getElementById("indentSpecialBy");
const borderColorInput = document.getElementById("borderColor");
const tableBtn = document.getElementById("tableBtn");
const tableFmtMenu = document.getElementById("tableMenu");
const cellShade = document.getElementById("cellShade");
const cellShadeNone = document.getElementById("cellShadeNone");
const cellVAlign = document.getElementById("cellVAlign");
const cellBorderColor = document.getElementById("cellBorderColor");
const tableBorderColor = document.getElementById("tableBorderColor");
const tableAlign = document.getElementById("tableAlign");
const insertTableBtn = document.getElementById("insertTableBtn");
const insertTableMenu = document.getElementById("insertTableMenu");
const gridPicker = document.getElementById("gridPicker");
const gridLabel = document.getElementById("gridLabel");
const ribbonTabs = [...document.querySelectorAll(".ribbon-tab")];
const ribbonPanels = [...document.querySelectorAll(".ribbon-panel")];
const tabTable = document.getElementById("tabTable");
const undoBtn = document.getElementById("undoBtn");
const redoBtn = document.getElementById("redoBtn");
const viewOutlineBtn = document.getElementById("viewOutlineBtn");
const viewZoomOut = document.getElementById("viewZoomOut");
const viewZoomIn = document.getElementById("viewZoomIn");

/** Shows the named ribbon tab's panel and marks its tab selected. */
function selectRibbonTab(name) {
  for (const t of ribbonTabs) t.setAttribute("aria-selected", String(t.dataset.tab === name));
  for (const p of ribbonPanels) p.hidden = p.dataset.panel !== name;
}
for (const t of ribbonTabs) {
  t.addEventListener("click", () => {
    if (!t.disabled) selectRibbonTab(t.dataset.tab);
  });
}
undoBtn.addEventListener("click", () => runEdit(() => doc.undo()));
redoBtn.addEventListener("click", () => runEdit(() => doc.redo()));
viewOutlineBtn.addEventListener("click", () => toggleOutline());
viewZoomOut.addEventListener("click", () => stepZoom(-1));
viewZoomIn.addEventListener("click", () => stepZoom(1));
const railOutline = document.getElementById("railOutline");
const outlinePanel = document.getElementById("outlinePanel");
const outlineClose = document.getElementById("outlineClose");
const outlineBody = document.getElementById("outlineBody");
const indentDecBtn = document.getElementById("indentDec");
const indentIncBtn = document.getElementById("indentInc");
const bulletListBtn = document.getElementById("bulletList");
const numberedListBtn = document.getElementById("numberedList");
const fontFamilySel = document.getElementById("fontFamily");
const paragraphStyleSel = document.getElementById("paragraphStyle");
const runControls = [superBtn, subBtn, fontSizeSel, textColorInput, highlightSel, fontFamilySel];
const paraControls = [
  ...Object.values(alignBtns),
  spacingBtn,
  paraOptsBtn,
  indentDecBtn,
  indentIncBtn,
  bulletListBtn,
  numberedListBtn,
  paragraphStyleSel,
];
const saveBtn = document.getElementById("save");
const zoomInBtn = document.getElementById("zoomIn");
const zoomOutBtn = document.getElementById("zoomOut");
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
    railOutline.disabled = false;
    populateStyles();
    dropEl.hidden = true;
    await provisionFonts(name);
    await renderAll();
    buildOutline();
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

  pagesEl.prepend(ruler); // the ruler sits above the pages, same width
  buildRuler();
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
  updateRulerMarkers();
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
  // Index among page wraps only — `pagesEl` also holds the ruler as a child, so
  // indexing over all children would be off by the ruler's slot (dead clicks).
  const idx = [...pagesEl.querySelectorAll(".page-wrap")].indexOf(wrap);
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

// ---- Right-click table menu (Google-Docs style: structure lives here, not the
//      toolbar) -----------------------------------------------------------------
const tableMenu = document.createElement("div");
tableMenu.className = "context-menu";
tableMenu.hidden = true;
document.body.appendChild(tableMenu);

const TABLE_MENU_ITEMS = [
  { label: "Insert row above", run: (n) => doc.insertRow(n, false) },
  { label: "Insert row below", run: (n) => doc.insertRow(n, true) },
  { label: "Insert column left", run: (n) => doc.insertColumn(n, false) },
  { label: "Insert column right", run: (n) => doc.insertColumn(n, true) },
  { divider: true },
  { label: "Delete row", run: (n) => doc.deleteRow(n), danger: true },
  { label: "Delete column", run: (n) => doc.deleteColumn(n), danger: true },
  { label: "Delete table", run: (n) => doc.deleteTable(n), danger: true },
];

function showTableMenu(clientX, clientY, node) {
  tableMenu.replaceChildren();
  for (const item of TABLE_MENU_ITEMS) {
    if (item.divider) {
      const hr = document.createElement("div");
      hr.className = "menu-divider";
      tableMenu.appendChild(hr);
      continue;
    }
    const b = document.createElement("button");
    b.type = "button";
    b.className = `menu-item${item.danger ? " danger" : ""}`;
    b.textContent = item.label;
    b.addEventListener("click", () => {
      hideTableMenu();
      runEdit(() => item.run(node));
    });
    tableMenu.appendChild(b);
  }
  tableMenu.hidden = false;
  // Clamp the menu into the viewport near the cursor.
  const w = tableMenu.offsetWidth;
  const h = tableMenu.offsetHeight;
  tableMenu.style.left = `${Math.max(8, Math.min(clientX, window.innerWidth - w - 8))}px`;
  tableMenu.style.top = `${Math.max(8, Math.min(clientY, window.innerHeight - h - 8))}px`;
}

function hideTableMenu() {
  tableMenu.hidden = true;
}

pagesEl.addEventListener("contextmenu", (e) => {
  const page = pageFromEvent(e);
  if (!page || !doc) return;
  const anchor = anchorAt(page, e);
  if (!anchor || !doc.inTable(anchor.node)) return; // not in a table → native menu
  e.preventDefault();
  selection = { anchor, focus: anchor }; // place the caret in the right-clicked cell
  drawSelection();
  showTableMenu(e.clientX, e.clientY, anchor.node);
});

document.addEventListener("pointerdown", (e) => {
  if (!tableMenu.hidden && !tableMenu.contains(e.target)) hideTableMenu();
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") hideTableMenu();
});
viewportEl.addEventListener("scroll", hideTableMenu, { passive: true });
window.addEventListener("resize", hideTableMenu);

// ---- Horizontal ruler (margins + the caret paragraph's indent markers) -------
const ruler = document.createElement("div");
ruler.className = "ruler";
ruler.hidden = true;
const rulerTrack = document.createElement("div");
rulerTrack.className = "ruler-track";
ruler.appendChild(rulerTrack);

let rulerGeom = null; // { widthTwip, marginStartTwip, marginEndTwip }
let rulerScale = 0; // px per twip at the current zoom
const markers = {}; // key -> element
const TWIPS_PER_INCH = 1440;
let tabInsertCode = 0; // the type new ruler tabs get: 0 L, 1 C, 2 R, 3 decimal
const TAB_LETTER = ["L", "C", "R", "."];

/** Rebuilds the ruler scale, margin zones, and ticks for the current page/zoom. */
function buildRuler() {
  if (!doc || !pages.length) {
    ruler.hidden = true;
    return;
  }
  const g = doc.pageGeometry();
  rulerGeom = {
    width: g.widthTwip,
    marginStart: g.marginStartTwip,
    marginEnd: g.marginEndTwip,
  };
  const pageWidthPx = pages[0].canvas.getBoundingClientRect().width;
  rulerScale = pageWidthPx / rulerGeom.width;
  ruler.style.width = `${pageWidthPx}px`;
  const px = (t) => t * rulerScale;

  rulerTrack.replaceChildren();
  const contentStart = rulerGeom.marginStart;

  // The white content span between the (shaded) page margins. Clicking it adds a
  // tab stop at that position (in the current tab type) on the caret paragraph.
  const content = document.createElement("div");
  content.className = "ruler-content";
  content.style.left = `${px(rulerGeom.marginStart)}px`;
  content.style.width = `${px(rulerGeom.width - rulerGeom.marginStart - rulerGeom.marginEnd)}px`;
  content.addEventListener("pointerdown", (e) => {
    if (!doc || !selection || e.button !== 0) return;
    const pos = Math.max(0, Math.round(e.offsetX / rulerScale));
    e.preventDefault();
    e.stopPropagation();
    runToolbarEdit((a, b, c, d) => doc.setTabStop(a, b, c, d, pos, tabInsertCode));
    updateRulerMarkers();
  });
  rulerTrack.appendChild(content);

  // Word-style tab-type selector at the ruler's left edge; click to cycle L/C/R/dot.
  const corner = document.createElement("button");
  corner.type = "button";
  corner.className = "tab-corner";
  corner.title = "Tab stop type — click to change";
  corner.textContent = TAB_LETTER[tabInsertCode];
  corner.addEventListener("click", () => {
    tabInsertCode = (tabInsertCode + 1) % TAB_LETTER.length;
    corner.textContent = TAB_LETTER[tabInsertCode];
  });
  rulerTrack.appendChild(corner);

  // Minor ticks every 1/8", plus a numbered major tick at each inch measured from
  // the left margin (0 at the content edge).
  for (let t = 0; t <= rulerGeom.width; t += TWIPS_PER_INCH / 8) {
    const tick = document.createElement("div");
    tick.className = "ruler-tick minor";
    tick.style.left = `${px(t)}px`;
    rulerTrack.appendChild(tick);
  }
  for (let i = 0, t = contentStart; t <= rulerGeom.width + 1; i++, t = contentStart + i * TWIPS_PER_INCH) {
    const tick = document.createElement("div");
    tick.className = "ruler-tick major";
    tick.style.left = `${px(t)}px`;
    rulerTrack.appendChild(tick);
    if (i > 0) {
      const num = document.createElement("div");
      num.className = "ruler-num";
      num.textContent = String(i);
      num.style.left = `${px(t)}px`;
      rulerTrack.appendChild(num);
    }
  }

  // Indent markers (recreated each build; positioned by the selection). Only the
  // markers are pointer-interactive; the rest of the ruler is click-through, so a
  // marker drag can never steal a page click.
  for (const [key, cls] of [
    ["firstLine", "down"],
    ["left", "up"],
    ["right", "up"],
  ]) {
    const m = document.createElement("div");
    m.className = `ruler-marker ${cls}`;
    m.dataset.marker = key;
    m.title =
      key === "firstLine" ? "First-line indent" : key === "left" ? "Left indent" : "Right indent";
    m.addEventListener("pointerdown", (e) => startMarkerDrag(key, e));
    rulerTrack.appendChild(m);
    markers[key] = m;
  }

  ruler.hidden = false;
  updateRulerMarkers();
}

/** Positions the three indent markers from the caret paragraph's indentation. */
function updateRulerMarkers() {
  if (!rulerGeom || !markers.left) return;
  const px = (t) => t * rulerScale;
  let start = 0;
  let end = 0;
  let firstLine = 0;
  if (doc && selection) {
    const ind = doc.paragraphIndent(selection.focus.node);
    start = ind.startTwip;
    end = ind.endTwip;
    firstLine = ind.firstLineTwip - ind.hangingTwip;
    ind.free();
  }
  const contentStart = rulerGeom.marginStart;
  const contentEnd = rulerGeom.width - rulerGeom.marginEnd;
  markers.left.style.left = `${px(contentStart + start)}px`;
  markers.firstLine.style.left = `${px(contentStart + start + firstLine)}px`;
  markers.right.style.left = `${px(contentEnd - end)}px`;
  renderTabStops();
}

/** Draws the caret paragraph's tab stops as glyphs on the ruler (recreated each
 *  update). Each glyph: click cycles its type, drag moves it, drag off removes it. */
function renderTabStops() {
  for (const g of rulerTrack.querySelectorAll(".tab-glyph")) g.remove();
  if (!doc || !selection || !rulerGeom) return;
  const px = (t) => t * rulerScale;
  const tabs = doc.paragraphTabs(selection.focus.node); // flat [pos, code, …]
  for (let k = 0; k < tabs.length; k += 2) {
    const pos = tabs[k];
    const code = tabs[k + 1];
    const g = document.createElement("div");
    g.className = `tab-glyph tab-${code}`;
    g.textContent = TAB_LETTER[code] ?? "L";
    g.style.left = `${px(rulerGeom.marginStart + pos)}px`;
    g.title = "Tab stop — click to change type, drag to move, drag off to remove";
    g.addEventListener("pointerdown", (e) => startTabDrag(pos, code, g, e));
    rulerTrack.appendChild(g);
  }
}

/** A tab-glyph pointer interaction: no move → cycle type; horizontal move →
 *  reposition; released off the ruler → delete (Word's drag-off-to-remove). */
function startTabDrag(pos, code, glyph, ev) {
  if (!doc || !selection || ev.button !== 0) return;
  ev.preventDefault();
  ev.stopPropagation();
  const trackRect = rulerTrack.getBoundingClientRect();
  const px = (t) => t * rulerScale;
  let moved = false;
  let curPos = pos;
  const onMove = (e) => {
    if (Math.abs(e.clientX - ev.clientX) > 3 || Math.abs(e.clientY - ev.clientY) > 3) moved = true;
    curPos = Math.max(0, Math.round((e.clientX - trackRect.left) / rulerScale - rulerGeom.marginStart));
    glyph.style.left = `${px(rulerGeom.marginStart + curPos)}px`; // live
  };
  const onUp = (e) => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    const offRuler = e.clientY > trackRect.bottom + 14 || e.clientY < trackRect.top - 14;
    if (offRuler) {
      runToolbarEdit((a, b, c, d) => doc.removeTabStop(a, b, c, d, pos));
    } else if (moved && curPos !== pos) {
      runToolbarEdit((a, b, c, d) => doc.moveTabStop(a, b, c, d, pos, curPos));
    } else {
      runToolbarEdit((a, b, c, d) => doc.setTabStop(a, b, c, d, pos, (code + 1) % TAB_LETTER.length));
    }
    updateRulerMarkers();
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

/** Drag an indent marker. Uses window-level move/up listeners (never
 *  setPointerCapture) so the pointer is always released — the ruler acts on the
 *  caret paragraph, so a selection is required. */
function startMarkerDrag(key, ev) {
  if (!doc || !selection || !rulerGeom) return;
  ev.preventDefault();
  ev.stopPropagation(); // don't let the pointerdown fall through to the page

  const trackRect = rulerTrack.getBoundingClientRect();
  const px = (t) => t * rulerScale;
  const contentStart = rulerGeom.marginStart;
  const contentEnd = rulerGeom.width - rulerGeom.marginEnd;

  // The left marker carries the first-line marker with it (Word/Docs behaviour);
  // capture the current first-line offset so it is preserved during the drag.
  const ind = doc.paragraphIndent(selection.focus.node);
  const startTwip = ind.startTwip;
  const firstLineOff = ind.firstLineTwip - ind.hangingTwip;
  ind.free();

  const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);
  const xTwipAt = (clientX) => clamp((clientX - trackRect.left) / rulerScale, 0, rulerGeom.width);

  // Live visual feedback while dragging (model is committed on pointerup).
  const preview = (x) => {
    if (key === "left") {
      markers.left.style.left = `${px(x)}px`;
      markers.firstLine.style.left = `${px(x + firstLineOff)}px`;
    } else {
      markers[key].style.left = `${px(x)}px`;
    }
  };

  // Resolve the marker's ruler x to an absolute indent for the WASM setter.
  const commit = async (x) => {
    let call;
    if (key === "left") {
      const twips = Math.round(x - contentStart);
      call = (sn, so, en, eo) => doc.setLeftIndent(sn, so, en, eo, twips);
    } else if (key === "firstLine") {
      const twips = Math.round(x - contentStart - startTwip);
      call = (sn, so, en, eo) => doc.setFirstLineIndent(sn, so, en, eo, twips);
    } else {
      const twips = Math.round(contentEnd - x);
      call = (sn, so, en, eo) => doc.setRightIndent(sn, so, en, eo, twips);
    }
    await runToolbarEdit(call);
    updateRulerMarkers(); // snap to the model's clamped truth
  };

  markers[key].classList.add("dragging");
  const onMove = (e) => preview(xTwipAt(e.clientX));
  const onUp = (e) => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    markers[key].classList.remove("dragging");
    commit(xTwipAt(e.clientX));
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

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
  buildOutline(); // headings may have changed (no-op when the panel is closed)
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
  buildOutline(); // a style change may add/remove a heading (no-op when closed)
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
  // Run controls work with a range (apply) or a caret (arm for typing), so they are
  // enabled whenever there is a selection.
  for (const el of runControls) el.disabled = !hasSel;
  for (const el of paraControls) el.disabled = !hasSel;

  const align = hasSel && doc ? doc.alignmentAt(selection.focus.node, selection.focus.offset) : "start";
  for (const [key, btn] of Object.entries(alignBtns)) {
    btn.setAttribute("aria-pressed", String(key === align));
  }

  // Reflect the current run styling (size / font / color / super-sub) — over a
  // selection, or what a collapsed caret inherits (so it's "picked up" on click,
  // not only when text is selected).
  let size = "";
  let font = "";
  let sup = false;
  let sub = false;
  if (doc && hasSel) {
    const rs = range
      ? doc.selectionRunStyle(
          selection.anchor.node,
          selection.anchor.offset,
          selection.focus.node,
          selection.focus.offset,
        )
      : doc.caretRunStyle(selection.focus.node, selection.focus.offset);
    if (rs.sizePoints) size = String(rs.sizePoints);
    font = rs.font;
    if (rs.color) textColorInput.value = rs.color;
    sup = rs.superscript;
    sub = rs.subscript;
    rs.free();
  }
  // An armed (pending) run format overrides the inherited value in the display.
  if (pendingFormat) {
    if (pendingFormat.sizeHalfPoints != null) size = String(pendingFormat.sizeHalfPoints / 2);
    if (pendingFormat.font != null) font = pendingFormat.font;
    if (pendingFormat.color) textColorInput.value = pendingFormat.color;
    if (pendingFormat.vertAlign != null) {
      sup = pendingFormat.vertAlign === "super";
      sub = pendingFormat.vertAlign === "sub";
    }
  }
  fontSizeSel.value = size;
  fontFamilySel.value = font;
  superBtn.setAttribute("aria-pressed", String(sup));
  subBtn.setAttribute("aria-pressed", String(sub));

  // Reflect the current paragraph style + spacing + list kind.
  paragraphStyleSel.value = hasSel && doc ? doc.paragraphStyleAt(selection.focus.node) : "";
  if (hasSel && doc) for (const p of popovers) if (!p.menu.hidden) p.reflect();
  const listKind = hasSel && doc ? doc.listStyleAt(selection.focus.node) : "";
  bulletListBtn.setAttribute("aria-pressed", String(listKind === "bullet"));
  numberedListBtn.setAttribute("aria-pressed", String(listKind === "numbered"));
  // The Table button (cell/table formatting) is enabled only inside a table;
  // Insert-table needs just a caret to drop the new table after.
  const inTable = hasSel && doc && doc.inTable(selection.focus.node);
  tableBtn.disabled = !inTable;
  insertTableBtn.disabled = !(hasSel && doc);
  // Ribbon: undo/redo/view controls need a document; the Table tab is contextual.
  undoBtn.disabled = !doc;
  redoBtn.disabled = !doc;
  viewOutlineBtn.disabled = !doc;
  viewOutlineBtn.setAttribute("aria-pressed", String(!outlinePanel.hidden));
  viewZoomOut.disabled = !doc;
  viewZoomIn.disabled = !doc;
  tabTable.disabled = !inTable;
  if (tabTable.disabled && tabTable.getAttribute("aria-selected") === "true") {
    selectRibbonTab("home");
  }
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
/** A run-format control: apply to a range, or arm into `pendingFormat` at a caret
 *  (so the next typed text carries it — same model as the B/I/U/S toggles). */
function armOrApplyRun(patch, applyFn) {
  if (hasRange()) {
    applyFn();
  } else if (selection) {
    pendingFormat = { ...(pendingFormat || {}), ...patch };
    updateToolbar();
  }
}
onButton(superBtn, () =>
  armOrApplyRun({ vertAlign: "super" }, () =>
    runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, "super")),
  ),
);
onButton(subBtn, () =>
  armOrApplyRun({ vertAlign: "sub" }, () =>
    runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, "sub")),
  ),
);
for (const [key, btn] of Object.entries(alignBtns)) {
  onButton(btn, () => runToolbarEdit((a, b, c, d) => doc.setAlignment(a, b, c, d, key)));
}
onButton(indentDecBtn, () => runToolbarEdit((a, b, c, d) => doc.adjustIndent(a, b, c, d, -360)));
onButton(indentIncBtn, () => runToolbarEdit((a, b, c, d) => doc.adjustIndent(a, b, c, d, 360)));
onButton(bulletListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "bullet")));
onButton(numberedListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "numbered")));

fontSizeSel.addEventListener("change", () => {
  const pt = Number(fontSizeSel.value);
  if (pt) {
    armOrApplyRun({ sizeHalfPoints: pt * 2 }, () =>
      runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, pt)),
    );
  }
});
// ---- Toolbar popovers (spacing, paragraph options) -------------------------
// One lightweight manager: anchor a menu under its button, only one open at a
// time, dismiss on outside-pointerdown / Escape. Each popover registers a
// `reflect()` that syncs its controls to the caret paragraph.
const TWIPS_PER_POINT = 20;
const popovers = [];

function openPopover(p) {
  if (!selection) return;
  for (const q of popovers) if (q !== p) closePopover(q);
  const r = p.btn.getBoundingClientRect();
  p.menu.hidden = false;
  p.menu.style.left = `${Math.round(r.left)}px`;
  p.menu.style.top = `${Math.round(r.bottom + 4)}px`;
  p.btn.setAttribute("aria-expanded", "true");
  p.reflect();
}

function closePopover(p) {
  p.menu.hidden = true;
  p.btn.setAttribute("aria-expanded", "false");
}

function registerPopover(btn, menu, reflect) {
  const p = { btn, menu, reflect };
  popovers.push(p);
  onButton(btn, () => (menu.hidden ? openPopover(p) : closePopover(p)));
  // Keep clicks inside the menu from stealing the selection focus, but let form
  // controls (inputs, selects) focus, toggle, and open normally.
  menu.addEventListener("mousedown", (e) => {
    if (!["INPUT", "SELECT", "OPTION"].includes(e.target.tagName)) e.preventDefault();
  });
  return p;
}

document.addEventListener("mousedown", (e) => {
  for (const p of popovers) {
    if (
      !p.menu.hidden &&
      !p.menu.contains(e.target) &&
      e.target !== p.btn &&
      !p.btn.contains(e.target)
    ) {
      closePopover(p);
    }
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") for (const p of popovers) if (!p.menu.hidden) closePopover(p);
});

// -- Line & paragraph spacing --------------------------------------------------
/** Reflect the caret paragraph's spacing into the menu (line-preset check +
 *  space before/after fields). */
function reflectSpacingMenu() {
  if (!doc || !selection) return;
  const s = doc.paragraphSpacing(selection.focus.node);
  const percent = s.lineRule === 0 ? s.linePercent : 0; // presets are `auto` multiples
  for (const b of spacingMenu.querySelectorAll(".spacing-line")) {
    b.setAttribute("aria-checked", String(Number(b.dataset.percent) === percent));
  }
  // Don't overwrite a field the user is mid-edit in.
  if (document.activeElement !== spaceBeforeInput) {
    spaceBeforeInput.value = s.beforeTwip >= 0 ? String(Math.round(s.beforeTwip / TWIPS_PER_POINT)) : "";
  }
  if (document.activeElement !== spaceAfterInput) {
    spaceAfterInput.value = s.afterTwip >= 0 ? String(Math.round(s.afterTwip / TWIPS_PER_POINT)) : "";
  }
}
registerPopover(spacingBtn, spacingMenu, reflectSpacingMenu);

for (const b of spacingMenu.querySelectorAll(".spacing-line")) {
  onButton(b, () => {
    runToolbarEdit((a, x, c, d) => doc.setLineSpacing(a, x, c, d, Number(b.dataset.percent)));
    reflectSpacingMenu();
  });
}

/** Commit a space-before/after field: blank clears (back to style default),
 *  otherwise points → twips (clamped ≥ 0). Ignores non-numeric input. */
function applySpace(input, setter) {
  const raw = input.value.trim();
  if (raw !== "" && !Number.isFinite(Number(raw))) return;
  const twips = raw === "" ? -1 : Math.max(0, Math.round(Number(raw) * TWIPS_PER_POINT));
  runToolbarEdit((a, x, c, d) => setter(a, x, c, d, twips));
}
spaceBeforeInput.addEventListener("change", () =>
  applySpace(spaceBeforeInput, (a, b, c, d, t) => doc.setSpaceBefore(a, b, c, d, t)),
);
spaceAfterInput.addEventListener("change", () =>
  applySpace(spaceAfterInput, (a, b, c, d, t) => doc.setSpaceAfter(a, b, c, d, t)),
);

// -- Paragraph options (indentation + shading + line/page-break flags) --------
/** Twips → inches string, trimming trailing zeros; "" for zero. */
function inchStr(twip) {
  if (!twip) return "";
  return (twip / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "");
}
/** An inches field's value → twips (≥ 0); "" or non-numeric → 0. */
function inchTwips(input) {
  const raw = input.value.trim();
  if (raw === "" || !Number.isFinite(Number(raw))) return 0;
  return Math.max(0, Math.round(Number(raw) * TWIPS_PER_INCH));
}

function reflectParaOptsMenu() {
  if (!doc || !selection) return;
  const node = selection.focus.node;
  // Indentation (inches).
  const ind = doc.paragraphIndent(node);
  const editingIndent = [indentLeftInput, indentRightInput, indentSpecialByInput, indentSpecialSel].includes(
    document.activeElement,
  );
  if (!editingIndent) {
    indentLeftInput.value = inchStr(ind.startTwip);
    indentRightInput.value = inchStr(ind.endTwip);
    if (ind.firstLineTwip > 0) {
      indentSpecialSel.value = "first";
      indentSpecialByInput.value = inchStr(ind.firstLineTwip);
    } else if (ind.hangingTwip > 0) {
      indentSpecialSel.value = "hanging";
      indentSpecialByInput.value = inchStr(ind.hangingTwip);
    } else {
      indentSpecialSel.value = "none";
    }
  }
  ind.free();
  // Line/page-break flags.
  const f = doc.paragraphFlags(node);
  pgKeepNext.checked = f.keepNext;
  pgKeepLines.checked = f.keepLines;
  pgBreakBefore.checked = f.pageBreakBefore;
  const rgb = doc.paragraphShadingAt(node);
  if (rgb >= 0 && document.activeElement !== paraShade) {
    paraShade.value = `#${rgb.toString(16).padStart(6, "0")}`;
  }
  // Borders: light the preset(s) matching the active edges (top=1,bottom=2,left=4,right=8).
  const edges = doc.paragraphBorderEdges(node);
  const bit = { top: 1, bottom: 2, left: 4, right: 8 };
  for (const b of paraOptsMenu.querySelectorAll(".border-btn")) {
    const k = b.dataset.border;
    const on = k === "box" ? edges === 0b1111 : k === "none" ? edges === 0 : (edges & bit[k]) !== 0;
    b.setAttribute("aria-pressed", String(on));
  }
}
registerPopover(paraOptsBtn, paraOptsMenu, reflectParaOptsMenu);

// Borders: presets toggle edges (box = all, none = clear) in the chosen color at a
// 1 pt single line (8 eighth-points).
for (const b of paraOptsMenu.querySelectorAll(".border-btn")) {
  onButton(b, () => {
    const [r, g, bl] = hexToRgb(borderColorInput.value);
    runToolbarEdit((a, x, c, d) => doc.setParagraphBorder(a, x, c, d, b.dataset.border, r, g, bl, 8));
    reflectParaOptsMenu();
  });
}

// -- Table & cell formatting (a single-node edit: applies to the caret's cell) --
/** Runs a `(node) => EditResult` edit on the caret's node, preserving the selection
 *  and repainting only the dirty pages (rebuild on a page-count change). */
function runNodeEdit(thunk) {
  if (!selection || !doc) return;
  let res;
  try {
    res = thunk(selection.focus.node);
  } catch (err) {
    console.warn("edit ignored:", err?.message ?? err);
    return;
  }
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  if (newCount !== pages.length) renderAll();
  else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
}

function reflectTableMenu() {
  if (!doc || !selection) return;
  const node = selection.focus.node;
  const rgb = doc.cellShadingAt(node);
  if (rgb >= 0 && document.activeElement !== cellShade) {
    cellShade.value = `#${rgb.toString(16).padStart(6, "0")}`;
  }
  const va = doc.cellVerticalAlignAt(node) || "top";
  for (const b of cellVAlign.querySelectorAll("button")) {
    b.setAttribute("aria-pressed", String(b.dataset.valign === va));
  }
  const edges = doc.cellBorderEdges(node);
  const bit = { top: 1, bottom: 2, left: 4, right: 8 };
  for (const b of tableFmtMenu.querySelectorAll(".border-btn")) {
    const k = b.dataset.cellborder;
    const on = k === "box" ? edges === 0b1111 : k === "none" ? edges === 0 : (edges & bit[k]) !== 0;
    b.setAttribute("aria-pressed", String(on));
  }
}
registerPopover(tableBtn, tableFmtMenu, reflectTableMenu);

cellShade.addEventListener("input", () => {
  const [r, g, b] = hexToRgb(cellShade.value);
  runNodeEdit((n) => doc.setCellShading(n, r, g, b, false));
});
onButton(cellShadeNone, () => runNodeEdit((n) => doc.setCellShading(n, 0, 0, 0, true)));
for (const b of cellVAlign.querySelectorAll("button")) {
  onButton(b, () => {
    runNodeEdit((n) => doc.setCellVerticalAlign(n, b.dataset.valign));
    reflectTableMenu();
  });
}
for (const b of tableFmtMenu.querySelectorAll(".border-btn")) {
  onButton(b, () => {
    const [r, g, bl] = hexToRgb(cellBorderColor.value);
    runNodeEdit((n) => doc.setCellBorder(n, b.dataset.cellborder, r, g, bl, 8));
    reflectTableMenu();
  });
}
for (const b of tableFmtMenu.querySelectorAll("[data-tableborder]")) {
  onButton(b, () => {
    const [r, g, bl] = hexToRgb(tableBorderColor.value);
    runNodeEdit((n) => doc.setTableBorder(n, b.dataset.tableborder, r, g, bl, 8));
  });
}
for (const b of tableAlign.querySelectorAll("button")) {
  onButton(b, () => runNodeEdit((n) => doc.setTableAlignment(n, b.dataset.talign)));
}

// -- Insert table: a hover grid picker (Google-Docs style) --------------------
const GRID_ROWS = 8;
const GRID_COLS = 10;
for (let r = 1; r <= GRID_ROWS; r++) {
  for (let c = 1; c <= GRID_COLS; c++) {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "gc";
    cell.dataset.r = String(r);
    cell.dataset.c = String(c);
    gridPicker.appendChild(cell);
  }
}
function highlightGrid(rows, cols) {
  for (const cell of gridPicker.children) {
    const on = Number(cell.dataset.r) <= rows && Number(cell.dataset.c) <= cols;
    cell.classList.toggle("on", on);
  }
  gridLabel.textContent = rows ? `${cols} × ${rows}` : "Insert table";
}
gridPicker.addEventListener("pointermove", (e) => {
  const cell = e.target.closest(".gc");
  if (cell) highlightGrid(Number(cell.dataset.r), Number(cell.dataset.c));
});
gridPicker.addEventListener("pointerleave", () => highlightGrid(0, 0));
gridPicker.addEventListener("pointerdown", (e) => {
  const cell = e.target.closest(".gc");
  if (!cell || !selection || !doc) return;
  e.preventDefault();
  const rows = Number(cell.dataset.r);
  const cols = Number(cell.dataset.c);
  runEdit(() => doc.insertTable(selection.focus.node, rows, cols));
  closePopover(insertTablePopover);
});
const insertTablePopover = registerPopover(insertTableBtn, insertTableMenu, () => highlightGrid(0, 0));

// ---- Outline panel (heading tree → scroll-to) -------------------------------
/** Rebuilds the outline list from the document's headings (no-op when hidden). */
function buildOutline() {
  if (!doc || outlinePanel.hidden) return;
  const rows = doc.documentOutline(); // "level\tnode\ttext"
  outlineBody.replaceChildren();
  if (!rows.length) {
    const empty = document.createElement("div");
    empty.className = "outline-empty";
    empty.textContent = "No headings yet. Apply a Heading style to build an outline.";
    outlineBody.appendChild(empty);
    return;
  }
  for (const row of rows) {
    const tab = row.indexOf("\t");
    const tab2 = row.indexOf("\t", tab + 1);
    const level = Math.min(6, Math.max(1, Number(row.slice(0, tab)) || 1));
    const node = row.slice(tab + 1, tab2);
    const text = row.slice(tab2 + 1);
    const item = document.createElement("button");
    item.type = "button";
    item.className = `outline-item lvl-${level}`;
    item.textContent = text;
    item.title = text;
    item.addEventListener("click", () => navigateToNode(node));
    outlineBody.appendChild(item);
  }
}

/** Places the caret at the start of `node` and scrolls it into view. */
function navigateToNode(node) {
  if (!doc) return;
  selection = { anchor: { node, offset: 0 }, focus: { node, offset: 0 } };
  drawSelection();
  scrollCaretIntoView();
}

function toggleOutline() {
  outlinePanel.hidden = !outlinePanel.hidden;
  railOutline.setAttribute("aria-pressed", String(!outlinePanel.hidden));
  buildOutline();
}
railOutline.addEventListener("click", toggleOutline);
outlineClose.addEventListener("click", toggleOutline);

// ---- Command palette (⌘K) — fuzzy search over real editor actions -----------
const cmdPalette = document.getElementById("cmdPalette");
const cmdInput = document.getElementById("cmdInput");
const cmdList = document.getElementById("cmdList");
let cmdMatches = [];
let cmdSel = 0;

/** The command set, rebuilt per open so dynamic entries (the document's styles)
 *  are current. Every command runs a real action; `noDoc` ones work with no doc. */
function buildCommands() {
  const fmt = (k) => () => toggleFormat(k);
  const align = (a) => () => runToolbarEdit((s, o, e, f) => doc.setAlignment(s, o, e, f, a));
  const cmds = [
    { label: "Open…", group: "File", kw: "load docx", noDoc: true, run: () => fileEl.click() },
    { label: "Save (download .docx)", group: "File", kw: "export download", run: () => saveDocx() },
    { label: "Undo", group: "Edit", kw: "revert", run: () => runEdit(() => doc.undo()) },
    { label: "Redo", group: "Edit", kw: "", run: () => runEdit(() => doc.redo()) },
    { label: "Bold", group: "Format", kw: "strong", run: fmt("bold") },
    { label: "Italic", group: "Format", kw: "emphasis", run: fmt("italic") },
    { label: "Underline", group: "Format", kw: "", run: fmt("underline") },
    { label: "Strikethrough", group: "Format", kw: "strike", run: fmt("strike") },
    { label: "Align left", group: "Paragraph", kw: "", run: align("start") },
    { label: "Align center", group: "Paragraph", kw: "centre", run: align("center") },
    { label: "Align right", group: "Paragraph", kw: "", run: align("end") },
    { label: "Justify", group: "Paragraph", kw: "align", run: align("justify") },
    { label: "Bullet list", group: "Paragraph", kw: "unordered", run: () => runToolbarEdit((s, o, e, f) => doc.toggleList(s, o, e, f, "bullet")) },
    { label: "Numbered list", group: "Paragraph", kw: "ordered", run: () => runToolbarEdit((s, o, e, f) => doc.toggleList(s, o, e, f, "numbered")) },
    { label: "Increase indent", group: "Paragraph", kw: "", run: () => runToolbarEdit((s, o, e, f) => doc.adjustIndent(s, o, e, f, 360)) },
    { label: "Decrease indent", group: "Paragraph", kw: "outdent", run: () => runToolbarEdit((s, o, e, f) => doc.adjustIndent(s, o, e, f, -360)) },
    { label: "Insert table (3×3)", group: "Insert", kw: "grid", run: () => selection && runEdit(() => doc.insertTable(selection.focus.node, 3, 3)) },
    { label: "Toggle outline", group: "View", kw: "headings navigation", run: () => toggleOutline() },
    { label: "Zoom in", group: "View", kw: "", run: () => stepZoom(1) },
    { label: "Zoom out", group: "View", kw: "", run: () => stepZoom(-1) },
    { label: "Settings", group: "View", kw: "theme accent dark", run: () => settingsBtn.click() },
  ];
  if (doc) {
    for (const name of doc.listStyles()) {
      cmds.push({
        label: `Style: ${name}`,
        group: "Style",
        kw: "paragraph heading",
        run: () => runToolbarEdit((s, o, e, f) => doc.setParagraphStyle(s, o, e, f, name)),
      });
    }
  }
  return cmds.filter((c) => doc || c.noDoc);
}

function renderCommands(query) {
  const q = query.trim().toLowerCase();
  const all = buildCommands();
  cmdMatches = q
    ? all.filter((c) => `${c.label} ${c.group} ${c.kw}`.toLowerCase().includes(q))
    : all;
  cmdSel = 0;
  cmdList.replaceChildren();
  if (!cmdMatches.length) {
    const empty = document.createElement("div");
    empty.className = "cmd-empty";
    empty.textContent = "No matching commands";
    cmdList.appendChild(empty);
    return;
  }
  cmdMatches.forEach((c, i) => {
    const item = document.createElement("button");
    item.type = "button";
    item.className = `cmd-item${i === cmdSel ? " sel" : ""}`;
    item.setAttribute("role", "option");
    item.innerHTML = `<span>${c.label}</span><span class="cmd-hint">${c.group}</span>`;
    item.addEventListener("mousemove", () => setCmdSel(i));
    item.addEventListener("click", () => runCommand(i));
    cmdList.appendChild(item);
  });
}

function setCmdSel(i) {
  cmdSel = i;
  const items = cmdList.querySelectorAll(".cmd-item");
  items.forEach((el, k) => el.classList.toggle("sel", k === i));
  items[i]?.scrollIntoView({ block: "nearest" });
}

function runCommand(i) {
  const cmd = cmdMatches[i];
  if (!cmd) return;
  closeCmd();
  cmd.run();
}

function openCmd() {
  cmdPalette.hidden = false;
  cmdInput.value = "";
  renderCommands("");
  cmdInput.focus();
}
function closeCmd() {
  cmdPalette.hidden = true;
}

cmdInput.addEventListener("input", () => renderCommands(cmdInput.value));
cmdInput.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    setCmdSel(Math.min(cmdSel + 1, cmdMatches.length - 1));
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    setCmdSel(Math.max(cmdSel - 1, 0));
  } else if (e.key === "Enter") {
    e.preventDefault();
    runCommand(cmdSel);
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeCmd();
  }
});
cmdPalette.addEventListener("pointerdown", (e) => {
  if (e.target === cmdPalette) closeCmd(); // click the backdrop
});
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    cmdPalette.hidden ? openCmd() : closeCmd();
  }
});

// Indentation: left/right absolute, and a first-line/hanging "special" indent
// (setFirstLineIndent encodes hanging as a negative value, 0 clears both).
indentLeftInput.addEventListener("change", () =>
  runToolbarEdit((a, b, c, d) => doc.setLeftIndent(a, b, c, d, inchTwips(indentLeftInput))),
);
indentRightInput.addEventListener("change", () =>
  runToolbarEdit((a, b, c, d) => doc.setRightIndent(a, b, c, d, inchTwips(indentRightInput))),
);
function applyIndentSpecial() {
  const by = inchTwips(indentSpecialByInput);
  const kind = indentSpecialSel.value;
  const twips = kind === "first" ? by : kind === "hanging" ? -by : 0;
  runToolbarEdit((a, b, c, d) => doc.setFirstLineIndent(a, b, c, d, twips));
}
indentSpecialSel.addEventListener("change", applyIndentSpecial);
indentSpecialByInput.addEventListener("change", applyIndentSpecial);

paraShade.addEventListener("input", () => {
  const [r, g, b] = hexToRgb(paraShade.value);
  runToolbarEdit((a, x, c, d) => doc.setParagraphShading(a, x, c, d, r, g, b, false));
});
onButton(paraShadeNone, () =>
  runToolbarEdit((a, x, c, d) => doc.setParagraphShading(a, x, c, d, 0, 0, 0, true)),
);
for (const [box, setter] of [
  [pgKeepNext, (a, b, c, d, on) => doc.setKeepWithNext(a, b, c, d, on)],
  [pgKeepLines, (a, b, c, d, on) => doc.setKeepLinesTogether(a, b, c, d, on)],
  [pgBreakBefore, (a, b, c, d, on) => doc.setPageBreakBefore(a, b, c, d, on)],
]) {
  box.addEventListener("change", () =>
    runToolbarEdit((a, b, c, d) => setter(a, b, c, d, box.checked)),
  );
}
highlightSel.addEventListener("change", () => {
  const name = highlightSel.value;
  armOrApplyRun({ highlight: name }, () =>
    runToolbarEdit((a, b, c, d) => doc.setHighlight(a, b, c, d, name)),
  );
  highlightSel.value = "none"; // highlight isn't reflected — keep it momentary
});
textColorInput.addEventListener("input", () => {
  const hex = textColorInput.value;
  const [r, g, b] = hexToRgb(hex);
  armOrApplyRun({ color: hex }, () =>
    runToolbarEdit((a, bo, c, d) => doc.setTextColor(a, bo, c, d, r, g, b)),
  );
});
fontFamilySel.addEventListener("change", () => {
  const family = fontFamilySel.value;
  if (family) {
    armOrApplyRun({ font: family }, () => runToolbarEdit((a, b, c, d) => doc.setFont(a, b, c, d, family)));
  }
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
        doc.insertStyledText(
          focus.node,
          focus.offset,
          key,
          pf.bold,
          pf.italic,
          pf.underline,
          pf.strike,
          pf.sizeHalfPoints,
          pf.color,
          pf.highlight,
          pf.vertAlign,
          pf.font,
        ),
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
