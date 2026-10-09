// The chart panel: the data behind a chart, its type, its title and its legend.
//
// `insertChart` put Word's sample data on the page and offered no way to change
// any of it, so a chart was a picture of three made-up series. The engine half
// (`casual-doc-wasm/src/chart.rs`) is one read, `chartData`, and one write,
// `setChartData`; this module is the surface that reaches them.
//
// ## Designed from the competitive standard first
//
// | Word / Google Docs | Here |
// | --- | --- |
// | Insert ▸ Chart opens the data sheet straight away (Word); Docs' chart editor is a right-hand panel with Setup and Customize | Insert ▸ Chart selects the new chart and opens this right-hand panel on its grid |
// | Data is a grid: row 1 series names, column A category names, the body numbers | the same grid, with the same orientation |
// | Edits apply to the chart as they are made | each cell applies when it is committed (Enter, Tab, leaving it), one undo step per cell |
// | Paste a block from Excel / Sheets into the sheet | a tab-separated paste fills from the cell it lands in and grows the grid to fit |
// | Change chart type is a gallery | seven families as a radio group, enumerated by the ENGINE so the list cannot drift |
// | Chart title and legend position on Customize / Chart Elements | a title field and a legend menu |
// | Double-click a chart, or right-click ▸ Edit Data | double-click, and "Edit data" on the chart's chip |
//
// Where the engine refuses (a chart using constructs this build does not
// model, or a combination chart) the grid is still SHOWN, read-only, with the
// reason above it — a control that vanishes cannot be told from a bug. A chart
// imported from a file IS editable; when it names an embedded workbook the
// panel says, before the first change, that saving replaces it.
//
// ## Cost
//
// Every read and write is O(series × rows) in the ONE chart named; nothing here
// walks the document (`docs/107` §4). `sync` runs on repaint and costs one
// `chartData` read and a string compare while the panel is open, nothing while
// it is closed.
import { editRefusalMessage } from "./edit_errors.mjs";

/** The icon each gallery family is drawn with, from the self-hosted Material
 *  Symbols font. A family the engine adds later falls back to `insert_chart`
 *  rather than to nothing. */
const KIND_ICONS = Object.freeze({
  column: "bar_chart",
  bar: "align_horizontal_left",
  line: "show_chart",
  area: "area_chart",
  pie: "pie_chart",
  doughnut: "donut_large",
  scatter: "scatter_plot",
});

/** Each family's name in the catalogue. Spelled out, never built from the
 *  token, so the locale extractor can see every key a translator must answer. */
const KIND_LABEL_KEYS = Object.freeze({
  column: "chart.kind.column",
  bar: "chart.kind.bar",
  line: "chart.kind.line",
  area: "chart.kind.area",
  pie: "chart.kind.pie",
  doughnut: "chart.kind.doughnut",
  scatter: "chart.kind.scatter",
});

/** Word's legend positions, in the order its Chart Elements flyout lists them,
 *  each with its catalogue key. */
const LEGEND_LABEL_KEYS = Object.freeze({
  none: "chart.legend.none",
  right: "chart.legend.right",
  top: "chart.legend.top",
  left: "chart.legend.left",
  bottom: "chart.legend.bottom",
  topRight: "chart.legend.topRight",
});
export const LEGEND_CHOICES = Object.freeze(Object.keys(LEGEND_LABEL_KEYS));

/** A number as the engine accepts one: Rust's `f64` grammar, finite. Checked
 *  here only so the refusal can name the cell in the reader's language before
 *  the round trip; the engine stays the authority and checks again.
 *
 *  O(text length). */
const NUMBER = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/;
export function isChartNumber(text) {
  const trimmed = String(text ?? "").trim();
  return trimmed === "" || (NUMBER.test(trimmed) && Number.isFinite(Number(trimmed)));
}

/** A pasted cell as the number it means, when the spreadsheet formatted it.
 *
 *  Excel and Sheets copy a cell's DISPLAYED text, so a value shown as `1,234`
 *  arrives with its grouping commas. Only the unambiguous English grouping shape
 *  is undone — `1,234` and `-12,345.6` — because `1,5` is a decimal in half the
 *  world and guessing would silently change a number. Anything else is kept as
 *  typed and the engine says whether it is a number.
 *
 *  O(text length). */
export function normalisePastedNumber(text) {
  const trimmed = String(text ?? "").trim();
  return /^[+-]?\d{1,3}(,\d{3})+(\.\d+)?$/.test(trimmed) ? trimmed.replaceAll(",", "") : trimmed;
}

/** Splits clipboard text into rows of cells.
 *
 *  Spreadsheets put tab-separated rows on the clipboard, one per line, with a
 *  trailing newline; that trailing empty line is not a row. A single value with
 *  no tab and no newline is one cell, which is what a plain paste into one cell
 *  is.
 *
 *  O(text length). */
export function parseClipboardGrid(text) {
  const lines = String(text ?? "").replace(/\r\n?/g, "\n").split("\n");
  while (lines.length > 1 && lines.at(-1) === "") lines.pop();
  return lines.map((line) => line.split("\t"));
}

/** A copy of `patch` with nothing shared, so an edit to the draft can never
 *  reach the view it was read from. O(cells). */
function clonePatch(patch) {
  return {
    kind: patch.kind,
    title: patch.title,
    legend: patch.legend,
    series: [...patch.series],
    labels: [...patch.labels],
    cells: patch.cells.map((row) => [...row]),
  };
}

/** The write payload a view describes — the six fields `setChartData` reads. */
export function patchFromView(view) {
  return clonePatch({
    kind: view.kind,
    title: view.title,
    legend: view.legend,
    series: view.series,
    labels: view.labels,
    cells: view.cells,
  });
}

/** Adds a row after `at` (or at the end). The new row's cells are blank — a
 *  value the model has — never zero, which would plot a point nobody typed. */
export function insertRow(patch, at = patch.labels.length - 1, label = "") {
  const next = clonePatch(patch);
  const index = Math.min(Math.max(at + 1, 0), next.labels.length);
  next.labels.splice(index, 0, label);
  next.cells.splice(index, 0, next.series.map(() => ""));
  return next;
}

/** Removes row `at`. The last row is never removed: a chart with no data is
 *  refused by the engine, and the button for it is disabled with that reason. */
export function removeRow(patch, at) {
  if (patch.labels.length <= 1) return clonePatch(patch);
  const next = clonePatch(patch);
  next.labels.splice(at, 1);
  next.cells.splice(at, 1);
  return next;
}

/** Adds a series (a column) after `at` (or at the end). */
export function insertSeries(patch, at = patch.series.length - 1, name = "") {
  const next = clonePatch(patch);
  const index = Math.min(Math.max(at + 1, 0), next.series.length);
  next.series.splice(index, 0, name);
  for (const row of next.cells) row.splice(index, 0, "");
  return next;
}

/** Removes series `at`; never the last one, for the same reason as rows. */
export function removeSeries(patch, at) {
  if (patch.series.length <= 1) return clonePatch(patch);
  const next = clonePatch(patch);
  next.series.splice(at, 1);
  for (const row of next.cells) row.splice(at, 1);
  return next;
}

/** Writes a pasted block into `patch` with its top-left at (`row`, `column`).
 *
 *  Grid coordinates: row -1 is the series-name header, column -1 is the label
 *  column, so a block copied WITH its headers from a spreadsheet and pasted on
 *  the corner lands exactly where the reader expects. The grid grows to fit,
 *  up to the engine's ceilings — what does not fit is reported as a count
 *  rather than dropped silently, and the new rows and series are named the way
 *  Word names them until the reader types better names.
 *
 *  O(pasted cells + grid cells). */
export function pasteBlock(patch, row, column, block, limits, names) {
  let next = clonePatch(patch);
  let clipped = 0;
  const lastRow = row + block.length - 1;
  const lastColumn = column + Math.max(0, ...block.map((cells) => cells.length)) - 1;
  while (next.labels.length - 1 < lastRow && next.labels.length < limits.maxRows) {
    next = insertRow(next, next.labels.length - 1, names.row(next.labels.length + 1));
  }
  while (next.series.length - 1 < lastColumn && next.series.length < limits.maxSeries) {
    next = insertSeries(next, next.series.length - 1, names.series(next.series.length + 1));
  }
  block.forEach((cells, dy) => {
    cells.forEach((raw, dx) => {
      const r = row + dy;
      const c = column + dx;
      if (r >= next.labels.length || c >= next.series.length) {
        clipped += 1;
        return;
      }
      if (r === -1 && c === -1) return; // the corner names nothing
      if (r === -1) next.series[c] = raw.trim();
      else if (c === -1) next.labels[r] = raw.trim();
      else next.cells[r][c] = normalisePastedNumber(raw);
    });
  });
  return { patch: next, clipped };
}

/** Builds the chart panel. Every dependency is injected (`io`), so the panel
 *  never reaches into `main.js` state and the whole edit path is testable.
 *
 *  @param {object} io
 *  @param {() => any} io.doc                  the live document handle, or null
 *  @param {() => {node: string, kind: string} | null} io.selection
 *  @param {() => boolean} io.blocked          true (after telling the reader) when no edit may apply now
 *  @param {(result: unknown) => Promise<unknown>} io.apply   lands an EditResult
 *  @param {(text: string, kind?: string) => void} io.setStatus
 *  @param {(key: string, params?: object) => string} io.t
 *  @param {() => HTMLElement} [io.host]       where the panel mounts
 */
export function createChartDataPanel(io) {
  const t = io.t;
  let el = null;
  let node = null;
  let view = null;
  let lastJson = "";
  let busy = false;
  const parts = {};

  /** The sentence for a coded engine refusal, in the reader's language when the
   *  code is one this panel can produce; the engine's own sentence otherwise. */
  const REFUSAL_KEYS = {
    "chart.partial-coverage": "chart.refused.partial",
    "chart.many-groups": "chart.refused.combo",
  };
  const routeRefusal = (code) => (REFUSAL_KEYS[code] ? t(REFUSAL_KEYS[code]) : "");

  function read(target) {
    const doc = io.doc();
    if (!doc || !target || typeof doc.chartData !== "function") return { json: "", view: null };
    const json = doc.chartData(target) || "";
    if (!json) return { json: "", view: null };
    try {
      return { json, view: JSON.parse(json) };
    } catch {
      return { json: "", view: null };
    }
  }

  function note(message, isError = false) {
    parts.note.textContent = message || "";
    parts.note.classList.toggle("error", !!isError && !!message);
  }

  const names = {
    row: (n) => (view?.firstColumn === "x" ? "" : t("chart.newCategory", { n })),
    series: (n) => t("chart.newSeries", { n }),
  };

  /** Everything the panel currently shows, as a write payload. Read from the
   *  DOM so a value typed and not yet committed is part of the next write — a
   *  structural edit made while a cell is mid-edit must not throw that cell
   *  away. O(cells). */
  function draft() {
    if (!view) return null;
    const patch = patchFromView(view);
    patch.title = parts.title.value;
    patch.legend = parts.legend.value;
    const checked = parts.gallery.querySelector('[aria-checked="true"]');
    if (checked) patch.kind = checked.dataset.kind;
    for (const input of parts.grid.querySelectorAll("input[data-row]")) {
      const r = Number(input.dataset.row);
      const c = Number(input.dataset.col);
      if (r === -1 && c >= 0) patch.series[c] = input.value;
      else if (c === -1 && r >= 0) patch.labels[r] = input.value;
      else if (r >= 0 && c >= 0) patch.cells[r][c] = input.value;
    }
    return patch;
  }

  /** The first cell whose text is not a number, as a localised sentence. */
  function invalidCell(patch) {
    const check = (text, r, series) =>
      isChartNumber(text) ? "" : t("chart.notANumber", { value: String(text).trim(), row: r + 1, series });
    for (let r = 0; r < patch.cells.length; r += 1) {
      for (let c = 0; c < patch.series.length; c += 1) {
        const sentence = check(patch.cells[r][c], r, patch.series[c] || t("chart.seriesFallback", { n: c + 1 }));
        if (sentence) return { sentence, r, c };
      }
    }
    if (view?.firstColumn === "x") {
      for (let r = 0; r < patch.labels.length; r += 1) {
        const sentence = check(patch.labels[r], r, t("chart.xValues"));
        if (sentence) return { sentence, r, c: -1 };
      }
    }
    return null;
  }

  /** Applies `patch` as one undoable edit, then redraws the panel from what the
   *  engine now holds — never from the patch, so the panel can only ever show
   *  data the document actually has. `focus` names the cell to land on after. */
  let queue = Promise.resolve(true);
  function commit(patch, focus = null) {
    // Serialised, never dropped: a cell committed on blur and the button click
    // that caused the blur arrive together, and each payload is the WHOLE grid,
    // so applying them in order is exactly right and skipping the second is not.
    queue = queue.then(() => commitNow(patch, focus), () => commitNow(patch, focus));
    return queue;
  }

  async function commitNow(patch, focus) {
    const doc = io.doc();
    if (!doc || !node || !view?.editable) return false;
    const bad = invalidCell(patch);
    if (bad) {
      note(bad.sentence, true);
      const cell = parts.grid.querySelector(`input[data-row="${bad.r}"][data-col="${bad.c}"]`);
      cell?.setAttribute("aria-invalid", "true");
      cell?.focus();
      return false;
    }
    if (io.blocked()) {
      render(read(node), focus);
      return false;
    }
    let result;
    try {
      result = doc.setChartData(node, JSON.stringify(patch));
    } catch (err) {
      // An unchanged grid is not a failure: a cell left without being retyped
      // commits the same chart, and the engine declines to push an undo step
      // that undoes nothing.
      if (err?.code === "chart.unchanged") return false;
      note(editRefusalMessage(err, { routeRefusal }), true);
      return false;
    }
    busy = true;
    try {
      await io.apply(result);
    } finally {
      busy = false;
    }
    note("");
    render(read(node), focus);
    return true;
  }

  function cellInput(r, c, value, label) {
    const input = document.createElement("input");
    input.type = "text";
    input.value = value;
    input.dataset.row = String(r);
    input.dataset.col = String(c);
    input.spellcheck = false;
    input.autocomplete = "off";
    input.setAttribute("aria-label", label);
    const numeric = (r >= 0 && c >= 0) || (c === -1 && r >= 0 && view.firstColumn === "x");
    if (numeric) {
      input.inputMode = "decimal";
      input.classList.add("chart-cell-number");
    } else if (view.titleLimit) {
      input.maxLength = view.titleLimit;
    }
    input.disabled = !view.editable;
    return input;
  }

  function iconButton(icon, label, onClick, disabledReason = "") {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "chart-grid-btn";
    const glyph = document.createElement("span");
    glyph.className = "ms";
    glyph.setAttribute("aria-hidden", "true");
    glyph.textContent = icon;
    btn.append(glyph);
    btn.setAttribute("aria-label", disabledReason ? `${label} — ${disabledReason}` : label);
    btn.title = disabledReason || label;
    btn.disabled = !view.editable || !!disabledReason;
    btn.addEventListener("click", onClick);
    return btn;
  }

  function renderGallery() {
    parts.gallery.replaceChildren();
    for (const kind of view.kinds) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "chart-kind";
      btn.dataset.kind = kind;
      btn.setAttribute("role", "radio");
      const on = kind === view.kind;
      btn.setAttribute("aria-checked", String(on));
      btn.tabIndex = on ? 0 : -1;
      btn.disabled = !view.editable;
      const glyph = document.createElement("span");
      glyph.className = "ms";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = KIND_ICONS[kind] ?? "insert_chart";
      const text = document.createElement("span");
      text.textContent = KIND_LABEL_KEYS[kind] ? t(KIND_LABEL_KEYS[kind]) : kind;
      btn.append(glyph, text);
      btn.addEventListener("click", () => chooseKind(kind));
      parts.gallery.append(btn);
    }
  }

  function chooseKind(kind) {
    for (const btn of parts.gallery.querySelectorAll(".chart-kind")) {
      const on = btn.dataset.kind === kind;
      btn.setAttribute("aria-checked", String(on));
      btn.tabIndex = on ? 0 : -1;
    }
    const patch = draft();
    if (patch) void commit({ ...patch, kind }, { kind });
  }

  function renderGrid() {
    const rows = view.labels.length;
    const columns = view.series.length;
    const table = document.createElement("table");
    table.className = "chart-grid";
    const head = document.createElement("thead");
    const headRow = document.createElement("tr");
    const corner = document.createElement("th");
    corner.scope = "col";
    corner.className = "chart-grid-corner";
    corner.textContent = view.firstColumn === "x" ? t("chart.xValues") : t("chart.categories");
    headRow.append(corner);
    const lastSeries = columns <= 1 ? t("chart.lastSeries") : "";
    view.series.forEach((name, c) => {
      const th = document.createElement("th");
      th.scope = "col";
      const wrap = document.createElement("div");
      wrap.className = "chart-grid-head";
      wrap.append(
        cellInput(-1, c, name, t("chart.seriesNameLabel", { n: c + 1 })),
        iconButton("close", t("chart.removeSeries", { name: name || t("chart.seriesFallback", { n: c + 1 }) }), () => {
          const patch = draft();
          if (patch) void commit(removeSeries(patch, c), { row: -1, col: Math.max(0, c - 1) });
        }, lastSeries),
      );
      th.append(wrap);
      headRow.append(th);
    });
    headRow.append(document.createElement("th"));
    head.append(headRow);
    const body = document.createElement("tbody");
    const lastRow = rows <= 1 ? t("chart.lastRow") : "";
    for (let r = 0; r < rows; r += 1) {
      const tr = document.createElement("tr");
      const th = document.createElement("th");
      th.scope = "row";
      th.append(cellInput(r, -1, view.labels[r], t("chart.rowNameLabel", { n: r + 1 })));
      tr.append(th);
      for (let c = 0; c < columns; c += 1) {
        const td = document.createElement("td");
        const series = view.series[c] || t("chart.seriesFallback", { n: c + 1 });
        const label = view.labels[r] || t("chart.rowFallback", { n: r + 1 });
        td.append(cellInput(r, c, view.cells[r]?.[c] ?? "", t("chart.cellLabel", { series, label })));
        tr.append(td);
      }
      const tail = document.createElement("td");
      tail.className = "chart-grid-tail";
      tail.append(
        iconButton("close", t("chart.removeRow", { name: view.labels[r] || t("chart.rowFallback", { n: r + 1 }) }), () => {
          const patch = draft();
          if (patch) void commit(removeRow(patch, r), { row: Math.max(0, r - 1), col: 0 });
        }, lastRow),
      );
      tr.append(tail);
      body.append(tr);
    }
    table.append(head, body);
    parts.grid.replaceChildren(table);
    const fullRows = rows >= view.maxRows ? t("chart.rowLimit", { n: view.maxRows }) : "";
    const fullSeries = columns >= view.maxSeries ? t("chart.seriesLimit", { n: view.maxSeries }) : "";
    parts.addRow.disabled = !view.editable || !!fullRows;
    parts.addRow.title = fullRows;
    parts.addSeries.disabled = !view.editable || !!fullSeries;
    parts.addSeries.title = fullSeries;
  }

  function render({ json, view: next }, focus = null) {
    if (!el) return;
    if (!next) {
      close();
      return;
    }
    view = next;
    lastJson = json;
    // Read-only, with the reason — or editable, with the one consequence a
    // reader must know BEFORE the first change: an imported chart's embedded
    // workbook is replaced on save (`docs/155` §12 Q-A).
    const notice = !view.editable
      ? routeRefusal(view.code) || view.reason
      : view.replacesWorkbook
        ? t("chart.replacesWorkbook")
        : "";
    parts.reason.hidden = !notice;
    parts.reason.textContent = notice;
    parts.title.value = view.title;
    parts.title.disabled = !view.editable;
    if (view.titleLimit) parts.title.maxLength = view.titleLimit;
    parts.legend.value = view.legend;
    parts.legend.disabled = !view.editable;
    renderGallery();
    renderGrid();
    if (!focus) return;
    if (focus.kind) parts.gallery.querySelector(`[data-kind="${focus.kind}"]`)?.focus();
    else if (focus.field) parts[focus.field]?.focus();
    else {
      const cell = parts.grid.querySelector(`input[data-row="${focus.row}"][data-col="${focus.col}"]`);
      cell?.focus();
      cell?.select();
    }
  }

  /** Spreadsheet keys: Enter commits and moves down, Tab across, arrows move
   *  when the caret is at the edge of the text, Escape abandons the cell. */
  function onGridKey(event) {
    const input = event.target;
    if (!(input instanceof HTMLInputElement) || input.dataset.row === undefined) return;
    const r = Number(input.dataset.row);
    const c = Number(input.dataset.col);
    let to = null;
    if (event.key === "Enter") to = { row: event.shiftKey ? r - 1 : r + 1, col: c };
    else if (event.key === "ArrowDown") to = { row: r + 1, col: c };
    else if (event.key === "ArrowUp") to = { row: r - 1, col: c };
    else if (event.key === "ArrowRight" && input.selectionStart === input.value.length) to = { row: r, col: c + 1 };
    else if (event.key === "ArrowLeft" && input.selectionEnd === 0) to = { row: r, col: c - 1 };
    else if (event.key === "Escape") {
      // First Escape abandons a pending entry; on an untouched cell it falls
      // through to the panel, which closes — a spreadsheet's two-step Escape.
      const original = (r === -1 ? view.series[c] : c === -1 ? view.labels[r] : view.cells[r][c]) ?? "";
      if (input.value === original && !input.hasAttribute("aria-invalid")) return;
      event.preventDefault();
      event.stopPropagation();
      input.value = original;
      input.removeAttribute("aria-invalid");
      note("");
      return;
    }
    if (!to) return;
    event.preventDefault();
    const rows = view.labels.length;
    const columns = view.series.length;
    // Enter on the last row adds a row, as typing down a spreadsheet column does.
    if (event.key === "Enter" && to.row >= rows && rows < view.maxRows) {
      const patch = draft();
      if (patch) void commit(insertRow(patch, rows - 1, names.row(rows + 1)), { row: rows, col: c });
      return;
    }
    to.row = Math.min(Math.max(to.row, -1), rows - 1);
    to.col = Math.min(Math.max(to.col, -1), columns - 1);
    const patch = draft();
    const changed = patch && JSON.stringify(patch) !== JSON.stringify(patchFromView(view));
    if (changed) void commit(patch, to);
    else parts.grid.querySelector(`input[data-row="${to.row}"][data-col="${to.col}"]`)?.focus();
  }

  function onGridChange(event) {
    const input = event.target;
    if (!(input instanceof HTMLInputElement) || input.dataset.row === undefined) return;
    const patch = draft();
    if (patch) void commit(patch);
  }

  function onGridPaste(event) {
    const input = event.target;
    if (!(input instanceof HTMLInputElement) || input.dataset.row === undefined || !view?.editable) return;
    const text = event.clipboardData?.getData("text/plain") ?? "";
    const block = parseClipboardGrid(text);
    // A single value is an ordinary paste into the field the caret is in.
    if (block.length === 1 && block[0].length === 1) return;
    event.preventDefault();
    const patch = draft();
    if (!patch) return;
    const r = Number(input.dataset.row);
    const c = Number(input.dataset.col);
    const { patch: next, clipped } = pasteBlock(patch, r, c, block, view, names);
    void commit(next, { row: r, col: c }).then((ok) => {
      if (ok && clipped) note(t("chart.pasteClipped", { count: clipped }), true);
    });
  }

  function ensure() {
    if (el) return el;
    el = document.createElement("aside");
    el.className = "side-panel properties-panel chart-data-panel";
    el.id = "chartDataPanel";
    el.hidden = true;
    el.setAttribute("aria-labelledby", "chartDataTitle");
    const headEl = document.createElement("header");
    headEl.className = "panel-head properties-panel-head";
    const heading = document.createElement("div");
    heading.className = "properties-panel-heading";
    const titleWrap = document.createElement("span");
    const strong = document.createElement("strong");
    strong.className = "panel-title";
    strong.id = "chartDataTitle";
    strong.textContent = t("chart.panelTitle");
    const small = document.createElement("small");
    small.textContent = t("chart.panelIntro");
    titleWrap.append(strong, small);
    heading.append(titleWrap);
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "panel-close";
    closeBtn.setAttribute("aria-label", t("chart.close"));
    closeBtn.title = t("chart.close");
    closeBtn.innerHTML = '<span class="ms" aria-hidden="true">close</span>';
    closeBtn.addEventListener("click", () => close());
    headEl.append(heading, closeBtn);

    const body = document.createElement("div");
    body.className = "panel-body properties-panel-body";
    parts.reason = document.createElement("p");
    parts.reason.className = "chart-data-reason";
    parts.reason.hidden = true;

    const typeGroup = document.createElement("fieldset");
    typeGroup.className = "dialog-group property-section";
    const typeLegend = document.createElement("legend");
    typeLegend.textContent = t("chart.typeHeading");
    parts.gallery = document.createElement("div");
    parts.gallery.className = "chart-kind-gallery";
    parts.gallery.setAttribute("role", "radiogroup");
    parts.gallery.setAttribute("aria-label", t("chart.typeHeading"));
    parts.gallery.addEventListener("keydown", (event) => {
      const keys = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };
      if (!(event.key in keys)) return;
      event.preventDefault();
      const buttons = [...parts.gallery.querySelectorAll(".chart-kind")];
      const at = buttons.indexOf(document.activeElement);
      const next = buttons[(at + keys[event.key] + buttons.length) % buttons.length];
      if (next) chooseKind(next.dataset.kind);
    });
    typeGroup.append(typeLegend, parts.gallery);

    const labelGroup = document.createElement("fieldset");
    labelGroup.className = "dialog-group property-section";
    const labelLegend = document.createElement("legend");
    labelLegend.textContent = t("chart.labelsHeading");
    const titleField = document.createElement("label");
    titleField.className = "dialog-field";
    titleField.append(t("chart.titleField"));
    parts.title = document.createElement("input");
    parts.title.type = "text";
    parts.title.autocomplete = "off";
    parts.title.placeholder = t("chart.titlePlaceholder");
    parts.title.addEventListener("change", () => {
      const patch = draft();
      if (patch) void commit(patch, { field: "title" });
    });
    parts.title.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        parts.title.blur();
      }
    });
    titleField.append(parts.title);
    const legendField = document.createElement("label");
    legendField.className = "dialog-field";
    legendField.append(t("chart.legendField"));
    parts.legend = document.createElement("select");
    for (const choice of LEGEND_CHOICES) {
      const option = document.createElement("option");
      option.value = choice;
      option.textContent = t(LEGEND_LABEL_KEYS[choice]);
      parts.legend.append(option);
    }
    parts.legend.addEventListener("change", () => {
      const patch = draft();
      if (patch) void commit(patch, { field: "legend" });
    });
    legendField.append(parts.legend);
    labelGroup.append(labelLegend, titleField, legendField);

    const dataGroup = document.createElement("fieldset");
    dataGroup.className = "dialog-group property-section chart-data-section";
    const dataLegend = document.createElement("legend");
    dataLegend.textContent = t("chart.dataHeading");
    const hint = document.createElement("p");
    hint.className = "chart-data-hint";
    hint.textContent = t("chart.dataHint");
    parts.grid = document.createElement("div");
    parts.grid.className = "chart-grid-scroll";
    parts.grid.addEventListener("keydown", onGridKey);
    parts.grid.addEventListener("change", onGridChange);
    parts.grid.addEventListener("paste", onGridPaste);
    parts.grid.addEventListener("input", (event) => {
      if (event.target instanceof HTMLInputElement && event.target.hasAttribute("aria-invalid")) {
        event.target.removeAttribute("aria-invalid");
        note("");
      }
    });
    const actions = document.createElement("div");
    actions.className = "chart-grid-actions";
    parts.addRow = document.createElement("button");
    parts.addRow.type = "button";
    parts.addRow.className = "dialog-button";
    parts.addRow.textContent = t("chart.addRow");
    parts.addRow.addEventListener("click", () => {
      const patch = draft();
      if (patch) void commit(insertRow(patch, patch.labels.length - 1, names.row(patch.labels.length + 1)), { row: patch.labels.length, col: -1 });
    });
    parts.addSeries = document.createElement("button");
    parts.addSeries.type = "button";
    parts.addSeries.className = "dialog-button";
    parts.addSeries.textContent = t("chart.addSeries");
    parts.addSeries.addEventListener("click", () => {
      const patch = draft();
      if (patch) void commit(insertSeries(patch, patch.series.length - 1, names.series(patch.series.length + 1)), { row: -1, col: patch.series.length });
    });
    actions.append(parts.addRow, parts.addSeries);
    parts.note = document.createElement("p");
    parts.note.className = "chart-data-note";
    parts.note.setAttribute("role", "status");
    parts.note.setAttribute("aria-live", "polite");
    dataGroup.append(dataLegend, hint, parts.grid, actions, parts.note);

    body.append(parts.reason, typeGroup, labelGroup, dataGroup);
    el.append(headEl, body);
    el.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !event.defaultPrevented) {
        event.preventDefault();
        close();
      }
    });
    // Inside `.workarea`, like every other side panel, so the page shifts aside
    // for it rather than being covered.
    (io.host?.() ?? document.querySelector(".workarea") ?? document.body).append(el);
    return el;
  }

  /** Opens the panel on the chart anchored at `target` (default: the selected
   *  chart). Returns false — and says why on the status line — when there is no
   *  chart to open. `focus` is "data" (the first cell) or "none". */
  function open(target = io.selection()?.node, { focus = "data" } = {}) {
    const result = read(target);
    if (!result.view) {
      io.setStatus(t("chart.noChartSelected"), "error");
      return false;
    }
    ensure();
    node = target;
    el.hidden = false;
    note("");
    render(result, focus === "data" ? { row: 0, col: 0 } : null);
    return true;
  }

  function close() {
    if (!el || el.hidden) return;
    const hadFocus = el.contains(document.activeElement);
    el.hidden = true;
    node = null;
    view = null;
    lastJson = "";
    if (hadFocus) io.returnFocus?.();
  }

  /** Keeps an open panel honest after any repaint: follows the selection to
   *  another chart, closes when its chart is gone, and redraws after an undo or
   *  redo changed the data underneath it. A cell being typed in is left alone —
   *  redrawing it would throw away what the reader is typing. */
  function sync() {
    if (!el || el.hidden || busy) return;
    const selected = io.selection();
    if (selected?.kind === "chart" && selected.node !== node) {
      const next = read(selected.node);
      if (next.view) {
        node = selected.node;
        note("");
        render(next);
        return;
      }
    }
    const current = read(node);
    if (!current.view) {
      close();
      return;
    }
    if (current.json === lastJson) return;
    const typing = el.contains(document.activeElement) && document.activeElement instanceof HTMLInputElement;
    if (typing) return;
    render(current);
  }

  return {
    open,
    close,
    sync,
    isOpen: () => !!el && !el.hidden,
    node: () => node,
  };
}
