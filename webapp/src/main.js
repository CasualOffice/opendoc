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

// The engine `render_page(i, dpi)` rasterizes at `dpi` device px per inch
// (device_px = twip / 1440 * dpi). We render at 96·zoom·devicePixelRatio for a
// crisp result on HiDPI screens, then down-scale via CSS to logical pixels.
const BASE_DPI = 96;

/** The currently open document handle (or null). Kept so a zoom change re-renders. */
let doc = null;
/** Monotonic token so a slow render from a previous file/zoom is discarded. */
let renderToken = 0;

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
  setStatus(`Rendering ${count} page${count === 1 ? "" : "s"} at ${Math.round(zoom * 100)}%…`);

  for (let i = 0; i < count; i++) {
    // Yield so a burst of pages does not freeze the tab; abort if superseded.
    if (i > 0 && i % 4 === 0) await new Promise((r) => requestAnimationFrame(r));
    if (token !== renderToken) return;

    const bmp = doc.renderPage(i, dpi);
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
    pagesEl.appendChild(canvas);
  }

  if (token === renderToken) setStatus(`${count} page${count === 1 ? "" : "s"} · ${Math.round(zoom * 100)}%`);
}

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
