// OpenDoc WASM viewer — P1G-001 harness.
//
// Loads the `casual-doc-wasm` module, opens a user-selected `.docx` fully
// client-side, and blits each rendered page onto a canvas. This is the
// browser-first surface the viewer→editor is built and fine-tuned on (docs 56/57);
// no server, deployable as static files (e.g. GitHub Pages).

import init, { open } from "../pkg/casual_doc_wasm.js";
import {
  NAMED_WEB_FONT_FACES,
  SCRIPT_FALLBACK_FONTS,
  fallbackKeysFor,
  fetchFontBytes,
  packFontBytes,
} from "./web_fonts.mjs";
import { embedMarker, extractMarker, htmlToRuns, runsToHtml } from "./clipboard.mjs";
import {
  keyboardPlatform,
  navigationDirection,
  wordDeletionDirection,
} from "./keyboard.mjs";
import {
  clampContextMenuPosition,
  moveMenuIndex,
  normalizeMenuEntries,
} from "./context_menu.mjs";

function escapeHtml(text) {
  return String(text)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** url → Uint8Array of already-fetched font bytes (persists across documents). */
const fontCache = new Map();

const statusEl = document.getElementById("status");
const reviewLiveRegion = document.getElementById("reviewLiveRegion");
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
const clearFormattingBtn = document.getElementById("clearFormatting");
const spacingBtn = document.getElementById("spacingBtn");
const spacingMenu = document.getElementById("spacingMenu");
const spaceBeforeInput = document.getElementById("spaceBefore");
const spaceAfterInput = document.getElementById("spaceAfter");
const paraOptsBtn = document.getElementById("paraOptsBtn");
const paragraphPropertiesPanel = document.getElementById("paragraphPropertiesPanel");
const paragraphPropertiesContext = document.getElementById("paragraphPropertiesContext");
const paragraphPropertiesCloseBtn = document.getElementById("paragraphPropertiesClose");
const paraPanelStyle = document.getElementById("paraPanelStyle");
const paraPanelAlign = document.getElementById("paraPanelAlign");
const paraLineSpacing = document.getElementById("paraLineSpacing");
const paraSpaceBefore = document.getElementById("paraSpaceBefore");
const paraSpaceAfter = document.getElementById("paraSpaceAfter");
const paraShade = document.getElementById("paraShade");
const paraShadeNone = document.getElementById("paraShadeNone");
const paraShadeMixed = document.getElementById("paraShadeMixed");
const paraBordersMixed = document.getElementById("paraBordersMixed");
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
const tableContext = document.getElementById("tableContext");
const tableRibbon = document.querySelector(".table-ribbon");
const tableRibbonControls = [...tableRibbon.querySelectorAll("button")];
const tablePropertiesBtn = document.getElementById("tablePropertiesBtn");
const tableStyleBtn = document.getElementById("tableStyleBtn");
const tableStyleMenu = document.getElementById("tableStyleMenu");
const tablePropertiesPanel = document.getElementById("tablePropertiesPanel");
const tablePropertiesContext = document.getElementById("tablePropertiesContext");
const tablePropertiesCloseBtn = document.getElementById("tablePropertiesClose");
const tableColumnWidthNote = document.getElementById("tableColumnWidthNote");
const mergeCellsBtn = document.getElementById("mergeCellsBtn");
const splitCellBtn = document.getElementById("splitCellBtn");
const splitCellDialog = document.getElementById("splitCellDialog");
const splitCellClose = document.getElementById("splitCellClose");
const splitCellCancel = document.getElementById("splitCellCancel");
const splitCellConfirm = document.getElementById("splitCellConfirm");
const splitCellRows = document.getElementById("splitCellRows");
const splitCellColumns = document.getElementById("splitCellColumns");
const tableHeaderRow = document.getElementById("tableHeaderRow");
const tableFixedLayout = document.getElementById("tableFixedLayout");
const tableColumnWidth = document.getElementById("tableColumnWidth");
const tableWidth = document.getElementById("tableWidth");
const tableIndent = document.getElementById("tableIndent");
const tableRowHeight = document.getElementById("tableRowHeight");
const tableRowHeightRule = document.getElementById("tableRowHeightRule");
const tableCellMargin = document.getElementById("tableCellMargin");
const tableCellSpacing = document.getElementById("tableCellSpacing");
const tableCaption = document.getElementById("tableCaption");
const tableDescription = document.getElementById("tableDescription");
const tableFormula = document.getElementById("tableFormula");
const tableFormulaApply = document.getElementById("tableFormulaApply");
const insertTableBtn = document.getElementById("insertTableBtn");
const insertLinkBtn = document.getElementById("insertLinkBtn");
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
const findBtn = document.getElementById("findBtn");
const findPanel = document.getElementById("findPanel");
const findInput = document.getElementById("findInput");
const replaceInput = document.getElementById("replaceInput");
const findPrevBtn = document.getElementById("findPrev");
const findNextBtn = document.getElementById("findNext");
const findStatus = document.getElementById("findStatus");
const findCase = document.getElementById("findCase");
const findWholeWord = document.getElementById("findWholeWord");
const findSelection = document.getElementById("findSelection");
let findScope = null;
const replaceOneBtn = document.getElementById("replaceOne");
const replaceAllBtn = document.getElementById("replaceAll");
const findCloseBtn = document.getElementById("findClose");

/** Shows the named ribbon tab's panel and marks its tab selected. */
function selectRibbonTab(name) {
  for (const t of ribbonTabs) t.setAttribute("aria-selected", String(t.dataset.tab === name));
  for (const p of ribbonPanels) p.hidden = p.dataset.panel !== name;
  // Recompute overflow synchronously (not on a later frame) so a control is
  // already in its final inline-or-overflow location the moment the panel shows —
  // the newly shown panel reflows and the previous panel's groups are restored.
  if (typeof updateRibbonOverflow === "function") updateRibbonOverflow();
}
for (const t of ribbonTabs) {
  t.addEventListener("click", () => {
    if (t.disabled) return;
    selectRibbonTab(t.dataset.tab);
    // Clicking a tab while collapsed brings the ribbon back (Word behavior).
    if (ribbonViewCollapsed) setRibbonCollapsed(false);
  });
}

// --- Compact ↔ ribbon view toggle (collapse/expand the band) -----------------
const ribbonViewToggle = document.getElementById("ribbonViewToggle");
let ribbonViewCollapsed = false;

/** Collapses the ribbon to just its tab strip (compact view) or expands it back
 *  to the full band, persisting the choice. */
function setRibbonCollapsed(collapsed) {
  ribbonViewCollapsed = collapsed;
  const ribbon = document.querySelector(".ribbon");
  ribbon?.classList.toggle("is-collapsed", collapsed);
  if (ribbonViewToggle) {
    ribbonViewToggle.setAttribute("aria-expanded", String(!collapsed));
    ribbonViewToggle.setAttribute(
      "aria-label",
      collapsed ? "Expand the ribbon" : "Collapse the ribbon",
    );
    ribbonViewToggle.title = collapsed
      ? "Expand the ribbon"
      : "Collapse the ribbon (compact view)";
    const icon = ribbonViewToggle.querySelector(".ms");
    if (icon) icon.textContent = collapsed ? "keyboard_arrow_down" : "keyboard_arrow_up";
  }
  try {
    localStorage.setItem("opendoc.ribbonCollapsed", collapsed ? "1" : "0");
  } catch {
    /* private mode / storage disabled — the toggle still works in-session */
  }
  if (!collapsed && typeof scheduleRibbonOverflow === "function") scheduleRibbonOverflow();
}

if (ribbonViewToggle) {
  ribbonViewToggle.addEventListener("click", () => setRibbonCollapsed(!ribbonViewCollapsed));
  try {
    if (localStorage.getItem("opendoc.ribbonCollapsed") === "1") setRibbonCollapsed(true);
  } catch {
    /* ignore */
  }
}

// --- Home ribbon: Clipboard, Styles gallery, overflow, tooltips (docs/64) ----
// The Home band mirrors template.png. Every control below maps to a real,
// working opendoc action; nothing is a placeholder (docs/64 "no dead controls").

const pasteBtn = document.getElementById("pasteBtn");
const cutBtn = document.getElementById("cutBtn");
const copyBtn = document.getElementById("copyBtn");
const replaceBtn = document.getElementById("replaceBtn");
// Clipboard buttons reuse the exact clipboard actions the command palette and
// keyboard already invoke (`paste`/`cut`/`copySelection`), so they are never a
// second code path. Replace opens the same Find & Replace panel as Find.
pasteBtn.addEventListener("click", () => { paste(); });
cutBtn.addEventListener("click", () => { cut(); });
copyBtn.addEventListener("click", () => { copySelection(); });
replaceBtn.addEventListener("click", () => { if (!findBtn.disabled) findBtn.click(); });

// Live Styles gallery — the visible control (template.png). It is populated from
// the document's real styles (`doc.listStyles()`, the same source as the hidden
// `#paragraphStyle` select it drives) and applies a style through the identical
// `setParagraphStyle` path the select's change handler uses.
const stylesGallery = document.getElementById("stylesGallery");
const stylesScrollPrev = document.querySelector('[data-styles-scroll="prev"]');
const stylesScrollNext = document.querySelector('[data-styles-scroll="next"]');

/** Rebuilds the Styles gallery cards from the document's style names. */
function buildStylesGallery(styles) {
  if (!stylesGallery) return;
  stylesGallery.replaceChildren();
  for (const name of styles) {
    const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "");
    const card = document.createElement("button");
    card.type = "button";
    card.className = `style-card style-card-${slug}`;
    card.dataset.style = name;
    card.setAttribute("role", "option");
    card.setAttribute("aria-selected", "false");
    card.title = name;
    const label = document.createElement("span");
    label.className = "style-card-name";
    label.textContent = name;
    card.appendChild(label);
    card.addEventListener("click", () => {
      if (card.disabled) return;
      runToolbarEdit((a, b, c, d) => doc.setParagraphStyle(a, b, c, d, name));
    });
    stylesGallery.appendChild(card);
  }
  updateStylesScrollAffordance();
}

/** Highlights the gallery card matching the reflected paragraph style. */
function syncStylesGalleryActive() {
  if (!stylesGallery) return;
  const active = paragraphStyleSel.value;
  for (const card of stylesGallery.children) {
    card.setAttribute("aria-selected", String(card.dataset.style === active));
  }
}

/** Shows the ‹/› scroll chevrons only when the gallery overflows its box. */
function updateStylesScrollAffordance() {
  if (!stylesGallery || !stylesScrollPrev || !stylesScrollNext) return;
  const overflowing = stylesGallery.scrollWidth > stylesGallery.clientWidth + 1;
  const atStart = stylesGallery.scrollLeft <= 1;
  const atEnd =
    stylesGallery.scrollLeft + stylesGallery.clientWidth >= stylesGallery.scrollWidth - 1;
  stylesScrollPrev.hidden = !overflowing || atStart;
  stylesScrollNext.hidden = !overflowing || atEnd;
}

if (stylesScrollPrev && stylesScrollNext && stylesGallery) {
  stylesScrollPrev.addEventListener("click", () => {
    stylesGallery.scrollBy({ left: -140, behavior: "smooth" });
  });
  stylesScrollNext.addEventListener("click", () => {
    stylesGallery.scrollBy({ left: 140, behavior: "smooth" });
  });
  stylesGallery.addEventListener("scroll", updateStylesScrollAffordance, { passive: true });
}

// --- Ribbon overflow: collapse groups that don't fit into a "⋯" menu ---------
const ribbonBodyEl = document.querySelector(".ribbon-body");
const ribbonEl = document.querySelector(".ribbon");
const ribbonOverflowBtn = document.getElementById("ribbonOverflowBtn");
const ribbonOverflowMenu = document.getElementById("ribbonOverflowMenu");
// Canonical group order per panel, captured before any group is relocated.
const ribbonPanelGroups = new Map(
  ribbonPanels.map((p) => [p, [...p.querySelectorAll(":scope > .rgroup")]]),
);

function closeRibbonOverflow() {
  if (!ribbonOverflowMenu) return;
  ribbonOverflowMenu.hidden = true;
  ribbonOverflowBtn?.setAttribute("aria-expanded", "false");
}

/** Reflows the active ribbon panel: groups that don't fit move into the "⋯"
 *  overflow menu so the ribbon never shows a horizontal scrollbar. */
function updateRibbonOverflow() {
  if (!ribbonBodyEl || !ribbonOverflowBtn || !ribbonOverflowMenu) return;
  closeRibbonOverflow();
  // Restore every group to its home panel in canonical order before measuring.
  for (const [panel, groups] of ribbonPanelGroups) {
    for (const g of groups) if (g.parentElement !== panel) panel.appendChild(g);
  }
  ribbonOverflowMenu.replaceChildren();
  ribbonOverflowBtn.hidden = true;
  const active = ribbonPanels.find((p) => !p.hidden);
  if (!active) return;
  const groups = ribbonPanelGroups.get(active) || [];
  const style = getComputedStyle(active);
  const avail =
    active.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  const widths = groups.map((g) => g.offsetWidth);
  const total = widths.reduce((a, b) => a + b, 0);
  if (total <= avail + 0.5) return; // everything fits — no overflow control
  // Reserve room for the ⋯ button and keep the widest fitting prefix inline.
  const reserve = 44;
  let used = 0;
  let cut = groups.length;
  for (let i = 0; i < groups.length; i++) {
    if (used + widths[i] > avail - reserve) {
      cut = i;
      break;
    }
    used += widths[i];
  }
  if (cut < 1) cut = 1; // always keep at least one group inline
  for (let i = cut; i < groups.length; i++) ribbonOverflowMenu.appendChild(groups[i]);
  ribbonOverflowBtn.hidden = false;
}

let ribbonOverflowFrame = 0;
function scheduleRibbonOverflow() {
  cancelAnimationFrame(ribbonOverflowFrame);
  ribbonOverflowFrame = requestAnimationFrame(updateRibbonOverflow);
}

if (ribbonOverflowBtn && ribbonOverflowMenu) {
  // The menu is fixed-positioned and lives on <body> so the ribbon's
  // `overflow:hidden` never clips the dropdown.
  document.body.appendChild(ribbonOverflowMenu);
  const positionOverflowMenu = () => {
    const rect = ribbonOverflowBtn.getBoundingClientRect();
    const mw = ribbonOverflowMenu.offsetWidth;
    const left = Math.max(6, Math.min(rect.right - mw, window.innerWidth - mw - 6));
    ribbonOverflowMenu.style.left = `${Math.round(left)}px`;
    ribbonOverflowMenu.style.top = `${Math.round(rect.bottom + 4)}px`;
  };
  ribbonOverflowBtn.addEventListener("click", () => {
    const open = ribbonOverflowMenu.hidden;
    ribbonOverflowMenu.hidden = !open;
    ribbonOverflowBtn.setAttribute("aria-expanded", String(open));
    if (open) positionOverflowMenu();
  });
  document.addEventListener("pointerdown", (e) => {
    if (ribbonOverflowMenu.hidden) return;
    if (e.target.closest("#ribbonOverflowMenu, #ribbonOverflowBtn")) return;
    closeRibbonOverflow();
  });
  if (typeof ResizeObserver === "function" && ribbonBodyEl) {
    new ResizeObserver(scheduleRibbonOverflow).observe(ribbonBodyEl);
  } else {
    window.addEventListener("resize", scheduleRibbonOverflow);
  }
}

// --- Delayed tooltips for icon-only ribbon controls (docs/64 §3) -------------
// A single custom tooltip (~350ms hover/focus delay) shows the control's name +
// shortcut. Reuses the existing `title`/`aria-label` content; the native title
// is suppressed only while the control is actively hovered so it never appears
// alongside the custom one, and is restored on leave (keeping dynamic titles and
// accessibility intact).
const TIP_SELECTOR = ".fmt, .ribbon-tab, .review-mode-seg, .style-card, .styles-scroll-btn";
const ribbonTooltip = document.createElement("div");
ribbonTooltip.className = "ribbon-tooltip";
ribbonTooltip.setAttribute("role", "tooltip");
ribbonTooltip.hidden = true;
document.body.appendChild(ribbonTooltip);
let tipTimer = 0;
let tipTarget = null;

function tipContentFor(el) {
  const raw = (el.dataset.tipTitle ?? el.getAttribute("title") ?? "").trim();
  const label = (el.getAttribute("aria-label") ?? "").trim();
  const match = raw.match(/^(.*?)\s*\(([^)]+)\)\s*$/);
  const name = (label || (match ? match[1] : raw)).trim();
  const shortcut = match ? match[2].trim() : "";
  return { name, shortcut };
}

function positionTip(el) {
  const rect = el.getBoundingClientRect();
  const tw = ribbonTooltip.offsetWidth;
  const th = ribbonTooltip.offsetHeight;
  let left = rect.left + rect.width / 2 - tw / 2;
  left = Math.max(6, Math.min(left, window.innerWidth - tw - 6));
  let top = rect.bottom + 6;
  if (top + th > window.innerHeight - 6) top = rect.top - th - 6;
  ribbonTooltip.style.left = `${Math.round(left)}px`;
  ribbonTooltip.style.top = `${Math.round(top)}px`;
}

function showTip(el) {
  const { name, shortcut } = tipContentFor(el);
  if (!name) return;
  ribbonTooltip.textContent = name;
  if (shortcut) {
    const kbd = document.createElement("kbd");
    kbd.textContent = shortcut;
    ribbonTooltip.appendChild(kbd);
  }
  ribbonTooltip.hidden = false;
  positionTip(el);
  ribbonTooltip.classList.add("is-visible");
}

function armTip(el) {
  if (el.getAttribute("title")) {
    el.dataset.tipTitle = el.getAttribute("title");
    el.removeAttribute("title");
  }
  tipTarget = el;
  clearTimeout(tipTimer);
  tipTimer = window.setTimeout(() => {
    if (tipTarget === el) showTip(el);
  }, 350);
}

function disarmTip(el) {
  if (el && el.dataset.tipTitle != null) {
    el.setAttribute("title", el.dataset.tipTitle);
    delete el.dataset.tipTitle;
  }
  if (tipTarget === el || !el) {
    clearTimeout(tipTimer);
    tipTimer = 0;
    tipTarget = null;
    ribbonTooltip.classList.remove("is-visible");
    ribbonTooltip.hidden = true;
  }
}

if (ribbonEl) {
  ribbonEl.addEventListener("pointerover", (e) => {
    const el = e.target.closest(TIP_SELECTOR);
    if (!el || !ribbonEl.contains(el) || el === tipTarget) return;
    if (tipTarget) disarmTip(tipTarget);
    armTip(el);
  });
  ribbonEl.addEventListener("pointerout", (e) => {
    if (!tipTarget) return;
    if (e.relatedTarget && tipTarget.contains(e.relatedTarget)) return;
    disarmTip(tipTarget);
  });
  ribbonEl.addEventListener("focusin", (e) => {
    const el = e.target.closest(TIP_SELECTOR);
    if (!el) return;
    if (tipTarget && tipTarget !== el) disarmTip(tipTarget);
    armTip(el);
  });
  ribbonEl.addEventListener("focusout", (e) => {
    const el = e.target.closest(TIP_SELECTOR);
    if (el) disarmTip(el);
  });
  ribbonEl.addEventListener("click", () => {
    if (tipTarget) disarmTip(tipTarget);
  });
  window.addEventListener("scroll", () => { if (tipTarget) disarmTip(tipTarget); }, true);
}

undoBtn.addEventListener("click", () => runEdit(() => doc.undo()));
redoBtn.addEventListener("click", () => runEdit(() => doc.redo()));
viewOutlineBtn.addEventListener("click", () => toggleOutline());
viewZoomOut.addEventListener("click", () => stepZoom(-1));
viewZoomIn.addEventListener("click", () => stepZoom(1));
const railOutline = document.getElementById("railOutline");
const railReview = document.getElementById("railReview");
const outlinePanel = document.getElementById("outlinePanel");
const outlineClose = document.getElementById("outlineClose");
const outlineBody = document.getElementById("outlineBody");
const a11yDocument = document.getElementById("a11yDocument");
const reviewBtn = document.getElementById("reviewBtn");
const reviewClose = document.getElementById("reviewClose");
const reviewFilters = [...document.querySelectorAll("[data-review-filter]")];
const selComment = document.getElementById("selComment");
const reviewModeButtons = [...document.querySelectorAll("[data-review-mode]")];
const reviewPrevious = document.getElementById("reviewPrevious");
const reviewNext = document.getElementById("reviewNext");
const reviewAcceptAll = document.getElementById("reviewAcceptAll");
const reviewRejectAll = document.getElementById("reviewRejectAll");
const reviewBulkActions = document.getElementById("reviewBulkActions");
const reviewModeControl = document.getElementById("reviewModeControl");
const reviewModeSegButtons = reviewModeControl
  ? [...reviewModeControl.querySelectorAll("[data-review-mode]")]
  : [];
const suggestingBanner = document.getElementById("suggestingBanner");
const suggestingBannerEdit = document.getElementById("suggestingBannerEdit");
const viewingBanner = document.getElementById("viewingBanner");
const viewingBannerEdit = document.getElementById("viewingBannerEdit");
const reviewSidebar = document.getElementById("reviewSidebar");
const reviewSidebarBody = document.getElementById("reviewSidebarBody");
const reviewSidebarHeader = document.getElementById("reviewSidebarHeader");
let reviewMode = "editing";
let reviewRevisionCursor = -1;
let activeReviewCommentId = null;
let activeReviewItemId = null;
let reviewPopover = null;
let reviewSidebarPreference = null;
let reviewComposerState = null;
let reviewDeleteConfirmId = null;
let reviewMarginFrame = 0;
// Sidebar virtualization (REVIEW-GAP-020). The full render (on content edit /
// resize) computes every item's stacked document-scroll position once and keeps
// it in `reviewLayout`; scrolling only re-windows that precomputed layout, never
// re-parsing the review payload or rebuilding cards. `reviewCardCache` retains
// each item's built DOM + measured height keyed by a content signature, so an
// unchanged card is never rebuilt or re-measured, and only cards inside (or near)
// the viewport are ever mounted into the DOM.
let reviewLayout = [];
// A caret→card index rebuilt each render: for every shown item, its card id and
// the model anchor range (node + byte offsets) its text occupies, so a caret
// landing in a commented or tracked-changed range can expand that exact card
// (REVIEW-GAP-019 caret-driven expansion, for comments AND revisions).
let reviewAnchorIndex = [];
const reviewCardCache = new Map();
let reviewWindowFrame = 0;
// Pixels above and below the viewport to keep mounted, so a scroll reveals an
// already-present card instead of a blank gap before the next frame mounts it.
const REVIEW_WINDOW_OVERSCAN = 800;

/** Reads the typed comment/revision review data (docs/81 REVIEW-GAP-022's
 *  `listComments`/`listRevisions`), shaped like the legacy combined
 *  `reviewSummary()` payload so `summary.comments`/`summary.revisions` call
 *  sites are unchanged. Prefer `doc.listComments()`/`doc.listRevisions()`
 *  directly when only one of the two is needed. */
function readReviewData(doc) {
  let comments = [];
  let revisions = [];
  try { comments = JSON.parse(doc.listComments()) ?? []; } catch { comments = []; }
  try { revisions = JSON.parse(doc.listRevisions()) ?? []; } catch { revisions = []; }
  return { comments, revisions };
}

// --- Per-author review color / attribution (docs/81 REVIEW-GAP-015) -----------
//
// Word and Google Docs give each distinct reviewer a stable, auto-assigned color
// so overlapping authors are distinguishable at a glance, plus a hover tooltip
// with name/date/change type (docs/68 §"Reference reading", §50). We mirror that:
// a fixed cycling palette, keyed deterministically by the author's stable
// identity, is assigned in the webapp only — presentation, never persisted into
// the model (the engine keeps just the opaque `author` string). The projection
// (`listComments`/`listRevisions`) exposes the author *name* (and comment
// initials), which is the stable key docs/68 §50 specifies hashing.

// Ten hues chosen to stay legible over the white document canvas and, as a solid
// avatar fill with white text, in both light and dark themes. Deliberately not
// the theme accent, so author colors never collide with selection/UI chrome.
const REVIEW_AUTHOR_PALETTE = [
  "#1a73e8", // blue
  "#188038", // green
  "#d93025", // red
  "#9334e6", // purple
  "#e37400", // orange
  "#0b8043", // deep green
  "#a50e0e", // dark red
  "#8430ce", // violet
  "#b06000", // amber-brown
  "#12805c", // teal
];

// The neutral fallback for an item with no author at all ("You"/"Unknown"): a
// grey that is not part of the palette, so an unattributed change never masquer-
// ades as a specific reviewer's color.
const REVIEW_AUTHOR_FALLBACK_COLOR = "#5f6368";

/** The stable per-author key: the author name, else the initials, else empty
 *  (the unattributed "You"/"Unknown" bucket). Case-folded so "Ada"/"ada" share
 *  one color. */
function reviewAuthorKey(item) {
  const name = String(item?.author ?? "").trim();
  if (name) return name.toLowerCase();
  const initials = String(item?.initials ?? "").trim();
  if (initials) return initials.toLowerCase();
  return "";
}

/** A deterministic palette color for an author key. Empty key → neutral
 *  fallback. Same key always yields the same color within and across sessions
 *  (pure function of the key), so an author's insertions, deletions, and
 *  comments all render in one color. */
function reviewAuthorColor(key) {
  if (!key) return REVIEW_AUTHOR_FALLBACK_COLOR;
  // FNV-1a-style rolling hash — stable, order-sensitive, no dependencies.
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    hash ^= key.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return REVIEW_AUTHOR_PALETTE[hash % REVIEW_AUTHOR_PALETTE.length];
}

/** The human display name for an author, matching the sidebar's existing
 *  convention: name, else initials, else "You" (an unattributed local edit). */
function reviewAuthorDisplay(item) {
  return String(item?.author ?? "").trim()
    || String(item?.initials ?? "").trim()
    || "You";
}

/** The change-type label used in an attribution tooltip. */
function reviewChangeTypeLabel(kind) {
  switch (kind) {
    case "insertion": return "Insertion";
    case "deletion": return "Deletion";
    case "replacement": return "Replacement";
    case "formatting": return "Formatting change";
    case "move": return "Move";
    case "move_from": return "Move (source)";
    case "move_to": return "Move (destination)";
    default: return "Change";
  }
}

/** The `author · type · date` attribution string shown on hover of a tracked
 *  change (its inline marker and its sidebar card). Omits empty segments. */
function reviewRevisionTooltip(revision) {
  return [
    reviewAuthorDisplay(revision),
    reviewChangeTypeLabel(revision.kind),
    formatReviewDate(revision.date),
  ].filter(Boolean).join(" · ");
}

/** The `author · date` attribution string for a comment marker/card. */
function reviewCommentTooltip(comment) {
  return [
    reviewAuthorDisplay(comment),
    comment.resolved ? "Resolved" : "Comment",
    formatReviewDate(comment.date),
  ].filter(Boolean).join(" · ");
}

/** A descriptive accessible name for a review card — who, what kind, and a short
 *  text snippet — so a screen reader announces the card's content and role
 *  rather than a nameless generic article (REVIEW-GAP-023). */
function reviewCardAriaLabel(item) {
  const author = reviewAuthorDisplay(item.data) || "You";
  const snippet = String(item.data.text || "").replace(/\s+/g, " ").trim().slice(0, 80);
  const suffix = snippet ? `: ${snippet}` : "";
  if (item.type === "comment") {
    const kind = item.data.parentParaId ? "Reply" : item.data.resolved ? "Resolved comment" : "Comment";
    return `${kind} by ${author}${suffix}`;
  }
  const kind = (reviewChangeTypeLabel(item.data.kind) || "change").toLowerCase();
  return `Suggested ${kind} by ${author}${suffix}`;
}

// Enable the three-state mode control (Editing / Suggesting / Viewing) once a
// document is loaded (its per-button pressed state is owned by setReviewMode),
// and reflect the review-sidebar workflow controls' availability: Next/Previous
// and Accept-all/Reject-all need at least one tracked change, and bulk decisions
// are hidden in read-only Viewing mode (REVIEW-GAP-018).
function updateReviewControls() {
  if (!doc) return;
  let count = 0;
  try { count = (JSON.parse(doc.listRevisions()) ?? []).length; } catch { count = 0; }
  for (const button of reviewModeSegButtons) button.disabled = false;
  if (reviewPrevious) reviewPrevious.disabled = count === 0;
  if (reviewNext) reviewNext.disabled = count === 0;
  const canDecide = count > 0 && reviewMode !== "viewing";
  if (reviewAcceptAll) reviewAcceptAll.disabled = !canDecide;
  if (reviewRejectAll) reviewRejectAll.disabled = !canDecide;
  if (reviewBulkActions) reviewBulkActions.hidden = reviewMode === "viewing";
}

/** The three review modes (docs/68 §"Suggesting mode"): `editing` applies
 *  edits directly, `suggesting` records them as tracked revisions, and
 *  `viewing` is fully read-only — no Operation reaches apply. Any unrecognized
 *  value falls back to `editing`. */
function setReviewMode(mode) {
  const previous = reviewMode;
  reviewMode =
    mode === "suggesting" ? "suggesting" : mode === "viewing" ? "viewing" : "editing";
  suggestingBanner.hidden = reviewMode !== "suggesting";
  if (viewingBanner) viewingBanner.hidden = reviewMode !== "viewing";
  for (const button of reviewModeButtons) {
    button.setAttribute("aria-pressed", String(button.dataset.reviewMode === reviewMode));
  }
  // Announce a genuine user mode change (not the load-time reset to Editing).
  if (reviewMode !== previous) {
    announceReview(`${reviewMode[0].toUpperCase()}${reviewMode.slice(1)} mode`);
  }
  updateReviewControls();
  drawSelection();
  // Toolbar controls must not retain focus after changing mode: clipboard,
  // typing, and deletion events are deliberately accepted only while the
  // canvas editor owns focus.
  focusEditorSurface();
}

function reviewRangeClientRect(startNode, startOffset, endNode, endOffset) {
  let rects = doc?.selectionRects(startNode, startOffset, endNode, endOffset) ?? [];
  if (rects.length < 5 && startNode === endNode && startOffset === endOffset) {
    rects = doc?.caretRect(startNode, startOffset) ?? [];
  }
  if (rects.length < 5) return null;
  const [pageNumber, x, y, width, height] = rects;
  const page = pages[pageNumber - 1];
  if (!page) return null;
  const canvasRect = page.canvas.getBoundingClientRect();
  const { sx, sy } = scaleOf(page);
  return {
    pageNumber,
    left: canvasRect.left + x * sx,
    right: canvasRect.left + (x + width) * sx,
    top: canvasRect.top + y * sy,
    bottom: canvasRect.top + (y + height) * sy,
    pageRight: canvasRect.right,
  };
}

function reviewFormattingValue(property, value) {
  if (value == null) return "inherited";
  if (typeof value === "boolean") return value ? "on" : "off";
  if (property === "sizeHalfPoints" && Number.isFinite(Number(value))) {
    return `${Number(value) / 2} pt`;
  }
  if (typeof value === "object") {
    const scalar = Object.values(value).find((candidate) =>
      ["string", "number", "boolean"].includes(typeof candidate));
    return scalar == null ? "custom" : String(scalar);
  }
  return String(value);
}

function reviewFormattingDescription(changes) {
  const labels = {
    bold: "Bold",
    italic: "Italic",
    underline: "Underline",
    strike: "Strikethrough",
    font: "Font",
    sizeHalfPoints: "Font size",
    color: "Text color",
    highlight: "Highlight",
    verticalAlignment: "Vertical alignment",
  };
  return (changes ?? []).map((change) => {
    const property = String(change?.property || "");
    const label = labels[property] || property || "Formatting";
    return `${label}: ${reviewFormattingValue(property, change?.before)} → ${reviewFormattingValue(property, change?.after)}`;
  }).join("\n");
}

function revisionRange(revision) {
  if (revision?.anchor?.node) {
    return {
      startNode: revision.anchor.node,
      startOffset: Number(revision.anchor.start) || 0,
      endNode: revision.anchor.node,
      endOffset: Number(revision.anchor.end) || Number(revision.anchor.start) || 0,
    };
  }
  const text = String(revision?.text || "");
  if (!doc || !text) return null;
  const first = doc.firstPosition();
  const match = doc.findText(text, first.node, first.offset, true, false);
  first.free();
  if (!match.found) { match.free(); return null; }
  const range = {
    startNode: match.startNode,
    startOffset: match.startOffset,
    endNode: match.endNode,
    endOffset: match.endOffset,
  };
  match.free();
  return range;
}

function reviewCardButton(label, action, danger = false, ariaLabel = "") {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `review-margin-action${danger ? " danger" : ""}`;
  button.textContent = label;
  // A descriptive accessible name where the visible verb alone ("Accept",
  // "Reply") lacks context for a screen reader (REVIEW-GAP-023).
  if (ariaLabel) {
    button.setAttribute("aria-label", ariaLabel);
    button.title = ariaLabel;
  }
  button.addEventListener("click", async (event) => {
    event.stopPropagation();
    await action();
  });
  return button;
}

function reviewIconButton(icon, label, action) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "review-margin-icon-action";
  button.setAttribute("aria-label", label);
  button.title = label;
  const glyph = document.createElement("span");
  glyph.className = "ms";
  glyph.setAttribute("aria-hidden", "true");
  glyph.textContent = icon;
  button.appendChild(glyph);
  button.addEventListener("click", async (event) => {
    event.stopPropagation();
    await action();
  });
  return button;
}

function scheduleReviewMarginRender() {
  if (reviewMarginFrame) return;
  reviewMarginFrame = requestAnimationFrame(() => {
    reviewMarginFrame = 0;
    renderReviewMarginItems();
  });
}

function renderReviewMarginItems() {
  reviewSidebarBody.replaceChildren();
  if (!doc || !pages.length) {
    reviewSidebar.hidden = true;
    viewportEl.classList.remove("has-review-sidebar");
    reviewLayout = [];
    reviewCardCache.clear();
    return;
  }
  const summary = readReviewData(doc);
  const items = [];
  const comments = summary.comments ?? [];
  updateReviewControls();
  for (const comment of comments) {
    if (!comment.anchor?.node) continue;
    // Open / Resolved / All comment filter (REVIEW-GAP-018/019). A reply always
    // follows its root's visibility so a thread is never half-shown: match on the
    // root comment's resolved state when this is a reply.
    if (reviewFilter !== "all") {
      const root = comment.parentParaId
        ? comments.find((c) => c.paraId === comment.parentParaId) ?? comment
        : comment;
      const resolved = !!root.resolved;
      if (reviewFilter === "resolved" ? !resolved : resolved) continue;
    }
    const rect = reviewRangeClientRect(comment.anchor.node, Number(comment.anchor.start) || 0, comment.anchor.node, Number(comment.anchor.end) || 0);
    if (rect) items.push({ type: "comment", data: comment, rect });
  }
  const revisionItems = [];
  const groupedRevisions = new Map();
  const groupedMoves = new Map();
  for (const revision of summary.revisions ?? []) {
    if (revision.movePair?.fromStart && revision.movePair?.toStart) {
      const moveKey = `${revision.movePair.fromStart}:${revision.movePair.toStart}`;
      const group = groupedMoves.get(moveKey) ?? [];
      group.push(revision);
      groupedMoves.set(moveKey, group);
      continue;
    }
    const groupId = String(revision.groupId || "");
    const groupKind = String(revision.groupKind || "");
    if (groupId && ["typing", "replacement", "formatting"].includes(groupKind)) {
      const group = groupedRevisions.get(groupId) ?? [];
      group.push(revision);
      groupedRevisions.set(groupId, group);
    } else {
      revisionItems.push(revision);
    }
  }
  for (const [groupId, revisions] of groupedRevisions) {
    const ranges = revisions.map(revisionRange).filter(Boolean);
    const node = ranges[0]?.startNode;
    if (!node || ranges.some((range) => range.startNode !== node || range.endNode !== node)) {
      revisionItems.push(...revisions);
      continue;
    }
    const deletions = revisions.filter((revision) => revision.kind === "deletion");
    const insertions = revisions.filter((revision) => revision.kind === "insertion");
    const formatting = revisions.filter((revision) => revision.kind === "formatting");
    const groupKind = String(revisions[0]?.groupKind || "");
    const kind = groupKind === "formatting"
      ? "formatting"
      : groupKind === "replacement" || deletions.length > 0
        ? "replacement"
        : "insertion";
    revisionItems.push({
      id: groupId,
      groupId,
      kind,
      author: revisions.find((revision) => revision.author)?.author,
      date: revisions.find((revision) => revision.date)?.date,
      text: (kind === "formatting" ? formatting : kind === "insertion" ? insertions : deletions)
        .map((revision) => String(revision.text || "")).join(""),
      oldText: deletions.map((revision) => String(revision.text || "")).join(""),
      newText: insertions.map((revision) => String(revision.text || "")).join(""),
      formattingDelta: formatting.flatMap((revision) =>
        Array.isArray(revision.formattingDelta) ? revision.formattingDelta : []),
      anchor: {
        node,
        start: Math.min(...ranges.map((range) => range.startOffset)),
        end: Math.max(...ranges.map((range) => range.endOffset)),
      },
      revisions,
    });
  }
  for (const [moveKey, revisions] of groupedMoves) {
    const source = revisions.filter((revision) => revision.kind === "move_from");
    const destination = revisions.filter((revision) => revision.kind === "move_to");
    const ranges = revisions.map(revisionRange).filter(Boolean);
    if (!source.length || !destination.length || ranges.length !== revisions.length) {
      revisionItems.push(...revisions);
      continue;
    }
    const movePair = revisions[0].movePair;
    revisionItems.push({
      id: `move:${moveKey}`,
      kind: "move",
      author: revisions.find((revision) => revision.author)?.author,
      date: revisions.find((revision) => revision.date)?.date,
      text: destination.map((revision) => String(revision.text || "")).join(""),
      oldText: source.map((revision) => String(revision.text || "")).join(""),
      newText: destination.map((revision) => String(revision.text || "")).join(""),
      anchor: source[0].anchor,
      destinationAnchor: destination[0].anchor,
      movePair,
      ranges,
      revisions,
    });
  }
  // Tracked changes are not "resolved" — the Resolved filter is comment-only, so
  // it hides revisions; Open and All show them (REVIEW-GAP-018/019).
  for (const revision of reviewFilter === "resolved" ? [] : revisionItems) {
    const ranges = revision.ranges ?? [revisionRange(revision)].filter(Boolean);
    const positioned = ranges
      .map((range) => ({
        range,
        rect: reviewRangeClientRect(
          range.startNode,
          range.startOffset,
          range.endNode,
          range.endOffset,
        ),
      }))
      .filter((item) => item.rect)
      .sort((a, b) => a.rect.pageNumber - b.rect.pageNumber || a.rect.top - b.rect.top);
    if (positioned.length) {
      items.push({
        type: "revision",
        data: revision,
        range: positioned[0].range,
        rect: positioned[0].rect,
        ranges: positioned.map((item) => item.range),
      });
    }
  }
  if (reviewComposerState?.range) {
    const { start, end } = reviewComposerState.range;
    const rect = reviewRangeClientRect(start.node, start.offset, end.node, end.offset);
    if (rect) items.push({ type: "composer", data: reviewComposerState, rect });
  }
  items.sort((a, b) => a.rect.pageNumber - b.rect.pageNumber || a.rect.top - b.rect.top);

  const show = reviewSidebarPreference ?? items.length > 0;
  reviewSidebar.hidden = !show;
  // Reserve the comment column's width in the page stack only while the column
  // is shown, so pages stay centered-ish and the single `.viewport` scrollbar
  // sits past the comments (never between the canvas and the comments).
  viewportEl.classList.toggle("has-review-sidebar", show);
  reviewBtn.setAttribute("aria-pressed", String(show));
  railReview.setAttribute("aria-pressed", String(show));
  if (!show) {
    reviewLayout = [];
    return;
  }

  // The comment layer rides inside `.viewport`'s single scroll context; its body
  // spans the page-stack height so the transparent margin remains click-to-
  // deselect and cards are never clipped. No scrollTop sync: one scroll owner.
  const viewportRect = viewportEl.getBoundingClientRect();
  reviewSidebarBody.style.height = `${Math.max(pagesEl.scrollHeight, viewportEl.clientHeight)}px`;
  // Cancel the sticky header's flow height so cards stay pixel-aligned to their
  // canvas anchors (the header floats above via `position: sticky`).
  if (reviewSidebarHeader) {
    reviewSidebarBody.style.marginTop = `-${reviewSidebarHeader.offsetHeight}px`;
  }

  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "review-sidebar-empty";
    empty.innerHTML = '<span class="ms" aria-hidden="true">chat_bubble_outline</span><br>No comments or suggestions yet.<br>Select text and choose Add comment.';
    reviewSidebarBody.appendChild(empty);
    reviewLayout = [];
    reviewCardCache.clear();
    return;
  }

  // Build (or reuse) each item's card, but do not mount them all: compute every
  // card's stacked document-scroll position once here, then mount only the
  // viewport window (`mountReviewWindow`). A collapsed card whose content
  // signature is unchanged is reused from `reviewCardCache` without rebuilding
  // or re-measuring; the active/expanded card and the composer always rebuild so
  // their live controls stay fresh (REVIEW-GAP-020).
  const built = [];
  reviewAnchorIndex = [];
  for (const item of items) {
    const itemId = item.type === "composer" ? "composer" : `${item.type}:${item.data.id}`;
    const anchor = item.data?.anchor;
    if (item.type !== "composer" && anchor?.node) {
      reviewAnchorIndex.push({
        itemId,
        node: anchor.node,
        start: Number(anchor.start) || 0,
        end: Number(anchor.end) || Number(anchor.start) || 0,
      });
    }
    const expanded = activeReviewItemId === itemId;
    const sig = reviewCardSignature(item, comments);
    let entry = reviewCardCache.get(itemId);
    if (entry && entry.sig === sig && item.type !== "composer" && !expanded) {
      built.push({ itemId, item, entry });
      continue;
    }
    if (item.type === "composer") {
      const composer = document.createElement("article");
      composer.className = "review-margin-card review-margin-composer expanded";
      composer.setAttribute("role", "group");
      composer.setAttribute("aria-label", "Add comment");
      const textarea = document.createElement("textarea");
      textarea.rows = 3;
      textarea.maxLength = 4096;
      textarea.placeholder = "Add a comment…";
      textarea.dataset.testid = "review-comment-composer";
      const actions = document.createElement("div");
      actions.className = "review-composer-actions";
      const cancel = reviewCardButton("Cancel", () => closeReviewPopover());
      cancel.dataset.testid = "review-comment-cancel";
      const submit = reviewCardButton("Comment", async () => {
        const text = textarea.value.trim();
        if (!text || !reviewComposerState?.range) return;
        const { start, end } = reviewComposerState.range;
        const metadata = currentReviewTimestamp();
        await runEdit(() => doc.addComment(start.node, start.offset, end.offset, text, undefined, undefined, metadata.date));
        reviewComposerState = null;
        reviewSidebarPreference = true;
        announceReview("Comment added");
        drawSelection();
      }, false, "Add comment");
      submit.dataset.testid = "review-comment-submit";
      textarea.addEventListener("keydown", (event) => {
        event.stopPropagation();
        if (event.key === "Escape") {
          event.preventDefault();
          closeReviewPopover();
        } else if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
          event.preventDefault();
          submit.click();
        }
      });
      actions.append(cancel, submit);
      composer.append(textarea, actions);
      entry = { el: composer, sig, height: 0, measured: false, focusTextarea: textarea, needsFocus: true };
      reviewCardCache.set(itemId, entry);
      built.push({ itemId, item, entry });
      continue;
    }

    const card = document.createElement("article");
    const revisionKind = item.type === "revision" ? ` review-margin-${String(item.data.kind || "change").replaceAll("_", "-")}` : "";
    card.className = `review-margin-card review-margin-${item.type}${revisionKind}${item.data.resolved ? " resolved" : ""}${expanded ? " expanded" : ""}`;
    card.tabIndex = 0;
    card.dataset.reviewItemId = itemId;
    card.setAttribute("aria-expanded", String(expanded));
    // A labelled, expandable group so a screen reader announces what the card is
    // (who, what kind, a text snippet) and its expanded/collapsed state, instead
    // of a nameless generic article (REVIEW-GAP-023). `group` (not `button`)
    // because an expanded card contains real action buttons.
    card.setAttribute("role", "group");
    card.setAttribute("aria-label", reviewCardAriaLabel(item));
    const header = document.createElement("div");
    header.className = "review-margin-card-head";
    const avatar = document.createElement("span");
    avatar.className = "review-margin-avatar";
    const authorName = item.data.author || item.data.initials || "You";
    avatar.textContent = authorName.trim().slice(0, 1).toUpperCase() || "U";
    // Per-author color (docs/81 REVIEW-GAP-015): the avatar chip is filled with
    // the author's stable palette color so the same reviewer is recognizable
    // across their comments and tracked changes, matching the inline markers.
    const authorColor = reviewAuthorColor(reviewAuthorKey(item.data));
    avatar.style.setProperty("--review-author-color", authorColor);
    // Attribution tooltip on the whole card: author · type · date for a tracked
    // change, author · state · date for a comment (reuses the native `title`
    // tooltip pattern used across the editor's chrome).
    card.title = item.type === "revision"
      ? reviewRevisionTooltip(item.data)
      : reviewCommentTooltip(item.data);
    const title = document.createElement("div");
    title.className = "review-margin-title";
    const author = document.createElement("strong");
    author.textContent = authorName;
    const meta = document.createElement("small");
    meta.textContent = item.type === "revision"
      ? formatReviewDate(item.data.date)
      : `${item.data.resolved ? "Resolved · " : ""}${formatReviewDate(item.data.date)}`;
    title.append(author, meta);
    header.append(avatar, title);
    if (expanded && item.type === "comment") {
      header.append(
        reviewIconButton(item.data.resolved ? "undo" : "check", item.data.resolved ? "Reopen comment" : "Resolve comment", async () => {
          const resolving = !item.data.resolved;
          await runEdit(() => doc.setCommentResolved(item.data.id, resolving));
          announceReview(resolving ? "Comment resolved" : "Comment reopened");
          if (resolving) {
            activeReviewItemId = null;
            activeReviewCommentId = null;
            reviewDeleteConfirmId = null;
            if (item.data.anchor?.node) {
              const end = Number(item.data.anchor.end) || Number(item.data.anchor.start) || 0;
              selection = {
                anchor: { node: item.data.anchor.node, offset: end },
                focus: { node: item.data.anchor.node, offset: end },
              };
            }
          }
          drawSelection();
        }),
        reviewIconButton("more_vert", "More options", () => {
          reviewDeleteConfirmId = reviewDeleteConfirmId === item.data.id ? null : item.data.id;
          scheduleReviewMarginRender();
        }),
      );
    }
    const body = document.createElement("p");
    body.className = "review-margin-body";
    if (item.type === "revision") {
      if (item.data.kind === "replacement") {
        body.textContent = `Replaced “${item.data.oldText}” with “${item.data.newText}”`;
      } else if (item.data.kind === "formatting") {
        const target = item.data.text || item.data.newText || item.data.oldText;
        const details = reviewFormattingDescription(item.data.formattingDelta);
        body.textContent = `Changed formatting for “${target}”${details ? `\n${details}` : ""}`;
      } else if (item.data.kind === "move") {
        body.textContent = `Moved “${item.data.newText || item.data.oldText}”`;
      } else {
        const verb = item.data.kind === "deletion"
          ? "Deleted"
          : item.data.kind === "insertion"
            ? "Added"
            : item.data.kind === "move_from"
              ? "Unpaired move source"
              : item.data.kind === "move_to"
                ? "Unpaired move destination"
                : "Changed";
        body.textContent = `${verb} “${String(item.data.text || "")}”`;
      }
    } else {
      body.textContent = String(item.data.text || "");
    }
    const actions = document.createElement("div");
    actions.className = "review-margin-card-actions";
    if (item.type === "comment") {
      if (reviewDeleteConfirmId === item.data.id) {
        const prompt = document.createElement("span");
        prompt.textContent = "Delete this thread?";
        actions.append(
          prompt,
          reviewCardButton("Delete", async () => {
            reviewDeleteConfirmId = null;
            activeReviewItemId = null;
            await runEdit(() => doc.deleteComment(item.data.id));
            drawSelection();
          }, true),
          reviewCardButton("Cancel", () => {
            reviewDeleteConfirmId = null;
            scheduleReviewMarginRender();
          }),
        );
      }
    } else if (!["move_from", "move_to"].includes(item.data.kind) || item.data.movePair) {
      const changeLabel = (reviewChangeTypeLabel(item.data.kind) || "change").toLowerCase();
      actions.append(
        reviewCardButton("Accept", async () => {
          await runEdit(() => item.data.movePair
            ? doc.decideMovePair(
              item.data.movePair.fromStart,
              item.data.movePair.toStart,
              true,
            )
            : item.data.groupId
              ? doc.decideRevisionGroup(item.data.groupId, true)
              : doc.decideRevision(item.data.id, true));
          announceReview(`Accepted ${changeLabel}`);
          drawSelection();
          focusEditorSurface();
        }, false, `Accept this ${changeLabel}`),
        reviewCardButton("Reject", async () => {
          await runEdit(() => item.data.movePair
            ? doc.decideMovePair(
              item.data.movePair.fromStart,
              item.data.movePair.toStart,
              false,
            )
            : item.data.groupId
              ? doc.decideRevisionGroup(item.data.groupId, false)
              : doc.decideRevision(item.data.id, false));
          announceReview(`Rejected ${changeLabel}`);
          drawSelection();
          focusEditorSurface();
        }, true, `Reject this ${changeLabel}`),
      );
    }
    const focus = () => {
      const expanding = activeReviewItemId !== itemId;
      activeReviewItemId = expanding ? itemId : null;
      reviewDeleteConfirmId = null;
      if (item.type === "comment") {
        if (expanding) {
          focusReviewComment(item.data, false);
        } else if (item.data.resolved && item.data.anchor?.node) {
          activeReviewCommentId = null;
          const end = Number(item.data.anchor.end) || Number(item.data.anchor.start) || 0;
          selection = {
            anchor: { node: item.data.anchor.node, offset: end },
            focus: { node: item.data.anchor.node, offset: end },
          };
          drawSelection();
        }
      } else {
        focusReviewRevision(item.data, false);
      }
      scheduleReviewMarginRender();
    };
    card.addEventListener("click", (event) => {
      if (event.target.closest("button, textarea, input, select")) return;
      focus();
    });
    card.addEventListener("keydown", (event) => {
      // Let inner controls (Accept/Reject, the move source/destination
      // navigation buttons, composers) handle their own Enter/Space instead
      // of also toggling the card's expansion — mirrors the click guard above.
      if (event.target.closest("button, textarea, input, select")) return;
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        focus();
      }
    });
    card.append(header, body);
    if (item.type === "revision" && item.data.kind === "move") {
      const path = document.createElement("div");
      path.className = "review-move-path";
      const source = reviewMoveEndButton(
        "From",
        item.data.anchor,
        "Original location",
        "Go to the moved text's original location",
      );
      const arrow = document.createElement("span");
      arrow.className = "ms";
      arrow.setAttribute("aria-hidden", "true");
      arrow.textContent = "arrow_forward";
      const destination = reviewMoveEndButton(
        "To",
        item.data.destinationAnchor,
        "New location",
        "Go to the moved text's new location",
      );
      path.append(source, arrow, destination);
      card.appendChild(path);
    }
    // REVIEW-GAP-012: thread comments that overlap this tracked change beneath
    // it, read-only, on an expanded revision card. `revisionThread` reports the
    // overlap; authoring a reply to a change is `addComment` over the change's
    // own range (unchanged DOCX comment ownership). Only rendered when the
    // overlap is non-empty, so a change with no related comment adds no chrome.
    if (item.type === "revision" && expanded && item.data.id) {
      let related = [];
      try {
        related = JSON.parse(doc.revisionThread(item.data.id) || "[]");
      } catch {
        related = [];
      }
      if (related.length) {
        const relatedList = document.createElement("div");
        relatedList.className = "review-margin-revision-comments";
        const relatedLabel = document.createElement("small");
        relatedLabel.textContent = "Related comments";
        relatedList.appendChild(relatedLabel);
        for (const related_comment of related) {
          const relatedItem = document.createElement("div");
          relatedItem.className = "review-margin-revision-comment";
          const relatedAuthor = document.createElement("strong");
          relatedAuthor.textContent =
            related_comment.author || related_comment.initials || "You";
          const relatedText = document.createElement("p");
          relatedText.textContent = String(related_comment.text || "");
          relatedItem.append(relatedAuthor, relatedText);
          relatedList.appendChild(relatedItem);
        }
        card.appendChild(relatedList);
      }
    }
    if (item.type === "comment") {
      const replies = comments.filter((comment) =>
        comment.parentParaId === item.data.paraId || comment.parentParaId === item.data.id);
      if (replies.length) {
        const thread = document.createElement("div");
        thread.className = "review-margin-replies";
        for (const reply of replies) {
          const replyItem = document.createElement("div");
          replyItem.className = `review-margin-reply${reply.resolved ? " resolved" : ""}`;
          const replyHead = document.createElement("div");
          replyHead.className = "review-margin-reply-head";
          const replyAuthor = document.createElement("strong");
          replyAuthor.textContent = reply.author || reply.initials || "You";
          const replyDate = document.createElement("small");
          replyDate.textContent = `${reply.resolved ? "Resolved · " : ""}${formatReviewDate(reply.date)}`;
          replyHead.append(replyAuthor, replyDate);
          const replyBody = document.createElement("p");
          replyBody.textContent = String(reply.text || "");
          replyItem.append(replyHead, replyBody);
          // REVIEW-GAP-011: edit or delete this specific reply (not just the
          // whole thread). Only on an expanded, unresolved parent card, so the
          // controls stay out of the collapsed/resolved presentation.
          if (expanded && !item.data.resolved) {
            const replyActionRow = document.createElement("div");
            replyActionRow.className = "review-margin-reply-actions";
            const editReply = reviewCardButton("Edit", () => {
              const input = document.createElement("input");
              input.type = "text";
              input.className = "review-margin-reply-edit";
              input.setAttribute("aria-label", "Edit reply text");
              input.maxLength = 4096;
              input.value = String(reply.text || "");
              const save = reviewCardButton("Save", async () => {
                const text = input.value.trim();
                if (!text) return;
                await runEdit(() => doc.updateComment(reply.id, text));
                announceReview("Reply updated");
                drawSelection();
              }, false, "Save reply");
              const cancelEdit = reviewCardButton("Cancel", () => {
                scheduleReviewMarginRender();
              });
              const editActions = document.createElement("div");
              editActions.className = "review-composer-actions";
              editActions.append(cancelEdit, save);
              input.addEventListener("click", (event) => event.stopPropagation());
              input.addEventListener("keydown", (event) => {
                event.stopPropagation();
                if (event.key === "Enter") {
                  event.preventDefault();
                  save.click();
                } else if (event.key === "Escape") {
                  event.preventDefault();
                  cancelEdit.click();
                }
              });
              replyBody.replaceWith(input);
              replyActionRow.replaceWith(editActions);
              input.focus({ preventScroll: true });
            });
            const deleteReply = reviewCardButton(
              "Delete",
              async () => {
                await runEdit(() => doc.deleteReply(reply.id));
                announceReview("Reply deleted");
                drawSelection();
              },
              true,
              "Delete reply",
            );
            replyActionRow.append(editReply, deleteReply);
            replyItem.appendChild(replyActionRow);
          }
          thread.appendChild(replyItem);
        }
        card.appendChild(thread);
      }
      if (expanded && !item.data.resolved) {
        const replyComposer = document.createElement("div");
        replyComposer.className = "review-reply-composer";
        const textarea = document.createElement("input");
        textarea.type = "text";
        textarea.maxLength = 4096;
        textarea.readOnly = true;
        textarea.placeholder = "Reply…";
        textarea.setAttribute("aria-label", "Reply to this comment");
        const submit = reviewCardButton("Reply", async () => {
          const text = textarea.value.trim();
          if (!text) return;
          const metadata = currentReviewTimestamp();
          await runEdit(() => doc.replyToComment(item.data.id, text, undefined, undefined, metadata.date));
          announceReview("Reply added");
          drawSelection();
        }, false, "Send reply");
        const cancel = reviewCardButton("Cancel", () => {
          textarea.value = "";
          textarea.readOnly = true;
          replyActions.hidden = true;
        });
        const replyActions = document.createElement("div");
        replyActions.className = "review-composer-actions";
        replyActions.hidden = true;
        replyActions.append(cancel, submit);
        textarea.addEventListener("click", (event) => {
          event.stopPropagation();
          textarea.readOnly = false;
          replyActions.hidden = false;
          textarea.focus({ preventScroll: true });
        });
        textarea.addEventListener("keydown", (event) => {
          event.stopPropagation();
          if (event.key === "Enter") {
            event.preventDefault();
            submit.click();
          } else if (event.key === "Escape") {
            event.preventDefault();
            cancel.click();
          }
        });
        replyComposer.append(textarea, replyActions);
        card.appendChild(replyComposer);
      }
    }
    if (actions.childElementCount) card.appendChild(actions);
    entry = { el: card, sig, height: 0, measured: false };
    reviewCardCache.set(itemId, entry);
    built.push({ itemId, item, entry });
  }

  // Measure only the freshly built cards, batched: mount them all off-view, read
  // every height (one layout for all reads, since no DOM mutation interleaves),
  // then detach them. Reused cards keep their cached measured height, so a large
  // review set is not re-measured on every edit.
  const toMeasure = built.filter(({ entry }) => !entry.measured);
  for (const { entry } of toMeasure) {
    entry.el.style.top = "-99999px";
    reviewSidebarBody.appendChild(entry.el);
  }
  for (const { entry } of toMeasure) {
    entry.height = entry.el.offsetHeight;
    entry.measured = true;
  }
  for (const { entry } of toMeasure) reviewSidebarBody.removeChild(entry.el);

  // Stacked-chip surfacing (REVIEW-GAP-019): when several chips collide on one
  // paragraph (same anchor Y), the active/clicked one claims true anchor
  // alignment and the others stack below it — the Google Docs pattern. Reorder
  // the active card to the front of its own collision cluster so the top-down
  // position pass places it at its anchor top; cross-cluster document order and
  // every other card's relative order are preserved.
  if (activeReviewItemId) {
    const activeIdx = built.findIndex((b) => b.itemId === activeReviewItemId);
    if (activeIdx > 0) {
      const activeTop = Math.round(built[activeIdx].item.rect.top);
      let clusterStart = activeIdx;
      for (let i = 0; i < activeIdx; i++) {
        if (Math.round(built[i].item.rect.top) === activeTop) { clusterStart = i; break; }
      }
      if (clusterStart < activeIdx) {
        const [active] = built.splice(activeIdx, 1);
        built.splice(clusterStart, 0, active);
      }
    }
  }

  // Position pass: stack every card in document-scroll coordinates exactly as
  // the non-virtualized layout did (anchor top, pushed down to clear the card
  // above), using the measured heights.
  let nextY = 8;
  const seen = new Set();
  const layout = [];
  for (const { itemId, item, entry } of built) {
    seen.add(itemId);
    const targetY = item.rect.top - viewportRect.top + viewportEl.scrollTop;
    const y = Math.max(8, targetY, nextY);
    nextY = y + entry.height + 8;
    layout.push({ itemId, top: y, entry });
  }
  // Drop cache entries (and their retained DOM) for items no longer present.
  for (const key of [...reviewCardCache.keys()]) {
    if (!seen.has(key)) reviewCardCache.delete(key);
  }
  reviewLayout = layout;
  mountReviewWindow();
}

/** A stable string that changes whenever anything affecting a card's rendered
 *  DOM or measured height changes, so `reviewCardCache` reuses a card only when
 *  it would render identically. The composer is never cached (always rebuilt so
 *  its live textarea/focus stays correct). */
function reviewCardSignature(item, comments) {
  if (item.type === "composer") return "composer";
  const d = item.data;
  const itemId = `${item.type}:${d.id}`;
  const expanded = activeReviewItemId === itemId;
  const confirm = reviewDeleteConfirmId === d.id;
  const replies = item.type === "comment"
    ? comments
      .filter((c) => c.parentParaId === d.paraId || c.parentParaId === d.id)
      .map((r) => `${r.id}${r.resolved ? 1 : 0}${r.text}${r.author}${r.date}`)
      .join("")
    : "";
  return JSON.stringify([
    item.type, d.id, d.kind || "", expanded ? 1 : 0, d.resolved ? 1 : 0, confirm ? 1 : 0,
    d.text || "", d.oldText || "", d.newText || "", d.author || "", d.initials || "", d.date || "",
    d.groupId || "", d.movePair ? 1 : 0,
    Array.isArray(d.formattingDelta) ? d.formattingDelta : 0,
    d.anchor ? `${d.anchor.node}:${d.anchor.start}:${d.anchor.end}` : "",
    replies,
  ]);
}

/** Mounts only the cards whose precomputed position falls inside (or within
 *  `REVIEW_WINDOW_OVERSCAN` of) the viewport, and detaches the rest. Runs after
 *  a full render and on every scroll frame; it never re-parses the review
 *  payload, recomputes geometry, or rebuilds a card — it only attaches/detaches
 *  retained DOM by comparing cached positions to the current scroll band. This
 *  is what keeps the mounted card count bounded regardless of review size
 *  (REVIEW-GAP-020). */
function mountReviewWindow() {
  if (reviewSidebar.hidden) return;
  const scrollTop = viewportEl.scrollTop;
  const bandTop = scrollTop - REVIEW_WINDOW_OVERSCAN;
  const bandBottom = scrollTop + viewportEl.clientHeight + REVIEW_WINDOW_OVERSCAN;
  for (const { itemId, top, entry } of reviewLayout) {
    // The composer and the active/expanded card are always kept mounted: they
    // own live focus/controls the user is interacting with.
    const force = itemId === "composer" || itemId === activeReviewItemId;
    const visible = force || (top + entry.height >= bandTop && top <= bandBottom);
    const mounted = entry.el.parentNode === reviewSidebarBody;
    if (visible) {
      entry.el.style.top = `${Math.round(top)}px`;
      if (!mounted) reviewSidebarBody.appendChild(entry.el);
      if (entry.needsFocus && entry.focusTextarea) {
        entry.needsFocus = false;
        const textarea = entry.focusTextarea;
        requestAnimationFrame(() => textarea.focus({ preventScroll: true }));
      }
    } else if (mounted) {
      reviewSidebarBody.removeChild(entry.el);
    }
  }
}

/** rAF-debounced `mountReviewWindow` for the scroll path: re-windowing is cheap
 *  (no parse/geometry/rebuild), but coalescing to one call per frame keeps a
 *  fast scroll from doing redundant DOM work. */
function scheduleReviewWindow() {
  if (reviewWindowFrame) return;
  reviewWindowFrame = requestAnimationFrame(() => {
    reviewWindowFrame = 0;
    mountReviewWindow();
  });
}

// One scroll owner: the comment layer lives inside `.viewport` and rides its
// scroll context natively, so cards stay pinned to their anchored text without
// any scroll-sync or per-frame re-render (that eliminates the momentum drift and
// the between-canvas scrollbar). Cards are positioned in document-scroll
// coordinates; only content edits and resizes recompute those positions. On a
// plain scroll we only re-window the already-computed layout (mount the cards
// entering the viewport, detach those leaving it) — no re-parse, no geometry,
// no rebuild — so a document with hundreds of comments scrolls with a bounded,
// viewport-sized number of mounted cards (REVIEW-GAP-020).
viewportEl.addEventListener("scroll", scheduleReviewWindow, { passive: true });
reviewSidebarBody.addEventListener("click", (event) => {
  if (event.target !== reviewSidebarBody || !activeReviewItemId) return;
  activeReviewItemId = null;
  activeReviewCommentId = null;
  reviewDeleteConfirmId = null;
  drawSelection();
});
window.addEventListener("resize", scheduleReviewMarginRender);
const linkChip = document.getElementById("linkChip");
const linkChipKind = document.getElementById("linkChipKind");
const linkChipTarget = document.getElementById("linkChipTarget");
const linkChipAction = document.getElementById("linkChipAction");
const linkChipEdit = document.getElementById("linkChipEdit");
const linkChipRemove = document.getElementById("linkChipRemove");
const indentDecBtn = document.getElementById("indentDec");
const indentIncBtn = document.getElementById("indentInc");
const bulletListBtn = document.getElementById("bulletList");
const numberedListBtn = document.getElementById("numberedList");
const restartListBtn = document.getElementById("restartList");
const continueListBtn = document.getElementById("continueList");
const fontFamilySel = document.getElementById("fontFamily");
const paragraphStyleSel = document.getElementById("paragraphStyle");
const runControls = [superBtn, subBtn, fontSizeSel, textColorInput, highlightSel, fontFamilySel, clearFormattingBtn];
const paraControls = [
  ...Object.values(alignBtns),
  spacingBtn,
  paraOptsBtn,
  indentDecBtn,
  indentIncBtn,
  bulletListBtn,
  numberedListBtn,
  restartListBtn,
  continueListBtn,
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
/** Current object selection (docs/85 §3.2 `Selection::Object`): a drawing/image/
 * text box selected as a unit, distinct from the text caret/range in `selection`.
 * `mode` is the interaction-grammar state (§4): "selected" shows the outline +
 * handles; "editing" is inside a container object's content. `null` = no object
 * selected. `selection` still holds a caret at the object's surrounding-text
 * anchor so a two-step Escape can collapse back to it. */
let objectSelection = null; // { node, kind, mode: "selected" | "editing" } | null
/** Current table-cell selection overlay, separate from text ranges. */
let tableSelection = null; // { node, mode: "row" | "column" | "table" }
let tableResizeDrag = null; // { node, col, page, startClientX, startWidthTwips, preview }
let dragging = false;
/** Primary-pointer gesture retained until pointerup so link activation is
 * suppressed after a drag/Shift extension. */
let pointerGesture = null;
let selectionAutoScrollFrame = 0;
let chromeRefreshFrame = 0;
let chromeRefreshStats = false;
let chromeRefreshOutline = false;
let chromeRefreshA11y = false;
/** The model-derived link currently represented by the host-owned link chip. */
let activeLink = null;
/** One-frame throttle for pointer feedback over canvas-painted link geometry. */
let linkHoverFrame = 0;
let pendingLinkHover = null;
/** Armed run formatting for typing at a collapsed caret (e.g. click Bold with no
 *  selection → next typed characters are bold). `null` when nothing is armed; else
 *  a subset of { bold, italic, underline, strike } → boolean. Cleared whenever the
 *  caret moves for any reason other than the typing that consumes it. */
let pendingFormat = null;
/** The open document's filename, for the Save download. */
let currentName = "document.docx";
/** True while an IME composition is active on the canvas editor surface. */
let composingText = false;
/** Host gesture identity for history coalescing. The engine also validates exact
 * caret continuity, so this id is permission to merge, never the sole criterion. */
let typingSession = 0;
let typingSessionActive = false;
let lastTypingAt = 0;
const TYPING_PAUSE_MS = 1000;
const EDITOR_KEYBOARD_PLATFORM = keyboardPlatform(navigator);

function focusEditorSurface() {
  pagesEl.focus({ preventScroll: true });
}

function resetPointerGesture() {
  pointerGesture = null;
  dragging = false;
  if (selectionAutoScrollFrame) cancelAnimationFrame(selectionAutoScrollFrame);
  selectionAutoScrollFrame = 0;
}

function isInteractiveChromeTarget(target) {
  if (!(target instanceof Element)) return false;
  return !!target.closest(
    "input, select, textarea, button, [contenteditable='true'], .context-menu, .settings-panel, .cmd-overlay, .find-panel, .link-chip",
  );
}

function eventTargetsEditor(event) {
  return event.target === pagesEl || pagesEl.contains(event.target) || document.activeElement === pagesEl;
}

function clientPointEvent(clientX, clientY) {
  return { clientX, clientY };
}

function setStatus(text, kind = "") {
  statusEl.textContent = text;
  statusEl.className = `status ${kind}`;
}

// Concise polite announcements for review events (comment added, change
// accepted/rejected, bulk decisions, filter/mode changes) to the review live
// region (REVIEW-GAP-023). Re-announcing the same string still fires by briefly
// clearing the node first, so repeated identical actions (e.g. two accepts) are
// each spoken. Never moves focus.
function announceReview(text) {
  if (!reviewLiveRegion || !text) return;
  reviewLiveRegion.textContent = "";
  // A microtask gap makes assistive tech treat the new text as a fresh change.
  requestAnimationFrame(() => {
    reviewLiveRegion.textContent = text;
  });
}

function scheduleChromeRefresh({ stats = false, outline = false, a11y = false } = {}) {
  chromeRefreshStats ||= stats;
  chromeRefreshOutline ||= outline;
  // The off-screen accessibility tree mirrors document structure, so it is
  // rebuilt on the same content-changed triggers as the outline.
  chromeRefreshA11y ||= a11y || outline;
  if (chromeRefreshFrame) return;
  chromeRefreshFrame = requestAnimationFrame(() => {
    chromeRefreshFrame = 0;
    const refreshStats = chromeRefreshStats;
    const refreshOutline = chromeRefreshOutline;
    const refreshA11y = chromeRefreshA11y;
    chromeRefreshStats = false;
    chromeRefreshOutline = false;
    chromeRefreshA11y = false;
    if (refreshStats) updateStats();
    if (refreshOutline) buildOutline();
    if (refreshA11y) buildAccessibilityTree();
  });
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
    return;
  }

  const params = new URLSearchParams(window.location.search);
  // The public demo and plain editor both open sample.docx. The smaller rich
  // corpus remains available only through the explicit e2e fixture route,
  // whose specs assert on its exact content. ?blank=1 opts out for the bare
  // local-upload state.
  if (params.get("fixture") === "rich") {
    await loadStartupDocument("./demo.docx", "opendoc-demo.docx");
  } else if (params.get("blank") !== "1") {
    await loadStartupDocument("./sample.docx", "sample.docx");
  }
}

async function loadStartupDocument(url, name) {
  try {
    setStatus("Loading the sample document…");
    const response = await fetch(url);
    if (!response.ok) throw new Error(`sample request returned ${response.status}`);
    await openBytes(new Uint8Array(await response.arrayBuffer()), name);
  } catch (err) {
    console.error(err);
    setStatus("The sample could not be loaded — you can still open a local DOCX", "error");
  }
}

async function openBytes(bytes, name) {
  try {
    setStatus(`Opening ${name}…`);
    hideLinkChip();
    clearLinkHover();
    // A previous document's memory is freed when it is dropped; replace it.
    if (doc) doc.free();
    doc = open(bytes);
    applyActiveAuthorToDocument();
    selection = null;
    tableSelection = null;
    reviewMode = "editing";
    suggestingBanner.hidden = true;
    if (viewingBanner) viewingBanner.hidden = true;
    reviewSidebarPreference = null;
    activeReviewCommentId = null;
    activeReviewItemId = null;
    reviewComposerState = null;
    reviewDeleteConfirmId = null;
    // A new document invalidates every retained card and its cached geometry.
    reviewLayout = [];
    reviewCardCache.clear();
    for (const button of reviewModeButtons) {
      button.setAttribute("aria-pressed", String(button.dataset.reviewMode === reviewMode));
    }
    breakTypingSession();
    currentName = name;
    docTitleEl.value = name;
    docTitleEl.hidden = false;
    titleDividerEl.hidden = false;
    saveBtn.disabled = false;
    railOutline.disabled = false;
    populateStyles();
    populateTableStyles();
    dropEl.hidden = true;
    document.body.classList.add("doc-loaded");
    const fontWarnings = await provisionFonts(name);
    await renderAll();
    if (fontWarnings.length > 0) {
      setStatus(
        `Opened ${name}; unavailable web fonts: ${[...new Set(fontWarnings)].join(", ")}`,
        "error",
      );
    }
    buildOutline();
    buildAccessibilityTree();
    drawSelection();
  } catch (err) {
    console.error(err);
    setStatus(`Could not open ${name}: ${err.message ?? err}`, "error");
  }
}

// ---- Document rename ---------------------------------------------------------
// The header title is the input itself (styled as plain text until focused),
// matching the Word/Docs "click the title to rename" convention. Renaming
// only changes `currentName` (what Save downloads as); it never touches the
// open document's own model or its docProps/core.xml `dc:title`.
function commitRename() {
  const trimmed = docTitleEl.value.trim();
  if (!trimmed) {
    docTitleEl.value = currentName;
    return;
  }
  const named = /\.docx$/i.test(trimmed) ? trimmed : `${trimmed}.docx`;
  currentName = named;
  docTitleEl.value = named;
}

docTitleEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    docTitleEl.blur();
  } else if (e.key === "Escape") {
    e.preventDefault();
    docTitleEl.value = currentName;
    docTitleEl.blur();
  }
});
docTitleEl.addEventListener("focus", () => docTitleEl.select());
docTitleEl.addEventListener("blur", commitRename);

// Provision the host-owned named families in one bounded batch/repagination,
// then fetch only the script fallbacks this document's uncovered code points
// require. Network failures do not block opening: the target-bundled
// metric-compatible faces remain available.
async function provisionFonts(name) {
  if (!doc) return [];
  const warnings = [];
  setStatus(`Fetching web fonts for ${name}…`);

  const named = await Promise.allSettled(
    NAMED_WEB_FONT_FACES.map((face) => fetchFontBytes(face.url, fontCache)),
  );
  const namedBytes = named
    .filter((result) => result.status === "fulfilled")
    .map((result) => result.value);
  if (namedBytes.length > 0) {
    const packed = packFontBytes(namedBytes);
    doc.registerFonts(packed.bytes, packed.lengths);
  }
  for (const [index, result] of named.entries()) {
    if (result.status === "rejected") {
      const face = NAMED_WEB_FONT_FACES[index];
      console.warn(`font ${face.family} (${face.url}) failed:`, result.reason);
      warnings.push(face.family);
    }
  }

  const missing = doc.missingCoverage();
  const keys = fallbackKeysFor(missing);
  if (keys.length === 0) return warnings;

  setStatus(`Fetching fonts for ${name} (${[...keys].join(", ")})…`);
  for (const key of keys) {
    const { url, scripts } = SCRIPT_FALLBACK_FONTS[key];
    try {
      const bytes = await fetchFontBytes(url, fontCache);
      doc.registerFallbackFont(bytes, scripts); // registers + re-paginates
    } catch (err) {
      console.warn(`font ${key} (${url}) failed:`, err);
      setStatus(`Could not load the ${key} font — some text may show as ▯`, "error");
      warnings.push(key);
    }
  }
  return warnings;
}

async function renderAll() {
  if (!doc) return;
  clearFindParagraphCache();
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
    // Clamp edge/gap clicks into the page box. The layout hit tester then
    // resolves the nearest caret on that page instead of returning no hit for
    // a point just outside the rasterized sheet.
    x: Math.max(0, Math.min(page.wTwip, Math.round((event.clientX - rect.left) / sx))),
    y: Math.max(0, Math.min(page.hTwip, Math.round((event.clientY - rect.top) / sy))),
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

/** Reads the current comment list once (JSON round-trip), or `[]` on
 *  failure. Shared by marker rendering and by the caret-driven activation
 *  below so both agree on the same anchors. */
function reviewComments() {
  if (!doc) return [];
  try { return JSON.parse(doc.listComments()) ?? []; } catch { return []; }
}

/** The comment (if any) whose anchor range contains `anchor` (a `{node,
 *  offset}` model position), matching the same open/resolved visibility rule
 *  `paintReviewMarkers` uses. Resolved comments only match while their card
 *  is already explicitly expanded. */
function reviewCommentAtAnchor(anchor) {
  if (!anchor?.node) return null;
  for (const comment of reviewComments()) {
    const explicitlyOpen = activeReviewItemId === `comment:${comment.id}`;
    if ((comment.resolved && !explicitlyOpen) || !comment.anchor?.node) continue;
    if (comment.anchor.node !== anchor.node) continue;
    const start = Number(comment.anchor.start) || 0;
    const end = Number(comment.anchor.end) || 0;
    if (anchor.offset >= start && anchor.offset <= end) return comment;
  }
  return null;
}

/** Non-blocking side effect of an authoritative caret placement
 *  (REVIEW-GAP-005, docs/81): if the resulting caret lands inside a
 *  commented range, expand/surface that comment's card. This never touches
 *  `selection` — document hit-testing (docs/80) is always the sole source
 *  of truth for where the caret lands; card expansion is derived from the
 *  resulting caret, never the other way around, and never blocks or delays
 *  caret placement. */
function syncActiveReviewCommentToCaret(anchor) {
  const comment = reviewCommentAtAnchor(anchor);
  if (comment) {
    activeReviewCommentId = comment.id;
    activeReviewItemId = `comment:${comment.id}`;
    reviewSidebarPreference = true;
    scheduleReviewMarginRender();
    return;
  }
  // No comment under the caret — surface a tracked-change card whose anchor range
  // contains the caret, so caret-driven expansion works for suggestions too
  // (REVIEW-GAP-019). The smallest containing range wins when several stack.
  if (!anchor?.node) return;
  const offset = Number(anchor.offset) || 0;
  let best = null;
  for (const entry of reviewAnchorIndex) {
    if (entry.node !== anchor.node) continue;
    if (offset < entry.start || offset > entry.end) continue;
    if (!best || entry.end - entry.start < best.end - best.start) best = entry;
  }
  if (!best || best.itemId === activeReviewItemId) return;
  activeReviewItemId = best.itemId;
  reviewSidebarPreference = true;
  scheduleReviewMarginRender();
}

/** Paint comment ranges in the existing overlay layer. This never touches the
 * canvas or document layout; it is the same interaction layer used by the
 * selection highlight and caret. These markers are a pure visual affordance:
 * they intentionally carry no click handler of their own, so a pointer event
 * always falls through to the page's normal pointerdown hit-testing (see
 * `onPointerDown` / `syncActiveReviewCommentToCaret`) instead of hijacking
 * caret placement (REVIEW-GAP-005). */
function paintReviewMarkers() {
  if (!doc) return;
  const summary = readReviewData(doc);
  for (const comment of summary.comments ?? []) {
    const explicitlyOpen = activeReviewItemId === `comment:${comment.id}`;
    if ((comment.resolved && !explicitlyOpen) || !comment.anchor?.node) continue;
    const { node, start, end } = comment.anchor;
    const rects = doc.selectionRects(node, Number(start) || 0, node, Number(end) || 0);
    const color = reviewAuthorColor(reviewAuthorKey(comment));
    const tooltip = reviewCommentTooltip(comment);
    for (let i = 0; i < rects.length; i += 5) {
      const el = place(rects.slice(i, i + 5), activeReviewCommentId === comment.id ? "review-comment-marker review-comment-marker-active" : "review-comment-marker");
      if (el) {
        el.dataset.reviewCommentId = comment.id;
        el.style.setProperty("--review-author-color", color);
        el.title = tooltip;
      }
    }
  }
  for (const revision of summary.revisions ?? []) {
    const range = revisionRange(revision);
    if (!range) continue;
    const deletionLike = revision.kind === "deletion" || revision.kind === "move_from";
    let rects = doc.selectionRects(range.startNode, range.startOffset, range.endNode, range.endOffset);
    if (rects.length < 5 && deletionLike && range.startNode === range.endNode) {
      rects = doc.caretRect(range.startNode, range.startOffset);
    }
    const moveItemId = revision.movePair?.fromStart && revision.movePair?.toStart
      ? `revision:move:${revision.movePair.fromStart}:${revision.movePair.toStart}`
      : null;
    const active = moveItemId && activeReviewItemId === moveItemId
      ? " review-revision-marker-active"
      : "";
    const kind = `${deletionLike
      ? "review-revision-marker review-deletion-marker"
      : "review-revision-marker review-insertion-marker"}${active}`;
    const color = reviewAuthorColor(reviewAuthorKey(revision));
    const tooltip = reviewRevisionTooltip(revision);
    for (let i = 0; i < rects.length; i += 5) {
      const el = place(rects.slice(i, i + 5), kind);
      if (el) {
        el.style.setProperty("--review-author-color", color);
        el.title = tooltip;
      }
    }
  }
}

/** Draws the current selection from engine geometry: a highlight for a real
 *  range, else a caret at the focus (so a click — or a range with no visible
 *  rects — always shows a cursor). */
function drawSelection() {
  if (!doc) return;
  clearOverlays();
  paintReviewMarkers();
  // A selected object owns the visible chrome (outline + handles) in place of the
  // text caret; in "editing" mode the ordinary text caret is shown instead.
  if (objectSelection && objectSelection.mode === "selected") {
    paintObjectSelection();
  } else if (selection) {
    paintTableSelection();
    paintActiveCell(selection.focus); // under the caret/highlight
    paintSelection(selection);
    paintTableResizeHandles(selection.focus);
  }
  updateObjectSelectionState();
  updateObjectContextBar();
  updateToolbar();
  updateReviewControls();
  scheduleReviewMarginRender();
  updatePageNumber();
  updateRulerMarkers();
  positionSelToolbar();
}

function paintTableSelection() {
  if (!tableSelection) return;
  const rects = doc.tableSelectionRects(tableSelection.node, tableSelection.mode);
  for (let i = 0; i + 4 < rects.length; i += 5) place(rects.slice(i, i + 5), "table-cell-selection");
}

/** Outlines the table cell the caret is in (nothing when not in a table), so the
 *  user always sees which cell they are editing. */
function paintActiveCell(focus) {
  const flat = doc.cellRect(focus.node); // [page, x, y, w, h] twips, or []
  if (flat.length >= 5) place(flat, "cell-outline");
}

/** Draws Word-style internal column resize handles for the active regular table. */
function paintTableResizeHandles(focus) {
  if (!doc?.inTable(focus.node)) return;
  const handles = doc.tableColumnResizeHandles(focus.node);
  for (let i = 0; i + 4 < handles.length; i += 5) {
    const [pageNumber, x, y, h, col] = handles.slice(i, i + 5);
    const page = pages[pageNumber - 1];
    if (!page) continue;
    const { sx, sy } = scaleOf(page);
    const el = document.createElement("div");
    el.className = "table-col-resize-handle";
    el.dataset.col = String(col);
    el.style.left = `${x * sx}px`;
    el.style.top = `${y * sy}px`;
    el.style.height = `${h * sy}px`;
    el.addEventListener("pointerdown", (event) => startTableColumnResize(event, page, focus.node, col));
    page.overlay.appendChild(el);
  }
}

/** Paints the selected object's outline + eight resize/move handles from engine
 *  geometry (docs/85 §3.3), the same overlay mechanism as the caret/highlight so
 *  the chrome matches the raster exactly. Handles are display-only this slice. */
function paintObjectSelection() {
  const { node } = objectSelection;
  place(doc.objectRect(node), "object-outline");
  const handles = doc.objectHandles(node); // [page, cx, cy, kind] * 8
  for (let i = 0; i + 3 < handles.length; i += 4) {
    const [pageNumber, cx, cy, kind] = handles.slice(i, i + 4);
    const page = pages[pageNumber - 1];
    if (!page) continue;
    const { sx, sy } = scaleOf(page);
    const el = document.createElement("div");
    el.className = "object-handle";
    el.dataset.handle = String(kind);
    el.style.left = `${cx * sx}px`;
    el.style.top = `${cy * sy}px`;
    page.overlay.appendChild(el);
  }
}

/** Reflects the object-selection state onto `#pages` as data attributes so the
 *  host (and tests) can observe the grammar state machine without reading into
 *  the overlay. */
function updateObjectSelectionState() {
  if (objectSelection) {
    pagesEl.dataset.objectSelected = objectSelection.node;
    pagesEl.dataset.objectKind = objectSelection.kind;
    pagesEl.dataset.objectMode = objectSelection.mode;
  } else {
    delete pagesEl.dataset.objectSelected;
    delete pagesEl.dataset.objectKind;
    delete pagesEl.dataset.objectMode;
  }
}

/** The lazily-created placeholder object context bar (docs/85 §4.1). */
let objectContextBarEl = null;

/** Shows/positions a placeholder context bar above a selected object, naming the
 *  object kind and the actions later slices will make real. Hidden when no object
 *  is selected. This is the §4.1 "context bar" seam; real `editorCommands(object)`
 *  descriptors are the P1G-OBJ-GRAMMAR command slice. */
function updateObjectContextBar() {
  if (!objectContextBarEl) {
    objectContextBarEl = document.createElement("div");
    objectContextBarEl.className = "object-context-bar";
    objectContextBarEl.hidden = true;
    document.body.appendChild(objectContextBarEl);
  }
  if (!objectSelection || objectSelection.mode !== "selected") {
    objectContextBarEl.hidden = true;
    return;
  }
  const rect = doc.objectRect(objectSelection.node); // [page, x, y, w, h]
  const page = rect.length >= 5 ? pages[rect[0] - 1] : null;
  if (!page) {
    objectContextBarEl.hidden = true;
    return;
  }
  const { rect: pageRect, sx, sy } = scaleOf(page);
  const label = objectSelection.kind === "textbox" ? "Text box" : "Image";
  const actions =
    objectSelection.kind === "textbox"
      ? "Edit text · Fill · Outline · Wrap · Delete"
      : "Replace · Crop · Alt text · Wrap · Delete";
  objectContextBarEl.innerHTML =
    `<strong>${label}</strong><small>${actions} (coming soon)</small>`;
  objectContextBarEl.hidden = false;
  // Position just above the object's top-left, clamped into the viewport.
  const left = pageRect.left + rect[1] * sx;
  const top = pageRect.top + rect[2] * sy - objectContextBarEl.offsetHeight - 8;
  objectContextBarEl.style.left = `${Math.max(8, left)}px`;
  objectContextBarEl.style.top = `${Math.max(8, top)}px`;
}

/** Selects an object as a unit (docs/85 §4.1). Keeps `selection` as a caret at
 *  the object's surrounding-text anchor so the two-step Escape can return to it. */
function selectObject(node, kind, anchor) {
  if (anchor) selection = { anchor, focus: anchor };
  objectSelection = { node, kind, mode: "selected" };
  pendingFormat = null;
  tableSelection = null;
  drawSelection();
}

/** Enters a container object's edit mode (docs/85 §4.3). A leaf object (image)
 *  has no edit mode — its primary context action is a later image slice. Placing
 *  a caret *inside* a text box's flowed body is the P1G-OBJ-TEXTBOX slice; here
 *  the grammar transitions state and the surrounding-text caret is shown. */
function enterObjectEditMode() {
  if (!objectSelection) return;
  if (objectSelection.kind !== "textbox") {
    setStatus("Image options (replace / crop / alt text) are a later editing slice");
    return;
  }
  objectSelection = { ...objectSelection, mode: "editing" };
  drawSelection();
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

/** The live IME composition overlay element, or `null` when no composition
 *  is in progress. Kept across `compositionupdate` calls so its text can be
 *  updated in place instead of recreated every keystroke. */
let imePreeditEl = null;

/** Shows the IME live-preedit overlay at a caret anchor: the in-progress
 * composition text the browser has not yet committed. Never touches the
 * document — `compositionend` still owns the actual insertion
 * (`commitComposedText`) — this only makes the intermediate state visible
 * (docs/67-EDITOR-UX-GAP-ANALYSIS.md, "IME live preedit"). */
function showImePreedit(node, offset, text) {
  hideImePreedit();
  const flat = doc?.caretRect(node, offset) ?? [];
  if (flat.length < 5) return;
  const [pageNumber, x, y, , h] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return;
  const { sx, sy } = scaleOf(page);
  const el = document.createElement("div");
  el.className = "ime-preedit";
  el.style.left = `${x * sx}px`;
  el.style.top = `${y * sy}px`;
  el.style.height = `${h * sy}px`;
  el.textContent = text;
  page.overlay.appendChild(el);
  imePreeditEl = el;
}

/** Updates the live-preedit overlay's text (a `compositionupdate` tick). */
function updateImePreedit(text) {
  if (imePreeditEl) imePreeditEl.textContent = text;
}

/** Removes the live-preedit overlay, if shown. */
function hideImePreedit() {
  imePreeditEl?.remove();
  imePreeditEl = null;
}

/** Places one flat `[page, x, y, w, h]` twip rect as a `kind` box on its page,
 *  converting twips → CSS px with that page's live scale. */
function place(flat, kind) {
  if (flat.length < 5) return null;
  const [pageNumber, x, y, w, h] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return null;
  const { sx, sy } = scaleOf(page);
  const el = document.createElement("div");
  el.className = kind;
  el.style.left = `${x * sx}px`;
  el.style.top = `${y * sy}px`;
  el.style.width = `${Math.max(w * sx, kind === "caret" ? 2 : 0)}px`;
  el.style.height = `${h * sy}px`;
  page.overlay.appendChild(el);
  return el;
}

/** Navigates to an internal-link target and makes the target page/caret visible. */
function navigateToAnchor(node, offset, pageNumber) {
  if (!node) return;
  pendingFormat = null;
  selection = {
    anchor: { node, offset },
    focus: { node, offset },
  };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView("center");
}

/** Copies a WASM-owned link hit into an ordinary JS value, then frees it. */
function linkAt(page, event) {
  if (!doc) return false;
  const { x, y } = pointToTwip(page, event);
  const hit = doc.linkAt(page.pageNumber, x, y);
  if (!hit) return null;
  const link = {
    kind: hit.kind,
    url: hit.url,
    anchor: hit.anchor,
    tooltip: hit.tooltip,
    startNode: hit.startNode,
    startOffset: hit.startOffset,
    endNode: hit.endNode,
    endOffset: hit.endOffset,
    targetNode: hit.targetNode,
    targetOffset: hit.targetOffset,
    targetPage: hit.targetPage,
  };
  hit.free();
  return link;
}

/** Clears the visible chip without changing the document selection. */
function hideLinkChip() {
  activeLink = null;
  linkChip.hidden = true;
}

/** Clears pointer feedback from every rendered page. */
function clearLinkHover() {
  pendingLinkHover = null;
  if (linkHoverFrame) cancelAnimationFrame(linkHoverFrame);
  linkHoverFrame = 0;
  for (const page of pages) page.canvas.classList.remove("link-hover");
}

/** Throttles the model query used to make canvas-painted links visibly hoverable. */
function scheduleLinkHover(page, event) {
  pendingLinkHover = { page, clientX: event.clientX, clientY: event.clientY };
  if (linkHoverFrame) return;
  linkHoverFrame = requestAnimationFrame(() => {
    linkHoverFrame = 0;
    const pending = pendingLinkHover;
    pendingLinkHover = null;
    if (!pending || dragging || !pages.includes(pending.page)) return;
    const hit = linkAt(pending.page, pending);
    for (const candidate of pages) {
      candidate.canvas.classList.toggle("link-hover", candidate === pending.page && !!hit);
    }
  });
}

/** Shows a bounded link chip and selects the exact authored model range, making
 * the target discoverable without hijacking drag or Shift-selection behavior. */
function showLinkChipAt(page, event) {
  const link = linkAt(page, event);
  if (!link) {
    hideLinkChip();
    return false;
  }
  activeLink = link;
  pendingFormat = null;
  selection = {
    anchor: { node: link.startNode, offset: link.startOffset },
    focus: { node: link.endNode, offset: link.endOffset },
  };
  drawSelection();

  const internal = link.kind === "internal";
  const resolved = !internal || (!!link.targetNode && !!link.targetPage);
  const target = internal ? `#${link.anchor}` : link.url;
  linkChipKind.textContent =
    link.tooltip || (internal ? "Document bookmark" : "External link");
  linkChipTarget.textContent = target;
  linkChipTarget.title = target;
linkChipAction.textContent = internal ? (resolved ? "Jump" : "Missing") : "Open";
linkChipAction.disabled = !resolved;
linkChipEdit.hidden = internal;
linkChipRemove.hidden = internal;
  linkChip.hidden = false;

  const width = linkChip.offsetWidth;
  const height = linkChip.offsetHeight;
  const left = Math.max(12, Math.min(event.clientX - 18, window.innerWidth - width - 12));
  let top = event.clientY + 14;
  if (top + height > window.innerHeight - 12) top = event.clientY - height - 14;
  linkChip.style.left = `${Math.round(left)}px`;
  linkChip.style.top = `${Math.max(12, Math.round(top))}px`;
  return true;
}

/** Activates a previously queried authored link. The runtime resolves
 * geometry/bookmarks; this host owns the external-scheme allowlist and browser
 * navigation. */
function activateLink(link) {
  if (!link) return false;
  hideLinkChip();

  if (link.kind === "internal") {
    if (!link.targetNode || !link.targetPage) {
      setStatus(`Bookmark “${link.anchor}” was not found`, "error");
      return true;
    }
    navigateToAnchor(link.targetNode, link.targetOffset, link.targetPage);
    setStatus(`Jumped to ${link.anchor}`);
    return true;
  }

  let target;
  try {
    target = new URL(link.url, window.location.href);
  } catch {
    setStatus("Blocked an invalid link target", "error");
    return true;
  }
  if (target.protocol === "http:" || target.protocol === "https:") {
    window.open(target.href, "_blank", "noopener,noreferrer");
  } else if (target.protocol === "mailto:") {
    window.location.assign(target.href);
  } else {
    setStatus(`Blocked ${target.protocol || "unknown"} link scheme`, "error");
  }
  return true;
}

function onPointerDown(page, event) {
  if (event.button !== 0) return;
  focusEditorSurface();
  hideLinkChip();
  clearLinkHover();
  pointerGesture = null;
  // Object selection (docs/85 §3.1) takes precedence over a text caret: a click
  // on a drawing/image/text box selects it as a unit and shows its handles.
  const { x, y } = pointToTwip(page, event);
  const object = doc.objectAt(page.pageNumber, x, y);
  if (object) {
    const node = object.node;
    const kind = object.kind;
    object.free?.();
    // A caret at the nearest text slot is the object's surrounding-text anchor
    // (for the two-step Escape); fall back to the current caret.
    const anchor = anchorAt(page, event) || selection?.focus || null;
    selectObject(node, kind, anchor);
    startSelectionAutoScroll();
    event.preventDefault();
    return;
  }
  // A click that is not on an object deselects any selected object and proceeds
  // with ordinary text hit-testing.
  objectSelection = null;
  const anchor = anchorAt(page, event);
  if (!anchor) {
    updateObjectSelectionState();
    updateObjectContextBar();
    return;
  }
  pendingFormat = null; // a click moves the caret → disarm typing format
  tableSelection = null;
  dragging = true;
  pointerGesture = {
    page,
    clientX: event.clientX,
    clientY: event.clientY,
    lastClientX: event.clientX,
    lastClientY: event.clientY,
    moved: false,
    shift: event.shiftKey,
  };
  // Shift+Click extends the current selection to the click (keeps the anchor).
  selection =
    event.shiftKey && selection
      ? { anchor: selection.anchor, focus: anchor }
      : { anchor, focus: anchor };
  // Non-blocking secondary effect (REVIEW-GAP-005): surface the sidebar card
  // for a comment the click landed inside, without altering the caret/range
  // hit-testing just computed above.
  syncActiveReviewCommentToCaret(anchor);
  drawSelection();
  startSelectionAutoScroll();
  event.preventDefault();
}

function startTableColumnResize(event, page, node, col) {
  if (!doc || !selection) return;
  event.preventDefault();
  event.stopPropagation();
  focusEditorSurface();
  hideLinkChip();
  hideContextMenu();
  clearLinkHover();
  resetPointerGesture();
  const startWidthTwips = doc.tableColumnWidthAt(node, col);
  if (startWidthTwips <= 0) return;
  const preview = document.createElement("div");
  preview.className = "table-col-resize-preview";
  preview.style.left = event.currentTarget.style.left;
  preview.style.top = "0";
  preview.style.height = `${page.overlay.clientHeight}px`;
  page.overlay.appendChild(preview);
  tableResizeDrag = {
    node,
    col,
    page,
    startClientX: event.clientX,
    startWidthTwips,
    preview,
    lastWidthTwips: startWidthTwips,
  };
  event.currentTarget.setPointerCapture?.(event.pointerId);
}

function cancelTableColumnResize() {
  if (!tableResizeDrag) return;
  tableResizeDrag.preview.remove();
  tableResizeDrag = null;
}

function updateTableColumnResize(event) {
  if (!tableResizeDrag) return;
  const { sx } = scaleOf(tableResizeDrag.page);
  const deltaTwips = Math.round((event.clientX - tableResizeDrag.startClientX) / sx);
  const widthTwips = Math.max(72, tableResizeDrag.startWidthTwips + deltaTwips);
  tableResizeDrag.lastWidthTwips = widthTwips;
  const deltaPx = deltaTwips * sx;
  tableResizeDrag.preview.style.transform = `translateX(${deltaPx}px)`;
  event.preventDefault();
}

function finishTableColumnResize(event) {
  if (!tableResizeDrag) return false;
  const drag = tableResizeDrag;
  tableResizeDrag = null;
  drag.preview.remove();
  event.preventDefault();
  if (Math.abs(drag.lastWidthTwips - drag.startWidthTwips) >= 8) {
    runEdit(() => doc.setTableColumnWidthAt(drag.node, drag.col, drag.lastWidthTwips), { gate: true });
  } else {
    drawSelection();
  }
  return true;
}

function onPointerMove(page, event) {
  if (tableResizeDrag) {
    updateTableColumnResize(event);
    return;
  }
  if (dragging && event.buttons === 0) {
    resetPointerGesture();
    return;
  }
  if (!dragging) {
    scheduleLinkHover(page, event);
    return;
  }
  updateDragSelection(event);
}

function updateDragSelection(event) {
  if (!dragging || !pointerGesture || !selection) return;
  pointerGesture.lastClientX = event.clientX;
  pointerGesture.lastClientY = event.clientY;
  if (
    pointerGesture &&
    Math.hypot(
      event.clientX - pointerGesture.clientX,
      event.clientY - pointerGesture.clientY,
    ) > 4
  ) {
    pointerGesture.moved = true;
  }
  const page = pageFromClientPoint(event.clientX, event.clientY);
  if (!page) return;
  const focus = anchorAt(page, event);
  if (!focus) return;
  selection = { anchor: selection.anchor, focus };
  drawSelection();
}

const AUTO_SCROLL_EDGE_PX = 56;
const AUTO_SCROLL_MAX_PX = 24;

function startSelectionAutoScroll() {
  if (selectionAutoScrollFrame) return;
  const tick = () => {
    selectionAutoScrollFrame = 0;
    if (!dragging || !pointerGesture) return;

    const rect = viewportEl.getBoundingClientRect();
    const y = pointerGesture.lastClientY;
    let dy = 0;
    if (y < rect.top + AUTO_SCROLL_EDGE_PX) {
      const ratio = Math.min(1, (rect.top + AUTO_SCROLL_EDGE_PX - y) / AUTO_SCROLL_EDGE_PX);
      dy = -Math.ceil(ratio * AUTO_SCROLL_MAX_PX);
    } else if (y > rect.bottom - AUTO_SCROLL_EDGE_PX) {
      const ratio = Math.min(1, (y - (rect.bottom - AUTO_SCROLL_EDGE_PX)) / AUTO_SCROLL_EDGE_PX);
      dy = Math.ceil(ratio * AUTO_SCROLL_MAX_PX);
    }

    if (dy !== 0) {
      const before = viewportEl.scrollTop;
      viewportEl.scrollTop = Math.max(0, before + dy);
      if (viewportEl.scrollTop !== before) {
        updateDragSelection(clientPointEvent(pointerGesture.lastClientX, pointerGesture.lastClientY));
      }
    }

    selectionAutoScrollFrame = requestAnimationFrame(tick);
  };
  selectionAutoScrollFrame = requestAnimationFrame(tick);
}

function onPointerUp(event) {
  if (finishTableColumnResize(event)) return;
  const gesture = pointerGesture;
  resetPointerGesture();
  if (
    gesture &&
    !gesture.shift &&
    !gesture.moved &&
    Math.hypot(event.clientX - gesture.clientX, event.clientY - gesture.clientY) <= 4
  ) {
    const link = linkAt(gesture.page, event);
    if (link?.kind === "internal" && link.targetNode && link.targetPage) {
      activateLink(link);
    } else {
      showLinkChipAt(gesture.page, event);
    }
  }
}

/** Double-click selects the word under the pointer. */
function selectWord(page, event) {
  const a = anchorAt(page, event);
  if (!a) return;
  focusEditorSurface();
  const bounds = doc.wordAt(a.node, a.offset); // [start, end] or []
  if (bounds.length === 2) {
    selection = {
      anchor: { node: a.node, offset: bounds[0] },
      focus: { node: a.node, offset: bounds[1] },
    };
    drawSelection();
  }
}

function selectionText() {
  if (!selection) return;
  const { anchor, focus } = selection;
  return doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset);
}

/** The selection as clipboard HTML: the exact `copyRichRuns` JSON embedded as
 * a leading comment (a lossless internal round-trip marker) plus a visible
 * rendering built from the same runs (what an external app sees). `null` if
 * there's nothing to copy. */
function selectionRichHtml() {
  if (!selection) return null;
  const { anchor, focus } = selection;
  const runsJson = doc.copyRichRuns(anchor.node, anchor.offset, focus.node, focus.offset);
  const runs = JSON.parse(runsJson);
  // When the selection spans block structure the flat runs flatten — a table or
  // a list — carry a structured payload for internal OpenDoc-to-OpenDoc paste
  // (`{ blocks, runs }`: the flat runs ride along so a Suggesting-mode paste, or
  // a structured paste the engine declines, still has the rich-run fallback).
  const structured = doc.copyStructured(anchor.node, anchor.offset, focus.node, focus.offset);
  if (structured) {
    const blocks = JSON.parse(structured).blocks;
    return embedMarker(JSON.stringify({ blocks, runs })) + runsToHtml(runs);
  }
  if (!runs.length) return null;
  return embedMarker(runsJson) + runsToHtml(runs);
}

async function copySelection(event = null) {
  const text = selectionText();
  if (!text) return;
  const html = selectionRichHtml();
  if (event?.clipboardData) {
    event.preventDefault();
    event.clipboardData.setData("text/plain", text);
    if (html) event.clipboardData.setData("text/html", html);
    const n = text.length;
    setStatus(`Copied ${n} character${n === 1 ? "" : "s"}`);
    return true;
  }
  try {
    if (html && window.ClipboardItem) {
      await navigator.clipboard.write([
        new ClipboardItem({
          "text/plain": new Blob([text], { type: "text/plain" }),
          "text/html": new Blob([html], { type: "text/html" }),
        }),
      ]);
    } else {
      await navigator.clipboard.writeText(text);
    }
    const n = text.length;
    setStatus(`Copied ${n} character${n === 1 ? "" : "s"}`);
    return true;
  } catch (err) {
    console.warn("clipboard write failed:", err);
    setStatus("Clipboard write was blocked by the browser", "err");
    return false;
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

function pageFromClientPoint(clientX, clientY) {
  const target = document.elementFromPoint(clientX, clientY);
  const direct = target ? pageFromEvent({ target }) : null;
  if (direct) return direct;
  if (!pages.length) return null;

  let best = null;
  let bestDistance = Infinity;
  for (const page of pages) {
    const rect = page.canvas.getBoundingClientRect();
    const dx = clientX < rect.left ? rect.left - clientX : clientX > rect.right ? clientX - rect.right : 0;
    const dy = clientY < rect.top ? rect.top - clientY : clientY > rect.bottom ? clientY - rect.bottom : 0;
    const dist = dx * dx + dy * dy;
    if (dist < bestDistance) {
      bestDistance = dist;
      best = page;
    }
  }
  return best;
}
pagesEl.addEventListener("pointerdown", (e) => {
  // Ambiguous page-gap clicks must not jump into the nearest table or other
  // fragment. Only a hit inside a concrete page may place the caret; drag
  // continuation still uses nearest-page resolution once a gesture exists.
  const page = pageFromEvent(e);
  if (page) onPointerDown(page, e);
});
pagesEl.addEventListener("pointermove", (e) => {
  const page = pageFromEvent(e);
  if (page && !dragging) onPointerMove(page, e);
});
window.addEventListener("pointermove", (e) => {
  if (tableResizeDrag) {
    updateTableColumnResize(e);
    return;
  }
  if (dragging) {
    if (e.buttons === 0) resetPointerGesture();
    else updateDragSelection(e);
  }
});
pagesEl.addEventListener("pointerleave", clearLinkHover);
pagesEl.addEventListener("dblclick", (e) => {
  const page = pageFromEvent(e);
  if (!page) return;
  // Double-click on an object enters its edit mode (container) or selects it
  // (leaf) — docs/85 §4.3; otherwise it selects the word under the caret.
  const { x, y } = pointToTwip(page, e);
  const object = doc?.objectAt(page.pageNumber, x, y);
  if (object) {
    const node = object.node;
    const kind = object.kind;
    object.free?.();
    focusEditorSurface();
    if (!objectSelection || objectSelection.node !== node) {
      selectObject(node, kind, anchorAt(page, e) || selection?.focus || null);
    }
    enterObjectEditMode();
    e.preventDefault();
    return;
  }
  selectWord(page, e);
});
// Triple-click selects the paragraph (the click's `detail` is the click count).
pagesEl.addEventListener("click", (e) => {
  if (e.detail !== 3) return;
  const page = pageFromEvent(e);
  if (!page) return;
  const a = anchorAt(page, e);
  if (!a) return;
  focusEditorSurface();
  selection = {
    anchor: { node: a.node, offset: 0 },
    focus: { node: a.node, offset: doc.paragraphLength(a.node) },
  };
  drawSelection();
});
window.addEventListener("pointerup", onPointerUp);
window.addEventListener("pointercancel", () => {
  cancelTableColumnResize();
  resetPointerGesture();
});
window.addEventListener("lostpointercapture", () => {
  cancelTableColumnResize();
  resetPointerGesture();
});
window.addEventListener("blur", () => {
  cancelTableColumnResize();
  resetPointerGesture();
});
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    cancelTableColumnResize();
    resetPointerGesture();
  }
});

linkChip.addEventListener("mousedown", (event) => {
  // Keep the model selection visible while the host control receives the click.
  event.preventDefault();
});
linkChipAction.addEventListener("click", () => activateLink(activeLink));
linkChipEdit.addEventListener("click", () => {
  if (!activeLink || !selection) return;
  const value = window.prompt("Link URL or #bookmark:", activeLink.url || `#${activeLink.anchor}`);
  if (value === null) return;
  const target = value.trim();
  if (!target) return;
  runToolbarEdit(() =>
    doc.setHyperlink(activeLink.startNode, activeLink.startOffset, activeLink.endOffset, target, activeLink.tooltip || null),
  );
  hideLinkChip();
});
linkChipRemove.addEventListener("click", () => {
  if (!activeLink || !selection || activeLink.startNode !== activeLink.endNode) return;
  runToolbarEdit(() => doc.removeHyperlink(activeLink.startNode, activeLink.startOffset, activeLink.endOffset));
  hideLinkChip();
});
document.addEventListener("pointerdown", (event) => {
  if (!linkChip.hidden && !linkChip.contains(event.target)) hideLinkChip();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    hideLinkChip();
    closeReviewPopover();
  }
});
document.addEventListener("keydown", (event) => {
  if (!doc || event.defaultPrevented) return;
  const mod = event.metaKey || event.ctrlKey;
  if (mod && event.altKey && event.key.toLowerCase() === "m") {
    event.preventDefault();
    openReviewComposer();
  } else if (mod && event.shiftKey && event.key.toLowerCase() === "e") {
    event.preventDefault();
    // Cycle Editing → Suggesting → Viewing → Editing for keyboard access to
    // all three modes (REVIEW-GAP-014).
    const next =
      reviewMode === "editing" ? "suggesting" : reviewMode === "suggesting" ? "viewing" : "editing";
    setReviewMode(next);
  }
});
viewportEl.addEventListener("scroll", hideLinkChip, { passive: true });
window.addEventListener("resize", hideLinkChip);

// ---- Context-aware editor menu ---------------------------------------------
// One surface serves prose, links, review ranges, lists, and tables. Commands
// call the same transaction-backed actions as the ribbon and palette.
const editorContextMenu = document.createElement("div");
editorContextMenu.className = "context-menu editor-context-menu";
editorContextMenu.hidden = true;
editorContextMenu.setAttribute("role", "menu");
editorContextMenu.setAttribute("aria-label", "Editor commands");
document.body.appendChild(editorContextMenu);
let contextMenuEntries = [];
let contextMenuIndex = -1;
let contextMenuReturnFocus = null;

function selectionContainsClientPoint(clientX, clientY) {
  if (!doc || !hasRange()) return false;
  const rects = doc.selectionRects(
    selection.anchor.node,
    selection.anchor.offset,
    selection.focus.node,
    selection.focus.offset,
  );
  for (let i = 0; i + 4 < rects.length; i += 5) {
    const [pageNumber, x, y, width, height] = rects.slice(i, i + 5);
    const page = pages[pageNumber - 1];
    if (!page) continue;
    const { rect, sx, sy } = scaleOf(page);
    if (
      clientX >= rect.left + x * sx &&
      clientX <= rect.left + (x + width) * sx &&
      clientY >= rect.top + y * sy &&
      clientY <= rect.top + (y + height) * sy
    ) {
      return true;
    }
  }
  return false;
}

function tableSelectionContainsClientPoint(clientX, clientY) {
  if (!doc || !tableSelection) return false;
  const rects = doc.tableSelectionRects(
    tableSelection.node,
    tableSelection.mode,
  );
  for (let i = 0; i + 4 < rects.length; i += 5) {
    const [pageNumber, x, y, width, height] = rects.slice(i, i + 5);
    const page = pages[pageNumber - 1];
    if (!page) continue;
    const { rect, sx, sy } = scaleOf(page);
    if (
      clientX >= rect.left + x * sx &&
      clientX <= rect.left + (x + width) * sx &&
      clientY >= rect.top + y * sy &&
      clientY <= rect.top + (y + height) * sy
    ) {
      return true;
    }
  }
  return false;
}

function anchorInsideRange(anchor, range) {
  return (
    anchor &&
    range &&
    range.startNode === anchor.node &&
    range.endNode === anchor.node &&
    anchor.offset >= range.startOffset &&
    anchor.offset <= range.endOffset
  );
}

function reviewContextAt(anchor) {
  if (!doc || !anchor) return { comment: null, revision: null };
  const summary = readReviewData(doc);
  const comment = (summary.comments ?? []).find((item) =>
    item.anchor?.node === anchor.node &&
    anchor.offset >= (Number(item.anchor.start) || 0) &&
    anchor.offset <= (Number(item.anchor.end) || Number(item.anchor.start) || 0),
  ) ?? null;
  const revision = (summary.revisions ?? []).find((item) =>
    anchorInsideRange(anchor, revisionRange(item)),
  ) ?? null;
  return { comment, revision };
}

function plainTableInfo(node) {
  if (!doc?.inTable(node)) return null;
  const info = doc.tableInfo(node);
  const value = info?.found ? {
    found: true,
    regular: info.regular,
    rowHeightRule: info.rowHeightRule,
    table: info.table,
  } : null;
  info?.free();
  return value;
}

function contextAt(anchor, link = null) {
  const review = reviewContextAt(anchor);
  return {
    surface: "context",
    anchor,
    link,
    comment: review.comment,
    revision: review.revision,
    table: plainTableInfo(anchor.node),
    listKind: doc.listStyleAt(anchor.node),
    hasRange: hasRange(),
    sameParagraphRange:
      hasRange() && selection.anchor.node === selection.focus.node,
    suggesting: reviewMode === "suggesting",
  };
}

function decideContextRevision(revision, accept) {
  if (!revision) return;
  return runEdit(() =>
    revision.movePair?.fromStart && revision.movePair?.toStart
      ? doc.decideMovePair(
        revision.movePair.fromStart,
        revision.movePair.toStart,
        accept,
      )
      : revision.groupId
        ? doc.decideRevisionGroup(revision.groupId, accept)
        : doc.decideRevision(revision.id, accept),
  );
}

function selectTableContext(node, mode) {
  tableSelection = { node, mode };
  drawSelection();
  setStatus(`Selected table ${mode}`);
  focusEditorSurface();
}

function editContextLink(link) {
  if (!link || link.startNode !== link.endNode) return;
  selection = {
    anchor: { node: link.startNode, offset: link.startOffset },
    focus: { node: link.endNode, offset: link.endOffset },
  };
  drawSelection();
  const current = link.url || `#${link.anchor}`;
  const value = window.prompt("Link URL or #bookmark:", current);
  if (value === null || !value.trim()) return;
  runToolbarEdit(() =>
    doc.setHyperlink(
      link.startNode,
      link.startOffset,
      link.endOffset,
      value.trim(),
      link.tooltip || null,
    ),
  );
}

function removeContextLink(link) {
  if (!link || link.startNode !== link.endNode) return;
  selection = {
    anchor: { node: link.startNode, offset: link.startOffset },
    focus: { node: link.endNode, offset: link.endOffset },
  };
  drawSelection();
  runToolbarEdit(() =>
    doc.removeHyperlink(link.startNode, link.startOffset, link.endOffset),
  );
}

function buildContextCommands(context) {
  const commands = editorCommands(context).filter((command) => command.contextMenu);
  const structuralEnabled = !context.suggesting;
  const structuralReason = structuralEnabled
    ? ""
    : "This structural change cannot be tracked in Suggesting mode";
  if (context.revision) {
    commands.push(
      {
        id: "review.accept",
        label: "Accept suggestion",
        group: "review",
        run: () => decideContextRevision(context.revision, true),
      },
      {
        id: "review.reject",
        label: "Reject suggestion",
        group: "review",
        danger: true,
        run: () => decideContextRevision(context.revision, false),
      },
    );
  }
  if (context.comment) {
    commands.push({
      id: "comment.open",
      label: "Open comment",
      group: "review",
      run: () => focusReviewComment(context.comment),
    });
  } else {
    commands.push({
      id: "comment.add",
      label: "Add comment",
      group: "review",
      shortcut: "⌘⌥M",
      enabled: context.sameParagraphRange,
      disabledReason: context.hasRange
        ? "Comments currently require one paragraph"
        : "Select text to add a comment",
      run: () => openReviewComposer(),
    });
  }
  if (context.link) {
    commands.push(
      {
        id: "link.edit",
        label: "Edit link…",
        group: "link",
        enabled: !context.suggesting,
        disabledReason: context.suggesting
          ? "Link changes cannot be tracked in Suggesting mode"
          : "",
        run: () => editContextLink(context.link),
      },
      {
        id: "link.remove",
        label: "Remove link",
        group: "link",
        enabled: !context.suggesting,
        disabledReason: context.suggesting
          ? "Link changes cannot be tracked in Suggesting mode"
          : "",
        run: () => removeContextLink(context.link),
      },
    );
  } else {
    commands.push({
      id: "link.add",
      label: "Add link…",
      group: "link",
      shortcut: "⌘K",
      enabled: context.sameParagraphRange && !context.suggesting,
      disabledReason: context.suggesting
        ? "Link changes cannot be tracked in Suggesting mode"
        : context.hasRange
          ? "Links must stay within one paragraph"
          : "Select text to add a link",
      run: () => editSelectionLink(),
    });
  }
  commands.push(
    {
      id: "paragraph.properties",
      label: "Paragraph properties",
      group: "paragraph",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => toggleParagraphProperties(true),
    },
    {
      id: "paragraph.bullets",
      label: context.listKind === "bullet" ? "Remove bullets" : "Bulleted list",
      group: "paragraph",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runToolbarEdit((a, b, c, d) =>
        doc.toggleList(a, b, c, d, "bullet")),
    },
    {
      id: "paragraph.numbering",
      label: context.listKind === "numbered" ? "Remove numbering" : "Numbered list",
      group: "paragraph",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runToolbarEdit((a, b, c, d) =>
        doc.toggleList(a, b, c, d, "numbered")),
    },
    {
      id: "paragraph.restart",
      label: "Restart numbering",
      group: "paragraph",
      visible: context.listKind === "numbered",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runNodeEdit(() => doc.restartList(context.anchor.node)),
    },
    {
      id: "paragraph.continue",
      label: "Continue numbering",
      group: "paragraph",
      visible: context.listKind === "numbered" && doc.canContinueList(context.anchor.node),
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runNodeEdit(() => doc.continueList(context.anchor.node)),
    },
    {
      id: "paragraph.indent.increase",
      label: "Increase indent",
      group: "paragraph",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => adjustIndentCommand(360),
    },
    {
      id: "paragraph.indent.decrease",
      label: "Decrease indent",
      group: "paragraph",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => adjustIndentCommand(-360),
    },
  );
  if (context.table) {
    const regular = context.table.regular;
    const selectedTable = tableSelection
      ? plainTableInfo(tableSelection.node)?.table
      : "";
    const hasTableSelection =
      !!selectedTable && selectedTable === context.table.table;
    const columnsReason = regular
      ? structuralReason
      : "Unavailable for merged or spanned tables";
    const tableMutation = (id, label, run, options = {}) => ({
      id,
      label,
      group: options.group ?? "table-structure",
      enabled:
        structuralEnabled &&
        (options.regular !== true || regular) &&
        (options.enabled ?? true),
      disabledReason:
        !structuralEnabled
          ? structuralReason
          : options.regular === true && !regular
            ? columnsReason
            : options.enabled === false
              ? options.disabledReason
              : "",
      danger: options.danger,
      run,
    });
    commands.push(
      {
        id: "table.select.row",
        label: "Select row",
        group: "table-select",
        run: () => selectTableContext(context.anchor.node, "row"),
      },
      {
        id: "table.select.column",
        label: "Select column",
        group: "table-select",
        enabled: regular,
        disabledReason: regular ? "" : columnsReason,
        run: () => selectTableContext(context.anchor.node, "column"),
      },
      {
        id: "table.select.table",
        label: "Select table",
        group: "table-select",
        run: () => selectTableContext(context.anchor.node, "table"),
      },
      tableMutation("table.insert.rowAbove", "Insert row above",
        () => runEdit(() => doc.insertRow(context.anchor.node, false), { gate: true })),
      tableMutation("table.insert.rowBelow", "Insert row below",
        () => runEdit(() => doc.insertRow(context.anchor.node, true), { gate: true })),
      tableMutation("table.insert.columnLeft", "Insert column left",
        () => runEdit(() => doc.insertColumn(context.anchor.node, false), { gate: true }),
        { regular: true }),
      tableMutation("table.insert.columnRight", "Insert column right",
        () => runEdit(() => doc.insertColumn(context.anchor.node, true), { gate: true }),
        { regular: true }),
      tableMutation("table.distribute.rows", "Distribute rows",
        () => runEdit(() => doc.distributeTableRows(context.anchor.node), { gate: true }),
        {
          regular: true,
          enabled: ["exact", "atLeast"].includes(context.table.rowHeightRule),
          disabledReason: "Rows need a fixed or minimum height before distribution",
        }),
      tableMutation("table.distribute.columns", "Distribute columns",
        () => runEdit(() => doc.distributeTableColumns(context.anchor.node), { gate: true }),
        { regular: true }),
      tableMutation("table.sort.ascending", "Sort ascending",
        () => runEdit(() => doc.sortTable(context.anchor.node, "ascending"), { gate: true }),
        { regular: true }),
      tableMutation("table.sort.descending", "Sort descending",
        () => runEdit(() => doc.sortTable(context.anchor.node, "descending"), { gate: true }),
        { regular: true }),
      tableMutation("table.merge", "Merge selected cells",
        async () => {
          await runEdit(() =>
            doc.mergeTableSelection(tableSelection.node, tableSelection.mode), { gate: true });
          tableSelection = null;
        },
        {
          enabled:
            hasTableSelection,
          disabledReason: "Select a row, column, or table before merging",
        }),
      tableMutation("table.split", "Split cell…",
        () => toggleSplitCellDialog(true)),
      {
        id: "table.cellFormat",
        label: "Cell formatting…",
        group: "table-properties",
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => {
          selectRibbonTab("table");
          tableBtn.click();
        },
      },
      {
        id: "table.properties",
        label: "Table properties",
        group: "table-properties",
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => toggleTableProperties(true),
      },
      tableMutation("table.delete.row", "Delete row",
        () => runEdit(() => doc.deleteRow(context.anchor.node), { gate: true }),
        { group: "table-delete", danger: true }),
      tableMutation("table.delete.column", "Delete column",
        () => runEdit(() => doc.deleteColumn(context.anchor.node), { gate: true }),
        { group: "table-delete", danger: true, regular: true }),
      tableMutation("table.delete.table", "Delete table",
        () => runEdit(() => doc.deleteTable(context.anchor.node), { gate: true }),
        { group: "table-delete", danger: true }),
    );
  }
  return commands;
}

function setContextMenuIndex(index, focus = true) {
  contextMenuIndex = index;
  const items = [...editorContextMenu.querySelectorAll(".menu-item")];
  for (const item of items) {
    const active = Number(item.dataset.menuIndex) === index;
    item.tabIndex = active ? 0 : -1;
    item.classList.toggle("active", active);
    if (active && focus) {
      item.focus({ preventScroll: true });
      item.scrollIntoView({ block: "nearest" });
    }
  }
}

function hideContextMenu({ restoreFocus = false } = {}) {
  if (editorContextMenu.hidden) return;
  editorContextMenu.hidden = true;
  contextMenuEntries = [];
  contextMenuIndex = -1;
  if (restoreFocus) {
    const target = contextMenuReturnFocus?.isConnected
      ? contextMenuReturnFocus
      : pagesEl;
    target.focus({ preventScroll: true });
  }
  contextMenuReturnFocus = null;
}

function runContextMenuEntry(index) {
  const command = contextMenuEntries[index];
  if (!command || command.separator || command.enabled === false) return;
  hideContextMenu({ restoreFocus: true });
  command.run();
}

function showContextMenu(clientX, clientY, context) {
  hideContextMenu();
  contextMenuReturnFocus =
    document.activeElement instanceof HTMLElement ? document.activeElement : pagesEl;
  contextMenuEntries = normalizeMenuEntries(buildContextCommands(context));
  editorContextMenu.replaceChildren();
  contextMenuEntries.forEach((entry, index) => {
    if (entry.separator) {
      const separator = document.createElement("div");
      separator.className = "menu-divider";
      separator.setAttribute("role", "separator");
      editorContextMenu.appendChild(separator);
      return;
    }
    const button = document.createElement("button");
    button.type = "button";
    button.className = `menu-item${entry.danger ? " danger" : ""}`;
    button.dataset.menuIndex = String(index);
    button.dataset.commandId = entry.id;
    button.setAttribute("role", "menuitem");
    button.disabled = entry.enabled === false;
    button.tabIndex = -1;
    if (entry.disabledReason) button.title = entry.disabledReason;
    const label = document.createElement("span");
    label.className = "menu-item-label";
    label.textContent = entry.label;
    button.appendChild(label);
    if (entry.shortcut || (entry.enabled === false && entry.disabledReason)) {
      const hint = document.createElement("span");
      hint.className = "menu-item-hint";
      hint.textContent = entry.enabled === false
        ? entry.disabledReason
        : entry.shortcut;
      button.appendChild(hint);
    }
    button.addEventListener("mousemove", () => {
      if (!button.disabled) setContextMenuIndex(index, false);
    });
    button.addEventListener("click", () => runContextMenuEntry(index));
    editorContextMenu.appendChild(button);
  });
  editorContextMenu.hidden = false;
  const position = clampContextMenuPosition(
    clientX,
    clientY,
    editorContextMenu.offsetWidth,
    editorContextMenu.offsetHeight,
    window.innerWidth,
    window.innerHeight,
  );
  editorContextMenu.style.left = `${position.left}px`;
  editorContextMenu.style.top = `${position.top}px`;
  const first = moveMenuIndex(contextMenuEntries, -1, 1);
  setContextMenuIndex(first);
}

function keyboardContextMenuPoint() {
  if (!selection || !doc) return null;
  const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
  if (flat.length < 5) return null;
  const [pageNumber, x, y, width, height] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return null;
  const { rect, sx, sy } = scaleOf(page);
  return {
    x: rect.left + (x + width) * sx,
    y: rect.top + (y + height) * sy,
    page,
  };
}

pagesEl.addEventListener("contextmenu", (event) => {
  const page = pageFromEvent(event);
  if (!page || !doc) return;
  const anchor = anchorAt(page, event);
  if (!anchor) return;
  event.preventDefault();
  const preserveSelection = selectionContainsClientPoint(
    event.clientX,
    event.clientY,
  ) || tableSelectionContainsClientPoint(event.clientX, event.clientY);
  if (!preserveSelection) {
    selection = { anchor, focus: anchor };
    tableSelection = null;
    drawSelection();
  }
  showContextMenu(
    event.clientX,
    event.clientY,
    contextAt(anchor, linkAt(page, event)),
  );
});

document.addEventListener("pointerdown", (event) => {
  if (!editorContextMenu.hidden && !editorContextMenu.contains(event.target)) {
    hideContextMenu();
  }
});
document.addEventListener("keydown", (event) => {
  if (!editorContextMenu.hidden) {
    if (event.key === "Escape") {
      event.preventDefault();
      hideContextMenu({ restoreFocus: true });
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setContextMenuIndex(
        moveMenuIndex(
          contextMenuEntries,
          contextMenuIndex,
          event.key === "ArrowDown" ? 1 : -1,
        ),
      );
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      setContextMenuIndex(
        moveMenuIndex(
          contextMenuEntries,
          contextMenuIndex,
          event.key === "Home" ? "first" : "last",
        ),
      );
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      runContextMenuEntry(contextMenuIndex);
    }
    return;
  }
  if (
    doc &&
    selection &&
    eventTargetsEditor(event) &&
    ((event.shiftKey && event.key === "F10") || event.key === "ContextMenu")
  ) {
    const point = keyboardContextMenuPoint();
    if (!point) return;
    event.preventDefault();
    const anchor = selection.focus;
    const link = linkAt(
      point.page,
      clientPointEvent(point.x, Math.max(point.y - 1, 0)),
    );
    showContextMenu(point.x, point.y, contextAt(anchor, link));
  }
});
viewportEl.addEventListener("scroll", () => hideContextMenu(), { passive: true });
window.addEventListener("resize", () => hideContextMenu());

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

/** Scroll the caret in the editor viewport (not an arbitrary page ancestor).
 * Navigation callers can request a centered target so headings/anchors have
 * useful reading room below the destination instead of landing on the viewport
 * edge. Normal caret movement keeps nearest-only behavior to avoid jitter. */
function scrollCaretIntoView(block = "nearest") {
  const caret = pagesEl.querySelector(".overlay .caret");
  if (!caret) return;
  const caretRect = caret.getBoundingClientRect();
  const viewportRect = viewportEl.getBoundingClientRect();
  const current = viewportEl.scrollTop;
  const max = Math.max(0, viewportEl.scrollHeight - viewportEl.clientHeight);
  let target = current;
  if (block === "center") {
    target = current + caretRect.top + caretRect.height / 2 - (viewportRect.top + viewportRect.height / 2);
  } else if (caretRect.top < viewportRect.top) {
    target = current + caretRect.top - viewportRect.top;
  } else if (caretRect.bottom > viewportRect.bottom) {
    target = current + caretRect.bottom - viewportRect.bottom;
  } else {
    return;
  }
  viewportEl.scrollTo({ top: Math.max(0, Math.min(max, target)), behavior: "auto" });
}

/** Apply an EditResult: place the caret, repaint only the dirty pages (or rebuild
 *  on a page-count change), redraw the caret, and keep it in view. */
async function applyEditResult(res) {
  const node = res.node;
  const offset = res.offset;
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  clearFindParagraphCache();
  selection = { anchor: { node, offset }, focus: { node, offset } };
  if (newCount !== pages.length) {
    await renderAll(); // structural change (page added/removed): rebuild the list
  } else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ stats: true, outline: true });
  scrollCaretIntoView();
}

/** Ends the current typing gesture. The next printable key receives a fresh
 * session id and therefore cannot merge with earlier history. */
function breakTypingSession() {
  typingSessionActive = false;
  lastTypingAt = 0;
}

/** Returns the gesture id for an adjacent typing tick. A pause is a semantic
 * boundary even if the caret has not moved. */
function typingSessionForKey() {
  const now = performance.now();
  if (!typingSessionActive || now - lastTypingAt > TYPING_PAUSE_MS) {
    typingSession = (typingSession + 1) >>> 0;
    if (typingSession === 0) typingSession = 1;
  }
  typingSessionActive = true;
  lastTypingAt = now;
  return typingSession;
}

// Pointer interaction always establishes a new caret/selection/command gesture.
document.addEventListener("pointerdown", breakTypingSession, { capture: true });

/** True (after showing the standard status message and returning focus to the
 *  canvas) if Suggesting mode should block a command whose mutation cannot -
 *  yet or ever - be represented as a tracked revision. This is the single
 *  fail-closed gate shared by every mutation path (`runEdit`, `runNodeEdit`,
 *  `runToolbarEdit`): a command that bypasses tracking must never silently
 *  apply while the mode still reads Suggesting (REVIEW-GAP-004). */
function blockUntrackedInSuggesting() {
  if (reviewMode !== "suggesting") return false;
  setStatus("This command cannot be tracked yet; switch to Editing to apply it", "error");
  focusEditorSurface();
  return true;
}

/** True (after showing the read-only status message and returning focus to the
 *  canvas) if Viewing mode should block a document mutation. Viewing is fully
 *  read-only — no Operation reaches apply (docs/68 §"Suggesting mode") — so
 *  every mutation path (typing, deletion, paste, toolbar formatting, table
 *  ops, and comment/revision decisions) fails closed here rather than
 *  depending on any individual command or menu item being disabled
 *  (REVIEW-GAP-014). Navigation, selection, scroll, and copy are not
 *  mutations and are unaffected. */
function blockMutationInViewing() {
  if (reviewMode !== "viewing") return false;
  setStatus("Viewing mode is read-only; switch to Editing to change the document", "error");
  focusEditorSurface();
  return true;
}

/** Runs an edit thunk and applies its result; unsupported edits are ignored.
 *  `gate: true` marks a mutation that has no tracked-revision representation
 *  (yet, or ever, per REVIEW-GAP-009's structural backlog): it is blocked
 *  outright in Suggesting mode rather than silently applying untracked. */
async function runEdit(thunk, { typing = false, gate = false } = {}) {
  if (blockMutationInViewing()) return;
  if (!typing) breakTypingSession();
  if (gate && blockUntrackedInSuggesting()) return;
  let res;
  try {
    res = thunk();
  } catch (err) {
    if (typing) breakTypingSession();
    console.warn("edit ignored:", err?.message ?? err);
    // The engine's error names (Unsupported, CrossParagraph, …) are internal
    // vocabulary, not user-facing text — a bounded, generic message is enough
    // to stop this from reading as "nothing happened" (docs/67, "Error/
    // reporting UX": never silently do nothing).
    setStatus("That edit isn't supported for this selection yet", "error");
    return;
  }
  await applyEditResult(res);
}

/** Move the caret by arrow key. Shift extends (moves the focus); plain collapses. */
function navCaret(dir, extend) {
  if (!selection) return;
  objectSelection = null; // moving the caret leaves any object selection
  breakTypingSession();
  pendingFormat = null; // caret moved → disarm typing format
  const collapseToStart = dir === "left" || dir === "wordLeft";
  const collapseToEnd = dir === "right" || dir === "wordRight";
  const c =
    !extend && hasRange() && (collapseToStart || collapseToEnd)
      ? doc.selectionEdge(
          selection.anchor.node,
          selection.anchor.offset,
          selection.focus.node,
          selection.focus.offset,
          collapseToEnd,
        )
      : doc.moveCaret(selection.focus.node, selection.focus.offset, dir);
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
async function runToolbarEdit(thunk, { allowInSuggesting = false } = {}) {
  if (blockMutationInViewing()) return;
  breakTypingSession();
  if (!allowInSuggesting && blockUntrackedInSuggesting()) return;
  const ends = selEndpoints();
  if (!ends) return;
  let res;
  try {
    res = thunk(...ends);
  } catch (err) {
    console.warn("edit ignored:", err?.message ?? err);
    setStatus("That tracked format is not supported for this selection yet", "error");
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
  scheduleChromeRefresh({ outline: true });
}

/** The uniform run-format state over the selection, or null if not a range. */
function selectionFormat() {
  if (!doc || !hasRange()) return null;
  const [sn, so, en, eo] = selEndpoints();
  const f = doc.selectionFormat(sn, so, en, eo);
  const state = {
    bold: f.bold,
    italic: f.italic,
    underline: f.underline,
    strike: f.strike,
    boldState: f.boldState,
    italicState: f.italicState,
    underlineState: f.underlineState,
    strikeState: f.strikeState,
  };
  f.free();
  return state;
}

/** The run format the collapsed caret inherits (what new typing would carry). */
function caretFormatState() {
  if (!doc || !selection) return { bold: false, italic: false, underline: false, strike: false };
  const f = doc.caretFormat(selection.focus.node, selection.focus.offset);
  const state = {
    bold: f.bold,
    italic: f.italic,
    underline: f.underline,
    strike: f.strike,
    boldState: f.boldState,
    italicState: f.italicState,
    underlineState: f.underlineState,
    strikeState: f.strikeState,
  };
  f.free();
  return state;
}

/** Reflects an authored font family without restricting imported documents to
 * the toolbar's starter list. The temporary option is presentation-only: the
 * renderer's physical substitution/fallback family is never written back as the
 * document's requested font. */
function reflectFontFamily(family) {
  const previous = fontFamilySel.querySelector("option[data-reflected-font]");
  if (previous && previous.value !== family) previous.remove();
  if (family && ![...fontFamilySel.options].some((option) => option.value === family)) {
    const option = document.createElement("option");
    option.value = family;
    option.textContent = family;
    option.dataset.reflectedFont = "";
    fontFamilySel.insertBefore(option, fontFamilySel.options[1] ?? null);
  }
  fontFamilySel.value = family;
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
  const patch = {
    [prop]: !state[prop],
  };
  armOrApplyRun(patch, () =>
    runToolbarEdit((sn, so, en, eo) => doc.formatSelection(
      sn,
      so,
      en,
      eo,
      prop === "bold" ? !state.bold : undefined,
      prop === "italic" ? !state.italic : undefined,
      prop === "underline" ? !state.underline : undefined,
      prop === "strike" ? !state.strike : undefined,
    )),
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
    const mixed = range && runState?.[`${key}State`] === 2;
    const pressed = range
      ? runState && runState[key]
      : (pendingFormat?.[key] ?? (caretFmt ? caretFmt[key] : false));
    fmtButtons[key].setAttribute("aria-pressed", mixed ? "mixed" : String(!!pressed));
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
  let sizeMixed = false;
  let fontMixed = false;
  let colorMixed = false;
  let highlight = "none";
  let highlightMixed = false;
  let verticalAlignMixed = false;
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
    sizeMixed = rs.sizeMixed;
    font = rs.font;
    fontMixed = rs.fontMixed;
    if (rs.color) textColorInput.value = rs.color;
    colorMixed = rs.colorMixed;
    highlight = rs.highlight || "none";
    highlightMixed = rs.highlightMixed;
    sup = rs.superscript;
    sub = rs.subscript;
    verticalAlignMixed = rs.verticalAlignMixed;
    rs.free();
  }
  // An armed (pending) run format overrides the inherited value in the display.
  if (pendingFormat) {
    if (pendingFormat.sizeHalfPoints != null) size = String(pendingFormat.sizeHalfPoints / 2);
    if (pendingFormat.font != null) font = pendingFormat.font;
    if (pendingFormat.color) textColorInput.value = pendingFormat.color;
    if (pendingFormat.highlight != null) highlight = pendingFormat.highlight;
    if (pendingFormat.vertAlign != null) {
      sup = pendingFormat.vertAlign === "super";
      sub = pendingFormat.vertAlign === "sub";
    }
    sizeMixed = false;
    fontMixed = false;
    colorMixed = false;
    highlightMixed = false;
    verticalAlignMixed = false;
  }
  fontSizeSel.value = size;
  fontSizeSel.placeholder = sizeMixed ? "Mixed" : "Size";
  fontSizeSel.closest(".ctl")?.classList.toggle("is-mixed", sizeMixed);
  reflectFontFamily(fontMixed ? "" : font);
  fontFamilySel.closest(".ctl")?.classList.toggle("is-mixed", fontMixed);
  textColorInput.closest(".ctl")?.classList.toggle("is-mixed", colorMixed);
  highlightSel.value = highlightMixed ? "" : highlight;
  highlightSel.closest(".ctl")?.classList.toggle("is-mixed", highlightMixed);
  superBtn.setAttribute("aria-pressed", verticalAlignMixed ? "mixed" : String(sup));
  subBtn.setAttribute("aria-pressed", verticalAlignMixed ? "mixed" : String(sub));

  // Reflect the current paragraph style + spacing + list kind.
  paragraphStyleSel.value = hasSel && doc ? doc.paragraphStyleAt(selection.focus.node) : "";
  if (hasSel && doc) for (const p of popovers) if (!p.menu.hidden) p.reflect();
  const listKind = hasSel && doc ? doc.listStyleAt(selection.focus.node) : "";
  bulletListBtn.setAttribute("aria-pressed", String(listKind === "bullet"));
  numberedListBtn.setAttribute("aria-pressed", String(listKind === "numbered"));
  restartListBtn.disabled = !hasSel || listKind !== "numbered";
  // Continue numbering is available only when the caret's numbered item has an
  // earlier numbered list at the same level to resume (the engine's own guard).
  continueListBtn.disabled =
    !hasSel || listKind !== "numbered" || !doc.canContinueList(selection.focus.node);
  // The contextual Table ribbon is enabled only inside a table; regular-grid
  // column commands stay unavailable on merged/spanned tables rather than
  // failing after the user clicks them.
  const inTable = hasSel && doc && doc.inTable(selection.focus.node);
  const tableInfo = inTable ? doc.tableInfo(selection.focus.node) : null;
  for (const control of tableRibbonControls) control.disabled = !inTable;
  tableStyleBtn.disabled = !inTable;
  const activeTableStyle = inTable && tableInfo?.found ? (doc.tableStyleAt?.(selection.focus.node) || "") : "";
  tableStyleBtn.title = activeTableStyle ? `Table style: ${activeTableStyle}` : "Choose table style";
  for (const control of tableRibbon.querySelectorAll(
    '[data-table-action*="column"]',
  )) {
    control.disabled = !inTable || !tableInfo?.regular;
  }
  for (const control of tableRibbon.querySelectorAll("[data-table-distribute]")) {
    control.disabled =
      !inTable ||
      !tableInfo?.regular ||
      (control.dataset.tableDistribute === "rows" &&
        !["exact", "atLeast"].includes(tableInfo.rowHeightRule));
  }
  for (const control of tableRibbon.querySelectorAll("[data-table-sort]")) {
    control.disabled = !inTable || !tableInfo?.regular;
  }
  mergeCellsBtn.disabled = !inTable || !tableSelection;
  tableContext.textContent = tableInfo?.found ? tableContextLabel(tableInfo) : "";
  if (!tablePropertiesPanel.hidden) {
    if (!tableInfo?.found) toggleTableProperties(false);
    else reflectTableProperties(selection.focus.node);
  }
  if (!paragraphPropertiesPanel.hidden) {
    if (!hasSel) toggleParagraphProperties(false);
    else reflectParagraphProperties();
  }
  tableInfo?.free();

  // Insert-table needs just a caret to drop the new table after.
  insertTableBtn.disabled = !(hasSel && doc);
  insertLinkBtn.disabled =
    !range || selection.anchor.node !== selection.focus.node;
  // Ribbon: undo/redo/view controls need a document; the Table tab is contextual.
  undoBtn.disabled = !doc || !doc.canUndo;
  redoBtn.disabled = !doc || !doc.canRedo;
  const undoLabel = doc?.undoLabel || "";
  const redoLabel = doc?.redoLabel || "";
  const undoName = undoLabel ? `Undo ${undoLabel}` : "Undo";
  const redoName = redoLabel ? `Redo ${redoLabel}` : "Redo";
  undoBtn.setAttribute("aria-label", undoName);
  redoBtn.setAttribute("aria-label", redoName);
  undoBtn.title = `${undoName} (⌘Z)`;
  redoBtn.title = `${redoName} (⌘⇧Z)`;
  findBtn.disabled = !doc;
  replaceBtn.disabled = !doc;
  // Clipboard buttons mirror the clipboard actions' own preconditions: copy/cut
  // need a range; paste needs a caret. The actions still fail closed in Viewing
  // mode, but the buttons also disable there so the affordance matches.
  copyBtn.disabled = !range;
  cutBtn.disabled = !range || reviewMode === "viewing";
  pasteBtn.disabled = !hasSel || !doc || reviewMode === "viewing";
  syncStylesGalleryActive();
  propertiesBtn.disabled = !doc;
  pageSetupBtn.disabled = !doc;
  viewOutlineBtn.disabled = !doc;
  viewOutlineBtn.setAttribute("aria-pressed", String(!outlinePanel.hidden));
  reviewBtn.disabled = !doc;
  reviewBtn.setAttribute("aria-pressed", String(!reviewSidebar.hidden));
  railReview.disabled = !doc;
  railReview.setAttribute("aria-pressed", String(!reviewSidebar.hidden));
  viewZoomOut.disabled = !doc;
  viewZoomIn.disabled = !doc;
  tabTable.disabled = !inTable;
  if (tabTable.disabled && tabTable.getAttribute("aria-selected") === "true") {
    selectRibbonTab("home");
  }
  if (!outlinePanel.hidden) reflectOutlineSelection();
}

/** Fills the paragraph-style dropdown from the open document's styles. */
function populateStyles() {
  const styles = doc ? doc.listStyles() : [];
  for (const select of [paragraphStyleSel, paraPanelStyle]) {
    select.replaceChildren();
    for (const [value, label] of [["", "Style"], ...styles.map((s) => [s, s])]) {
      const opt = document.createElement("option");
      opt.value = value;
      opt.textContent = label;
      select.appendChild(opt);
    }
  }
  // The visible Styles gallery mirrors the same style list.
  buildStylesGallery(styles);
  scheduleRibbonOverflow();
}

function populateTableStyles() {
  tableStyleMenu.replaceChildren();
  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "table-style-choice table-style-clear";
  clear.dataset.tableStyle = "";
  clear.textContent = "No table style";
  tableStyleMenu.appendChild(clear);
  for (const style of doc?.listTableStyles?.() || []) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "table-style-choice";
    button.dataset.tableStyle = style;
    button.innerHTML = `<span class="table-style-swatch" aria-hidden="true"><i></i><i></i><i></i></span><span>${escapeHtml(style)}</span>`;
    tableStyleMenu.appendChild(button);
  }
}
let tableStylePopover;
tableStyleMenu.addEventListener("click", (event) => {
  const choice = event.target.closest("[data-table-style]");
  if (!choice || !selection || !doc) return;
  closePopover(tableStylePopover);
  runEdit(() => doc.applyTableStyle(selection.focus.node, choice.dataset.tableStyle), { gate: true });
});

// mousedown (not click) so a button never steals the selection focus mid-edit.
function onButton(el, handler) {
  el.addEventListener("mousedown", (e) => {
    e.preventDefault();
    handler();
  });
  // Native buttons activated from the keyboard emit `click` without a preceding
  // mouse event (`detail === 0`). Preserve the selection on pointer activation
  // while keeping every command reachable with Enter/Space.
  el.addEventListener("click", (e) => {
    if (e.detail !== 0) return;
    e.preventDefault();
    handler();
  });
}

/** Creates/updates the selected same-paragraph text as an external URL or
 * `#bookmark`; an empty submitted value removes an exact existing link. */
function editSelectionLink() {
  if (!doc || !selection || !hasRange()) return;
  const { anchor, focus } = selection;
  if (anchor.node !== focus.node) {
    setStatus("Links must stay within one paragraph", "error");
    return;
  }
  const start = Math.min(anchor.offset, focus.offset);
  const end = Math.max(anchor.offset, focus.offset);
  const value = window.prompt(
    "Link URL or #bookmark (leave empty to remove an existing link):",
    "https://",
  );
  if (value === null) return;
  const target = value.trim();
  if (target) {
    runToolbarEdit(() =>
      doc.setHyperlink(anchor.node, start, end, target),
    );
  } else {
    runToolbarEdit(() => doc.removeHyperlink(anchor.node, start, end));
  }
}

onButton(insertLinkBtn, editSelectionLink);
for (const key of ["bold", "italic", "underline", "strike"]) {
  onButton(fmtButtons[key], () => toggleFormat(key));
}
onButton(clearFormattingBtn, () => {
  if (!hasRange()) return;
  if (reviewMode === "suggesting") {
    setStatus("Clear formatting is not tracked; switch to Editing to apply it", "error");
    return;
  }
  runToolbarEdit((a, b, c, d) => doc.clearFormatting(a, b, c, d));
});
function suggestRunFormat(patch) {
  return runToolbarEdit((sn, so, en, eo) => {
    if (sn !== en) throw new Error("Tracked formatting requires one paragraph");
    return doc.suggestFormat(
      sn,
      Math.min(so, eo),
      Math.max(so, eo),
      patch.bold,
      patch.italic,
      patch.underline,
      patch.strike,
      patch.sizeHalfPoints,
      patch.color,
      patch.highlight,
      patch.vertAlign,
      patch.font,
      undefined,
      new Date().toISOString(),
    );
  }, { allowInSuggesting: true });
}
/** A run-format control: apply to a range, or arm into `pendingFormat` at a caret
 *  (so the next typed text carries it — same model as the B/I/U/S toggles). */
function armOrApplyRun(patch, applyFn) {
  if (hasRange()) {
    if (reviewMode === "suggesting") suggestRunFormat(patch);
    else applyFn();
  } else if (selection) {
    pendingFormat = { ...(pendingFormat || {}), ...patch };
    updateToolbar();
  }
}
onButton(superBtn, () => {
  const value = superBtn.getAttribute("aria-pressed") === "true" ? "baseline" : "super";
  armOrApplyRun({ vertAlign: value }, () =>
    runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, value)),
  );
});
onButton(subBtn, () => {
  const value = subBtn.getAttribute("aria-pressed") === "true" ? "baseline" : "sub";
  armOrApplyRun({ vertAlign: value }, () =>
    runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, value)),
  );
});
for (const [key, btn] of Object.entries(alignBtns)) {
  onButton(btn, () => runToolbarEdit((a, b, c, d) => doc.setAlignment(a, b, c, d, key)));
}
/** Word-style indent commands: list items change numbering level, while ordinary
 * paragraphs retain the existing 0.25in paragraph-indent behavior. */
function adjustIndentCommand(delta) {
  if (!selection || !doc) return;
  const listKind = doc.listStyleAt(selection.focus.node);
  runToolbarEdit((a, b, c, d) =>
    listKind
      ? doc.adjustListLevel(a, b, c, d, delta > 0 ? 1 : -1)
      : doc.adjustIndent(a, b, c, d, delta),
  );
}
onButton(indentDecBtn, () => adjustIndentCommand(-360));
onButton(indentIncBtn, () => adjustIndentCommand(360));
onButton(bulletListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "bullet")));
onButton(numberedListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "numbered")));
onButton(restartListBtn, () => {
  if (selection && doc) runNodeEdit(() => doc.restartList(selection.focus.node));
});
onButton(continueListBtn, () => {
  if (selection && doc) runNodeEdit(() => doc.continueList(selection.focus.node));
});

fontSizeSel.addEventListener("change", () => {
  const pt = Number(fontSizeSel.value);
  const valid =
    fontSizeSel.value.trim() !== "" &&
    Number.isFinite(pt) &&
    pt >= 1 &&
    pt <= 1638 &&
    Number.isInteger(pt * 2);
  fontSizeSel.setCustomValidity(valid ? "" : "Enter a font size from 1 to 1638 pt in 0.5 pt steps.");
  if (!valid) {
    fontSizeSel.reportValidity();
    updateToolbar();
    return;
  }
  armOrApplyRun({ sizeHalfPoints: Math.round(pt * 2) }, () =>
    runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, pt)),
  );
});
// ---- Toolbar popovers (compact anchored menus such as spacing) --------------
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
  p.btn.setAttribute("aria-expanded", "true");
  p.reflect();
  const gutter = 8;
  const width = p.menu.offsetWidth;
  const height = p.menu.offsetHeight;
  const left = Math.min(
    Math.max(gutter, r.left),
    Math.max(gutter, window.innerWidth - width - gutter),
  );
  const below = r.bottom + 4;
  const above = r.top - height - 4;
  const top =
    below + height <= window.innerHeight - gutter
      ? below
      : Math.max(gutter, above);
  p.menu.style.left = `${Math.round(left)}px`;
  p.menu.style.top = `${Math.round(top)}px`;
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

tableStylePopover = registerPopover(tableStyleBtn, tableStyleMenu, () => {});

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

// -- Paragraph properties inspector ------------------------------------------
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

/** An inches field's value → signed twips; blank/non-numeric → 0. */
function signedInchTwips(input) {
  const raw = input.value.trim();
  if (raw === "" || !Number.isFinite(Number(raw))) return 0;
  return Math.round(Number(raw) * TWIPS_PER_INCH);
}

function setMixedCheckbox(input, state) {
  input.indeterminate = state === 2;
  input.checked = state === 1;
}

function reflectParagraphProperties() {
  if (!doc || !selection) return;
  const [startNode, startOffset, endNode, endOffset] = selEndpoints();
  const state = doc.selectionParagraphState(startNode, startOffset, endNode, endOffset);
  paragraphPropertiesContext.textContent =
    state.count === 1 ? "1 paragraph" : `${state.count} paragraphs`;

  paraPanelStyle.options[0].textContent = state.styleMixed ? "Mixed" : "Style";
  if (document.activeElement !== paraPanelStyle) {
    paraPanelStyle.value = state.styleMixed ? "" : state.style;
  }
  for (const button of paraPanelAlign.querySelectorAll("button[data-palign]")) {
    button.setAttribute(
      "aria-pressed",
      state.alignmentMixed ? "mixed" : String(button.dataset.palign === state.alignment),
    );
  }

  if (document.activeElement !== indentLeftInput) {
    indentLeftInput.placeholder = state.startMixed ? "Mixed" : "";
    indentLeftInput.value = state.startMixed ? "" : inchStr(state.startTwip);
  }
  if (document.activeElement !== indentRightInput) {
    indentRightInput.placeholder = state.endMixed ? "Mixed" : "";
    indentRightInput.value = state.endMixed ? "" : inchStr(state.endTwip);
  }
  if (![indentSpecialByInput, indentSpecialSel].includes(document.activeElement)) {
    if (state.firstLineMixed || state.hangingMixed) {
      indentSpecialSel.value = "";
      indentSpecialByInput.value = "";
      indentSpecialByInput.placeholder = "Mixed";
    } else if (state.firstLineTwip > 0) {
      indentSpecialSel.value = "first";
      indentSpecialByInput.value = inchStr(state.firstLineTwip);
      indentSpecialByInput.placeholder = "";
    } else if (state.hangingTwip > 0) {
      indentSpecialSel.value = "hanging";
      indentSpecialByInput.value = inchStr(state.hangingTwip);
      indentSpecialByInput.placeholder = "";
    } else {
      indentSpecialSel.value = "none";
      indentSpecialByInput.value = "";
      indentSpecialByInput.placeholder = "";
    }
  }

  if (document.activeElement !== paraLineSpacing) {
    paraLineSpacing.value = state.lineMixed ? "" : String(state.linePercent || "");
  }
  if (document.activeElement !== paraSpaceBefore) {
    paraSpaceBefore.placeholder = state.beforeMixed ? "Mixed" : "";
    paraSpaceBefore.value =
      state.beforeMixed || state.beforeTwip < 0
        ? ""
        : String(Math.round(state.beforeTwip / TWIPS_PER_POINT));
  }
  if (document.activeElement !== paraSpaceAfter) {
    paraSpaceAfter.placeholder = state.afterMixed ? "Mixed" : "";
    paraSpaceAfter.value =
      state.afterMixed || state.afterTwip < 0
        ? ""
        : String(Math.round(state.afterTwip / TWIPS_PER_POINT));
  }

  setMixedCheckbox(pgKeepNext, state.keepNextState);
  setMixedCheckbox(pgKeepLines, state.keepLinesState);
  setMixedCheckbox(pgBreakBefore, state.pageBreakBeforeState);
  paraShadeMixed.hidden = !state.shadingMixed;
  paraShadeNone.setAttribute(
    "aria-pressed",
    state.shadingMixed ? "mixed" : String(state.shading < 0),
  );
  if (!state.shadingMixed && state.shading >= 0 && document.activeElement !== paraShade) {
    paraShade.value = `#${state.shading.toString(16).padStart(6, "0")}`;
  }
  paraBordersMixed.hidden = !state.bordersMixed;
  const bit = { top: 1, bottom: 2, left: 4, right: 8 };
  for (const b of paragraphPropertiesPanel.querySelectorAll(".border-btn")) {
    const k = b.dataset.border;
    const on =
      k === "box"
        ? state.borderEdges === 0b1111
        : k === "none"
          ? state.borderEdges === 0
          : (state.borderEdges & bit[k]) !== 0;
    b.setAttribute("aria-pressed", state.bordersMixed ? "mixed" : String(on));
  }
  state.free();
}

function toggleParagraphProperties(open) {
  const show = open ?? paragraphPropertiesPanel.hidden;
  if (show && (!doc || !selection)) return;
  const returnFocus =
    !show && paragraphPropertiesPanel.contains(document.activeElement);
  paragraphPropertiesPanel.hidden = !show;
  paraOptsBtn.setAttribute("aria-expanded", String(show));
  if (show) {
    toggleTableProperties(false);
    for (const popover of popovers) closePopover(popover);
    reflectParagraphProperties();
    queueMicrotask(() => paraPanelStyle.focus());
  } else if (returnFocus) {
    paraOptsBtn.focus({ preventScroll: true });
  }
}

paraOptsBtn.addEventListener("click", (event) => {
  event.stopPropagation();
  toggleParagraphProperties();
});
paragraphPropertiesCloseBtn.addEventListener("click", () =>
  toggleParagraphProperties(false),
);
document.addEventListener("keydown", (event) => {
  if (
    event.key === "Escape" &&
    !paragraphPropertiesPanel.hidden &&
    (document.activeElement === paraOptsBtn ||
      paragraphPropertiesPanel.contains(document.activeElement))
  ) {
    event.preventDefault();
    toggleParagraphProperties(false);
  }
});

// Borders: presets toggle edges (box = all, none = clear) in the chosen color at a
// 1 pt single line (8 eighth-points).
for (const b of paragraphPropertiesPanel.querySelectorAll(".border-btn")) {
  onButton(b, () => {
    const [r, g, bl] = hexToRgb(borderColorInput.value);
    runToolbarEdit((a, x, c, d) => doc.setParagraphBorder(a, x, c, d, b.dataset.border, r, g, bl, 8));
    reflectParagraphProperties();
  });
}
paraPanelStyle.addEventListener("change", () =>
  runToolbarEdit((a, b, c, d) =>
    doc.setParagraphStyle(a, b, c, d, paraPanelStyle.value),
  ),
);
for (const button of paraPanelAlign.querySelectorAll("button[data-palign]")) {
  onButton(button, () =>
    runToolbarEdit((a, b, c, d) =>
      doc.setAlignment(a, b, c, d, button.dataset.palign),
    ),
  );
}
paraLineSpacing.addEventListener("change", () => {
  if (!paraLineSpacing.value) return;
  runToolbarEdit((a, b, c, d) =>
    doc.setLineSpacing(a, b, c, d, Number(paraLineSpacing.value)),
  );
});
paraSpaceBefore.addEventListener("change", () =>
  applySpace(paraSpaceBefore, (a, b, c, d, twips) =>
    doc.setSpaceBefore(a, b, c, d, twips),
  ),
);
paraSpaceAfter.addEventListener("change", () =>
  applySpace(paraSpaceAfter, (a, b, c, d, twips) =>
    doc.setSpaceAfter(a, b, c, d, twips),
  ),
);

// -- Table & cell formatting (a single-node edit: applies to the caret's cell) --
/** Runs a `(node) => EditResult` edit on the caret's node, preserving the selection
 *  and repainting only the dirty pages (rebuild on a page-count change). Every
 *  current caller is a table/list structural mutation with no tracked-revision
 *  representation (REVIEW-GAP-009), so this always fails closed in Suggesting
 *  mode instead of silently applying untracked (REVIEW-GAP-004). */
function runNodeEdit(thunk) {
  if (!selection || !doc) return false;
  if (blockMutationInViewing()) return false;
  if (blockUntrackedInSuggesting()) return false;
  let res;
  try {
    res = thunk(selection.focus.node);
  } catch (err) {
    console.warn("edit ignored:", err?.message ?? err);
    setStatus(err?.message ?? "Table change could not be applied", "error");
    return false;
  }
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  if (newCount !== pages.length) renderAll();
  else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ outline: true });
  return true;
}

function tableContextLabel(info) {
  return `${info.rows}×${info.columns} table · row ${info.row + 1}, column ${info.column + 1}${info.regular ? "" : " · merged/spanned"}`;
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
const tablePopover = registerPopover(tableBtn, tableFmtMenu, reflectTableMenu);

const TABLE_RIBBON_ACTIONS = {
  "insert-row-above": (n) => doc.insertRow(n, false),
  "insert-row-below": (n) => doc.insertRow(n, true),
  "insert-column-left": (n) => doc.insertColumn(n, false),
  "insert-column-right": (n) => doc.insertColumn(n, true),
  "delete-row": (n) => doc.deleteRow(n),
  "delete-column": (n) => doc.deleteColumn(n),
  "delete-table": (n) => doc.deleteTable(n),
};

for (const b of tableRibbon.querySelectorAll("[data-table-action]")) {
  onButton(b, () => {
    if (!selection || !doc) return;
    const run = TABLE_RIBBON_ACTIONS[b.dataset.tableAction];
    if (!run) return;
    tableSelection = null;
    runEdit(() => run(selection.focus.node), { gate: true });
  });
}

for (const b of tableRibbon.querySelectorAll("[data-table-distribute]")) {
  onButton(b, () => {
    if (!selection || !doc) return;
    const command = b.dataset.tableDistribute;
    tableSelection = null;
    runEdit(() =>
      command === "rows"
        ? doc.distributeTableRows(selection.focus.node)
        : doc.distributeTableColumns(selection.focus.node),
      { gate: true },
    );
  });
}

for (const b of tableRibbon.querySelectorAll("[data-table-sort]")) {
  onButton(b, () => {
    if (!selection || !doc) return;
    runEdit(() => doc.sortTable(selection.focus.node, b.dataset.tableSort), { gate: true });
  });
}

for (const b of tableRibbon.querySelectorAll("[data-table-select]")) {
  onButton(b, () => {
    if (!selection || !doc) return;
    const mode = b.dataset.tableSelect;
    tableSelection = { node: selection.focus.node, mode };
    drawSelection();
    setStatus(`Selected table ${mode}`);
    updateToolbar();
    focusEditorSurface();
  });
}

onButton(mergeCellsBtn, async () => {
  if (!selection || !doc) return;
  if (!tableSelection) {
    setStatus("Select a table row, column, or table first", "error");
    return;
  }
  await runEdit(() => doc.mergeTableSelection(tableSelection.node, tableSelection.mode), { gate: true });
  tableSelection = null;
  updateToolbar();
});

function toggleSplitCellDialog(open) {
  splitCellDialog.hidden = !open;
  if (open) {
    splitCellRows.value = "1";
    splitCellColumns.value = "2";
    splitCellColumns.focus();
  } else {
    splitCellBtn.focus({ preventScroll: true });
  }
}

onButton(splitCellBtn, () => {
  if (!selection || !doc) return;
  toggleSplitCellDialog(true);
});

onButton(splitCellClose, () => toggleSplitCellDialog(false));
onButton(splitCellCancel, () => toggleSplitCellDialog(false));
onButton(splitCellConfirm, async () => {
  if (!selection || !doc) return;
  const rows = Number.parseInt(splitCellRows.value, 10);
  const columns = Number.parseInt(splitCellColumns.value, 10);
  if (!Number.isInteger(rows) || !Number.isInteger(columns) || rows < 1 || columns < 1 || rows > 20 || columns > 20) {
    splitCellColumns.setCustomValidity("Enter whole numbers from 1 to 20.");
    splitCellColumns.reportValidity();
    return;
  }
  splitCellColumns.setCustomValidity("");
  await runEdit(() => doc.splitMergedCell(selection.focus.node, rows, columns), { gate: true });
  toggleSplitCellDialog(false);
  tableSelection = null;
  updateToolbar();
});

cellShade.addEventListener("change", () => {
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

// -- Table properties inspector ----------------------------------------------
let tablePropertiesCurrent = null;
let tablePropertiesNode = null;

function dialogTwipsValue(twips) {
  return twips < 0
    ? ""
    : (twips / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "") || "0";
}

function optionalDialogTwips(input) {
  const raw = input.value.trim();
  return raw === "" ? -1 : Math.round(Number(raw) * TWIPS_PER_INCH);
}

function updateTableRowHeightField() {
  const automatic = tableRowHeightRule.value === "auto";
  tableRowHeight.disabled = automatic;
  tableRowHeight.required = !automatic;
  if (automatic) tableRowHeight.setCustomValidity("");
}

function reflectTableProperties(node = selection?.focus.node) {
  if (!doc || node == null) return false;
  const info = doc.tableInfo(node);
  if (!info.found) {
    info.free();
    return false;
  }

  tablePropertiesNode = node;
  tablePropertiesContext.textContent = tableContextLabel(info);
  tableCaption.value = info.caption || "";
  tableDescription.value = info.description || "";
  tableHeaderRow.checked = info.headerRow;
  tableFixedLayout.checked = info.fixedLayout;
  tableColumnWidth.disabled = !info.regular;
  tableColumnWidth.value = dialogTwipsValue(info.columnWidthTwips);
  tableColumnWidthNote.textContent = info.regular
    ? "Sets the width of the current column."
    : "Column sizing is unavailable for merged or spanned tables.";
  tableWidth.value = dialogTwipsValue(info.tableWidthTwips);
  tableIndent.value = dialogTwipsValue(info.tableIndentTwips);
  tableRowHeight.value = dialogTwipsValue(info.rowHeightTwips);
  tableRowHeightRule.value = info.rowHeightRule || "auto";
  tableCellMargin.value = dialogTwipsValue(info.cellMarginTwips);
  tableCellSpacing.value = dialogTwipsValue(info.cellSpacingTwips);
  for (const button of tableAlign.querySelectorAll("button")) {
    button.setAttribute("aria-pressed", String(button.dataset.talign === info.alignment));
  }
  updateTableRowHeightField();

  tablePropertiesCurrent = {
    alignment: info.alignment,
    tableWidthTwips: optionalDialogTwips(tableWidth),
    tableIndentTwips: signedInchTwips(tableIndent),
    fixedLayout: info.fixedLayout,
    headerRow: info.headerRow,
    columnWidthTwips: optionalDialogTwips(tableColumnWidth),
    rowHeightTwips:
      tableRowHeightRule.value === "auto" ? -1 : optionalDialogTwips(tableRowHeight),
    rowHeightRule: info.rowHeightRule || "auto",
    cellMarginTwips: optionalDialogTwips(tableCellMargin),
    cellSpacingTwips: optionalDialogTwips(tableCellSpacing),
    caption: tableCaption.value,
    description: tableDescription.value,
  };
  info.free();
  return true;
}

function toggleTableProperties(open) {
  const show = open ?? tablePropertiesPanel.hidden;
  if (show && !reflectTableProperties()) return;
  const returnFocus = !show && tablePropertiesPanel.contains(document.activeElement);
  tablePropertiesPanel.hidden = !show;
  tablePropertiesBtn.setAttribute("aria-expanded", String(show));
  if (show) {
    toggleParagraphProperties(false);
    closePopover(tablePopover);
    queueMicrotask(() =>
      tableAlign.querySelector('button[aria-pressed="true"]')?.focus(),
    );
  } else if (returnFocus) {
    tablePropertiesBtn.focus({ preventScroll: true });
  }
}

tablePropertiesBtn.addEventListener("click", (event) => {
  event.stopPropagation();
  toggleTableProperties();
});
tableFormulaApply.addEventListener("click", () => {
  if (!selection || !doc || !tableFormula.value.trim()) return;
  runNodeEdit((node) => doc.calculateTableFormula(node, tableFormula.value));
});
tablePropertiesCloseBtn.addEventListener("click", () => toggleTableProperties(false));
tableAlign.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-talign]");
  if (!button) return;
  for (const candidate of tableAlign.querySelectorAll("button")) {
    candidate.setAttribute("aria-pressed", String(candidate === button));
  }
  commitTableProperties();
});
tablePropertiesPanel.addEventListener("change", (event) => {
  if (!(event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement)) {
    return;
  }
  if (event.target === tableRowHeightRule) updateTableRowHeightField();
  commitTableProperties();
});

function tablePropertiesPatch() {
  const inputs = [
    tableWidth,
    tableIndent,
    tableColumnWidth,
    tableRowHeight,
    tableCellMargin,
    tableCellSpacing,
  ].filter((input) => !input.disabled);
  for (const input of inputs) {
    input.setCustomValidity("");
    if (!input.checkValidity()) {
      input.reportValidity();
      input.focus();
      return null;
    }
  }
  if (tableRowHeightRule.value !== "auto" && tableRowHeight.value.trim() === "") {
    tableRowHeight.setCustomValidity("Enter a row height or choose Auto.");
    tableRowHeight.reportValidity();
    tableRowHeight.focus();
    return null;
  }

  const next = {
    alignment:
      tableAlign.querySelector('button[aria-pressed="true"]')?.dataset.talign ?? "left",
    tableWidthTwips: optionalDialogTwips(tableWidth),
    tableIndentTwips: signedInchTwips(tableIndent),
    fixedLayout: tableFixedLayout.checked,
    headerRow: tableHeaderRow.checked,
    columnWidthTwips: optionalDialogTwips(tableColumnWidth),
    rowHeightTwips:
      tableRowHeightRule.value === "auto" ? -1 : optionalDialogTwips(tableRowHeight),
    rowHeightRule: tableRowHeightRule.value,
    cellMarginTwips: optionalDialogTwips(tableCellMargin),
    cellSpacingTwips: optionalDialogTwips(tableCellSpacing),
    caption: tableCaption.value,
    description: tableDescription.value,
  };
  const patch = {};
  for (const [key, value] of Object.entries(next)) {
    if (key === "columnWidthTwips" && tableColumnWidth.disabled) continue;
    if (value !== tablePropertiesCurrent[key]) patch[key] = value;
  }
  // The bridge requires the value and rule together whenever row height changes.
  if ("rowHeightTwips" in patch || "rowHeightRule" in patch) {
    patch.rowHeightTwips = next.rowHeightTwips;
    patch.rowHeightRule = next.rowHeightRule;
  }
  return patch;
}

function commitTableProperties() {
  if (!doc || !tablePropertiesCurrent || !tablePropertiesNode) return;
  const patch = tablePropertiesPatch();
  if (!patch) return;
  if (Object.keys(patch).length === 0) {
    return;
  }
  const applied = runNodeEdit(() =>
    doc.applyTableProperties(tablePropertiesNode, JSON.stringify(patch)),
  );
  if (applied) {
    const activeNode =
      selection && doc.inTable(selection.focus.node) ? selection.focus.node : null;
    if (activeNode == null) toggleTableProperties(false);
    else reflectTableProperties(activeNode);
    setStatus("Table properties updated");
  }
}
document.addEventListener("keydown", (event) => {
  if (
    tablePropertiesPanel.hidden ||
    (document.activeElement !== tablePropertiesBtn &&
      !tablePropertiesPanel.contains(document.activeElement))
  ) {
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    toggleTableProperties(false);
  }
});

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
gridPicker.addEventListener("pointerdown", async (e) => {
  const cell = e.target.closest(".gc");
  if (!cell || !selection || !doc) return;
  e.preventDefault();
  const rows = Number(cell.dataset.r);
  const cols = Number(cell.dataset.c);
  await runEdit(() => doc.insertTable(selection.focus.node, rows, cols), { gate: true });
  closePopover(insertTablePopover);
  focusEditorSurface();
});
const insertTablePopover = registerPopover(insertTableBtn, insertTableMenu, () => highlightGrid(0, 0));

// ---- Off-screen accessibility tree (docs/67 row 9) --------------------------
/**
 * Rebuilds the read-only, off-screen structural mirror of the document from the
 * engine's `accessibilityTree()` projection so a screen reader can read the
 * canvas (which paints pixels only, exposing no structure). Headings become
 * `h1`–`h6` (levels 7–9 clamp to `h6`), list items group into `ul`/`ol`,
 * tables become real `table`/`tr`/`td`, and everything else is a `p`. This is
 * never an editing surface — the model stays the source of truth (docs/67 Open
 * Risks). Rebuilt on the same coalesced content-change frame as the outline.
 */
function buildAccessibilityTree() {
  if (!a11yDocument) return;
  if (!doc) {
    a11yDocument.replaceChildren();
    return;
  }
  let nodes;
  try {
    nodes = JSON.parse(doc.accessibilityTree());
  } catch {
    nodes = [];
  }
  const frag = document.createDocumentFragment();
  let listEl = null;
  let listOrdered = null;
  const flushList = () => {
    if (listEl) {
      frag.appendChild(listEl);
      listEl = null;
      listOrdered = null;
    }
  };
  for (const node of Array.isArray(nodes) ? nodes : []) {
    if (node.kind === "listItem") {
      if (!listEl || listOrdered !== node.ordered) {
        flushList();
        listEl = document.createElement(node.ordered ? "ol" : "ul");
        listOrdered = node.ordered;
      }
      const li = document.createElement("li");
      li.textContent = String(node.text ?? "");
      listEl.appendChild(li);
      continue;
    }
    flushList();
    if (node.kind === "heading") {
      const level = Math.min(6, Math.max(1, Number(node.level) || 1));
      const heading = document.createElement(`h${level}`);
      heading.textContent = String(node.text ?? "");
      frag.appendChild(heading);
    } else if (node.kind === "table") {
      const table = document.createElement("table");
      const tbody = document.createElement("tbody");
      for (const row of Array.isArray(node.rows) ? node.rows : []) {
        const tr = document.createElement("tr");
        for (const cell of Array.isArray(row) ? row : []) {
          const td = document.createElement("td");
          td.textContent = String(cell ?? "");
          tr.appendChild(td);
        }
        tbody.appendChild(tr);
      }
      table.appendChild(tbody);
      frag.appendChild(table);
    } else {
      const paragraph = document.createElement("p");
      paragraph.textContent = String(node.text ?? "");
      frag.appendChild(paragraph);
    }
  }
  flushList();
  a11yDocument.replaceChildren(frag);
}

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
    item.dataset.node = node;
    item.textContent = text;
    item.title = text;
    item.addEventListener("click", () => navigateToNode(node));
    outlineBody.appendChild(item);
  }
  reflectOutlineSelection();
}

/** Keeps the outline's active row synchronized with the model-backed caret. */
function reflectOutlineSelection() {
  const activeNode = selection?.focus?.node ?? "";
  for (const item of outlineBody.querySelectorAll(".outline-item")) {
    const active = item.dataset.node === activeNode;
    item.classList.toggle("is-active", active);
    if (active) item.setAttribute("aria-current", "location");
    else item.removeAttribute("aria-current");
  }
}

/** Places the caret at the start of `node` and scrolls it into view. */
function navigateToNode(node) {
  if (!doc) return;
  selection = { anchor: { node, offset: 0 }, focus: { node, offset: 0 } };
  drawSelection();
  scrollCaretIntoView("center");
}

function toggleOutline() {
  outlinePanel.hidden = !outlinePanel.hidden;
  railOutline.setAttribute("aria-pressed", String(!outlinePanel.hidden));
  buildOutline();
}
railOutline.addEventListener("click", toggleOutline);
outlineClose.addEventListener("click", toggleOutline);

function reviewText(value) {
  return value == null || value === "" ? "Not provided" : String(value);
}

function formatReviewDate(value) {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return String(value);
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

/**
 * Timestamp for a review action. Author/initials are no longer read here:
 * they come from the WASM engine's active host identity (`doc.setActiveAuthor`,
 * kept in sync with the Identity settings below), so callers pass `undefined`
 * for those arguments and let the engine fall back to it.
 */
function currentReviewTimestamp() {
  return { date: new Date().toISOString() };
}

let reviewFilter = "open";
let reviewReplyParent = null;

function focusReviewComment(comment, expand = true) {
  const anchor = comment?.anchor;
  if (!anchor?.node) return;
  reviewSidebarPreference = true;
  activeReviewCommentId = comment.id;
  if (expand) activeReviewItemId = `comment:${comment.id}`;
  selection = {
    anchor: { node: anchor.node, offset: Number(anchor.start) || 0 },
    focus: { node: anchor.node, offset: Number(anchor.end) || Number(anchor.start) || 0 },
  };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView("center");
  scheduleReviewMarginRender();
}

function closeReviewPopover() {
  reviewPopover?.remove();
  reviewPopover = null;
  reviewComposerState = null;
  reviewReplyParent = null;
  scheduleReviewMarginRender();
}

function showReviewPopover(item) {
  closeReviewPopover();
  if (!item) return;
  const popover = document.createElement("div");
  popover.className = "review-popover";
  popover.setAttribute("role", "dialog");
  const head = document.createElement("div");
  head.className = "review-popover-head";
  const author = document.createElement("strong");
  author.textContent = item.author || item.initials || (item.kind ? item.kind.replaceAll("_", " ") : "Comment");
  const meta = document.createElement("span");
  meta.className = "review-popover-meta";
  meta.textContent = item.date || (item.resolved ? "Resolved" : "Open");
  head.append(author, meta);
  const body = document.createElement("div");
  body.className = "review-popover-body";
  body.textContent = String(item.text || "");
  const actions = document.createElement("div");
  actions.className = "review-popover-actions";
  const addAction = (label, handler) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "review-card-action";
    button.textContent = label;
    button.addEventListener("click", async (event) => { event.stopPropagation(); await handler(); });
    actions.appendChild(button);
  };
  if (item.kind) {
    addAction("Accept", async () => { await runEdit(() => doc.decideRevision(item.id, true)); closeReviewPopover(); });
    addAction("Reject", async () => { await runEdit(() => doc.decideRevision(item.id, false)); closeReviewPopover(); });
  } else {
    addAction(item.resolved ? "Reopen" : "Resolve", async () => { await runEdit(() => doc.setCommentResolved(item.id, !item.resolved)); closeReviewPopover(); });
    addAction("Reply", async () => { closeReviewPopover(); openReviewComposer(item.id); });
    addAction("Delete", async () => { await runEdit(() => doc.deleteComment(item.id)); closeReviewPopover(); });
  }
  const close = document.createElement("button");
  close.type = "button";
  close.className = "panel-close";
  close.setAttribute("aria-label", "Close review item");
  close.textContent = "×";
  close.addEventListener("click", closeReviewPopover);
  head.appendChild(close);
  popover.append(head, body, actions);
  document.body.appendChild(popover);
  reviewPopover = popover;
  const rect = selection && doc ? doc.caretRect(selection.focus.node, selection.focus.offset) : [];
  const page = rect.length >= 5 ? pages[rect[0] - 1] : null;
  const scale = page ? scaleOf(page) : { sx: 1, sy: 1 };
  const pageRect = page?.canvas.getBoundingClientRect();
  const left = pageRect && rect.length >= 5 ? pageRect.left + rect[1] * scale.sx : window.innerWidth / 2;
  const top = pageRect && rect.length >= 5 ? pageRect.top + rect[2] * scale.sy : window.innerHeight / 2;
  popover.style.left = `${Math.max(12, Math.min(window.innerWidth - popover.offsetWidth - 12, left))}px`;
  popover.style.top = `${Math.max(12, Math.min(window.innerHeight - popover.offsetHeight - 12, top + 18))}px`;
}

/** Move the caret to the next/previous tracked text change.  Revision anchors
 * are intentionally resolved through the engine's text index rather than DOM
 * ranges, so this also works for changes inside tables and content controls. */
function navigateReviewRevision(direction) {
  if (!doc) return;
  const revisions = JSON.parse(doc.listRevisions()) ?? [];
  const usable = revisions.filter((revision) => String(revision.text || "").length);
  if (!usable.length) return;
  reviewRevisionCursor = (reviewRevisionCursor + (direction > 0 ? 1 : -1) + usable.length) % usable.length;
  const revision = usable[reviewRevisionCursor];
  // Announce which change the caret moved to, for a screen reader following
  // Next/Previous without watching the canvas (REVIEW-GAP-023).
  const changeKind = (reviewChangeTypeLabel(revision.kind) || "change").toLowerCase();
  announceReview(`Change ${reviewRevisionCursor + 1} of ${usable.length}: ${changeKind} by ${reviewAuthorDisplay(revision) || "You"}`);
  const range = revisionRange(revision);
  if (range) {
    selection = {
      anchor: { node: range.startNode, offset: range.startOffset },
      focus: { node: range.endNode, offset: range.endOffset },
    };
    drawSelection();
    focusEditorSurface();
    scrollCaretIntoView("center");
    syncActiveReviewCommentToCaret(selection.focus);
    return;
  }
  const start = selection?.focus || doc.firstPosition();
  const match = doc.findText(String(revision.text), start.node, start.offset, direction > 0, false);
  if (start.free) start.free();
  if (!match.found) {
    match.free();
    return;
  }
  selection = {
    anchor: { node: match.startNode, offset: match.startOffset },
    focus: { node: match.endNode, offset: match.endOffset },
  };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView("center");
  match.free();
  syncActiveReviewCommentToCaret(selection.focus);
}

/**
 * Moves the caret/selection to one end of a tracked move — its source
 * (`move_from`) or destination (`move_to`) anchor — and scrolls that location
 * to the centre of the viewport. This is the keyboard-accessible "go to the
 * original / new location" navigation a move review card exposes for both ends
 * of the move (REVIEW-GAP-016), so a reviewer can jump to precisely where the
 * text came from and where it went.
 */
function navigateToReviewAnchor(anchor) {
  if (!doc || !anchor?.node) return;
  reviewSidebarPreference = true;
  const start = Number(anchor.start) || 0;
  const rawEnd = Number(anchor.end);
  const end = Number.isFinite(rawEnd) ? rawEnd : start;
  selection = {
    anchor: { node: anchor.node, offset: start },
    focus: { node: anchor.node, offset: end },
  };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView("center");
}

/**
 * A keyboard-accessible navigation control for one end of a tracked move.
 * The visible secondary line is the precise page the end sits on (from the
 * same range geometry the sidebar uses to place cards), falling back to a
 * generic location label when geometry is unavailable (e.g. an off-screen or
 * not-yet-laid-out anchor). Activating it jumps the caret to that end.
 */
function reviewMoveEndButton(endLabel, anchor, fallbackLocation, action) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "review-move-end";
  const start = Number(anchor?.start) || 0;
  const rawEnd = Number(anchor?.end);
  const end = Number.isFinite(rawEnd) ? rawEnd : start;
  const rect = anchor?.node
    ? reviewRangeClientRect(anchor.node, start, anchor.node, end)
    : null;
  const locationText = rect ? `Page ${rect.pageNumber}` : fallbackLocation;
  const label = document.createElement("b");
  label.textContent = endLabel;
  const location = document.createElement("span");
  location.textContent = locationText;
  button.append(label, location);
  const description = rect ? `${action} (page ${rect.pageNumber})` : action;
  button.title = description;
  button.setAttribute("aria-label", description);
  button.disabled = !anchor?.node;
  button.addEventListener("click", (event) => {
    event.stopPropagation();
    navigateToReviewAnchor(anchor);
  });
  return button;
}

function focusReviewRevision(revision, expand = true) {
  if (!doc || !revision?.text) return;
  reviewSidebarPreference = true;
  if (expand) activeReviewItemId = `revision:${revision.id}`;
  // Clicking a card scrolls the canvas to that change's anchor (card→canvas
  // sync, REVIEW-GAP-019). For a tracked move, the destination is the sensible
  // default landing spot; its per-end buttons still jump to source/destination.
  if (revision.kind === "move" && revision.destinationAnchor?.node) {
    const dest = revision.destinationAnchor;
    selection = {
      anchor: { node: dest.node, offset: Number(dest.start) || 0 },
      focus: { node: dest.node, offset: Number(dest.end) || Number(dest.start) || 0 },
    };
    drawSelection();
    focusEditorSurface();
    scrollCaretIntoView("center");
    scheduleReviewMarginRender();
    return;
  }
  const range = revisionRange(revision);
  if (range) {
    selection = {
      anchor: { node: range.startNode, offset: range.startOffset },
      focus: { node: range.endNode, offset: range.endOffset },
    };
    drawSelection();
    focusEditorSurface();
    scrollCaretIntoView("center");
    scheduleReviewMarginRender();
    return;
  }
  const first = selection?.focus || doc.firstPosition();
  const match = doc.findText(String(revision.text), first.node, first.offset, true, false);
  if (first.free) first.free();
  if (!match.found) { match.free(); return; }
  selection = {
    anchor: { node: match.startNode, offset: match.startOffset },
    focus: { node: match.endNode, offset: match.endOffset },
  };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView("center");
  scheduleReviewMarginRender();
  match.free();
}

function toggleReview(open) {
  const show = open ?? reviewSidebar.hidden;
  reviewSidebarPreference = show;
  if (!show) {
    activeReviewItemId = null;
    activeReviewCommentId = null;
    reviewComposerState = null;
  }
  scheduleReviewMarginRender();
  // Focus management (REVIEW-GAP-023): closing the sidebar returns focus to the
  // rail toggle that owns it, so keyboard/AT users are not stranded.
  if (!show) railReview?.focus?.({ preventScroll: true });
}
reviewBtn.addEventListener("click", () => toggleReview());
railReview.addEventListener("click", () => toggleReview());
reviewClose.addEventListener("click", () => toggleReview(false));
reviewAcceptAll.addEventListener("click", async () => { if (doc) { await runEdit(() => doc.decideAllRevisions(true)); announceReview("All changes accepted"); scheduleReviewMarginRender(); } });
reviewRejectAll.addEventListener("click", async () => { if (doc) { await runEdit(() => doc.decideAllRevisions(false)); announceReview("All changes rejected"); scheduleReviewMarginRender(); } });
reviewPrevious.addEventListener("click", () => navigateReviewRevision(-1));
reviewNext.addEventListener("click", () => navigateReviewRevision(1));
// The visible mode control (`#reviewModeControl`) is a three-button segmented
// group; each button carries `data-review-mode` and is wired below.
suggestingBannerEdit.addEventListener("click", () => setReviewMode("editing"));
if (viewingBannerEdit) viewingBannerEdit.addEventListener("click", () => setReviewMode("editing"));
function openReviewComposer(parent = null) {
  if (!doc || (!parent && (!hasRange() || !selection))) return;
  reviewSidebarPreference = true;
  if (parent) {
    reviewReplyParent = parent;
    activeReviewItemId = `comment:${parent}`;
    reviewComposerState = null;
    scheduleReviewMarginRender();
    return;
  }
  const forward = selection.anchor.node === selection.focus.node
    && selection.anchor.offset <= selection.focus.offset;
  const start = forward ? { ...selection.anchor } : { ...selection.focus };
  const end = forward ? { ...selection.focus } : { ...selection.anchor };
  if (start.node !== end.node) {
    setStatus("Comments currently require a single-paragraph selection", "error");
    return;
  }
  reviewComposerState = { range: { start, end } };
  activeReviewItemId = null;
  scheduleReviewMarginRender();
}
// Comment authoring/reply uses the in-sidebar composer (`openReviewComposer` →
// `reviewComposerState`, rendered by `renderReviewMarginItems`); the legacy
// hidden side-panel composer was removed (docs/81 REVIEW-GAP-018/026).
selComment.addEventListener("mousedown", (event) => event.preventDefault());
selComment.addEventListener("click", () => openReviewComposer());
for (const filter of reviewFilters) {
  filter.addEventListener("click", () => {
    reviewFilter = filter.dataset.reviewFilter;
    for (const button of reviewFilters) {
      button.setAttribute("aria-pressed", String(button === filter));
    }
    const label = { open: "Open comments", resolved: "Resolved comments", all: "All comments" }[reviewFilter] || reviewFilter;
    announceReview(`Filter: ${label}`);
    scheduleReviewMarginRender();
  });
}
for (const mode of reviewModeButtons) {
  mode.addEventListener("click", () => setReviewMode(mode.dataset.reviewMode));
}

// ---- Command palette (⌘K) — fuzzy search over real editor actions -----------
const cmdPalette = document.getElementById("cmdPalette");
const cmdInput = document.getElementById("cmdInput");
const cmdList = document.getElementById("cmdList");
const searchTrigger = document.getElementById("searchTrigger");
let cmdMatches = [];
let cmdSel = 0;
let cmdReturnFocus = null;

/** Shared command descriptors for search and contextual surfaces. Dynamic
 * entries are rebuilt so document styles and availability never go stale. */
function editorCommands(context = { surface: "palette" }) {
  const fmt = (k) => () => toggleFormat(k);
  const align = (a) => () => runToolbarEdit((s, o, e, f) => doc.setAlignment(s, o, e, f, a));
  const cmds = [
    { id: "file.open", label: "Open…", group: "File", kw: "load docx", noDoc: true, run: () => fileEl.click() },
    { id: "file.save", label: "Save (download .docx)", group: "File", kw: "export download", shortcut: "⌘S", run: () => saveDocx() },
    {
      id: "edit.undo",
      label: doc?.undoLabel ? `Undo ${doc.undoLabel}` : "Undo",
      group: "Edit",
      kw: "revert",
      shortcut: "⌘Z",
      contextMenu: true,
      enabled: !!doc?.canUndo,
      disabledReason: "Nothing to undo",
      run: () => runEdit(() => doc.undo()),
    },
    {
      id: "edit.redo",
      label: doc?.redoLabel ? `Redo ${doc.redoLabel}` : "Redo",
      group: "Edit",
      kw: "",
      shortcut: "⌘⇧Z",
      contextMenu: true,
      enabled: !!doc?.canRedo,
      disabledReason: "Nothing to redo",
      run: () => runEdit(() => doc.redo()),
    },
    {
      id: "edit.cut",
      label: "Cut",
      group: "Clipboard",
      kw: "",
      shortcut: "⌘X",
      contextMenu: true,
      enabled: context.hasRange ?? hasRange(),
      disabledReason: "Select content to cut",
      run: () => cut(),
    },
    {
      id: "edit.copy",
      label: "Copy",
      group: "Clipboard",
      kw: "",
      shortcut: "⌘C",
      contextMenu: true,
      enabled: context.hasRange ?? hasRange(),
      disabledReason: "Select content to copy",
      run: () => copySelection(),
    },
    {
      id: "edit.paste",
      label: "Paste",
      group: "Clipboard",
      kw: "",
      shortcut: "⌘V",
      contextMenu: true,
      enabled: !!doc && !!selection,
      disabledReason: "Place the caret before pasting",
      run: () => paste(),
    },
    {
      id: "edit.selectAll",
      label: "Select all",
      group: "Clipboard",
      kw: "selection document",
      shortcut: "⌘A",
      contextMenu: true,
      enabled: !!doc,
      run: () => selectAll(),
    },
    { id: "edit.find", label: "Find and replace", group: "Edit", kw: "search replace", shortcut: "⌘F", run: () => openFind() },
    { id: "format.bold", label: "Bold", group: "Format", kw: "strong", shortcut: "⌘B", run: fmt("bold") },
    { id: "format.italic", label: "Italic", group: "Format", kw: "emphasis", shortcut: "⌘I", run: fmt("italic") },
    { id: "format.underline", label: "Underline", group: "Format", kw: "", shortcut: "⌘U", run: fmt("underline") },
    { id: "format.strike", label: "Strikethrough", group: "Format", kw: "strike", run: fmt("strike") },
    { id: "format.superscript", label: "Superscript", group: "Format", kw: "raise exponent", run: () => superBtn.click() },
    { id: "format.subscript", label: "Subscript", group: "Format", kw: "lower", run: () => subBtn.click() },
    { id: "format.clear", label: "Clear direct formatting", group: "Format", kw: "reset defaults", run: () => clearFormattingBtn.click() },
    { id: "paragraph.align.start", label: "Align left", group: "Paragraph", kw: "", run: align("start") },
    { id: "paragraph.align.center", label: "Align center", group: "Paragraph", kw: "centre", run: align("center") },
    { id: "paragraph.align.end", label: "Align right", group: "Paragraph", kw: "", run: align("end") },
    { id: "paragraph.align.justify", label: "Justify", group: "Paragraph", kw: "align", run: align("justify") },
    { id: "paragraph.list.bullet", label: "Bullet list", group: "Paragraph", kw: "unordered", run: () => runToolbarEdit((s, o, e, f) => doc.toggleList(s, o, e, f, "bullet")) },
    { id: "paragraph.list.numbered", label: "Numbered list", group: "Paragraph", kw: "ordered", run: () => runToolbarEdit((s, o, e, f) => doc.toggleList(s, o, e, f, "numbered")) },
    { id: "paragraph.list.restart", label: "Restart numbering", group: "Paragraph", kw: "list restart 1", run: () => selection && runNodeEdit(() => doc.restartList(selection.focus.node)) },
    { id: "paragraph.list.continue", label: "Continue numbering", group: "Paragraph", kw: "list continue resume", run: () => selection && runNodeEdit(() => doc.continueList(selection.focus.node)) },
    { id: "paragraph.indent.increase", label: "Increase indent", group: "Paragraph", kw: "", run: () => adjustIndentCommand(360) },
    { id: "paragraph.indent.decrease", label: "Decrease indent", group: "Paragraph", kw: "outdent", run: () => adjustIndentCommand(-360) },
    { id: "insert.table", label: "Insert table (3×3)", group: "Insert", kw: "grid", run: () => selection && runEdit(() => doc.insertTable(selection.focus.node, 3, 3), { gate: true }) },
    { id: "insert.link", label: "Add or edit link", group: "Insert", kw: "hyperlink url bookmark toc", shortcut: "⌘K", run: () => editSelectionLink() },
    { id: "insert.bookmark", label: "Bookmark manager", group: "Insert", kw: "bookmarks navigate links", run: () => openBookmarkManager() },
    { id: "view.outline", label: "Toggle outline", group: "View", kw: "headings navigation", run: () => toggleOutline() },
    { id: "view.zoomIn", label: "Zoom in", group: "View", kw: "", run: () => stepZoom(1) },
    { id: "view.zoomOut", label: "Zoom out", group: "View", kw: "", run: () => stepZoom(-1) },
    { id: "view.settings", label: "Settings", group: "View", kw: "theme accent dark", run: () => settingsBtn.click() },
    { id: "layout.pageSetup", label: "Page setup", group: "Layout", kw: "margins orientation paper size", run: () => togglePageSetup(true) },
    { id: "layout.paragraph", label: "Paragraph properties", group: "Layout", kw: "spacing borders shading indent", run: () => toggleParagraphProperties(true) },
    {
      id: "review.comment",
      label: "Add comment",
      group: "Review",
      kw: "annotate note",
      shortcut: "⌘⌥M",
      enabled: context.hasRange ?? hasRange(),
      disabledReason: "Select text to comment on",
      run: () => openReviewComposer(),
    },
    { id: "review.toggle", label: "Toggle comments & suggestions", group: "Review", kw: "sidebar review panel", run: () => toggleReview() },
    { id: "review.mode.editing", label: "Editing mode", group: "Review", kw: "review mode edit", run: () => setReviewMode("editing") },
    { id: "review.mode.suggesting", label: "Suggesting mode (track changes)", group: "Review", kw: "review mode track changes suggest", run: () => setReviewMode("suggesting") },
    { id: "review.mode.viewing", label: "Viewing mode (read-only)", group: "Review", kw: "review mode view read only", run: () => setReviewMode("viewing") },
    { id: "review.next", label: "Next change", group: "Review", kw: "revision suggestion navigate", run: () => navigateReviewRevision(1) },
    { id: "review.previous", label: "Previous change", group: "Review", kw: "revision suggestion navigate", run: () => navigateReviewRevision(-1) },
    { id: "review.acceptAll", label: "Accept all changes", group: "Review", kw: "revision suggestion approve", run: () => reviewAcceptAll.click() },
    { id: "review.rejectAll", label: "Reject all changes", group: "Review", kw: "revision suggestion discard", run: () => reviewRejectAll.click() },
  ];
  if (doc) {
    for (const name of doc.listStyles()) {
      cmds.push({
        id: `style.${name}`,
        label: `Style: ${name}`,
        group: "Style",
        kw: "paragraph heading",
        run: () => runToolbarEdit((s, o, e, f) => doc.setParagraphStyle(s, o, e, f, name)),
      });
    }
  }
  return cmds.filter((command) => doc || command.noDoc);
}

function buildCommands() {
  return editorCommands({ surface: "palette" });
}

function openBookmarkManager() {
  if (!doc) return;
  const names = doc.listBookmarks?.() || [];
  if (!names.length) {
    setStatus("No bookmarks in this document");
    return;
  }
  const choice = window.prompt(`Bookmarks:\n${names.join("\n")}\n\nEnter a bookmark name to jump:`, names[0]);
  if (choice === null) return;
  const encoded = doc.bookmarkPosition(choice.trim());
  const [node, offset] = encoded.split("\t");
  if (!node || !offset) {
    setStatus(`Bookmark “${choice.trim()}” was not found`, "error");
    return;
  }
  navToPosition({ node, offset: Number(offset) }, false);
}

function renderCommands(query) {
  const q = query.trim().toLowerCase();
  const all = buildCommands();
  cmdMatches = q
    ? all.filter((c) => `${c.label} ${c.group} ${c.kw}`.toLowerCase().includes(q))
    : all;
  cmdSel = cmdMatches.findIndex((command) => command.enabled !== false);
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
    item.disabled = c.enabled === false;
    if (c.disabledReason) item.title = c.disabledReason;
    // The hint column shows the disabled reason when unavailable, else the
    // command's keyboard shortcut when it has one (so the palette teaches the
    // shortcut), else its group.
    const hint = c.enabled === false ? c.disabledReason : (c.shortcut || c.group);
    item.innerHTML = `<span>${escapeHtml(c.label)}</span><span class="cmd-hint">${escapeHtml(hint)}</span>`;
    item.addEventListener("mousemove", () => setCmdSel(i));
    item.addEventListener("click", () => runCommand(i));
    cmdList.appendChild(item);
  });
}

function setCmdSel(i) {
  if (i < 0 || cmdMatches[i]?.enabled === false) return;
  cmdSel = i;
  const items = cmdList.querySelectorAll(".cmd-item");
  items.forEach((el, k) => el.classList.toggle("sel", k === i));
  items[i]?.scrollIntoView({ block: "nearest" });
}

function moveCmdSelection(direction) {
  if (!cmdMatches.some((command) => command.enabled !== false)) return;
  let index = cmdSel;
  for (let count = 0; count < cmdMatches.length; count++) {
    index = (index + direction + cmdMatches.length) % cmdMatches.length;
    if (cmdMatches[index].enabled !== false) {
      setCmdSel(index);
      return;
    }
  }
}

function runCommand(i) {
  const cmd = cmdMatches[i];
  if (!cmd || cmd.enabled === false) return;
  closeCmd();
  cmd.run();
}

function openCmd() {
  cmdReturnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  cmdPalette.hidden = false;
  searchTrigger.setAttribute("aria-expanded", "true");
  cmdInput.value = "";
  renderCommands("");
  cmdInput.focus();
}
function closeCmd() {
  cmdPalette.hidden = true;
  searchTrigger.setAttribute("aria-expanded", "false");
  cmdReturnFocus?.focus();
  cmdReturnFocus = null;
}

cmdInput.addEventListener("input", () => renderCommands(cmdInput.value));
cmdInput.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    moveCmdSelection(1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    moveCmdSelection(-1);
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
  const mod = e.metaKey || e.ctrlKey;
  if (!mod) return;
  const lower = e.key.toLowerCase();
  // Command palette: ⌘⇧P (the VS Code / editor-palette convention). Moved off
  // ⌘K so that ⌘K can carry the Word / Google Docs / Pages standard "insert or
  // edit hyperlink" — the two used to collide (docs/67 audit row 8). The header
  // Search pill is the discoverable on-screen entry point (doc 69 §1.4.1).
  if (e.shiftKey && lower === "p") {
    e.preventDefault();
    cmdPalette.hidden ? openCmd() : closeCmd();
    return;
  }
  // ⌘K inserts/edits a hyperlink on the current text selection. Skipped while a
  // chrome input (find box, dialog field, the palette itself) is focused so it
  // never hijacks typing there.
  if (!e.shiftKey && lower === "k" && doc && !isInteractiveChromeTarget(e.target)) {
    e.preventDefault();
    if (hasRange() && selection) {
      editSelectionLink();
    } else {
      setStatus("Select text to add a link", "error");
    }
    return;
  }
  if (lower === "s" && doc) {
    e.preventDefault();
    saveDocx();
  }
});
// Visible entry point for the palette (doc 69 §1.4.1): the shortcut already
// worked, it just had no on-screen affordance to discover it.
searchTrigger.addEventListener("click", openCmd);

// ---- Find / replace ---------------------------------------------------------
const FIND_SCAN_CAP = 5000;

function setFindStatus(text, miss = false) {
  findStatus.textContent = text;
  findStatus.classList.toggle("miss", miss);
}

/** Scans every match in document order, starting from the top, up to
 * FIND_SCAN_CAP — bounded like every other pagination/parse loop in this
 * codebase, since findText wraps around and would otherwise loop forever.
 * Once every match has been visited once, the engine's wrap fallback can
 * re-surface *any* earlier match (not necessarily the first one), so
 * termination checks membership in every key seen so far, not just the
 * first — comparing only to the first match's key under-counts (it can
 * oscillate between two later matches forever without ever revisiting the
 * exact first one). */
const findTextEncoder = new TextEncoder();
const findParagraphTextCache = new Map();

function paragraphTextForFind(node) {
  if (!findParagraphTextCache.has(node)) {
    const length = doc.paragraphLength(node);
    findParagraphTextCache.set(node, doc.copyText(node, 0, node, length));
  }
  return findParagraphTextCache.get(node);
}

function byteOffsetToStringIndex(text, byteOffset) {
  if (byteOffset <= 0) return 0;
  let bytes = 0;
  for (let i = 0; i < text.length; ) {
    if (bytes >= byteOffset) return i;
    const cp = text.codePointAt(i);
    const width = findTextEncoder.encode(String.fromCodePoint(cp)).length;
    bytes += width;
    i += cp > 0xffff ? 2 : 1;
  }
  return text.length;
}

function isWholeWordMatch(match, query) {
  const text = paragraphTextForFind(match.startNode);
  const start = byteOffsetToStringIndex(text, match.startOffset);
  const end = byteOffsetToStringIndex(text, match.endOffset);
  const word = /[\p{L}\p{N}_]/u;
  return !word.test(text[start - 1] || "") && !word.test(text[end] || "");
}

function clearFindParagraphCache() {
  findParagraphTextCache.clear();
}

function matchInFindSelection(match) {
  if (!findSelection.checked) return true;
  if (!findScope) return false;
  return match.startNode === findScope.node && match.startOffset >= findScope.start && match.endOffset <= findScope.end;
}

function scanAllMatches(query, matchCase, wholeWord = false) {
  const matches = [];
  if (!doc || !query) return matches;
  const first = doc.firstPosition();
  let node = first.node;
  let offset = first.offset;
  first.free();
  const seen = new Set();
  for (let i = 0; i < FIND_SCAN_CAP; i++) {
    const match = doc.findText(query, node, offset, true, matchCase);
    if (!match.found) {
      match.free();
      break;
    }
    const key = `${match.startNode}:${match.startOffset}`;
    if (seen.has(key)) {
      match.free();
      break;
    }
    seen.add(key);
    const candidate = {
      startNode: match.startNode,
      startOffset: match.startOffset,
      endNode: match.endNode,
      endOffset: match.endOffset,
    };
    if ((!wholeWord || isWholeWordMatch(candidate, query)) && matchInFindSelection(candidate)) {
      matches.push(candidate);
    }
    node = match.endNode;
    offset = match.endOffset;
    match.free();
  }
  return matches;
}

function updateFindStatus() {
  const query = findInput.value;
  if (!query) {
    setFindStatus("");
    return;
  }
  const matches = scanAllMatches(query, findCase.checked, findWholeWord.checked);
  if (!matches.length) {
    setFindStatus("No match", true);
    return;
  }
  if (matches.length === 1) {
    setFindStatus("1 match");
    return;
  }
  const idx = selection
    ? matches.findIndex(
        (m) => m.startNode === selection.anchor.node && m.startOffset === selection.anchor.offset,
      )
    : -1;
  setFindStatus(idx >= 0 ? `${idx + 1} of ${matches.length}` : `${matches.length} matches`);
}

function selectedPlainText() {
  if (!selection) return "";
  const { anchor, focus } = selection;
  if (anchor.node === focus.node && anchor.offset === focus.offset) return "";
  return doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset);
}

function queryMatchesSelection() {
  const query = findInput.value;
  if (!query) return false;
  const selected = selectedPlainText();
  if (!selected.includes("\n")) {
    return findCase.checked
      ? selected === query
      : selected.toLocaleLowerCase() === query.toLocaleLowerCase();
  }
  return false;
}

function selectTextMatch(match) {
  if (!match || match.found === false) {
    setFindStatus("No match", true);
    return false;
  }
  selection = {
    anchor: { node: match.startNode, offset: match.startOffset },
    focus: { node: match.endNode, offset: match.endOffset },
  };
  drawSelection();
  // Deliberately does NOT call focusEditorSurface(): this runs on every
  // keystroke while live-searching (findInput's "input" listener), and
  // stealing focus back to the canvas mid-typing sent subsequent keystrokes
  // to the document instead of the find box. Focus returns to the canvas
  // only when the panel actually closes (closeFind).
  scrollCaretIntoView();
  updateFindStatus();
  return true;
}

function findFromSelection(forward) {
  if (!doc || !findInput.value) {
    setFindStatus("");
    return false;
  }
  const matches = scanAllMatches(findInput.value, findCase.checked, findWholeWord.checked);
  if (!matches.length) {
    setFindStatus("No match", true);
    return false;
  }
  const current = selection
    ? matches.findIndex(
        (m) => m.startNode === selection.anchor.node && m.startOffset === selection.anchor.offset,
      )
    : -1;
  const index = current >= 0
    ? (current + (forward ? 1 : matches.length - 1)) % matches.length
    : forward ? 0 : matches.length - 1;
  return selectTextMatch(matches[index]);
}

async function replaceCurrentMatch() {
  if (!doc || !findInput.value) return;
  if (blockUntrackedInSuggesting()) return;
  if (!queryMatchesSelection()) {
    findFromSelection(true);
    return;
  }
  const { anchor, focus } = selection;
  await runEdit(() =>
    doc.replaceSelection(anchor.node, anchor.offset, focus.node, focus.offset, replaceInput.value),
  );
  findFromSelection(true);
}

/** Replaces every match in document order. Each iteration re-finds from just
 * past the previous replacement (not from the top), so a replacement text
 * that itself contains the query (e.g. "cat" -> "cats") can never re-match
 * what was just inserted and loop forever; FIND_SCAN_CAP is a bounded
 * backstop regardless. */
async function replaceAllMatches() {
  if (!doc || !findInput.value) return;
  if (blockUntrackedInSuggesting()) return;
  const query = findInput.value;
  const replacement = replaceInput.value;
  const matchCase = findCase.checked;
const wholeWord = findWholeWord.checked;
  const replacementBytes = new TextEncoder().encode(replacement).length;
  const first = doc.firstPosition();
  let node = first.node;
  let offset = first.offset;
  first.free();
  let count = 0;
  for (let i = 0; i < FIND_SCAN_CAP; i++) {
    const match = doc.findText(query, node, offset, true, matchCase);
    if (!match.found) {
      match.free();
      break;
    }
    const candidate = {
      startNode: match.startNode,
      startOffset: match.startOffset,
      endNode: match.endNode,
      endOffset: match.endOffset,
    };
    const { startNode, startOffset, endNode, endOffset } = candidate;
    match.free();
    if (
      (wholeWord && !isWholeWordMatch(candidate, query)) ||
      !matchInFindSelection(candidate)
    ) {
      node = endNode;
      offset = endOffset;
      continue;
    }
    await runEdit(() => doc.replaceSelection(startNode, startOffset, endNode, endOffset, replacement));
    count++;
    node = startNode;
    offset = startOffset + replacementBytes;
  }
  setFindStatus(count ? `Replaced ${count}` : "No match", count === 0);
}

function openFind() {
  if (!doc) return;
  findPanel.hidden = false;
  if (findSelection.checked) findSelection.dispatchEvent(new Event("change"));
  const selected = selectedPlainText();
  if (selected && !selected.includes("\n") && selected.length <= 80) findInput.value = selected;
  updateFindStatus();
  findInput.focus();
  findInput.select();
}

function closeFind() {
  findPanel.hidden = true;
  focusEditorSurface();
}

findInput.addEventListener("input", () => {
  if (findInput.value) findFromSelection(true);
  else setFindStatus("");
});
findCase.addEventListener("change", () => updateFindStatus());
findWholeWord.addEventListener("change", () => updateFindStatus());
findSelection.addEventListener("change", () => {
  if (findSelection.checked) {
    findScope =
      selection && hasRange() && selection.anchor.node === selection.focus.node
        ? {
            node: selection.anchor.node,
            start: Math.min(selection.anchor.offset, selection.focus.offset),
            end: Math.max(selection.anchor.offset, selection.focus.offset),
          }
        : null;
  } else {
    findScope = null;
  }
  updateFindStatus();
});
findInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    findFromSelection(!e.shiftKey);
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeFind();
  }
});
replaceInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    replaceCurrentMatch();
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeFind();
  }
});
findCase.addEventListener("change", () => findFromSelection(true));
findPrevBtn.addEventListener("click", () => findFromSelection(false));
findNextBtn.addEventListener("click", () => findFromSelection(true));
replaceOneBtn.addEventListener("click", replaceCurrentMatch);
replaceAllBtn.addEventListener("click", replaceAllMatches);
findCloseBtn.addEventListener("click", closeFind);
findBtn.addEventListener("click", () => openFind());
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    openFind();
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

paraShade.addEventListener("change", () => {
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
  if (!name) return;
  armOrApplyRun({ highlight: name }, () =>
    runToolbarEdit((a, b, c, d) => doc.setHighlight(a, b, c, d, name)),
  );
});
textColorInput.addEventListener("change", () => {
  const hex = textColorInput.value;
  const [r, g, b] = hexToRgb(hex);
  armOrApplyRun({ color: hex }, () =>
    runToolbarEdit((a, bo, c, d) => doc.setTextColor(a, bo, c, d, r, g, b)),
  );
});

// ---- Floating selection toolbar (appears above a text selection) ------------
const selToolbar = document.getElementById("selToolbar");
const selColor = document.getElementById("selColor");
const selHighlight = document.getElementById("selHighlight");

/** Shows the floating toolbar centred just above the current range selection (or
 *  below it when there's no room), or hides it when the selection is collapsed. */
function positionSelToolbar() {
  if (activeLink || !selection || !hasRange()) {
    selToolbar.hidden = true;
    return;
  }
  const rects = pagesEl.querySelectorAll(".overlay .highlight");
  if (!rects.length) {
    selToolbar.hidden = true;
    return;
  }
  let top = Infinity;
  let bottom = -Infinity;
  let left = Infinity;
  let right = -Infinity;
  for (const el of rects) {
    const b = el.getBoundingClientRect();
    top = Math.min(top, b.top);
    bottom = Math.max(bottom, b.bottom);
    left = Math.min(left, b.left);
    right = Math.max(right, b.right);
  }
  selToolbar.hidden = false; // must be visible to measure
  const tw = selToolbar.offsetWidth;
  const th = selToolbar.offsetHeight;
  const viewport = viewportEl.getBoundingClientRect();
  const topBound = Math.max(8, viewport.top + 8);
  const bottomBound = Math.min(window.innerHeight - 8, viewport.bottom - 8);
  let x = (left + right) / 2 - tw / 2;
  let y = top - th - 8;
  if (y < topBound) y = bottom + 8; // no room above → drop below the selection
  if (y + th > bottomBound) {
    // A selection at the viewport bottom may have no full-height slot below it;
    // keep the bar visible and out of the browser chrome rather than allowing a
    // fixed-position toolbar to disappear below the window.
    y = Math.min(y, bottomBound - th);
  }
  y = Math.max(topBound, y);
  x = Math.max(8, Math.min(x, window.innerWidth - tw - 8));
  selToolbar.style.left = `${Math.round(x)}px`;
  selToolbar.style.top = `${Math.round(y)}px`;
}

for (const b of selToolbar.querySelectorAll("[data-fmt]")) {
  onButton(b, () => toggleFormat(b.dataset.fmt));
}
selColor.addEventListener("change", () => {
  const [r, g, b] = hexToRgb(selColor.value);
  armOrApplyRun({ color: selColor.value }, () =>
    runToolbarEdit((a, bo, c, d) => doc.setTextColor(a, bo, c, d, r, g, b)),
  );
});
selHighlight.addEventListener("change", () => {
  const name = selHighlight.value;
  armOrApplyRun({ highlight: name }, () =>
    runToolbarEdit((a, b, c, d) => doc.setHighlight(a, b, c, d, name)),
  );
  selHighlight.value = "none";
});
// Keep clicks inside the bar from collapsing the selection; hide on viewport scroll.
selToolbar.addEventListener("mousedown", (e) => {
  if (e.target.tagName !== "INPUT" && e.target.tagName !== "SELECT") e.preventDefault();
});
viewportEl.addEventListener("scroll", () => (selToolbar.hidden = true), { passive: true });
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

/** Moves the caret to an engine Caret result (document bounds). Shift extends. */
function navToPosition(caret, extend) {
  breakTypingSession();
  pendingFormat = null; // caret moved → disarm typing format
  const to = { node: caret.node, offset: caret.offset };
  if (typeof caret.free === "function") caret.free();
  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView();
}

/** Page Up/Down moves the model caret by one visible editor viewport. Browser
 * geometry chooses only the page-local probe; `hitTest` returns the model anchor
 * that remains the source of truth. */
function navByViewport(dir, extend) {
  if (!selection) return;
  breakTypingSession();
  pendingFormat = null;
  const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
  if (flat.length < 5) return;

  const [pageNumber, x, y, w] = flat;
  const caretPage = pages[pageNumber - 1];
  if (!caretPage) return;
  const { rect: pageRect, sx, sy } = scaleOf(caretPage);
  const viewportRect = viewportEl.getBoundingClientRect();
  const distance = Math.max(48, viewportRect.height - 48);
  const targetX = pageRect.left + x * sx + Math.max(1, (w * sx) / 2);
  const targetY = pageRect.top + y * sy + (dir === "pageUp" ? -distance : distance);
  const page = pageFromClientPoint(targetX, targetY);
  if (!page) return;
  const to = anchorAt(page, clientPointEvent(targetX, targetY));
  if (!to) return;

  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView();
}

/** Selects the whole document (⌘A). */
function selectAll() {
  if (!doc) return;
  breakTypingSession();
  const a = doc.firstPosition();
  const b = doc.lastPosition();
  selection = {
    anchor: { node: a.node, offset: a.offset },
    focus: { node: b.node, offset: b.offset },
  };
  a.free();
  b.free();
  drawSelection();
  focusEditorSurface();
}

function editorClipboardEvent(event) {
  return doc && selection && !isInteractiveChromeTarget(event.target) && eventTargetsEditor(event);
}

function editorTextInputEvent(event) {
  return doc && selection && !isInteractiveChromeTarget(event.target) && eventTargetsEditor(event);
}

/** Cut (⌘X): copy the selection to the clipboard, then delete it. */
async function cut(event = null) {
  if (!hasRange()) return;
  // Cut is a mutation (copy + delete); in read-only Viewing mode it is blocked
  // before touching the clipboard so it never partially executes as a copy.
  if (blockMutationInViewing()) return;
  const copied = await copySelection(event);
  if (!copied) return;
  const { anchor, focus } = selection;
  if (reviewMode === "suggesting" && anchor.node !== focus.node) {
    setStatus("Cross-paragraph cuts cannot be tracked yet; switch to Editing to cut", "error");
    return;
  }
  await runEdit(() => reviewMode === "suggesting" && anchor.node === focus.node
    ? doc.suggestDelete(anchor.node, Math.min(anchor.offset, focus.offset), Math.max(anchor.offset, focus.offset), undefined, new Date().toISOString())
    : doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset));
}

async function pasteText(text, actionKind = "paste") {
  if (!doc || !selection) return;
  if (!text) return;
  const { anchor, focus } = selection;
  const sameParagraph = anchor.node === focus.node && !text.includes("\n");
  if (reviewMode === "suggesting" && sameParagraph) {
    const start = Math.min(anchor.offset, focus.offset);
    const end = Math.max(anchor.offset, focus.offset);
    await runEdit(() => end > start
      ? doc.suggestReplace(anchor.node, start, end, text, undefined, new Date().toISOString())
      : doc.suggestInsert(anchor.node, start, text, undefined, new Date().toISOString()));
    return;
  }
  if (reviewMode === "suggesting") {
    setStatus("Multi-paragraph paste cannot be tracked yet; switch to Editing to paste it", "error");
    return;
  }
  await runEdit(() => doc.insertPlainTextAs(anchor.node, anchor.offset, focus.node, focus.offset, text, actionKind));
}

async function commitComposedText(text) {
  if (!text) return;
  pendingFormat = null;
  await pasteText(text, "typing");
}

/**
 * Suggesting-mode rich paste at a collapsed caret, single paragraph only
 * (REVIEW-GAP-008): inserts each clipboard run as its own tracked
 * `suggestStyledInsert`, chained under one gesture (`typingSessionForKey`)
 * so the whole paste is one review card and one Undo step — the same
 * paragraph-snapshot coalescing real adjacent keystrokes already use (docs
 * 82 §3; see `suggest_insert`'s `continuing_group` in casual-doc-wasm).
 * This is what lets a rich paste keep its bold/italic/color/etc. per run
 * instead of flattening to one plain-text tracked insertion.
 *
 * Returns whether it handled the paste. It does not: multi-paragraph
 * content (`paragraphBreak` runs — REVIEW-GAP-009's structural-tracking
 * backlog), or a paste that also needs to replace an existing selection (no
 * tracked multi-run *replacement* group exists yet — the model's
 * `RevisionGroupKind::Replacement` requires exactly one deletion plus one
 * insertion). Both remain the existing flattened-plain-text fallback in
 * `pasteRichRunsJson`. A run's `href` (hyperlink) is not carried into the
 * tracked insertion either — the same as the existing flattened fallback,
 * so this is not a regression, just an unchanged, explicit limitation.
 */
async function pasteTrackedRichRuns(runs) {
  if (!doc || !selection || hasRange()) return false;
  if (!Array.isArray(runs) || runs.some((run) => run.paragraphBreak)) return false;
  const insertable = runs.filter((run) => !run.paragraphBreak && run.text);
  if (!insertable.length) return false;

  breakTypingSession();
  const session = typingSessionForKey();
  const node = selection.focus.node;
  let offset = selection.focus.offset;
  for (const run of insertable) {
    await runEdit(
      () => doc.suggestStyledInsert(
        node,
        offset,
        run.text,
        run.bold,
        run.italic,
        run.underline,
        run.strike,
        run.sizeHalfPoints,
        run.color,
        run.highlight,
        run.vertAlign,
        run.font,
        undefined,
        new Date().toISOString(),
        session,
      ),
      { typing: true },
    );
    offset += run.text.length;
  }
  // Close the gesture explicitly so a later, unrelated keystroke cannot
  // merge into this paste's group (mirrors the pause-based boundary real
  // typing uses; nothing else calls `typingSessionForKey` between here and
  // the next real key).
  breakTypingSession();
  return true;
}

/** Replaces the selection with a rich-run clipboard fragment, as one
 * undoable action (`doc.pasteRichRuns` — the paste counterpart of
 * `copyRichRuns`). `runsJson` must be a JSON array in the shape
 * `copyRichRuns` produces. */
async function pasteRichRunsJson(runsJson) {
  if (!doc || !selection) return;
  if (reviewMode === "suggesting") {
    let runs = null;
    try {
      runs = JSON.parse(runsJson);
    } catch { /* malformed payload; runs stays null */ }
    if (await pasteTrackedRichRuns(runs)) return;
    // Falls back to a flattened, single-format tracked replace/insert for
    // the cases `pasteTrackedRichRuns` intentionally does not cover yet
    // (see its doc comment): an existing selection to replace, or
    // multi-paragraph content.
    const text = Array.isArray(runs)
      ? runs.map((run) => run.paragraphBreak ? "\n" : String(run.text ?? "")).join("")
      : "";
    if (text) {
      await pasteText(text, "paste");
      return;
    }
    // No plain-text fallback could be derived (malformed clipboard payload, or
    // an entirely non-text rich fragment) — `doc.pasteRichRuns` has no tracked
    // representation, so this must fail closed rather than silently apply
    // untracked (REVIEW-GAP-004).
    blockUntrackedInSuggesting();
    return;
  }
  const { anchor, focus } = selection;
  await runEdit(() =>
    doc.pasteRichRuns(anchor.node, anchor.offset, focus.node, focus.offset, runsJson),
  );
}

/** Tries to paste `html` as a rich fragment: the internal round-trip marker
 * if present (an OpenDoc-to-OpenDoc copy, lossless), else a best-effort
 * sanitized parse of the DOM (an external app's paste — Word, Docs, a
 * browser selection). Returns whether anything was pasted, so the caller can
 * fall back to plain text when `html` carries no usable content. */
async function pasteHtml(html) {
  if (!html) return false;
  const internal = extractMarker(html);
  if (internal) {
    let parsed = null;
    try {
      parsed = JSON.parse(internal);
    } catch { /* not JSON — treat as no usable internal payload below */ }
    // A structured fragment (`{ blocks, runs }`) reconstructs tables/lists in
    // Editing mode; Suggesting mode has no tracked representation for structural
    // paste (GAP-009), so it uses the flat runs. If the engine declines the
    // structured paste (a range selection, or a caret inside a table cell), fall
    // back to the flat runs too.
    if (parsed && !Array.isArray(parsed) && Array.isArray(parsed.blocks)) {
      if (
        reviewMode !== "suggesting" &&
        (await pasteStructured(JSON.stringify({ blocks: parsed.blocks })))
      ) {
        return true;
      }
      await pasteRichRunsJson(JSON.stringify(parsed.runs ?? []));
      return true;
    }
    await pasteRichRunsJson(internal);
    return true;
  }
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const runs = htmlToRuns(parsed.body);
  if (!runs.length) return false;
  await pasteRichRunsJson(JSON.stringify(runs));
  return true;
}

/** Editing-mode structured paste: reconstructs a copied fragment of tables and
 * list paragraphs at the caret via `doc.pasteStructured`, as one undoable
 * action. Returns true when applied; false when the engine declines (a range
 * selection, or a caret that is not a top-level body paragraph), so the caller
 * falls back to the flat rich-run paste. Calls the engine directly (not through
 * `runEdit`, which swallows the decline) so the fallback can see it. */
async function pasteStructured(fragmentJson) {
  if (!doc || !selection) return false;
  const { anchor, focus } = selection;
  breakTypingSession();
  let res;
  try {
    res = doc.pasteStructured(anchor.node, anchor.offset, focus.node, focus.offset, fragmentJson);
  } catch {
    return false;
  }
  await applyEditResult(res);
  return true;
}

/** Paste (⌘V): insert clipboard content at the caret, replacing any
 *  selection. Rich HTML (internal or external) wins when present; plain text
 *  with newline-as-paragraph-split remains the fallback. */
async function paste(event = null) {
  if (!doc || !selection) return;
  // Read-only Viewing mode blocks paste up front (it still calls
  // preventDefault below) so no clipboard read or insertion is attempted.
  if (reviewMode === "viewing") {
    if (event?.clipboardData) event.preventDefault();
    blockMutationInViewing();
    return;
  }
  if (event?.clipboardData) {
    event.preventDefault();
    const html = event.clipboardData.getData("text/html");
    if (await pasteHtml(html)) return;
    await pasteText(event.clipboardData.getData("text/plain"));
    return;
  }
  try {
    if (navigator.clipboard.read) {
      const items = await navigator.clipboard.read();
      for (const item of items) {
        if (!item.types.includes("text/html")) continue;
        const html = await (await item.getType("text/html")).text();
        if (await pasteHtml(html)) return;
      }
    }
    const text = await navigator.clipboard.readText();
    await pasteText(text);
  } catch (err) {
    console.warn("paste failed:", err);
    setStatus("Clipboard paste was blocked by the browser", "err");
  }
}

document.addEventListener("copy", (e) => {
  if (editorClipboardEvent(e) && hasRange()) copySelection(e);
});
document.addEventListener("cut", (e) => {
  if (editorClipboardEvent(e) && hasRange()) cut(e);
});
document.addEventListener("paste", (e) => {
  if (editorClipboardEvent(e)) paste(e);
});
document.addEventListener("compositionstart", (e) => {
  if (!editorTextInputEvent(e)) return;
  breakTypingSession();
  composingText = true;
  pendingFormat = null;
  if (selection) showImePreedit(selection.focus.node, selection.focus.offset, e.data || "");
});
document.addEventListener("compositionupdate", (e) => {
  if (!composingText) return;
  updateImePreedit(e.data || "");
});
document.addEventListener("compositionend", async (e) => {
  hideImePreedit();
  if (!editorTextInputEvent(e)) {
    composingText = false;
    return;
  }
  e.preventDefault();
  composingText = false;
  await commitComposedText(e.data || "");
});

const FORMAT_KEYS = { b: "bold", i: "italic", u: "underline" };

document.addEventListener("keydown", async (e) => {
  if (!doc) return;
  // The canvas editor owns keystrokes only while its focus owner is active.
  // Chrome controls, popovers, and link chips keep normal browser semantics.
  if (isInteractiveChromeTarget(e.target) || !eventTargetsEditor(e)) return;

  const mod = e.metaKey || e.ctrlKey;
  const key = e.key;
  const lower = key.toLowerCase();

  if (composingText || e.isComposing || key === "Process") return;

  // The object interaction grammar (docs/85 §4) owns the keyboard while an
  // object is selected. Escape is the two-step exit (editing → selected → text);
  // Enter/Delete act on the object; a selected object swallows text keys so a
  // stale caret is never edited.
  if (objectSelection) {
    if (key === "Escape") {
      e.preventDefault();
      if (objectSelection.mode === "editing") {
        objectSelection = { ...objectSelection, mode: "selected" };
      } else {
        objectSelection = null; // collapse to the surrounding-text caret
      }
      drawSelection();
      return;
    }
    if (objectSelection.mode === "selected") {
      if (key === "Enter") {
        e.preventDefault();
        enterObjectEditMode(); // double-click's keyboard twin (§4.3)
        return;
      }
      if (key === "Delete" || key === "Backspace") {
        e.preventDefault();
        setStatus("Deleting an object is a later editing slice");
        return;
      }
      // Swallow text-producing keys; navigation/modifier combos fall through so
      // the user can still move the caret off the object.
      if (key.length === 1 && !mod) {
        e.preventDefault();
        return;
      }
    }
  }

  // Word's Windows/Linux shortcut for clearing direct character formatting.
  // macOS keeps Ctrl+Space available to the host/input source.
  if (e.ctrlKey && !e.metaKey && key === " " && hasRange()) {
    e.preventDefault();
    if (reviewMode === "suggesting") {
      setStatus("Clear formatting is not tracked; switch to Editing to apply it", "error");
      return;
    }
    await runToolbarEdit((a, b, c, d) => doc.clearFormatting(a, b, c, d));
    return;
  }

  // Clipboard, select-all, history (⌘/Ctrl based).
  if (mod && lower === "c") {
    e.preventDefault();
    breakTypingSession();
    await copySelection();
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
    breakTypingSession();
    toggleFormat(FORMAT_KEYS[lower]);
    return;
  }

  if (!selection) return;

  // Navigation uses an explicit macOS/Windows keymap. It runs before the
  // generic modifier guard so the supported Ctrl/Command/Option combinations
  // reach semantic engine moves. Shift extends every navigation intent.
  const navDir = navigationDirection(e, EDITOR_KEYBOARD_PLATFORM);
  if (navDir === "docStart" || navDir === "docEnd") {
    e.preventDefault();
    navToPosition(navDir === "docStart" ? doc.firstPosition() : doc.lastPosition(), e.shiftKey);
    return;
  }
  if (navDir === "pageUp" || navDir === "pageDown") {
    e.preventDefault();
    navByViewport(navDir, e.shiftKey);
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
    if (doc.inTable(selection.focus.node)) {
      try {
        const c = doc.moveTableCell(selection.focus.node, !e.shiftKey);
        navToPosition(c, false);
      } catch {
        // First/last-cell boundaries are expected no-ops for this navigation slice.
      }
      return;
    }
    if (reviewMode === "suggesting") {
      setStatus("Indent and list structure changes cannot be tracked yet; switch to Editing", "error");
      return;
    }
    const listKind = doc.listStyleAt(selection.focus.node);
    if (listKind) {
      await runEdit(() =>
        doc.adjustListLevel(
          selection.anchor.node,
          selection.anchor.offset,
          selection.focus.node,
          selection.focus.offset,
          e.shiftKey ? -1 : 1,
        ),
      );
    } else {
      await runToolbarEdit((a, b, c, d) => doc.adjustIndent(a, b, c, d, e.shiftKey ? -360 : 360));
    }
    return;
  }

  const { anchor, focus } = selection;
  const range = hasRange();
  const wordDelete = wordDeletionDirection(e, EDITOR_KEYBOARD_PLATFORM);

  if (wordDelete) {
    e.preventDefault();
    if (reviewMode === "suggesting") {
      const start = range
        ? (anchor.offset <= focus.offset ? anchor : focus)
        : wordDelete === "backward"
          ? (() => { const c = doc.moveCaret(focus.node, focus.offset, "wordLeft"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })()
          : focus;
      const end = range
        ? (anchor.offset <= focus.offset ? focus : anchor)
        : wordDelete === "forward"
          ? (() => { const c = doc.moveCaret(focus.node, focus.offset, "wordRight"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })()
          : focus;
      if (start.node === end.node && start.offset < end.offset) {
        await runEdit(() => doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
      } else {
        setStatus("This word deletion crosses a paragraph and cannot be tracked yet", "error");
      }
      return;
    }
    await runEdit(() =>
      range
        ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset)
        : wordDelete === "backward"
          ? doc.deleteWordBackward(focus.node, focus.offset)
          : doc.deleteWordForward(focus.node, focus.offset),
    );
    return;
  }

  if (mod) {
    breakTypingSession();
    return; // leave other ⌘/Ctrl shortcuts to the browser
  }

  if (key === "Backspace") {
    e.preventDefault();
    if (!range && focus.offset === 0) {
      const listKind = doc.listStyleAt(focus.node);
      if (listKind) {
        const level = doc.listLevelAt?.(focus.node) ?? 0;
        await runToolbarEdit((a, b, c, d) =>
          level > 0
            ? doc.adjustListLevel(a, b, c, d, -1)
            : doc.toggleList(a, b, c, d, listKind),
        );
        return;
      }
    }
    if (reviewMode === "suggesting") {
      const start = range ? (anchor.offset <= focus.offset ? anchor : focus) : (() => { const c = doc.moveCaret(focus.node, focus.offset, "left"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })();
      const end = range ? (anchor.offset <= focus.offset ? focus : anchor) : focus;
      if (start.node === end.node && start.offset < end.offset) {
        await runEdit(() => doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
        return;
      }
      setStatus("This deletion crosses a paragraph and cannot be tracked yet", "error");
      return;
    }
    await runEdit(() => range ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset) : doc.deleteBackward(focus.node, focus.offset));
    return;
  }
  if (key === "Delete") {
    e.preventDefault();
    if (reviewMode === "suggesting") {
      const start = range ? (anchor.offset <= focus.offset ? anchor : focus) : focus;
      const end = range ? (anchor.offset <= focus.offset ? focus : anchor) : (() => { const c = doc.moveCaret(focus.node, focus.offset, "right"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })();
      if (start.node === end.node && start.offset < end.offset) {
        await runEdit(() => doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
        return;
      }
      setStatus("This deletion crosses a paragraph and cannot be tracked yet", "error");
      return;
    }
    await runEdit(() => range ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset) : doc.deleteForward(focus.node, focus.offset));
    return;
  }
  if (key === "Enter") {
    e.preventDefault();
    if (reviewMode === "suggesting") {
      setStatus("Paragraph breaks cannot be tracked yet; switch to Editing to insert one", "error");
      return;
    }
    // Word/Docs convention: Enter on an empty list item exits the list instead
    // of creating another empty bullet/number. The current paragraph remains in
    // place, so the caret does not jump and Undo restores the list marker.
    if (!range) {
      const listKind = doc.listStyleAt(focus.node);
      if (listKind && doc.paragraphLength(focus.node) === 0) {
        const level = doc.listLevelAt?.(focus.node) ?? 0;
        await runToolbarEdit((a, b, c, d) =>
          level > 0
            ? doc.adjustListLevel(a, b, c, d, -1)
            : doc.toggleList(a, b, c, d, listKind),
        );
        return;
      }
    }
    await runEdit(() =>
      doc.insertPlainTextAs(
        anchor.node,
        anchor.offset,
        focus.node,
        focus.offset,
        "\n",
        "paragraphBreak",
      ),
    );
    return;
  }
  // A printable character (single key, no modifiers).
  if (key.length === 1) {
    e.preventDefault();
    const session = typingSessionForKey();
    if (range) {
      pendingFormat = null; // typing over a selection uses the selection's own runs
      if (reviewMode === "suggesting" && anchor.node !== focus.node) {
        setStatus("Cross-paragraph replacement cannot be tracked yet; switch to Editing", "error");
        return;
      }
      await runEdit(
        () => reviewMode === "suggesting"
          ? doc.suggestReplace(
            anchor.node,
            Math.min(anchor.offset, focus.offset),
            Math.max(anchor.offset, focus.offset),
            key,
            undefined,
            new Date().toISOString(),
            session,
          )
          : doc.typeText(anchor.node, anchor.offset, focus.node, focus.offset, key, session),
        { typing: true },
      );
    } else if (pendingFormat) {
      const pf = pendingFormat; // armed format persists across consecutive typing
      await runEdit(
        () => reviewMode === "suggesting"
          ? doc.suggestStyledInsert(
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
            undefined,
            new Date().toISOString(),
            session,
          )
          : doc.typeStyledText(
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
              session,
            ),
        { typing: true },
      );
    } else {
      await runEdit(
        () => reviewMode === "suggesting"
          ? doc.suggestInsert(
            focus.node,
            focus.offset,
            key,
            undefined,
            new Date().toISOString(),
            session,
          )
          : doc.typeText(focus.node, focus.offset, focus.node, focus.offset, key, session),
        { typing: true },
      );
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

// ---- Settings: theme + accent + reviewer identity, persisted (OSS-customizable) ----
const settingsBtn = document.getElementById("settingsBtn");
const settingsPanel = document.getElementById("settingsPanel");
const themeSeg = document.getElementById("themeSeg");
const accentSwatches = document.getElementById("accentSwatches");
const accentCustom = document.getElementById("accentCustom");
const settingsReset = document.getElementById("settingsReset");
const authorNameInput = document.getElementById("authorName");
const authorInitialsInput = document.getElementById("authorInitials");

const DEFAULT_SETTINGS = { theme: "system", accent: "#e2622a", authorName: "", authorInitials: "" };
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

/**
 * Pushes the host's reviewer identity into the open document through the
 * explicit `setActiveAuthor` seam (see docs/68 "Host identity seam" and
 * docs/81 REVIEW-GAP-013) — this is the one place identity crosses from the
 * host UI into the engine. A blank name still resolves to "You" so
 * `suggestInsert`/`suggestDelete`/etc. (which require a non-empty author)
 * keep working out of the box; a blank initials field lets the engine derive
 * initials from the name instead of duplicating that logic here.
 */
function applyActiveAuthorToDocument() {
  if (!doc) return;
  const name = settings.authorName.trim() || "You";
  const initials = settings.authorInitials.trim() || undefined;
  doc.setActiveAuthor(name, initials, undefined);
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
  authorNameInput.value = settings.authorName;
  authorInitialsInput.value = settings.authorInitials;
  applyActiveAuthorToDocument();
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
authorNameInput.addEventListener("input", () => {
  settings.authorName = authorNameInput.value;
  saveSettings();
  applyActiveAuthorToDocument();
});
authorInitialsInput.addEventListener("input", () => {
  settings.authorInitials = authorInitialsInput.value;
  saveSettings();
  applyActiveAuthorToDocument();
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

// ---- Document properties (docProps/core.xml — title, author, subject, …) ----
const propertiesBtn = document.getElementById("propertiesBtn");
const propertiesPanel = document.getElementById("propertiesPanel");
const propTitle = document.getElementById("propTitle");
const propCreator = document.getElementById("propCreator");
const propSubject = document.getElementById("propSubject");
const propCategory = document.getElementById("propCategory");
const propKeywords = document.getElementById("propKeywords");
const propDescription = document.getElementById("propDescription");
const propertiesApplyBtn = document.getElementById("propertiesApply");
const propertiesCancelBtn = document.getElementById("propertiesCancel");
const propertiesCloseBtn = document.getElementById("propertiesClose");
const metaCreated = document.getElementById("metaCreated");
const metaModified = document.getElementById("metaModified");
const metaLastModifiedBy = document.getElementById("metaLastModifiedBy");
const metaLastPrinted = document.getElementById("metaLastPrinted");
const metaRevision = document.getElementById("metaRevision");
const metaLanguage = document.getElementById("metaLanguage");
const metaContentStatus = document.getElementById("metaContentStatus");
const metaVersion = document.getElementById("metaVersion");
const metaApplication = document.getElementById("metaApplication");
const metaAppVersion = document.getElementById("metaAppVersion");
const metaTemplate = document.getElementById("metaTemplate");
const metaCompany = document.getElementById("metaCompany");
const metaManager = document.getElementById("metaManager");
const metaTotalTime = document.getElementById("metaTotalTime");
const metaSavedStats = document.getElementById("metaSavedStats");
const metaCustomSection = document.getElementById("metaCustomSection");
const metaCustomList = document.getElementById("metaCustomList");

const PROP_FIELDS = [
  ["title", propTitle],
  ["creator", propCreator],
  ["subject", propSubject],
  ["category", propCategory],
  ["keywords", propKeywords],
  ["description", propDescription],
];

function displayMetadataValue(element, value, formatter = String) {
  const hasValue = value !== null && value !== undefined && value !== "";
  element.textContent = hasValue ? formatter(value) : "Not set";
  element.classList.toggle("metadata-empty", !hasValue);
  if (hasValue) element.title = String(value);
  else element.removeAttribute("title");
}

function formatMetadataDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function customMetadataValue(value) {
  if (!value || typeof value !== "object") return "";
  if (value.type === "bool") return value.value ? "True" : "False";
  return value.value ?? "";
}

function reflectDocumentMetadata() {
  const metadata = JSON.parse(doc.documentMetadata());
  const core = metadata.core ?? {};
  const app = metadata.app ?? {};

  displayMetadataValue(metaCreated, core.created, formatMetadataDate);
  displayMetadataValue(metaModified, core.modified, formatMetadataDate);
  displayMetadataValue(metaLastModifiedBy, core.lastModifiedBy);
  displayMetadataValue(metaLastPrinted, core.lastPrinted, formatMetadataDate);
  displayMetadataValue(metaRevision, core.revision);
  displayMetadataValue(metaLanguage, core.language);
  displayMetadataValue(metaContentStatus, core.contentStatus);
  displayMetadataValue(metaVersion, core.version);

  displayMetadataValue(metaApplication, app.application);
  displayMetadataValue(metaAppVersion, app.appVersion);
  displayMetadataValue(metaTemplate, app.template);
  displayMetadataValue(metaCompany, app.company);
  displayMetadataValue(metaManager, app.manager);
  displayMetadataValue(
    metaTotalTime,
    app.totalTime,
    (minutes) => `${Number(minutes).toLocaleString()} min`,
  );

  const savedCounts = [
    ["pages", app.pages],
    ["words", app.words],
    ["characters", app.characters],
    ["paragraphs", app.paragraphs],
  ]
    .filter(([, value]) => value !== null && value !== undefined)
    .map(([label, value]) => `${Number(value).toLocaleString()} ${label}`)
    .join(" · ");
  displayMetadataValue(metaSavedStats, savedCounts);

  metaCustomList.replaceChildren();
  const custom = Array.isArray(metadata.custom) ? metadata.custom : [];
  for (const property of custom) {
    const row = document.createElement("div");
    const name = document.createElement("dt");
    const value = document.createElement("dd");
    name.textContent = property.name;
    value.textContent = customMetadataValue(property.value) || "Not set";
    row.append(name, value);
    metaCustomList.append(row);
  }
  metaCustomSection.hidden = custom.length === 0;
}

function syncModalLock() {
  const modalOpen =
    !propertiesPanel.hidden ||
    (typeof pageSetupMenu !== "undefined" && !pageSetupMenu.hidden);
  document.body.classList.toggle("modal-open", modalOpen);
}

function trapModalFocus(e, modal) {
  if (e.key !== "Tab") return;
  const focusable = [
    ...modal.querySelectorAll(
      'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])',
    ),
  ].filter((element) => element.getClientRects().length > 0);
  if (focusable.length === 0) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}

function toggleProperties(open) {
  const show = open ?? propertiesPanel.hidden;
  if (show && doc) {
    const current = JSON.parse(doc.documentProperties());
    for (const [key, input] of PROP_FIELDS) input.value = current[key] ?? "";
    reflectDocumentMetadata();
  }
  const returnFocus = !show && propertiesPanel.contains(document.activeElement);
  propertiesPanel.hidden = !show;
  propertiesBtn.setAttribute("aria-expanded", String(show));
  syncModalLock();
  if (show) {
    queueMicrotask(() => propTitle.focus());
  } else if (returnFocus) {
    propertiesBtn.focus({ preventScroll: true });
  }
}
propertiesBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  toggleProperties();
});
propertiesCancelBtn.addEventListener("click", () => toggleProperties(false));
propertiesCloseBtn.addEventListener("click", () => toggleProperties(false));
propertiesApplyBtn.addEventListener("click", async () => {
  if (!doc) return;
  const current = JSON.parse(doc.documentProperties());
  for (const [key, input] of PROP_FIELDS) {
    const value = input.value.trim();
    current[key] = value ? value : null;
  }
  await runEdit(() => doc.setDocumentProperties(JSON.stringify(current)), { gate: true });
  toggleProperties(false);
});
propertiesPanel.addEventListener("mousedown", (e) => {
  if (e.target === propertiesPanel) toggleProperties(false);
});
document.addEventListener("keydown", (e) => {
  if (!propertiesPanel.hidden) {
    if (e.key === "Escape") {
      e.preventDefault();
      toggleProperties(false);
    } else {
      trapModalFocus(e, propertiesPanel);
    }
  }
});

// ---- Page setup (page size, margins, orientation) ----------------------------
const pageSetupBtn = document.getElementById("pageSetupBtn");
const pageSetupMenu = document.getElementById("pageSetupMenu");
const pageOrientationSeg = document.getElementById("pageOrientationSeg");
const pageWidthInput = document.getElementById("pageWidth");
const pageHeightInput = document.getElementById("pageHeight");
const pageMarginTopInput = document.getElementById("pageMarginTop");
const pageMarginBottomInput = document.getElementById("pageMarginBottom");
const pageMarginLeftInput = document.getElementById("pageMarginLeft");
const pageMarginRightInput = document.getElementById("pageMarginRight");
const pageSetupApplyBtn = document.getElementById("pageSetupApply");
const pageSetupCancelBtn = document.getElementById("pageSetupCancel");
const pageSetupCloseBtn = document.getElementById("pageSetupClose");
const pagePreviewSheet = document.getElementById("pagePreviewSheet");
const pagePreviewMargins = document.getElementById("pagePreviewMargins");
const pagePreviewLabel = document.getElementById("pagePreviewLabel");
const pageSetupSection = document.getElementById("pageSetupSection");
const pageColumnCount = document.getElementById("pageColumnCount");
const pageColumnGap = document.getElementById("pageColumnGap");
const pageColumnSeparator = document.getElementById("pageColumnSeparator");

let pageSetupCurrent = null; // the last-fetched {section, pageSize, pageMargins, orientation}

function reflectPageSetupColumns(columns) {
  const value = columns ?? { count: 1, spaceTwips: 0, separator: false };
  pageColumnCount.value = String(Math.min(4, Math.max(1, value.count ?? 1)));
  pageColumnGap.value = pageInchStr(value.spaceTwips ?? 0);
  pageColumnSeparator.checked = value.separator === true;
}

function pageSetupColumnsPayload() {
  const current = pageSetupCurrent.columns;
  const count = Number(pageColumnCount.value) || 1;
  const spaceTwips = inchTwips(pageColumnGap);
  const separator = pageColumnSeparator.checked;
  // Opening Page Setup and changing only page size/margins must not erase
  // explicit unequal column widths. Normalize to equal columns only when a
  // column control itself actually changed.
  if (
    current &&
    count === current.count &&
    spaceTwips === (current.spaceTwips ?? 0) &&
    separator === (current.separator === true)
  ) {
    return current;
  }
  return {
    ...(current ?? {}),
    count,
    spaceTwips,
    separator,
    equalWidth: true,
    columns: [],
  };
}

/** Twips → inches string for a page-geometry field (unlike inchStr, 0 shows
 * as "0" — a page dimension/margin is never meaningfully "unset"). */
function pageInchStr(twip) {
  return (twip / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "") || "0";
}

function updatePageSetupPreview() {
  const width = Math.max(1, Number(pageWidthInput.value) || 1);
  const height = Math.max(1, Number(pageHeightInput.value) || 1);
  const top = Math.max(0, Number(pageMarginTopInput.value) || 0);
  const bottom = Math.max(0, Number(pageMarginBottomInput.value) || 0);
  const left = Math.max(0, Number(pageMarginLeftInput.value) || 0);
  const right = Math.max(0, Number(pageMarginRightInput.value) || 0);
  const previewPercent = (value, dimension) =>
    `${Math.min(38, Math.max(3, (value / dimension) * 100))}%`;

  pagePreviewSheet.dataset.orientation = width > height ? "landscape" : "portrait";
  pagePreviewSheet.style.setProperty("--page-ratio", `${width} / ${height}`);
  pagePreviewMargins.style.setProperty("--preview-margin-top", previewPercent(top, height));
  pagePreviewMargins.style.setProperty("--preview-margin-bottom", previewPercent(bottom, height));
  pagePreviewMargins.style.setProperty("--preview-margin-left", previewPercent(left, width));
  pagePreviewMargins.style.setProperty("--preview-margin-right", previewPercent(right, width));
  pagePreviewLabel.textContent = `${pageInchStr(width * TWIPS_PER_INCH)} × ${pageInchStr(height * TWIPS_PER_INCH)} in`;
}

function reflectPageSetup() {
  if (!doc) return false;
  const raw = doc.pageSetupSections(selection?.focus?.node ?? "");
  const list = raw === "null" ? null : JSON.parse(raw);
  if (!list?.sections?.length) return false;
  pageSetupSection.replaceChildren();
  for (const [index, section] of list.sections.entries()) {
    const option = document.createElement("option");
    option.value = section.section;
    option.textContent = `Section ${index + 1}`;
    pageSetupSection.appendChild(option);
  }
  pageSetupSection.value = list.current;
  pageSetupCurrent = list.sections.find((section) => section.section === list.current) ?? list.sections[0];
  const { pageSize, pageMargins, orientation } = pageSetupCurrent;
  pageWidthInput.value = pageInchStr(pageSize.widthTwips);
  pageHeightInput.value = pageInchStr(pageSize.heightTwips);
  pageMarginTopInput.value = pageInchStr(pageMargins.topTwips);
  pageMarginBottomInput.value = pageInchStr(pageMargins.bottomTwips);
  pageMarginLeftInput.value = pageInchStr(pageMargins.startTwips);
  pageMarginRightInput.value = pageInchStr(pageMargins.endTwips);
  reflectPageSetupColumns(pageSetupCurrent.columns);
  const activeOrientation =
    orientation ?? (pageSize.widthTwips > pageSize.heightTwips ? "landscape" : "portrait");
  for (const btn of pageOrientationSeg.querySelectorAll("button")) {
    btn.setAttribute("aria-pressed", String(btn.dataset.orientation === activeOrientation));
  }
  updatePageSetupPreview();
  return true;
}

pageSetupSection.addEventListener("change", () => {
  if (!doc) return;
  const raw = doc.pageSetupSections(selection?.focus?.node ?? "");
  const list = raw === "null" ? null : JSON.parse(raw);
  pageSetupCurrent = list?.sections?.find((section) => section.section === pageSetupSection.value) ?? null;
  if (!pageSetupCurrent) return;
  const { pageSize, pageMargins, orientation } = pageSetupCurrent;
  pageWidthInput.value = pageInchStr(pageSize.widthTwips);
  pageHeightInput.value = pageInchStr(pageSize.heightTwips);
  pageMarginTopInput.value = pageInchStr(pageMargins.topTwips);
  pageMarginBottomInput.value = pageInchStr(pageMargins.bottomTwips);
  pageMarginLeftInput.value = pageInchStr(pageMargins.startTwips);
  pageMarginRightInput.value = pageInchStr(pageMargins.endTwips);
  reflectPageSetupColumns(pageSetupCurrent.columns);
  const activeOrientation = orientation ?? (pageSize.widthTwips > pageSize.heightTwips ? "landscape" : "portrait");
  for (const btn of pageOrientationSeg.querySelectorAll("button")) {
    btn.setAttribute("aria-pressed", String(btn.dataset.orientation === activeOrientation));
  }
  updatePageSetupPreview();
});

function togglePageSetup(open) {
  const show = open ?? pageSetupMenu.hidden;
  if (show && !reflectPageSetup()) return; // no section geometry to edit
  const returnFocus = !show && pageSetupMenu.contains(document.activeElement);
  pageSetupMenu.hidden = !show;
  pageSetupBtn.setAttribute("aria-expanded", String(show));
  syncModalLock();
  if (show) {
    queueMicrotask(() =>
      pageOrientationSeg.querySelector('button[aria-pressed="true"]')?.focus(),
    );
  } else if (returnFocus) {
    pageSetupBtn.focus({ preventScroll: true });
  }
}
pageSetupBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  togglePageSetup();
});
pageOrientationSeg.addEventListener("click", (e) => {
  const btn = e.target.closest("button[data-orientation]");
  if (!btn) return;
  for (const b of pageOrientationSeg.querySelectorAll("button")) {
    b.setAttribute("aria-pressed", String(b === btn));
  }
  // Swap width/height to match, mirroring Word's orientation toggle.
  const w = Number(pageWidthInput.value) || 0;
  const h = Number(pageHeightInput.value) || 0;
  const wantLandscape = btn.dataset.orientation === "landscape";
  if (wantLandscape === w > h) return; // already matches
  const widthTwips = inchTwips(pageWidthInput);
  const heightTwips = inchTwips(pageHeightInput);
  pageWidthInput.value = pageInchStr(heightTwips);
  pageHeightInput.value = pageInchStr(widthTwips);
  updatePageSetupPreview();
});
pageSetupCancelBtn.addEventListener("click", () => togglePageSetup(false));
pageSetupCloseBtn.addEventListener("click", () => togglePageSetup(false));
for (const input of [
  pageWidthInput,
  pageHeightInput,
  pageMarginTopInput,
  pageMarginBottomInput,
  pageMarginLeftInput,
  pageMarginRightInput,
]) {
  input.addEventListener("input", updatePageSetupPreview);
}
pageSetupApplyBtn.addEventListener("click", async () => {
  if (!doc || !pageSetupCurrent) return;
  const orientation =
    pageOrientationSeg.querySelector('button[aria-pressed="true"]')?.dataset.orientation ??
    "portrait";
  const payload = {
    section: pageSetupCurrent.section,
    pageSize: {
      widthTwips: inchTwips(pageWidthInput),
      heightTwips: inchTwips(pageHeightInput),
    },
    pageMargins: {
      ...pageSetupCurrent.pageMargins,
      topTwips: inchTwips(pageMarginTopInput),
      bottomTwips: inchTwips(pageMarginBottomInput),
      startTwips: inchTwips(pageMarginLeftInput),
      endTwips: inchTwips(pageMarginRightInput),
    },
    columns: pageSetupColumnsPayload(),
    orientation,
  };
  await runEdit(() => doc.setPageSetup(JSON.stringify(payload)), { gate: true });
  togglePageSetup(false);
});
pageSetupMenu.addEventListener("mousedown", (e) => {
  if (e.target === pageSetupMenu) togglePageSetup(false);
});
document.addEventListener("keydown", (e) => {
  if (!pageSetupMenu.hidden) {
    if (e.key === "Escape") {
      e.preventDefault();
      togglePageSetup(false);
    } else {
      trapModalFocus(e, pageSetupMenu);
    }
  }
});

fileEl.disabled = true;
updateToolbar(); // start with the toolbar controls disabled (no selection yet)
boot();
