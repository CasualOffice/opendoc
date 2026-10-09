// The chart's data, as ONLYOFFICE and Word edit it: a sheet in its own window.
//
// ONLYOFFICE's Chart tab ▸ Edit Data opens a spreadsheet editor over the
// document; Word's Chart Design ▸ Edit Data opens a data sheet. Both are a grid
// of rows (categories) and columns (series) that the chart follows. This module
// is that window — a modal Chart Data dialog — plus the grid arithmetic it runs
// on. The chart's TYPE, elements, style and axes are not here: they are the
// Chart tab's and the settings panel's (`chart_panel.mjs`), all reading one
// command tree (`chart_commands.mjs`), which is ONLYOFFICE's own split.
//
// | Word / ONLYOFFICE | Here |
// | --- | --- |
// | Insert ▸ Chart opens the data straight away | the dialog opens on the new chart, first value focused |
// | row 1 series names, column A categories, the body numbers | the same grid |
// | edits reach the chart as they are made | each committed cell is one undoable write; the chart repaints behind the dialog |
// | paste a block from a spreadsheet | a tab-separated paste fills from the cell it lands in and grows the grid |
//
// A chart the engine will not let anyone change (one using constructs this
// build cannot rewrite, or a combination chart) opens READ-ONLY with the reason
// above the grid — a control that vanishes cannot be told from a bug.
//
// Cost: every read and write is O(series × rows) in the one chart; nothing
// walks the document (`docs/107` §4).

/** Legend positions in ONLYOFFICE's Chart Elements ▸ Legend order, each with
 *  its catalogue key. Spelled out so the locale extractor sees every key. */
export const LEGEND_LABEL_KEYS = Object.freeze({
  none: "chart.legend.none",
  top: "chart.legend.top",
  bottom: "chart.legend.bottom",
  left: "chart.legend.left",
  right: "chart.legend.right",
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

/** The write payload a view describes — the six grid fields `setChartData`
 *  reads. Formatting is sent separately, as `format`, only when it changes. */
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

/** The Chart Data dialog.
 *
 *  @param {object} io
 *  @param {ReturnType<import("./chart_commands.mjs").createChartBridge>} io.bridge
 *  @param {(element: HTMLElement, options: object) => {open: Function, close: Function, isOpen: boolean}} io.registerModal
 *  @param {(key: string, params?: object) => string} io.t
 *  @param {() => void} [io.fallbackFocus]
 */
export function createChartDataDialog(io) {
  const t = io.t;
  let el = null;
  let modal = null;
  let node = null;
  let view = null;
  const parts = {};

  function note(message, isError = false) {
    parts.note.textContent = message || "";
    parts.note.classList.toggle("error", !!isError && !!message);
  }

  const names = {
    row: (n) => (view?.firstColumn === "x" ? "" : t("chart.newCategory", { n })),
    series: (n) => t("chart.newSeries", { n }),
  };

  /** The grid as a write payload, read from the DOM so a value typed and not
   *  yet committed is part of the next write. O(cells). */
  function draft() {
    if (!view) return null;
    const patch = patchFromView(view);
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

  /** Writes `patch` and redraws from what the engine NOW holds — never from the
   *  patch, so the grid can only show data the document has. */
  async function commit(patch, focus = null) {
    if (!node || !view?.editable) return false;
    const bad = invalidCell(patch);
    if (bad) {
      note(bad.sentence, true);
      const cell = parts.grid.querySelector(`input[data-row="${bad.r}"][data-col="${bad.c}"]`);
      cell?.setAttribute("aria-invalid", "true");
      cell?.focus();
      return false;
    }
    let refused = "";
    const changed = await io.bridge.write(node, (current) => ({ ...patch, kind: current.kind, title: current.title, legend: current.legend }), {
      onRefused: (sentence) => {
        refused = sentence;
      },
    });
    if (refused) note(refused, true);
    else note("");
    render(focus);
    return changed;
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
    input.readOnly = !view.editable;
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

  function render(focus = null) {
    if (!el || !node) return;
    // A redraw replaces every input, so without this the cell being typed in
    // vanishes from under the caret — and a late `change` from the replaced
    // input (Chrome fires one on removal) redraws AGAIN with no target, which
    // dropped focus onto <body>. The cell keeps focus unless told otherwise.
    const active = document.activeElement;
    if (!focus && active instanceof HTMLInputElement && parts.grid.contains(active)) {
      focus = { row: Number(active.dataset.row), col: Number(active.dataset.col), keepCaret: true };
    }
    const next = io.bridge.view(node);
    if (!next) {
      close();
      return;
    }
    view = next;
    const notice = !view.editable
      ? io.bridge.routeRefusal(view.code) || view.reason
      : view.replacesWorkbook
        ? t("chart.replacesWorkbook")
        : "";
    parts.reason.hidden = !notice;
    parts.reason.textContent = notice;
    parts.reason.classList.toggle("is-refusal", !view.editable);
    renderGrid();
    if (!focus) return;
    const cell = parts.grid.querySelector(`input[data-row="${focus.row}"][data-col="${focus.col}"]`);
    cell?.focus();
    if (!focus.keepCaret) cell?.select();
  }

  /** Abandons the entry in the focused cell, if it differs from the document's
   *  value or was refused. True when there was one to abandon. */
  function revertPendingCell() {
    const input = document.activeElement;
    if (!(input instanceof HTMLInputElement) || !parts.grid?.contains(input) || !view) return false;
    const r = Number(input.dataset.row);
    const c = Number(input.dataset.col);
    const original = (r === -1 ? view.series[c] : c === -1 ? view.labels[r] : view.cells[r]?.[c]) ?? "";
    if (input.value === original && !input.hasAttribute("aria-invalid")) return false;
    input.value = original;
    input.removeAttribute("aria-invalid");
    note("");
    return true;
  }

  /** Spreadsheet keys: Enter commits and moves down (adding a row on the last),
   *  Tab across, arrows at the edge of the text. Escape is the modal's, which
   *  asks `revertPendingCell` first. */
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
    if (!to) return;
    event.preventDefault();
    const rows = view.labels.length;
    const columns = view.series.length;
    if (event.key === "Enter" && to.row >= rows && rows < view.maxRows && view.editable) {
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
    const block = parseClipboardGrid(event.clipboardData?.getData("text/plain") ?? "");
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
    if (el) return;
    el = document.createElement("div");
    el.id = "chartDataDialog";
    el.className = "dialog-overlay chart-data-overlay";
    el.hidden = true;
    el.setAttribute("role", "dialog");
    el.setAttribute("aria-modal", "true");
    el.setAttribute("aria-labelledby", "chartDataTitle");
    el.setAttribute("aria-describedby", "chartDataDescription");
    const card = document.createElement("section");
    card.className = "dialog-card chart-data-card";
    const head = document.createElement("header");
    head.className = "dialog-head";
    const heading = document.createElement("div");
    heading.className = "dialog-heading";
    const titles = document.createElement("div");
    const h2 = document.createElement("h2");
    h2.id = "chartDataTitle";
    h2.textContent = t("chart.dataTitle");
    const p = document.createElement("p");
    p.id = "chartDataDescription";
    p.textContent = t("chart.dataHint");
    titles.append(h2, p);
    heading.append(titles);
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "dialog-close";
    closeBtn.title = t("chart.closeData");
    closeBtn.setAttribute("aria-label", t("chart.closeData"));
    closeBtn.innerHTML = '<span class="ms" aria-hidden="true">close</span>';
    closeBtn.addEventListener("click", () => close());
    head.append(heading, closeBtn);

    const body = document.createElement("div");
    body.className = "dialog-body chart-data-body";
    parts.reason = document.createElement("p");
    parts.reason.className = "chart-data-reason";
    parts.reason.hidden = true;
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
    body.append(parts.reason, parts.grid, actions, parts.note);

    const foot = document.createElement("footer");
    foot.className = "dialog-foot";
    const hint = document.createElement("span");
    hint.className = "dialog-note";
    hint.textContent = t("chart.dataUndoNote");
    const done = document.createElement("div");
    done.className = "dialog-actions";
    const doneBtn = document.createElement("button");
    doneBtn.type = "button";
    doneBtn.className = "dialog-button dialog-button-primary";
    doneBtn.textContent = t("chart.done");
    doneBtn.addEventListener("click", () => close());
    done.append(doneBtn);
    foot.append(hint, done);
    card.append(head, body, foot);
    el.append(card);
    document.body.append(el);
    modal = io.registerModal(el, {
      initialFocus: () => parts.grid.querySelector('input[data-row="0"][data-col="0"]') ?? doneBtn,
      // The spreadsheet's two-step Escape: the first abandons a pending entry,
      // the next one closes the window.
      escape: () => revertPendingCell(),
      fallbackFocus: () => io.fallbackFocus?.(),
      onClose: () => {
        // A value typed and not yet committed is committed on the way out —
        // closing the window is how a spreadsheet user says "done".
        const patch = draft();
        if (patch && JSON.stringify(patch) !== JSON.stringify(patchFromView(view)) && !invalidCell(patch)) {
          void io.bridge.write(node, () => patch);
        }
        node = null;
        view = null;
      },
    });
  }

  /** Opens the dialog on the chart at `target`. False when it is not a chart. */
  function open(target) {
    if (!io.bridge.view(target)) return false;
    ensure();
    node = target;
    note("");
    render();
    modal.open();
    const first = parts.grid.querySelector('input[data-row="0"][data-col="0"]');
    first?.focus();
    first?.select();
    return true;
  }

  function close() {
    if (modal?.isOpen) modal.close("done");
  }

  /** Redraws after an undo or redo changed the data underneath an open dialog,
   *  unless a cell is being typed in. */
  function sync() {
    if (!modal?.isOpen || !node) return;
    const current = io.bridge.view(node);
    if (!current) {
      close();
      return;
    }
    if (current.json === view?.json) return;
    if (el.contains(document.activeElement) && document.activeElement instanceof HTMLInputElement) return;
    render();
  }

  return { open, close, sync, isOpen: () => !!modal?.isOpen, node: () => node };
}
