// The proofing contract: what crosses the worker boundary, and the whole of
// what decides a finding.
//
// Designed in `docs/146` §4/§5 (the owner's architecture of 2026-09-28),
// decided in **ADR-042**. It shipped as Increment A — stability — and Increment B
// (installation) added exactly one thing to it: a per-locale SUPPLEMENT tier,
// which is what an installed language pack's words arrive as. Nothing here knows
// how a pack is fetched, verified or stored (`proof_packs.mjs`, `proof_store.mjs`)
// and nothing here knows about a document profile, which is Increment C; the
// shapes below leave room for it rather than pretending to carry it.
//
// ## Why this file is pure, and stays pure
//
// It is in `PURE_MODULES`: no DOM, no engine, no `fetch`, no storage, no
// timers. Three things follow from that, and each is a requirement rather than
// a nicety:
//
//   1. **The worker and the in-process fallback run the SAME function.** A
//      browser with no `Worker` (or an embed that refuses one) gets identical
//      findings, because `createProofResponder()` is what both call. Two
//      implementations of one rule diverge; this repository has paid for that
//      lesson elsewhere (SKILL.md §8, "prefer one mechanism over two").
//   2. **ADR-042 §6 — proofing is a separate optional package with host-injected
//      network and storage.** Nothing here may reach a webapp global or assume
//      the editor's own `fetch`. Dictionary TEXT arrives as a message payload;
//      who fetched it, and under what host policy, is not this module's
//      business.
//   3. It is unit-testable in node with no browser, which is the only reason
//      the offset and staleness guards can be driven red cheaply.
//
// ## The three rules that are load-bearing here
//
//   * **A missing or failed word list is UNAVAILABLE, never empty.** An empty
//     dictionary means "no word is known", which underlines the whole document.
//     That was a real, measured defect (`docs/146` §2, the CORRECTION) and the
//     type below makes it unrepresentable: `null` is "not loaded yet",
//     `UNAVAILABLE` is "asked and refused", and only a parsed pair checks
//     anything.
//   * **Spelling and grammar are decided independently, per paragraph.** A
//     grammar finding never waits on a spelling asset.
//   * **Offsets on the wire are UTF-16** — JS string indices into
//     `ProofInput.text` — because the worker operates on JS strings. The
//     conversion to the engine's UTF-8 byte offsets happens once, in the
//     document adapter (`spell_check.mjs`), and nowhere else (`docs/146` §5).

import {
  findMisspellings,
  isKnownWord,
  parseDictionary,
  parseGlossary,
  suggestionsFor,
  tokenizeWords,
  skipReason,
} from "./spelling.mjs";
import { findGrammarIssues, grammarSupports } from "./grammar.mjs";

/** The protocol's own version. A worker built from a different revision than
 *  the page — which a four-hour asset cache can genuinely produce — announces
 *  its version in `ready`, and a mismatch falls back to in-process rather than
 *  exchanging messages neither side agrees on. */
export const PROOF_PROTOCOL_VERSION = 1;

/** Message kinds. Namespaced, because a worker's `message` event is shared with
 *  anything else the host may post to it. */
export const PROOF_MESSAGE = Object.freeze({
  /** page → worker: resources changed (word list, glossary, an installed
   *  language pack's supplement, user's words, the session's ignore sets).
   *  Partial: only the fields present are applied. */
  resources: "proof:resources",
  /** page → worker: check these paragraphs. */
  check: "proof:check",
  /** worker → page: I am loaded and this is my protocol version. */
  ready: "proof:ready",
  /** worker → page: findings for a `check`. */
  findings: "proof:findings",
});

/** A language whose word list was asked for and refused — offline, a 404, a
 *  parse failure. Distinct from `null` ("not asked yet") and from a parsed
 *  dictionary, because the three mean three different things to a user and
 *  collapsing two of them is what flagged every word in the document. */
export const UNAVAILABLE = "unavailable";

/** The version of the shipped word lists and rules — the BASIC tier `docs/146`
 *  §6 describes, which is the data already in `webapp/dict/`.
 *
 *  It is one half of the cache key's `activePackVersion`: Increment B's
 *  `proof_packs.mjs` composes this constant, how much of the basic tier has
 *  arrived, and every installed pack's immutable version into the token the key
 *  actually carries (`activePackVersion`). Installing or removing a pack
 *  therefore invalidates results rather than serving the previous data's
 *  answers, which is what §4 asks the key to do. */
export const BASIC_PACK_VERSION = "basic-1";

/** How many distinct unknown words one `check` may spend the DEEP suggestion
 *  scan on.
 *
 *  Suggestions are computed while checking rather than on right-click, which
 *  reverses `docs/114` §5.4 — see the correction recorded there. The reversal is
 *  only sound because the work is off the main thread, and "off the main thread"
 *  is not the same as "unbounded": a window of 400 paragraphs of gibberish would
 *  otherwise put thousands of dictionary scans in front of the findings the
 *  reader is waiting for. Past this budget a finding still carries whatever the
 *  cheap distance-1 pass found and reports `suggestionsComplete: false`, so the
 *  menu can say "not ready" instead of the lie "there are none". */
export const DEEP_SUGGESTION_BUDGET = 24;

/** Suggestions offered for one word. Word shows five. */
export const MAX_SUGGESTIONS = 5;

/** How many words the per-worker suggestion memo holds. Suggestions depend only
 *  on (word, locale, settings revision, active pack version), so the same typo
 *  repeated down a document costs once. */
const SUGGESTION_MEMO_LIMIT = 500;

/**
 * The cache key for one paragraph's findings.
 *
 * `docs/146` §4: `(documentId, paragraphId, paragraphRevision, locale,
 * activePackVersion, settingsRevision, profileRevision)`. Every part is here
 * because every part changes the ANSWER for the same text:
 *
 *   * `documentId` — a new document reuses node ids from the same counter, so
 *     without it a paragraph in the new document can be served the old one's
 *     findings.
 *   * `paragraphRevision` — the edit generation of that paragraph. The text is
 *     compared as well (the cache stores it), so this is belt and braces; what
 *     it buys is a cheap total order for rejecting a stale worker REPLY, where
 *     comparing text would mean shipping the text back.
 *   * `locale` — `colour` is right in en-GB and wrong in en-US.
 *   * `packVersion` — installing or removing a pack must not serve the previous
 *     pack's answers (Increment B).
 *   * `settingsRevision` — which passes are on, plus the rules and words the
 *     user has switched off. This replaces the `cache.clear()` that used to
 *     follow every Ignore: clearing threw away every OTHER paragraph's work to
 *     express "one word changed".
 *   * `profileRevision` — the document terminology profile of `docs/146` §7.
 *     **Constant until Increment C builds one.** It is in the key's shape now,
 *     and said to be constant here, rather than being added later by somebody
 *     who has to rediscover why the key has to hold it.
 */
export function proofCacheKey({
  documentId,
  paragraphId,
  paragraphRevision,
  locale,
  packVersion = BASIC_PACK_VERSION,
  settingsRevision = 0,
  profileRevision = 0,
}) {
  return [
    documentId,
    paragraphId,
    paragraphRevision,
    locale,
    packVersion,
    settingsRevision,
    profileRevision,
  ].join("\u0000");
}

/**
 * Whether a reply still describes the document as it is now.
 *
 * `docs/146` §4: "Before painting or replacing, verify document ID and
 * paragraph revision; discard stale findings." The worker is asynchronous, so
 * every reply is about a past state by construction; this is the one question
 * that decides whether that past is still the present.
 *
 * **The cache key is the test, and that is deliberate.** §4 names the document
 * id and the paragraph revision, and both are IN the key — along with the
 * locale, the pack version and the settings revision, which are the other three
 * ways a past answer can be wrong about the present. A separate id-and-revision
 * comparison beside this one would be a second mechanism that passes while this
 * one is what actually decides, and it would leave neither drivable red.
 *
 * The case that made this necessary is not exotic and is not about editing: a
 * check posted before the word list arrived comes back with grammar findings
 * only — correct when it was asked — and applying it to a paragraph the
 * coordinator is now ready to spell-check writes "no misspellings" into the
 * cache under the key the NEXT check would compute. The document is then never
 * spell-checked at all, in a way no text comparison can see, because the text
 * never changed. The resources did.
 *
 * `expectedKey(paragraphId)` is the key the coordinator would compute for that
 * paragraph right now, or `null` when it is not in the current scan.
 */
export function isFreshResult(result, { expectedKey }) {
  if (!result || typeof result.key !== "string") return false;
  const wanted = expectedKey(result.paragraphId);
  return wanted !== null && wanted === result.key;
}

/**
 * Every finding in one paragraph, in document order, with UTF-16 offsets.
 *
 * This is the whole of the decision — the stages `docs/146` §4 names, minus the
 * ones no increment has built (confusion candidates and consistency are
 * Increment D and C). It is a plain function of its input and its resources so
 * that a test can hold the resources still and vary one thing.
 *
 * O(paragraph length) for a fixed rule set, plus the suggestion scans, which
 * are budgeted by the caller.
 */
export function proofParagraph(input, resources) {
  const text = String(input.text ?? "");
  const locale = input.locale;
  const findings = [];
  const packVersion = resources.packVersion ?? BASIC_PACK_VERSION;

  // ---- Spelling ------------------------------------------------------------
  // Decided from the dictionary ALONE. A locale with no list, or one whose list
  // failed to arrive, contributes no spelling findings — never every word.
  const words = resources.dictionaries.get(locale);
  // The installed language pack for this locale, if one is active (`docs/146`
  // §6, ADR-042). A FOURTH tier, never folded into the glossary — `isKnownWord`'s
  // docstring says why, and it is a difference in lifetime rather than in taste.
  // Absent is `null`, and `null` accepts nothing extra; there is no "empty pack"
  // state, for the same reason there is no empty dictionary.
  const supplement = resources.supplements?.get(locale) ?? null;
  const spellable = input.spelling && words && words !== UNAVAILABLE && resources.glossary;
  if (spellable) {
    for (const token of findMisspellings(text, words.all, {
      personal: resources.personal,
      ignored: resources.ignoredWords,
      glossary: resources.glossary,
      supplement,
    })) {
      const { suggestions, complete } = resources.suggest(token.word, locale, words);
      findings.push({
        kind: "spelling",
        ruleId: "spelling.unknown-word",
        start: token.start,
        end: token.end,
        word: token.word,
        locale,
        message: `“${token.word}” is not in the dictionary`,
        replacements: suggestions.map((word) => ({ text: word })),
        suggestionsComplete: complete,
        confidence: 1,
        packVersion,
      });
    }
  }

  // ---- Grammar -------------------------------------------------------------
  // Independent of every spelling asset, deliberately and by construction: it
  // reads `text` and nothing else. The owner rated grammar above spelling
  // (`docs/114`), and a grammar mark that waits on a word list it never
  // consults is the coupling `docs/146` §2 asked to be removed.
  if (input.grammar && grammarSupports(locale)) {
    for (const issue of findGrammarIssues(text, { ignored: resources.ignoredRules })) {
      findings.push({
        kind: "grammar",
        ruleId: issue.ruleId,
        start: issue.start,
        end: issue.end,
        word: text.slice(issue.start, issue.end),
        locale,
        message: issue.message,
        replacements: issue.replacements.map((replacement) => ({ text: replacement })),
        suggestionsComplete: true,
        confidence: 1,
        packVersion,
      });
    }
  }

  findings.sort((a, b) => a.start - b.start || a.end - b.end);
  return findings;
}

/**
 * The worker's own state machine, as a plain object.
 *
 * `proof_worker.mjs` is a dozen lines of `self.addEventListener` over this, and
 * `spell_check.mjs` calls the very same responder when no `Worker` can be
 * built. That is the point: the fallback is a TRANSPORT, not a second
 * implementation.
 *
 * `handle(message)` returns the reply to post back, or `null` for a message
 * that produces none.
 */
export function createProofResponder() {
  /** locale → parsed dictionary, or `UNAVAILABLE`. Never an empty dictionary. */
  const dictionaries = new Map();
  /** locale → the installed pack's word `Set`. Absent means no pack, which is
   *  not the same as an empty one and is why a locale with no entry has none
   *  rather than an empty `Set`. */
  const supplements = new Map();
  /** word|locale|revision|pack → `{ suggestions, complete }`, bounded LRU. */
  const memo = new Map();
  const resources = {
    dictionaries,
    supplements,
    glossary: null,
    personal: new Set(),
    ignoredWords: new Set(),
    ignoredRules: new Set(),
    packVersion: BASIC_PACK_VERSION,
    settingsRevision: 0,
    /** Deep-scan budget for the check currently running. Reset per `check`. */
    deepBudget: DEEP_SUGGESTION_BUDGET,
    suggest(word, locale, dictionary) {
      // `packVersion` is in the key because an installed pack CHANGES the answer:
      // before it arrives `microservics` has no close neighbour, after it does. A
      // memo keyed only by the word and the settings would serve the pre-install
      // answer for the life of the worker, which is the same defect the result
      // cache had — an answer that was correct when it was computed being served
      // after the resources moved (`docs/146` §2, finding 2).
      const key = `${resources.packVersion}\u0000${locale}\u0000${resources.settingsRevision}\u0000${word}`;
      const hit = memo.get(key);
      if (hit) {
        memo.delete(key);
        memo.set(key, hit);
        return hit;
      }
      // The cheap pass always runs; only the dictionary scan is budgeted, and
      // it is the only part that can cost milliseconds.
      const deep = resources.deepBudget > 0;
      const suggestions = suggestionsFor(word, dictionary, {
        limit: MAX_SUGGESTIONS,
        personal: resources.personal,
        glossary: resources.glossary,
        supplement: supplements.get(locale) ?? null,
        deepScan: deep,
      });
      // "Complete" means the answer is the one a full search would give. A
      // shallow pass that found nothing has NOT established that there is
      // nothing, and the menu must not say so.
      const complete = deep || suggestions.length > 0;
      if (deep) resources.deepBudget -= 1;
      const answer = { suggestions, complete };
      memo.set(key, answer);
      while (memo.size > SUGGESTION_MEMO_LIMIT) memo.delete(memo.keys().next().value);
      return answer;
    },
  };

  function applyResources(message) {
    // The word list crosses the boundary as TEXT and is parsed HERE. That is
    // most of the point of the worker: building an ~84,000-entry `Set` is the
    // single largest piece of main-thread work proofing ever did, and it used
    // to happen in the middle of whatever the reader was doing.
    if (Array.isArray(message.dictionaries)) {
      for (const entry of message.dictionaries) {
        dictionaries.set(
          entry.locale,
          entry.unavailable ? UNAVAILABLE : parseDictionary(entry.text),
        );
      }
    }
    // An installed pack's words, per locale. They cross the boundary as an ARRAY
    // and become a `Set` here, for the same reason the dictionary crosses as text
    // and is parsed here: building the set is the work, and the work belongs off
    // the main thread. `words: null` REMOVES the locale's supplement, which is
    // what uninstalling a pack sends — and removal has to be representable, or a
    // pack could be removed from storage and stay live in the worker for the rest
    // of the session.
    if (Array.isArray(message.supplements)) {
      for (const entry of message.supplements) {
        if (!entry || typeof entry.locale !== "string") continue;
        if (entry.words === null) supplements.delete(entry.locale);
        else supplements.set(entry.locale, new Set(entry.words));
      }
    }
    if (message.glossaryText !== undefined) {
      resources.glossary =
        message.glossaryText === null ? null : parseGlossary(message.glossaryText);
    }
    if (message.personal) resources.personal = new Set(message.personal);
    if (message.ignoredWords) resources.ignoredWords = new Set(message.ignoredWords);
    if (message.ignoredRules) resources.ignoredRules = new Set(message.ignoredRules);
    if (message.packVersion) resources.packVersion = message.packVersion;
    if (typeof message.settingsRevision === "number") {
      resources.settingsRevision = message.settingsRevision;
      // The memo is keyed by the revision, so nothing has to be swept: the old
      // entries fall out of the bounded map on their own.
    }
    return { type: PROOF_MESSAGE.ready, version: PROOF_PROTOCOL_VERSION };
  }

  function runCheck(message) {
    resources.deepBudget = DEEP_SUGGESTION_BUDGET;
    const results = [];
    for (const paragraph of message.paragraphs ?? []) {
      results.push({
        paragraphId: paragraph.paragraphId,
        paragraphRevision: paragraph.paragraphRevision,
        locale: paragraph.locale,
        documentId: message.documentId,
        // Echoed, never interpreted. It is the caller's own identity for the
        // conditions this answer was computed under, and the caller is the only
        // one that can say whether they still hold.
        key: paragraph.key,
        findings: proofParagraph(
          {
            text: paragraph.text,
            locale: paragraph.locale,
            spelling: message.spelling === true,
            grammar: message.grammar === true,
          },
          resources,
        ),
      });
    }
    return {
      type: PROOF_MESSAGE.findings,
      requestId: message.requestId,
      documentId: message.documentId,
      results,
    };
  }

  return {
    handle(message) {
      if (!message || typeof message.type !== "string") return null;
      if (message.type === PROOF_MESSAGE.resources) return applyResources(message);
      if (message.type === PROOF_MESSAGE.check) return runCheck(message);
      return null;
    },
    /** Inspection seam for the guards: what the responder currently holds. */
    state: resources,
  };
}

/**
 * Whether a word would be flagged, for one paragraph, with no message passing.
 *
 * Exported for `document_profile.mjs` in Increment C and for the guards; it is
 * the same question `proofParagraph` asks, without the finding around it.
 */
export function wouldFlag(word, dictionary, resources) {
  if (!dictionary || dictionary === UNAVAILABLE) return false;
  if (skipReason(word)) return false;
  return !isKnownWord(word, dictionary.all, resources);
}

/** Exported so a guard can assert the tokenizer the protocol uses is the one
 *  `spelling.mjs` defines, rather than a second copy. */
export { tokenizeWords };
