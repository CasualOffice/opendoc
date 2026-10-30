// SPDX-License-Identifier: Apache-2.0
// LanguageTool in the text (`docs/183` §19): the editor sends the paragraphs a
// person changed; this page asks a LanguageTool server about them and reports
// the matches back. The EDITOR draws them as underlines, opens the card on a
// click and applies a replacement the person clicks. This page has no panel,
// draws nothing and can edit nothing.
(function () {
  "use strict";
  var Core = window.LanguageToolCore;

  var editor = null;
  var info = null;
  var strings = {};
  function s(key, params) {
    var text = strings["plugins.languagetool." + key] || "";
    Object.keys(params || {}).forEach(function (n) { text = text.split("{" + n + "}").join(String(params[n])); });
    return text;
  }
  /** Paragraphs waiting to be checked, by node: the newest text wins. */
  var pending = new Map();
  var running = false;
  /** After a refusal (429 rate limit, a network failure) wait before asking again. */
  var backoffUntil = 0;
  var backoffMs = 0;

  function server() {
    return (info && info.config && typeof info.config.server === "string" && info.config.server) || Core.DEFAULT_SERVER;
  }

  function later(ms) {
    return new Promise(function (resolve) { setTimeout(resolve, ms); });
  }

  /** One request for paragraphs of one language. */
  function check(paragraphs, language) {
    var blocks = paragraphs.map(function (p) { return { kind: "paragraph", id: p.node, text: p.text }; });
    var parts = Core.chunks(blocks, 6000);
    var issues = [];
    return parts.reduce(function (prev, chunk) {
      return prev.then(function () {
        return fetch(Core.checkUrl(server()), {
          method: "POST",
          headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
          body: Core.requestBody(chunk.text, language),
        }).then(function (response) {
          if (!response.ok) throw Object.assign(new Error("server"), { status: response.status });
          return response.json();
        }).then(function (json) {
          issues = issues.concat(Core.issuesFrom(json, chunk).issues);
        });
      });
    }, Promise.resolve()).then(function () {
      return Core.resultsFor(paragraphs, issues);
    });
  }

  /** Drains the queue one language group at a time, never two requests at once. */
  function drain() {
    if (running || !pending.size) return;
    running = true;
    var wait = Math.max(0, backoffUntil - Date.now());
    later(wait).then(function () {
      var batch = Array.from(pending.values());
      pending.clear();
      var groups = {};
      batch.forEach(function (p) {
        var lang = Core.languageFor(p.language);
        (groups[lang] = groups[lang] || []).push(p);
      });
      return Object.keys(groups).reduce(function (prev, lang) {
        return prev.then(function () {
          return check(groups[lang], lang).then(function (results) {
            backoffMs = 0;
            return editor.proofing.report(results);
          });
        });
      }, Promise.resolve()).catch(function (err) {
        // Put the paragraphs back (unless newer text arrived) and wait: the
        // public server allows a few requests a minute.
        batch.forEach(function (p) { if (!pending.has(p.node)) pending.set(p.node, p); });
        backoffMs = Math.min(60000, backoffMs ? backoffMs * 2 : 4000);
        backoffUntil = Date.now() + backoffMs;
        // Said once per failure streak, not on every retry.
        if (err && err.status !== 429 && backoffMs === 4000) {
          var host = new URL(server()).host;
          var message = err.status ? s("error.server", { status: err.status }) : s("error.network", { server: host });
          if (message) void editor.ui.notify(message, "error").catch(function () {});
        }
      });
    }).finally(function () {
      running = false;
      if (pending.size) drain();
    });
  }

  var ready = null;
  window.OpenDocPlugin.connect().then(function (client) {
    editor = client;
    // Listen first: the editor may send the first paragraphs before the
    // handshake below has finished, and a paragraph it sent is not sent again.
    editor.proofing.onCheck(function (paragraphs) {
      paragraphs.forEach(function (p) {
        if (p && typeof p.node === "string" && typeof p.text === "string") pending.set(p.node, p);
      });
      void ready.then(drain);
    });
    ready = editor.initialize().then(function (initial) {
      info = initial;
      return editor.strings(["plugins.languagetool.error.network", "plugins.languagetool.error.server"]);
    }).then(function (table) {
      strings = table || {};
      window.languageToolProofing = { pending: function () { return pending.size; } };
    });
  });
})();
