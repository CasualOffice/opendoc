// The compatibility findings: the header chip that counts them, and the dialog
// that says what they are.
//
// `importReportJson` started being reported as a COUNT ("191 import findings")
// and nothing more: the chip was a `<span role="status">` that could not be
// clicked, could not take focus, and no command reached what it counted
// (`desk-11-import-findings.png`). SKILL §1 is explicit that verbatim retention
// is only an advantage over ONLYOFFICE if loss is detected AND reported, and a
// number with nothing behind it reports that something happened without saying
// what — the reader cannot tell a dropped tracked change from a kept settings
// flag. So the chip is a button now, and it opens the findings themselves,
// grouped by what HAPPENED to each, which is the question a reader deciding
// whether to save over the original is actually asking:
//
//   lost          `retentionOutcome: not_retained` — gone from the saved file;
//   refused       `blocked` / `rejected` — refused for safety or limits;
//   approximated  `degraded` and `preserved` — shown approximately, original kept;
//   preserved     `omitted` and `preserved` — kept in the file, not shown here;
//   other         anything the engine adds later, never silently dropped.
//
// Ordered most-serious first. The same chip and dialog serve the export report
// after a save, because the chip already switches phase then.
//
// The dialog is built here rather than authored in `editor.html`, and goes
// through the one modal contract (`modal.mjs`) like every authored one, so
// Escape, the backdrop, the focus trap and focus return are the shared ones and
// `dialog-contract.spec.mjs` holds it to them through the roster. Its words are
// written at OPEN, because it is built while `main.js` is still evaluating —
// before any catalogue exists — and a locale change since must reach it.
//
// Complexity: parsing is O(entries) once per open or save; rendering is
// O(entries) per opening. The report is aggregated per feature by the engine, so
// entries are tens, not one per occurrence.
import { n, t } from "./i18n.mjs";
import { compatibilityOccurrenceCount } from "./format_io.mjs";
import { showCompatibilityFindings } from "./save_formats.mjs";

/** The groups, most serious first, and the catalogue key each is named by. */
export const FINDING_KINDS = Object.freeze([
  Object.freeze({ id: "lost", key: "findings.kind.lost" }),
  Object.freeze({ id: "refused", key: "findings.kind.refused" }),
  Object.freeze({ id: "approximated", key: "findings.kind.approximated" }),
  Object.freeze({ id: "preserved", key: "findings.kind.preserved" }),
  Object.freeze({ id: "other", key: "findings.kind.unsorted" }),
]);

/** Which group one report entry belongs to. Retention first: a construct that
 *  was approximated AND not kept is, to a reader, not kept. */
export function findingKind(entry) {
  const retention = entry?.retentionOutcome;
  if (retention === "not_retained") return "lost";
  if (retention === "blocked" || retention === "rejected") return "refused";
  // Both remaining groups promise "the original is kept", so both require it.
  if (retention === "preserved" && entry?.modelOutcome === "degraded") return "approximated";
  if (retention === "preserved" && entry?.modelOutcome === "omitted") return "preserved";
  return "other";
}

/**
 * A report's entries grouped by kind, in `FINDING_KINDS` order, empty groups
 * left out, each with its occurrence total.
 *
 * @param {string} reportJson the engine's `{entries: [...]}`; empty means none.
 * @returns {Array<{id: string, key: string, total: number, entries: object[]}>}
 */
export function groupFindings(reportJson) {
  if (!reportJson) return [];
  const report = JSON.parse(reportJson);
  const entries = Array.isArray(report?.entries) ? report.entries : [];
  const groups = new Map(FINDING_KINDS.map((kind) => [kind.id, { ...kind, total: 0, entries: [] }]));
  for (const entry of entries) {
    const group = groups.get(findingKind(entry));
    const occurrences = Number.isSafeInteger(entry?.occurrences) && entry.occurrences > 0 ? entry.occurrences : 0;
    group.total += occurrences;
    group.entries.push(entry);
  }
  return [...groups.values()].filter((group) => group.entries.length > 0);
}

const ID = "compatibilityFindingsDialog";

/** Builds the dialog's skeleton once. Text is filled at open. */
function buildDialog(view) {
  const make = (tag, props = {}) => Object.assign(view.createElement(tag), props);
  const overlay = make("div", { id: ID, className: "dialog-overlay", hidden: true });
  overlay.setAttribute("role", "dialog");
  overlay.setAttribute("aria-modal", "true");
  overlay.setAttribute("aria-labelledby", `${ID}Title`);
  overlay.setAttribute("aria-describedby", `${ID}Description`);
  const card = make("section", { className: "dialog-card findings-dialog" });
  const head = make("header", { className: "dialog-head" });
  const heading = make("div", { className: "dialog-heading" });
  const words = make("div");
  words.append(make("h2", { id: `${ID}Title` }), make("p", { id: `${ID}Description` }));
  heading.append(words);
  const close = make("button", { id: `${ID}Close`, type: "button", className: "dialog-close" });
  close.append(make("span", { className: "ms", textContent: "close" }));
  close.firstChild.setAttribute("aria-hidden", "true");
  head.append(heading, close);
  const body = make("div", { id: `${ID}Body`, className: "dialog-body findings-body" });
  const foot = make("footer", { className: "dialog-foot" });
  const actions = make("div", { className: "dialog-actions" });
  const done = make("button", { id: `${ID}Done`, type: "button", className: "dialog-button dialog-button-primary" });
  actions.append(done);
  foot.append(actions);
  card.append(head, body, foot);
  overlay.append(card);
  view.body.append(overlay);
  return { overlay, close, body, done };
}

/**
 * @param {object} io
 * @param {HTMLElement|null} io.chip `#compatibilityStatus`, a button.
 * @param {Function} io.registerModal the modal contract.
 * @param {() => HTMLElement} io.fallbackFocus where focus goes if the chip is gone.
 * @param {Document} [io.view]
 */
export function createCompatibilityFindings(io) {
  const view = io.view ?? io.chip?.ownerDocument ?? globalThis.document;
  const parts = buildDialog(view);
  const modal = io.registerModal(parts.overlay, {
    initialFocus: () => parts.done,
    fallbackFocus: io.fallbackFocus,
  });
  /** What the chip counts right now. */
  let current = { reportJson: "", phase: "import", count: 0 };

  function render() {
    view.getElementById(`${ID}Title`).textContent = t("findings.title");
    view.getElementById(`${ID}Description`).textContent = t(
      current.phase === "export" ? "findings.exportIntro" : "findings.importIntro",
    );
    parts.close.setAttribute("aria-label", t("findings.closeLabel"));
    parts.close.title = t("findings.close");
    parts.done.textContent = t("findings.close");
    parts.body.replaceChildren();
    for (const group of groupFindings(current.reportJson)) {
      const section = view.createElement("section");
      section.className = "findings-group";
      section.dataset.kind = group.id;
      const title = view.createElement("h3");
      title.className = "findings-group-title";
      title.textContent = t(group.key);
      const total = view.createElement("span");
      total.className = "findings-total";
      total.textContent = n(group.total);
      title.append(" ", total);
      const list = view.createElement("ul");
      list.className = "findings-list";
      for (const entry of group.entries) {
        const row = view.createElement("li");
        row.className = "findings-row";
        const feature = view.createElement("code");
        feature.className = "findings-feature";
        feature.textContent = String(entry.feature ?? "");
        const count = view.createElement("span");
        count.className = "findings-count";
        count.textContent = "×" + n(Number(entry.occurrences) || 0);
        row.append(feature, count);
        const part = entry.location?.partName;
        if (part && part !== entry.feature) {
          const where = view.createElement("span");
          where.className = "findings-where";
          where.textContent = part;
          row.append(where);
        }
        list.append(row);
      }
      section.append(title, list);
      parts.body.append(section);
    }
  }

  function open() {
    if (current.count === 0) return;
    render();
    modal.open();
  }

  for (const button of [parts.close, parts.done]) button.addEventListener("click", () => modal.close());
  io.chip?.addEventListener("click", open);

  return {
    /** Records a report and paints the chip. Returns the occurrence count. */
    show(reportJson, phase) {
      const count = reportJson ? compatibilityOccurrenceCount(reportJson) : 0;
      current = { reportJson: reportJson ?? "", phase, count };
      showCompatibilityFindings(io.chip, count, phase);
      if (modal.isOpen) render();
      return count;
    },
    open,
    /** The palette / File-page row. Disabled WITH the reason when there is
     *  nothing to show, never removed: a reader who finds no row cannot tell
     *  "nothing was lost" from "this editor does not say". */
    commands: () => [
      {
        id: "file.compatibilityReport",
        label: t("findings.command"),
        group: "File",
        kw: "compatibility findings import export report loss lost fidelity check issues conversion",
        enabled: current.count > 0,
        disabledReason: t("findings.none"),
        run: open,
      },
    ],
  };
}
