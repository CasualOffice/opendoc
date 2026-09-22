// Spelling: every decision, as plain functions over plain data.
//
// Designed in `docs/114`; the row is `docs/109` HF-035 / `docs/105` OO-003.
// The DOM half — the lazy fetch, the windowed scan, the overlay markers and the
// context-menu rows — lives in `spell_check.mjs`. Nothing here touches the
// browser or the engine, which is what lets the rules that actually decide
// whether a word is wrong be unit-tested in node with no wasm behind them.
//
// Three constraints shape this file, all of them from the project rather than
// from spelling:
//
//   1. **No runtime npm dependency.** `webapp/package.json` has none and adding
//      the first one is the owner's decision, so there is no Hunspell engine
//      here: the dictionary is a pre-expanded word list and membership is a
//      `Set` lookup. `docs/114` §2 works through why that is not a compromise.
//   2. **Per-keystroke work is O(1) in document size** (`docs/107` §4). Nothing
//      on the keystroke path may touch the dictionary; `SpellScheduler` below is
//      the whole of what a keystroke pays, and it is the same three
//      constant-time steps `drafts.mjs`'s `DraftScheduler.noteDirty()` takes.
//   3. **Suggestions are computed on demand, never while checking.** The scan
//      only ever asks "is this word in the list".

/** Separates a dictionary file's common tier from the rest. Written by
 *  `tools/build-dictionary.mjs`; see `docs/114` §2.4. */
export const SECTION_SEPARATOR = "---";

/** The language a document with no `w:lang` — or an unreadable one — is checked
 *  against. Deliberately NOT `navigator.language`: that makes the tests
 *  non-deterministic for no user benefit (`docs/114` §4.1). */
export const DEFAULT_SPELL_LANGUAGE = "en-US";

/** The dictionary files that exist, as shipped in `webapp/dict/`. */
export const SPELL_LANGUAGES = Object.freeze(["en-US", "en-GB"]);

/** BCP-47 tag → the dictionary that checks it, or `null` for "not checkable".
 *
 *  The Commonwealth dialects share the en-GB list, which is what LibreOffice
 *  and Word's own dictionary sharing does; any other `en-*` falls back to
 *  en-US. Adding French is adding one file and one row here.
 *
 *  Returning `null` rather than quietly defaulting is the point: a document in
 *  a language we have no list for must SAY so (`docs/114` §2.5, SKILL.md §10),
 *  because a checker that silently stops looks exactly like a clean document. */
export function dictionaryForLanguage(tag) {
  const normalized = String(tag ?? "").trim().replace(/_/g, "-");
  if (!normalized) return DEFAULT_SPELL_LANGUAGE;
  const [primary, ...rest] = normalized.split("-");
  if (primary.toLowerCase() !== "en") return null;
  const region = (rest.at(-1) ?? "").toUpperCase();
  if (region === "GB" || region === "UK") return "en-GB";
  if (["CA", "AU", "NZ", "IE", "ZA", "IN"].includes(region)) return "en-GB";
  return "en-US";
}

/** What the status bar says about a language nothing can check. Named, not
 *  generic: the user has to be able to tell "off" from "unsupported". */
export function unsupportedLanguageMessage(tag) {
  return `No spelling dictionary for ${tag}`;
}

/**
 * Parses a dictionary file into its two tiers.
 *
 * The file is the common tier, a line of `---`, then the rest — both sorted,
 * so the artifact stays diffable and this is one `indexOf` plus two `split`s.
 * A file with no separator is treated as one tier, which keeps a hand-written
 * test fixture usable.
 */
export function parseDictionary(text) {
  const body = String(text ?? "").replace(/\r\n/g, "\n");
  const marker = `\n${SECTION_SEPARATOR}\n`;
  const at = body.startsWith(`${SECTION_SEPARATOR}\n`) ? 0 : body.indexOf(marker);
  const split = (chunk) => chunk.split("\n").filter((line) => line.length > 0);
  if (at < 0) {
    const all = new Set(split(body));
    return { common: new Set(), all };
  }
  const head = at === 0 ? "" : body.slice(0, at);
  const tail = body.slice(at === 0 ? SECTION_SEPARATOR.length + 1 : at + marker.length);
  const common = new Set(split(head));
  const all = new Set([...common, ...split(tail)]);
  return { common, all };
}

/** An empty dictionary — what the checker holds before a fetch resolves, so
 *  every caller has the same shape to work against. */
export function emptyDictionary() {
  return { common: new Set(), all: new Set() };
}

// ---- Tokenizing ------------------------------------------------------------

/** Characters that can be INSIDE a word. Letters and marks, plus the two
 *  apostrophes (`don't`, `don’t`) and the hyphen — the hyphen is kept so a
 *  token like `e-mail` can be recognized and then split, rather than producing
 *  two tokens one of which is a single letter. */
const WORD_BODY = /[\p{L}\p{M}\p{Nd}'’-]/u;

/**
 * The spans of `text` that are an address rather than prose: a URL, an email,
 * a bare host name, a file path, or a dotted identifier.
 *
 * This is a correction to `docs/114` §5.3, which proposed skipping "a token
 * containing `://`, `@`, or a `.` between two letters". That rule cannot fire:
 * `:`, `/`, `@` and `.` are not word characters, so `http://x.com` is never ONE
 * token — it tokenizes as `http`, `x`, `com`, and `info@docscentre.com` (which
 * really is in the fidelity corpus) as `info`, `docscentre`, `com`. Three of
 * those six are not English words and every one of them would have been
 * squiggled. So the addresses are found in the TEXT first and any token inside
 * one is dropped.
 *
 * Returns `[[start, end], …]` in JS string indices, in order, possibly
 * overlapping. O(text.length).
 */
export function addressSpans(text) {
  const source = String(text ?? "");
  const patterns = [
    // scheme://rest, and the `mailto:`/`tel:` shapes that have no slashes
    /[A-Za-z][A-Za-z0-9+.-]*:\/\/\S+/gu,
    /\b(?:mailto|tel|file|data):\S+/gu,
    // an email address
    /[^\s@<>()[\]]+@[^\s@<>()[\]]+\.[A-Za-z]{2,}/gu,
    // a dotted host or file name — `docscentre.com`, `www.x.co.uk`, `notes.txt`
    /\b(?:[\p{L}\p{Nd}_-]+\.)+[A-Za-z]{2,}\b/gu,
    // a path, POSIX or Windows, with at least one separator between segments
    /\b[\p{L}\p{Nd}_.-]*(?:[/\\][\p{L}\p{Nd}_.-]+)+/gu,
  ];
  const spans = [];
  for (const pattern of patterns) {
    for (const match of source.matchAll(pattern)) {
      spans.push([match.index, match.index + match[0].length]);
    }
  }
  return spans.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

/**
 * Every word-shaped token in `text`, as `{ start, end, word }` in JS string
 * indices (UTF-16 code units — the caller converts to the engine's UTF-8 byte
 * offsets, which is what `byteOffsetToStringIndex` already does the other way).
 *
 * O(text.length). The caller only ever hands it ONE paragraph.
 */
export function tokenizeWords(text) {
  const source = String(text ?? "");
  const tokens = [];
  let start = -1;
  for (let i = 0; i <= source.length; i += 1) {
    const inWord = i < source.length && WORD_BODY.test(source[i]);
    if (inWord && start < 0) start = i;
    if (!inWord && start >= 0) {
      // Trim leading/trailing connectors: `"don't"` keeps its apostrophe but
      // `'quoted'` and `dash-` do not carry one into the lookup.
      let from = start;
      let to = i;
      while (from < to && /['’-]/.test(source[from])) from += 1;
      while (to > from && /['’-]/.test(source[to - 1])) to -= 1;
      if (to > from) tokens.push({ start: from, end: to, word: source.slice(from, to) });
      start = -1;
    }
  }
  return tokens;
}

// ---- Skip rules ------------------------------------------------------------

/**
 * Why `word` is not checkable at all, or `""` when it is.
 *
 * Each rule removes a class of token that a dictionary can only be wrong
 * about. The two Word offers as options are options here too, defaulting on,
 * which is what `105` OO-003 asks for.
 *
 * A token containing `://`, `@`, or a `.` between two letters is an address,
 * not prose. NOTE for whoever writes a test: the fidelity corpus contains
 * `info@docscentre.com`, so `@` must never be used as a probe marker either
 * (`docs/114` §5.3) — use something like `QZX`.
 */
export function skipReason(word, { ignoreNumbers = true, ignoreUpper = true } = {}) {
  if (word.length <= 1) return "single character";
  if (/:\/\//.test(word) || word.includes("@")) return "address";
  if (/\p{L}\.\p{L}/u.test(word)) return "address";
  if (ignoreNumbers && /\p{Nd}/u.test(word)) return "contains a digit";
  if (ignoreUpper && word === word.toUpperCase() && /\p{L}/u.test(word)) return "all caps";
  if (!/\p{L}/u.test(word)) return "no letters";
  return "";
}

// ---- The capitalization rule ------------------------------------------------

/** Hunspell's rule, not a lowercase comparison (`docs/114` §5.3):
 *
 *   * a **lower-case** entry matches the word in lower case, Title Case or ALL
 *     CAPS — `apple` accepts `apple`, `Apple`, `APPLE`;
 *   * a **Title-case** entry matches only Title Case and ALL CAPS — `London`
 *     accepts `London` and `LONDON`, and `london` stays flagged.
 *
 *  That is what Word does, and it is why including SCOWL's proper-name lists
 *  does not throw away the information case carries.
 */
function matchesWithCase(word, dictionary) {
  if (dictionary.has(word)) return true;
  const lower = word.toLowerCase();
  if (word !== lower && dictionary.has(lower)) {
    // A word that is not all-lower matches a lower-case entry only when its
    // own shape is Title Case or ALL CAPS — `iPhone` typed as `iPHone` should
    // not be accepted by the entry `iphone`.
    const upper = word.toUpperCase();
    const title = word[0].toUpperCase() + lower.slice(1);
    if (word === upper || word === title) return true;
  }
  if (word === word.toUpperCase()) {
    const title = word[0] + word.slice(1).toLowerCase();
    if (dictionary.has(title)) return true;
  }
  return false;
}

/** Strips one trailing possessive `'s` / `’s`, or returns `null`. The file
 *  deliberately does not carry the ~29,500 possessive forms SCOWL ships
 *  (`tools/build-dictionary.mjs`), so they are handled by rule. */
function possessiveStem(word) {
  const match = word.match(/^(.*\p{L})['’]s$/u);
  return match ? match[1] : null;
}

/**
 * Whether `word` is spelled correctly against `dictionary` (a `Set`), the
 * user's `personal` words and the session `ignored` set.
 *
 * O(1) in the document and O(1) in the dictionary — a handful of `Set` lookups.
 */
export function isKnownWord(word, dictionary, { personal, ignored } = {}) {
  if (!word) return true;
  if (personal && (personal.has(word) || personal.has(word.toLowerCase()))) return true;
  if (ignored && (ignored.has(word) || ignored.has(word.toLowerCase()))) return true;
  if (matchesWithCase(word, dictionary)) return true;
  const stem = possessiveStem(word);
  if (stem && matchesWithCase(stem, dictionary)) return true;
  if (stem && personal && personal.has(stem)) return true;
  // A hyphenated compound is correct when every part is (`e-mail`, `well-known`).
  if (word.includes("-")) {
    const parts = word.split("-").filter((part) => part.length > 0);
    if (parts.length > 1 && parts.every((part) => isKnownWord(part, dictionary, { personal, ignored })))
      return true;
  }
  return false;
}

/**
 * Every misspelling in one paragraph's text, as `{ start, end, word }` in JS
 * string indices.
 *
 * `caretOffset`, when given, suppresses the word the caret is inside — Word's
 * behaviour, and the reason the common case of typing costs nothing at all
 * (`docs/114` §3).
 *
 * O(paragraph length). It is never called on more than the paragraphs in the
 * page window (`docs/114` §5.2).
 */
export function findMisspellings(text, dictionary, options = {}) {
  const { caretOffset = null, personal, ignored, ...skipOptions } = options;
  const spans = addressSpans(text);
  const inAddress = (token) =>
    spans.some(([from, to]) => token.start < to && token.end > from);
  const found = [];
  for (const token of tokenizeWords(text)) {
    if (caretOffset !== null && caretOffset >= token.start && caretOffset <= token.end) continue;
    if (skipReason(token.word, skipOptions)) continue;
    if (inAddress(token)) continue;
    if (isKnownWord(token.word, dictionary, { personal, ignored })) continue;
    found.push(token);
  }
  return found;
}

// ---- Suggestions ------------------------------------------------------------

const EDIT_ALPHABET = "abcdefghijklmnopqrstuvwxyz'";

/** Every Damerau-Levenshtein distance-1 edit of `word`: deletion, transposition,
 *  replacement, insertion. ~`54n + 25` strings for an `n`-character word, so a
 *  membership test over all of them is a few hundred `Set` lookups. */
function edits1(word) {
  const out = new Set();
  for (let i = 0; i < word.length; i += 1) {
    out.add(word.slice(0, i) + word.slice(i + 1));
    if (i + 1 < word.length) {
      out.add(word.slice(0, i) + word[i + 1] + word[i] + word.slice(i + 2));
    }
    for (const c of EDIT_ALPHABET) {
      out.add(word.slice(0, i) + c + word.slice(i + 1));
      out.add(word.slice(0, i) + c + word.slice(i));
    }
  }
  for (const c of EDIT_ALPHABET) out.add(word + c);
  out.delete(word);
  return out;
}

/** Re-applies the input word's case to a lower-case candidate, so a suggestion
 *  for `Teh` reads `The` and one for `TEH` reads `THE`. */
function matchCase(candidate, like) {
  if (like === like.toUpperCase() && like !== like.toLowerCase()) return candidate.toUpperCase();
  if (like[0] === like[0].toUpperCase()) return candidate[0].toUpperCase() + candidate.slice(1);
  return candidate;
}

/** How a candidate differs from the word, cheapest kind first. Hunspell ranks
 *  by edit KIND before anything else (`swapchar` before `badchar` before
 *  `forgotchar`) and it is the difference between `teh` → `the` and
 *  `teh` → `tea`: all four of tea/tee/ten/the are one edit away, in the common
 *  tier, and start with `t`, so without this the tie breaks alphabetically and
 *  the right answer comes fourth. */
const EDIT_TRANSPOSE = 0;
const EDIT_EXTRA = 1; // the typed word has a character too many
const EDIT_SUBSTITUTE = 2;
const EDIT_MISSING = 3; // the typed word is missing a character

function editKind(word, candidate) {
  if (candidate.length < word.length) return EDIT_EXTRA;
  if (candidate.length > word.length) return EDIT_MISSING;
  let first = -1;
  let differences = 0;
  for (let i = 0; i < word.length; i += 1) {
    if (word[i] === candidate[i]) continue;
    differences += 1;
    if (first < 0) first = i;
    if (differences > 2) return EDIT_SUBSTITUTE;
  }
  if (
    differences === 2 &&
    word[first] === candidate[first + 1] &&
    word[first + 1] === candidate[first]
  )
    return EDIT_TRANSPOSE;
  return EDIT_SUBSTITUTE;
}

/**
 * Damerau-Levenshtein distance between `a` and `b`, or `max + 1` as soon as it
 * is provably greater than `max`.
 *
 * O(|a| × |b|) worst case, but the early abort and the band around the diagonal
 * make it a few dozen cell updates for the length-filtered candidates it is
 * actually run on. It is the inner loop of `suggestionsFor`'s second stage and
 * is exported so its own behaviour can be unit-tested.
 */
/** Three scratch rows, reused across calls and grown on demand.
 *
 *  Allocating them per call — three holey `Array`s per ROW, tens of thousands
 *  of times per right-click — measured 5–10× slower than this on the shipped
 *  list. Safe because JavaScript here is single-threaded and the function is
 *  not reentrant: nothing it calls calls it back. */
let distanceRows = [new Int32Array(64), new Int32Array(64), new Int32Array(64)];

function distanceScratch(size) {
  if (distanceRows[0].length < size) {
    const grown = Math.max(size, distanceRows[0].length * 2);
    distanceRows = [new Int32Array(grown), new Int32Array(grown), new Int32Array(grown)];
  }
  return distanceRows;
}

export function boundedEditDistance(a, b, max) {
  const n = a.length;
  const m = b.length;
  if (Math.abs(n - m) > max) return max + 1;
  if (n === 0) return m;
  if (m === 0) return n;
  const rows = distanceScratch(m + 1);
  let twoAgo = rows[0];
  let previous = rows[1];
  let current = rows[2];
  for (let j = 0; j <= m; j += 1) previous[j] = j;
  for (let i = 1; i <= n; i += 1) {
    current[0] = i;
    let rowMin = i;
    const lo = Math.max(1, i - max - 1);
    const hi = Math.min(m, i + max + 1);
    for (let j = 1; j <= m; j += 1) {
      if (j < lo || j > hi) {
        current[j] = max + 1;
        continue;
      }
      const cost = a.charCodeAt(i - 1) === b.charCodeAt(j - 1) ? 0 : 1;
      let value = Math.min(current[j - 1] + 1, previous[j] + 1, previous[j - 1] + cost);
      if (
        i > 1 &&
        j > 1 &&
        a.charCodeAt(i - 1) === b.charCodeAt(j - 2) &&
        a.charCodeAt(i - 2) === b.charCodeAt(j - 1)
      ) {
        value = Math.min(value, twoAgo[j - 2] + 1);
      }
      current[j] = value;
      if (value < rowMin) rowMin = value;
    }
    if (rowMin > max) return max + 1;
    const spare = twoAgo;
    twoAgo = previous;
    previous = current;
    current = spare;
  }
  return previous[m];
}

/**
 * Up to `limit` suggestions for `word`, best first.
 *
 * Two stages, both paid ONLY when the context menu opens for one word — never
 * on the scan path, which asks nothing but "is this word in the list"
 * (`docs/114` §5.4).
 *
 *   1. Generate the ~`54n + 25` distance-1 edits and keep those in the list.
 *      A few hundred `Set` lookups: sub-millisecond.
 *   2. Only if that found fewer than three, scan the dictionary with a bounded
 *      Damerau-Levenshtein, length-filtered and first-two-characters-filtered.
 *
 * **Stage 2 is a deliberate departure from `docs/114` §5.4**, which specified
 * generating distance-2 edits and estimated "~360,000 lookups, tens of
 * milliseconds". Measured on the shipped 83,775-word en-US list, generating the
 * distance-2 neighbourhood costs **1.5–3.5 seconds** (the cost is building
 * ~200,000 strings, not looking them up), which would lock the tab on a
 * right-click. The bounded scan over the dictionary answers the same question
 * in **4–26 ms** for the same words and ranks better, because it knows each
 * candidate's real distance instead of inferring it from which pass found it.
 *
 * Ranking: distance, then same first letter (typists rarely get the first
 * letter wrong), then edit kind, then the common tier, then length difference,
 * then alphabetical — a total order, so the tests can name cases.
 */
export function suggestionsFor(word, { common, all }, { limit = 5, personal } = {}) {
  const lower = String(word ?? "").toLowerCase();
  if (!lower) return [];
  const canonical = (candidate) => {
    if (all.has(candidate)) return candidate;
    const title = candidate[0].toUpperCase() + candidate.slice(1);
    if (all.has(title)) return title;
    if (personal?.has(candidate)) return candidate;
    return null;
  };

  /** lower-case candidate → { distance, shown } */
  const scored = new Map();
  const consider = (candidate, distance) => {
    if (candidate === lower) return;
    const existing = scored.get(candidate);
    if (existing && existing.distance <= distance) return;
    const shown = canonical(candidate);
    if (!shown) return;
    scored.set(candidate, { distance, shown });
  };

  for (const candidate of edits1(lower)) consider(candidate, 1);

  if (scored.size < 3) {
    // Two prefilters before the distance is computed at all, because they run
    // 83,775 times and it runs a few thousand: the length must be within the
    // bound, and a real typo almost never changes BOTH of the first two
    // characters. `| 32` folds an ASCII/Latin-1 capital without allocating a
    // lower-cased copy of every entry. Both filters can in principle reject a
    // true distance-2 neighbour; that is the trade that keeps a right-click
    // under a frame, and it is stated rather than hidden.
    const head = lower.charCodeAt(0);
    const second = lower.length > 1 ? lower.charCodeAt(1) : -1;
    for (const entry of all) {
      const delta = entry.length - lower.length;
      if (delta > 2 || delta < -2) continue;
      if (
        (entry.charCodeAt(0) | 32) !== head &&
        (entry.length > 1 ? entry.charCodeAt(1) | 32 : -1) !== second
      )
        continue;
      const candidate = entry.toLowerCase();
      const distance = boundedEditDistance(lower, candidate, 2);
      if (distance <= 2) consider(candidate, distance);
    }
    if (personal) {
      for (const entry of personal) {
        const candidate = entry.toLowerCase();
        const distance = boundedEditDistance(lower, candidate, 2);
        if (distance <= 2) consider(candidate, distance);
      }
    }
  }

  // A two-letter answer to an eight-letter typo is noise, not a suggestion —
  // Word suppresses these too.
  if (lower.length >= 4) {
    for (const [candidate] of scored) if (candidate.length < 3) scored.delete(candidate);
  }

  const isCommon = (candidate) =>
    common.has(candidate) || common.has(candidate[0].toUpperCase() + candidate.slice(1));

  return [...scored.entries()]
    .sort(([wordA, a], [wordB, b]) => {
      if (a.distance !== b.distance) return a.distance - b.distance;
      const headA = wordA[0] === lower[0];
      const headB = wordB[0] === lower[0];
      if (headA !== headB) return headA ? -1 : 1;
      const commonA = isCommon(wordA);
      const commonB = isCommon(wordB);
      if (commonA !== commonB) return commonA ? -1 : 1;
      const kindA = editKind(lower, wordA);
      const kindB = editKind(lower, wordB);
      if (kindA !== kindB) return kindA - kindB;
      const lenA = Math.abs(wordA.length - lower.length);
      const lenB = Math.abs(wordB.length - lower.length);
      if (lenA !== lenB) return lenA - lenB;
      return wordA < wordB ? -1 : 1;
    })
    .slice(0, limit)
    .map(([, entry]) => matchCase(entry.shown, word));
}

// ---- The keystroke cadence ---------------------------------------------------

/**
 * The debounce between an edit and a re-check.
 *
 * Deliberately the same SHAPE as `DraftScheduler` in `drafts.mjs` — store a
 * flag, clear a timer, set a timer — rather than a second cadence invented for
 * this feature. `noteDirty()` is the ONLY thing the keystroke path calls, and
 * it is O(1) in document size: it never looks at the document, the dictionary
 * or the page window.
 *
 * Timers are injected, so `spelling.test.mjs` can prove the zero-scans-while-
 * typing claim in node instead of asserting it in prose.
 */
export class SpellScheduler {
  constructor({
    check,
    quiesceMs = 400,
    setTimer = (fn, ms) => setTimeout(fn, ms),
    clearTimer = (handle) => clearTimeout(handle),
  }) {
    this.check = check;
    this.quiesceMs = quiesceMs;
    this.setTimer = setTimer;
    this.clearTimer = clearTimer;
    this.handle = null;
    this.dirty = false;
  }

  /** O(1). Called from the edit choke point, on every keystroke.
   *
   *  `quiesceMs` overrides the default for this one call — a scroll waits less
   *  than an edit, because nothing is changing under the reader. It is an
   *  argument rather than a second scheduler on purpose: two schedulers racing
   *  over one check is two cadences to reason about and only one of them would
   *  stay right. */
  noteDirty(quiesceMs = this.quiesceMs) {
    this.dirty = true;
    if (this.handle !== null) this.clearTimer(this.handle);
    this.handle = this.setTimer(() => {
      this.handle = null;
      this.dirty = false;
      this.check();
    }, quiesceMs);
  }

  /** Runs the pending check now (a scroll, a mode change, an explicit refresh). */
  flush() {
    if (this.handle !== null) this.clearTimer(this.handle);
    this.handle = null;
    this.dirty = false;
    this.check();
  }

  cancel() {
    if (this.handle !== null) this.clearTimer(this.handle);
    this.handle = null;
    this.dirty = false;
  }
}

// ---- The per-paragraph cache -------------------------------------------------

/**
 * `nodeId → { text, misspellings }`, bounded.
 *
 * An entry is valid while `copyText` returns the same string, so scrolling back
 * is free and an edit elsewhere in the document invalidates nothing. The bound
 * is what stops a long session in a long document growing it without limit; the
 * eviction is least-recently-used, which for a scroll-driven workload is
 * "furthest from the window" (`docs/114` §5.2).
 */
export class ParagraphCache {
  constructor(limit = 400) {
    this.limit = limit;
    this.entries = new Map();
  }

  /** The cached misspellings for `nodeId` if the text still matches, else null. */
  get(nodeId, text) {
    const entry = this.entries.get(nodeId);
    if (!entry || entry.text !== text) return null;
    // Re-insert so Map iteration order is least-recently-used first.
    this.entries.delete(nodeId);
    this.entries.set(nodeId, entry);
    return entry.misspellings;
  }

  set(nodeId, text, misspellings) {
    this.entries.delete(nodeId);
    this.entries.set(nodeId, { text, misspellings });
    while (this.entries.size > this.limit) {
      const oldest = this.entries.keys().next().value;
      this.entries.delete(oldest);
    }
  }

  clear() {
    this.entries.clear();
  }

  get size() {
    return this.entries.size;
  }
}
