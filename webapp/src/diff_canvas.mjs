// The diff canvas (ADR-065): what changed, shown ON THE PAGE where it changed.
//
// ## What the owner asked for, and what it replaces
//
// "A diff canvas for version diff, to see the changes on that version: what
// anyone has removed or added or changed in position, just like Google Docs."
// Google's version history paints a version's edits into the document in the
// editor's colour; Word's Compare produces a redlined document. Both are the
// SAME picture — the newer text with the older text struck back in where it
// was — and that picture is what this module puts on the canvas.
//
// It replaces two surfaces a reader could not use: version history showed an
// old version PLAIN, with its differences as a block-level text diff in a
// second panel on the other side of the page (ADR-064); and Review ▸ Compare
// wrote its findings into the reader's own document as tracked changes, refused
// any document that already had some, and listed a whole removed paragraph as
// "found, but not marked" because a tracked change cannot add a paragraph.
//
// ## The known pattern, named first (`SKILL` §8)
//
// A **redline** (Word's "compare → new document", Google's history view): a
// read-only third document derived from two inputs, painted with the review
// layer's own marks. Nothing here paints; the engine builds the redline
// (`WasmDocument.showComparison`) and the canvas renders it with markup on,
// exactly as it renders any document with tracked changes. So the author
// colour, the underline and strike, and the moves' double lines are review's
// own, and one fix to how a tracked change looks fixes this too.
//
// ## What this module owns
//
//   * `redlineEntries` — the redline's changes as a reader steps through them:
//     one entry per change, in document order, a replacement as ONE entry.
//   * `createChangeNavigator` — "3 of 12", previous/next, and a legend that
//     says what each mark means and how many there are.
//
// The redline itself is built by `compare_documents.mjs`'s `buildRedline`, beside
// the slice driver it runs on. Callers own WHERE it goes on screen and what the
// bar's actions are:
// version history (`version_panel.mjs`) and Review ▸ Compare
// (`compare_documents.mjs`) each put it on the canvas through `main.js`'s one
// preview seam.

import { n, t } from "./i18n.mjs";
import { reviewAuthorColor, reviewAuthorKey } from "./review_labels.mjs";

/** The kinds a reader is shown. A move is ONE kind with two ends: the reader's
 *  question is "what moved", and "moved from here" / "moved to here" is which
 *  end they are looking at, said on the entry rather than in the legend. */
export const ADDED = "added";
export const REMOVED = "removed";
export const MOVED = "moved";
export const FORMATTED = "formatted";

/** Engine revision kind → the kind a reader is shown. */
const KIND_OF_REVISION = Object.freeze({
  insertion: ADDED,
  deletion: REMOVED,
  move_from: MOVED,
  move_to: MOVED,
});

/** The sidecar families whose changes have no mark of their own on the page,
 *  but DO sit on a paragraph a reader can be taken to. */
const ANCHORED_WITHOUT_MARK = new Set(["formatting", "style", "table", "object"]);

/** Catalogue keys, written out so the extractor finds them (`t("literal")`). */
const LEGEND_LABEL = Object.freeze({
  [ADDED]: () => t("compare.kind.insertion"),
  [REMOVED]: () => t("compare.kind.deletion"),
  [MOVED]: () => t("diffCanvas.moved"),
  [FORMATTED]: () => t("compare.kind.formatting"),
});

/** The label ON an entry: a move says which end it is. */
export function entryLabel(entry) {
  if (entry.replaced) return t("diffCanvas.replaced");
  if (entry.kind === MOVED) {
    return entry.end === "from" ? t("compare.kind.move_from") : t("compare.kind.move_to");
  }
  return LEGEND_LABEL[entry.kind]?.() ?? "";
}

/**
 * The redline's changes, as a reader steps through them.
 *
 *   * Only THIS comparison's marks — the ones stamped with its author and date.
 *     A version that carries its own suggestions shows them too (they are part
 *     of it), but they are not "what changed in this version".
 *   * A replacement — the engine groups a deletion and the insertion that
 *     replaced it — is ONE entry, because it is one edit.
 *   * A formatting, style, table or object change has no mark, and still sits
 *     on a paragraph; it is an entry that takes the reader there.
 *   * In document order, from the engine's own `order` (O(changes) to apply,
 *     no document walk here).
 *
 * Pure: no DOM, no engine. O(revisions + changes log changes).
 *
 * @param {object[]} revisions `listRevisions()` of the redline.
 * @param {object} diff the parsed sidecar.
 * @param {object} summary `showComparison`'s summary.
 * @param {{author: string, date: string}} stamp
 */
export function redlineEntries(revisions, diff, summary, stamp) {
  const entries = [];
  const groups = new Map();
  for (const revision of Array.isArray(revisions) ? revisions : []) {
    if (revision?.author !== stamp.author || revision?.date !== stamp.date) continue;
    const kind = KIND_OF_REVISION[revision.kind];
    if (!kind || !revision.anchor?.node) continue;
    const key = revision.groupId ? `group:${revision.groupId}` : `revision:${revision.id}`;
    const grouped = groups.get(key);
    if (grouped) {
      // The second half of a replacement: one entry, reading old → new.
      if (kind === REMOVED) grouped.removed = revision.text ?? "";
      else grouped.text = revision.text ?? "";
      grouped.replaced = Boolean(grouped.removed && grouped.text);
      continue;
    }
    const entry = {
      id: revision.id,
      kind,
      end: revision.kind === "move_from" ? "from" : revision.kind === "move_to" ? "to" : "",
      text: kind === REMOVED ? "" : revision.text ?? "",
      removed: kind === REMOVED ? revision.text ?? "" : "",
      replaced: false,
      anchor: { node: revision.anchor.node, start: revision.anchor.start, end: revision.anchor.end },
    };
    groups.set(key, entry);
    entries.push(entry);
  }
  const anchors = summary?.anchors ?? {};
  for (const change of Array.isArray(diff?.changes) ? diff.changes : []) {
    if (!ANCHORED_WITHOUT_MARK.has(change.family)) continue;
    if (change.kind === "insertion" || change.kind === "deletion") continue;
    const node = anchors[change.id];
    if (!node) continue;
    entries.push({
      id: change.id,
      kind: FORMATTED,
      end: "",
      text: change.rightText ?? "",
      removed: "",
      replaced: false,
      fields: Array.isArray(change.fields) ? change.fields : [],
      anchor: { node, start: 0, end: 0 },
    });
  }
  const position = new Map((summary?.order ?? []).map((id, index) => [id, index]));
  const rank = (entry) => position.get(entry.anchor.node) ?? Number.MAX_SAFE_INTEGER;
  // Stable: entries on one paragraph keep the order they were listed in.
  return entries
    .map((entry, index) => ({ entry, index }))
    .sort(
      (a, b) =>
        rank(a.entry) - rank(b.entry) ||
        (Number(a.entry.anchor.start) || 0) - (Number(b.entry.anchor.start) || 0) ||
        a.index - b.index,
    )
    .map(({ entry }) => entry);
}

/** How many entries of each kind, in the legend's order. Pure. */
export function legendCounts(entries) {
  const counts = { [ADDED]: 0, [REMOVED]: 0, [MOVED]: 0, [FORMATTED]: 0 };
  for (const entry of entries) {
    if (entry.replaced) {
      counts[ADDED] += 1;
      counts[REMOVED] += 1;
    } else if (entry.kind in counts) {
      counts[entry.kind] += 1;
    }
  }
  return counts;
}

/** The one-line text an entry shows: what was added, removed, or moved,
 *  trimmed to a readable length. */
export function entryText(entry, limit = 80) {
  const clip = (text) => {
    const flat = String(text ?? "").replace(/\s+/g, " ").trim();
    return flat.length > limit ? `${flat.slice(0, limit - 1)}…` : flat;
  };
  if (entry.replaced) return `${clip(entry.removed)} → ${clip(entry.text)}`;
  return clip(entry.kind === REMOVED ? entry.removed : entry.text);
}

/**
 * "3 of 12", previous/next, and the legend — rendered into `root`.
 *
 * Keyboard: the two buttons are ordinary buttons, and `root` handles
 * Alt+ArrowDown / Alt+ArrowUp (Google's "next/previous edit" chord family) so
 * the reader can step without leaving the keyboard.
 *
 * @param {HTMLElement} root
 * @param {{navigate: (entry: object) => void}} io
 */
export function createChangeNavigator(root, io) {
  let entries = [];
  let current = -1;
  let author = "";

  const previous = button("diffCanvas.previous", "keyboard_arrow_up", () => step(-1));
  const next = button("diffCanvas.next", "keyboard_arrow_down", () => step(1));
  const position = document.createElement("span");
  position.className = "diff-nav-position";
  position.setAttribute("aria-live", "polite");
  const legend = document.createElement("span");
  legend.className = "diff-legend";
  const by = document.createElement("span");
  by.className = "diff-nav-author";

  function button(key, icon, run) {
    const element = document.createElement("button");
    element.type = "button";
    element.className = "diff-nav-step";
    element.title = t(key);
    element.setAttribute("aria-label", t(key));
    const glyph = document.createElement("span");
    glyph.className = "ms";
    glyph.setAttribute("aria-hidden", "true");
    glyph.textContent = icon;
    element.append(glyph);
    element.addEventListener("click", run);
    return element;
  }

  function step(delta) {
    if (entries.length === 0) return;
    current = current < 0 ? (delta > 0 ? 0 : entries.length - 1) : (current + delta + entries.length) % entries.length;
    reflect();
    io.navigate(entries[current]);
  }

  function reflect() {
    const total = entries.length;
    previous.disabled = total === 0;
    next.disabled = total === 0;
    position.textContent =
      total === 0
        ? t("compare.identical")
        : current < 0
          ? t("compare.changeCount", { count: n(total) })
          : `${t("diffCanvas.position", { index: n(current + 1), count: n(total) })} · ${entryLabel(entries[current])}`;
  }

  function renderLegend() {
    const counts = legendCounts(entries);
    const color = reviewAuthorColor(reviewAuthorKey({ author }));
    legend.replaceChildren();
    for (const kind of [ADDED, REMOVED, MOVED, FORMATTED]) {
      if (counts[kind] === 0) continue;
      const chip = document.createElement("span");
      chip.className = `diff-legend-item diff-legend-${kind}`;
      chip.dataset.diffKind = kind;
      const sample = document.createElement("span");
      sample.className = "diff-legend-sample";
      sample.style.setProperty("--diff-author-color", color);
      sample.textContent = LEGEND_LABEL[kind]();
      const count = document.createElement("span");
      count.className = "diff-legend-count";
      count.textContent = n(counts[kind]);
      chip.append(sample, count);
      legend.append(chip);
    }
    by.replaceChildren();
    if (author) {
      const swatch = document.createElement("span");
      swatch.className = "diff-author-swatch";
      swatch.style.setProperty("--diff-author-color", color);
      swatch.setAttribute("aria-hidden", "true");
      const name = document.createElement("span");
      name.textContent = t("diffCanvas.changesBy", { name: author });
      by.append(swatch, name);
    }
  }

  // Three groups that wrap as groups: who, where ("‹ 3 of 12 ›"), and the key.
  const steps = document.createElement("span");
  steps.className = "diff-nav-steps";
  steps.append(previous, position, next);
  root.classList.add("diff-nav");
  root.replaceChildren(by, steps, legend);
  root.addEventListener("keydown", (event) => {
    if (!event.altKey || (event.key !== "ArrowDown" && event.key !== "ArrowUp")) return;
    event.preventDefault();
    step(event.key === "ArrowDown" ? 1 : -1);
  });

  return {
    /** Shows a new set of changes; nothing is selected until the reader steps. */
    show(next, options = {}) {
      entries = Array.isArray(next) ? next : [];
      author = String(options.author ?? "");
      current = -1;
      root.hidden = false;
      renderLegend();
      reflect();
    },
    /** Selects one entry (a list row was clicked) without navigating again. */
    select(entry) {
      current = entries.indexOf(entry);
      reflect();
    },
    step,
    clear() {
      entries = [];
      current = -1;
      root.hidden = true;
    },
    get entries() {
      return entries;
    },
  };
}
