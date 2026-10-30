// SPDX-License-Identifier: Apache-2.0
// Casual AI's panel: the assistant as a first-party plugin on the public plugin
// API (`docs/183` §13, owner decision 3). Everything it does goes through the
// same port any plugin has: it reads with the read tools, asks the configured
// model with `ai/complete` (never seeing the key), and proposes every change
// with `propose_edits`, which opens the editor's private preview. The person
// accepts or discards; an accepted change lands as tracked suggestions by
// "Casual AI", one undo step.
//
// Streaming shows HERE, in the panel, never as jumps in the document (owner
// rule 4). Stop aborts the model call at once.
(function () {
  "use strict";
  var Core = window.CasualAssistantCore;
  var $ = function (id) { return document.getElementById(id); };
  var NS = "plugins.assistant.";
  var KEYS = [
    "disclosure.endpoint", "disclosure.host", "logLabel", "empty", "you", "ai", "prompt.label", "prompt.placeholder",
    "prompt.draftPlaceholder", "send", "stop", "clear", "cleared", "mode.label", "mode.ask", "mode.draft", "chip.summary",
    "chip.keyPoints", "chip.actions", "chip.review", "chip.replies", "working", "reading", "stopped", "proposed",
    "reviewProposed", "dropped", "citation", "insert", "truncated", "noSelection", "noCaret", "unchanged", "unparsable",
    "translateTo", "translateGo", "noComments", "draftReply", "historyNote", "error.generic", "error.auth", "error.rate",
    "error.network", "error.timeout", "error.budget", "error.tooLong", "error.refused", "replyFor", "caveat", "workingOn", "retry",
  ].concat(
    ["ask", "rewrite", "shorten", "expand", "grammar", "formal", "friendly", "confident", "translate", "summarise", "review", "draft"].map(function (id) { return "command." + id; }),
  ).map(function (k) { return NS + k; });
  var LANGUAGES = ["ar", "de", "en", "es", "fr", "hi", "id", "it", "ja", "ko", "nl", "pl", "pt", "ru", "tr", "uk", "vi", "zh"];
  var CONTEXT_BUDGET = 6000;
  var READ_CAP = 60000;

  var editor = null;
  var info = null;
  var strings = {};
  var history = [];
  var running = null;

  function s(key, params) {
    var text = strings[NS + key] || "";
    if (params) Object.keys(params).forEach(function (name) { text = text.split("{" + name + "}").join(String(params[name])); });
    return text;
  }

  // ---- Rendering ----------------------------------------------------------

  function bubble(role, text) {
    $("empty").hidden = true;
    var p = document.createElement("p");
    p.className = "ca-msg " + (role === "user" ? "ca-msg-user" : "ca-msg-ai");
    var who = document.createElement("span");
    who.className = "ca-msg-who";
    who.textContent = role === "user" ? s("you") : s("ai");
    var body = document.createElement("span");
    body.className = "ca-msg-body";
    body.textContent = text || "";
    p.append(who, body);
    $("log").append(p);
    $("log").scrollTop = $("log").scrollHeight;
    return { root: p, body: body };
  }

  function renderAnswer(target, text, ids) {
    var parsed = Core.parseCitations(text, ids);
    target.body.replaceChildren();
    var n = 0;
    parsed.segments.forEach(function (seg) {
      if (seg.text !== undefined) {
        target.body.append(document.createTextNode(seg.text));
        return;
      }
      n += 1;
      var chip = document.createElement("button");
      chip.type = "button";
      chip.className = "ca-cite";
      chip.textContent = seg.cite.replace(/^p/, "¶");
      chip.setAttribute("aria-label", s("citation", { n: seg.cite.slice(1) }));
      chip.title = chip.getAttribute("aria-label");
      chip.addEventListener("click", function () { void editor.reveal(seg.node); });
      target.body.append(chip);
    });
    return parsed;
  }

  function setWorking(text) {
    $("working").hidden = !text;
    $("workingText").textContent = text || "";
    $("send").disabled = !!text;
    Array.prototype.forEach.call(document.querySelectorAll("[data-chip]"), function (b) { b.disabled = !!text; });
  }

  function errorText(err) {
    var code = (err && err.code) || "";
    if (code === "cancelled") return s("stopped");
    if (code === "auth-failed") return s("error.auth");
    if (code === "rate-limited") return s("error.rate");
    if (code === "network") return s("error.network");
    if (code === "timeout") return s("error.timeout");
    if (code === "over-budget") return s("error.budget");
    if (code === "context-too-long") return s("error.tooLong");
    if (code === "content-filtered" || code === "host-refused") return s("error.refused");
    return s("error.generic", { code: code || "error" });
  }

  // ---- Model calls, streamed into a bubble ------------------------------------

  /** One streamed completion into `target`. Resolves with the full text. */
  function complete(body, target, workingKey) {
    var controller = new AbortController();
    running = controller;
    setWorking(s(workingKey || "working"));
    var text = "";
    return editor.ai
      .complete(body, {
        signal: controller.signal,
        onChunk: function (chunk) {
          var delta = chunk && chunk.choices && chunk.choices[0] && chunk.choices[0].delta;
          if (delta && typeof delta.content === "string") {
            text += delta.content;
            if (target) {
              target.body.textContent = text;
              $("log").scrollTop = $("log").scrollHeight;
            }
          }
        },
      })
      .then(function (result) {
        var message = result.response.choices[0].message;
        return typeof message.content === "string" ? message.content : text;
      })
      .finally(function () {
        if (running === controller) running = null;
        setWorking("");
      });
  }

  $("stop").addEventListener("click", function () {
    if (running) running.abort();
  });

  // ---- Reading -------------------------------------------------------------

  /** Paragraph blocks of the body, paged up to READ_CAP tokens. */
  function readAll() {
    var blocks = [];
    var used = 0;
    var revision = 0;
    function page(cursor) {
      var args = { maxTokens: 8000, format: "json" };
      if (cursor) args.cursor = cursor;
      return editor.tools.call("read_blocks", args).then(function (answer) {
        if (!answer || !answer.ok) throw Object.assign(new Error("read"), { code: "read" });
        var value = answer.value;
        revision = value.revision;
        (value.blocks || []).forEach(function (b) {
          blocks.push(b);
          used += Core.estimate(b.text || "");
        });
        if (value.nextCursor && used < READ_CAP) return page(value.nextCursor);
        return { blocks: blocks, revision: revision };
      });
    }
    return page(null);
  }

  function readFrom(node, maxTokens) {
    return editor.tools.call("read_blocks", { start: node, maxTokens: maxTokens || 800, format: "json" }).then(function (answer) {
      if (!answer || !answer.ok) throw Object.assign(new Error("read"), { code: "read" });
      return answer.value;
    });
  }

  function propose(planResult, doneKey, params) {
    if (!planResult.ok) {
      bubble("ai", s(planResult.reason === "unchanged" || planResult.reason === "nothing" ? "unchanged" : "unparsable"));
      return Promise.resolve(null);
    }
    return editor.tools.call("propose_edits", planResult.plan).then(function (answer) {
      var note = s(doneKey || "proposed", params);
      if (planResult.dropped) note += " " + s("dropped", { count: planResult.dropped });
      bubble("ai", note);
      return answer;
    });
  }

  function fail(err, target) {
    var text = errorText(err);
    if (target) {
      target.body.textContent = (target.body.textContent ? target.body.textContent + "\n" : "") + text;
    } else bubble("ai", text);
  }

  // ---- Selection actions ---------------------------------------------------

  function selectionAction(action, language) {
    return editor.selection().then(function (sel) {
      if (!sel || !sel.hasRange || !sel.text.trim()) {
        bubble("ai", s("noSelection"));
        return null;
      }
      var title = actionTitle(action);
      // The panel stays closed (Docs' Refine, Notion's Edit with AI): progress is
      // said in the editor's status line and the result opens in the preview.
      void editor.ui.notify(s("workingOn", { action: title })).catch(function () {});
      bubble("user", title + ": “" + (sel.text.length > 160 ? sel.text.slice(0, 159) + "…" : sel.text) + "”");
      var out = bubble("ai", "");
      if (sel.anchor && sel.focus && sel.anchor.node === sel.focus.node) {
        return readFrom(sel.anchor.node, 1500).then(function (value) {
          var block = (value.blocks || [])[0];
          var start = Math.min(sel.anchor.offset, sel.focus.offset);
          var anchor = block ? Core.anchorFor(block.id, block.text, sel.text, start) : null;
          return complete(Core.selectionRequest(action, sel.text, language), out).then(function (answer) {
            return propose(Core.planForSelection(anchor, answer, value.revision));
          });
        });
      }
      // Across paragraphs: each paragraph the selection touches is rewritten
      // whole, as one JSON answer, then proposed paragraph by paragraph.
      return readFrom(sel.anchor.node, 8000).then(function (value) {
        var paras = [];
        var blocks = value.blocks || [];
        for (var i = 0; i < blocks.length; i += 1) {
          if (blocks[i].kind === "paragraph") paras.push(blocks[i]);
          if (blocks[i].id === sel.focus.node) break;
        }
        out.body.textContent = s("working");
        return complete(Core.paragraphsRequest(action, paras, language), null).then(function (answer) {
          out.body.textContent = "";
          return propose(Core.planForParagraphs(answer, paras, value.revision));
        });
      });
    }).catch(function (err) {
      fail(err);
      void editor.ui.notify(errorText(err), "error").catch(function () {});
    });
  }

  function actionTitle(action) {
    return s("command." + action) || action;
  }

  function languageName(code) {
    try {
      return new Intl.DisplayNames([info.locale || "en"], { type: "language" }).of(code) || code;
    } catch (err) {
      return code;
    }
  }

  function showTranslate() {
    var select = $("translateTo");
    if (!select.options.length) {
      LANGUAGES.forEach(function (code) {
        var o = document.createElement("option");
        o.value = code;
        o.textContent = languageName(code);
        select.append(o);
      });
    }
    var base = String((info && info.locale) || "en").split("-")[0];
    select.value = LANGUAGES.indexOf(base) >= 0 ? base : "en";
    $("translateRow").hidden = false;
    select.focus();
  }

  $("translateGo").addEventListener("click", function () {
    $("translateRow").hidden = true;
    var code = $("translateTo").value;
    void selectionAction("translate", new Intl.DisplayNames(["en"], { type: "language" }).of(code) || code);
  });

  // ---- Ask, summarise, review, draft, reply --------------------------------

  function persist() {
    if (!info || !info.documentKey) return;
    editor.storage.get().then(function (all) {
      var store = all && typeof all === "object" ? all : {};
      store[info.documentKey] = Core.trimHistory(history, 20);
      var keys = Object.keys(store);
      if (keys.length > 30) delete store[keys[0]];
      return editor.storage.set(store);
    }).catch(function () {});
  }

  /** Insert and Try again under an answer, as Gemini's side panel offers. */
  function answerActions(out, text, again) {
    var actions = document.createElement("div");
    actions.className = "ca-msg-actions";
    var insert = document.createElement("button");
    insert.type = "button";
    insert.className = "btn btn-quiet";
    insert.textContent = s("insert");
    insert.addEventListener("click", function () { void insertAtCaret(text.replace(/\s*\[p\d+(?:\s*,\s*p\d+)*\]/g, "")); });
    var retry = document.createElement("button");
    retry.type = "button";
    retry.className = "btn btn-quiet";
    retry.textContent = s("retry");
    retry.addEventListener("click", function () { if (!running) void again(); });
    actions.append(insert, retry);
    out.root.append(actions);
    $("log").scrollTop = $("log").scrollHeight;
  }

  function ask(question) {
    bubble("user", question);
    var out = bubble("ai", "");
    setWorking(s("reading"));
    return readAll().then(function (doc) {
      var context = Core.selectContext(doc.blocks, question, CONTEXT_BUDGET);
      var prior = Core.trimHistory(history, 12);
      return complete(Core.askRequest(question, context, prior), out).then(function (answer) {
        renderAnswer(out, answer, context.ids);
        answerActions(out, answer, function () { return ask(question); });
        if (context.truncated) bubble("ai", s("truncated"));
        history.push({ role: "user", content: question }, { role: "assistant", content: answer.replace(/\[p\d+(?:\s*,\s*p\d+)*\]/g, "") });
        persist();
      });
    }).catch(function (err) { setWorking(""); fail(err, out); });
  }

  function summarise(kind) {
    var label = s("chip." + kind);
    bubble("user", label);
    var out = bubble("ai", "");
    setWorking(s("reading"));
    return readAll().then(function (doc) {
      var context = Core.selectContext(doc.blocks, "", CONTEXT_BUDGET);
      return complete(Core.summaryRequest(kind, context), out).then(function (answer) {
        renderAnswer(out, answer, context.ids);
        answerActions(out, answer, function () { return summarise(kind); });
        if (context.truncated) bubble("ai", s("truncated"));
      });
    }).catch(function (err) { setWorking(""); fail(err, out); });
  }

  function review() {
    bubble("user", s("chip.review"));
    var out = bubble("ai", "");
    setWorking(s("reading"));
    return readAll().then(function (doc) {
      var context = Core.selectContext(doc.blocks, "", 8000);
      var byId = {};
      doc.blocks.forEach(function (b) { if (b.kind === "paragraph") byId[b.id] = b; });
      out.body.textContent = s("working");
      return complete(Core.reviewRequest(context), null).then(function (answer) {
        out.body.textContent = "";
        var result = Core.planForReview(answer, context, byId, doc.revision);
        return propose(result, "reviewProposed", { comments: result.comments || 0, suggestions: result.suggestions || 0 });
      });
    }).catch(function (err) { setWorking(""); fail(err, out); });
  }

  function insertAtCaret(text) {
    return editor.selection().then(function (sel) {
      var node = sel && (sel.focus || sel.anchor) && (sel.focus || sel.anchor).node;
      if (!node) {
        bubble("ai", s("noCaret"));
        return null;
      }
      return readFrom(node, 200).then(function (value) {
        return propose(Core.planForDraft(node, text, value.revision));
      });
    }).catch(function (err) { fail(err); });
  }

  function draft(prompt) {
    bubble("user", prompt);
    var out = bubble("ai", "");
    return editor.selection().then(function (sel) {
      var node = sel && (sel.focus || sel.anchor) && (sel.focus || sel.anchor).node;
      if (!node) {
        out.body.textContent = s("noCaret");
        return null;
      }
      return readFrom(node, 1000).then(function (value) {
        var context = Core.selectContext(value.blocks || [], prompt, 1000);
        return complete(Core.draftRequest(prompt, context), out).then(function (answer) {
          return propose(Core.planForDraft(node, answer, value.revision));
        });
      });
    }).catch(function (err) { fail(err, out); });
  }

  function replies() {
    bubble("user", s("chip.replies"));
    return editor.tools.call("list_comments", { limit: 50 }).then(function (answer) {
      var all = (answer && answer.ok && answer.value.comments) || [];
      var open = all.filter(function (c) { return !c.resolved && !c.parentParaId && c.anchor; });
      if (!open.length) {
        bubble("ai", s("noComments"));
        return null;
      }
      var list = document.createElement("ul");
      list.className = "ca-threads";
      open.slice(0, 20).forEach(function (comment) {
        var item = document.createElement("li");
        item.className = "ca-thread";
        var text = document.createElement("p");
        text.className = "ca-msg";
        text.textContent = (comment.author ? comment.author + ": " : "") + comment.text;
        var go = document.createElement("button");
        go.type = "button";
        go.className = "btn btn-quiet";
        go.textContent = s("draftReply");
        go.addEventListener("click", function () { void draftReply(comment, all); });
        item.append(text, go);
        list.append(item);
      });
      $("empty").hidden = true;
      $("log").append(list);
      return null;
    }).catch(function (err) { fail(err); });
  }

  function draftReply(comment, all) {
    var thread = [comment].concat(all.filter(function (c) { return c.parentParaId && c.parentParaId === comment.paraId; }));
    bubble("user", s("replyFor", { text: comment.text.length > 80 ? comment.text.slice(0, 79) + "…" : comment.text }));
    var out = bubble("ai", "");
    return readFrom(comment.anchor.node, 400).then(function (value) {
      var block = (value.blocks || [])[0];
      var quoted = block ? block.text.slice(comment.anchor.start, comment.anchor.end) : "";
      return complete(Core.replyRequest(thread, quoted), out).then(function (answer) {
        return propose(Core.planForReply(comment, answer, value.revision));
      });
    }).catch(function (err) { fail(err, out); });
  }

  // ---- Wiring --------------------------------------------------------------

  function send() {
    var text = $("prompt").value.trim();
    if (!text || running) return;
    $("prompt").value = "";
    if ($("mode").value === "draft") void draft(text);
    else void ask(text);
  }
  $("send").addEventListener("click", send);
  $("prompt").addEventListener("keydown", function (event) {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      send();
    } else if (event.key === "Escape" && running) {
      event.preventDefault();
      running.abort();
    }
  });
  $("mode").addEventListener("change", function () {
    $("prompt").placeholder = s($("mode").value === "draft" ? "prompt.draftPlaceholder" : "prompt.placeholder");
  });
  Array.prototype.forEach.call(document.querySelectorAll("[data-chip]"), function (button) {
    button.addEventListener("click", function () {
      var chip = button.dataset.chip;
      if (chip === "review") void review();
      else if (chip === "replies") void replies();
      else void summarise(chip);
    });
  });
  $("clear").addEventListener("click", function () {
    history = [];
    $("log").replaceChildren($("empty"));
    $("empty").hidden = false;
    persist();
    bubble("ai", s("cleared"));
  });

  function label() {
    $("log").setAttribute("aria-label", s("logLabel"));
    $("empty").textContent = s("empty");
    $("promptLabel").textContent = s("prompt.label");
    $("prompt").placeholder = s("prompt.placeholder");
    $("modeLabel").textContent = s("mode.label");
    $("modeAsk").textContent = s("mode.ask");
    $("modeDraft").textContent = s("mode.draft");
    $("send").textContent = s("send");
    $("stopLabel").textContent = s("stop");
    $("clear").textContent = s("clear");
    $("historyNote").textContent = s("historyNote");
    $("translateLabel").textContent = s("translateTo");
    $("translateGo").textContent = s("translateGo");
    Array.prototype.forEach.call(document.querySelectorAll("[data-chip]"), function (b) { b.textContent = s("chip." + b.dataset.chip); });
    var p = info.provider || {};
    $("disclosure").textContent = (p.kind === "host" || p.kind === "mcp-sampling" || !p.endpoint ? s("disclosure.host") : s("disclosure.endpoint", { endpoint: p.endpoint })) + " " + s("caveat");
  }

  function restore() {
    history = [];
    $("log").replaceChildren($("empty"));
    $("empty").hidden = false;
    if (!info.documentKey) return Promise.resolve();
    return editor.storage.get().then(function (all) {
      history = Core.trimHistory(all && all[info.documentKey], 20);
      history.forEach(function (h) { bubble(h.role === "user" ? "user" : "ai", h.content); });
    }).catch(function () {});
  }

  window.OpenDocPlugin.connect().then(function (client) {
    editor = client;
    return editor.initialize().then(function (initial) {
      info = initial;
      return editor.strings(KEYS).then(function (table) {
        strings = table || {};
        label();
        return restore();
      });
    }).then(function () {
      editor.on("contribution/invoked", function (params) {
        var command = params && params.command;
        // Ask and Translate need the panel; the host opens it for everything that
        // is not a selection action, so these two open it themselves.
        if (command === "ask" || command === "translate") void editor.ui.panel(true).catch(function () {});
        if (command === "ask" || command === "panel") $("prompt").focus();
        else if (command === "summarise") void summarise("summary");
        else if (command === "review") void review();
        else if (command === "draft") {
          $("mode").value = "draft";
          $("prompt").placeholder = s("prompt.draftPlaceholder");
          $("prompt").focus();
        } else if (command === "translate") showTranslate();
        else if (Core.ACTION_IDS.indexOf(command) >= 0) void selectionAction(command);
      });
      editor.on("document/opened", function (params) {
        info.documentKey = params.documentKey;
        void restore();
      });
      editor.on("theme/changed", function (params) {
        if (params && params.locale && params.locale !== info.locale) info.locale = params.locale;
      });
      window.casualAssistant = { ask: ask, summarise: summarise, review: review, selectionAction: selectionAction, draft: draft, replies: replies };
    });
  });
})();
