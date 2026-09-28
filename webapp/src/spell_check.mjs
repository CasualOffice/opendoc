// Proofing — the COORDINATOR. It reads the document, decides what to ask, and
// paints what comes back. It decides nothing about language.
//
// Designed in `docs/114` (what shipped) and `docs/146` §4 (where it is going);
// decided in **ADR-042**. This module used to be both halves: it owned the
// dictionaries, parsed them, ran the rules and ranked the suggestions, all on
// the thread painting the document. Increment A moves every one of those
// decisions behind `proof_protocol.mjs` and runs them in `proof_worker.js`.
// What is left here is the three things that genuinely need a browser and an
// engine:
//
//   1. **Extraction** — which paragraphs are in the page window, what their
//      text is, and what language they are in.
//   2. **Transport** — getting that to a worker and the findings back, with a
//      fallback that is a transport and not a second implementation.
//   3. **Presentation and mutation** — overlay marks, the context-menu rows,
//      and one undoable replacement through the path Replace All already uses.
//
// Five things here are load-bearing and are easy to undo by accident:
//
//   1. **The scan is windowed.** It looks at the paragraphs on the pages in the
//      page window and at nothing else. The accessibility mirror was the last
//      thing here that projected the WHOLE document and it cost 87% of the time
//      to open a 16,000-paragraph file and killed the tab at 65,000 (`docs/104`
//      HF-158).
//   2. **Nothing on the keystroke path touches proofing beyond a timer.**
//      `noteEdited()` is three constant-time steps — `SpellScheduler`, the
//      shape `drafts.mjs` already uses for the same reason.
//   3. **Replacing a word is not a new mutation path.** It goes back out
//      through the host's `replaceRange`, which is `doc.replaceRanges`. One
//      undoable action, closed in Viewing, tracked in Suggesting, for free.
//   4. **A finding is verified against the document before it is believed.**
//      The worker answers about a past state by construction, so every reply is
//      checked against the current document id and the paragraph's current
//      revision and dropped if either moved (`docs/146` §4).
//   5. **UTF-16 crosses the boundary; UTF-8 meets the engine.** Findings arrive
//      as JS string indices and are converted to the engine's byte offsets
//      HERE, at the document adapter, and nowhere else (`docs/146` §5). Every
//      other file in this feature can stay in one coordinate system.
//
// Everything the module needs from the application arrives in `io`, so the only
// globals it reaches for are `fetch`, `Worker` and the timers — and a host may
// replace all three (ADR-042 §6: proofing is a separate optional package with
// host-injected network and transport).

import {
  DEFAULT_SPELL_LANGUAGE,
  GLOSSARY_FILE,
  ParagraphCache,
  SpellScheduler,
  dictionaryForLanguage,
  unsupportedLanguageMessage,
} from "./spelling.mjs";
import {
  BASIC_PACK_VERSION,
  MAX_SUGGESTIONS,
  PROOF_MESSAGE,
  PROOF_PROTOCOL_VERSION,
  createProofResponder,
  isFreshResult,
  proofCacheKey,
} from "./proof_protocol.mjs";
import { stringIndexToByteOffset } from "./text_rules.mjs";

/** The class the squiggle is painted with. `pointer-events: none` in the
 *  stylesheet, exactly like `.review-comment-marker`: a marker that swallowed
 *  the event would break caret placement, and that defect has already been
 *  fixed once in this overlay (REVIEW-GAP-005). */
export const SPELL_MARKER_CLASS = "spell-error";

/** The grammar mark. A different class, and a different colour, because Word
 *  and Docs both distinguish them and a reader needs to know whether they are
 *  being told about a word or about a sentence. */
export const GRAMMAR_MARKER_CLASS = "grammar-error";

/** Joins the parts of a composite key.
 *
 *  U+0000, written as an ESCAPE and never as a raw byte: a literal NUL in a
 *  source file makes git classify it as binary and silently stop showing its
 *  diffs, which `tracker_counts.test.mjs` already records as a trap this
 *  repository fell into once. `tests/source_bytes.test.mjs` now fails the build
 *  if one reappears anywhere under `webapp/src`. */
const KEY_SEP = "\u0000";

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

/** How long a freshly constructed worker has to say it is alive before the
 *  coordinator gives up on it and runs the checks in process.
 *
 *  A worker can fail in ways that never reach an `error` event — a module whose
 *  top-level import rejects never evaluates, so it never registers a `message`
 *  listener and never answers anything. Waiting for an event that will not come
 *  is how proofing stops silently, and a document that is silently unchecked is
 *  indistinguishable from a clean one (`docs/146` §1). */
export const PROOF_READY_TIMEOUT_MS = 1500;

/** How long one outstanding check may hold the queue closed.
 *
 *  The queue is capped at one check (`docs/146` §4) so that work about a state
 *  the reader has already left cannot get in front of the answer they are
 *  waiting for. The cap must not be able to become a deadlock: if a reply never
 *  arrives, the next scan goes out anyway, and the abandoned reply is rejected
 *  by its key if it ever turns up. */
export const PROOF_REPLY_GRACE_MS = 3000;

/** How many paragraph revisions are remembered. Only paragraphs that have been
 *  scanned are in here, and the map exists to answer "is this reply still about
 *  the current text" — a bound well above the scan bound costs nothing and
 *  keeps a long scrolling session from growing it. */
const MAX_TRACKED_REVISIONS = 4000;

export { MAX_SUGGESTIONS };

/** What `statusNote` says while a language's word list is missing rather than
 *  merely absent. A standing condition, so it is answered every time the status
 *  line is cleared and not announced once — `docs/114` §10.2 records what
 *  happens otherwise. */
export function unavailableLanguageMessage(language) {
  return `Spelling dictionary for ${language} could not be loaded`;
}

/**
 * Builds the proofing coordinator.
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
 *   `enabled()`         the remembered spelling on/off preference
 *   `grammarEnabled()`  the remembered grammar on/off preference
 *   `defaultLanguage()` the language to use where the document does not say
 *   `status(text, kind)` the status line
 *   `repaint()`         "the markers changed, repaint the overlay"
 *   `openWords()`       resolves the personal-dictionary store, or null
 *   `fetchText(url)`    defaults to `fetch`; injected so a test can serve a
 *                       dictionary without a server, and so a HOST can apply
 *                       its own network policy (ADR-042 §6)
 *   `proofTransport(onReply)` optional: a host-supplied worker transport
 */
export function createSpellChecker(io) {
  const cache = new ParagraphCache(MAX_SCAN_PARAGRAPHS);
  /** The word lists that have been asked for: language → "pending" | "ready" |
   *  "unavailable". The PARSED list lives in the worker; this side only needs
   *  to know whether to ask again, which is why nothing large is here. */
  const assets = new Map();
  /** "pending" | "ready" | "unavailable" for the product glossary. */
  let glossaryState = "";
  /** Bumped whenever a word list or the glossary ARRIVES or fails.
   *
   *  It is part of `packVersion` in the cache key, and it has to be, for a
   *  reason that cost a debugging session: the key otherwise says nothing about
   *  WHICH resources produced an answer. A check posted before the word list
   *  landed comes back with grammar findings only — correct at the time — and
   *  its reply then writes that answer into the cache under a key the re-check
   *  after the word list arrives computes identically. The re-check HITS, and
   *  the document is never spell-checked at all. Clearing the cache on arrival
   *  does not fix it: the stale reply lands after the clear. A key that names
   *  the resources does, and it is what `docs/146` §4 asks for — a pack change
   *  invalidates rather than serves. */
  let assetsCounter = 0;
  /** Grammar rules the user has switched off for this session, by rule id. */
  let ignoredRules = new Set();
  /** Words the user asked to ignore for this session only. Word's behaviour:
   *  not persisted, and cleared when the document is replaced. */
  let ignored = new Set();
  /** Individual occurrences dismissed with "Ignore once", keyed by node and
   *  the text that was there — so the dismissal dies when the paragraph
   *  changes, which is what "once" means. */
  let ignoredOnce = new Set();
  /** The user's own words. The worker holds the copy that does the lookups;
   *  this one exists so `personalWords()` can answer without a round trip. */
  let personal = new Set();
  let personalStore = null;
  /** The nodes the last scan covered, in document order, with their text. */
  let scanned = [];
  /** Tags reported as having no dictionary at all, so the status line says it
   *  once per language instead of on every scroll. */
  let reportedUnsupported = new Set();
  let unsupportedTag = "";
  /** The language of a paragraph IN THE WINDOW whose word list was asked for
   *  and refused, if any. Derived by each scan rather than latched on failure:
   *  a list that failed for a language the reader is not looking at is not a
   *  standing condition for them, and a status note about it would be noise
   *  about a document they do not have open. */
  let unavailableLanguage = "";

  /** Which document the findings are about. Bumped by `reset()`: node ids come
   *  from a counter that restarts per import, so without this a paragraph in a
   *  newly opened document can be served the previous document's findings. */
  let documentId = 1;
  /** nodeId → `{ text, revision }`. The revision is bumped whenever the text is
   *  observed to differ, which gives every reply a cheap total order to be
   *  checked against without shipping the text back. */
  const revisions = new Map();
  /** Bumped by anything the USER changed that alters the answer without
   *  altering the text: an ignored word, an ignored rule, a word added to the
   *  personal dictionary, a document replaced. It is in the cache key, which is
   *  what lets those actions stop clearing the whole cache to express that one
   *  thing changed. */
  let settingsCounter = 1;

  /** The `settingsRevision` of `docs/146` §4, which that tuple defines as
   *  covering "which passes are on" as well as the user's own choices. The two
   *  switches are not a counter — they are state the host owns and re-reads
   *  every scan — so they are folded in here rather than given a hook each.
   *  Without them a paragraph checked with grammar off is served straight back
   *  from the cache when grammar is switched on, and the marks never appear. */
  /** The `activePackVersion` of `docs/146` §4. Increment A ships one pack, so
   *  the constant identifies the DATA and the counter identifies how much of it
   *  has arrived. Increment B replaces the whole string with a real version. */
  function packVersionNow() {
    return `${BASIC_PACK_VERSION}.${assetsCounter}`;
  }

  function settingsRevisionFor(spelling, grammar) {
    return `${settingsCounter}:${spelling ? "s" : ""}${grammar ? "g" : ""}`;
  }
  let requestId = 0;
  /** The request id of the check currently in flight, or 0.
   *
   *  `docs/146` §4: "Budget and cap the worker queue" and "abort or supersede
   *  stale work". The worker is ONE thread, and the scheduler can fire a scan
   *  per scroll, per edit and per asset arrival — so an unbounded queue puts
   *  work about a state the reader has already left in front of the answer they
   *  are waiting for. At most one check is outstanding; anything that wants
   *  another while one is in flight sets `rescanPending` and gets it the moment
   *  the reply lands, with the newest document state, which is the answer that
   *  was wanted anyway. */
  let outstanding = 0;
  let outstandingAt = 0;
  let rescanPending = false;
  const now = () => (io.now ? io.now() : Date.now());

  const scheduler = new SpellScheduler({ check: runScan, quiesceMs: SPELL_QUIESCE_MS });

  const fetchText =
    io.fetchText ??
    (async (url) => {
      const response = await fetch(url);
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
      return response.text();
    });

  // ---- Transport ------------------------------------------------------------

  /** The last resources message, replayed if the worker has to be replaced. */
  let lastResources = null;
  let transport = null;
  /** Whether a transport has said it is alive. An in-process one always is. */
  let workerReady = false;

  /** An in-process responder, used when no worker can be built OR when one
   *  fails after being built. It is the SAME `createProofResponder` the worker
   *  runs, so a browser without workers gets identical findings — the fallback
   *  is a transport, not a second implementation. */
  function inProcessTransport(onReply) {
    const responder = createProofResponder();
    workerReady = true;
    return {
      post(message) {
        const reply = responder.handle(message);
        // A microtask, not a synchronous return: the caller must not be able to
        // depend on the answer arriving in the same turn, because with a real
        // worker it never does.
        if (reply) queueMicrotask(() => onReply(reply));
      },
      dispose() {},
    };
  }

  /** Falls back to in-process and replays what the worker had been told. Called
   *  when a worker errors — which is asynchronous, so it can happen long after
   *  construction succeeded. Proofing degrading to the main thread is bad;
   *  proofing silently stopping is worse, and a document that looks clean
   *  because the checker died is the failure `docs/146` §1 forbids. */
  function degrade(reason) {
    console.warn("proofing worker", reason);
    transport?.dispose();
    transport = inProcessTransport(onReply);
    outstanding = 0;
    rescanPending = false;
    if (lastResources) transport.post(lastResources);
    cache.clear();
    scheduler.flush();
  }

  function createTransport() {
    if (io.proofTransport) return io.proofTransport(onReply);
    if (io.useWorker === false || typeof Worker !== "function") {
      return inProcessTransport(onReply);
    }
    try {
      // The same stamping trick the dictionary fetch uses, and for the same
      // reason: a URL resolved against `import.meta.url` drops this module's
      // `?v=<build>`, and an unstamped worker is served from a fixed URL with a
      // four-hour cache.
      const stamp = new URL(import.meta.url).search;
      const url = `${new URL("./proof_worker.js", import.meta.url)}${stamp}`;
      const worker = new Worker(url, { type: "module" });
      worker.addEventListener("message", (event) => onReply(event.data));
      worker.addEventListener("error", (event) => degrade(event.message ?? "worker error"));
      // The deadline is the half of this that an event cannot give us: a module
      // worker whose own import rejects never evaluates, so it registers no
      // message listener, answers nothing, and reports nothing.
      setTimeout(() => {
        if (!workerReady) degrade(`no ready within ${PROOF_READY_TIMEOUT_MS}ms`);
      }, PROOF_READY_TIMEOUT_MS);
      return { post: (message) => worker.postMessage(message), dispose: () => worker.terminate() };
    } catch {
      return inProcessTransport(onReply);
    }
  }

  function post(message) {
    transport ??= createTransport();
    transport.post(message);
  }

  /** Tells the worker what it is checking WITH.
   *
   *  ACCUMULATED, not replaced: the dictionary and the glossary arrive in
   *  separate messages, and a replay that carried only the most recent one
   *  would rebuild a worker with half its resources. Word lists accumulate by
   *  locale, for the same reason. */
  function sendResources(patch) {
    lastResources = {
      ...lastResources,
      ...patch,
      type: PROOF_MESSAGE.resources,
      packVersion: packVersionNow(),
      settingsRevision: settingsCounter,
      personal: [...personal],
      ignoredWords: [...ignored],
      ignoredRules: [...ignoredRules],
      dictionaries: [
        ...(lastResources?.dictionaries ?? []).filter(
          (entry) => !(patch.dictionaries ?? []).some((next) => next.locale === entry.locale),
        ),
        ...(patch.dictionaries ?? []),
      ],
    };
    post(lastResources);
  }

  // ---- Assets ---------------------------------------------------------------

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

  /**
   * Loads the glossary once, the first time SPELLING runs.
   *
   * It is a spelling asset: it is the tier that stops our own product names
   * being flagged. A grammar-only pass has no use for it and no longer fetches
   * it — that is half of `docs/146` §2's decoupling, and the half the claim
   * there did not name.
   */
  function loadGlossary() {
    if (glossaryState) return;
    glossaryState = "pending";
    fetchText(dictionaryUrl(GLOSSARY_FILE))
      .then((text) => {
        glossaryState = "ready";
        assetsCounter += 1;
        sendResources({ glossaryText: text });
        scheduler.flush();
      })
      .catch((error) => {
        // A glossary that fails to arrive degrades to "no glossary", which
        // flags our own product names: visible, and better than pretending.
        glossaryState = "ready";
        assetsCounter += 1;
        sendResources({ glossaryText: "" });
        scheduler.flush();
        console.warn("spelling glossary", error?.message ?? error);
      });
  }

  /** Asks for `language`'s word list if this is the first time. Returns the
   *  asset's state, so the scan knows whether spelling can run for it. */
  function requestDictionary(language) {
    const state = assets.get(language);
    if (state) return state;
    assets.set(language, "pending");
    fetchText(dictionaryUrl(language))
      .then((text) => {
        assets.set(language, "ready");
        assetsCounter += 1;
        sendResources({ dictionaries: [{ locale: language, text }] });
        scheduler.flush();
      })
      .catch((error) => {
        // THE defect this increment exists to prevent: the failure path used to
        // install an EMPTY word list, and an empty word list means no word is
        // known — so a dictionary that failed to load underlined every word in
        // the document while the status line said it had failed. `UNAVAILABLE`
        // is a third state, and the worker contributes no spelling findings for
        // it (`docs/146` §2 CORRECTION).
        assets.set(language, "unavailable");
        assetsCounter += 1;
        sendResources({ dictionaries: [{ locale: language, unavailable: true }] });
        io.status?.(unavailableLanguageMessage(language), "error");
        scheduler.flush();
        console.warn("spelling dictionary", language, error?.message ?? error);
      });
    return "pending";
  }

  // ---- Extraction -----------------------------------------------------------

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
   * are separate stories, and the engine will not hand them over: `hitTest`
   * walks the body fragments only, and `moveCaret` left/right stops at a story
   * boundary by design — which is the property this walk relies on to terminate.
   * Painting those nodes would already work; ENUMERATING them needs a new
   * engine export, which is why `docs/146` §8 ranks it P3 and why it is not in
   * this increment.
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

  /** The language to check `node` in.
   *
   *  One `languageAt` call per PARAGRAPH over its whole range: if the paragraph
   *  is uniform the answer covers every word in it. `""` means the paragraph
   *  mixes languages (or the engine does not expose `w:lang` at all, which is
   *  the pre-`languageAt` build), and then the document default applies —
   *  per-run spans on a mixed paragraph are `docs/146` §8's P3 row. */
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

  /** The revision of `node` given the text just read from the engine. Bumped
   *  when the text differs from the last time it was seen, which is what makes
   *  a stale reply detectable without sending the text back. */
  function revisionFor(node, text) {
    const entry = revisions.get(node);
    if (entry && entry.text === text) {
      // Re-insert so the bound evicts least-recently-seen.
      revisions.delete(node);
      revisions.set(node, entry);
      return entry.revision;
    }
    const next = { text, revision: (entry?.revision ?? 0) + 1 };
    revisions.delete(node);
    revisions.set(node, next);
    while (revisions.size > MAX_TRACKED_REVISIONS) {
      revisions.delete(revisions.keys().next().value);
    }
    return next.revision;
  }

  /** The key the coordinator would compute for `node` right now, or `null` when
   *  it is not in the current scan. This is what a reply is checked against. */
  function expectedKey(node) {
    return scanned.find((row) => row.node === node)?.key ?? null;
  }

  // ---- The scan -------------------------------------------------------------

  /**
   * Re-checks the paragraphs in the page window. O(window), never O(document).
   *
   * It does no checking itself: it reads, decides what is already known, and
   * posts the rest. Everything after the post is `onReply`'s.
   */
  function runScan() {
    const doc = io.getDoc?.();
    const spelling = io.enabled?.() ?? false;
    const grammar = io.grammarEnabled?.() ?? false;
    if (!doc || (!spelling && !grammar)) {
      scanned = [];
      return;
    }
    // A spelling asset is fetched only when SPELLING is on. A grammar-only pass
    // asks for nothing over the network at all (`docs/146` §2).
    if (spelling) loadGlossary();
    const pages = io.windowPages?.() ?? [];
    if (!pages.length) {
      scanned = [];
      return;
    }
    const caret = io.caret?.() ?? null;
    const next = [];
    const ask = [];
    let missing = "";
    let failed = "";

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
      const locale = dictionaryForLanguage(tag);
      if (!locale) {
        // No word list exists for this language at all. Grammar is English-only
        // too, so there is nothing either pass can say about this paragraph —
        // but the user must be told WHICH language, or an unchecked document is
        // indistinguishable from a correct one.
        missing = tag;
        continue;
      }
      // Spelling runs only when its assets are actually usable. Grammar is
      // decided from the text alone, so it runs regardless — which is the whole
      // of the decoupling: a grammar mark never waits on, and is never withheld
      // by, a word list it does not consult.
      const spellingUsable =
        spelling && assets.get(locale) === "ready" && glossaryState === "ready";
      if (spelling && requestDictionary(locale) === "unavailable") failed = locale;
      if (!spellingUsable && !grammar) continue;

      const revision = revisionFor(node, text);
      const key = proofCacheKey({
        documentId,
        paragraphId: node,
        paragraphRevision: revision,
        locale,
        packVersion: packVersionNow(),
        settingsRevision: settingsRevisionFor(spelling, grammar),
        // Increment C's document terminology profile. Constant until it exists;
        // in the key now so that adding it later invalidates rather than serves
        // stale answers (`docs/146` §4, ADR-042).
        profileRevision: 0,
      });
      const row = { node, text, language: locale, key, misspellings: cache.get(key, text) };
      if (!row.misspellings) {
        row.misspellings = [];
        ask.push({
          paragraphId: node,
          paragraphRevision: revision,
          locale,
          text,
          key,
          spelling: spellingUsable,
        });
      }
      if (caret?.node === node) row.caretOffset = caret.offset;
      next.push(row);
    }

    const before = markerSignature(scanned);
    scanned = next;
    unsupportedTag = missing;
    unavailableLanguage = failed;
    if (missing && !reportedUnsupported.has(missing)) {
      reportedUnsupported.add(missing);
      io.status?.(unsupportedLanguageMessage(missing));
    }
    if (markerSignature(scanned) !== before) io.repaint?.();

    if (!ask.length) return;
    if (outstanding && now() - outstandingAt < PROOF_REPLY_GRACE_MS) {
      // A check is already in flight. Coalesce rather than queue: this scan's
      // paragraphs are re-read from the document when the reply lands.
      rescanPending = true;
      // ...and if that reply never lands, come back anyway. A reply can be
      // lost, and without this the cap on the queue becomes a stall that only a
      // scroll would break.
      scheduler.noteDirty(PROOF_REPLY_GRACE_MS);
      return;
    }
    // The worker is told which passes to run per REQUEST, and `spelling` is
    // narrowed per paragraph by the row's own flag, because a window can mix a
    // language whose list arrived with one whose list is still in flight.
    requestId += 1;
    outstanding = requestId;
    outstandingAt = now();
    post({
      type: PROOF_MESSAGE.check,
      requestId,
      documentId,
      spelling: ask.some((row) => row.spelling),
      grammar,
      // A paragraph whose word list has not arrived is still sent, so grammar
      // can be decided now; the worker simply holds no dictionary for its
      // locale and contributes no spelling findings for it.
      paragraphs: ask.map((row) => ({
        paragraphId: row.paragraphId,
        paragraphRevision: row.paragraphRevision,
        locale: row.locale,
        key: row.key,
        text: row.text,
      })),
    });
  }

  /**
   * A reply from the worker.
   *
   * Two guards, both from `docs/146` §4, and both able to fire in ordinary use
   * rather than only under contrivance: the document can be replaced and the
   * paragraph can be edited while a check is in flight.
   */
  function onReply(message) {
    if (!message || typeof message.type !== "string") return;
    if (message.type === PROOF_MESSAGE.ready) {
      if (message.version !== PROOF_PROTOCOL_VERSION) {
        degrade(`protocol ${message.version} != ${PROOF_PROTOCOL_VERSION}`);
        return;
      }
      workerReady = true;
      return;
    }
    if (message.type !== PROOF_MESSAGE.findings) return;
    if (message.requestId === outstanding) outstanding = 0;

    // Deliberately NOT matched against a set of outstanding request ids. That
    // would be a SECOND staleness mechanism, and it would be the one actually
    // doing the work — leaving the document-and-revision check of `docs/146` §4
    // impossible to drive red, which is a guard that cannot fail (SKILL.md §4).
    // A late reply whose paragraph has not changed is still a correct answer,
    // so there is nothing to reject about it.
    let changed = false;
    for (const result of message.results ?? []) {
      // STALENESS. The reply describes a past state by construction; this is
      // the one question that decides whether that past is still the present.
      if (!isFreshResult(result, { expectedKey })) continue;
      const row = scanned.find((candidate) => candidate.node === result.paragraphId);
      if (!row) continue;
      // The ONE place UTF-16 becomes UTF-8. Findings are JS string indices
      // everywhere else in this feature (`docs/146` §5).
      const byte = (index) => stringIndexToByteOffset(row.text, index);
      row.misspellings = result.findings.map((finding) => ({
        ...finding,
        replacements: finding.replacements.map((replacement) => replacement.text),
        byteStart: byte(finding.start),
        byteEnd: byte(finding.end),
      }));
      cache.set(row.key, row.text, row.misspellings);
      changed = true;
    }
    // The scan runs on a timer and the reply lands later still — always AFTER
    // whatever repaint the scroll or the edit already did. Without a repaint of
    // its own its results never reach the screen.
    if (changed) io.repaint?.();
    if (!outstanding && rescanPending) {
      rescanPending = false;
      scheduler.flush();
    }
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
    return ignoredOnce.has(`${entry.word}${KEY_SEP}${paragraph.text}${KEY_SEP}${entry.start}`);
  }

  /** Paints the marks for what the last scan found. Called from the overlay
   *  repaint, so it must be cheap and must never scan. */
  function paint() {
    const doc = io.getDoc?.();
    if (!doc) return;
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
        const grammar = entry.kind === "grammar";
        for (let i = 0; i + 4 < rects.length; i += 5) {
          const el = io.place(
            rects.slice(i, i + 5),
            grammar ? GRAMMAR_MARKER_CLASS : SPELL_MARKER_CLASS,
          );
          if (!el) continue;
          if (grammar) {
            el.dataset.grammarRule = entry.ruleId;
            el.title = entry.message;
          } else {
            el.dataset.spellWord = entry.word;
          }
        }
      }
    }
  }

  /** The spelling or grammar finding at a model position, or null. Drives the
   *  context menu; the name is kept because `main.js` and the specs use it, and
   *  the `kind` field says which of the two it is. */
  function misspellingAt(anchor) {
    if (!anchor?.node) return null;
    const paragraph = scanned.find((candidate) => candidate.node === anchor.node);
    if (!paragraph) return null;
    const offset = Number(anchor.offset) || 0;
    for (const entry of paragraph.misspellings) {
      if (offset < entry.byteStart || offset > entry.byteEnd) continue;
      if (dismissed(entry, paragraph)) continue;
      return {
        kind: entry.kind,
        ruleId: entry.ruleId,
        message: entry.message,
        replacements: entry.replacements ?? [],
        suggestionsComplete: entry.suggestionsComplete !== false,
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

  /**
   * Suggestions for one finding.
   *
   * They arrive WITH the finding now, computed in the worker while checking.
   * That reverses `docs/114` §5.4 ("computed on demand only, never while
   * checking") and the correction is recorded there: the rule existed because
   * the bounded dictionary scan costs 2.6–19.5 ms and was being paid on the
   * main thread inside a right-click. Off the main thread that reason is gone,
   * and what replaces it is a budget (`DEEP_SUGGESTION_BUDGET`) plus a per-word
   * memo, because "not on the main thread" is not the same as "unbounded".
   */
  function suggestions(flagged) {
    return flagged?.replacements ?? [];
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
      ignoredOnce.add(`${flagged.word}${KEY_SEP}${flagged.paragraphText}${KEY_SEP}${flagged.index}`);
      io.repaint?.();
    },

    /** Switches one grammar RULE off for this session. The right-click row that
     *  calls it names the rule, because "ignore this kind of mark" is what the
     *  user actually wants when a rule is wrong about their prose. */
    ignoreRule(ruleId) {
      ignoredRules.add(ruleId);
      settingsCounter += 1;
      sendResources({});
      scheduler.flush();
    },

    /** Dismisses the word everywhere, for this session only — Word does not
     *  persist Ignore All either, and a persisted one is indistinguishable from
     *  Add to dictionary without a management surface to tell them apart. */
    ignoreAll(word) {
      ignored.add(word);
      settingsCounter += 1;
      sendResources({});
      scheduler.flush();
    },

    /** Adds a word to the personal dictionary, and persists it. Resolves to
     *  true when it was stored, false when storage refused — the caller says so
     *  rather than pretending (`docs/112` §4.1). */
    async addToDictionary(word) {
      personal.add(word);
      settingsCounter += 1;
      sendResources({});
      scheduler.flush();
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
        settingsCounter += 1;
        sendResources({});
        scheduler.flush();
      } catch (error) {
        console.warn("personal dictionary", error?.message ?? error);
      }
    },

    /** The document was replaced. Session state goes; the personal dictionary
     *  and the loaded word lists stay — they are not document-scoped. The
     *  document id is bumped so a reply still in flight for the previous
     *  document cannot be painted onto this one. */
    reset() {
      documentId += 1;
      outstanding = 0;
      rescanPending = false;
      revisions.clear();
      cache.clear();
      ignored = new Set();
      ignoredOnce = new Set();
      ignoredRules = new Set();
      settingsCounter += 1;
      sendResources({});
      scanned = [];
      reportedUnsupported = new Set();
      unsupportedTag = "";
      scheduler.cancel();
    },

    /** Turned off: every marker goes and the scan stops. The COMMAND stays
     *  enabled so it can be turned back on (SKILL.md §10, never a dead
     *  control). */
    setEnabled(on) {
      settingsCounter += 1;
      if (on || io.grammarEnabled?.()) {
        scheduler.flush();
      } else {
        scanned = [];
        scheduler.cancel();
      }
      io.repaint?.();
    },

    /**
     * What the status line should say about proofing when it has nothing else
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
     * A word list that FAILED to load is the same class of standing condition
     * and is answered here too. It used to be announced once and then be
     * invisible — while the document was, at that time, entirely underlined.
     *
     * "Spell check is off" is deliberately NOT reported here: that state is
     * already legible in the Tools menu, the palette row and the Settings
     * checkbox, and a permanent status line repeating a setting the user chose
     * is nagging, not information.
     */
    statusNote() {
      if (!io.enabled?.()) return "";
      if (unsupportedTag) return unsupportedLanguageMessage(unsupportedTag);
      return unavailableLanguage ? unavailableLanguageMessage(unavailableLanguage) : "";
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
  if (flagged.kind === "grammar")
    return grammarContextCommands(flagged, suggestions, actions, blockedReason);
  const rows = [];
  if (suggestions.length === 0) {
    // Two different answers, because they are two different facts and only one
    // of them is "there is nothing to offer". A finding whose deep search was
    // over the per-check budget has NOT established that there are no
    // suggestions, and saying so would be the kind of confidently wrong empty
    // state this repository keeps finding in its own UI.
    const searched = flagged.suggestionsComplete !== false;
    rows.push({
      id: "spell.noSuggestions",
      label: searched ? "No spelling suggestions" : "Suggestions are not ready yet",
      group: "spelling",
      enabled: false,
      disabledReason: searched
        ? `“${flagged.word}” is not in the dictionary and nothing close to it is`
        : `“${flagged.word}” is not in the dictionary; the search for alternatives has not finished`,
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

/**
 * The right-click rows for a grammar finding.
 *
 * Shaped like Word's and Docs' grammar menu rather than their spelling one,
 * because the questions are different: a misspelling asks "which word did you
 * mean", a grammar mark asks "what is wrong here" — so the RULE'S MESSAGE leads
 * as a disabled explanation, the correction follows, and the dismissal is
 * per-rule ("stop telling me about repeated punctuation") rather than per-word.
 *
 * `Ignore once` is deliberately absent: a grammar mark is about a phrase that
 * is still being written, and "ignore this one occurrence forever" is not a
 * thing anyone wants from a rule — turning the rule off is.
 */
export function grammarContextCommands(flagged, replacements, actions, blockedReason = "") {
  const rows = [
    {
      id: "grammar.explain",
      label: flagged.message,
      group: "spelling",
      enabled: false,
      disabledReason: flagged.message,
      run: () => {},
    },
  ];
  replacements.forEach((text, index) => {
    rows.push({
      id: `grammar.suggestion.${index}`,
      label: text,
      group: "spelling",
      enabled: !blockedReason,
      disabledReason: blockedReason,
      run: () => actions.replace(flagged, text),
    });
  });
  rows.push({
    id: "grammar.ignoreRule",
    label: "Ignore this rule",
    group: "spellingDismiss",
    run: () => actions.ignoreRule(flagged.ruleId),
  });
  return rows;
}
