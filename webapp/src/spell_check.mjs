// Spell checking: the half that touches the engine, the overlay and the network.
//
// Designed in `docs/114`; the row is `docs/109` HF-035 / `docs/105` OO-003. The
// rules that decide whether a word is wrong — tokenizing, the skip list, the
// capitalization rule, suggestion generation and ranking — are in
// `spelling.mjs`, which is pure and unit-tested in node. What is here is the
// wiring those rules need and cannot be written without a browser: the lazy
// dictionary fetch, the WINDOWED scan, the per-paragraph cache, the overlay
// markers, and the behaviour behind the context-menu rows.
//
// Three things about this module are load-bearing and are easy to undo by
// accident:
//
//   1. **The scan is windowed.** It looks at the paragraphs on the pages in the
//      page window and at nothing else. The accessibility mirror was the last
//      thing here that projected the WHOLE document and it cost 87% of the time
//      to open a 16,000-paragraph file and killed the tab at 65,000 (`docs/104`
//      HF-158). A spelling pass over the document on open would reintroduce
//      exactly that, and it would do it on a feature nobody asked to wait for.
//   2. **Nothing on the keystroke path touches the dictionary.** `noteEdited()`
//      is three constant-time steps — `SpellScheduler`, which is the shape
//      `drafts.mjs` already uses for the same reason.
//   3. **Replacing a word is not a new mutation path.** It goes back out
//      through the host's `replaceRange`, which is `doc.replaceRanges` — the
//      call Replace All makes. One undoable action, closed in Viewing, tracked
//      in Suggesting, for free.
//
// Everything the module needs from the application arrives in `io`, so the only
// globals it reaches for are `fetch`, `document` and the timers.

import {
  DEFAULT_SPELL_LANGUAGE,
  ParagraphCache,
  SpellScheduler,
  dictionaryForLanguage,
  emptyDictionary,
  findMisspellings,
  parseDictionary,
  suggestionsFor,
  unsupportedLanguageMessage,
} from "./spelling.mjs";
import { stringIndexToByteOffset } from "./text_rules.mjs";

/** The class the squiggle is painted with. `pointer-events: none` in the
 *  stylesheet, exactly like `.review-comment-marker`: a marker that swallowed
 *  the event would break caret placement, and that defect has already been
 *  fixed once in this overlay (REVIEW-GAP-005). */
export const SPELL_MARKER_CLASS = "spell-error";

/** How long after the last edit the window is re-checked. */
export const SPELL_QUIESCE_MS = 400;

/** How long after a scroll. Shorter because nothing is changing under the
 *  reader — the paragraphs are already final and only the window moved — and a
 *  squiggle that arrives half a second after the text does reads as a bug. */
export const SPELL_SCROLL_QUIESCE_MS = 90;

/** Hard bound on one scan, in paragraphs. A page holds ~30, a window holds a
 *  handful of pages; this is the backstop that keeps a pathological layout
 *  (a page of empty paragraphs, a table of hundreds of one-line cells) from
 *  turning a scroll into a document walk. */
export const MAX_SCAN_PARAGRAPHS = 400;

/** Suggestions offered in the context menu. Word shows five. */
export const MAX_SUGGESTIONS = 5;

/**
 * Builds the spell checker.
 *
 * `io` is the whole of its contact with the application:
 *
 *   `getDoc()`          the open document, or null
 *   `windowPages()`     `[{ pageNumber, wTwip, hTwip }]` for the pages that
 *                       currently have a sheet — the page window, 1-based
 *   `place(flat, kind)` puts one `[page, x, y, w, h]` twip rect on its page's
 *                       overlay and returns the element (main.js's `place`)
 *   `caret()`           `{ node, offset }` or null — the word being typed is
 *                       not flagged until the caret leaves it
 *   `enabled()`         the remembered on/off preference
 *   `defaultLanguage()` the language to use where the document does not say
 *   `status(text, kind)` the status line
 *   `repaint()`         "the markers changed, repaint the overlay"
 *   `openWords()`       resolves the personal-dictionary store, or null
 *   `fetchText(url)`    defaults to `fetch`; injected so a test can serve a
 *                       dictionary without a server
 */
export function createSpellChecker(io) {
  const cache = new ParagraphCache(MAX_SCAN_PARAGRAPHS);
  /** language tag → parsed dictionary, once fetched. */
  const dictionaries = new Map();
  /** language tag → the in-flight fetch, so a scroll cannot start a second. */
  const pending = new Map();
  /** Words the user asked to ignore for this session only. Word's behaviour:
   *  not persisted, and cleared when the document is replaced. */
  let ignored = new Set();
  /** Individual occurrences dismissed with "Ignore once", keyed by node and
   *  the text that was there — so the dismissal dies when the paragraph
   *  changes, which is what "once" means. */
  let ignoredOnce = new Set();
  /** The user's own words, mirrored in memory so a check is a `Set` lookup.
   *  The store is the truth; this is the copy the O(1) path reads. */
  let personal = new Set();
  let personalStore = null;
  /** The nodes the last scan covered, in document order, with their text. */
  let scanned = [];
  /** Tags we have already reported as unsupported, so the status line says it
   *  once per language instead of on every scroll. */
  let reportedUnsupported = new Set();
  let unsupportedTag = "";

  const scheduler = new SpellScheduler({ check: runScan, quiesceMs: SPELL_QUIESCE_MS });

  const fetchText =
    io.fetchText ??
    (async (url) => {
      const response = await fetch(url);
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
      return response.text();
    });

  /**
   * Where a language's word list lives.
   *
   * The build stamps every module with `?v=<build>` through the page's import
   * map (`stamp-assets.py`), and a URL resolved against `import.meta.url` drops
   * that query — so the dictionary would be served from a fixed URL with a
   * four-hour cache and a deploy could pair a new build with an old list. The
   * module's own stamp is re-appended, which is the cheapest correct answer and
   * needs no build change at all.
   */
  function dictionaryUrl(language) {
    if (io.dictionaryUrl) return io.dictionaryUrl(language);
    const stamp = new URL(import.meta.url).search;
    const url = new URL(`../dict/${language}.txt`, import.meta.url);
    return `${url}${stamp}`;
  }

  /** The dictionary for `language`, fetching it if this is the first ask.
   *  Returns null while the fetch is in flight, so the scan paints what it has
   *  and comes back when the fetch resolves — never blocking a repaint. */
  function dictionary(language) {
    const loaded = dictionaries.get(language);
    if (loaded) return loaded;
    if (pending.has(language)) return null;
    const load = fetchText(dictionaryUrl(language))
      .then((text) => {
        dictionaries.set(language, parseDictionary(text));
        pending.delete(language);
        cache.clear();
        scheduler.flush();
        io.repaint?.();
      })
      .catch((error) => {
        pending.delete(language);
        // A dictionary that did not arrive is a checker that is not running,
        // and the user has to be able to tell that from a clean document.
        dictionaries.set(language, emptyDictionary());
        io.status?.(`Spelling dictionary for ${language} could not be loaded`, "error");
        console.warn("spelling dictionary", language, error?.message ?? error);
      });
    pending.set(language, load);
    return null;
  }

  /** The page a paragraph starts on, or 0 when it has no geometry. */
  function pageOf(doc, node) {
    try {
      return doc.caretRect(node, 0)[0] ?? 0;
    } catch {
      return 0;
    }
  }

  /**
   * The paragraphs on the pages currently in the window, in document order.
   *
   * Seeds by hit-testing the vertical middle of the first page in the window —
   * definitely body text, not the header band — then walks `moveCaret` back to
   * the start of the window and forward to its end. ~4 engine calls per
   * paragraph and nothing proportional to the document's size.
   *
   * **Body text only.** Header, footer, footnote, endnote and text-box stories
   * are separate stories and `moveCaret` does not walk out of one; they are a
   * stated gap (`docs/114` §5.2, `docs/109` HF-035), not an oversight.
   */
  function paragraphsInWindow(doc, pages) {
    const first = pages[0];
    const last = pages.at(-1);
    let seed = null;
    for (const fraction of [0.5, 0.35, 0.65, 0.25, 0.75]) {
      let hit = null;
      try {
        hit = doc.hitTest(
          first.pageNumber,
          Math.round(first.wTwip / 2),
          Math.round(first.hTwip * fraction),
        );
      } catch {
        hit = null;
      }
      if (hit) {
        seed = hit.node;
        hit.free();
        break;
      }
    }
    if (!seed) return [];

    let start = seed;
    for (let i = 0; i < MAX_SCAN_PARAGRAPHS; i += 1) {
      const moved = doc.moveCaret(start, 0, "left");
      const node = moved.node;
      moved.free();
      // The engine answers with the position it was given at the very start of
      // the document, so "the node did not change" is the end of the walk.
      if (node === start) break;
      if (pageOf(doc, node) < first.pageNumber) break;
      start = node;
    }

    const nodes = [start];
    let node = start;
    for (let i = 0; i < MAX_SCAN_PARAGRAPHS; i += 1) {
      const moved = doc.moveCaret(node, doc.paragraphLength(node), "right");
      const next = moved.node;
      moved.free();
      if (next === node) break;
      if (pageOf(doc, next) > last.pageNumber) break;
      node = next;
      nodes.push(node);
    }
    return nodes;
  }

  /** The language to check `node` in, and the dictionary it resolves to.
   *
   *  One `languageAt` call per PARAGRAPH over its whole range: if the paragraph
   *  is uniform the answer covers every word in it. `""` means the paragraph
   *  mixes languages (or the engine does not expose `w:lang` at all, which is
   *  the pre-`languageAt` build), and then the document default applies —
   *  per-run spans on a mixed paragraph are a stated gap (`docs/114` §8). */
  function languageFor(doc, node, length) {
    let tag = "";
    if (typeof doc.languageAt === "function") {
      try {
        tag = doc.languageAt(node, 0, length) || "";
      } catch {
        tag = "";
      }
    }
    return tag || io.defaultLanguage?.() || DEFAULT_SPELL_LANGUAGE;
  }

  /** Re-checks the paragraphs in the page window. O(window), never O(document). */
  function runScan() {
    const doc = io.getDoc?.();
    if (!doc || !io.enabled?.()) {
      scanned = [];
      return;
    }
    const pages = io.windowPages?.() ?? [];
    if (!pages.length) {
      scanned = [];
      return;
    }
    const caret = io.caret?.() ?? null;
    const next = [];
    let missing = "";
    for (const node of paragraphsInWindow(doc, pages)) {
      let length = 0;
      let text = "";
      try {
        length = doc.paragraphLength(node);
        text = doc.copyText(node, 0, node, length);
      } catch {
        continue;
      }
      const tag = languageFor(doc, node, length);
      const language = dictionaryForLanguage(tag);
      if (!language) {
        missing = tag;
        continue;
      }
      const words = dictionary(language);
      if (!words) continue; // the fetch is in flight; this paragraph waits
      const key = `${node} ${language}`;
      let found = cache.get(key, text);
      if (!found) {
        found = findMisspellings(text, words.all, { personal, ignored }).map((token) => ({
          ...token,
          byteStart: stringIndexToByteOffset(text, token.start),
          byteEnd: stringIndexToByteOffset(text, token.end),
        }));
        cache.set(key, text, found);
      }
      // The word the caret is inside is suppressed at PAINT time rather than at
      // cache time, so moving the caret does not invalidate the paragraph.
      next.push({ node, text, language, misspellings: found });
      if (caret?.node === node) {
        // Cheap: only the caret's own paragraph pays this.
        next.at(-1).caretOffset = caret.offset;
      }
    }
    const before = markerSignature(scanned);
    scanned = next;
    unsupportedTag = missing;
    if (missing && !reportedUnsupported.has(missing)) {
      reportedUnsupported.add(missing);
      io.status?.(unsupportedLanguageMessage(missing));
    }
    // The scan runs on a timer, AFTER whatever repaint the scroll or the edit
    // already did — so its results reach the screen only if it asks for one.
    // Guarded by a signature so an idle rescan that found the same words does
    // not rebuild every overlay in the window for nothing.
    if (markerSignature(scanned) !== before) io.repaint?.();
  }

  /** A cheap identity for what `paint` would draw. */
  function markerSignature(paragraphs) {
    return paragraphs
      .map(
        (paragraph) =>
          `${paragraph.node}:${paragraph.caretOffset ?? ""}:` +
          paragraph.misspellings.map((m) => `${m.byteStart}-${m.byteEnd}`).join(","),
      )
      .join("|");
  }

  /** Whether this occurrence has been dismissed with "Ignore once". */
  function dismissed(entry, paragraph) {
    return ignoredOnce.has(`${entry.word} ${paragraph.text} ${entry.start}`);
  }

  /** Paints the squiggles for what the last scan found. Called from the overlay
   *  repaint, so it must be cheap and must never scan. */
  function paint() {
    const doc = io.getDoc?.();
    if (!doc || !io.enabled?.()) return;
    for (const paragraph of scanned) {
      for (const entry of paragraph.misspellings) {
        if (
          paragraph.caretOffset !== undefined &&
          paragraph.caretOffset >= entry.byteStart &&
          paragraph.caretOffset <= entry.byteEnd
        )
          continue;
        if (dismissed(entry, paragraph)) continue;
        let rects = [];
        try {
          rects = doc.selectionRects(paragraph.node, entry.byteStart, paragraph.node, entry.byteEnd);
        } catch {
          continue;
        }
        for (let i = 0; i + 4 < rects.length; i += 5) {
          const el = io.place(rects.slice(i, i + 5), SPELL_MARKER_CLASS);
          if (el) el.dataset.spellWord = entry.word;
        }
      }
    }
  }

  /** The misspelling at a model position, or null. Drives the context menu. */
  function misspellingAt(anchor) {
    if (!anchor?.node || !io.enabled?.()) return null;
    const paragraph = scanned.find((candidate) => candidate.node === anchor.node);
    if (!paragraph) return null;
    const offset = Number(anchor.offset) || 0;
    for (const entry of paragraph.misspellings) {
      if (offset < entry.byteStart || offset > entry.byteEnd) continue;
      if (dismissed(entry, paragraph)) continue;
      return {
        node: paragraph.node,
        word: entry.word,
        start: entry.byteStart,
        end: entry.byteEnd,
        language: paragraph.language,
        paragraphText: paragraph.text,
        index: entry.start,
      };
    }
    return null;
  }

  /** Suggestions for one flagged word, computed HERE and only here — when the
   *  menu opens, for one word, never during a scan (`docs/114` §5.4). */
  function suggestions(flagged) {
    const words = dictionaries.get(flagged.language);
    if (!words) return [];
    return suggestionsFor(flagged.word, words, { limit: MAX_SUGGESTIONS, personal });
  }

  return {
    /** O(1) in document size. The ONLY thing the edit path calls. */
    noteEdited() {
      scheduler.noteDirty();
    },

    /** The page window moved. Same scheduler, shorter quiesce: nothing is
     *  changing under the reader, so there is nothing to wait out. */
    noteWindowChanged() {
      scheduler.noteDirty(SPELL_SCROLL_QUIESCE_MS);
    },

    /** Check now (a mode change, the preference being switched on, a test). */
    refresh() {
      scheduler.flush();
    },

    paint,
    misspellingAt,
    suggestions,

    /** Dismisses this occurrence. It comes back if the paragraph changes,
     *  which is Word's "Ignore Once". */
    ignoreOnce(flagged) {
      ignoredOnce.add(`${flagged.word} ${flagged.paragraphText} ${flagged.index}`);
      io.repaint?.();
    },

    /** Dismisses the word everywhere, for this session only — Word does not
     *  persist Ignore All either, and a persisted one is indistinguishable from
     *  Add to dictionary without a management surface to tell them apart. */
    ignoreAll(word) {
      ignored.add(word);
      cache.clear();
      scheduler.flush();
      io.repaint?.();
    },

    /** Adds a word to the personal dictionary, and persists it. Resolves to
     *  true when it was stored, false when storage refused — the caller says so
     *  rather than pretending (`docs/112` §4.1). */
    async addToDictionary(word) {
      personal.add(word);
      cache.clear();
      scheduler.flush();
      io.repaint?.();
      try {
        personalStore ??= await io.openWords?.();
        if (!personalStore) return false;
        await personalStore.add(word);
        return true;
      } catch (error) {
        console.warn("personal dictionary", error?.message ?? error);
        return false;
      }
    },

    /** Reads the personal dictionary at boot. Failure is not fatal: the checker
     *  runs without it, and the words the user adds this session still work in
     *  memory. */
    async loadPersonal() {
      try {
        personalStore ??= await io.openWords?.();
        if (!personalStore) return;
        personal = new Set(await personalStore.list());
        cache.clear();
        scheduler.flush();
        io.repaint?.();
      } catch (error) {
        console.warn("personal dictionary", error?.message ?? error);
      }
    },

    /** The document was replaced. Session state goes; the personal dictionary
     *  and the fetched word lists stay — they are not document-scoped. */
    reset() {
      cache.clear();
      ignored = new Set();
      ignoredOnce = new Set();
      scanned = [];
      reportedUnsupported = new Set();
      unsupportedTag = "";
      scheduler.cancel();
    },

    /** Turned off: every marker goes and the scan stops. The COMMAND stays
     *  enabled so it can be turned back on (SKILL.md §10, never a dead
     *  control). */
    setEnabled(on) {
      if (on) {
        scheduler.flush();
      } else {
        cache.clear();
        scanned = [];
        scheduler.cancel();
      }
      io.repaint?.();
    },

    /**
     * What the status line should say about spelling when it has nothing else
     * to say, or `""`.
     *
     * This exists because a one-shot announcement is not enough for a standing
     * condition. A document declaring a language we have no list for is
     * announced once when the scan first sees it — and then the font upgrade
     * re-renders, `renderAll` writes and clears its own progress message, and
     * the explanation is gone while the condition it explained is still true.
     * The user is then looking at an unchecked document with no reason given,
     * which is the exact failure mode the message exists to prevent. So
     * `renderAll` asks again every time it clears.
     *
     * "Spell check is off" is deliberately NOT reported here: that state is
     * already legible in the Tools menu, the palette row and the Settings
     * checkbox, and a permanent status line repeating a setting the user chose
     * is nagging, not information.
     */
    statusNote() {
      if (!io.enabled?.()) return "";
      return unsupportedTag ? unsupportedLanguageMessage(unsupportedTag) : "";
    },

    /** Test/inspection seam: how many paragraphs the last scan covered. */
    scannedCount() {
      return scanned.length;
    },

    personalWords() {
      return [...personal];
    },
  };
}

/**
 * The right-click rows for a flagged word, in Word's and Docs' order: the
 * suggestions first, then the three dismissals.
 *
 * Built here rather than in `main.js` so the shape is one declaration the
 * context menu and any future proofing pane both read, and so the "no
 * suggestions" case cannot be forgotten — an empty menu is never an acceptable
 * answer (SKILL.md §10), so zero suggestions still renders a disabled row that
 * says so.
 *
 * `actions` supplies the behaviour: `replace(flagged, word)` (which must go
 * through the host's existing `replaceRanges` path — NOT a new mutation path),
 * `ignoreOnce`, `ignoreAll`, `addToDictionary`. `blockedReason` disables the
 * replacements — and only the replacements — when the document cannot be
 * mutated; ignoring and adding a word are host-side and stay available in
 * Viewing mode, which is where a reader is most likely to be annoyed by a
 * false positive.
 */
export function spellingContextCommands(flagged, suggestions, actions, blockedReason = "") {
  const rows = [];
  if (suggestions.length === 0) {
    rows.push({
      id: "spell.noSuggestions",
      label: "No spelling suggestions",
      group: "spelling",
      enabled: false,
      disabledReason: `“${flagged.word}” is not in the dictionary and nothing close to it is`,
      run: () => {},
    });
  } else {
    suggestions.forEach((word, index) => {
      rows.push({
        id: `spell.suggestion.${index}`,
        label: word,
        group: "spelling",
        enabled: !blockedReason,
        disabledReason: blockedReason,
        run: () => actions.replace(flagged, word),
      });
    });
  }
  rows.push(
    {
      id: "spell.ignoreOnce",
      label: "Ignore once",
      group: "spellingDismiss",
      run: () => actions.ignoreOnce(flagged),
    },
    {
      id: "spell.ignoreAll",
      label: "Ignore all",
      group: "spellingDismiss",
      run: () => actions.ignoreAll(flagged.word),
    },
    {
      id: "spell.addToDictionary",
      label: "Add to dictionary",
      group: "spellingDismiss",
      run: () => actions.addToDictionary(flagged.word),
    },
  );
  return rows;
}
