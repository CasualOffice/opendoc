// SPDX-License-Identifier: Apache-2.0
// Add-in proofing: an add-in's writing issues drawn IN the text, the way
// LanguageTool's browser add-on, Grammarly and Google Docs' own spelling and
// grammar show them (`docs/183` §19, RM-53).
//
// The add-in supplies matches; the HOST draws them, through the built-in spell
// checker's own window, scheduler and overlay (`spell_check.mjs` `setExternal`):
//
//   * The checker's debounced scan (400 ms after the last edit, 90 ms after a
//     scroll) hands this layer the page window's paragraphs it already read. Only
//     a paragraph whose text differs from what an add-in was last sent is sent
//     again (`proofing/check`), so an edit re-checks the paragraph it changed.
//   * The add-in answers with `proofing/report`. Every result names the text it
//     is about; a result about text the paragraph no longer holds is dropped, and
//     every match must quote that text exactly at its offsets (verified, never
//     believed). Answers are cached by text, so an undo brings its marks back at
//     once.
//   * Marks are painted with the SAME classes and the SAME batched
//     `decorationRectsOnPages` call as the built-in squiggles, and a span an
//     add-in marks is withheld from the built-in squiggle: one underline, never
//     two.
//   * A click on a mark, or ⌥⏎ / ⌘' / ⌘; from the keyboard, opens a small card at
//     the word in the existing inline-card component (`.review-inline-card`):
//     category, message, replacement chips, Ignore and, for spelling, Add to
//     dictionary.
//
// THE ADD-IN NEVER EDITS. A replacement is applied by the host, from a click or
// key press the host received in its own card (`event.isTrusted`), on a span the
// host re-reads from the document at that moment, through the one replacement
// path the built-in spell check uses (`replaceMisspelling` → `doc.replaceRanges`
// in `runEdit`): one undo step, tracked in Suggesting, refused in Viewing. There
// is no RPC an add-in can call to apply anything, and the chips are offered only
// to an add-in holding `suggest` (`docs/183` §19.3).
//
// Complexity: `window` is O(window paragraphs + marks in them); `paint` is one
// engine call for every mark on screen; `covers` is O(marks in one paragraph);
// the count and Next/Previous are O(marks held), bounded by `LIMITS`.
import { t } from "./i18n.mjs";
import { stringIndexToByteOffset } from "./text_rules.mjs";

/** How each category is drawn: an underline (the built-in squiggle's own
 *  classes) or a Hemingway-style highlight, and the paper token it is drawn in.
 *  Spelling red and grammar blue are the built-in checker's (and Google Docs')
 *  colours; style and the readability findings use the other paper tokens. */
export const CATEGORIES = Object.freeze({
  spelling: Object.freeze({ look: "underline", className: "spell-error", token: "--paper-spelling", dictionary: true }),
  grammar: Object.freeze({ look: "underline", className: "grammar-error", token: "--paper-grammar" }),
  punctuation: Object.freeze({ look: "underline", className: "grammar-error", token: "--paper-grammar" }),
  style: Object.freeze({ look: "underline", className: "grammar-error addin-style", token: "--paper-comment" }),
  long: Object.freeze({ look: "highlight", className: "addin-highlight addin-highlight-long", token: "--paper-comment" }),
  passive: Object.freeze({ look: "highlight", className: "addin-highlight addin-highlight-passive", token: "--paper-insert" }),
  filler: Object.freeze({ look: "highlight", className: "addin-highlight addin-highlight-filler", token: "--paper-grammar" }),
});

/** What one report may carry. Over a bound, the excess is dropped and counted. */
export const LIMITS = Object.freeze({
  paragraphsPerCheck: 60,
  charsPerCheck: 60_000,
  resultsPerReport: 200,
  matchesPerParagraph: 100,
  messageChars: 300,
  replacements: 5,
  replacementChars: 120,
  summaryChars: 160,
  cachedTexts: 600,
  marksHeld: 5000,
});

const SEP = "\u0000";
const clip = (value, max) => {
  const chars = [...String(value ?? "")];
  return chars.length <= max ? chars.join("") : `${chars.slice(0, max - 1).join("")}…`;
};

/**
 * Checks one `proofing/report` against the texts the host sent. Pure.
 *
 * @param {object} params `{results: [{node, text, matches: [{start, end, category,
 *   message, replacements?, rule?}]}], summary?}` — offsets are UTF-16 indices into
 *   `text`
 * @param {(node: string) => string | undefined} sentText the text this add-in was
 *   last sent for `node`
 * @returns {{results: {node: string, text: string, marks: object[]}[], dropped: number, summary: string}}
 */
export function checkReport(params, sentText) {
  const out = [];
  let dropped = 0;
  const results = Array.isArray(params?.results) ? params.results : [];
  for (const result of results.slice(0, LIMITS.resultsPerReport)) {
    const node = typeof result?.node === "string" ? result.node : "";
    const text = typeof result?.text === "string" ? result.text : null;
    // STALENESS: an answer about text this paragraph no longer holds.
    if (!node || text === null || sentText(node) !== text) {
      dropped += Array.isArray(result?.matches) ? result.matches.length : 1;
      continue;
    }
    const marks = [];
    const matches = Array.isArray(result.matches) ? result.matches : [];
    for (const match of matches.slice(0, LIMITS.matchesPerParagraph)) {
      const start = Number(match?.start);
      const end = Number(match?.end);
      const category = String(match?.category ?? "");
      if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end <= start || end > text.length || !Object.hasOwn(CATEGORIES, category)) {
        dropped += 1;
        continue;
      }
      const exact = text.slice(start, end);
      // A match whose quoted text is not what is there is not believed.
      if (typeof match.exact === "string" && match.exact !== exact) {
        dropped += 1;
        continue;
      }
      const replacements = (Array.isArray(match.replacements) ? match.replacements : [])
        .filter((r) => typeof r === "string" && r !== exact)
        .slice(0, LIMITS.replacements)
        .map((r) => clip(r, LIMITS.replacementChars));
      marks.push(Object.freeze({
        node,
        start,
        end,
        exact,
        category,
        message: clip(match.message, LIMITS.messageChars),
        rule: clip(match.rule ?? "", 80),
        replacements: Object.freeze([...new Set(replacements)]),
        byteStart: stringIndexToByteOffset(text, start),
        byteEnd: stringIndexToByteOffset(text, end),
      }));
    }
    dropped += Math.max(0, matches.length - LIMITS.matchesPerParagraph);
    out.push({ node, text, marks });
  }
  dropped += Math.max(0, results.length - LIMITS.resultsPerReport);
  return { results: out, dropped, summary: clip(params?.summary ?? "", LIMITS.summaryChars) };
}

/** Paragraphs grouped into checks within the bounds. Pure. */
export function batches(paragraphs) {
  const out = [];
  let current = [];
  let chars = 0;
  for (const paragraph of paragraphs) {
    const size = paragraph.text.length;
    if (current.length && (current.length >= LIMITS.paragraphsPerCheck || chars + size > LIMITS.charsPerCheck)) {
      out.push(current);
      current = [];
      chars = 0;
    }
    current.push(paragraph);
    chars += size;
  }
  if (current.length) out.push(current);
  return out;
}

/** The issue after (or before) a caret position, wrapping. `issues` are in
 *  document order; `at` is `{ordinal, offset}`. Pure. */
export function stepIssue(issues, at, direction = 1) {
  if (!issues.length) return null;
  const key = (issue) => [issue.ordinal, issue.mark.byteStart];
  const after = (a, b) => a[0] > b[0] || (a[0] === b[0] && a[1] > b[1]);
  const here = [at?.ordinal ?? -Infinity, at?.offset ?? -1];
  if (direction > 0) return issues.find((issue) => after(key(issue), here)) ?? issues[0];
  for (let i = issues.length - 1; i >= 0; i -= 1) if (after(here, key(issues[i]))) return issues[i];
  return issues.at(-1);
}

/**
 * The layer the spell checker paints through.
 *
 * @param {object} ed the editor context
 * @param {{view?: Window, announce?: (text: string) => void}} [io]
 */
export function createAddinProofing(ed, { view = globalThis, announce = (text) => ed.announceReview?.(text) } = {}) {
  const doc = view.document;
  /** id → {id, name, canReplace, send(paragraphs), sent: Map, marks: Map, cache: Map, summary} */
  const providers = new Map();
  /** node → {text, ordinal, typing} for paragraphs seen in a window. */
  const known = new Map();
  /** "Ignore": dies with the paragraph's text, like Word's Ignore Once. */
  const ignored = new Set();
  /** Fixes the person accepted, until the paragraph shows what became of them.
   *  A direct fix changes the text, so its mark goes with it; until the next scan
   *  reads the new text the mark is held back so it does not flash. A TRACKED fix
   *  leaves the paragraph's proofing text as it was (the deleted word is still
   *  there and a tracked insertion is not proofing text), so its mark is held back
   *  on that text. An undo back to the original text after the change was seen
   *  brings the mark back. */
  const accepted = [];
  let card = null;
  let cardMark = null;
  const listeners = new Set();

  const ignoreKey = (provider, mark, text) => [provider, mark.node, text, mark.start, mark.exact].join(SEP);
  const isIgnored = (provider, mark) => {
    const text = known.get(mark.node)?.text;
    return ignored.has(ignoreKey(provider, mark, text));
  };
  const words = () => new Set(ed.spellChecker?.personalWords?.() ?? []);

  /** Every live mark, in document order: what the count, the paint and
   *  Next/Previous all read. O(marks held). */
  function issues() {
    const personal = words();
    const out = [];
    for (const provider of providers.values()) {
      for (const [node, entry] of provider.marks) {
        const seen = known.get(node);
        if (!seen || seen.text !== entry.text) continue;
        for (const mark of entry.marks) {
          if (isIgnored(provider.id, mark)) continue;
          if (CATEGORIES[mark.category].dictionary && personal.has(mark.exact)) continue;
          out.push({ provider, mark, ordinal: seen.ordinal });
        }
      }
    }
    out.sort((a, b) => a.ordinal - b.ordinal || a.mark.byteStart - b.mark.byteStart);
    return out;
  }

  function changed() {
    if (ed.paintOverlayLayer) ed.paintOverlayLayer();
    else ed.drawSelection?.();
    for (const listener of listeners) listener();
  }

  function holdAnswer(provider, node, text, marks) {
    provider.marks.set(node, { text, marks });
    provider.cache.delete(text);
    provider.cache.set(text, marks);
    while (provider.cache.size > LIMITS.cachedTexts) provider.cache.delete(provider.cache.keys().next().value);
  }

  const layer = {
    active: () => providers.size > 0,

    /** The window the checker's scan read: `rows` in document order. */
    window(rows, pages, caret) {
      const base = (pages[0]?.pageNumber ?? 1) * 100_000;
      let repaint = false;
      rows.forEach((row, index) => {
        if (typeof row?.node !== "string") return;
        const text = String(row.text ?? "");
        const before = known.get(row.node);
        // Typing GROWS the paragraph at the caret; an undo or a replacement that puts
        // a word back under the caret is not typing, and its mark shows at once.
        const typing = !!before && (text === before.text ? before.typing : text.length > before.text.length) && caret?.node === row.node;
        known.set(row.node, { text, ordinal: base + index, typing, caretOffset: caret?.node === row.node ? Number(caret.offset) : null, language: String(row.language ?? "") });
        if (before?.text !== text) repaint = true;
      });
      // Accepted replacements meet the paragraph's next text.
      for (let i = accepted.length - 1; i >= 0; i -= 1) {
        const entry = accepted[i];
        const now = known.get(entry.mark.node)?.text;
        if (now === undefined) continue;
        if (now === entry.text) {
          // Back to the text the fix was made on, after a change: an undo.
          if (entry.changed) accepted.splice(i, 1);
        } else {
          entry.changed = true;
          if (now.slice(entry.mark.start, entry.mark.end) !== entry.mark.exact) accepted.splice(i, 1);
        }
      }
      while (accepted.length > 50) accepted.shift();
      for (const provider of providers.values()) {
        const ask = [];
        for (const row of rows) {
          if (typeof row?.node !== "string") continue;
          const text = String(row.text ?? "");
          if (provider.sent.get(row.node) === text) continue;
          provider.sent.set(row.node, text);
          const cached = provider.cache.get(text);
          if (cached) {
            holdAnswer(provider, row.node, text, cached.map((mark) => ({ ...mark, node: row.node })));
            repaint = true;
            continue;
          }
          provider.marks.delete(row.node);
          if (text.trim()) ask.push({ node: row.node, text, language: String(row.language ?? "") });
        }
        for (const batch of batches(ask)) provider.send(batch);
      }
      if (repaint) queueMicrotask(changed);
    },

    paint(engineDoc, pages) {
      const first = pages[0].pageNumber;
      const last = pages.at(-1).pageNumber;
      const marks = issues().filter(({ mark }) => {
        const seen = known.get(mark.node);
        // The word being typed is not marked until the caret leaves it.
        return !(seen?.typing && seen.caretOffset >= mark.byteStart && seen.caretOffset <= mark.byteEnd) && !hiddenByAccept(mark);
      });
      if (!marks.length) return;
      let flat = [];
      try {
        flat = engineDoc.decorationRectsOnPages(first, last, JSON.stringify(marks.map(({ mark }) => [mark.node, mark.byteStart, mark.byteEnd])));
      } catch {
        return;
      }
      let at = 0;
      for (const { provider, mark } of marks) {
        const count = flat[at] ?? 0;
        const rects = flat.slice(at + 1, at + 1 + count * 5);
        at += 1 + count * 5;
        for (let i = 0; i + 4 < rects.length; i += 5) {
          const el = ed.place(rects.slice(i, i + 5), CATEGORIES[mark.category].className);
          if (!el) continue;
          el.dataset.addinMark = mark.category;
          el.dataset.addin = provider.id;
          if (cardMark && cardMark.mark === mark) el.classList.add("addin-mark-current");
        }
      }
    },

    /** Whether an add-in's underline covers this built-in squiggle's span. */
    covers(node, byteStart, byteEnd) {
      for (const provider of providers.values()) {
        const entry = provider.marks.get(node);
        if (!entry || known.get(node)?.text !== entry.text) continue;
        for (const mark of entry.marks) {
          if (CATEGORIES[mark.category].look !== "underline") continue;
          if (mark.byteStart < byteEnd && byteStart < mark.byteEnd) return true;
        }
      }
      return false;
    },

    reset() {
      known.clear();
      ignored.clear();
      accepted.length = 0;
      closeCard();
      for (const provider of providers.values()) {
        provider.sent.clear();
        provider.marks.clear();
        provider.summary = "";
      }
      for (const listener of listeners) listener();
    },
  };

  function hiddenByAccept(mark) {
    const now = known.get(mark.node)?.text;
    return accepted.some((entry) => {
      if (entry.mark.node !== mark.node || entry.mark.start !== mark.start || entry.mark.exact !== mark.exact) return false;
      if (now !== entry.text) return true;
      return entry.tracked || (!entry.changed && Date.now() - entry.at < 2000);
    });
  }

  // ---- The card ---------------------------------------------------------------

  function closeCard({ refocus = false } = {}) {
    if (!card) return;
    card.remove();
    card = null;
    cardMark = null;
    ed.drawSelection?.();
    if (refocus) ed.focusEditorSurface?.();
  }

  function markAtCaret() {
    const caret = ed.selection?.focus;
    if (!caret || ed.hasRange?.()) return issueAt(caret);
    // A selection that is exactly one mark (Next/Previous selects it) is "at" it.
    const { anchor, focus } = ed.selection;
    if (anchor.node !== focus.node) return null;
    return issueAt({ node: focus.node, offset: Math.min(anchor.offset, focus.offset) });
  }

  function issueAt(caret) {
    if (!caret?.node) return null;
    const offset = Number(caret.offset) || 0;
    return issues().find(({ mark }) => mark.node === caret.node && offset >= mark.byteStart && offset <= mark.byteEnd && !hiddenByAccept(mark)) ?? null;
  }

  function apply(issue, replacement, event) {
    // Only a person's own click or key press in the host's card applies a fix.
    if (!event?.isTrusted) return;
    const { mark } = issue;
    const engineDoc = ed.doc;
    if (!engineDoc || !issue.provider.canReplace) return;
    // Re-read the span: the fix applies only to the text the card was about.
    let now = "";
    try {
      now = engineDoc.copyText(mark.node, mark.byteStart, mark.node, mark.byteEnd);
    } catch {
      now = "";
    }
    if (now !== mark.exact) {
      closeCard({ refocus: true });
      announce(t("proofing.addin.stale"));
      return;
    }
    accepted.push({ provider: issue.provider.id, mark, text: known.get(mark.node)?.text ?? "", tracked: ed.reviewMode === "suggesting", changed: false, at: Date.now() });
    // Read the paragraph again as soon as the edit has landed, rather than after
    // the typing quiesce, so the count and the marks follow the fix at once.
    view.setTimeout?.(() => ed.spellChecker?.refresh?.(), 60);
    view.setTimeout?.(changed, 2100);
    closeCard({ refocus: true });
    ed.replaceMisspelling({ node: mark.node, start: mark.byteStart, end: mark.byteEnd }, replacement);
    announce(replacement ? t("proofing.addin.applied", { text: replacement }) : t("proofing.addin.removed", { text: mark.exact }));
    for (const listener of listeners) listener();
  }

  function ignore(issue) {
    ignored.add(ignoreKey(issue.provider.id, issue.mark, known.get(issue.mark.node)?.text));
    closeCard({ refocus: true });
    announce(t("proofing.addin.ignored"));
    changed();
  }

  async function addToDictionary(issue) {
    closeCard({ refocus: true });
    await ed.addWordToDictionary?.(issue.mark.exact);
    changed();
  }

  function chip(label, { icon = "", aria = label, onClick, danger = false, disabledReason = "" }) {
    const button = doc.createElement("button");
    button.type = "button";
    if (danger) button.className = "danger";
    if (icon) {
      const glyph = doc.createElement("span");
      glyph.className = "ms";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = icon;
      button.append(glyph);
    }
    button.append(doc.createTextNode(label));
    button.setAttribute("aria-label", aria);
    button.title = disabledReason || aria;
    if (disabledReason) button.disabled = true;
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      onClick(event);
    });
    return button;
  }

  /** Opens the card for `issue`, below the word. `focus` for the keyboard. */
  function openCard(issue, { focus = false } = {}) {
    if (!issue) return false;
    closeCard();
    const { mark, provider } = issue;
    const look = CATEGORIES[mark.category];
    cardMark = issue;
    const el = doc.createElement("div");
    el.className = "review-inline-card addin-card";
    el.setAttribute("role", "dialog");
    el.setAttribute("aria-label", t("proofing.addin.card", { category: t(`proofing.addin.category.${mark.category}`) }));
    el.tabIndex = -1;
    el.dataset.addin = provider.id;
    el.dataset.category = mark.category;

    const head = doc.createElement("div");
    head.className = "review-inline-head";
    const dot = doc.createElement("span");
    dot.className = "review-inline-dot";
    dot.style.background = `var(${look.token})`;
    const who = doc.createElement("div");
    who.className = "review-inline-who";
    const category = doc.createElement("strong");
    category.textContent = t(`proofing.addin.category.${mark.category}`);
    const by = doc.createElement("small");
    by.textContent = provider.name;
    who.append(category, by);
    head.append(dot, who);

    const body = doc.createElement("p");
    body.className = "review-inline-body";
    body.textContent = mark.message || t(`proofing.addin.category.${mark.category}`);
    el.append(head, body);

    const bar = doc.createElement("div");
    bar.className = "review-inline-bar addin-card-bar";
    // The fixes are edits: offered only to an add-in holding `suggest`, and, as
    // in the built-in spell check, refused with the reason in Viewing.
    const viewing = ed.reviewMode === "viewing";
    if (provider.canReplace) {
      for (const replacement of mark.replacements.slice(0, 3)) {
        const label = replacement || t("proofing.addin.delete");
        bar.append(chip(label, {
          aria: replacement ? t("proofing.addin.replaceWith", { text: replacement }) : t("proofing.addin.removeText", { text: mark.exact }),
          disabledReason: viewing ? t("proofing.addin.viewing") : "",
          onClick: (event) => apply(issue, replacement, event),
        }));
      }
    }
    bar.append(chip(t("proofing.addin.ignore"), { icon: "close", onClick: () => ignore(issue), danger: true }));
    if (look.dictionary) bar.append(chip(t("proofing.addin.addToDictionary"), { icon: "book", onClick: () => void addToDictionary(issue) }));
    el.append(bar);
    el.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closeCard({ refocus: true });
      }
    });
    doc.body.append(el);
    card = el;
    position(issue);
    ed.drawSelection?.();
    if (focus) (bar.querySelector("button:not(:disabled)") ?? el).focus({ preventScroll: true });
    return true;
  }

  function position(issue) {
    if (!card) return;
    const rect = ed.reviewRangeClientRect?.(issue.mark.node, issue.mark.byteStart, issue.mark.node, issue.mark.byteEnd);
    const box = rect ?? { left: view.innerWidth / 2, top: view.innerHeight / 2, bottom: view.innerHeight / 2 };
    const left = Math.max(12, Math.min(view.innerWidth - card.offsetWidth - 12, box.left));
    const below = box.bottom + 6;
    const top = below + card.offsetHeight + 12 > view.innerHeight ? Math.max(12, box.top - card.offsetHeight - 6) : below;
    card.style.left = `${left}px`;
    card.style.top = `${top}px`;
  }

  /** Moves to the next (or previous) issue, selects it and opens its card. */
  function go(direction) {
    const list = issues().filter(({ mark }) => !hiddenByAccept(mark));
    if (!list.length) {
      announce(t("proofing.addin.none"));
      return false;
    }
    // From inside a mark, step from its start: Next leaves it, Previous goes
    // to the one before. Otherwise from the caret.
    const inside = markAtCaret();
    const focus = ed.selection?.focus;
    const from = inside
      ? { ordinal: inside.ordinal, offset: inside.mark.byteStart }
      : focus
        ? { ordinal: known.get(focus.node)?.ordinal ?? -Infinity, offset: Number(focus.offset) || 0 }
        : null;
    const issue = stepIssue(list, from, direction);
    ed.selection = { anchor: { node: issue.mark.node, offset: issue.mark.byteStart }, focus: { node: issue.mark.node, offset: issue.mark.byteEnd } };
    ed.drawSelection?.();
    ed.scrollReviewSelectionIntoView?.();
    const index = list.indexOf(issue) + 1;
    announce(t("proofing.addin.moved", { index, total: list.length, category: t(`proofing.addin.category.${issue.mark.category}`), message: issue.mark.message }));
    view.requestAnimationFrame?.(() => openCard(issue, { focus: true })) ?? openCard(issue, { focus: true });
    return true;
  }

  // A click that leaves the caret in a mark opens its card (LanguageTool,
  // Grammarly and Docs all open theirs from the word). Not on a drag or a
  // range, and not for the built-in squiggles: with no add-in running, a click
  // does exactly what it did before.
  ed.pagesEl?.addEventListener("click", (event) => {
    if (!providers.size || event.button !== 0 || ed.hasRange?.()) return;
    const issue = markAtCaret();
    if (issue) openCard(issue);
    else closeCard();
  });
  doc.addEventListener("pointerdown", (event) => {
    if (card && !card.contains(event.target)) closeCard();
  }, true);
  doc.addEventListener("keydown", (event) => {
    if (!card || card.contains(event.target)) return;
    if (event.key === "Escape") closeCard();
    // Typing goes on: the card is about text that is about to change.
    else if (!event.metaKey && !event.ctrlKey && !event.altKey) closeCard();
  });
  ed.viewportEl?.addEventListener("scroll", () => card && cardMark && position(cardMark), { passive: true });

  return {
    layer,
    /** A running add-in that proofreads. `send(paragraphs)` posts one check. */
    register({ id, name, canReplace, send }) {
      providers.set(id, { id, name, canReplace: !!canReplace, send, sent: new Map(), marks: new Map(), cache: new Map(), summary: "" });
      ed.spellChecker?.setExternal?.(layer);
      for (const listener of listeners) listener();
    },
    unregister(id) {
      if (!providers.delete(id)) return;
      if (cardMark?.provider.id === id) closeCard();
      if (!providers.size) ed.spellChecker?.setExternal?.(null);
      else ed.drawSelection?.();
      for (const listener of listeners) listener();
    },
    /** `proofing/report` from add-in `id`. Returns `{accepted, dropped}`. */
    report(id, params) {
      const provider = providers.get(id);
      if (!provider) return { accepted: 0, dropped: 0 };
      const checked = checkReport(params, (node) => provider.sent.get(node));
      let marks = 0;
      for (const result of checked.results) {
        if (provider.marks.size >= LIMITS.marksHeld) break;
        holdAnswer(provider, result.node, result.text, result.marks);
        marks += result.marks.length;
      }
      if (checked.summary) provider.summary = checked.summary;
      changed();
      return { accepted: marks, dropped: checked.dropped };
    },
    issues,
    count: () => issues().filter(({ mark }) => !hiddenByAccept(mark)).length,
    summaries: () => [...providers.values()].map((provider) => ({ name: provider.name, summary: provider.summary })),
    running: () => providers.size > 0,
    next: () => go(1),
    previous: () => go(-1),
    /** Opens the card for the issue at the caret, for the keyboard. */
    openAtCaret() {
      const issue = markAtCaret();
      if (!issue) {
        announce(t("proofing.addin.noneHere"));
        return false;
      }
      return openCard(issue, { focus: true });
    },
    openCard,
    closeCard,
    get card() {
      return card;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
