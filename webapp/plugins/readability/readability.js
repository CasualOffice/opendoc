// SPDX-License-Identifier: Apache-2.0
// Readability's panel (`docs/183` §18): the document's reading-ease score and
// its long sentences, passive voice and filler words, each proposable as a
// comment through the editor's private preview. Signed "Readability (add-in)".
(function () {
  "use strict";
  var Core = window.ReadabilityCore;
  var $ = function (id) { return document.getElementById(id); };
  var NS = "plugins.readability.";
  var KEYS = ["about", "check", "checking", "score", "scoreOther", "none", "found", "commentAll", "comment", "goTo", "proposed",
    "kind.long", "kind.passive", "kind.filler", "note.long", "note.passive", "note.filler", "error.read"].map(function (k) { return NS + k; });

  var editor = null;
  var info = null;
  var strings = {};
  var list = [];
  var revision = 0;

  function s(key, params) {
    var text = strings[NS + key] || "";
    if (params) Object.keys(params).forEach(function (n) { text = text.split("{" + n + "}").join(String(params[n])); });
    return text;
  }

  function note(f) {
    return f.kind === "long" ? s("note.long", { count: f.words }) : s("note." + f.kind, { text: f.exact });
  }

  function propose(items) {
    var result = Core.planForComments(items, revision, note, 50);
    if (!result.ok) return Promise.resolve();
    return editor.tools.call("propose_edits", result.plan).then(function () { $("status").textContent = s("proposed"); });
  }

  function render() {
    var ul = $("findings");
    ul.replaceChildren();
    list.forEach(function (f) {
      var item = document.createElement("li");
      item.className = "pp-item";
      var kind = document.createElement("p");
      kind.className = "pp-item-text";
      kind.textContent = s("kind." + f.kind) + (f.words ? " · " + f.words : "");
      var quote = document.createElement("p");
      quote.className = "pp-item-quote";
      var mark = document.createElement("mark");
      mark.textContent = f.exact;
      quote.append(mark, document.createTextNode(f.kind === "long" ? "…" : ""));
      var actions = document.createElement("div");
      actions.className = "pp-item-actions";
      var comment = document.createElement("button");
      comment.type = "button";
      comment.className = "btn";
      comment.textContent = s("comment");
      comment.addEventListener("click", function () { void propose([f]); });
      var go = document.createElement("button");
      go.type = "button";
      go.className = "btn btn-quiet";
      go.textContent = s("goTo");
      go.addEventListener("click", function () { void editor.reveal(f.node); });
      actions.append(comment, go);
      item.append(kind, quote, actions);
      ul.append(item);
    });
    $("commentAll").hidden = list.length === 0;
    $("status").textContent = list.length ? s("found", { count: list.length }) : s("none");
  }

  function check() {
    $("check").disabled = true;
    $("status").textContent = s("checking");
    var blocks = [];
    function page(cursor) {
      var args = { maxTokens: 8000, format: "json" };
      if (cursor) args.cursor = cursor;
      return editor.tools.call("read_blocks", args).then(function (answer) {
        if (!answer || !answer.ok) throw new Error("read");
        revision = answer.value.revision;
        blocks = blocks.concat(answer.value.blocks || []);
        return answer.value.nextCursor && blocks.length < 4000 ? page(answer.value.nextCursor) : blocks;
      });
    }
    page(null).then(function (all) {
      var english = /^en\b/i.test(String((info && info.locale) || "en"));
      list = Core.findings(all, { english: english });
      var texts = all.filter(function (b) { return b.kind === "paragraph" && !b.headingLevel && b.text; }).map(function (b) { return b.text; });
      var sc = english ? Core.scores(texts) : null;
      $("score").textContent = sc ? s("score", { ease: sc.ease, grade: sc.grade }) : s("scoreOther");
      render();
    }).catch(function () {
      $("status").textContent = s("error.read");
    }).finally(function () {
      $("check").disabled = false;
    });
  }

  $("check").addEventListener("click", check);
  $("commentAll").addEventListener("click", function () { void propose(list); });

  window.OpenDocPlugin.connect().then(function (client) {
    editor = client;
    return editor.initialize().then(function (initial) {
      info = initial;
      return editor.strings(KEYS);
    }).then(function (table) {
      strings = table || {};
      $("about").textContent = s("about");
      $("check").textContent = s("check");
      $("commentAll").textContent = s("commentAll");
      editor.on("contribution/invoked", function (params) {
        if (params && params.command === "check") check();
      });
      window.readabilityPanel = { check: check };
    });
  });
})();
