// SPDX-License-Identifier: Apache-2.0
// Casual AI's panel: the assistant as a first-party plugin on the public plugin
// API (`docs/183` §13, §21, owner decision 3). Everything it does goes through
// the same port any plugin has: it reads with the read tools, asks the configured
// model with `ai/complete` (never seeing the key), and proposes every change
// with `propose_edits`, which opens the editor's private preview. The person
// accepts or discards; an accepted change lands as tracked suggestions by
// "Casual AI", one undo step.
//
// THE PANEL (owner review, 2026-10-31: "too crowded, redundant information, too
// narrow, we need something tangible that basically works"). Designed from
// Gemini's side panel in Google Docs and Copilot's in Word:
//
//   * the conversation fills the body, with four suggestions while it is empty;
//   * ONE composer at the bottom: a growing text field, Send that becomes Stop
//     while the model writes, and a chip naming the selection only while there
//     is one (so a question can be about it, and its answer can replace it);
//   * every answer is a CARD with one clear primary action that changes the
//     document — Insert at the cursor, Replace selection, Add as comment, or
//     Apply as suggestions — plus Copy and Try again. A replacement or a review
//     shows a compact inline diff in the card before anything is applied;
//   * what is not the conversation (Clear history, where text goes, the
//     provider settings) is behind the editor's ⋯ in the header line, and the
//     endpoint is said once, on first use, not on every open.
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
    "prompt.draftPlaceholder", "send", "stop", "cleared", "chip.summary", "chip.keyPoints", "chip.draft", "chip.review",
    "working", "reading", "stopped", "proposed", "dropped", "citation", "insert", "replace", "comment", "apply", "copy",
    "copied", "truncated", "noSelection", "noCaret", "unchanged", "unparsable", "translateTo", "translateGo", "noComments",
    "draftReply", "historyNote", "error.generic", "error.auth", "error.rate", "error.network", "error.timeout",
    "error.budget", "error.tooLong", "error.refused", "replyFor", "caveat", "workingOn", "retry", "attached", "detach",
    "draftMode", "draftCancel", "reviewSummary", "reviewProposed", "diff.inserted", "diff.deleted",
  ].concat(
    ["ask", "rewrite", "shorten", "expand", "grammar", "formal", "friendly", "confident", "translate", "summarise", "review", "draft", "replies"].map(function (id) { return "command." + id; }),
  ).map(function (k) { return NS + k; });
  var LANGUAGES = ["ar", "de", "en", "es", "fr", "hi", "id", "it", "ja", "ko", "nl", "pl", "pt", "ru", "tr", "uk", "vi", "zh"];
  var CONTEXT_BUDGET = 6000;
  var READ_CAP = 60000;
  /** The storage key that remembers the first-run note was shown. Not a
   *  document key (those are 8 hex digits), so eviction never takes it. */
  var SEEN = "~seen";

  var editor = null;
  var info = null;
  var strings = {};
  var history = [];
  var running = null;
  /** The selection the composer's chip names: `{text, anchor, focus}`, or null. */
  var attached = null;
  var mode = "ask";
  var seenNote = true;

  function s(key, params) {
    var text = strings[NS + key] || "";
    if (params) Object.keys(params).forEach(function (name) { text = text.split("{" + name + "}").join(String(params[name])); });
    return text;
  }
  var clip = function (text, max) { return text.length > max ? text.slice(0, max - 1) + "…" : text; };

  // ---- Rendering ----------------------------------------------------------

  function scrollDown() { $("log").scrollTop = $("log").scrollHeight; }

  /** The empty state goes once the conversation starts. */
  function started() {
    $("empty").hidden = true;
  }

  function userMessage(text) {
    started();
    var p = document.createElement("p");
    p.className = "ca-msg ca-msg-user";
    var who = document.createElement("span");
    who.className = "sr-only";
    who.textContent = s("you") + ": ";
    p.append(who, document.createTextNode(text || ""));
    $("log").append(p);
    scrollDown();
    return p;
  }

  /** An answer card: body (the streamed text), extra (a diff, a review list),
   *  note (what happened), actions. */
  function card() {
    started();
    var root = document.createElement("article");
    root.className = "ca-card ca-msg-ai";
    root.setAttribute("aria-label", s("ai"));
    var body = document.createElement("p");
    body.className = "ca-card-body";
    var extra = document.createElement("div");
    extra.className = "ca-card-extra";
    extra.hidden = true;
    var note = document.createElement("p");
    note.className = "ca-card-note";
    var actions = document.createElement("div");
    actions.className = "ca-card-actions";
    actions.hidden = true;
    root.append(body, extra, note, actions);
    $("log").append(root);
    scrollDown();
    return { root: root, body: body, extra: extra, note: note, actions: actions };
  }

  function renderAnswer(target, text, ids) {
    var parsed = Core.parseCitations(text, ids);
    target.body.replaceChildren();
    parsed.segments.forEach(function (seg) {
      if (seg.text !== undefined) {
        target.body.append(document.createTextNode(seg.text));
        return;
      }
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

  /** The compact inline diff: struck deletions, underlined insertions, each
   *  also said aloud (screen readers do not reliably announce ins/del). */
  function diffView(before, after) {
    var p = document.createElement("p");
    p.className = "ca-diff";
    Core.wordDiff(before, after).forEach(function (seg) {
      if (seg.kind === "same") {
        p.append(document.createTextNode(seg.text));
        return;
      }
      var mark = document.createElement(seg.kind);
      var said = document.createElement("span");
      said.className = "sr-only";
      said.textContent = s(seg.kind === "ins" ? "diff.inserted" : "diff.deleted") + " ";
      mark.append(said, document.createTextNode(seg.text));
      p.append(mark);
    });
    return p;
  }

  function button(label, className, icon) {
    var b = document.createElement("button");
    b.type = "button";
    b.className = className;
    if (icon) {
      var glyph = document.createElement("span");
      glyph.className = "ms";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = icon;
      b.append(glyph);
      b.setAttribute("aria-label", label);
      b.title = label;
    } else b.textContent = label;
    return b;
  }

  /**
   * The card's actions: `doc` is the ordered list of document actions (the first
   * is the primary), each `{id, label, run}`; `copy` is the text Copy writes;
   * `again` re-runs the turn in place.
   */
  function setActions(target, doc, copy, again) {
    var row = target.actions;
    row.replaceChildren();
    doc.forEach(function (action, index) {
      var b = button(action.label, index === 0 ? "btn btn-primary" : "btn");
      b.dataset.action = action.id;
      b.addEventListener("click", function () {
        if (running) return;
        Array.prototype.forEach.call(row.querySelectorAll("[data-action]"), function (x) { x.disabled = true; });
        Promise.resolve()
          .then(action.run)
          .then(function (note) { if (note) target.note.textContent = note; })
          .catch(function (err) { target.note.textContent = errorText(err); })
          .finally(function () { Array.prototype.forEach.call(row.querySelectorAll("[data-action]"), function (x) { x.disabled = false; }); });
      });
      row.append(b);
    });
    var spacer = document.createElement("span");
    spacer.className = "ca-spacer";
    row.append(spacer);
    if (copy) {
      var c = button(s("copy"), "btn btn-quiet ca-icon-btn", "content_copy");
      c.dataset.action = "copy";
      c.addEventListener("click", function () {
        editor.ui.copy(copy).then(function (ok) {
          if (!ok) return;
          c.firstChild.textContent = "check";
          c.setAttribute("aria-label", s("copied"));
          $("working").textContent = s("copied");
          setTimeout(function () {
            c.firstChild.textContent = "content_copy";
            c.setAttribute("aria-label", s("copy"));
          }, 1500);
        }).catch(function () {});
      });
      row.append(c);
    }
    if (again) {
      var r = button(s("retry"), "btn btn-quiet ca-icon-btn", "refresh");
      r.dataset.action = "retry";
      r.addEventListener("click", function () {
        if (running) return;
        target.root.remove();
        void again();
      });
      row.append(r);
    }
    row.hidden = false;
    scrollDown();
  }

  function setWorking(text) {
    var busy = !!text;
    $("working").textContent = text || "";
    $("send").hidden = busy;
    $("stop").hidden = !busy;
    Array.prototype.forEach.call(document.querySelectorAll("[data-chip], .ca-card-actions [data-action]"), function (b) { b.disabled = busy; });
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

  function fail(err, target) {
    var text = errorText(err);
    target.body.classList.remove("ca-pending");
    target.note.textContent = text;
  }

  // ---- Model calls, streamed into a card ------------------------------------

  /** One streamed completion into `target.body`. Resolves with the full text. */
  function complete(body, target, workingKey) {
    var controller = new AbortController();
    running = controller;
    setWorking(s(workingKey || "working"));
    var text = "";
    if (target) target.body.classList.add("ca-pending");
    return editor.ai
      .complete(body, {
        signal: controller.signal,
        onChunk: function (chunk) {
          var delta = chunk && chunk.choices && chunk.choices[0] && chunk.choices[0].delta;
          if (delta && typeof delta.content === "string") {
            text += delta.content;
            if (target) {
              target.body.textContent = text;
              scrollDown();
            }
          }
        },
      })
      .then(function (result) {
        var message = result.response.choices[0].message;
        return typeof message.content === "string" ? message.content : text;
      })
      .finally(function () {
        if (target) target.body.classList.remove("ca-pending");
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

  /** Sends a plan to the private preview. Resolves with the card's note. */
  function propose(planResult, doneKey, params) {
    if (!planResult.ok) {
      return Promise.resolve(s(planResult.reason === "unchanged" || planResult.reason === "nothing" ? "unchanged" : "unparsable"));
    }
    return editor.tools.call("propose_edits", planResult.plan).then(function (answer) {
      if (answer && answer.ok === false) return errorText({ code: (answer.refusal && answer.refusal.code) || (answer.error && answer.error.code) || "refused" });
      var note = s(doneKey || "proposed", params);
      if (planResult.dropped) note += " " + s("dropped", { count: planResult.dropped });
      return note;
    });
  }

  // ---- The document actions an answer card offers ----------------------------

  /** Where the cursor is now: `{node, offset}` (the end of a range in one
   *  paragraph, so an insert never lands inside the selected text). */
  function caret() {
    return editor.selection().then(function (sel) {
      var at = sel && (sel.focus || sel.anchor);
      if (!at || !at.node) return null;
      var offset = at.offset;
      if (sel.hasRange && sel.anchor && sel.focus && sel.anchor.node === sel.focus.node) offset = Math.max(sel.anchor.offset, sel.focus.offset);
      return { node: at.node, offset: offset };
    });
  }

  function insertAtCaret(text) {
    return caret().then(function (at) {
      if (!at) return s("noCaret");
      return readFrom(at.node, 200).then(function (value) { return propose(Core.planForInsert(at, text, value.revision)); });
    });
  }

  /** The anchor for a remembered one-paragraph selection, re-found in the
   *  paragraph as it is NOW (the person may have typed since). */
  function anchorOf(sel) {
    return readFrom(sel.anchor.node, 1500).then(function (value) {
      var block = (value.blocks || [])[0];
      var start = Math.min(sel.anchor.offset, sel.focus.offset);
      return { anchor: block ? Core.anchorFor(block.id, block.text, sel.text, start) : null, revision: value.revision };
    });
  }

  function replaceSelection(sel, text) {
    return anchorOf(sel).then(function (found) {
      if (!found.anchor) return s("noSelection");
      return propose(Core.planForSelection(found.anchor, Core.stripCitations(text), found.revision));
    });
  }

  function addComment(sel, text) {
    if (sel && singleParagraph(sel)) {
      return anchorOf(sel).then(function (found) {
        if (!found.anchor) return s("noSelection");
        return propose(Core.planForComment(found.anchor, text, found.revision));
      });
    }
    return caret().then(function (at) {
      if (!at) return s("noCaret");
      return readFrom(at.node, 200).then(function (value) { return propose(Core.planForComment({ node: at.node }, text, value.revision)); });
    });
  }

  var singleParagraph = function (sel) { return !!(sel && sel.anchor && sel.focus && sel.anchor.node === sel.focus.node); };

  /** The actions for an answer to a question, a summary or a draft. */
  function answerActions(target, answer, sel, again) {
    var plain = Core.cleanText(Core.stripCitations(answer));
    var insert = { id: "insert", label: s("insert"), run: function () { return insertAtCaret(answer); } };
    var comment = { id: "comment", label: s("comment"), run: function () { return addComment(sel, answer); } };
    var list = [insert, comment];
    if (sel && singleParagraph(sel)) {
      var replace = { id: "replace", label: s("replace"), run: function () { return replaceSelection(sel, answer); } };
      if (Core.looksLikeRevision(sel.text, answer)) {
        // A revision of the selection: shown as a diff, replaced first.
        target.body.hidden = true;
        target.extra.replaceChildren(diffView(sel.text, plain));
        target.extra.hidden = false;
        list = [replace, comment];
      } else list = [comment, replace];
    }
    setActions(target, list, plain, again);
  }

  // ---- Selection actions (the ✦ menu) ---------------------------------------

  function selectionAction(action, language) {
    return editor.selection().then(function (sel) {
      if (!sel || !sel.hasRange || !sel.text.trim()) {
        card().note.textContent = s("noSelection");
        return null;
      }
      var title = actionTitle(action);
      // The panel stays closed (Docs' Refine, Notion's Edit with AI): progress is
      // said in the editor's status line and the result opens in the preview.
      void editor.ui.notify(s("workingOn", { action: title })).catch(function () {});
      userMessage(title + ": “" + clip(sel.text, 160) + "”");
      var out = card();
      if (singleParagraph(sel)) {
        return anchorOf(sel).then(function (found) {
          return complete(Core.selectionRequest(action, sel.text, language), out).then(function (answer) {
            var text = Core.cleanText(answer);
            out.body.hidden = true;
            out.extra.replaceChildren(diffView(sel.text, text));
            out.extra.hidden = false;
            var replace = { id: "replace", label: s("replace"), run: function () { return propose(Core.planForSelection(found.anchor, answer, found.revision)); } };
            setActions(out, [replace], text, function () { return selectionAction(action, language); });
            return replace.run().then(function (note) { out.note.textContent = note; });
          });
        }).catch(function (err) { fail(err, out); notifyError(err); });
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
          var result = Core.planForParagraphs(answer, paras, value.revision);
          var apply = { id: "apply", label: s("apply"), run: function () { return propose(result); } };
          setActions(out, [apply], null, function () { return selectionAction(action, language); });
          return apply.run().then(function (note) { out.note.textContent = note; });
        });
      }).catch(function (err) { fail(err, out); notifyError(err); });
    });
  }

  function notifyError(err) {
    void editor.ui.notify(errorText(err), "error").catch(function () {});
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
      var keys = Object.keys(store).filter(function (k) { return k !== SEEN; });
      if (keys.length > 30) delete store[keys[0]];
      if (seenNote) store[SEEN] = true;
      return editor.storage.set(store);
    }).catch(function () {});
  }

  function ask(question, sel, retry) {
    if (!retry) userMessage(question);
    var out = card();
    setWorking(s("reading"));
    return readAll().then(function (doc) {
      var context = Core.selectContext(doc.blocks, question + (sel ? " " + sel.text : ""), CONTEXT_BUDGET);
      var prior = Core.trimHistory(history, 12);
      return complete(Core.askRequest(question, context, prior, sel ? sel.text : null), out).then(function (answer) {
        renderAnswer(out, answer, context.ids);
        if (context.truncated) out.note.textContent = s("truncated");
        answerActions(out, answer, sel, function () { return ask(question, sel, true); });
        if (!retry) history.push({ role: "user", content: question }, { role: "assistant", content: Core.stripCitations(answer) });
        persist();
      });
    }).catch(function (err) { setWorking(""); fail(err, out); });
  }

  function summarise(kind, retry) {
    if (!retry) userMessage(s("chip." + kind));
    var out = card();
    setWorking(s("reading"));
    return readAll().then(function (doc) {
      var context = Core.selectContext(doc.blocks, "", CONTEXT_BUDGET);
      return complete(Core.summaryRequest(kind, context), out).then(function (answer) {
        renderAnswer(out, answer, context.ids);
        if (context.truncated) out.note.textContent = s("truncated");
        answerActions(out, answer, null, function () { return summarise(kind, true); });
        // Part of the discussion a later "write a paragraph from this" draws on.
        if (!retry) history.push({ role: "user", content: s("chip." + kind) }, { role: "assistant", content: Core.stripCitations(answer) });
        persist();
      });
    }).catch(function (err) { setWorking(""); fail(err, out); });
  }

  function review(retry) {
    if (!retry) userMessage(s("chip.review"));
    var out = card();
    setWorking(s("reading"));
    return readAll().then(function (doc) {
      var context = Core.selectContext(doc.blocks, "", 8000);
      var byId = {};
      doc.blocks.forEach(function (b) { if (b.kind === "paragraph") byId[b.id] = b; });
      return complete(Core.reviewRequest(context), out).then(function (answer) {
        out.body.textContent = "";
        var result = Core.planForReview(answer, context, byId, doc.revision);
        if (!result.ok) {
          out.note.textContent = s(result.reason === "nothing" ? "unchanged" : "unparsable");
          setActions(out, [], null, function () { return review(true); });
          return;
        }
        // Each finding, shown before anything is applied: the suggested change
        // as a diff, or the quoted text, and the comment under it.
        var list = document.createElement("ul");
        list.className = "ca-review";
        var copy = [];
        result.items.forEach(function (item) {
          var li = document.createElement("li");
          li.className = "ca-review-item";
          if (item.replacement) li.append(diffView(item.quote, item.replacement));
          else {
            var quote = document.createElement("p");
            quote.className = "ca-diff";
            quote.textContent = item.quote;
            li.append(quote);
          }
          var note = document.createElement("p");
          note.className = "ca-review-comment";
          var icon = document.createElement("span");
          icon.className = "ms";
          icon.setAttribute("aria-hidden", "true");
          icon.textContent = "comment";
          note.append(icon, document.createTextNode(" " + item.comment));
          li.append(note);
          list.append(li);
          copy.push("“" + item.quote + "”" + (item.replacement ? " → “" + item.replacement + "”" : "") + "\n" + item.comment);
        });
        out.extra.replaceChildren(list);
        out.extra.hidden = false;
        var summary = s("reviewSummary", { comments: result.comments || 0, suggestions: result.suggestions || 0 });
        if (result.dropped) summary += " " + s("dropped", { count: result.dropped });
        out.note.textContent = summary;
        var counts = { comments: result.comments || 0, suggestions: result.suggestions || 0 };
        var apply = { id: "apply", label: s("apply"), run: function () { return propose(result, "reviewProposed", counts); } };
        setActions(out, [apply], copy.join("\n\n"), function () { return review(true); });
      });
    }).catch(function (err) { setWorking(""); fail(err, out); });
  }

  function draft(prompt, retry) {
    if (!retry) userMessage(prompt);
    var out = card();
    return caret().then(function (at) {
      if (!at) {
        out.note.textContent = s("noCaret");
        return null;
      }
      return readFrom(at.node, 1000).then(function (value) {
        var context = Core.selectContext(value.blocks || [], prompt, 1000);
        return complete(Core.draftRequest(prompt, context, Core.trimHistory(history, 12)), out).then(function (answer) {
          var insert = { id: "insert", label: s("insert"), run: function () { return insertAtCaret(answer); } };
          setActions(out, [insert], Core.cleanText(answer), function () { return draft(prompt, true); });
          if (!retry) history.push({ role: "user", content: prompt }, { role: "assistant", content: Core.cleanText(answer) });
          persist();
        });
      });
    }).catch(function (err) { fail(err, out); });
  }

  function replies() {
    userMessage(s("command.replies"));
    var out = card();
    return editor.tools.call("list_comments", { limit: 50 }).then(function (answer) {
      var all = (answer && answer.ok && answer.value.comments) || [];
      var open = all.filter(function (c) { return !c.resolved && !c.parentParaId && c.anchor; });
      if (!open.length) {
        out.note.textContent = s("noComments");
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
        var go = button(s("draftReply"), "btn btn-quiet");
        go.addEventListener("click", function () { if (!running) void draftReply(comment, all); });
        item.append(text, go);
        list.append(item);
      });
      out.extra.replaceChildren(list);
      out.extra.hidden = false;
      return null;
    }).catch(function (err) { fail(err, out); });
  }

  function draftReply(comment, all, retry) {
    var thread = [comment].concat(all.filter(function (c) { return c.parentParaId && c.parentParaId === comment.paraId; }));
    if (!retry) userMessage(s("replyFor", { text: clip(comment.text, 80) }));
    var out = card();
    return readFrom(comment.anchor.node, 400).then(function (value) {
      var block = (value.blocks || [])[0];
      var quoted = block ? block.text.slice(comment.anchor.start, comment.anchor.end) : "";
      return complete(Core.replyRequest(thread, quoted), out).then(function (answer) {
        var reply = { id: "comment", label: s("comment"), run: function () { return propose(Core.planForReply(comment, answer, value.revision)); } };
        setActions(out, [reply], Core.cleanText(answer), function () { return draftReply(comment, all, true); });
      });
    }).catch(function (err) { fail(err, out); });
  }

  function clearHistory() {
    history = [];
    $("log").replaceChildren($("empty"));
    $("empty").hidden = false;
    $("firstRun").hidden = true;
    persist();
    $("working").textContent = s("cleared");
  }

  // ---- The composer -----------------------------------------------------------

  function setMode(next) {
    mode = next;
    $("draftMode").hidden = mode !== "draft";
    $("prompt").placeholder = s(mode === "draft" ? "prompt.draftPlaceholder" : "prompt.placeholder");
  }

  /** The chip names the selection only while there is one. */
  function reflectSelection() {
    return editor.selection().then(function (sel) {
      attached = sel && sel.hasRange && sel.text && sel.text.trim() ? { text: sel.text, anchor: sel.anchor, focus: sel.focus } : null;
      $("attached").hidden = !attached;
      if (attached) {
        var label = s("attached", { text: clip(attached.text.replace(/\s+/g, " ").trim(), 60) });
        $("attachedText").textContent = label;
        $("attached").title = label;
      }
    }).catch(function () {});
  }

  function grow() {
    var field = $("prompt");
    field.style.height = "auto";
    field.style.height = Math.min(field.scrollHeight, 160) + "px";
  }

  function send() {
    var text = $("prompt").value.trim();
    if (!text || running) return;
    $("prompt").value = "";
    grow();
    if (!seenNote) {
      seenNote = true;
      $("firstRun").hidden = true;
      persist();
    }
    if (mode === "draft") {
      setMode("ask");
      void draft(text);
    } else void ask(text, attached);
  }
  $("send").addEventListener("click", send);
  $("prompt").addEventListener("input", grow);
  $("prompt").addEventListener("keydown", function (event) {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      send();
    } else if (event.key === "Escape" && running) {
      event.preventDefault();
      running.abort();
    }
  });
  $("detach").addEventListener("click", function () {
    attached = null;
    $("attached").hidden = true;
    $("prompt").focus();
  });
  $("draftCancel").addEventListener("click", function () {
    setMode("ask");
    $("prompt").focus();
  });
  Array.prototype.forEach.call(document.querySelectorAll("[data-chip]"), function (chip) {
    chip.addEventListener("click", function () {
      if (running) return;
      var kind = chip.dataset.chip;
      if (kind === "review") void review();
      else if (kind === "draft") {
        setMode("draft");
        $("prompt").focus();
      } else void summarise(kind);
    });
  });

  function label() {
    $("log").setAttribute("aria-label", s("logLabel"));
    $("emptyText").textContent = s("empty");
    $("promptLabel").textContent = s("prompt.label");
    $("sendLabel").textContent = s("send");
    $("send").title = s("send");
    $("stopLabel").textContent = s("stop");
    $("stop").title = s("stop");
    $("translateLabel").textContent = s("translateTo");
    $("translateGo").textContent = s("translateGo");
    $("detach").setAttribute("aria-label", s("detach"));
    $("detach").title = s("detach");
    $("draftModeText").textContent = s("draftMode");
    $("draftCancel").setAttribute("aria-label", s("draftCancel"));
    $("draftCancel").title = s("draftCancel");
    $("caveat").textContent = s("caveat");
    Array.prototype.forEach.call(document.querySelectorAll("[data-chip]"), function (b) { b.lastElementChild.textContent = s("chip." + b.dataset.chip); });
    var p = info.provider || {};
    $("firstRun").textContent = (p.kind === "host" || p.kind === "mcp-sampling" || !p.endpoint ? s("disclosure.host") : s("disclosure.endpoint", { endpoint: p.endpoint })) + " " + s("historyNote");
    setMode(mode);
  }

  function restore() {
    history = [];
    $("log").replaceChildren($("empty"));
    $("empty").hidden = false;
    return editor.storage.get().then(function (all) {
      seenNote = !!(all && all[SEEN]);
      $("firstRun").hidden = seenNote;
      if (!info.documentKey) return;
      history = Core.trimHistory(all && all[info.documentKey], 20);
      history.forEach(function (h) {
        if (h.role === "user") {
          userMessage(h.content);
          return;
        }
        var out = card();
        out.body.textContent = h.content;
        answerActions(out, h.content, null, null);
      });
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
      void reflectSelection();
      editor.on("selection/changed", function () { void reflectSelection(); });
      editor.on("contribution/invoked", function (params) {
        var command = params && params.command;
        // Ask and Translate need the panel; the host opens it for everything that
        // is not a selection action, so these two open it themselves.
        if (command === "ask" || command === "translate") void editor.ui.panel(true).catch(function () {});
        if (command === "ask" || command === "panel") {
          void reflectSelection();
          $("prompt").focus();
        } else if (command === "summarise") void summarise("summary");
        else if (command === "review") void review();
        else if (command === "draft") {
          setMode("draft");
          $("prompt").focus();
        } else if (command === "replies") void replies();
        else if (command === "clear") clearHistory();
        else if (command === "translate") showTranslate();
        else if (Core.ACTION_IDS.indexOf(command) >= 0) void selectionAction(command);
      });
      editor.on("document/opened", function (params) {
        info.documentKey = params.documentKey;
        void restore();
      });
      editor.on("theme/changed", function (params) {
        if (params && params.locale && params.locale !== info.locale) info.locale = params.locale;
      });
      window.casualAssistant = { ask: ask, summarise: summarise, review: review, selectionAction: selectionAction, draft: draft, replies: replies, clear: clearHistory };
    });
  });
})();
