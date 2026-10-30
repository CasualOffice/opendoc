// SPDX-License-Identifier: Apache-2.0
// Readability for OpenDoc: the logic (`docs/183` §18). Runs entirely in the
// add-in's sandbox — no network, no model — and proposes its findings as
// comments the person accepts or discards.
//
// What it measures, stated plainly so it is not read as more:
//   * LONG SENTENCES, in any language that separates words with spaces: more
//     than `longWords` words. Not offered for Chinese, Japanese or Thai text.
//   * PASSIVE VOICE and FILLER WORDS, in English only: a form of "to be"
//     followed by a past participle, and a short list of words that rarely
//     add meaning. Heuristics, so each is a comment to consider, never a fix.
//   * THE FLESCH READING EASE and the Flesch-Kincaid grade, for English only:
//     the published formulas over counted sentences, words and syllables,
//     where syllables are estimated from vowel groups.
(function (root) {
  "use strict";

  var BE = "(?:am|is|are|was|were|be|been|being)";
  var IRREGULAR = "(?:built|done|made|given|taken|seen|written|found|held|shown|told|known|sent|kept|left|paid|put|said|sold|thought|brought|bought|caught|taught|read|run|set|cut|hit|led|met|won|chosen|driven|eaten|fallen|forgotten|frozen|hidden|ridden|risen|spoken|stolen|sworn|torn|worn|woken)";
  var PASSIVE = new RegExp("\\b" + BE + "\\s+(?:\\w+ly\\s+)?(?:\\w+ed|" + IRREGULAR + ")\\b", "gi");
  var FILLERS = ["very", "really", "just", "quite", "basically", "actually", "literally", "simply", "totally", "rather"];
  var FILLER = new RegExp("\\b(?:" + FILLERS.join("|") + ")\\b", "gi");
  var CJK = /[぀-ヿ㐀-鿿가-힯฀-๿]/;

  /** Sentences of `text` with their start offsets. */
  function sentences(text) {
    var out = [];
    var re = /[^.!?。！？]+[.!?。！？]*["'”’)\]]*/g;
    var m;
    while ((m = re.exec(text))) {
      var raw = m[0];
      var lead = raw.length - raw.replace(/^\s+/, "").length;
      var s = raw.trim();
      if (s) out.push({ text: s, start: m.index + lead });
    }
    return out;
  }

  function words(text) {
    return String(text).match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu) || [];
  }

  /** Estimated English syllables: vowel groups, a silent final e, at least one. */
  function syllables(word) {
    var w = String(word).toLowerCase().replace(/[^a-z]/g, "");
    if (!w) return 0;
    if (w.length <= 3) return 1;
    w = w.replace(/(?:[^laeiouy]es|ed|[^laeiouy]e)$/, "").replace(/^y/, "");
    var groups = w.match(/[aeiouy]{1,2}/g);
    return Math.max(1, groups ? groups.length : 1);
  }

  /** Flesch reading ease and Flesch-Kincaid grade over paragraphs (English). */
  function scores(texts) {
    var sCount = 0;
    var wCount = 0;
    var syl = 0;
    texts.forEach(function (t) {
      var ss = sentences(t);
      sCount += ss.length;
      ss.forEach(function (s) {
        var w = words(s.text);
        wCount += w.length;
        w.forEach(function (x) { syl += syllables(x); });
      });
    });
    if (!sCount || !wCount) return null;
    var ease = 206.835 - 1.015 * (wCount / sCount) - 84.6 * (syl / wCount);
    var grade = 0.39 * (wCount / sCount) + 11.8 * (syl / wCount) - 15.59;
    return { ease: Math.round(ease), grade: Math.max(0, Math.round(grade * 10) / 10), sentences: sCount, words: wCount };
  }

  /** The 1-based occurrence of `exact` in `text` starting at `offset`, or 0. */
  function occurrenceAt(text, exact, offset) {
    var n = 0;
    for (var at = text.indexOf(exact); at >= 0 && at <= offset; at = text.indexOf(exact, at + 1)) {
      n += 1;
      if (at === offset) return n;
    }
    return 0;
  }

  /**
   * Findings over paragraph blocks: `[{node, kind: "long"|"passive"|"filler",
   * exact, occurrence, words?}]`. English-only kinds run only when `english`.
   */
  function findings(blocks, options) {
    var o = options || {};
    var longWords = o.longWords || 25;
    var english = o.english !== false;
    var out = [];
    (blocks || []).forEach(function (b) {
      if (!b || b.kind !== "paragraph" || typeof b.text !== "string" || !b.text.trim() || b.headingLevel) return;
      var text = b.text;
      if (!CJK.test(text)) {
        sentences(text).forEach(function (s) {
          var n = words(s.text).length;
          if (n > longWords) {
            // Quote the sentence's opening words: enough to anchor, short enough to read.
            var quote = s.text.split(/\s+/).slice(0, 8).join(" ");
            var occ = occurrenceAt(text, quote, s.start);
            if (occ) out.push({ node: b.id, kind: "long", exact: quote, occurrence: occ, words: n });
          }
        });
      }
      if (english) {
        var m;
        PASSIVE.lastIndex = 0;
        while ((m = PASSIVE.exec(text))) {
          var occP = occurrenceAt(text, m[0], m.index);
          if (occP) out.push({ node: b.id, kind: "passive", exact: m[0], occurrence: occP });
        }
        FILLER.lastIndex = 0;
        while ((m = FILLER.exec(text))) {
          var occF = occurrenceAt(text, m[0], m.index);
          if (occF) out.push({ node: b.id, kind: "filler", exact: m[0], occurrence: occF });
        }
      }
    });
    return out;
  }

  /** Findings as comment proposals; `message(finding)` writes each comment. */
  function planForComments(list, revision, message, max) {
    var edits = (list || []).slice(0, max || 50).map(function (f) {
      return { op: "comment", target: { node: f.node, exact: f.exact, occurrence: f.occurrence }, text: message(f) };
    });
    return edits.length ? { ok: true, plan: { base_revision: revision, edits: edits } } : { ok: false, reason: "nothing" };
  }

  root.ReadabilityCore = Object.freeze({
    sentences: sentences,
    words: words,
    syllables: syllables,
    scores: scores,
    findings: findings,
    planForComments: planForComments,
  });
})(typeof self !== "undefined" ? self : globalThis);
