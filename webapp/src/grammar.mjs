// Grammar rules, as plain functions over a paragraph's text.
//
// The owner raised grammar above spelling in priority on 2026-09-23, which
// reverses `docs/114` §1's "grammar is a separate row and is not designed
// here". What is here is the SEAM plus a first rule set; `docs/114` §11 says
// exactly which classes of error are and are not covered, and the short answer
// is that everything needing part-of-speech tagging is not.
//
// ## Why these rules and not a rule engine
//
// Two constraints decide it, and both are the project's rather than grammar's.
//
//   * **No new runtime npm dependency.** `webapp/package.json` has zero and
//     adding the first is the owner's decision.
//   * **Apache-2.0-compatible data only.** This is the licence analysis, and it
//     is the reason there is no rule import: **LanguageTool is LGPL-2.1, and so
//     is its rule data** (`grammar.xml` and the n-gram sets ship under the same
//     licence as the engine). After the Deadline is GPL. `retext`/`write-good`
//     are MIT but are npm packages, which the first constraint forbids, and
//     vendoring their word lists would be an import of somebody else's curation
//     rather than a rule. So: the rules below are hand-authored here, are
//     Apache-2.0 like the rest of this repository, and cite the convention they
//     encode rather than a source they copy.
//
// ## Why these rules and not more of them
//
// Every rule here is chosen to be **high precision without a part-of-speech
// tagger**. A grammar checker that cries wolf is worse than none: the user
// stops reading the marks, and then the real ones are invisible too. So a rule
// is only admitted when it can be decided from the literal text — a closed set
// of pronoun/verb pairs, a doubled word, an article before a vowel sound — and
// anything needing to know that a token is a noun is deliberately absent and
// named as absent.
//
// Pure: no DOM, no engine, no dictionary lookup on the hot path. The DOM half
// is in `spell_check.mjs`, which runs these over the SAME windowed scan and the
// same per-paragraph cache the spelling pass uses, so grammar costs no extra
// document walk and nothing on the keystroke path.

import { addressSpans } from "./spelling.mjs";

/** Sentence-ending punctuation, and the closers that may follow it.
 *
 *  The lookbehind excludes a mark that is part of a RUN — an ellipsis trails
 *  off rather than ending a sentence ("Well... maybe" is correct), and `!!` is
 *  the repeated-punctuation rule's business, not this one's. Without it the
 *  capitalisation rule marked the word after every ellipsis in the document. */
const SENTENCE_END = /(?<![.!?])([.!?])(["'’”)\]]*)(\s+)/g;

/** Abbreviations that end in a period and do NOT end a sentence. Closed, short,
 *  and the common ones only: a longer list buys less than it costs, because
 *  every entry is also a chance to MISS a real sentence boundary. */
const NON_TERMINAL_ABBREVIATIONS = new Set([
  "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "vs", "etc", "e.g", "i.e",
  "no", "fig", "vol", "ed", "al", "inc", "ltd", "co", "approx", "dept", "est",
  "min", "max", "ca", "cf", "ibid", "viz", "pp",
]);

/**
 * Pronoun/verb pairs that are wrong in standard written English, and what they
 * should be.
 *
 * A closed table, which is the whole point: real subject–verb agreement needs
 * to know which token is the subject and whether it is plural, and that needs a
 * tagger. A PRONOUN subject needs none — the pronoun is the subject, its number
 * is fixed, and the verb is the next word. That covers the agreement errors
 * people actually make in prose without a single guess.
 *
 * `he don't` and `they was` are flagged because Word flags them; they are
 * grammatical in several dialects and this is a standard-written-English check,
 * which the message says.
 */
const PRONOUN_VERBS = new Map(
  Object.entries({
    "i is": "I am",
    "i are": "I am",
    "i has": "I have",
    "i does": "I do",
    "i were": "I was",
    "he are": "he is",
    "he have": "he has",
    "he do": "he does",
    "he were": "he was",
    "he don't": "he doesn't",
    "she are": "she is",
    "she have": "she has",
    "she do": "she does",
    "she were": "she was",
    "she don't": "she doesn't",
    "it are": "it is",
    "it have": "it has",
    "it do": "it does",
    "it were": "it was",
    "it don't": "it doesn't",
    "they is": "they are",
    "they was": "they were",
    "they has": "they have",
    "they does": "they do",
    "they doesn't": "they don't",
    "we is": "we are",
    "we was": "we were",
    "we has": "we have",
    "we does": "we do",
    "we doesn't": "we don't",
    "you is": "you are",
    "you was": "you were",
    "you has": "you have",
    "you does": "you do",
    "you doesn't": "you don't",
  }),
);

/** `of` after a modal is always a misheard `have`. No ambiguity at all. */
const MODAL_OF = new Set(["could", "would", "should", "must", "might", "may"]);

/** Words whose written form begins with a consonant letter but a VOWEL sound,
 *  and vice versa. The letter test is right the overwhelming majority of the
 *  time; these are the exceptions that make it wrong, and they are the ones
 *  people write. */
const VOWEL_SOUND_EXCEPTIONS = new Set(["hour", "hours", "honest", "honestly", "honour", "honor", "honours", "honors", "heir", "heirs"]);
const CONSONANT_SOUND_EXCEPTIONS = new Set(["one", "once", "university", "universal", "union", "unique", "unit", "united", "user", "users", "usage", "use", "useful", "european", "euro", "ubiquitous", "utility", "utopia"]);

/** A rule's finding. `start`/`end` are JS string indices into the paragraph. */
function finding(ruleId, start, end, message, replacements = []) {
  return { ruleId, start, end, message, replacements };
}

/** Word-shaped tokens with their positions — the same shape `tokenizeWords`
 *  produces, kept local so a grammar rule can also see punctuation. */
function words(text) {
  const out = [];
  for (const match of text.matchAll(/[\p{L}\p{M}][\p{L}\p{M}'’]*/gu)) {
    out.push({ start: match.index, end: match.index + match[0].length, word: match[0] });
  }
  return out;
}

// ---- The rules ---------------------------------------------------------------

/** `the the`, `is is` — a doubled word, the single most common typing error a
 *  spell checker cannot see (both words are spelled correctly). Case-insensitive
 *  on the second word only, so "That that" is flagged but "The The Beatles" is
 *  not treated as two separate mistakes. */
function doubledWord(text) {
  const found = [];
  const tokens = words(text);
  for (let i = 1; i < tokens.length; i += 1) {
    const previous = tokens[i - 1];
    const current = tokens[i];
    if (previous.word.toLowerCase() !== current.word.toLowerCase()) continue;
    // Only when nothing but whitespace separates them: "had, had" is a list.
    if (!/^\s+$/.test(text.slice(previous.end, current.start))) continue;
    // "had had" and "that that" are both grammatical in ordinary English.
    if (["had", "that"].includes(current.word.toLowerCase())) continue;
    found.push(
      finding(
        "doubled-word",
        previous.start,
        current.end,
        `“${current.word}” is repeated`,
        [previous.word],
      ),
    );
  }
  return found;
}

/** `a apple` / `an dog`. */
function articleAgreement(text) {
  const found = [];
  const tokens = words(text);
  for (let i = 0; i + 1 < tokens.length; i += 1) {
    const article = tokens[i].word.toLowerCase();
    if (article !== "a" && article !== "an") continue;
    const next = tokens[i + 1];
    const following = next.word.toLowerCase();
    // Only when the article and the word are separated by plain space; an
    // intervening line break or punctuation means they are not a phrase.
    if (!/^ ?$/.test(text.slice(tokens[i].end, next.start))) continue;
    let vowelSound = /^[aeiou]/.test(following);
    if (VOWEL_SOUND_EXCEPTIONS.has(following)) vowelSound = true;
    if (CONSONANT_SOUND_EXCEPTIONS.has(following)) vowelSound = false;
    // An acronym read letter by letter (FBI, MP, SQL) starts with a vowel SOUND
    // when its first letter is one of these. Only applied to ALL-CAPS tokens,
    // where letter-by-letter reading is the norm.
    if (next.word === next.word.toUpperCase() && next.word.length > 1) {
      vowelSound = /^[AEFHILMNORSX]/.test(next.word);
    }
    const wanted = vowelSound ? "an" : "a";
    if (article === wanted) continue;
    const cased = tokens[i].word[0] === tokens[i].word[0].toUpperCase()
      ? wanted[0].toUpperCase() + wanted.slice(1)
      : wanted;
    found.push(
      finding(
        "article-agreement",
        tokens[i].start,
        tokens[i].end,
        `Use “${cased}” before “${next.word}”`,
        [cased],
      ),
    );
  }
  return found;
}

/** A space before `,` `.` `;` `:` `!` `?`. English only — French puts one
 *  before `;:!?` by convention, which is why the caller gates this by language. */
function spaceBeforePunctuation(text) {
  const found = [];
  for (const match of text.matchAll(/[ \t]+([,.;:!?])/g)) {
    found.push(
      finding(
        "space-before-punctuation",
        match.index,
        match.index + match[0].length,
        `Remove the space before “${match[1]}”`,
        [match[1]],
      ),
    );
  }
  return found;
}

/** A missing space AFTER `,` `;` `:` — and after `.` `!` `?` only between two
 *  letters, so `3.5`, `example.com` and `i.e.` are untouched. */
function missingSpaceAfterPunctuation(text) {
  const found = [];
  // Lookaround rather than capture, for two reasons: the mark then covers the
  // punctuation itself instead of underlining a letter either side of it, and
  // consuming those letters made the matches non-overlapping — in `a,b,c` the
  // first match ate the `b`, so the second comma was never seen.
  for (const match of text.matchAll(/(?<=\p{L})[,;:](?=\p{L})/gu)) {
    found.push(
      finding(
        "missing-space",
        match.index,
        match.index + 1,
        `Add a space after “${match[0]}”`,
        [`${match[0]} `],
      ),
    );
  }
  return found;
}

/** `!!`, `,,`, `??` — repeated punctuation. `...` and `--` are deliberate. */
function repeatedPunctuation(text) {
  const found = [];
  for (const match of text.matchAll(/([,;:!?])\1+/g)) {
    found.push(
      finding(
        "repeated-punctuation",
        match.index,
        match.index + match[0].length,
        `“${match[0]}” repeats a punctuation mark`,
        [match[1]],
      ),
    );
  }
  return found;
}

/** `could of` → `could have`. */
function modalOf(text) {
  const found = [];
  const tokens = words(text);
  for (let i = 0; i + 1 < tokens.length; i += 1) {
    if (!MODAL_OF.has(tokens[i].word.toLowerCase())) continue;
    if (tokens[i + 1].word.toLowerCase() !== "of") continue;
    found.push(
      finding(
        "modal-of",
        tokens[i + 1].start,
        tokens[i + 1].end,
        `“${tokens[i].word} of” should be “${tokens[i].word} have”`,
        ["have"],
      ),
    );
  }
  return found;
}

/** Pronoun–verb agreement, from the closed table above. */
function pronounVerbAgreement(text) {
  const found = [];
  const tokens = words(text);
  for (let i = 0; i + 1 < tokens.length; i += 1) {
    const pair = `${tokens[i].word.toLowerCase()} ${tokens[i + 1].word.toLowerCase()}`;
    const corrected = PRONOUN_VERBS.get(pair);
    if (!corrected) continue;
    if (!/^ ?$/.test(text.slice(tokens[i].end, tokens[i + 1].start))) continue;
    const cased = tokens[i].word[0] === tokens[i].word[0].toUpperCase()
      ? corrected[0].toUpperCase() + corrected.slice(1)
      : corrected;
    found.push(
      finding(
        "subject-verb-agreement",
        tokens[i].start,
        tokens[i + 1].end,
        `“${text.slice(tokens[i].start, tokens[i + 1].end)}” does not agree in standard written English`,
        [cased],
      ),
    );
  }
  return found;
}

/** A lower-case letter starting a new sentence. */
function sentenceCapitalisation(text) {
  const found = [];
  SENTENCE_END.lastIndex = 0;
  for (const match of text.matchAll(SENTENCE_END)) {
    const after = match.index + match[0].length;
    const next = text[after];
    if (!next || next !== next.toLowerCase() || next === next.toUpperCase()) continue;
    // Not after an abbreviation: "Dr. smith" is a different (unhandled) error,
    // and "e.g. this" is correct.
    const before = text.slice(0, match.index);
    // The word before the period, WITH the period the match consumed — so
    // `Dr.` arrives as `dr.` and is compared without it. One comparison, not
    // two: an earlier version also tested the un-stripped form, which no input
    // can reach (every entry is stored without its period) and which a mutation
    // test proved was dead.
    const lastWord = before.match(/[\p{L}.]+$/u)?.[0]?.toLowerCase() ?? "";
    if (NON_TERMINAL_ABBREVIATIONS.has(lastWord.replace(/\.$/, ""))) continue;
    // A single initial ("J. Smith") is not a sentence end either.
    if (/^\p{L}$/u.test(lastWord)) continue;
    const word = text.slice(after).match(/^[\p{L}'’]+/u)?.[0] ?? next;
    const end = after + word.length;
    // The new sentence itself starting with an abbreviation: "…arrived. e.g.
    // this" is correct, and offering to capitalise the `e` of `e.g.` is a
    // nuisance mark on correct prose. Same for an initial: ". J. Smith".
    if (text[end] === "." && (word.length === 1 || NON_TERMINAL_ABBREVIATIONS.has(word.toLowerCase())))
      continue;
    found.push(
      finding(
        "sentence-capitalisation",
        after,
        end,
        "Begin the sentence with a capital letter",
        [next.toUpperCase() + text.slice(after + 1, end)],
      ),
    );
  }
  return found;
}

/** Every rule, in the order they are applied. Exported so a test can assert the
 *  set rather than discovering it, and so `docs/114` §11's table of what IS and
 *  IS NOT covered can be checked against the code. */
export const GRAMMAR_RULES = Object.freeze([
  { id: "doubled-word", run: doubledWord },
  { id: "article-agreement", run: articleAgreement },
  { id: "subject-verb-agreement", run: pronounVerbAgreement },
  { id: "modal-of", run: modalOf },
  { id: "space-before-punctuation", run: spaceBeforePunctuation },
  { id: "missing-space", run: missingSpaceAfterPunctuation },
  { id: "repeated-punctuation", run: repeatedPunctuation },
  { id: "sentence-capitalisation", run: sentenceCapitalisation },
]);

/** The languages these rules are correct for. They encode ENGLISH convention —
 *  `space-before-punctuation` is actively wrong in French — so the caller must
 *  not run them on anything else, and `docs/114` §11 records that as the reason
 *  grammar is English-only for now. */
export function grammarSupports(language) {
  return String(language ?? "").startsWith("en");
}

/**
 * Every grammar finding in one paragraph, in document order.
 *
 * `caretOffset`, when given, suppresses a finding the caret is inside — the
 * same rule spelling uses, and for the same reason: a sentence being typed is
 * not yet a sentence with a mistake in it.
 *
 * O(paragraph length) per rule, with a fixed number of rules, so O(paragraph).
 * Run over the SAME windowed set of paragraphs as the spelling pass.
 */
export function findGrammarIssues(text, { caretOffset = null, ignored } = {}) {
  const source = String(text ?? "");
  const spans = addressSpans(source);
  const inAddress = (from, to) => spans.some(([s, e]) => from < e && to > s);
  const found = [];
  for (const rule of GRAMMAR_RULES) {
    for (const issue of rule.run(source)) {
      if (inAddress(issue.start, issue.end)) continue;
      if (caretOffset !== null && caretOffset >= issue.start && caretOffset <= issue.end) continue;
      if (ignored?.has(issue.ruleId)) continue;
      found.push(issue);
    }
  }
  // Document order, and at most one finding per position: two rules firing on
  // the same span would paint two marks on one phrase.
  found.sort((a, b) => a.start - b.start || a.end - b.end);
  const kept = [];
  for (const issue of found) {
    if (kept.length && issue.start < kept.at(-1).end) continue;
    kept.push(issue);
  }
  return kept;
}
