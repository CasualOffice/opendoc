// The font family menu (searchable, each name in its own face, recently used),
// grow and shrink font, change case, and the list marker galleries. Moved out
// of `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { escapeHtml, recaseRichRuns } from "./text_rules.mjs";
import { nextFontSizeStep } from "./style_picker.mjs";
import { onButton, closePopover, registerPopover } from "./popover_manager.mjs";
import { makeMenuHeading } from "./picker_chrome.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createFontMenu(ed) {
  // ---- Font family menu (Q2): searchable, own-typeface, recently-used group ----
  // No font-enumeration API is exposed to the webapp, so this is a curated common
  // list (the registry seam populates faces for rendering; this list is the menu
  // inventory). The caret's actual family is always included so an imported font
  // stays selectable/visible even when it is not in the list.
  const COMMON_FONTS = [
    "Arial", "Calibri", "Cambria", "Comic Sans MS", "Consolas", "Courier New",
    "Georgia", "Helvetica", "Lato", "Montserrat", "Noto Sans", "Noto Serif",
    "Open Sans", "Roboto", "Segoe UI", "Tahoma", "Times New Roman",
    "Trebuchet MS", "Verdana",
  ];
  const recentFonts = [];
  let fontMenuActiveIndex = -1;

  function fontInventory() {
    const all = new Set(COMMON_FONTS);
    if (ed.currentFontFamily) all.add(ed.currentFontFamily);
    return [...all].sort((a, b) => a.localeCompare(b));
  }

  /** (Re)renders the font list filtered by the search box; recently-used first,
   *  then the alphabetical inventory, each name shown in its own typeface. */
  function renderFontMenu() {
    const query = ed.fontMenuInput.value.trim().toLowerCase();
    const match = (name) => name.toLowerCase().includes(query);
    const recent = recentFonts.filter(match);
    const inventory = fontInventory().filter((name) => match(name) && !recent.includes(name));
    ed.fontMenuList.replaceChildren();

    const addRow = (name, group) => {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "font-menu-item";
      row.setAttribute("role", "option");
      row.dataset.font = name;
      row.style.fontFamily = `"${name}", system-ui, sans-serif`;
      row.setAttribute("aria-selected", String(name === ed.currentFontFamily));
      if (name === ed.currentFontFamily) row.classList.add("is-current");
      row.innerHTML = `<span class="font-menu-check ms" aria-hidden="true">check</span><span class="font-menu-name">${escapeHtml(name)}</span>`;
      ed.fontMenuList.appendChild(row);
      if (group) row.dataset.group = group;
    };

    if (recent.length) {
      const h = makeMenuHeading("Recently used");
      ed.fontMenuList.appendChild(h);
      for (const name of recent) addRow(name, "recent");
      ed.fontMenuList.appendChild(makeMenuHeading("All fonts"));
    }
    for (const name of inventory) addRow(name);

    const rows = ed.fontMenuList.querySelectorAll(".font-menu-item");
    ed.fontMenuEmpty.hidden = rows.length > 0;
    // Default the active row to the current font (or the first row).
    fontMenuActiveIndex = [...rows].findIndex((r) => r.dataset.font === ed.currentFontFamily);
    if (fontMenuActiveIndex < 0 && rows.length) fontMenuActiveIndex = 0;
    paintFontActive();
  }
  function paintFontActive() {
    const rows = ed.fontMenuList.querySelectorAll(".font-menu-item");
    rows.forEach((row, i) => row.classList.toggle("is-active", i === fontMenuActiveIndex));
    rows[fontMenuActiveIndex]?.scrollIntoView({ block: "nearest" });
  }
  function chooseFont(name) {
    ed.applyFontFamily(name);
    ed.reflectFontFamily(name);
    ed.recordRecent(recentFonts, name);
    closePopover(fontPopover);
    ed.focusEditorSurface();
  }

  const fontPopover = registerPopover(ed.fontFamilyBtn, ed.fontMenu, () => {
    ed.fontMenuInput.value = "";
    renderFontMenu();
    requestAnimationFrame(() => ed.fontMenuInput.focus());
  });
  ed.fontMenuInput.addEventListener("input", renderFontMenu);
  ed.fontMenuInput.addEventListener("keydown", (e) => {
    const rows = ed.fontMenuList.querySelectorAll(".font-menu-item");
    if (e.key === "ArrowDown") {
      e.preventDefault();
      fontMenuActiveIndex = Math.min(rows.length - 1, fontMenuActiveIndex + 1);
      paintFontActive();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      fontMenuActiveIndex = Math.max(0, fontMenuActiveIndex - 1);
      paintFontActive();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const row = rows[fontMenuActiveIndex];
      if (row) chooseFont(row.dataset.font);
    }
  });
  ed.fontMenuList.addEventListener("click", (e) => {
    const row = e.target.closest(".font-menu-item");
    if (row) chooseFont(row.dataset.font);
  });

  // ---- Grow / shrink font (Q5) -------------------------------------------------
  // The ladder and the step rule are `nextFontSizeStep` in `style_picker.mjs`,
  // beside `previewPx`: both are point sizes, and the epsilon and the past-the-end
  // behaviour are exactly the parts that want a unit test rather than a browser.
  function currentFontPt() {
    const v = Number(ed.fontSizeSel.value);
    if (Number.isFinite(v) && v >= 1) return v;
    if (ed.pendingFormat?.sizeHalfPoints != null) return ed.pendingFormat.sizeHalfPoints / 2;
    return 11;
  }
  function stepFontSize(dir) {
    const next = nextFontSizeStep(currentFontPt(), dir);
    ed.armOrApplyRun({ sizeHalfPoints: Math.round(next * 2) }, () =>
      ed.runToolbarEdit((a, b, c, d) => ed.doc.setFontSize(a, b, c, d, next)),
    );
  }
  onButton(ed.growFontBtn, () => stepFontSize(1));
  onButton(ed.shrinkFontBtn, () => stepFontSize(-1));

  // ---- Change case (Q5): transform selected text, preserving per-run format ----
  // The cross-run re-slicing rule is `recaseRichRuns` in `text_rules.mjs`, with
  // the two formatting boundaries that make it hard; what is left here is reading
  // the runs out of the engine and writing them back.
  async function applyChangeCase(mode) {
    if (!ed.doc || !ed.hasRange()) return;
    const { anchor, focus } = ed.selection;
    let runs;
    try {
      runs = JSON.parse(ed.doc.copyRichRuns(anchor.node, anchor.offset, focus.node, focus.offset));
    } catch {
      return;
    }
    if (!Array.isArray(runs) || !runs.length) return;
    await ed.pasteRichRunsJson(JSON.stringify(recaseRichRuns(runs, mode)));
  }
  const changeCasePopover = registerPopover(ed.changeCaseBtn, ed.changeCaseMenu, () => {});
  ed.changeCaseMenu.addEventListener("click", (e) => {
    const item = e.target.closest("[data-case]");
    if (!item) return;
    closePopover(changeCasePopover);
    void applyChangeCase(item.dataset.case);
  });

  // -- List marker-format galleries (bullet glyph / number format) ---------------
  // The bullet and numbered buttons carry a ▾ split that opens a small gallery of
  // marker choices; picking one retargets the caret's list through the gated
  // `setListFormat` path (one undo; blocked in Viewing/Suggesting like the sibling
  // restart/continue list ops). The main button keeps its plain on/off toggle.
  /** Mark the gallery cell matching the caret's current list marker as checked. */
  function reflectListGallery(menu) {
    const current = ed.doc && ed.selection ? ed.doc.listFormatAt(ed.selection.focus.node) : "";
    for (const cell of menu.querySelectorAll(".list-gallery-cell")) {
      cell.setAttribute("aria-checked", String(cell.dataset.spec === current));
    }
  }
  return { applyChangeCase, fontInventory, reflectListGallery, stepFontSize };
}
