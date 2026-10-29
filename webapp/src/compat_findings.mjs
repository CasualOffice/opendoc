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
// Each row says what the finding IS in words a reader shares, with the
// engine's stable id kept beside it, muted, so support can still match it
// (`109` FID-AT-05; the words and the fallback are `findings_catalogue.mjs`).
//
// WORD'S OWN BOOKKEEPING is not content. `sample.docx` opened as "Kept in the
// file, not shown or editable here 183", and 165 of the 183 were revision-save
// ids — a number that told the reader something alarming about nothing they
// would miss. Entries the catalogue DECLARES bookkeeping are therefore listed in
// a collapsed disclosure inside their own kind group — still under the heading
// that says what happened to them, so a dropped thumbnail reads as dropped — and
// are kept out of the headline counts here: the group's total and the chip's.
// (`main.js`'s Save status line still counts them; `show()` returns the headline
// for it to use — `109` FID-FW-01.) They are never hidden: the disclosure is a
// real button with `aria-expanded`, and opening it lists every entry with its
// count.
//
// Complexity: parsing is O(entries) once per open or save; rendering is
// O(entries log entries) per opening (rows are sorted by their words). The
// report is aggregated per feature by the engine, so entries are tens, not one
// per occurrence.
import { activeLocale, n, t } from "./i18n.mjs";
import { describeFinding, isBookkeeping } from "./findings_catalogue.mjs";
import { showCompatibilityFindings } from "./save_formats.mjs";

/** The groups, most serious first, and the catalogue key each is named by. */
export const FINDING_KINDS = Object.freeze([
  Object.freeze({ id: "lost", key: "findings.kind.lost" }),
  Object.freeze({ id: "refused", key: "findings.kind.refused" }),
  // A PDF's reconstruction (`docs/182` P2): what it says about each construct is
  // what the IMPORT did to it, because the original file is never written over
  // and a save to PDF before an edit is that file. So these three are named by
  // the model outcome, not by retention, and only for `pdf.import.*` ids.
  Object.freeze({ id: "pdfOmitted", key: "findings.kind.pdfOmitted" }),
  Object.freeze({ id: "pdfDegraded", key: "findings.kind.pdfDegraded" }),
  Object.freeze({ id: "pdfInferred", key: "findings.kind.pdfInferred" }),
  Object.freeze({ id: "approximated", key: "findings.kind.approximated" }),
  Object.freeze({ id: "preserved", key: "findings.kind.preserved" }),
  Object.freeze({ id: "other", key: "findings.kind.unsorted" }),
]);

/** Which group one report entry belongs to. Retention first: a construct that
 *  was approximated AND not kept is, to a reader, not kept. */
export function findingKind(entry) {
  if (String(entry?.feature ?? "").startsWith("pdf.import.")) {
    if (entry?.modelOutcome === "omitted") return "pdfOmitted";
    if (entry?.modelOutcome === "degraded") return "pdfDegraded";
    if (entry?.modelOutcome === "mapped") return "pdfInferred";
  }
  const retention = entry?.retentionOutcome;
  if (retention === "not_retained") return "lost";
  if (retention === "blocked" || retention === "rejected") return "refused";
  // Both remaining groups promise "the original is kept", so both require it.
  if (retention === "preserved" && entry?.modelOutcome === "degraded") return "approximated";
  if (retention === "preserved" && entry?.modelOutcome === "omitted") return "preserved";
  return "other";
}

/** One entry's occurrence count, or 0 for a count that is not a positive safe
 *  integer — the same rule `format_io.mjs`'s `compatibilityOccurrenceCount`
 *  applies, so the chip and the groups can never disagree about one entry. */
function occurrencesOf(entry) {
  return Number.isSafeInteger(entry?.occurrences) && entry.occurrences > 0 ? entry.occurrences : 0;
}

/** The entries of a report, or none. An empty string is "no report", which is
 *  not an error. A report that is not JSON throws; one with no `entries` array
 *  throws when `strict` — the chip's count, where "0 findings" for a malformed
 *  report would claim a clean document (`format_io.mjs`'s
 *  `importFindingCount`) — and groups as nothing otherwise. */
function reportEntries(reportJson, strict = false) {
  if (!reportJson) return [];
  const report = JSON.parse(reportJson);
  if (Array.isArray(report?.entries)) return report.entries;
  if (strict) throw new Error("compatibility report has no entries array");
  return [];
}

/**
 * A report's entries grouped by kind, in `FINDING_KINDS` order, empty groups
 * left out.
 *
 * `total` and `entries` are the CONTENT findings — what the group's headline
 * counts. `bookkeeping` holds the entries the catalogue declares Word's own
 * bookkeeping, with their own total, so a group is kept when it holds only
 * bookkeeping: what happened to those entries is still said by the heading
 * they sit under.
 *
 * @param {string} reportJson the engine's `{entries: [...]}`; empty means none.
 * @returns {Array<{id: string, key: string, total: number, entries: object[],
 *   bookkeeping: {total: number, entries: object[]}}>}
 */
export function groupFindings(reportJson) {
  const groups = new Map(
    FINDING_KINDS.map((kind) => [
      kind.id,
      { ...kind, total: 0, entries: [], bookkeeping: { total: 0, entries: [] } },
    ]),
  );
  for (const entry of reportEntries(reportJson)) {
    const group = groups.get(findingKind(entry));
    const bucket = isBookkeeping(entry) ? group.bookkeeping : group;
    bucket.total += occurrencesOf(entry);
    bucket.entries.push(entry);
  }
  return [...groups.values()].filter(
    (group) => group.entries.length > 0 || group.bookkeeping.entries.length > 0,
  );
}

/**
 * What the chip and the commands count.
 *
 * `headline` is the occurrences a reader would miss — everything but Word's own
 * bookkeeping — and is what the chip prints. `entries` counts every entry,
 * bookkeeping included, and decides whether there is anything to open: a
 * document whose only findings are bookkeeping shows no chip, and its report is
 * still one command away rather than unreachable.
 *
 * @param {string} reportJson
 * @returns {{headline: number, bookkeeping: number, entries: number}}
 */
export function findingTotals(reportJson) {
  const totals = { headline: 0, bookkeeping: 0, entries: 0 };
  for (const entry of reportEntries(reportJson, true)) {
    totals.entries += 1;
    if (isBookkeeping(entry)) totals.bookkeeping += occurrencesOf(entry);
    else totals.headline += occurrencesOf(entry);
  }
  return totals;
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
  // The PDF commands, as buttons, when the dialog summarises a PDF
  // (`pdf_commands.mjs`): filled at render, empty otherwise.
  const extra = make("div", { id: `${ID}Actions`, className: "findings-actions" });
  const done = make("button", { id: `${ID}Done`, type: "button", className: "dialog-button dialog-button-primary" });
  actions.append(extra, done);
  foot.append(actions);
  card.append(head, body, foot);
  overlay.append(card);
  view.body.append(overlay);
  return { overlay, close, body, done, extra };
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
  let current = {
    reportJson: "",
    phase: "import",
    origin: null,
    layout: null,
    ocr: null,
    actions: null,
    totals: { headline: 0, bookkeeping: 0, entries: 0 },
  };
  /** Whether the dialog has the PDF disclosure to say, findings or none: a
   *  PDF opened for editing, or a flowing document converted from one. */
  const disclosing = () =>
    (current.origin === "pdf" || current.origin === "pdfReflow") && current.phase === "import";
  /** Which kind groups' bookkeeping is open. Reset at every open, kept across a
   *  re-render while open, so a save under an open dialog does not fold what
   *  the reader just unfolded. */
  const expanded = new Set();

  const make = (tag, className) => {
    const element = view.createElement(tag);
    if (className) element.className = className;
    return element;
  };

  /** One finding: its words first, its count, where it is, and the engine's id
   *  last and muted — the thing support matches on, read last by a screen
   *  reader too, after the words. Its `title` says what it is. Not a
   *  visually-hidden label: the modal roster's clipping guard
   *  (`dialog-contract.spec.mjs`) rightly measures a 1px box holding 55px of
   *  text as clipped, and exempting it would blind the guard to real clipping. */
  function findingRow(described, entry) {
    const row = make("li", "findings-row");
    row.dataset.feature = described.feature;
    if (!described.known) row.dataset.uncatalogued = "true";
    const name = make("span", "findings-name");
    name.textContent = described.text;
    const count = make("span", "findings-count");
    count.textContent = "×" + n(Number(entry.occurrences) || 0);
    row.append(name, count);
    if (described.where) {
      const where = make("span", "findings-where");
      where.textContent = t(described.where.key, described.where.params);
      row.append(where);
    }
    const feature = make("code", "findings-feature");
    feature.textContent = described.feature;
    feature.title = t("findings.featureId");
    row.append(feature);
    return row;
  }

  /** The rows of a list, ordered by what a reader reads, not by the engine's
   *  id — which is ASCII order and so looks shuffled once it is in words. */
  function findingList(entries) {
    const collator = new Intl.Collator(activeLocale());
    const rows = entries
      .map((entry) => {
        const described = describeFinding(entry);
        return { entry, described: { ...described, text: t(described.words) } };
      })
      .sort(
        (a, b) =>
          collator.compare(a.described.text, b.described.text) ||
          (a.described.feature < b.described.feature ? -1 : 1),
      );
    const list = make("ul", "findings-list");
    for (const { entry, described } of rows) list.append(findingRow(described, entry));
    return list;
  }

  /** Word's own bookkeeping for one kind group: a real disclosure, collapsed
   *  until asked, naming its count on the button so a reader knows what is
   *  behind it before opening it. */
  function bookkeepingDisclosure(group) {
    const box = make("div", "findings-bookkeeping");
    const panelId = `${ID}Bookkeeping-${group.id}`;
    const toggle = make("button", "findings-disclosure");
    toggle.type = "button";
    toggle.setAttribute("aria-controls", panelId);
    const glyph = make("span", "ms");
    glyph.setAttribute("aria-hidden", "true");
    const label = make("span", "findings-disclosure-label");
    label.textContent = t("findings.bookkeeping.label");
    const total = make("span", "findings-bookkeeping-total");
    total.textContent = n(group.bookkeeping.total);
    toggle.append(glyph, label, " ", total);
    const panel = make("div", "findings-bookkeeping-panel");
    panel.id = panelId;
    const note = make("p", "findings-note");
    note.textContent = t("findings.bookkeeping.note");
    panel.append(note, findingList(group.bookkeeping.entries));
    const paint = () => {
      const open = expanded.has(group.id);
      toggle.setAttribute("aria-expanded", String(open));
      glyph.textContent = open ? "expand_more" : "chevron_right";
      panel.hidden = !open;
    };
    toggle.addEventListener("click", () => {
      if (expanded.has(group.id)) expanded.delete(group.id);
      else expanded.add(group.id);
      paint();
    });
    paint();
    box.append(toggle, panel);
    return box;
  }

  /** What a PDF-origin document IS, said before its findings (`docs/182`,
   *  `docs/131` §16.2): processed here, a reconstruction, the original kept. */
  function pdfDisclosure() {
    const reflowed = current.origin === "pdfReflow";
    // The default open (`docs/182` §4): the PDF rebuilt as a flowing document,
    // its exact layout one command away.
    const flowing = !reflowed && current.layout === "flowing";
    const section = make("section", "findings-group findings-pdf");
    section.dataset.kind = reflowed ? "pdfReflowDisclosure" : flowing ? "pdfFlowingDisclosure" : "pdfDisclosure";
    const title = make("h3", "findings-group-title");
    title.textContent = t(reflowed || flowing ? "findings.pdfReflow.title" : "findings.pdf.title");
    const list = make("ul", "findings-pdf-facts");
    const facts = reflowed
      ? ["findings.pdfReflow.new", "findings.pdfReflow.inferred", "findings.pdfReflow.source", "findings.pdf.local"]
      : flowing
        ? ["findings.pdf.local", "findings.pdfReflow.inferred", "findings.pdfFlowing.keepLayout", "findings.pdfFlowing.save"]
        : ["findings.pdf.local", "findings.pdf.reconstruction", "findings.pdf.original"];
    for (const key of facts) {
      const item = make("li");
      item.dataset.fact = key.slice(key.lastIndexOf(".") + 1);
      item.textContent = t(key);
      list.append(item);
    }
    section.append(title, list);
    if (current.totals.entries === 0) {
      const none = make("p", "findings-note");
      none.textContent = t("findings.pdf.none");
      section.append(none);
    }
    return section;
  }

  /** What OCR did to a PDF, or could do: said whenever a page is a scan, so
   *  "the text of page 3 is only a picture" is never left for the reader to
   *  discover (`docs/182` §20). */
  function ocrSection() {
    const ocr = current.ocr;
    const scanned = (ocr?.rasterPages ?? 0) + (ocr?.hybridPages ?? 0);
    if (current.origin !== "pdf" || !ocr || scanned === 0) return null;
    const section = make("section", "findings-group findings-ocr");
    section.dataset.kind = "pdfOcr";
    const title = make("h3", "findings-group-title");
    title.textContent = t("findings.pdf.ocrTitle");
    const note = make("p", "findings-note");
    if (ocr.provider) {
      note.dataset.state = "applied";
      note.textContent = t("findings.pdf.ocrApplied", { provider: ocr.provider });
    } else {
      note.dataset.state = "pending";
      note.textContent = t("findings.pdf.ocrPending", { count: n(scanned) });
      const offer = (current.actions?.() ?? []).find((action) => action.id === "file.pdf.recogniseText");
      if (offer && !offer.enabled && offer.reason) {
        const why = make("p", "findings-note findings-ocr-reason");
        why.dataset.state = "unavailable";
        why.textContent = offer.reason;
        section.append(title, note, why);
        return section;
      }
    }
    section.append(title, note);
    return section;
  }

  /** The PDF commands as buttons beside Done. A disabled one keeps its
   *  reason as its title; running one closes this dialog first. */
  function renderActions() {
    parts.extra.replaceChildren();
    if (!disclosing()) return;
    for (const action of current.actions?.() ?? []) {
      const button = make("button", "dialog-button");
      button.type = "button";
      button.dataset.action = action.id;
      button.textContent = action.label;
      button.disabled = !action.enabled;
      if (!action.enabled && action.reason) button.title = action.reason;
      button.addEventListener("click", () => {
        modal.close();
        action.run();
      });
      parts.extra.append(button);
    }
  }

  function render() {
    view.getElementById(`${ID}Title`).textContent = t("findings.title");
    view.getElementById(`${ID}Description`).textContent = t(
      current.phase === "export" ? "findings.exportIntro" : "findings.importIntro",
    );
    parts.close.setAttribute("aria-label", t("findings.closeLabel"));
    parts.close.title = t("findings.close");
    parts.done.textContent = t("findings.close");
    parts.body.replaceChildren();
    if (disclosing()) parts.body.append(pdfDisclosure());
    const ocr = disclosing() ? ocrSection() : null;
    if (ocr) parts.body.append(ocr);
    renderActions();
    if (current.totals.headline === 0 && current.totals.bookkeeping > 0) {
      const only = make("p", "findings-note findings-only-bookkeeping");
      only.textContent = t("findings.bookkeeping.only");
      parts.body.append(only);
    }
    for (const group of groupFindings(current.reportJson)) {
      const section = make("section", "findings-group");
      section.dataset.kind = group.id;
      const title = make("h3", "findings-group-title");
      title.textContent = t(group.key);
      // The headline counts what a reader would miss. A group holding only
      // bookkeeping states no number here; its count is on the disclosure.
      if (group.entries.length > 0) {
        const total = make("span", "findings-total");
        total.textContent = n(group.total);
        title.append(" ", total);
      }
      section.append(title);
      if (group.entries.length > 0) section.append(findingList(group.entries));
      if (group.bookkeeping.entries.length > 0) section.append(bookkeepingDisclosure(group));
      parts.body.append(section);
    }
  }

  function open() {
    if (current.totals.entries === 0 && !disclosing()) return;
    expanded.clear();
    render();
    modal.open();
  }

  for (const button of [parts.close, parts.done]) button.addEventListener("click", () => modal.close());
  io.chip?.addEventListener("click", open);

  return {
    /** Records a report and paints the chip. Returns the HEADLINE count — the
     *  occurrences a reader would miss, Word's own bookkeeping excluded — which
     *  is the number the chip prints and the one a status line should say. */
    show(reportJson, phase, { origin = null, layout = null, ocr = null, actions = null } = {}) {
      const totals = reportJson ? findingTotals(reportJson) : { headline: 0, bookkeeping: 0, entries: 0 };
      // An export report keeps the open document's origin and actions: the
      // document is still what it was opened as.
      const keep = phase === "export" ? current : { origin, layout, ocr, actions };
      current = {
        reportJson: reportJson ?? "",
        phase,
        origin: keep.origin,
        layout: keep.layout,
        ocr: keep.ocr,
        actions: keep.actions,
        totals,
      };
      showCompatibilityFindings(io.chip, totals.headline, phase);
      if (modal.isOpen) render();
      return totals.headline;
    },
    open,
    /** The palette / File-page row. Disabled WITH the reason when there is
     *  nothing to show, never removed: a reader who finds no row cannot tell
     *  "nothing was lost" from "this editor does not say". Enabled by ANY
     *  entry, bookkeeping included — a report with no chip is still reachable. */
    commands: () => [
      {
        id: "file.compatibilityReport",
        label: t("findings.command"),
        group: "File",
        kw: "compatibility findings import export report loss lost fidelity check issues conversion",
        enabled: current.totals.entries > 0 || disclosing(),
        disabledReason: t("findings.none"),
        run: open,
      },
    ],
  };
}
