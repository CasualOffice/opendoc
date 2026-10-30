// SPDX-License-Identifier: Apache-2.0
// Readability in the text (`docs/183` §19): long sentences, passive voice and
// filler words found in the paragraphs the editor sends, reported back as
// writing issues the EDITOR highlights in the text, Hemingway-style. The
// reading-ease score rides along as the report's summary (the status count's
// tooltip). Runs entirely in this sandbox; nothing leaves the device.
(function () {
  "use strict";
  var Core = window.ReadabilityCore;
  var NS = "plugins.readability.";
  var KEYS = ["score", "note.long", "note.passive", "note.filler"].map(function (k) { return NS + k; });

  var editor = null;
  var info = null;
  var strings = {};
  /** node → text, every paragraph seen, for the document's score. */
  var texts = new Map();

  function s(key, params) {
    var text = strings[NS + key] || "";
    if (params) Object.keys(params).forEach(function (n) { text = text.split("{" + n + "}").join(String(params[n])); });
    return text;
  }

  function englishFor(tag) {
    return /^en\b/i.test(String(tag || (info && info.locale) || "en"));
  }

  function message(match, text) {
    if (match.kind === "long") return s("note.long", { count: match.words });
    return s("note." + match.kind, { text: text.slice(match.start, match.end) });
  }

  function summary() {
    var all = Array.from(texts.values());
    if (!all.length) return "";
    var sc = Core.scores(all);
    return sc ? s("score", { ease: sc.ease, grade: sc.grade }) : "";
  }

  function onCheck(paragraphs) {
    var results = paragraphs.map(function (p) {
      var english = englishFor(p.language);
      if (english) texts.set(p.node, p.text);
      else texts.delete(p.node);
      while (texts.size > 4000) texts.delete(texts.keys().next().value);
      return {
        node: p.node,
        text: p.text,
        matches: Core.matchesIn(p.text, { english: english }).map(function (m) {
          return { start: m.start, end: m.end, category: m.kind, message: message(m, p.text), rule: "readability." + m.kind };
        }),
      };
    });
    void editor.proofing.report(results, summary());
  }

  var ready = null;
  window.OpenDocPlugin.connect().then(function (client) {
    editor = client;
    editor.proofing.onCheck(function (paragraphs) { void ready.then(function () { onCheck(paragraphs); }); });
    ready = editor.initialize().then(function (initial) {
      info = initial;
      return editor.strings(KEYS);
    }).then(function (table) {
      strings = table || {};
    });
  });
})();
