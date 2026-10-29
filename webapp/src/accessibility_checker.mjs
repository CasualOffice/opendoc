// Check accessibility: Word's Review ▸ Check Accessibility, over our own
// accessibility projection (HF-064).
//
// The editor made ITSELF accessible — the off-screen mirror, the roles, the
// announced changes — and never audited the document being written. Word's
// Accessibility Checker does: it lists Errors (missing alternative text,
// missing table header) and Warnings (skipped heading levels, merged cells,
// blank table rows) and takes the reader to each one. ONLYOFFICE has no checker
// at all (`docs/105` §4), so this is judged against Word.
//
// THE SOURCE IS THE PROJECTION A SCREEN READER GETS. `accessibilityTreeWindow`
// is exactly what assistive technology is handed — headings with levels,
// pictures with their alt text or none, tables with their header rows and cell
// spans — so a finding here is a finding about what a screen-reader user will
// actually meet, not about a second reading of the model that could disagree
// with it. Its parallel `nodes` array names the body block each finding came
// from, which is what Go to moves the caret to.
//
// CONTRAST is the one rule the projection cannot answer, because a screen
// reader is handed no colours. The engine grades it instead
// (`accessibilityContrastFindings`, `crates/casual-doc-wasm/src/
// accessibility_check.rs`): WCAG 2.1 §1.4.3's 4.5:1, or 3:1 for large text,
// against the highlight, shading, cell fill or page colour actually behind each
// run. Word files it as a warning ("Hard-to-read text contrast"), and so does
// this.
//
// NOT CHECKED, and said rather than implied: hyperlink text ("click here"),
// which the projection does not carry; theme and automatic text colours, and
// text outside the body, which the contrast rule does not grade (that module's
// comment says why); and blocks inside a collapsed heading, which the projection
// omits by design (ADR-049) — the dialog says so when the document has a fold.
//
// COST: one O(document) projection walk per check, on request only, never per
// keystroke.
import { t } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";

/** The installed checker, so every surface reaches the ONE opener: ESM gives one
 *  instance of this module, the shape `popover_manager.mjs` uses. */
let installed = null;

/** Opens the check — the palette's and the menu's way in. */
export function openAccessibilityCheck() {
  installed?.open();
}

/** Severity order, Word's: errors first. */
const SEVERITY = Object.freeze({ error: 0, warning: 1 });

/**
 * The findings for a projection, in document order within each severity.
 *
 * Pure: `blocks` is `accessibilityTreeWindow`'s `blocks`, `nodes` its parallel
 * owner ids. Each finding is `{severity, rule, node, where}`; `where` is the
 * data a sentence is built from (`{picture}`, `{table, caption}`, or
 * `{heading, from, to}`), so this module's caller decides the words.
 *
 * O(blocks + table cells).
 *
 * @param {ReadonlyArray<object>} blocks
 * @param {ReadonlyArray<string>} [nodes]
 * @returns {Array<{severity: "error"|"warning", rule: string, node: string|null, where: object}>}
 */
export function checkAccessibility(blocks, nodes = []) {
  const out = [];
  let picture = 0;
  let table = 0;
  let lastLevel = 0;
  blocks.forEach((block, i) => {
    const node = nodes[i] ?? null;
    switch (block?.kind) {
      case "image": {
        picture += 1;
        if (!String(block.alt ?? "").trim()) {
          out.push({ severity: "error", rule: "missingAlt", node, where: { picture } });
        }
        break;
      }
      case "heading": {
        const level = Number(block.level) || 1;
        // Word's rule: a heading may go DOWN any number of levels, and up one at
        // a time. The first heading may be any level after a title.
        if (lastLevel > 0 && level > lastLevel + 1) {
          out.push({
            severity: "warning",
            rule: "headingSkip",
            node,
            where: { heading: String(block.text ?? ""), from: lastLevel, to: level },
          });
        }
        lastLevel = level;
        break;
      }
      case "table": {
        table += 1;
        const rows = Array.isArray(block.rows) ? block.rows : [];
        const where = { table, caption: block.caption ?? "" };
        if (rows.length > 1 && !(block.headerRows?.length > 0)) {
          out.push({ severity: "error", rule: "tableHeader", node, where });
        }
        if (rows.some((row) => row.some((cell) => (cell.colSpan ?? 1) > 1 || (cell.rowSpan ?? 1) > 1))) {
          out.push({ severity: "warning", rule: "mergedCells", node, where });
        }
        if (rows.some((row) => row.length > 0 && row.every((cell) => !String(cell.text ?? "").trim() && !(cell.checkboxes?.length > 0)))) {
          out.push({ severity: "warning", rule: "emptyRow", node, where });
        }
        break;
      }
      default:
        break;
    }
  });
  return out
    .map((finding, order) => ({ finding, order }))
    .sort((a, b) => SEVERITY[a.finding.severity] - SEVERITY[b.finding.severity] || a.order - b.order)
    .map(({ finding }) => finding);
}

/**
 * The engine's contrast findings as checker findings: Word's "Hard-to-read
 * text contrast", a warning. Pure. O(findings).
 *
 * @param {ReadonlyArray<{node: string, offset: number, text: string, ratio: number}>} rows
 */
export function contrastFindings(rows) {
  return (rows ?? []).map((row) => ({
    severity: "warning",
    rule: "lowContrast",
    node: row.node ?? null,
    offset: Number(row.offset) || 0,
    where: { text: String(row.text ?? ""), ratio: Number(row.ratio) || 0 },
  }));
}

/** Findings from both sources, errors first, each source in document order. */
export function mergeFindings(...lists) {
  return lists
    .flat()
    .map((finding, order) => ({ finding, order }))
    .sort((a, b) => SEVERITY[a.finding.severity] - SEVERITY[b.finding.severity] || a.order - b.order)
    .map(({ finding }) => finding);
}

/** The sentence naming where a finding is. */
function whereText(finding) {
  const w = finding.where;
  if (w.picture != null) return t("a11yCheck.where.picture", { n: String(w.picture) });
  if (w.table != null) {
    const label = t("a11yCheck.where.table", { n: String(w.table) });
    return w.caption ? `${label} — ${w.caption}` : label;
  }
  if (w.heading != null) return `“${w.heading}”`;
  if (w.ratio != null) return t("a11yCheck.where.contrast", { text: w.text, ratio: w.ratio.toFixed(2) });
  return "";
}

/** The rule's title and its fix, through literal keys the extractor can read. */
function ruleText(rule) {
  switch (rule) {
    case "missingAlt":
      return [t("a11yCheck.rule.missingAlt"), t("a11yCheck.fix.missingAlt")];
    case "tableHeader":
      return [t("a11yCheck.rule.tableHeader"), t("a11yCheck.fix.tableHeader")];
    case "headingSkip":
      return [t("a11yCheck.rule.headingSkip"), t("a11yCheck.fix.headingSkip")];
    case "mergedCells":
      return [t("a11yCheck.rule.mergedCells"), t("a11yCheck.fix.mergedCells")];
    case "emptyRow":
      return [t("a11yCheck.rule.emptyRow"), t("a11yCheck.fix.emptyRow")];
    case "lowContrast":
      return [t("a11yCheck.rule.lowContrast"), t("a11yCheck.fix.lowContrast")];
    default:
      return [rule, ""];
  }
}

/**
 * Wires the Accessibility check dialog.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc
 * @param {(node: string, offset?: number) => boolean} io.goTo puts the caret in
 *        a body block — a table's or a block control's first paragraph — and
 *        answers false when there is nowhere it can go
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {() => Element|null} io.fallbackFocus
 * @returns {{open: () => void, run: () => Array<object>|null}}
 */
export function createAccessibilityChecker(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("a11yCheckDialog");
  const list = el("a11yCheckList");
  const summary = el("a11yCheckSummary");
  const foldNote = el("a11yCheckFoldNote");
  if (!dialog || !list) return { open() {}, run: () => null };
  // The band button opens a dialog, so it takes focus before the shared surface
  // handler runs (see page_borders.mjs): otherwise Escape returns to the editor.
  el("reviewA11yCheckBtn")?.addEventListener(
    "click",
    (event) => event.currentTarget.focus({ preventScroll: true }),
    { capture: true },
  );
  const modal = registerModal(dialog, {
    initialFocus: () => dialog.querySelector("[data-a11y-initial]"),
    fallbackFocus: io.fallbackFocus,
    defaultAction: () => modal.close("done"),
  });
  for (const id of ["a11yCheckDone", "a11yCheckClose"]) el(id)?.addEventListener("click", () => modal.close("done"));

  /** The whole projection, then the findings. Null without a document. */
  function run() {
    const doc = io.getDoc();
    if (!doc) return null;
    const head = JSON.parse(doc.accessibilityTreeWindow(0, 0));
    const all = JSON.parse(doc.accessibilityTreeWindow(0, Math.max(0, Number(head.total) || 0)));
    let contrast = [];
    try {
      contrast = JSON.parse(doc.accessibilityContrastFindings?.() ?? "[]");
    } catch {
      contrast = [];
    }
    return mergeFindings(checkAccessibility(all.blocks ?? [], all.nodes ?? []), contrastFindings(contrast));
  }

  function render(findings) {
    list.replaceChildren();
    const errors = findings.filter((f) => f.severity === "error").length;
    const warnings = findings.length - errors;
    summary.textContent = findings.length
      ? t("a11yCheck.summary", { errors: String(errors), warnings: String(warnings) })
      : t("a11yCheck.none");
    for (const finding of findings) {
      const [title, fix] = ruleText(finding.rule);
      const item = document.createElement("li");
      item.className = `a11y-check-item is-${finding.severity}`;
      const head = document.createElement("p");
      head.className = "a11y-check-head";
      const badge = document.createElement("span");
      badge.className = "a11y-check-severity";
      badge.textContent = finding.severity === "error" ? t("a11yCheck.severity.error") : t("a11yCheck.severity.warning");
      const name = document.createElement("strong");
      name.textContent = title;
      head.append(badge, " ", name);
      const where = document.createElement("p");
      where.className = "a11y-check-where";
      where.textContent =
        finding.rule === "headingSkip"
          ? `${whereText(finding)} — ${t("a11yCheck.where.levels", { from: String(finding.where.from), to: String(finding.where.to) })}`
          : whereText(finding);
      const how = document.createElement("p");
      how.className = "a11y-check-fix";
      how.textContent = fix;
      item.append(head, where, how);
      // Every finding with a place has a Go to; a table's goes to its first
      // cell, which is where Word's checker puts the selection too.
      if (finding.node) {
        const go = document.createElement("button");
        go.type = "button";
        go.className = "dialog-button";
        go.textContent = t("a11yCheck.goTo");
        go.setAttribute("aria-label", t("a11yCheck.goToNamed", { name: `${title}, ${where.textContent}` }));
        go.addEventListener("click", () => {
          if (io.goTo(finding.node, finding.offset ?? 0)) modal.close("goTo");
          else io.setStatus(t("a11yCheck.goToUnavailable"), "error");
        });
        item.append(go);
      }
      list.append(item);
    }
  }

  function open() {
    const doc = io.getDoc();
    if (!doc) {
      io.setStatus(t("command.needsDocument"), "error");
      return;
    }
    const findings = run() ?? [];
    let folded = 0;
    try {
      folded = (JSON.parse(doc.foldState()).folded ?? []).length;
    } catch {
      folded = 0;
    }
    if (foldNote) foldNote.hidden = folded === 0;
    render(findings);
    // Word's pane opens on the first finding; with none, on Close. Marked so the
    // dialog contract can name the element without knowing the findings.
    for (const node of dialog.querySelectorAll("[data-a11y-initial]")) delete node.dataset.a11yInitial;
    (list.querySelector("button") ?? el("a11yCheckDone")).dataset.a11yInitial = "";
    modal.open();
  }

  installed = { open, run };
  return installed;
}
