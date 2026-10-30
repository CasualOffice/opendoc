// SPDX-License-Identifier: Apache-2.0
// LanguageTool's panel (`docs/183` §18): check the document against a
// LanguageTool server and propose each fix as a suggestion through the
// editor's private preview. Signed "LanguageTool (add-in)" by the engine.
(function () {
  "use strict";
  var Core = window.LanguageToolCore;
  var $ = function (id) { return document.getElementById(id); };
  var NS = "plugins.languagetool.";
  var KEYS = ["disclosure", "language", "auto", "check", "checking", "none", "found", "dropped", "fixAll", "suggest", "noReplacement",
    "proposed", "error.network", "error.server", "error.read", "goTo"].map(function (k) { return NS + k; });
  var LANGUAGES = ["en-US", "en-GB", "de-DE", "fr", "es", "it", "nl", "pl-PL", "pt-PT", "pt-BR", "ru-RU", "uk-UA", "ca-ES", "sv", "da-DK", "ja-JP", "zh-CN"];

  var editor = null;
  var info = null;
  var strings = {};
  var issues = [];
  var revision = 0;
  var busy = null;

  function s(key, params) {
    var text = strings[NS + key] || "";
    if (params) Object.keys(params).forEach(function (n) { text = text.split("{" + n + "}").join(String(params[n])); });
    return text;
  }

  function server() {
    return (info && info.config && typeof info.config.server === "string" && info.config.server) || Core.DEFAULT_SERVER;
  }

  function button(label, onClick, primary) {
    var b = document.createElement("button");
    b.type = "button";
    b.className = primary ? "btn btn-primary" : "btn";
    b.textContent = label;
    b.addEventListener("click", onClick);
    return b;
  }

  function propose(edits) {
    if (!edits.length) return Promise.resolve();
    return editor.tools.call("propose_edits", { base_revision: revision, edits: edits }).then(function () {
      $("status").textContent = s("proposed");
    });
  }

  function render(dropped) {
    var list = $("issues");
    list.replaceChildren();
    issues.forEach(function (issue) {
      var item = document.createElement("li");
      item.className = "pp-item";
      var text = document.createElement("p");
      text.className = "pp-item-text";
      text.textContent = issue.message;
      var quote = document.createElement("p");
      quote.className = "pp-item-quote";
      var mark = document.createElement("mark");
      mark.textContent = issue.exact;
      quote.append(mark);
      var actions = document.createElement("div");
      actions.className = "pp-item-actions";
      issue.replacements.slice(0, 3).forEach(function (r) {
        var b = button(r || "∅", function () { void propose([Core.editFor(issue, r)]); });
        b.setAttribute("aria-label", s("suggest", { text: r }));
        actions.append(b);
      });
      if (!issue.replacements.length) {
        var none = document.createElement("span");
        none.className = "note";
        none.textContent = s("noReplacement");
        actions.append(none);
      }
      var go = button(s("goTo"), function () { void editor.reveal(issue.node); });
      go.className = "btn btn-quiet";
      actions.append(go);
      item.append(text, quote, actions);
      list.append(item);
    });
    $("fixAll").hidden = !issues.some(function (i) { return i.replacements.length; });
    var status = issues.length ? s("found", { count: issues.length }) : s("none");
    if (dropped) status += " " + s("dropped", { count: dropped });
    $("status").textContent = status;
  }

  function check() {
    if (busy) return;
    busy = new AbortController();
    var signal = busy.signal;
    $("check").disabled = true;
    $("status").textContent = s("checking");
    var blocks = [];
    function page(cursor) {
      var args = { maxTokens: 8000, format: "json" };
      if (cursor) args.cursor = cursor;
      return editor.tools.call("read_blocks", args).then(function (answer) {
        if (!answer || !answer.ok) throw Object.assign(new Error("read"), { code: "read" });
        revision = answer.value.revision;
        blocks = blocks.concat(answer.value.blocks || []);
        return answer.value.nextCursor && blocks.length < 4000 ? page(answer.value.nextCursor) : blocks;
      });
    }
    page(null)
      .then(function (all) {
        var parts = Core.chunks(all, 6000);
        var found = [];
        var dropped = 0;
        return parts.reduce(function (prev, chunk) {
          return prev.then(function () {
            return fetch(Core.checkUrl(server()), {
              method: "POST",
              headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
              body: Core.requestBody(chunk.text, $("language").value),
              signal: signal,
            }).then(function (response) {
              if (!response.ok) throw Object.assign(new Error("server"), { code: "server", status: response.status });
              return response.json();
            }).then(function (json) {
              var mapped = Core.issuesFrom(json, chunk);
              found = found.concat(mapped.issues);
              dropped += mapped.dropped;
            });
          });
        }, Promise.resolve()).then(function () {
          issues = found;
          render(dropped);
        });
      })
      .catch(function (err) {
        var code = err && err.code;
        $("status").textContent = code === "read" ? s("error.read") : code === "server" ? s("error.server", { status: err.status }) : s("error.network", { server: new URL(server()).host });
      })
      .finally(function () {
        busy = null;
        $("check").disabled = false;
      });
  }

  $("check").addEventListener("click", check);
  $("fixAll").addEventListener("click", function () {
    var result = Core.planForAll(issues, revision, 100);
    if (result.ok) void propose(result.plan.edits);
  });

  window.OpenDocPlugin.connect().then(function (client) {
    editor = client;
    return editor.initialize().then(function (initial) {
      info = initial;
      return editor.strings(KEYS);
    }).then(function (table) {
      strings = table || {};
      $("disclosure").textContent = s("disclosure", { server: new URL(server()).host });
      $("languageLabel").textContent = s("language");
      $("check").textContent = s("check");
      $("fixAll").textContent = s("fixAll");
      var select = $("language");
      var auto = document.createElement("option");
      auto.value = "auto";
      auto.textContent = s("auto");
      select.append(auto);
      var names;
      try { names = new Intl.DisplayNames([info.locale || "en"], { type: "language" }); } catch (err) { names = null; }
      LANGUAGES.forEach(function (code) {
        var o = document.createElement("option");
        o.value = code;
        o.textContent = names ? names.of(code) : code;
        select.append(o);
      });
      editor.on("contribution/invoked", function (params) {
        if (params && params.command === "check") check();
      });
      window.languageToolPanel = { check: check };
    });
  });
})();
